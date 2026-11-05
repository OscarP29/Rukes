//! Búsqueda de archivos con índice propio, caché en disco y watcher (RF-040 a RF-044).

pub mod index;

use std::collections::BTreeSet;
use std::hash::{Hash as _, Hasher as _};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex, RwLock};
use std::time::{Duration, Instant};

use notify::{EventKind, RecursiveMode, Watcher as _};
use rukes_core::i18n::tr;
use rukes_core::{
    Action, ActionId, ActionOutcome, CancelToken, Icon, Plugin, PluginError, PluginMetadata, Query,
    ResultItem,
};
use rukes_platform::shell::{self, Verb};

pub use index::{Index, Kind, Rules};

use crate::util;

/// Resultados por consulta.
const LIMIT: usize = 30;
/// Espera para agrupar eventos del watcher.
const DEBOUNCE: Duration = Duration::from_millis(400);
/// Cada cuánto se guarda la caché si hubo cambios.
const SAVE_EVERY: Duration = Duration::from_secs(30);
/// Un `refresh` (al mostrar la ventana o «Recargar») solo reindexa si el último recorrido
/// completo es más antiguo que esto: el watcher mantiene el índice al día.
const RESCAN_AFTER: Duration = Duration::from_secs(30 * 60);

/// Ajustes del plugin de archivos.
#[derive(Debug, Clone, PartialEq)]
pub struct FilesSettings {
    /// Carpetas indexadas (ya expandidas).
    pub roots: Vec<PathBuf>,
    /// Patrones excluidos.
    pub exclude: Vec<String>,
    /// Indexar ocultos.
    pub include_hidden: bool,
    /// Caracteres mínimos para buscar sin prefijo.
    pub min_chars: usize,
    /// Prioridad.
    pub priority: f32,
    /// Shell para «abrir terminal aquí».
    pub shell: String,
    /// Terminal (`wt` o vacío).
    pub terminal: String,
    /// Buscar con Everything si está abierto (RF-041).
    pub everything: bool,
}

impl FilesSettings {
    fn hash(&self) -> u64 {
        let mut h = std::collections::hash_map::DefaultHasher::new();
        self.roots.hash(&mut h);
        self.exclude.hash(&mut h);
        self.include_hidden.hash(&mut h);
        h.finish()
    }
}

/// Expande `~` a la carpeta del usuario.
pub fn expand_root(s: &str) -> PathBuf {
    let home = std::env::var_os("USERPROFILE")
        .map(PathBuf::from)
        .unwrap_or_default();
    match s.strip_prefix('~') {
        Some(rest) => home.join(rest.trim_start_matches(['/', '\\'])),
        None => PathBuf::from(s),
    }
    .components()
    .collect()
}

/// Índice compartido por las tres variantes del plugin (global, `f:` y `d:`).
pub struct FileIndex {
    index: RwLock<Index>,
    settings: FilesSettings,
    rules: Rules,
    cache: PathBuf,
    scanning: AtomicBool,
    dirty: AtomicBool,
    last_scan: Mutex<Option<Instant>>,
    watcher: Mutex<Option<notify::RecommendedWatcher>>,
    stop: CancelToken,
}

impl FileIndex {
    /// Carga la caché (si hay) y prepara el índice. El recorrido empieza con [`FileIndex::rescan`].
    pub fn new(settings: FilesSettings, cache: PathBuf) -> Arc<Self> {
        let rules = Rules::new(&settings.exclude, settings.include_hidden);
        let t = Instant::now();
        let index = Index::load(&cache, settings.hash()).unwrap_or_default();
        tracing::info!(
            entries = index.len(),
            ms = t.elapsed().as_millis() as u64,
            "caché de archivos cargada"
        );
        Arc::new(Self {
            index: RwLock::new(index),
            settings,
            rules,
            cache,
            scanning: AtomicBool::new(false),
            dirty: AtomicBool::new(false),
            last_scan: Mutex::new(None),
            watcher: Mutex::new(None),
            stop: CancelToken::new(),
        })
    }

    /// Ajustes con los que se creó.
    pub fn settings(&self) -> &FilesSettings {
        &self.settings
    }

    /// Número de entradas.
    pub fn len(&self) -> usize {
        self.index.read().map_or(0, |i| i.len())
    }

    /// `true` si está vacío.
    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }

    /// Recorre las carpetas en segundo plano (si no hay ya un recorrido en curso) y arranca el
    /// watcher al terminar.
    pub fn rescan(self: &Arc<Self>, force: bool) {
        let recent = self
            .last_scan
            .lock()
            .ok()
            .and_then(|g| *g)
            .is_some_and(|t| t.elapsed() < RESCAN_AFTER);
        if (recent && !force) || self.scanning.swap(true, Ordering::SeqCst) {
            return;
        }
        let me = Arc::clone(self);
        let spawned = std::thread::Builder::new()
            .name("rukes-files-scan".into())
            .spawn(move || {
                let t = Instant::now();
                let fresh = Index::scan(&me.settings.roots, &me.rules, &me.stop);
                if me.stop.is_cancelled() {
                    return;
                }
                let entries = fresh.len();
                let bytes = fresh.memory_bytes();
                if let Ok(mut g) = me.index.write() {
                    *g = fresh;
                }
                if let Ok(mut g) = me.last_scan.lock() {
                    *g = Some(Instant::now());
                }
                tracing::info!(
                    entries,
                    mem_kb = bytes / 1024,
                    ms = t.elapsed().as_millis() as u64,
                    "índice de archivos listo"
                );
                me.save();
                me.scanning.store(false, Ordering::SeqCst);
                me.start_watcher();
            });
        if spawned.is_err() {
            self.scanning.store(false, Ordering::SeqCst);
        }
    }

    fn save(&self) {
        let res = self
            .index
            .read()
            .map(|i| i.save(&self.cache, self.settings.hash()));
        if let Ok(Err(e)) = res {
            tracing::warn!(error = %e, "no se pudo guardar la caché de archivos");
        }
        self.dirty.store(false, Ordering::SeqCst);
    }

    fn start_watcher(self: &Arc<Self>) {
        let Ok(mut slot) = self.watcher.lock() else {
            return;
        };
        if slot.is_some() {
            return;
        }
        let (tx, rx) = crossbeam_channel::unbounded::<notify::Result<notify::Event>>();
        let mut watcher = match notify::recommended_watcher(move |ev| {
            let _ = tx.send(ev);
        }) {
            Ok(w) => w,
            Err(e) => {
                tracing::warn!(error = %e, "no se pudo crear el watcher de archivos");
                return;
            }
        };
        for root in &self.settings.roots {
            if let Err(e) = watcher.watch(root, RecursiveMode::Recursive) {
                tracing::warn!(root = %root.display(), error = %e, "no se pudo vigilar la carpeta");
            }
        }
        *slot = Some(watcher);
        let weak = Arc::downgrade(self);
        let _ = std::thread::Builder::new()
            .name("rukes-files-watch".into())
            .spawn(move || {
                let mut last_save = Instant::now();
                while let Ok(first) = rx.recv() {
                    let Some(me) = weak.upgrade() else { return };
                    let mut dirs = BTreeSet::new();
                    let mut overflow = false;
                    let mut collect = |ev: notify::Result<notify::Event>| match ev {
                        Ok(ev) if !matches!(ev.kind, EventKind::Access(_)) => {
                            overflow |= ev.need_rescan();
                            for p in ev.paths {
                                if let Some(parent) = p.parent() {
                                    dirs.insert(parent.to_path_buf());
                                }
                            }
                        }
                        Ok(_) => {}
                        Err(_) => overflow = true,
                    };
                    collect(first);
                    let deadline = Instant::now() + DEBOUNCE;
                    while let Ok(ev) = rx.recv_deadline(deadline) {
                        collect(ev);
                    }
                    if overflow {
                        me.rescan(true);
                    } else {
                        me.apply(&dirs);
                    }
                    if me.dirty.load(Ordering::SeqCst) && last_save.elapsed() > SAVE_EVERY {
                        me.save();
                        last_save = Instant::now();
                    }
                }
            });
    }

    /// Aplica cambios en las carpetas indicadas (de la más corta a la más larga, para que las
    /// carpetas nuevas se recorran desde su padre).
    fn apply(&self, dirs: &BTreeSet<PathBuf>) {
        let mut dirs: Vec<&PathBuf> = dirs.iter().collect();
        dirs.sort_by_key(|d| d.as_os_str().len());
        let Ok(mut idx) = self.index.write() else {
            return;
        };
        let mut changed = false;
        for d in dirs {
            if let Some(i) = idx.find(d) {
                changed |= idx.refresh_dir(i, &self.rules, &self.stop);
            }
        }
        if idx.garbage_ratio() > 0.25 {
            *idx = idx.compact();
        }
        if changed {
            self.dirty.store(true, Ordering::SeqCst);
        }
    }

    /// Busca y devuelve `(ruta, es_carpeta, puntuación)`.
    pub fn search(
        &self,
        text: &str,
        kind: Kind,
        ext: Option<&str>,
        cancel: &CancelToken,
    ) -> Vec<(PathBuf, bool, f32)> {
        let Ok(idx) = self.index.read() else {
            return Vec::new();
        };
        idx.search(text, kind, ext, LIMIT, cancel)
            .into_iter()
            .map(|(i, s)| (idx.path(i), idx.is_dir(i), s))
            .collect()
    }
}

impl Drop for FileIndex {
    fn drop(&mut self) {
        self.stop.cancel();
    }
}

/// Separa un filtro de extensión (`.pdf`) del resto de la consulta (RF-044).
pub fn split_ext(text: &str) -> (String, Option<String>) {
    let mut ext = None;
    let rest: Vec<&str> = text
        .split_whitespace()
        .filter(|w| {
            let is_ext = ext.is_none()
                && w.len() >= 2
                && w.len() <= 11
                && w.starts_with('.')
                && w[1..].chars().all(|c| c.is_ascii_alphanumeric());
            if is_ext {
                ext = Some(w[1..].to_ascii_lowercase());
            }
            !is_ext
        })
        .collect();
    (rest.join(" "), ext)
}

fn display_dir(p: &Path) -> String {
    let s = p.display().to_string();
    match std::env::var("USERPROFILE") {
        Ok(home) if s.len() >= home.len() && s[..home.len()].eq_ignore_ascii_case(&home) => {
            format!("~{}", &s[home.len()..])
        }
        _ => s,
    }
}

/// Abre una terminal en `dir` con el shell y terminal configurados.
pub fn terminal_here(shell_name: &str, terminal: &str, dir: &Path) -> Result<(), PluginError> {
    let exe = match shell_name {
        "cmd" => "cmd.exe",
        "nu" | "nushell" => "nu.exe",
        "powershell" => "powershell.exe",
        _ => "pwsh.exe",
    };
    let res = if terminal == "wt" {
        let args = format!("-d \"{}\" {exe}", dir.display());
        shell::launch("wt.exe", Some(&args), Some(dir), Verb::Open)
            .or_else(|_| shell::launch(exe, None, Some(dir), Verb::Open))
    } else {
        shell::launch(exe, None, Some(dir), Verb::Open)
    };
    res.or_else(|_| shell::launch("powershell.exe", None, Some(dir), Verb::Open))
        .map_err(|e| PluginError::Failed(e.to_string()))
}

/// Consulta para Everything: el texto, más `file:`/`folder:` y `ext:` según el filtro.
pub fn everything_query(text: &str, kind: Kind, ext: Option<&str>) -> String {
    let mut q = String::new();
    match kind {
        Kind::Files => q.push_str("file: "),
        Kind::Dirs => q.push_str("folder: "),
        Kind::Any => {}
    }
    if let Some(e) = ext {
        q.push_str(&format!("ext:{} ", e.trim_start_matches('.')));
    }
    q.push_str(text.trim());
    q.trim().to_owned()
}

/// Plugin de archivos. Hay una instancia global y otras dos con prefijo (`f:` y `d:`).
pub struct FilesPlugin {
    index: Arc<FileIndex>,
    kind: Kind,
    prefix: Option<String>,
}

impl FilesPlugin {
    /// Las tres variantes sobre el mismo índice.
    pub fn variants(index: &Arc<FileIndex>) -> [FilesPlugin; 3] {
        [
            FilesPlugin {
                index: Arc::clone(index),
                kind: Kind::Any,
                prefix: None,
            },
            FilesPlugin {
                index: Arc::clone(index),
                kind: Kind::Files,
                prefix: Some("f:".into()),
            },
            FilesPlugin {
                index: Arc::clone(index),
                kind: Kind::Dirs,
                prefix: Some("d:".into()),
            },
        ]
    }

    /// Búsqueda con Everything (todo el disco). `None` si falla: se usa el índice propio.
    fn search_everything(&self, text: &str, ext: Option<&str>) -> Option<Vec<ResultItem>> {
        let q = everything_query(text, self.kind, ext);
        let found = match rukes_platform::everything::search(&q, 100) {
            Ok(f) => f,
            Err(e) => {
                tracing::debug!(error = %e, "Everything no disponible; se usa el índice");
                return None;
            }
        };
        let mut f = rukes_core::Fuzzy::new(text);
        let mut items: Vec<ResultItem> = found
            .into_iter()
            .map(|(p, dir)| {
                let name = p
                    .file_name()
                    .map(|n| n.to_string_lossy().into_owned())
                    .unwrap_or_default();
                let s = if text.is_empty() {
                    0.5
                } else {
                    f.score(&name).unwrap_or(0.3)
                };
                Self::item(p, dir, s)
            })
            .collect();
        items.sort_by(|a, b| b.fuzzy.total_cmp(&a.fuzzy));
        items.truncate(50);
        Some(items)
    }

    fn item(path: PathBuf, dir: bool, score: f32) -> ResultItem {
        let name = path.file_name().map_or_else(
            || path.display().to_string(),
            |n| n.to_string_lossy().into_owned(),
        );
        let mut it = ResultItem::new(path.to_string_lossy().to_lowercase(), name);
        it.subtitle = path.parent().map(display_dir).unwrap_or_default();
        it.icon = Icon::File(path.clone());
        it.preview = Some(path.clone());
        it.fuzzy = score;
        it.actions
            .push(Action::new(ActionId::OPEN_LOCATION, "action.open_location"));
        it.actions
            .push(Action::new(ActionId::COPY_PATH, "action.copy_path"));
        if !dir {
            it.actions
                .push(Action::new(ActionId::OPEN_WITH, "action.open_with"));
        }
        it.actions
            .push(Action::new(ActionId::TERMINAL_HERE, "action.terminal_here"));
        it.payload = path.to_string_lossy().into_owned();
        it
    }
}

impl Plugin for FilesPlugin {
    fn metadata(&self) -> PluginMetadata {
        let (id, name) = match self.kind {
            Kind::Any => ("files", tr("files.name")),
            Kind::Files => ("files-f", tr("files.name_files")),
            Kind::Dirs => ("files-d", tr("files.name_dirs")),
        };
        PluginMetadata {
            id: id.into(),
            name: name.into(),
            prefix: self.prefix.clone(),
            keywords: Vec::new(),
            priority: self.index.settings.priority,
            global: self.kind == Kind::Any,
            list_on_keyword: false,
        }
    }

    fn query(&self, q: &Query, cancel: &CancelToken, sink: &mut dyn FnMut(Vec<ResultItem>)) {
        let (text, ext) = split_ext(&q.text);
        if !q.targeted && text.chars().count() < self.index.settings.min_chars && ext.is_none() {
            sink(Vec::new());
            return;
        }
        if text.is_empty() && ext.is_none() {
            sink(Vec::new());
            return;
        }
        if self.index.settings.everything
            && rukes_platform::everything::available()
            && let Some(items) = self.search_everything(&text, ext.as_deref())
        {
            if !cancel.is_cancelled() {
                sink(items);
            }
            return;
        }
        let items = self
            .index
            .search(&text, self.kind, ext.as_deref(), cancel)
            .into_iter()
            .map(|(p, dir, s)| Self::item(p, dir, s))
            .collect();
        sink(items);
    }

    fn execute(&self, item: &ResultItem, action: &str) -> Result<ActionOutcome, PluginError> {
        let path = Path::new(&item.payload);
        let fail = |e: rukes_platform::PlatformError| PluginError::Failed(e.to_string());
        match action {
            ActionId::OPEN => {
                shell::launch(&item.payload, None, path.parent(), Verb::Open).map_err(fail)?
            }
            ActionId::OPEN_LOCATION => shell::open_location(path).map_err(fail)?,
            ActionId::COPY_PATH => return util::copy(&item.payload),
            ActionId::OPEN_WITH => {
                let args = format!("shell32.dll,OpenAs_RunDLL {}", item.payload);
                shell::launch("rundll32.exe", Some(&args), None, Verb::Open).map_err(fail)?;
            }
            ActionId::TERMINAL_HERE => {
                let dir = if path.is_dir() {
                    path
                } else {
                    path.parent().unwrap_or(path)
                };
                let s = &self.index.settings;
                terminal_here(&s.shell, &s.terminal, dir)?;
            }
            other => return Err(PluginError::UnsupportedAction(other.into())),
        }
        Ok(ActionOutcome::Hide)
    }

    fn refresh(&self) {
        // Solo la variante global reindexa (las tres comparten índice).
        if self.kind == Kind::Any {
            self.index.rescan(false);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn everything_queries() {
        assert_eq!(everything_query("informe", Kind::Any, None), "informe");
        assert_eq!(
            everything_query("informe", Kind::Files, Some(".pdf")),
            "file: ext:pdf informe"
        );
        assert_eq!(everything_query("", Kind::Dirs, None), "folder:");
    }

    #[test]
    fn splits_extension_filter() {
        assert_eq!(
            split_ext("informe .pdf"),
            ("informe".into(), Some("pdf".into()))
        );
        assert_eq!(split_ext(".PDF"), (String::new(), Some("pdf".into())));
        assert_eq!(split_ext("v1.2 notas"), ("v1.2 notas".into(), None));
        assert_eq!(split_ext("a . b"), ("a . b".into(), None));
    }

    #[test]
    fn expands_home() {
        let home = PathBuf::from(std::env::var("USERPROFILE").unwrap());
        assert_eq!(expand_root("~/Documents"), home.join("Documents"));
        assert_eq!(expand_root("D:/datos"), PathBuf::from("D:\\datos"));
    }
}
