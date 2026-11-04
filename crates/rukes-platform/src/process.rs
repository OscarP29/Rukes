//! Procesos: listado con memoria y tiempo de CPU, y terminación (RF-051, RF-052).

use std::collections::HashMap;
use std::path::PathBuf;

use windows::Win32::Foundation::{CloseHandle, FILETIME, HANDLE};
use windows::Win32::System::Diagnostics::ToolHelp::{
    CreateToolhelp32Snapshot, PROCESSENTRY32W, Process32FirstW, Process32NextW, TH32CS_SNAPPROCESS,
};
use windows::Win32::System::ProcessStatus::{GetProcessMemoryInfo, PROCESS_MEMORY_COUNTERS};
use windows::Win32::System::Threading::{
    GetProcessTimes, OpenProcess, PROCESS_ACCESS_RIGHTS, PROCESS_NAME_WIN32,
    PROCESS_QUERY_LIMITED_INFORMATION, PROCESS_TERMINATE, QueryFullProcessImageNameW,
    TerminateProcess,
};
use windows::core::PWSTR;

use crate::{Context as _, PlatformError, Result};

/// Procesos que nunca se terminan desde el launcher.
const PROTECTED: &[&str] = &[
    "system",
    "registry",
    "smss.exe",
    "csrss.exe",
    "wininit.exe",
    "winlogon.exe",
    "services.exe",
    "lsass.exe",
    "lsaiso.exe",
    "svchost.exe",
    "fontdrvhost.exe",
    "dwm.exe",
    "memory compression",
    "secure system",
];

/// Información de un proceso.
#[derive(Debug, Clone)]
pub struct ProcessInfo {
    /// PID.
    pub pid: u32,
    /// PID del padre.
    pub parent: u32,
    /// Nombre del ejecutable (`chrome.exe`).
    pub name: String,
    /// Ruta completa, si se puede leer.
    pub exe: Option<PathBuf>,
    /// Working set en bytes (0 si no hay acceso).
    pub memory: u64,
    /// Tiempo de CPU acumulado (kernel + usuario) en unidades de 100 ns.
    pub cpu_time: u64,
}

impl ProcessInfo {
    /// `true` si es un proceso del sistema que no se debe terminar.
    pub fn is_protected(&self) -> bool {
        self.pid <= 4
            || self.pid == std::process::id()
            || PROTECTED.contains(&self.name.to_ascii_lowercase().as_str())
    }
}

struct Handle(HANDLE);

impl Drop for Handle {
    fn drop(&mut self) {
        // SAFETY: handle abierto por `open` y cerrado una sola vez.
        let _ = unsafe { CloseHandle(self.0) };
    }
}

fn open(pid: u32, access: PROCESS_ACCESS_RIGHTS) -> Option<Handle> {
    // SAFETY: sin punteros; un PID inválido o sin permisos devuelve error.
    unsafe { OpenProcess(access, false, pid) }.ok().map(Handle)
}

fn image_path(h: &Handle) -> Option<PathBuf> {
    let mut buf = vec![0u16; 1024];
    let mut len = buf.len() as u32;
    // SAFETY: buffer local con su longitud.
    unsafe {
        QueryFullProcessImageNameW(h.0, PROCESS_NAME_WIN32, PWSTR(buf.as_mut_ptr()), &mut len)
    }
    .ok()?;
    Some(PathBuf::from(String::from_utf16_lossy(
        &buf[..len as usize],
    )))
}

/// Ruta del ejecutable de un proceso.
pub fn exe_path(pid: u32) -> Option<PathBuf> {
    image_path(&open(pid, PROCESS_QUERY_LIMITED_INFORMATION)?)
}

fn ft(t: FILETIME) -> u64 {
    (u64::from(t.dwHighDateTime) << 32) | u64::from(t.dwLowDateTime)
}

fn details(pid: u32) -> (Option<PathBuf>, u64, u64) {
    let Some(h) = open(pid, PROCESS_QUERY_LIMITED_INFORMATION) else {
        return (None, 0, 0);
    };
    let mut mem = PROCESS_MEMORY_COUNTERS {
        cb: size_of::<PROCESS_MEMORY_COUNTERS>() as u32,
        ..Default::default()
    };
    let (mut c, mut e, mut k, mut u) = Default::default();
    // SAFETY: estructuras locales con su tamaño.
    let (mem_ok, times_ok) = unsafe {
        (
            GetProcessMemoryInfo(h.0, &mut mem, mem.cb).is_ok(),
            GetProcessTimes(h.0, &mut c, &mut e, &mut k, &mut u).is_ok(),
        )
    };
    (
        image_path(&h),
        if mem_ok { mem.WorkingSetSize as u64 } else { 0 },
        if times_ok { ft(k) + ft(u) } else { 0 },
    )
}

/// Lista todos los procesos.
pub fn list() -> Result<Vec<ProcessInfo>> {
    // SAFETY: sin punteros; el handle se cierra al salir.
    let snap = Handle(unsafe { CreateToolhelp32Snapshot(TH32CS_SNAPPROCESS, 0) }.ctx("Toolhelp")?);
    let mut entry = PROCESSENTRY32W {
        dwSize: size_of::<PROCESSENTRY32W>() as u32,
        ..Default::default()
    };
    let mut out = Vec::with_capacity(256);
    // SAFETY: `entry` es local y lleva `dwSize`.
    let mut ok = unsafe { Process32FirstW(snap.0, &mut entry) }.is_ok();
    while ok {
        let name_len = entry
            .szExeFile
            .iter()
            .position(|&c| c == 0)
            .unwrap_or(entry.szExeFile.len());
        let pid = entry.th32ProcessID;
        let (exe, memory, cpu_time) = details(pid);
        out.push(ProcessInfo {
            pid,
            parent: entry.th32ParentProcessID,
            name: String::from_utf16_lossy(&entry.szExeFile[..name_len]),
            exe,
            memory,
            cpu_time,
        });
        // SAFETY: como arriba.
        ok = unsafe { Process32NextW(snap.0, &mut entry) }.is_ok();
    }
    Ok(out)
}

/// Termina un proceso.
pub fn kill(pid: u32) -> Result<()> {
    let h = open(pid, PROCESS_TERMINATE)
        .ok_or_else(|| PlatformError::Invalid(format!("no se puede abrir el proceso {pid}")))?;
    // SAFETY: handle con permiso de terminación.
    unsafe { TerminateProcess(h.0, 1) }.ctx("TerminateProcess")
}

/// PIDs de los descendientes de `root` (hijos, nietos...), los más profundos primero.
pub fn descendants(all: &[ProcessInfo], root: u32) -> Vec<u32> {
    let mut children: HashMap<u32, Vec<u32>> = HashMap::new();
    for p in all {
        // El PID 0 es su propio padre; se evita el ciclo.
        if p.pid != p.parent {
            children.entry(p.parent).or_default().push(p.pid);
        }
    }
    let mut order = Vec::new();
    let mut stack = vec![root];
    let mut seen = std::collections::HashSet::new();
    while let Some(pid) = stack.pop() {
        for &c in children.get(&pid).map_or(&[][..], Vec::as_slice) {
            if seen.insert(c) {
                order.push(c);
                stack.push(c);
            }
        }
    }
    order.reverse();
    order
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn lists_self_and_kills_a_child() {
        let all = list().unwrap();
        let me = all.iter().find(|p| p.pid == std::process::id()).unwrap();
        assert!(me.memory > 0);
        assert!(me.is_protected());

        let mut child = std::process::Command::new("ping")
            .args(["-n", "30", "127.0.0.1"])
            .stdout(std::process::Stdio::null())
            .spawn()
            .unwrap();
        let all = list().unwrap();
        assert!(descendants(&all, std::process::id()).contains(&child.id()));
        kill(child.id()).unwrap();
        let status = child.wait().unwrap();
        assert_eq!(status.code(), Some(1));
    }

    #[test]
    fn descendants_deepest_first() {
        let p = |pid, parent| ProcessInfo {
            pid,
            parent,
            name: String::new(),
            exe: None,
            memory: 0,
            cpu_time: 0,
        };
        let all = [p(0, 0), p(10, 1), p(11, 10), p(12, 11), p(20, 1)];
        let d = descendants(&all, 10);
        assert_eq!(d, vec![12, 11]);
    }
}
