//! Proceso de un plugin externo: NDJSON sobre stdin/stdout (RF-170, RF-172).

use std::collections::BTreeMap;
use std::io::{BufRead, BufReader, Write};
use std::os::windows::process::CommandExt as _;
use std::path::{Path, PathBuf};
use std::process::{Child, ChildStdin, Command, Stdio};
use std::sync::atomic::{AtomicBool, AtomicU32, AtomicU64, Ordering};
use std::sync::{Arc, Mutex, MutexGuard};
use std::time::{Duration, Instant};

use crossbeam_channel::{Receiver, RecvTimeoutError, Sender};
use rukes_core::CancelToken;
use serde_json::{Value, json};

/// Versión del protocolo.
pub const PROTOCOL: u64 = 1;
/// Fallos seguidos que desactivan el plugin hasta recargarlo.
pub const MAX_FAILURES: u32 = 3;
/// Tiempo máximo para `initialize`.
pub const INIT_TIMEOUT: Duration = Duration::from_secs(5);

const CREATE_NO_WINDOW: u32 = 0x0800_0000;

/// Error de una llamada.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CallError {
    /// No respondió a tiempo.
    Timeout,
    /// La consulta se canceló (el usuario siguió escribiendo). No cuenta como fallo.
    Cancelled,
    /// El proceso terminó o no se pudo escribir.
    Died(String),
    /// No se pudo arrancar.
    Spawn(String),
    /// El plugin respondió con un error.
    Remote(String),
    /// Desactivado tras varios fallos.
    Disabled,
}

impl std::fmt::Display for CallError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Timeout => write!(f, "timeout"),
            Self::Cancelled => write!(f, "cancelled"),
            Self::Died(e) => write!(f, "process died: {e}"),
            Self::Spawn(e) => write!(f, "could not start: {e}"),
            Self::Remote(e) => write!(f, "{e}"),
            Self::Disabled => write!(f, "disabled"),
        }
    }
}

type Pending = Arc<Mutex<BTreeMap<u64, Sender<Result<Value, String>>>>>;
/// Resultados incrementales (`results`) por id de consulta.
type Streams = Arc<Mutex<BTreeMap<u64, Sender<Value>>>>;

struct Running {
    child: Child,
    stdin: ChildStdin,
    alive: Arc<AtomicBool>,
}

impl Drop for Running {
    fn drop(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}

fn lock<T>(m: &Mutex<T>) -> MutexGuard<'_, T> {
    m.lock().unwrap_or_else(std::sync::PoisonError::into_inner)
}

/// Un plugin externo en su proceso. Se arranca al primer uso y se reinicia si muere.
pub struct PluginProcess {
    name: String,
    dir: PathBuf,
    program: String,
    args: Vec<String>,
    config: Value,
    running: Mutex<Option<Running>>,
    pending: Pending,
    streams: Streams,
    next_id: AtomicU64,
    failures: AtomicU32,
    disabled: AtomicBool,
    last_error: Mutex<Option<String>>,
}

/// Programa a ejecutar: si existe dentro de la carpeta del plugin, esa ruta; si no, tal cual
/// (un comando del `PATH`, como `python`).
fn resolve(dir: &Path, program: &str) -> String {
    let local = dir.join(program);
    if local.is_file() {
        local.to_string_lossy().into_owned()
    } else {
        program.to_owned()
    }
}

impl PluginProcess {
    /// Prepara el proceso (no lo arranca).
    pub fn new(name: &str, dir: &Path, program: &str, args: &[String], config: Value) -> Self {
        Self {
            name: name.to_owned(),
            dir: dir.to_owned(),
            program: resolve(dir, program),
            args: args.to_vec(),
            config,
            running: Mutex::new(None),
            pending: Arc::default(),
            streams: Arc::default(),
            next_id: AtomicU64::new(1),
            failures: AtomicU32::new(0),
            disabled: AtomicBool::new(false),
            last_error: Mutex::new(None),
        }
    }

    /// Desactivado tras [`MAX_FAILURES`] fallos seguidos.
    pub fn is_disabled(&self) -> bool {
        self.disabled.load(Ordering::SeqCst)
    }

    /// El proceso está en marcha.
    pub fn is_running(&self) -> bool {
        lock(&self.running)
            .as_ref()
            .is_some_and(|r| r.alive.load(Ordering::SeqCst))
    }

    /// Último error.
    pub fn last_error(&self) -> Option<String> {
        lock(&self.last_error).clone()
    }

    /// Fallos seguidos.
    pub fn failures(&self) -> u32 {
        self.failures.load(Ordering::SeqCst)
    }

    /// Para el proceso y borra los fallos: se volverá a arrancar en el próximo uso.
    pub fn restart(&self) {
        *lock(&self.running) = None;
        self.failures.store(0, Ordering::SeqCst);
        self.disabled.store(false, Ordering::SeqCst);
        *lock(&self.last_error) = None;
    }

    /// Para el proceso.
    pub fn stop(&self) {
        *lock(&self.running) = None;
    }

    fn spawn(&self) -> Result<Running, CallError> {
        let mut child = Command::new(&self.program)
            .args(&self.args)
            .current_dir(&self.dir)
            .env("RUKES_PROTOCOL", PROTOCOL.to_string())
            .env("PYTHONUNBUFFERED", "1")
            .env("PYTHONIOENCODING", "utf-8")
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .creation_flags(CREATE_NO_WINDOW)
            .spawn()
            .map_err(|e| CallError::Spawn(format!("{}: {e}", self.program)))?;
        let (Some(stdin), Some(stdout), Some(stderr)) =
            (child.stdin.take(), child.stdout.take(), child.stderr.take())
        else {
            let _ = child.kill();
            return Err(CallError::Spawn("stdio".into()));
        };
        let alive = Arc::new(AtomicBool::new(true));
        let (pending, streams) = (Arc::clone(&self.pending), Arc::clone(&self.streams));
        let (flag, name) = (Arc::clone(&alive), self.name.clone());
        let reader = std::thread::Builder::new()
            .name("rukes-ext-out".into())
            .spawn(move || {
                for line in BufReader::new(stdout).lines() {
                    let Ok(line) = line else { break };
                    let line = line.trim();
                    if line.is_empty() {
                        continue;
                    }
                    let Ok(msg) = serde_json::from_str::<Value>(line) else {
                        tracing::warn!(plugin = %name, line = %crate::util::ellipsis(line, 200), "línea no JSON en stdout");
                        continue;
                    };
                    let Some(id) = msg.get("id").and_then(Value::as_u64) else {
                        match msg.get("method").and_then(Value::as_str) {
                            Some("log") => {
                                let text = msg["params"]["message"].as_str().unwrap_or_default();
                                tracing::info!(plugin = %name, "{text}");
                            }
                            Some("results") => {
                                let qid = msg["params"]["query_id"].as_u64().unwrap_or(0);
                                let tx = lock(&streams).get(&qid).cloned();
                                if let Some(tx) = tx {
                                    let _ = tx.send(msg["params"]["items"].clone());
                                }
                            }
                            _ => {}
                        }
                        continue;
                    };
                    let tx = lock(&pending).remove(&id);
                    if let Some(tx) = tx {
                        let r = match msg.get("error") {
                            Some(e) if !e.is_null() => Err(e
                                .get("message")
                                .and_then(Value::as_str)
                                .map_or_else(|| e.to_string(), str::to_owned)),
                            _ => Ok(msg.get("result").cloned().unwrap_or(Value::Null)),
                        };
                        let _ = tx.send(r);
                    }
                }
                flag.store(false, Ordering::SeqCst);
                // Las llamadas pendientes ven el canal cerrado: el proceso murió.
                lock(&pending).clear();
            });
        let name = self.name.clone();
        let errs = std::thread::Builder::new()
            .name("rukes-ext-err".into())
            .spawn(move || {
                for line in BufReader::new(stderr).lines().map_while(Result::ok) {
                    tracing::info!(plugin = %name, stderr = %line);
                }
            });
        if reader.is_err() || errs.is_err() {
            let _ = child.kill();
            return Err(CallError::Spawn("thread".into()));
        }
        tracing::info!(plugin = %self.name, pid = child.id(), "plugin externo arrancado");
        Ok(Running {
            child,
            stdin,
            alive,
        })
    }

    /// Envía un mensaje y devuelve el receptor de su respuesta.
    fn send(
        &self,
        method: &str,
        params: Value,
    ) -> Result<(u64, Receiver<Result<Value, String>>), CallError> {
        let mut guard = lock(&self.running);
        let dead = guard.as_mut().is_none_or(|r| {
            !r.alive.load(Ordering::SeqCst) || r.child.try_wait().ok().flatten().is_some()
        });
        if dead {
            *guard = None;
            let mut r = self.spawn()?;
            let id = self.next_id.fetch_add(1, Ordering::SeqCst);
            let (tx, rx) = crossbeam_channel::bounded(1);
            lock(&self.pending).insert(id, tx);
            let init = json!({
                "id": id,
                "method": "initialize",
                "params": {
                    "protocol": PROTOCOL,
                    "config": self.config,
                    "locale": rukes_core::i18n::code(),
                    "plugin_dir": self.dir,
                },
            });
            write_line(&mut r.stdin, &init)?;
            match rx.recv_timeout(INIT_TIMEOUT) {
                Ok(Ok(_)) => {}
                Ok(Err(e)) => return Err(CallError::Remote(format!("initialize: {e}"))),
                Err(RecvTimeoutError::Timeout) => {
                    lock(&self.pending).remove(&id);
                    return Err(CallError::Timeout);
                }
                Err(RecvTimeoutError::Disconnected) => {
                    return Err(CallError::Died("initialize".into()));
                }
            }
            *guard = Some(r);
        }
        let id = self.next_id.fetch_add(1, Ordering::SeqCst);
        let (tx, rx) = crossbeam_channel::bounded(1);
        lock(&self.pending).insert(id, tx);
        let msg = json!({ "id": id, "method": method, "params": params });
        let Some(r) = guard.as_mut() else {
            return Err(CallError::Died("stdin".into()));
        };
        if let Err(e) = write_line(&mut r.stdin, &msg) {
            lock(&self.pending).remove(&id);
            *guard = None;
            return Err(e);
        }
        Ok((id, rx))
    }

    fn notify(&self, method: &str, params: Value) {
        if let Some(r) = lock(&self.running).as_mut() {
            let _ = write_line(&mut r.stdin, &json!({ "method": method, "params": params }));
        }
    }

    /// Llamada con tiempo máximo.
    pub fn call(&self, method: &str, params: Value, timeout: Duration) -> Result<Value, CallError> {
        self.call_streaming(method, params, timeout, None, &mut |_| {})
    }

    /// Llamada con tiempo máximo. Con `cancel` (token e id de consulta), se abandona en cuanto
    /// se cancela la consulta (y se avisa al plugin con `cancel`); los `results` que el plugin
    /// envíe antes de responder llegan a `partial`.
    pub fn call_streaming(
        &self,
        method: &str,
        params: Value,
        timeout: Duration,
        cancel: Option<(&CancelToken, u64)>,
        partial: &mut dyn FnMut(Value),
    ) -> Result<Value, CallError> {
        if self.is_disabled() {
            return Err(CallError::Disabled);
        }
        let stream = cancel.map(|(_, qid)| {
            let (tx, rx) = crossbeam_channel::unbounded();
            lock(&self.streams).insert(qid, tx);
            (qid, rx)
        });
        let result = self.call_inner(
            method,
            params,
            timeout,
            cancel,
            stream.as_ref().map(|s| &s.1),
            partial,
        );
        if let Some((qid, _)) = stream {
            lock(&self.streams).remove(&qid);
        }
        match &result {
            Ok(_) | Err(CallError::Remote(_)) => {
                self.failures.store(0, Ordering::SeqCst);
            }
            Err(CallError::Cancelled | CallError::Disabled) => {}
            Err(e) => {
                let n = self.failures.fetch_add(1, Ordering::SeqCst) + 1;
                tracing::warn!(plugin = %self.name, method, error = %e, failures = n, "fallo de plugin externo");
                *lock(&self.last_error) = Some(e.to_string());
                // Un plugin colgado se para: el siguiente uso arranca uno nuevo.
                self.stop();
                if n >= MAX_FAILURES {
                    tracing::warn!(plugin = %self.name, "plugin externo desactivado hasta recargarlo");
                    self.disabled.store(true, Ordering::SeqCst);
                }
            }
        }
        if let Err(CallError::Remote(e)) = &result {
            *lock(&self.last_error) = Some(e.clone());
        }
        result
    }

    fn call_inner(
        &self,
        method: &str,
        params: Value,
        timeout: Duration,
        cancel: Option<(&CancelToken, u64)>,
        stream: Option<&Receiver<Value>>,
        partial: &mut dyn FnMut(Value),
    ) -> Result<Value, CallError> {
        let start = Instant::now();
        let (id, rx) = self.send(method, params)?;
        let deadline = start + timeout;
        loop {
            if let Some((c, qid)) = cancel
                && c.is_cancelled()
            {
                lock(&self.pending).remove(&id);
                self.notify("cancel", json!({ "query_id": qid }));
                return Err(CallError::Cancelled);
            }
            let left = deadline.saturating_duration_since(Instant::now());
            if left.is_zero() {
                lock(&self.pending).remove(&id);
                return Err(CallError::Timeout);
            }
            if let Some(s) = stream {
                for items in s.try_iter() {
                    partial(items);
                }
            }
            match rx.recv_timeout(left.min(Duration::from_millis(20))) {
                Ok(Ok(v)) => {
                    // El lector entrega en orden: los `results` previos ya están en el canal.
                    if let Some(s) = stream {
                        for items in s.try_iter() {
                            partial(items);
                        }
                    }
                    return Ok(v);
                }
                Ok(Err(e)) => return Err(CallError::Remote(e)),
                Err(RecvTimeoutError::Timeout) => {}
                Err(RecvTimeoutError::Disconnected) => {
                    return Err(CallError::Died(method.into()));
                }
            }
        }
    }
}

fn write_line(stdin: &mut ChildStdin, msg: &Value) -> Result<(), CallError> {
    let mut line = msg.to_string();
    line.push('\n');
    stdin
        .write_all(line.as_bytes())
        .and_then(|()| stdin.flush())
        .map_err(|e| CallError::Died(e.to_string()))
}

impl Drop for PluginProcess {
    fn drop(&mut self) {
        self.stop();
    }
}
