//! Notas, tareas y recordatorios (sección 5.12, RF-120 a RF-123).
//!
//! Las notas y las tareas son archivos Markdown normales (compatibles con Obsidian):
//!
//! ```text
//! - 2026-11-04 09:05 — texto de la nota
//! - [ ] tarea pendiente
//! - [x] tarea hecha ✅ 2026-11-04
//! ```
//!
//! Los recordatorios se guardan en JSON y avisan con una notificación; los que vencieron con
//! Rukes cerrado avisan al arrancar.

use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex, MutexGuard, Weak};
use std::time::Duration;

use crossbeam_channel::{Receiver, RecvTimeoutError, Sender};
use rukes_core::i18n::{tr, trf};
use rukes_core::{
    Action, ActionId, ActionOutcome, CancelToken, Fuzzy, Icon, Plugin, PluginCommand, PluginError,
    PluginMetadata, Query, ResultItem,
};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};

use crate::util::{ellipsis, now_unix};

fn lock<T>(m: &Mutex<T>) -> MutexGuard<'_, T> {
    m.lock().unwrap_or_else(std::sync::PoisonError::into_inner)
}

fn failed(e: impl std::fmt::Display) -> PluginError {
    PluginError::Failed(e.to_string())
}

/// `2026-11-04` y `09:05` en hora local.
fn stamp() -> (String, String) {
    let t = rukes_platform::time::local_now();
    (
        format!("{}-{:02}-{:02}", t.year, t.month, t.day),
        format!("{:02}:{:02}", t.hour, t.minute),
    )
}

fn open_file(path: &Path) -> Result<ActionOutcome, PluginError> {
    rukes_platform::shell::launch(
        &path.to_string_lossy(),
        None,
        None,
        rukes_platform::shell::Verb::Open,
    )
    .map_err(failed)?;
    Ok(ActionOutcome::Hide)
}

/// Añade una línea al final de un archivo Markdown; si no existe lo crea con `header`.
pub fn append_line(path: &Path, header: &str, line: &str) -> std::io::Result<()> {
    if let Some(dir) = path.parent() {
        std::fs::create_dir_all(dir)?;
    }
    let mut text = std::fs::read_to_string(path).unwrap_or_else(|_| format!("# {header}\n\n"));
    let nl = if text.contains("\r\n") { "\r\n" } else { "\n" };
    if !text.is_empty() && !text.ends_with('\n') {
        text.push_str(nl);
    }
    text.push_str(line);
    text.push_str(nl);
    std::fs::write(path, text)
}

// ------------------------------------------------- comandos (Plugin::call)

/// Texto del argumento `name`; error si falta o está vacío.
fn text_arg<'a>(args: &'a Value, name: &str) -> Result<&'a str, PluginError> {
    args[name]
        .as_str()
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .ok_or_else(|| PluginError::BadArgs(format!("falta «{name}» (texto)")))
}

/// Número entero opcional `name`, o `default`.
fn usize_arg(args: &Value, name: &str, default: usize) -> usize {
    args[name]
        .as_u64()
        .map_or(default, |n| usize::try_from(n).unwrap_or(usize::MAX))
}

/// Una línea de texto no puede romper el formato Markdown de la lista.
fn one_line(text: &str) -> String {
    text.split_whitespace().collect::<Vec<_>>().join(" ")
}

/// Texto de una línea sin la viñeta: `- [ ] texto` → `texto` si coincide con `marker`.
fn bullet<'a>(line: &'a str, marker: &str) -> Option<&'a str> {
    let l = line.trim_start();
    let l = l.strip_prefix("- ").or_else(|| l.strip_prefix("* "))?;
    let rest = l.strip_prefix(marker)?;
    let rest = rest.trim();
    (!rest.is_empty()).then_some(rest)
}

// ---------------------------------------------------------------- notas

/// `note <texto>`: añade una nota con fecha; `note` solo lista las últimas.
pub struct NotesPlugin {
    /// Archivo Markdown.
    pub file: PathBuf,
}

impl NotesPlugin {
    /// Añade una nota.
    pub fn add(&self, text: &str) -> std::io::Result<()> {
        let (d, t) = stamp();
        append_line(
            &self.file,
            tr("notes.header"),
            &format!("- {d} {t} — {}", text.trim()),
        )
    }

    /// Últimas notas, de la más reciente a la más antigua.
    pub fn recent(&self, n: usize) -> Vec<String> {
        let text = std::fs::read_to_string(&self.file).unwrap_or_default();
        text.lines()
            .rev()
            .filter_map(|l| bullet(l, ""))
            .take(n)
            .map(str::to_owned)
            .collect()
    }
}

impl Plugin for NotesPlugin {
    fn metadata(&self) -> PluginMetadata {
        PluginMetadata {
            id: "notes".into(),
            name: tr("notes.name").into(),
            prefix: None,
            keywords: vec!["note".into(), "nota".into()],
            priority: 0.3,
            global: false,
            list_on_keyword: true,
        }
    }

    fn query(&self, q: &Query, _c: &CancelToken, sink: &mut dyn FnMut(Vec<ResultItem>)) {
        let text = q.text.trim();
        let mut out = Vec::new();
        if text.is_empty() {
            let mut it = ResultItem::new("n:open", tr("notes.open"));
            it.subtitle = self.file.display().to_string();
            it.icon = Icon::Named("note".into());
            it.actions = vec![Action::new(ActionId::OPEN, tr("action.open"))];
            out.push(it);
            for (i, n) in self.recent(10).into_iter().enumerate() {
                let mut it = ResultItem::new(format!("n:{i}"), ellipsis(&n, 120));
                it.subtitle = tr("notes.recent").into();
                it.icon = Icon::Named("note".into());
                it.fuzzy = 0.9 - i as f32 * 0.01;
                it.detail = Some(n);
                it.actions = vec![Action::new(ActionId::OPEN, tr("notes.open"))];
                out.push(it);
            }
        } else {
            let mut it =
                ResultItem::new("n:add", trf("notes.add", &[("text", &ellipsis(text, 100))]));
            it.subtitle = self.file.display().to_string();
            it.icon = Icon::Named("note".into());
            it.alias_match = true;
            it.payload = text.to_owned();
            it.actions = vec![
                Action::new(ActionId::OPEN, tr("notes.save")),
                Action::new(ActionId::EDIT, tr("notes.open")),
            ];
            out.push(it);
        }
        sink(out);
    }

    fn execute(&self, item: &ResultItem, action: &str) -> Result<ActionOutcome, PluginError> {
        if item.id == "n:add" && action == ActionId::OPEN {
            self.add(&item.payload).map_err(failed)?;
            return Ok(ActionOutcome::Hide);
        }
        open_file(&self.file)
    }

    fn commands(&self) -> Vec<PluginCommand> {
        vec![
            PluginCommand::new("add", "Añade una nota con fecha. #{text} → #{text, file}"),
            PluginCommand::new(
                "list",
                "Últimas notas, la más reciente primero. #{limit: 10} → [texto]",
            ),
        ]
    }

    fn call(&self, command: &str, args: &Value) -> Result<Value, PluginError> {
        match command {
            "add" => {
                let text = one_line(text_arg(args, "text")?);
                self.add(&text).map_err(failed)?;
                Ok(json!({ "text": text, "file": self.file.display().to_string() }))
            }
            "list" => Ok(json!(self.recent(usize_arg(args, "limit", 10)))),
            other => Err(PluginError::UnknownCommand(other.into())),
        }
    }
}

// ---------------------------------------------------------------- tareas

/// Tarea pendiente.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Task {
    /// Línea del archivo (desde 0).
    pub line: usize,
    /// Texto.
    pub text: String,
}

/// `todo <texto>` añade una tarea; `todo` lista las pendientes y Enter completa una.
pub struct TodoPlugin {
    /// Archivo Markdown.
    pub file: PathBuf,
}

impl TodoPlugin {
    /// Añade una tarea.
    pub fn add(&self, text: &str) -> std::io::Result<()> {
        append_line(
            &self.file,
            tr("todo.header"),
            &format!("- [ ] {}", text.trim()),
        )
    }

    /// Tareas pendientes en orden del archivo.
    pub fn pending(&self) -> Vec<Task> {
        let text = std::fs::read_to_string(&self.file).unwrap_or_default();
        text.lines()
            .enumerate()
            .filter_map(|(line, l)| {
                bullet(l, "[ ] ").map(|t| Task {
                    line,
                    text: t.to_owned(),
                })
            })
            .collect()
    }

    /// Marca como hecha la tarea `task`. Si el archivo cambió, busca la misma tarea por texto.
    pub fn complete(&self, task: &Task) -> std::io::Result<bool> {
        let text = std::fs::read_to_string(&self.file)?;
        let mut lines: Vec<&str> = text.split('\n').collect();
        let is_it = |l: &str| bullet(l.trim_end_matches('\r'), "[ ] ") == Some(task.text.as_str());
        let idx = if lines.get(task.line).is_some_and(|l| is_it(l)) {
            Some(task.line)
        } else {
            lines.iter().position(|l| is_it(l))
        };
        let Some(idx) = idx else { return Ok(false) };
        let (d, _) = stamp();
        let old = lines[idx];
        let cr = if old.ends_with('\r') { "\r" } else { "" };
        let indent = &old[..old.len() - old.trim_start().len()];
        let done = format!("{indent}- [x] {} ✅ {d}{cr}", task.text);
        lines[idx] = &done;
        std::fs::write(&self.file, lines.join("\n"))?;
        Ok(true)
    }
}

impl Plugin for TodoPlugin {
    fn metadata(&self) -> PluginMetadata {
        PluginMetadata {
            id: "todo".into(),
            name: tr("todo.name").into(),
            prefix: None,
            keywords: vec!["todo".into(), "tarea".into(), "tareas".into()],
            priority: 0.3,
            global: false,
            list_on_keyword: true,
        }
    }

    fn query(&self, q: &Query, _c: &CancelToken, sink: &mut dyn FnMut(Vec<ResultItem>)) {
        let text = q.text.trim();
        let mut out = Vec::new();
        if !text.is_empty() {
            let mut it =
                ResultItem::new("t:add", trf("todo.add", &[("text", &ellipsis(text, 100))]));
            it.subtitle = self.file.display().to_string();
            it.icon = Icon::Named("todo".into());
            it.alias_match = true;
            it.fuzzy = 1.0;
            it.payload = serde_json::json!({ "text": text, "raw": q.raw }).to_string();
            it.actions = vec![
                Action::new(ActionId::OPEN, tr("todo.save")),
                Action::new(ActionId::EDIT, tr("todo.open")),
            ];
            out.push(it);
        }
        let mut f = Fuzzy::new(text);
        let pending = self.pending();
        let n = pending.len();
        for (i, t) in pending.into_iter().enumerate() {
            let score = if text.is_empty() {
                1.0 - i as f32 * 0.001
            } else {
                match f.score(&t.text) {
                    Some(s) => s * 0.9,
                    None => continue,
                }
            };
            let mut it = ResultItem::new(format!("t:{}", t.line), t.text.clone());
            it.subtitle = trf("todo.pending", &[("n", &n)]);
            it.icon = Icon::Named("todo".into());
            it.fuzzy = score;
            it.payload =
                serde_json::json!({ "line": t.line, "text": t.text, "raw": q.raw }).to_string();
            it.actions = vec![
                Action::new(ActionId::OPEN, tr("todo.complete")),
                Action::new(ActionId::EDIT, tr("todo.open")),
            ];
            out.push(it);
        }
        if out.is_empty() {
            let mut it = ResultItem::new("t:none", tr("todo.none"));
            it.subtitle = self.file.display().to_string();
            it.icon = Icon::Named("todo".into());
            it.actions = vec![Action::new(ActionId::OPEN, tr("todo.open"))];
            out.push(it);
        }
        sink(out);
    }

    fn execute(&self, item: &ResultItem, action: &str) -> Result<ActionOutcome, PluginError> {
        if action == ActionId::EDIT || item.id == "t:none" {
            return open_file(&self.file);
        }
        let p: serde_json::Value = serde_json::from_str(&item.payload).map_err(failed)?;
        let text = p["text"].as_str().unwrap_or_default();
        let raw = p["raw"].as_str().unwrap_or_default();
        if item.id == "t:add" {
            self.add(text).map_err(failed)?;
            // Se muestra la lista con la tarea nueva.
            let kw = raw.split_whitespace().next().unwrap_or("todo");
            return Ok(ActionOutcome::ReplaceQuery(format!("{kw} ")));
        }
        let task = Task {
            line: p["line"].as_u64().unwrap_or(0) as usize,
            text: text.to_owned(),
        };
        if !self.complete(&task).map_err(failed)? {
            return Err(PluginError::Failed(tr("todo.gone").into()));
        }
        Ok(ActionOutcome::ReplaceQuery(raw.to_owned()))
    }

    fn commands(&self) -> Vec<PluginCommand> {
        vec![
            PluginCommand::new("add", "Añade una tarea pendiente. #{text} → #{text, file}"),
            PluginCommand::new(
                "list",
                "Tareas pendientes en el orden del archivo. → [#{line, text}]",
            ),
            PluginCommand::new(
                "complete",
                "Marca como hecha la tarea con ese texto exacto. #{text} → true si existía",
            ),
        ]
    }

    fn call(&self, command: &str, args: &Value) -> Result<Value, PluginError> {
        match command {
            "add" => {
                let text = one_line(text_arg(args, "text")?);
                self.add(&text).map_err(failed)?;
                Ok(json!({ "text": text, "file": self.file.display().to_string() }))
            }
            "list" => Ok(Value::Array(
                self.pending()
                    .into_iter()
                    .map(|t| json!({ "line": t.line, "text": t.text }))
                    .collect(),
            )),
            "complete" => {
                let task = Task {
                    line: usize_arg(args, "line", usize::MAX),
                    text: text_arg(args, "text")?.to_owned(),
                };
                Ok(json!(self.complete(&task).map_err(failed)?))
            }
            other => Err(PluginError::UnknownCommand(other.into())),
        }
    }
}

// ---------------------------------------------------------------- recordatorios

/// Recordatorio pendiente.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Reminder {
    /// Identificador.
    pub id: u64,
    /// Vence (segundos Unix).
    pub due: u64,
    /// Texto.
    pub text: String,
}

/// Aviso al vencer: `(título, texto)`.
pub type Notifier = Arc<dyn Fn(&str, &str) + Send + Sync>;

/// Recordatorios con su hilo de aviso. Vive en el host para no duplicar el hilo al recargar los
/// plugins.
pub struct RemindStore {
    file: PathBuf,
    list: Mutex<Vec<Reminder>>,
    next: AtomicU64,
    wake: Sender<()>,
    notifier: Mutex<Option<Notifier>>,
}

impl RemindStore {
    /// Carga los recordatorios y arranca el hilo. Los vencidos avisan en cuanto haya notificador.
    pub fn open(file: PathBuf) -> Arc<Self> {
        let list: Vec<Reminder> = std::fs::read(&file)
            .ok()
            .and_then(|b| serde_json::from_slice(&b).ok())
            .unwrap_or_default();
        let next = list.iter().map(|r| r.id).max().unwrap_or(0) + 1;
        let (tx, rx) = crossbeam_channel::unbounded();
        let s = Arc::new(Self {
            file,
            list: Mutex::new(list),
            next: AtomicU64::new(next),
            wake: tx,
            notifier: Mutex::new(None),
        });
        let weak = Arc::downgrade(&s);
        let spawned = std::thread::Builder::new()
            .name("rukes-reminders".into())
            .spawn(move || scheduler(&weak, &rx));
        if let Err(e) = spawned {
            tracing::warn!(error = %e, "no se pudo arrancar el hilo de recordatorios");
        }
        s
    }

    /// Fija quién muestra los avisos.
    pub fn set_notifier(&self, n: Notifier) {
        *lock(&self.notifier) = Some(n);
        let _ = self.wake.send(());
    }

    fn save(&self) {
        let list = lock(&self.list).clone();
        let result = (|| -> std::io::Result<()> {
            if let Some(dir) = self.file.parent() {
                std::fs::create_dir_all(dir)?;
            }
            let tmp = self.file.with_extension("tmp");
            std::fs::write(&tmp, serde_json::to_vec_pretty(&list)?)?;
            std::fs::rename(&tmp, &self.file)
        })();
        if let Err(e) = result {
            tracing::warn!(error = %e, "no se pudieron guardar los recordatorios");
        }
    }

    /// Crea un recordatorio que vence en `secs` segundos.
    pub fn add(&self, secs: u64, text: &str) -> Reminder {
        let r = Reminder {
            id: self.next.fetch_add(1, Ordering::SeqCst),
            due: now_unix() + secs,
            text: text.trim().to_owned(),
        };
        lock(&self.list).push(r.clone());
        self.save();
        let _ = self.wake.send(());
        r
    }

    /// Cancela uno. Devuelve si existía.
    pub fn cancel(&self, id: u64) -> bool {
        let removed = {
            let mut l = lock(&self.list);
            let before = l.len();
            l.retain(|r| r.id != id);
            l.len() != before
        };
        if removed {
            self.save();
        }
        removed
    }

    /// Pendientes, del más próximo al más lejano.
    pub fn pending(&self) -> Vec<Reminder> {
        let mut v = lock(&self.list).clone();
        v.sort_by_key(|r| r.due);
        v
    }

    /// Saca los vencidos (si hay notificador) y devuelve cuántos segundos faltan para el siguiente.
    fn fire_due(&self) -> Option<u64> {
        let notifier = lock(&self.notifier).clone();
        let now = now_unix();
        let due: Vec<Reminder> = if notifier.is_some() {
            let mut l = lock(&self.list);
            let (due, rest): (Vec<_>, Vec<_>) = l.drain(..).partition(|r| r.due <= now);
            *l = rest;
            due
        } else {
            Vec::new()
        };
        if let Some(n) = &notifier
            && !due.is_empty()
        {
            self.save();
            for r in &due {
                let text = if r.text.is_empty() {
                    tr("remind.default").to_owned()
                } else {
                    r.text.clone()
                };
                n(tr("remind.title"), &text);
            }
        }
        lock(&self.list)
            .iter()
            .map(|r| r.due.saturating_sub(now))
            .min()
    }
}

fn scheduler(store: &Weak<RemindStore>, wake: &Receiver<()>) {
    loop {
        let wait = match store.upgrade() {
            Some(s) => s.fire_due(),
            None => return,
        };
        // Como mucho un minuto: así se recupera de cambios de hora o suspensiones.
        let wait = Duration::from_secs(wait.unwrap_or(60).clamp(1, 60));
        match wake.recv_timeout(wait) {
            Ok(()) | Err(RecvTimeoutError::Timeout) => {}
            Err(RecvTimeoutError::Disconnected) => return,
        }
    }
}

/// `15m`, `1h30m`, `30s`, `2h`, `1d` o `14:30` (hoy, o mañana si ya pasó). Devuelve segundos.
pub fn parse_when(s: &str, now: (u32, u32, u32)) -> Option<u64> {
    let s = s.trim().to_lowercase();
    if let Some((h, m)) = s.split_once(':') {
        let (h, m): (u32, u32) = (h.parse().ok()?, m.parse().ok()?);
        if h > 23 || m > 59 {
            return None;
        }
        let (nh, nm, ns) = now;
        let target = h * 3600 + m * 60;
        let cur = nh * 3600 + nm * 60 + ns;
        let d = if target > cur {
            target - cur
        } else {
            target + 86_400 - cur
        };
        return Some(u64::from(d));
    }
    let mut total = 0u64;
    let mut num = String::new();
    let mut any = false;
    for c in s.chars() {
        if c.is_ascii_digit() {
            num.push(c);
            continue;
        }
        let n: u64 = num.parse().ok()?;
        num.clear();
        total += n * match c {
            's' => 1,
            'm' => 60,
            'h' => 3600,
            'd' => 86_400,
            _ => return None,
        };
        any = true;
    }
    if !num.is_empty() {
        // Un número suelto son minutos: `remind 10 té`.
        total += num.parse::<u64>().ok()? * 60;
        any = true;
    }
    (any && total > 0 && total <= 366 * 86_400).then_some(total)
}

/// «en 15 min (14:35)».
fn when_label(due: u64) -> String {
    let left = due.saturating_sub(now_unix());
    let rel = if left < 60 {
        trf("remind.in", &[("n", &left), ("unit", &"s")])
    } else if left < 3600 {
        trf("remind.in", &[("n", &left.div_ceil(60)), ("unit", &"min")])
    } else if left < 86_400 {
        let (h, m) = (left / 3600, (left % 3600) / 60);
        trf(
            "remind.in",
            &[("n", &format!("{h} h {m:02}")), ("unit", &"min")],
        )
    } else {
        trf("remind.in", &[("n", &(left / 86_400)), ("unit", &"d")])
    };
    let at = rukes_platform::time::local_from(std::time::UNIX_EPOCH + Duration::from_secs(due))
        .map(|t| format!(" ({:02}:{:02})", t.hour, t.minute))
        .unwrap_or_default();
    format!("{rel}{at}")
}

/// `remind 15m llamar a Ana`; `remind` solo lista los pendientes (Ctrl+D cancela).
pub struct RemindPlugin {
    /// Recordatorios.
    pub store: Arc<RemindStore>,
}

impl Plugin for RemindPlugin {
    fn metadata(&self) -> PluginMetadata {
        PluginMetadata {
            id: "remind".into(),
            name: tr("remind.name").into(),
            prefix: None,
            keywords: vec!["remind".into(), "recordar".into()],
            priority: 0.3,
            global: false,
            list_on_keyword: true,
        }
    }

    fn query(&self, q: &Query, _c: &CancelToken, sink: &mut dyn FnMut(Vec<ResultItem>)) {
        let text = q.text.trim();
        let mut out = Vec::new();
        if !text.is_empty() {
            let (when, what) = text
                .split_once(char::is_whitespace)
                .map_or((text, ""), |(a, b)| (a, b.trim()));
            let t = rukes_platform::time::local_now();
            let mut it = match parse_when(when, (t.hour, t.minute, t.second)) {
                Some(secs) => {
                    let label = if what.is_empty() {
                        tr("remind.default")
                    } else {
                        what
                    };
                    let mut it = ResultItem::new(
                        "r:add",
                        trf("remind.add", &[("text", &ellipsis(label, 80))]),
                    );
                    it.subtitle = when_label(now_unix() + secs);
                    it.payload = serde_json::json!({ "secs": secs, "text": what }).to_string();
                    it.actions = vec![Action::new(ActionId::OPEN, tr("remind.save"))];
                    it
                }
                None => {
                    let mut it = ResultItem::new("r:help", tr("remind.help"));
                    it.subtitle = tr("remind.help_sub").into();
                    it
                }
            };
            it.icon = Icon::Named("remind".into());
            it.alias_match = true;
            it.fuzzy = 1.0;
            out.push(it);
        } else {
            for (i, r) in self.store.pending().into_iter().enumerate() {
                let title = if r.text.is_empty() {
                    tr("remind.default").to_owned()
                } else {
                    r.text.clone()
                };
                let mut it = ResultItem::new(format!("r:{}", r.id), title);
                it.subtitle = format!("{} · {}", when_label(r.due), tr("remind.cancel_hint"));
                it.icon = Icon::Named("remind".into());
                it.fuzzy = 1.0 - i as f32 * 0.001;
                it.payload = serde_json::json!({ "id": r.id, "raw": q.raw }).to_string();
                it.actions = vec![Action::new(ActionId::DELETE, tr("remind.cancel"))];
                out.push(it);
            }
            if out.is_empty() {
                let mut it = ResultItem::new("r:help", tr("remind.none"));
                it.subtitle = tr("remind.help_sub").into();
                it.icon = Icon::Named("remind".into());
                out.push(it);
            }
        }
        sink(out);
    }

    fn execute(&self, item: &ResultItem, action: &str) -> Result<ActionOutcome, PluginError> {
        if item.id == "r:help" {
            return Ok(ActionOutcome::KeepOpen);
        }
        let p: serde_json::Value = serde_json::from_str(&item.payload).map_err(failed)?;
        if item.id == "r:add" {
            let secs = p["secs"].as_u64().unwrap_or(60);
            let r = self.store.add(secs, p["text"].as_str().unwrap_or_default());
            return Ok(ActionOutcome::Notify(trf(
                "remind.set",
                &[("when", &when_label(r.due))],
            )));
        }
        match action {
            ActionId::DELETE => {
                self.store.cancel(p["id"].as_u64().unwrap_or(0));
                Ok(ActionOutcome::ReplaceQuery(
                    p["raw"].as_str().unwrap_or_default().to_owned(),
                ))
            }
            // Enter sobre un pendiente: nada que hacer.
            _ => Ok(ActionOutcome::KeepOpen),
        }
    }

    fn commands(&self) -> Vec<PluginCommand> {
        vec![
            PluginCommand::new(
                "add",
                "Crea un recordatorio. #{when: \"15m\" | \"1h30m\" | \"14:30\"} o #{seconds} o \
                 #{at: segundos Unix}, y #{text} → #{id, due, text}",
            ),
            PluginCommand::new(
                "list",
                "Pendientes, el más próximo primero. → [#{id, due, text}]",
            ),
            PluginCommand::new("cancel", "Cancela uno. #{id} → true si existía"),
        ]
    }

    fn call(&self, command: &str, args: &Value) -> Result<Value, PluginError> {
        let json_of = |r: &Reminder| json!({ "id": r.id, "due": r.due, "text": r.text });
        match command {
            "add" => {
                let secs = reminder_delay(args, now_unix())?;
                let text = args["text"].as_str().map(one_line).unwrap_or_default();
                Ok(json_of(&self.store.add(secs, &text)))
            }
            "list" => Ok(Value::Array(
                self.store.pending().iter().map(json_of).collect(),
            )),
            "cancel" => {
                let id = args["id"]
                    .as_u64()
                    .ok_or_else(|| PluginError::BadArgs("falta «id» (número)".into()))?;
                Ok(json!(self.store.cancel(id)))
            }
            other => Err(PluginError::UnknownCommand(other.into())),
        }
    }
}

/// Máximo de antelación de un recordatorio.
const MAX_DELAY: u64 = 366 * 86_400;

/// Segundos hasta que venza un recordatorio pedido con `when`, `seconds` o `at` (Unix).
fn reminder_delay(args: &Value, now: u64) -> Result<u64, PluginError> {
    let secs = if let Some(w) = args["when"].as_str() {
        let t = rukes_platform::time::local_now();
        parse_when(w, (t.hour, t.minute, t.second)).ok_or_else(|| {
            PluginError::BadArgs(format!(
                "«when» no válido: {w} (usa 15m, 1h30m, 2d o 14:30)"
            ))
        })?
    } else if let Some(s) = args["seconds"].as_u64() {
        s
    } else if let Some(at) = args["at"].as_u64() {
        at.checked_sub(now)
            .ok_or_else(|| PluginError::BadArgs("«at» ya pasó".into()))?
    } else {
        return Err(PluginError::BadArgs(
            "falta «when» (15m, 14:30...), «seconds» o «at»".into(),
        ));
    };
    if secs > MAX_DELAY {
        return Err(PluginError::BadArgs(
            "como mucho un año de antelación".into(),
        ));
    }
    Ok(secs)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tmp(name: &str) -> PathBuf {
        let d = std::env::temp_dir().join(format!("rukes-notes-{}-{name}", std::process::id()));
        let _ = std::fs::remove_dir_all(&d);
        d
    }

    #[test]
    fn parses_durations_and_clock_times() {
        let now = (10, 0, 0);
        assert_eq!(parse_when("15m", now), Some(900));
        assert_eq!(parse_when("1h30m", now), Some(5400));
        assert_eq!(parse_when("30s", now), Some(30));
        assert_eq!(parse_when("10", now), Some(600));
        assert_eq!(parse_when("2D", now), Some(172_800));
        assert_eq!(parse_when("10:30", now), Some(1800));
        assert_eq!(parse_when("9:00", now), Some(23 * 3600));
        assert_eq!(parse_when("10:00", now), Some(86_400));
        for bad in ["", "abc", "15x", "25:00", "0m", "llamar"] {
            assert_eq!(parse_when(bad, now), None, "{bad}");
        }
    }

    #[test]
    fn notes_and_tasks_are_plain_markdown() {
        let _ = rukes_core::i18n::set_language("es", "");
        let dir = tmp("md");
        let notes = NotesPlugin {
            file: dir.join("notas.md"),
        };
        notes.add("primera").unwrap();
        notes.add("segunda").unwrap();
        let text = std::fs::read_to_string(&notes.file).unwrap();
        assert!(text.starts_with("# "));
        assert!(text.trim_end().ends_with("— segunda"));
        assert!(notes.recent(5)[0].ends_with("segunda"));

        let todo = TodoPlugin {
            file: dir.join("tareas.md"),
        };
        todo.add("comprar pan").unwrap();
        todo.add("llamar a Ana").unwrap();
        let p = todo.pending();
        assert_eq!(p.len(), 2);
        // Si el archivo cambió por fuera, se busca la tarea por su texto.
        std::fs::write(
            &todo.file,
            format!("intro\r\n{}", std::fs::read_to_string(&todo.file).unwrap()),
        )
        .unwrap();
        assert!(todo.complete(&p[1]).unwrap());
        assert!(!todo.complete(&p[1]).unwrap());
        let text = std::fs::read_to_string(&todo.file).unwrap();
        assert!(text.contains("- [x] llamar a Ana ✅ "));
        assert!(text.contains("- [ ] comprar pan"));
        assert_eq!(todo.pending().len(), 1);
        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn todo_query_and_actions() {
        let _ = rukes_core::i18n::set_language("es", "");
        let dir = tmp("todo");
        let todo = TodoPlugin {
            file: dir.join("t.md"),
        };
        let q = |raw: &str, text: &str| Query {
            id: rukes_core::QueryId(1),
            raw: raw.into(),
            text: text.into(),
            targeted: true,
        };
        let mut got = Vec::new();
        todo.query(&q("todo pan", "pan"), &CancelToken::default(), &mut |v| {
            got = v
        });
        assert_eq!(got[0].id, "t:add");
        assert!(matches!(
            todo.execute(&got[0], ActionId::OPEN).unwrap(),
            ActionOutcome::ReplaceQuery(r) if r == "todo "
        ));
        todo.query(&q("todo ", ""), &CancelToken::default(), &mut |v| got = v);
        assert_eq!(got[0].title, "pan");
        assert!(matches!(
            todo.execute(&got[0], ActionId::OPEN).unwrap(),
            ActionOutcome::ReplaceQuery(r) if r == "todo "
        ));
        assert!(todo.pending().is_empty());
        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn commands_for_other_plugins_and_scripts() {
        let _ = rukes_core::i18n::set_language("es", "");
        let dir = tmp("cmds");
        let notes = NotesPlugin {
            file: dir.join("n.md"),
        };
        let r = notes
            .call("add", &json!({ "text": "desde\nTelegram  " }))
            .unwrap();
        assert_eq!(r["text"], "desde Telegram");
        assert!(notes.recent(1)[0].ends_with("— desde Telegram"));
        assert!(
            notes.call("list", &json!({})).unwrap()[0]
                .as_str()
                .unwrap()
                .ends_with("desde Telegram")
        );
        assert!(matches!(
            notes.call("add", &json!({ "text": "  " })),
            Err(PluginError::BadArgs(_))
        ));
        assert!(matches!(
            notes.call("borrar", &json!({})),
            Err(PluginError::UnknownCommand(_))
        ));

        let todo = TodoPlugin {
            file: dir.join("t.md"),
        };
        todo.call("add", &json!({ "text": "pan" })).unwrap();
        assert_eq!(
            todo.call("list", &json!(null)).unwrap(),
            json!([{ "line": 2, "text": "pan" }])
        );
        assert_eq!(
            todo.call("complete", &json!({ "text": "pan" })).unwrap(),
            json!(true)
        );
        assert_eq!(
            todo.call("complete", &json!({ "text": "pan" })).unwrap(),
            json!(false)
        );

        let remind = RemindPlugin {
            store: RemindStore::open(dir.join("r.json")),
        };
        let r = remind
            .call("add", &json!({ "seconds": 600, "text": "té" }))
            .unwrap();
        assert_eq!(r["text"], "té");
        let id = r["id"].as_u64().unwrap();
        assert!(remind.call("add", &json!({ "when": "15m" })).is_ok());
        for bad in [
            json!({ "when": "pronto" }),
            json!({ "at": 1 }),
            json!({ "seconds": 400 * 86_400 }),
            json!({ "text": "sin cuándo" }),
        ] {
            assert!(
                matches!(remind.call("add", &bad), Err(PluginError::BadArgs(_))),
                "{bad}"
            );
        }
        assert_eq!(remind.call("list", &json!({})).unwrap()[0]["id"], id);
        assert_eq!(
            remind.call("cancel", &json!({ "id": id })).unwrap(),
            json!(true)
        );
        assert_eq!(
            remind
                .call("list", &json!({}))
                .unwrap()
                .as_array()
                .unwrap()
                .len(),
            1
        );
        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn reminders_fire_persist_and_cancel() {
        let dir = tmp("remind");
        let file = dir.join("reminders.json");
        let store = RemindStore::open(file.clone());
        let a = store.add(3600, "más tarde");
        let b = store.add(0, "ya");
        assert_eq!(store.pending().len(), 2);
        // Persisten.
        drop(store);
        let store = RemindStore::open(file.clone());
        assert_eq!(store.pending().len(), 2);
        let (tx, rx) = crossbeam_channel::unbounded::<String>();
        store.set_notifier(Arc::new(move |_t: &str, body: &str| {
            let _ = tx.send(body.to_owned());
        }));
        assert_eq!(rx.recv_timeout(Duration::from_secs(5)).unwrap(), "ya");
        let p = store.pending();
        assert_eq!(p, std::slice::from_ref(&a));
        assert_ne!(a.id, b.id);
        assert!(store.cancel(a.id));
        assert!(!store.cancel(a.id));
        assert!(store.pending().is_empty());
        let _ = std::fs::remove_dir_all(dir);
    }
}
