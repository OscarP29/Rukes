//! Job objects: agrupan un proceso y sus descendientes para poder terminarlos juntos (RF-137).
//!
//! El job no lleva `KILL_ON_JOB_CLOSE`: si un script abre una aplicación y termina, la
//! aplicación sigue viva al cerrar el job. Solo [`Job::terminate`] mata el árbol.

use std::os::windows::io::AsRawHandle;
use std::process::Child;

use windows::Win32::Foundation::{CloseHandle, HANDLE};
use windows::Win32::System::JobObjects::{
    AssignProcessToJobObject, CreateJobObjectW, TerminateJobObject,
};
use windows::core::PCWSTR;

use crate::{Context as _, Result};

/// Job object propio.
#[derive(Debug)]
pub struct Job(HANDLE);

// SAFETY: un handle de job puede usarse desde cualquier hilo.
unsafe impl Send for Job {}
// SAFETY: las operaciones sobre el job son seguras entre hilos (el kernel las serializa).
unsafe impl Sync for Job {}

impl Job {
    /// Crea un job anónimo.
    pub fn new() -> Result<Self> {
        // SAFETY: sin atributos de seguridad ni nombre.
        let h = unsafe { CreateJobObjectW(None, PCWSTR::null()) }.ctx("CreateJobObjectW")?;
        Ok(Self(h))
    }

    /// Mete el proceso en el job. Sus hijos creados a partir de ahora también entran.
    pub fn assign(&self, child: &Child) -> Result<()> {
        let process = HANDLE(child.as_raw_handle());
        // SAFETY: handle del job propio y del proceso, vivos mientras exista `child`.
        unsafe { AssignProcessToJobObject(self.0, process) }.ctx("AssignProcessToJobObject")
    }

    /// Termina todos los procesos del job.
    pub fn terminate(&self, code: u32) -> Result<()> {
        // SAFETY: handle del job propio.
        unsafe { TerminateJobObject(self.0, code) }.ctx("TerminateJobObject")
    }
}

impl Drop for Job {
    fn drop(&mut self) {
        // SAFETY: handle propio, se cierra una sola vez.
        let _ = unsafe { CloseHandle(self.0) };
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::os::windows::process::CommandExt as _;
    use std::time::{Duration, Instant};

    #[test]
    fn terminate_kills_the_tree() {
        // `cmd` lanza un `ping` largo como hijo: terminar el job mata a los dos.
        let mut child = std::process::Command::new("cmd.exe")
            .args(["/d", "/c", "ping -n 30 127.0.0.1 >nul"])
            .creation_flags(0x0800_0000)
            .spawn()
            .unwrap();
        let job = Job::new().unwrap();
        job.assign(&child).unwrap();
        std::thread::sleep(Duration::from_millis(300));
        job.terminate(1).unwrap();
        let t = Instant::now();
        let status = child.wait().unwrap();
        assert_eq!(status.code(), Some(1));
        assert!(t.elapsed() < Duration::from_secs(2));
    }
}
