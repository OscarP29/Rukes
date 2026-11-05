//! `wall`: gestor de fondos de pantalla con miniaturas, previsualización, elección al azar y
//! cambio automático (RF-180).

use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex, MutexGuard};
use std::time::{Duration, Instant};

use crossbeam_channel::{RecvTimeoutError, Sender};
use rukes_core::i18n::{tr, trf};
use rukes_core::{
    Action as UiAction, ActionId, ActionOutcome, CancelToken, Fuzzy, Icon, Plugin, PluginError,
    PluginMetadata, Query, ResultItem,
};
use rukes_platform::wallpaper::{self, Position};

/// Extensiones que Windows acepta como fondo.
pub const IMAGE_EXT: &[&str] = &[
    "jpg", "jpeg", "jfif", "png", "bmp", "gif", "tif", "tiff", "webp", "heic", "avif",
];
/// Imágenes como mucho.
const MAX_IMAGES: usize = 5000;
/// Profundidad de subcarpetas.
const MAX_DEPTH: usize = 4;
/// La lista de imágenes se vuelve a leer pasado este tiempo.
const RESCAN_AFTER: Duration = Duration::from_secs(30);

/// Ajustes de `[plugins.wall]`.
#[derive(Debug, Clone, PartialEq)]
pub struct WallSettings {
    /// Palabra clave.
    pub keyword: String,
    /// Carpetas con imágenes.
    pub dirs: Vec<PathBuf>,
    /// Ajuste de la imagen.
    pub position: Position,
    /// Cambio automático.
    pub interval: Option<Duration>,
    /// Al rotar, al azar.
    pub shuffle: bool,
}

impl Default for WallSettings {
    fn default() -> Self {
        Self {
            keyword: "wall".into(),
            dirs: Vec::new(),
            position: Position::Fill,
            interval: None,
            shuffle: true,
        }
    }
}

fn lock<T>(m: &Mutex<T>) -> MutexGuard<'_, T> {
    m.lock().unwrap_or_else(std::sync::PoisonError::into_inner)
}

/// `true` si la extensión es de imagen.
pub fn is_image(p: &Path) -> bool {
    p.extension()
        .and_then(|e| e.to_str())
        .is_some_and(|e| IMAGE_EXT.contains(&e.to_ascii_lowercase().as_str()))
}

/// Imágenes de las carpetas (con subcarpetas), sin repetir, por carpeta y nombre.
pub fn scan(dirs: &[PathBuf]) -> Vec<PathBuf> {
    fn walk(dir: &Path, depth: usize, out: &mut Vec<PathBuf>) {
        let Ok(rd) = std::fs::read_dir(dir) else {
            return;
        };
        let mut entries: Vec<_> = rd.flatten().collect();
        entries.sort_by_key(std::fs::DirEntry::file_name);
        for e in entries {
            if out.len() >= MAX_IMAGES {
                return;
            }
            let p = e.path();
            match e.file_type() {
                Ok(t) if t.is_dir() && depth < MAX_DEPTH => walk(&p, depth + 1, out),
                Ok(t) if t.is_file() && is_image(&p) => out.push(p),
                _ => {}
            }
        }
    }
    let mut out = Vec::new();
    for d in dirs {
        walk(d, 0, &mut out);
    }
    let mut seen = std::collections::HashSet::new();
    out.retain(|p| seen.insert(p.to_string_lossy().to_lowercase()));
    out
}

fn same(a: &Path, b: &Path) -> bool {
    a.to_string_lossy()
        .eq_ignore_ascii_case(&b.to_string_lossy())
}

/// Índice al azar en `0..n` distinto de `avoid` (si hay más de uno).
fn random_index(n: usize, avoid: Option<usize>) -> Option<usize> {
    if n == 0 {
        return None;
    }
    let r = getrandom::u32().unwrap_or(0) as usize;
    match avoid {
        Some(a) if n > 1 && a < n => Some((a + 1 + r % (n - 1)) % n),
        _ => Some(r % n),
    }
}

/// Siguiente fondo al rotar: al azar o el que sigue al actual.
pub fn next_of(list: &[PathBuf], current: Option<&Path>, shuffle: bool) -> Option<PathBuf> {
    let pos = current.and_then(|c| list.iter().position(|p| same(p, c)));
    let i = if shuffle {
        random_index(list.len(), pos)?
    } else {
        pos.map_or(0, |i| (i + 1) % list.len().max(1))
    };
    list.get(i).cloned()
}

struct Shared {
    settings: WallSettings,
    images: Mutex<(Option<Instant>, Arc<Vec<PathBuf>>)>,
    next_change: Mutex<Option<Instant>>,
}

impl Shared {
    fn images(&self) -> Arc<Vec<PathBuf>> {
        let mut g = lock(&self.images);
        if g.0.is_none_or(|t| t.elapsed() > RESCAN_AFTER) {
            *g = (Some(Instant::now()), Arc::new(scan(&self.settings.dirs)));
        }
        Arc::clone(&g.1)
    }

    fn set(&self, p: &Path) -> Result<(), String> {
        wallpaper::set(p, self.settings.position).map_err(|e| e.to_string())?;
        tracing::info!(path = %p.display(), "fondo de pantalla");
        Ok(())
    }

    fn rotate(&self) {
        let list = self.images();
        let cur = current();
        if let Some(p) = next_of(&list, cur.as_deref(), self.settings.shuffle)
            && let Err(e) = self.set(&p)
        {
            tracing::warn!(error = %e, "no se pudo cambiar el fondo");
        }
    }
}

/// Fondo actual (el del primer monitor que tenga imagen).
pub fn current() -> Option<PathBuf> {
    wallpaper::current()
        .ok()?
        .into_iter()
        .find(|p| !p.as_os_str().is_empty())
}

/// Plugin `wall`. Si hay `interval`, un hilo cambia el fondo solo; termina al soltar el plugin.
pub struct WallPlugin {
    shared: Arc<Shared>,
    _stop: Option<Sender<()>>,
}

impl WallPlugin {
    /// Crea el plugin (y la rotación, si está configurada).
    pub fn new(settings: WallSettings) -> Self {
        let interval = settings.interval.map(|d| d.max(Duration::from_secs(60)));
        let shared = Arc::new(Shared {
            settings,
            images: Mutex::new((None, Arc::new(Vec::new()))),
            next_change: Mutex::new(None),
        });
        let stop = interval.and_then(|every| {
            let (tx, rx) = crossbeam_channel::bounded::<()>(0);
            let s = Arc::clone(&shared);
            let spawned = std::thread::Builder::new()
                .name("rukes-wall".into())
                .spawn(move || {
                    loop {
                        *lock(&s.next_change) = Some(Instant::now() + every);
                        match rx.recv_timeout(every) {
                            Err(RecvTimeoutError::Timeout) => s.rotate(),
                            _ => return,
                        }
                    }
                });
            match spawned {
                Ok(_) => Some(tx),
                Err(e) => {
                    tracing::warn!(error = %e, "no se pudo arrancar la rotación de fondos");
                    None
                }
            }
        });
        Self {
            shared,
            _stop: stop,
        }
    }

    /// Ajustes con que se creó.
    pub fn settings(&self) -> &WallSettings {
        &self.shared.settings
    }

    fn image_item(&self, p: &Path, score: f32, cur: Option<&Path>) -> ResultItem {
        let name = p
            .file_stem()
            .map(|s| s.to_string_lossy().into_owned())
            .unwrap_or_default();
        let mut it = ResultItem::new(format!("img:{}", p.display()), name);
        let mut sub = Vec::new();
        if cur.is_some_and(|c| same(c, p)) {
            sub.push(tr("wall.current_mark").to_owned());
        }
        if let Some(d) = p.parent().and_then(Path::file_name) {
            sub.push(d.to_string_lossy().into_owned());
        }
        if let Ok(m) = std::fs::metadata(p) {
            sub.push(format!("{} KB", m.len() / 1024));
        }
        it.subtitle = sub.join(" · ");
        it.icon = Icon::Thumbnail(p.to_owned());
        it.preview = Some(p.to_owned());
        it.fuzzy = score;
        it.payload = p.to_string_lossy().into_owned();
        it.actions = vec![
            UiAction::new(ActionId::OPEN, tr("wall.set")),
            UiAction::new(ActionId::OPEN_LOCATION, "action.open_location"),
            UiAction::new(ActionId::COPY_PATH, "action.copy_path"),
        ];
        it
    }

    fn random_item(&self, list: &[PathBuf]) -> ResultItem {
        let mut it = ResultItem::new("random", tr("wall.random"));
        it.subtitle = trf("wall.n_images", &[("n", &list.len())]);
        it.icon = Icon::Named("wall".into());
        it.fuzzy = 1.0;
        it.actions = vec![UiAction::new(ActionId::OPEN, tr("wall.random_action"))];
        it
    }

    fn rotation_item(&self) -> Option<ResultItem> {
        let every = self.shared.settings.interval?;
        let mut it = ResultItem::new("rotate", tr("wall.rotation"));
        let left = lock(&self.shared.next_change)
            .map(|t| t.saturating_duration_since(Instant::now()))
            .unwrap_or(every);
        let mode = if self.shared.settings.shuffle {
            tr("wall.shuffle")
        } else {
            tr("wall.in_order")
        };
        it.subtitle = trf(
            "wall.rotation_sub",
            &[
                ("every", &crate::workflows::human_duration(every)),
                (
                    "left",
                    &crate::workflows::human_duration(Duration::from_secs(
                        left.as_secs() - left.as_secs() % 60 + 60,
                    )),
                ),
                ("mode", &mode),
            ],
        );
        it.icon = Icon::Named("wall".into());
        it.fuzzy = 0.99;
        it.actions = vec![UiAction::new(ActionId::OPEN, tr("wall.next_now"))];
        Some(it)
    }

    /// Resultados de una consulta (`text` sin la palabra clave).
    pub fn search(&self, text: &str) -> Vec<ResultItem> {
        let list = self.shared.images();
        let cur = current();
        let lower = text.trim().to_lowercase();
        if list.is_empty() {
            let mut it = ResultItem::new("empty", tr("wall.none"));
            it.subtitle = self
                .shared
                .settings
                .dirs
                .iter()
                .map(|d| d.display().to_string())
                .collect::<Vec<_>>()
                .join(" · ");
            it.icon = Icon::Named("wall".into());
            it.actions = vec![];
            return vec![it];
        }
        let mut out = Vec::new();
        if lower.is_empty() {
            if let Some(c) = &cur {
                let mut it = self.image_item(c, 1.0, Some(c));
                it.id = "current".into();
                it.title = trf("wall.current", &[("name", &it.title)]);
                out.push(it);
            }
            out.push(self.random_item(&list));
            out.extend(self.rotation_item());
            out.extend(
                list.iter()
                    .enumerate()
                    .map(|(i, p)| self.image_item(p, 0.9 - i as f32 * 0.0001, cur.as_deref())),
            );
            return out;
        }
        if ["random", "aleatorio", "azar"]
            .iter()
            .any(|w| w.starts_with(&lower))
        {
            out.push(self.random_item(&list));
        }
        if ["next", "siguiente"].iter().any(|w| w.starts_with(&lower)) {
            out.extend(self.rotation_item());
        }
        let mut f = Fuzzy::new(&lower);
        for p in list.iter() {
            let name = p
                .file_stem()
                .map(|s| s.to_string_lossy().into_owned())
                .unwrap_or_default();
            let folder = p
                .parent()
                .and_then(Path::file_name)
                .map(|s| s.to_string_lossy().into_owned())
                .unwrap_or_default();
            if let Some(s) = f.best([name.as_str(), folder.as_str()].into_iter()) {
                out.push(self.image_item(p, s * 0.95, cur.as_deref()));
            }
        }
        out
    }
}

impl Plugin for WallPlugin {
    fn metadata(&self) -> PluginMetadata {
        PluginMetadata {
            id: "wall".into(),
            name: tr("wall.name").into(),
            prefix: None,
            keywords: vec![self.shared.settings.keyword.clone()],
            priority: 0.0,
            global: false,
            list_on_keyword: false,
        }
    }

    fn query(&self, q: &Query, _c: &CancelToken, sink: &mut dyn FnMut(Vec<ResultItem>)) {
        sink(self.search(&q.text));
    }

    fn execute(&self, item: &ResultItem, action: &str) -> Result<ActionOutcome, PluginError> {
        let s = &self.shared;
        let target = match item.id.as_str() {
            "empty" => return Ok(ActionOutcome::KeepOpen),
            "random" | "rotate" => {
                let list = s.images();
                let cur = current();
                let shuffle = item.id == "random" || s.settings.shuffle;
                next_of(&list, cur.as_deref(), shuffle)
                    .ok_or_else(|| PluginError::Failed(tr("wall.none").into()))?
            }
            _ => PathBuf::from(&item.payload),
        };
        match action {
            ActionId::OPEN => {
                s.set(&target).map_err(PluginError::Failed)?;
                let name = target
                    .file_stem()
                    .map(|n| n.to_string_lossy().into_owned())
                    .unwrap_or_default();
                Ok(ActionOutcome::Notify(trf("wall.done", &[("name", &name)])))
            }
            ActionId::COPY_PATH => crate::util::copy(&target.to_string_lossy()),
            ActionId::OPEN_LOCATION => rukes_platform::shell::open_location(&target)
                .map(|()| ActionOutcome::Hide)
                .map_err(|e| PluginError::Failed(e.to_string())),
            other => Err(PluginError::UnsupportedAction(other.into())),
        }
    }

    fn refresh(&self) {
        *lock(&self.shared.images) = (
            Some(Instant::now()),
            Arc::new(scan(&self.shared.settings.dirs)),
        );
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tmp(tag: &str) -> PathBuf {
        let d = std::env::temp_dir().join(format!("rukes-wall-{tag}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&d);
        std::fs::create_dir_all(d.join("sub")).unwrap();
        d
    }

    #[test]
    fn scans_images_recursively() {
        let d = tmp("scan");
        for f in ["b.jpg", "a.PNG", "nota.txt", "sub/c.webp"] {
            std::fs::write(d.join(f), b"x").unwrap();
        }
        let list = scan(&[d.clone(), d.clone()]);
        let names: Vec<String> = list
            .iter()
            .map(|p| p.file_name().unwrap().to_string_lossy().into_owned())
            .collect();
        assert_eq!(names, ["a.PNG", "b.jpg", "c.webp"]);
        let _ = std::fs::remove_dir_all(d);
    }

    #[test]
    fn next_in_order_and_random_avoids_current() {
        let list: Vec<PathBuf> = ["a", "b", "c"].iter().map(PathBuf::from).collect();
        assert_eq!(
            next_of(&list, Some(Path::new("A")), false),
            Some("b".into())
        );
        assert_eq!(
            next_of(&list, Some(Path::new("c")), false),
            Some("a".into())
        );
        assert_eq!(next_of(&list, None, false), Some("a".into()));
        for _ in 0..50 {
            let n = next_of(&list, Some(Path::new("b")), true).unwrap();
            assert_ne!(n, PathBuf::from("b"));
        }
        assert_eq!(next_of(&[], None, true), None);
    }

    #[test]
    fn lists_and_filters_without_touching_the_wallpaper() {
        let _ = rukes_core::i18n::set_language("es", "");
        let d = tmp("list");
        for f in ["playa.jpg", "montaña.png", "sub/ciudad.jpg"] {
            std::fs::write(d.join(f), b"x").unwrap();
        }
        let p = WallPlugin::new(WallSettings {
            dirs: vec![d.clone()],
            ..WallSettings::default()
        });
        let all = p.search("");
        assert!(all.iter().any(|i| i.id == "random"));
        let imgs: Vec<&ResultItem> = all.iter().filter(|i| i.id.starts_with("img:")).collect();
        assert_eq!(imgs.len(), 3);
        assert!(imgs.iter().all(|i| i.preview.is_some()));
        assert!(matches!(imgs[0].icon, Icon::Thumbnail(_)));
        let r = p.search("playa");
        assert_eq!(r[0].title, "playa");
        let r = p.search("random");
        assert_eq!(r[0].id, "random");
        let _ = std::fs::remove_dir_all(d);
    }
}
