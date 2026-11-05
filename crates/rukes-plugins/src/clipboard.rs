//! Portapapeles con historial (sección 5.10, RF-100 a RF-104).
//!
//! El historial vive en memoria y se guarda cifrado con DPAPI (ligado al usuario de Windows) en
//! `clipboard/history.bin`. La escucha del portapapeles la hace la aplicación y llama a
//! [`ClipHistory::record`].

use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex, MutexGuard, RwLock, Weak};
use std::time::Duration;

use rukes_core::i18n::{tr, trf};
use rukes_core::{
    Action, ActionId, ActionOutcome, CancelToken, Fuzzy, Icon, Plugin, PluginError, PluginMetadata,
    Query, ResultItem,
};
use serde::{Deserialize, Serialize};

use crate::util::{ago, ellipsis, now_unix};

/// Copias más largas que esto no se guardan.
pub const MAX_CHARS: usize = 100_000;
/// Resultados que se muestran como mucho.
const MAX_SHOWN: usize = 100;
/// Texto que se usa para buscar dentro de cada entrada.
const SEARCH_CHARS: usize = 2_000;
/// Texto completo para el panel de previsualización.
const DETAIL_CHARS: usize = 4_000;
/// Espera antes de guardar (agrupa copias seguidas).
const SAVE_DELAY: Duration = Duration::from_millis(800);
const FORMAT_VERSION: u32 = 1;

/// Ajustes del historial.
#[derive(Debug, Clone, PartialEq)]
pub struct ClipSettings {
    /// Entradas sin fijar que se conservan.
    pub max_items: usize,
    /// Días tras los que caduca una entrada sin fijar (0 = nunca).
    pub expire_days: u32,
    /// Ejecutables (en minúsculas) cuyas copias no se registran.
    pub exclude: Vec<String>,
    /// `true`: Enter pega en la ventana anterior; `false`: solo copia.
    pub paste: bool,
}

impl Default for ClipSettings {
    fn default() -> Self {
        Self {
            max_items: 500,
            expire_days: 30,
            exclude: Vec::new(),
            paste: true,
        }
    }
}

/// Una entrada del historial.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ClipEntry {
    /// Id estable.
    pub id: u64,
    /// Texto.
    pub text: String,
    /// Última vez copiado o usado (segundos Unix).
    pub time: u64,
    /// Fijado: no caduca ni cuenta para el límite.
    pub pinned: bool,
    /// Programa que lo copió.
    pub source: Option<String>,
}

#[derive(Serialize, Deserialize)]
struct Stored {
    version: u32,
    entries: Vec<ClipEntry>,
}

/// Historial compartido entre la escucha y el plugin.
pub struct ClipHistory {
    file: PathBuf,
    entries: Mutex<Vec<ClipEntry>>,
    settings: RwLock<ClipSettings>,
    next: AtomicU64,
    save_tx: crossbeam_channel::Sender<()>,
}

fn lock<T>(m: &Mutex<T>) -> MutexGuard<'_, T> {
    m.lock().unwrap_or_else(std::sync::PoisonError::into_inner)
}

/// Normaliza el nombre de un ejecutable para compararlo (`KeePass.EXE` → `keepass.exe`).
pub fn exe_key(s: &str) -> String {
    let s = s.trim().to_lowercase();
    if s.ends_with(".exe") {
        s
    } else {
        format!("{s}.exe")
    }
}

impl ClipHistory {
    /// Abre el historial de `file` (si no se puede descifrar, empieza vacío).
    pub fn open(file: PathBuf, settings: ClipSettings) -> Arc<Self> {
        let entries = Self::load(&file).unwrap_or_default();
        let next = entries.iter().map(|e| e.id).max().unwrap_or(0) + 1;
        let (tx, rx) = crossbeam_channel::unbounded::<()>();
        let h = Arc::new(Self {
            file,
            entries: Mutex::new(entries),
            settings: RwLock::new(settings),
            next: AtomicU64::new(next),
            save_tx: tx,
        });
        h.prune();
        let weak: Weak<Self> = Arc::downgrade(&h);
        let _ = std::thread::Builder::new()
            .name("rukes-clip-save".into())
            .spawn(move || {
                while rx.recv().is_ok() {
                    while rx.recv_timeout(SAVE_DELAY).is_ok() {}
                    match weak.upgrade() {
                        Some(h) => h.save_now(),
                        None => break,
                    }
                }
            });
        h
    }

    fn load(file: &std::path::Path) -> Option<Vec<ClipEntry>> {
        let enc = std::fs::read(file).ok()?;
        let plain = match rukes_platform::dpapi::unprotect(&enc) {
            Ok(p) => p,
            Err(e) => {
                tracing::warn!(%e, "no se pudo descifrar el historial del portapapeles");
                return None;
            }
        };
        let s: Stored = serde_json::from_slice(&plain).ok()?;
        (s.version == FORMAT_VERSION).then_some(s.entries)
    }

    /// Guarda ya (al salir).
    pub fn save_now(&self) {
        let entries = lock(&self.entries).clone();
        let res = serde_json::to_vec(&Stored {
            version: FORMAT_VERSION,
            entries,
        })
        .map_err(|e| e.to_string())
        .and_then(|plain| rukes_platform::dpapi::protect(&plain).map_err(|e| e.to_string()))
        .and_then(|enc| {
            if let Some(dir) = self.file.parent() {
                std::fs::create_dir_all(dir).map_err(|e| e.to_string())?;
            }
            let tmp = self.file.with_extension("tmp");
            std::fs::write(&tmp, enc)
                .and_then(|()| std::fs::rename(&tmp, &self.file))
                .map_err(|e| e.to_string())
        });
        if let Err(e) = res {
            tracing::warn!(%e, "no se pudo guardar el historial del portapapeles");
        }
    }

    fn changed(&self) {
        let _ = self.save_tx.send(());
    }

    /// Cambia los ajustes.
    pub fn set_settings(&self, s: ClipSettings) {
        *self
            .settings
            .write()
            .unwrap_or_else(std::sync::PoisonError::into_inner) = s;
        self.prune();
        self.changed();
    }

    /// Ajustes actuales.
    pub fn settings(&self) -> ClipSettings {
        self.settings
            .read()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .clone()
    }

    /// Registra una copia. Devuelve `false` si se descartó (vacía, demasiado larga o de una
    /// aplicación excluida).
    pub fn record(&self, text: &str, source: Option<&str>) -> bool {
        self.record_at(text, source, now_unix())
    }

    fn record_at(&self, text: &str, source: Option<&str>, now: u64) -> bool {
        if text.trim().is_empty() || text.chars().count() > MAX_CHARS {
            return false;
        }
        let settings = self.settings();
        if let Some(src) = source
            && settings.exclude.iter().any(|x| exe_key(x) == exe_key(src))
        {
            tracing::debug!(source = src, "copia de una aplicación excluida");
            return false;
        }
        {
            let mut e = lock(&self.entries);
            if let Some(pos) = e.iter().position(|x| x.text == text) {
                let mut old = e.remove(pos);
                old.time = now;
                old.source = source.map(str::to_owned).or(old.source);
                e.insert(0, old);
            } else {
                e.insert(
                    0,
                    ClipEntry {
                        id: self.next.fetch_add(1, Ordering::SeqCst),
                        text: text.to_owned(),
                        time: now,
                        pinned: false,
                        source: source.map(str::to_owned),
                    },
                );
            }
        }
        self.prune_at(now_unix().max(now));
        self.changed();
        true
    }

    fn prune(&self) {
        self.prune_at(now_unix());
    }

    /// Aplica el límite y la caducidad (las fijadas no cuentan).
    fn prune_at(&self, now: u64) {
        let s = self.settings();
        let max_age = u64::from(s.expire_days) * 86_400;
        let mut kept = 0;
        lock(&self.entries).retain(|e| {
            if e.pinned {
                return true;
            }
            if s.expire_days > 0 && now.saturating_sub(e.time) > max_age {
                return false;
            }
            kept += 1;
            kept <= s.max_items
        });
    }

    /// Entradas: fijadas primero, luego de la más reciente a la más antigua.
    pub fn entries(&self) -> Vec<ClipEntry> {
        let mut v = lock(&self.entries).clone();
        v.sort_by(|a, b| b.pinned.cmp(&a.pinned).then(b.time.cmp(&a.time)));
        v
    }

    fn get(&self, id: u64) -> Option<ClipEntry> {
        lock(&self.entries).iter().find(|e| e.id == id).cloned()
    }

    /// Fija o suelta. Devuelve el estado nuevo.
    pub fn toggle_pin(&self, id: u64) -> Option<bool> {
        let r = lock(&self.entries)
            .iter_mut()
            .find(|e| e.id == id)
            .map(|e| {
                e.pinned = !e.pinned;
                e.pinned
            });
        self.changed();
        r
    }

    /// Borra una entrada.
    pub fn remove(&self, id: u64) {
        lock(&self.entries).retain(|e| e.id != id);
        self.changed();
    }

    /// Marca una entrada como usada (sube al principio).
    pub fn touch(&self, id: u64) {
        if let Some(e) = lock(&self.entries).iter_mut().find(|e| e.id == id) {
            e.time = now_unix();
        }
        self.changed();
    }
}

/// Plugin `clip`.
pub struct ClipboardPlugin {
    /// Historial.
    pub history: Arc<ClipHistory>,
    /// Palabra clave.
    pub keyword: String,
}

/// Primera línea con texto, en una línea.
fn title_of(text: &str) -> String {
    let first = text.lines().find(|l| !l.trim().is_empty()).unwrap_or("");
    let more = text.trim().lines().count() > 1;
    let t = ellipsis(first.trim(), 120);
    if more { format!("{t} ⏎") } else { t }
}

impl ClipboardPlugin {
    fn item(&self, e: &ClipEntry, raw: &str, score: f32) -> ResultItem {
        let mut it = ResultItem::new(format!("c:{}", e.id), title_of(&e.text));
        let mut sub = vec![ago(e.time)];
        if e.pinned {
            sub.insert(0, tr("clip.pinned").into());
        }
        sub.push(trf("clip.chars", &[("n", &e.text.chars().count())]));
        if let Some(s) = &e.source {
            sub.push(s.clone());
        }
        it.subtitle = sub.join(" · ");
        it.icon = Icon::Named("clipboard".into());
        it.fuzzy = score;
        it.detail = Some(e.text.chars().take(DETAIL_CHARS).collect());
        let paste = self.history.settings().paste;
        it.actions = vec![
            Action::new(
                ActionId::OPEN,
                tr(if paste { "clip.paste" } else { "clip.copy" }),
            ),
            Action::new(ActionId::COPY_PATH, tr("clip.copy")),
            Action::new(
                ActionId::PIN,
                tr(if e.pinned { "clip.unpin" } else { "clip.pin" }),
            ),
            Action::new(ActionId::DELETE, tr("clip.delete")),
        ];
        // Id y consulta: tras fijar o borrar se repite la búsqueda para refrescar la lista.
        it.payload = format!("{}\n{raw}", e.id);
        it
    }

    /// Resultados para un texto.
    pub fn search(&self, text: &str, raw: &str) -> Vec<ResultItem> {
        let entries = self.history.entries();
        let n = entries.len().max(1) as f32;
        if text.trim().is_empty() {
            return entries
                .iter()
                .take(MAX_SHOWN)
                .enumerate()
                .map(|(i, e)| self.item(e, raw, 1.0 - i as f32 / (n + 1.0)))
                .collect();
        }
        let mut f = Fuzzy::new(text);
        let mut found: Vec<(f32, usize)> = entries
            .iter()
            .enumerate()
            .filter_map(|(i, e)| {
                let hay: String = e.text.chars().take(SEARCH_CHARS).collect();
                // A igualdad de coincidencia, las más recientes primero.
                f.score(&hay).map(|s| (s - i as f32 / (n * 100.0), i))
            })
            .collect();
        found.sort_by(|a, b| b.0.total_cmp(&a.0));
        found
            .into_iter()
            .take(MAX_SHOWN)
            .map(|(s, i)| self.item(&entries[i], raw, s))
            .collect()
    }
}

fn parse_payload(p: &str) -> Option<(u64, &str)> {
    let (id, raw) = p.split_once('\n')?;
    Some((id.parse().ok()?, raw))
}

impl Plugin for ClipboardPlugin {
    fn metadata(&self) -> PluginMetadata {
        PluginMetadata {
            id: "clipboard".into(),
            name: tr("clip.name").into(),
            prefix: None,
            keywords: vec![self.keyword.clone()],
            priority: 0.3,
            global: false,
            list_on_keyword: true,
        }
    }

    fn query(&self, q: &Query, _cancel: &CancelToken, sink: &mut dyn FnMut(Vec<ResultItem>)) {
        sink(self.search(&q.text, &q.raw));
    }

    fn execute(&self, item: &ResultItem, action: &str) -> Result<ActionOutcome, PluginError> {
        let (id, raw) =
            parse_payload(&item.payload).ok_or_else(|| PluginError::Failed("payload".into()))?;
        let entry = self
            .history
            .get(id)
            .ok_or_else(|| PluginError::Failed(tr("clip.gone").into()))?;
        match action {
            ActionId::OPEN if self.history.settings().paste => {
                self.history.touch(id);
                Ok(ActionOutcome::Paste {
                    text: entry.text,
                    cursor_left: 0,
                    restore: false,
                })
            }
            ActionId::OPEN | ActionId::COPY_PATH => {
                self.history.touch(id);
                rukes_platform::clipboard::set_text(&entry.text)
                    .map_err(|e| PluginError::Failed(e.to_string()))?;
                Ok(ActionOutcome::Hide)
            }
            ActionId::PIN => {
                self.history.toggle_pin(id);
                Ok(ActionOutcome::ReplaceQuery(raw.to_owned()))
            }
            ActionId::DELETE => {
                self.history.remove(id);
                Ok(ActionOutcome::ReplaceQuery(raw.to_owned()))
            }
            other => Err(PluginError::UnsupportedAction(other.into())),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn history(name: &str, s: ClipSettings) -> (Arc<ClipHistory>, PathBuf) {
        let dir = std::env::temp_dir().join(format!("rukes-clip-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        let file = dir.join("history.bin");
        (ClipHistory::open(file, s), dir)
    }

    #[test]
    fn records_dedupes_limits_and_excludes() {
        let (h, dir) = history(
            "rec",
            ClipSettings {
                max_items: 3,
                expire_days: 1,
                exclude: vec!["KeePass".into()],
                paste: true,
            },
        );
        assert!(h.record("uno", Some("code.exe")));
        assert!(h.record("dos", None));
        assert!(!h.record("   ", None));
        assert!(!h.record("secreto", Some("keepass.exe")));
        assert!(!h.record(&"x".repeat(MAX_CHARS + 1), None));
        assert!(h.record("uno", Some("notepad.exe")));
        let e = h.entries();
        assert_eq!(e.len(), 2);
        assert_eq!(e[0].text, "uno");
        assert_eq!(e[0].source.as_deref(), Some("notepad.exe"));
        // Fijada: no cuenta para el límite ni caduca.
        let dos = e[1].id;
        assert_eq!(h.toggle_pin(dos), Some(true));
        for t in ["a", "b", "c", "d"] {
            h.record(t, None);
        }
        let e = h.entries();
        assert_eq!(e[0].text, "dos");
        assert_eq!(e.len(), 4);
        assert!(!e.iter().any(|x| x.text == "uno"));
        // Caducidad.
        h.record_at("viejo", None, 1);
        assert!(!h.entries().iter().any(|x| x.text == "viejo"));
        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn persists_encrypted() {
        let (h, dir) = history("save", ClipSettings::default());
        h.record("contraseña-no-guardada-en-claro", None);
        h.save_now();
        let raw = std::fs::read(dir.join("history.bin")).unwrap();
        let needle = b"no-guardada-en-claro";
        assert!(!raw.windows(needle.len()).any(|w| w == needle));
        let again = ClipHistory::open(dir.join("history.bin"), ClipSettings::default());
        assert_eq!(again.entries()[0].text, "contraseña-no-guardada-en-claro");
        // Archivo corrupto: se empieza de cero.
        std::fs::write(dir.join("history.bin"), b"basura").unwrap();
        assert!(
            ClipHistory::open(dir.join("history.bin"), ClipSettings::default())
                .entries()
                .is_empty()
        );
        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn search_and_actions() {
        let _ = rukes_core::i18n::set_language("es", "");
        let (h, dir) = history("search", ClipSettings::default());
        h.record("git status", None);
        h.record("línea uno\nlínea dos", None);
        let p = ClipboardPlugin {
            history: Arc::clone(&h),
            keyword: "clip".into(),
        };
        let all = p.search("", "clip ");
        assert_eq!(all.len(), 2);
        assert_eq!(all[0].title, "línea uno ⏎");
        let found = p.search("git", "clip git");
        assert_eq!(found[0].title, "git status");
        assert_eq!(found[0].detail.as_deref(), Some("git status"));
        match p.execute(&found[0], ActionId::OPEN).unwrap() {
            ActionOutcome::Paste { text, .. } => assert_eq!(text, "git status"),
            other => panic!("{other:?}"),
        }
        assert!(matches!(
            p.execute(&found[0], ActionId::PIN).unwrap(),
            ActionOutcome::ReplaceQuery(q) if q == "clip git"
        ));
        assert!(
            p.search("git", "")[0]
                .subtitle
                .starts_with(tr("clip.pinned"))
        );
        p.execute(&found[0], ActionId::DELETE).unwrap();
        assert!(p.search("git", "").is_empty());
        let _ = std::fs::remove_dir_all(dir);
    }
}
