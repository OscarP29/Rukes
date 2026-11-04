//! Carpeta de scripts vigilada (RF-130): todo archivo ejecutable con cabecera `@rukes.*`
//! aparece en el launcher sin reiniciar.

use std::collections::BTreeMap;
use std::io::Read as _;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex, RwLock, Weak};
use std::time::{Duration, SystemTime};

use notify::{RecursiveMode, Watcher as _};

use crate::header::{self, HEADER_BYTES, Meta};
use crate::lang::Lang;

/// Profundidad máxima de subcarpetas.
const MAX_DEPTH: usize = 3;
/// Espera tras el último cambio antes de volver a leer la carpeta.
const DEBOUNCE: Duration = Duration::from_millis(250);

/// Un script descubierto.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Script {
    /// Ruta absoluta.
    pub path: PathBuf,
    /// Lenguaje.
    pub lang: Lang,
    /// Metadatos de la cabecera.
    pub meta: Meta,
    /// Última modificación.
    pub modified: Option<SystemTime>,
}

impl Script {
    /// Carpeta del script.
    pub fn dir(&self) -> &Path {
        self.path.parent().unwrap_or(Path::new("."))
    }

    /// Tiempo máximo efectivo (`None` = sin límite).
    pub fn timeout(&self, default: Duration) -> Option<Duration> {
        let t = self.meta.timeout.unwrap_or(default);
        (!t.is_zero()).then_some(t)
    }
}

fn pretty_stem(path: &Path) -> String {
    let stem = path
        .file_stem()
        .map(|s| s.to_string_lossy().replace(['-', '_'], " "))
        .unwrap_or_default();
    let mut c = stem.chars();
    match c.next() {
        Some(f) => f.to_uppercase().chain(c).collect(),
        None => stem,
    }
}

fn read_head(path: &Path) -> Option<String> {
    let mut f = std::fs::File::open(path).ok()?;
    let mut buf = vec![0u8; HEADER_BYTES];
    let mut n = 0;
    while n < buf.len() {
        match f.read(&mut buf[n..]) {
            Ok(0) => break,
            Ok(k) => n += k,
            Err(_) => return None,
        }
    }
    buf.truncate(n);
    // Windows PowerShell guarda a veces en UTF-16 LE.
    if let Some(rest) = buf.strip_prefix(b"\xFF\xFE") {
        let units: Vec<u16> = rest
            .chunks_exact(2)
            .map(|c| u16::from_le_bytes([c[0], c[1]]))
            .collect();
        return Some(String::from_utf16_lossy(&units));
    }
    Some(String::from_utf8_lossy(&buf).into_owned())
}

/// Lee un script. `None` si no es ejecutable o no tiene cabecera (los `.exe` no la necesitan).
pub fn load(path: &Path, interpreters: &BTreeMap<String, String>) -> Option<Script> {
    let lang = Lang::of(path, interpreters)?;
    let modified = std::fs::metadata(path).and_then(|m| m.modified()).ok();
    let mut meta = if lang == Lang::Exe {
        Meta::default()
    } else {
        header::parse(&read_head(path)?)?
    };
    if meta.name.is_empty() {
        meta.name = pretty_stem(path);
    }
    for w in &meta.warnings {
        tracing::warn!(script = %path.display(), "{w}");
    }
    Some(Script {
        path: path.to_path_buf(),
        lang,
        meta,
        modified,
    })
}

fn walk(dir: &Path, depth: usize, interpreters: &BTreeMap<String, String>, out: &mut Vec<Script>) {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return;
    };
    for e in entries.flatten() {
        let path = e.path();
        let name = e.file_name().to_string_lossy().to_lowercase();
        let Ok(ft) = e.file_type() else {
            continue;
        };
        if ft.is_dir() {
            let skip = name.starts_with(['.', '_'])
                || matches!(
                    name.as_str(),
                    "node_modules" | "venv" | "env" | "lib" | "libs"
                );
            if !skip && depth < MAX_DEPTH {
                walk(&path, depth + 1, interpreters, out);
            }
        } else if ft.is_file()
            && !name.starts_with(['.', '_', '~'])
            && let Some(s) = load(&path, interpreters)
        {
            out.push(s);
        }
    }
}

/// Lee todos los scripts de `dir`, ordenados por nombre.
pub fn scan(dir: &Path, interpreters: &BTreeMap<String, String>) -> Vec<Arc<Script>> {
    let mut out = Vec::new();
    walk(dir, 0, interpreters, &mut out);
    out.sort_by(|a, b| {
        a.meta
            .name
            .to_lowercase()
            .cmp(&b.meta.name.to_lowercase())
            .then_with(|| a.path.cmp(&b.path))
    });
    let mut seen: Vec<(String, PathBuf)> = Vec::new();
    for s in &out {
        for a in &s.meta.aliases {
            let key = a.to_lowercase();
            if let Some((_, other)) = seen.iter().find(|(k, _)| *k == key) {
                tracing::warn!(alias = %a, script = %s.path.display(), other = %other.display(), "alias repetido");
            } else {
                seen.push((key, s.path.clone()));
            }
        }
    }
    out.into_iter().map(Arc::new).collect()
}

/// Ejemplos que se copian la primera vez que se crea la carpeta.
const SEED: &[(&str, &str)] = &[
    (
        "hola.ps1",
        include_str!("../../../scripts-examples/hola.ps1"),
    ),
    (
        "mi-ip.rhai",
        include_str!("../../../scripts-examples/mi-ip.rhai"),
    ),
];

type OnChange = Arc<dyn Fn() + Send + Sync>;

/// Catálogo de scripts con su watcher.
pub struct Catalog {
    dir: PathBuf,
    interpreters: BTreeMap<String, String>,
    scripts: RwLock<Arc<Vec<Arc<Script>>>>,
    watcher: Mutex<Option<notify::RecommendedWatcher>>,
    on_change: OnChange,
}

impl Catalog {
    /// Lee la carpeta (creándola con los ejemplos si no existe) y empieza a vigilarla.
    /// `on_change` se llama tras cada cambio en la lista de scripts.
    pub fn open(
        dir: PathBuf,
        interpreters: BTreeMap<String, String>,
        on_change: OnChange,
    ) -> Arc<Self> {
        if !dir.exists() {
            match std::fs::create_dir_all(&dir) {
                Ok(()) => {
                    for (name, text) in SEED {
                        let text = text.replace("\r\n", "\n");
                        let text = if name.ends_with(".ps1") {
                            format!("\u{feff}{text}")
                        } else {
                            text
                        };
                        if let Err(e) = std::fs::write(dir.join(name), text) {
                            tracing::warn!(%e, "no se pudo copiar un script de ejemplo");
                        }
                    }
                }
                Err(e) => {
                    tracing::warn!(dir = %dir.display(), %e, "no se pudo crear la carpeta de scripts")
                }
            }
        }
        let scripts = scan(&dir, &interpreters);
        tracing::info!(dir = %dir.display(), scripts = scripts.len(), "scripts cargados");
        let catalog = Arc::new(Self {
            dir,
            interpreters,
            scripts: RwLock::new(Arc::new(scripts)),
            watcher: Mutex::new(None),
            on_change,
        });
        catalog.watch();
        catalog
    }

    fn watch(self: &Arc<Self>) {
        let (tx, rx) = crossbeam_channel::unbounded::<()>();
        let watcher = notify::recommended_watcher(move |res: notify::Result<notify::Event>| {
            if res.is_ok_and(|e| !e.kind.is_access()) {
                let _ = tx.send(());
            }
        });
        let mut watcher = match watcher {
            Ok(w) => w,
            Err(e) => {
                tracing::warn!(%e, "no se pudo vigilar la carpeta de scripts");
                return;
            }
        };
        if let Err(e) = watcher.watch(&self.dir, RecursiveMode::Recursive) {
            tracing::warn!(%e, dir = %self.dir.display(), "no se pudo vigilar la carpeta de scripts");
            return;
        }
        *self.watcher.lock().unwrap_or_else(|p| p.into_inner()) = Some(watcher);
        let weak: Weak<Self> = Arc::downgrade(self);
        let _ = std::thread::Builder::new()
            .name("rukes-scripts-watch".into())
            .spawn(move || {
                // Termina cuando se suelta el watcher (se cierra el canal).
                while rx.recv().is_ok() {
                    while rx.recv_timeout(DEBOUNCE).is_ok() {}
                    let Some(c) = weak.upgrade() else {
                        break;
                    };
                    c.rescan();
                }
            });
    }

    /// Carpeta vigilada.
    pub fn dir(&self) -> &Path {
        &self.dir
    }

    /// Scripts actuales.
    pub fn scripts(&self) -> Arc<Vec<Arc<Script>>> {
        self.scripts
            .read()
            .map(|s| Arc::clone(&s))
            .unwrap_or_default()
    }

    /// Script por ruta.
    pub fn get(&self, path: &Path) -> Option<Arc<Script>> {
        self.scripts().iter().find(|s| s.path == path).cloned()
    }

    /// Vuelve a leer la carpeta. Devuelve `true` si algo cambió (y avisa con `on_change`).
    pub fn rescan(&self) -> bool {
        let fresh = scan(&self.dir, &self.interpreters);
        let changed = {
            let current = self.scripts();
            current.len() != fresh.len() || current.iter().zip(&fresh).any(|(a, b)| a != b)
        };
        if changed {
            tracing::info!(scripts = fresh.len(), "scripts actualizados");
            if let Ok(mut s) = self.scripts.write() {
                *s = Arc::new(fresh);
            }
            (self.on_change)();
        }
        changed
    }
}

impl Drop for Catalog {
    fn drop(&mut self) {
        // Soltar el watcher cierra el canal y termina el hilo.
        if let Ok(mut w) = self.watcher.lock() {
            w.take();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicUsize, Ordering};
    use std::time::Instant;

    fn interpreters() -> BTreeMap<String, String> {
        BTreeMap::from([("py".to_owned(), "python".to_owned())])
    }

    fn temp(name: &str) -> PathBuf {
        let d = std::env::temp_dir().join(format!("rukes-cat-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&d);
        d
    }

    #[test]
    fn scan_finds_scripts_with_headers() {
        let d = temp("scan");
        std::fs::create_dir_all(d.join("sub/deep")).unwrap();
        std::fs::create_dir_all(d.join("node_modules")).unwrap();
        std::fs::write(d.join("a.ps1"), "# @rukes.name Alfa\n# @rukes.alias al\n").unwrap();
        std::fs::write(d.join("sub/b-c.py"), "# @rukes.alias x\n").unwrap();
        std::fs::write(d.join("sub/deep/z.cmd"), ":: @rukes.name Zeta\r\n").unwrap();
        std::fs::write(d.join("lib.py"), "def helper(): pass\n").unwrap();
        std::fs::write(d.join("notas.txt"), "# @rukes.name No\n").unwrap();
        std::fs::write(d.join("node_modules/m.ps1"), "# @rukes.name No\n").unwrap();
        std::fs::write(d.join("_tmp.ps1"), "# @rukes.name No\n").unwrap();
        // UTF-16 LE con BOM.
        let utf16: Vec<u8> = b"\xFF\xFE"
            .iter()
            .copied()
            .chain(
                "# @rukes.name Dieciséis\n"
                    .encode_utf16()
                    .flat_map(u16::to_le_bytes),
            )
            .collect();
        std::fs::write(d.join("u.ps1"), utf16).unwrap();
        let s = scan(&d, &interpreters());
        let names: Vec<&str> = s.iter().map(|s| s.meta.name.as_str()).collect();
        assert_eq!(names, ["Alfa", "B c", "Dieciséis", "Zeta"]);
        let _ = std::fs::remove_dir_all(d);
    }

    #[test]
    fn new_dir_is_seeded_and_changes_are_picked_up() {
        let d = temp("watch");
        let calls = Arc::new(AtomicUsize::new(0));
        let c2 = Arc::clone(&calls);
        let cat = Catalog::open(
            d.clone(),
            interpreters(),
            Arc::new(move || {
                c2.fetch_add(1, Ordering::SeqCst);
            }),
        );
        let seeded = cat.scripts().len();
        assert_eq!(seeded, SEED.len());
        std::fs::write(d.join("nuevo.py"), "# @rukes.name Nuevo\n").unwrap();
        let t = Instant::now();
        while cat.scripts().len() == seeded && t.elapsed() < Duration::from_secs(5) {
            std::thread::sleep(Duration::from_millis(50));
        }
        assert_eq!(cat.scripts().len(), seeded + 1);
        assert!(calls.load(Ordering::SeqCst) >= 1);
        assert!(cat.get(&d.join("nuevo.py")).is_some());
        drop(cat);
        let _ = std::fs::remove_dir_all(d);
    }
}
