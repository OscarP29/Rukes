//! Lanzar procesos con captura de salida, tiempo máximo y cancelación (RF-137, RF-138).

use std::io::Read;
use std::os::windows::process::CommandExt as _;
use std::process::{Command, Stdio};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use base64::Engine as _;
use rukes_platform::job::Job;

use crate::lang::Launch;

/// `CREATE_NO_WINDOW`: sin ventana de consola.
pub const CREATE_NO_WINDOW: u32 = 0x0800_0000;
/// `CREATE_NEW_CONSOLE`: consola propia y visible.
pub const CREATE_NEW_CONSOLE: u32 = 0x0000_0010;

/// Bytes máximos que se capturan de cada flujo.
pub const MAX_CAPTURE: usize = 1024 * 1024;

/// Cómo terminó un proceso.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Exit {
    /// Terminó con este código.
    Code(i32),
    /// Se superó el tiempo máximo y se terminó el árbol de procesos.
    Timeout,
    /// El usuario lo canceló.
    Cancelled,
    /// No se pudo lanzar.
    SpawnError(String),
}

/// Salida capturada.
#[derive(Debug, Clone)]
pub struct Captured {
    /// Fin.
    pub exit: Exit,
    /// Salida estándar.
    pub stdout: String,
    /// Errores.
    pub stderr: String,
}

/// Hueco donde se publica el job del proceso en curso, para poder cancelarlo desde otro hilo.
pub type JobSlot = Mutex<Option<Arc<Job>>>;

fn lock<T>(m: &Mutex<T>) -> std::sync::MutexGuard<'_, T> {
    m.lock().unwrap_or_else(|p| p.into_inner())
}

fn reader(
    mut r: impl Read + Send + 'static,
    buf: Arc<Mutex<Vec<u8>>>,
    done: crossbeam_channel::Sender<()>,
) {
    let _ = std::thread::Builder::new()
        .name("rukes-script-io".into())
        .spawn(move || {
            let mut chunk = [0u8; 8192];
            loop {
                match r.read(&mut chunk) {
                    Ok(0) | Err(_) => break,
                    Ok(n) => {
                        let mut b = lock(&buf);
                        let room = MAX_CAPTURE.saturating_sub(b.len());
                        b.extend_from_slice(&chunk[..n.min(room)]);
                    }
                }
            }
            let _ = done.send(());
        });
}

/// Ejecuta `cmd` sin ventana, capturando stdout y stderr. Se termina el árbol de procesos si
/// pasa `deadline` o si `cancelled()` devuelve `true`.
pub fn run_captured(
    mut cmd: Command,
    deadline: Option<Instant>,
    cancelled: &dyn Fn() -> bool,
    slot: Option<&JobSlot>,
) -> Captured {
    cmd.stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .creation_flags(CREATE_NO_WINDOW);
    let mut child = match cmd.spawn() {
        Ok(c) => c,
        Err(e) => {
            return Captured {
                exit: Exit::SpawnError(e.to_string()),
                stdout: String::new(),
                stderr: String::new(),
            };
        }
    };
    let job = Job::new().ok().map(Arc::new);
    if let Some(j) = &job {
        if let Err(e) = j.assign(&child) {
            tracing::debug!(%e, "no se pudo asignar el proceso al job");
        }
        if let Some(s) = slot {
            *lock(s) = Some(Arc::clone(j));
        }
    }
    let out = Arc::new(Mutex::new(Vec::new()));
    let err = Arc::new(Mutex::new(Vec::new()));
    let (tx, rx) = crossbeam_channel::bounded(2);
    let mut streams = 0;
    if let Some(s) = child.stdout.take() {
        reader(s, Arc::clone(&out), tx.clone());
        streams += 1;
    }
    if let Some(s) = child.stderr.take() {
        reader(s, Arc::clone(&err), tx);
        streams += 1;
    }
    let kill = |child: &mut std::process::Child| {
        let killed = job.as_ref().is_some_and(|j| j.terminate(1).is_ok());
        if !killed {
            let _ = child.kill();
        }
        let _ = child.wait();
    };
    let exit = loop {
        match child.try_wait() {
            Ok(Some(status)) => break Exit::Code(status.code().unwrap_or(-1)),
            Ok(None) => {}
            Err(e) => break Exit::SpawnError(e.to_string()),
        }
        if cancelled() {
            kill(&mut child);
            break Exit::Cancelled;
        }
        if deadline.is_some_and(|d| Instant::now() >= d) {
            kill(&mut child);
            break Exit::Timeout;
        }
        std::thread::sleep(Duration::from_millis(10));
    };
    // Si el script dejó un proceso hijo vivo con la salida heredada, las tuberías siguen
    // abiertas: se espera poco y se usa lo leído hasta entonces.
    let wait_until = Instant::now() + Duration::from_millis(400);
    for _ in 0..streams {
        let left = wait_until.saturating_duration_since(Instant::now());
        if rx.recv_timeout(left).is_err() {
            break;
        }
    }
    if let Some(s) = slot {
        *lock(s) = None;
    }
    let stdout = rukes_platform::text::decode_console(&lock(&out));
    let stderr = rukes_platform::text::decode_console(&lock(&err));
    Captured {
        exit,
        stdout,
        stderr,
    }
}

/// Cita un texto como cadena literal de PowerShell (`'...'`). PowerShell también acepta las
/// comillas tipográficas simples como delimitadores, así que se duplican igual.
pub fn ps_quote(s: &str) -> String {
    let mut out = String::with_capacity(s.len() + 2);
    out.push('\'');
    for c in s.chars() {
        if matches!(c, '\'' | '\u{2018}' | '\u{2019}' | '\u{201A}' | '\u{201B}') {
            out.push(c);
        }
        out.push(c);
    }
    out.push('\'');
    out
}

/// Script de PowerShell que fija `env`, ejecuta el programa con sus argumentos (todo como
/// literales, sin interpretar) y, si `pause`, espera a que el usuario pulse Enter.
pub fn wrapper_script(
    launch: &Launch,
    user_args: &[String],
    env: &[(String, String)],
    pause: Option<&str>,
) -> String {
    let mut s = String::new();
    for (k, v) in env {
        // Los nombres son nuestros (RUKES_*), pero se validan igual.
        if k.chars().all(|c| c.is_ascii_alphanumeric() || c == '_') {
            s.push_str(&format!("${{env:{k}}} = {}\n", ps_quote(v)));
        }
    }
    s.push_str("& ");
    s.push_str(&ps_quote(&launch.program.to_string_lossy()));
    for a in &launch.args {
        s.push(' ');
        s.push_str(&ps_quote(&a.to_string_lossy()));
    }
    for a in user_args {
        s.push(' ');
        s.push_str(&ps_quote(a));
    }
    s.push_str("\n$rukesCode = $LASTEXITCODE\n");
    if let Some(msg) = pause {
        s.push_str(&format!(
            "Write-Host ''\nRead-Host {} | Out-Null\n",
            ps_quote(msg)
        ));
    }
    s.push_str("exit $rukesCode\n");
    s
}

/// Codifica un script para `-EncodedCommand` (UTF-16LE en Base64).
pub fn encode_ps(script: &str) -> String {
    let utf16: Vec<u8> = script.encode_utf16().flat_map(u16::to_le_bytes).collect();
    base64::engine::general_purpose::STANDARD.encode(utf16)
}

/// Une argumentos en una línea de comandos de Windows con las reglas de `CommandLineToArgvW`
/// (para `ShellExecute`, que recibe una sola cadena).
pub fn join_args<S: AsRef<str>>(args: &[S]) -> String {
    let mut out = String::new();
    for (i, a) in args.iter().enumerate() {
        if i > 0 {
            out.push(' ');
        }
        let a = a.as_ref();
        if !a.is_empty() && !a.contains([' ', '\t', '"']) {
            out.push_str(a);
            continue;
        }
        out.push('"');
        let mut backslashes = 0;
        for c in a.chars() {
            match c {
                '\\' => backslashes += 1,
                '"' => {
                    out.extend(std::iter::repeat_n('\\', backslashes * 2 + 1));
                    out.push('"');
                    backslashes = 0;
                }
                c => {
                    out.extend(std::iter::repeat_n('\\', backslashes));
                    out.push(c);
                    backslashes = 0;
                }
            }
        }
        out.extend(std::iter::repeat_n('\\', backslashes * 2));
        out.push('"');
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::ffi::OsString;
    use std::path::PathBuf;

    fn cmd(script: &str) -> Command {
        let mut c = Command::new("cmd.exe");
        c.args(["/d", "/c", script]);
        c
    }

    #[test]
    fn captures_stdout_stderr_and_code() {
        let r = run_captured(
            cmd("echo hola& echo error 1>&2& exit /b 3"),
            None,
            &|| false,
            None,
        );
        assert_eq!(r.exit, Exit::Code(3));
        assert_eq!(r.stdout.trim(), "hola");
        assert_eq!(r.stderr.trim(), "error");
    }

    #[test]
    fn timeout_kills_the_tree_quickly() {
        let t = Instant::now();
        let r = run_captured(
            cmd("ping -n 30 127.0.0.1"),
            Some(Instant::now() + Duration::from_millis(300)),
            &|| false,
            None,
        );
        assert_eq!(r.exit, Exit::Timeout);
        assert!(t.elapsed() < Duration::from_secs(3), "{:?}", t.elapsed());
    }

    #[test]
    fn cancel_from_another_thread() {
        let flag = Arc::new(std::sync::atomic::AtomicBool::new(false));
        let f = Arc::clone(&flag);
        std::thread::spawn(move || {
            std::thread::sleep(Duration::from_millis(200));
            f.store(true, std::sync::atomic::Ordering::SeqCst);
        });
        let slot = JobSlot::default();
        let r = run_captured(
            cmd("ping -n 30 127.0.0.1"),
            None,
            &|| flag.load(std::sync::atomic::Ordering::SeqCst),
            Some(&slot),
        );
        assert_eq!(r.exit, Exit::Cancelled);
        assert!(lock(&slot).is_none());
    }

    #[test]
    fn spawn_error_is_reported() {
        let r = run_captured(Command::new("no-existe-rukes.exe"), None, &|| false, None);
        assert!(matches!(r.exit, Exit::SpawnError(_)));
    }

    #[test]
    fn quoting() {
        assert_eq!(ps_quote("it's"), "'it''s'");
        assert_eq!(ps_quote("a\u{2019}b"), "'a\u{2019}\u{2019}b'");
        assert_eq!(ps_quote("$(rm x); `n"), "'$(rm x); `n'");
        assert_eq!(
            join_args(&["a", "b c", "", "x\"y", "C:\\d\\"]),
            "a \"b c\" \"\" \"x\\\"y\" C:\\d\\"
        );
        assert_eq!(join_args(&["C:\\a b\\"]), "\"C:\\a b\\\\\"");
        let l = Launch {
            program: PathBuf::from("C:\\p.exe"),
            args: vec![OsString::from("-File"), OsString::from("C:\\s's.ps1")],
        };
        let w = wrapper_script(
            &l,
            &["Mi bug".into()],
            &[
                ("RUKES_QUERY".into(), "a'b".into()),
                ("BAD NAME".into(), "x".into()),
            ],
            Some("Enter"),
        );
        assert!(w.contains("${env:RUKES_QUERY} = 'a''b'"));
        assert!(!w.contains("BAD NAME"));
        assert!(w.contains("& 'C:\\p.exe' '-File' 'C:\\s''s.ps1' 'Mi bug'"));
        assert!(w.contains("Read-Host 'Enter'"));
    }

    #[test]
    fn wrapper_runs_in_powershell_and_passes_args_literally() {
        // El argumento con `$()` y `;` llega tal cual, sin ejecutarse.
        let l = Launch {
            program: PathBuf::from("cmd.exe"),
            args: vec![
                OsString::from("/d"),
                OsString::from("/c"),
                OsString::from("echo"),
            ],
        };
        let script = wrapper_script(&l, &["$(exit 7); x".into()], &[], None);
        let mut c = Command::new(crate::lang::powershell());
        c.args([
            "-NoProfile",
            "-NonInteractive",
            "-EncodedCommand",
            &encode_ps(&script),
        ]);
        let r = run_captured(
            c,
            Some(Instant::now() + Duration::from_secs(20)),
            &|| false,
            None,
        );
        assert_eq!(r.exit, Exit::Code(0), "{r:?}");
        assert!(r.stdout.contains("$(exit 7); x"), "{r:?}");
    }
}
