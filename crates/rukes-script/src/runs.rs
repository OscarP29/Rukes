//! Ejecuciones en curso e historial persistente (`logs`, RF-137 y RF-138).

use std::collections::VecDeque;
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, AtomicU64, AtomicUsize, Ordering};
use std::sync::{Arc, Mutex, MutexGuard};
use std::time::{Instant, SystemTime, UNIX_EPOCH};

use serde::{Deserialize, Serialize};

use crate::proc::JobSlot;

/// Bytes de cada flujo que se guardan en el historial.
pub const KEEP_OUTPUT: usize = 16 * 1024;

/// Cómo terminó una ejecución.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Status {
    /// Código 0.
    Ok,
    /// Código distinto de 0.
    Failed,
    /// Tiempo agotado.
    Timeout,
    /// Cancelada.
    Cancelled,
    /// No se pudo lanzar.
    Error,
    /// Se lanzó sin capturar la salida (administrador).
    Detached,
}

/// Una ejecución terminada.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Record {
    /// Id.
    pub id: u64,
    /// Nombre del script.
    pub name: String,
    /// Ruta.
    pub path: PathBuf,
    /// Inicio (segundos Unix).
    pub started: u64,
    /// Duración.
    pub duration_ms: u64,
    /// Fin.
    pub status: Status,
    /// Código de salida.
    pub code: Option<i32>,
    /// Argumentos.
    pub args: Vec<(String, String)>,
    /// Salida (recortada).
    pub stdout: String,
    /// Errores (recortados).
    pub stderr: String,
}

impl Record {
    /// Salida completa guardada (stdout y, si hay, stderr).
    pub fn output(&self) -> String {
        match (self.stdout.trim_end(), self.stderr.trim_end()) {
            (o, "") => o.to_owned(),
            ("", e) => e.to_owned(),
            (o, e) => format!("{o}\n{e}"),
        }
    }
}

/// Una ejecución en curso.
pub struct Running {
    /// Id.
    pub id: u64,
    /// Nombre del script.
    pub name: String,
    /// Ruta.
    pub path: PathBuf,
    /// Inicio.
    pub started: Instant,
    /// Inicio (segundos Unix).
    pub started_unix: u64,
    cancelled: Arc<AtomicBool>,
    /// Job del proceso en curso.
    pub job: Arc<JobSlot>,
}

impl Running {
    /// Pide cancelar: marca la ejecución y termina su árbol de procesos.
    pub fn cancel(&self) {
        self.cancelled.store(true, Ordering::SeqCst);
        let job = self.job.lock().ok().and_then(|j| j.clone());
        if let Some(j) = job {
            let _ = j.terminate(1);
        }
    }

    /// `true` si se pidió cancelar.
    pub fn is_cancelled(&self) -> bool {
        self.cancelled.load(Ordering::SeqCst)
    }

    /// Comprobación para el motor Rhai y el lanzador.
    pub fn cancel_flag(&self) -> Arc<AtomicBool> {
        Arc::clone(&self.cancelled)
    }
}

fn lock<T>(m: &Mutex<T>) -> MutexGuard<'_, T> {
    m.lock().unwrap_or_else(|p| p.into_inner())
}

/// Segundos Unix actuales.
pub fn now_unix() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |d| d.as_secs())
}

fn cut(s: &str) -> String {
    if s.len() <= KEEP_OUTPUT {
        return s.to_owned();
    }
    let mut end = KEEP_OUTPUT;
    while !s.is_char_boundary(end) {
        end -= 1;
    }
    format!("{}\n…", &s[..end])
}

/// Registro de ejecuciones.
pub struct Runs {
    file: PathBuf,
    cap: AtomicUsize,
    next: AtomicU64,
    running: Mutex<Vec<Arc<Running>>>,
    history: Mutex<VecDeque<Record>>,
}

impl Runs {
    /// Carga el historial de `file` (como mucho `cap` entradas).
    pub fn open(file: PathBuf, cap: usize) -> Self {
        let history: VecDeque<Record> = rukes_core::persist::load_json::<Vec<Record>>(&file)
            .unwrap_or_default()
            .into_iter()
            .take(cap)
            .collect();
        let next = history.iter().map(|r| r.id).max().unwrap_or(0) + 1;
        Self {
            file,
            cap: AtomicUsize::new(cap),
            next: AtomicU64::new(next),
            running: Mutex::new(Vec::new()),
            history: Mutex::new(history),
        }
    }

    /// Cambia el tamaño del historial.
    pub fn set_cap(&self, cap: usize) {
        self.cap.store(cap, Ordering::SeqCst);
    }

    /// Registra una ejecución que empieza.
    pub fn start(&self, name: &str, path: &std::path::Path) -> Arc<Running> {
        let run = Arc::new(Running {
            id: self.next.fetch_add(1, Ordering::SeqCst),
            name: name.to_owned(),
            path: path.to_path_buf(),
            started: Instant::now(),
            started_unix: now_unix(),
            cancelled: Arc::new(AtomicBool::new(false)),
            job: Arc::new(JobSlot::default()),
        });
        lock(&self.running).push(Arc::clone(&run));
        run
    }

    /// Pasa una ejecución al historial y lo guarda.
    pub fn finish(&self, mut record: Record) {
        lock(&self.running).retain(|r| r.id != record.id);
        record.stdout = cut(&record.stdout);
        record.stderr = cut(&record.stderr);
        let snapshot: Vec<Record> = {
            let mut h = lock(&self.history);
            h.push_front(record);
            h.truncate(self.cap.load(Ordering::SeqCst));
            h.iter().cloned().collect()
        };
        if let Err(e) = rukes_core::persist::save_json(&self.file, &snapshot) {
            tracing::warn!(%e, "no se pudo guardar el historial de scripts");
        }
    }

    /// Ejecuciones en curso (la más reciente primero).
    pub fn running(&self) -> Vec<Arc<Running>> {
        let mut v = lock(&self.running).clone();
        v.reverse();
        v
    }

    /// Historial (la más reciente primero).
    pub fn history(&self) -> Vec<Record> {
        lock(&self.history).iter().cloned().collect()
    }

    /// Una entrada del historial.
    pub fn get(&self, id: u64) -> Option<Record> {
        lock(&self.history).iter().find(|r| r.id == id).cloned()
    }

    /// Cancela una ejecución en curso. `false` si ya no corre.
    pub fn cancel(&self, id: u64) -> bool {
        let run = lock(&self.running).iter().find(|r| r.id == id).cloned();
        run.map(|r| r.cancel()).is_some()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn record(id: u64) -> Record {
        Record {
            id,
            name: "x".into(),
            path: PathBuf::from("x.ps1"),
            started: 1,
            duration_ms: 2,
            status: Status::Ok,
            code: Some(0),
            args: vec![],
            stdout: "ñ".repeat(KEEP_OUTPUT),
            stderr: String::new(),
        }
    }

    #[test]
    fn history_is_capped_cut_and_persisted() {
        let file = std::env::temp_dir().join(format!("rukes-runs-{}.json", std::process::id()));
        let _ = std::fs::remove_file(&file);
        let runs = Runs::open(file.clone(), 2);
        for _ in 0..3 {
            let r = runs.start("x", std::path::Path::new("x.ps1"));
            assert_eq!(runs.running().len(), 1);
            runs.finish(record(r.id));
        }
        assert!(runs.running().is_empty());
        let h = runs.history();
        assert_eq!(h.len(), 2);
        assert_eq!(h[0].id, 3);
        assert!(h[0].stdout.len() <= KEEP_OUTPUT + 4);
        let again = Runs::open(file.clone(), 50);
        assert_eq!(again.history().len(), 2);
        assert_eq!(again.start("y", std::path::Path::new("y")).id, 4);
        let _ = std::fs::remove_file(file);
    }

    #[test]
    fn cancel_marks_the_run() {
        let runs = Runs::open(std::env::temp_dir().join("rukes-runs-none.json"), 5);
        let r = runs.start("x", std::path::Path::new("x"));
        assert!(runs.cancel(r.id));
        assert!(r.is_cancelled());
        assert!(!runs.cancel(999));
    }
}
