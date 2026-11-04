//! Recarga en caliente de `config.toml` (RF-201).

use std::path::{Path, PathBuf};
use std::time::Duration;

use notify::{RecursiveMode, Watcher};

use crate::{Config, ConfigError};

/// Espera tras el último evento antes de recargar (los editores guardan en varios pasos).
const DEBOUNCE: Duration = Duration::from_millis(150);

/// Vigila el archivo de configuración. Al soltarse deja de vigilar.
pub struct ConfigWatcher {
    _watcher: notify::RecommendedWatcher,
}

impl ConfigWatcher {
    /// Vigila `path` y llama a `on_change` con el resultado de recargarlo. Se vigila la carpeta
    /// para soportar editores que sustituyen el archivo al guardar.
    pub fn spawn(
        path: &Path,
        on_change: impl Fn(Result<Config, ConfigError>) + Send + 'static,
    ) -> notify::Result<Self> {
        let (tx, rx) = crossbeam_channel::unbounded::<()>();
        let file: PathBuf = path.to_path_buf();
        let name = file.file_name().map(ToOwned::to_owned);
        let mut watcher = notify::recommended_watcher(move |ev: notify::Result<notify::Event>| {
            if let Ok(ev) = ev
                && ev.paths.iter().any(|p| p.file_name() == name.as_deref())
            {
                let _ = tx.send(());
            }
        })?;
        let dir = path.parent().unwrap_or(Path::new("."));
        watcher.watch(dir, RecursiveMode::NonRecursive)?;

        std::thread::Builder::new()
            .name("rukes-config-watch".into())
            .spawn(move || {
                while rx.recv().is_ok() {
                    // Agrupa ráfagas de eventos.
                    while rx.recv_timeout(DEBOUNCE).is_ok() {}
                    match std::fs::read_to_string(&file) {
                        Ok(text) => on_change(crate::parse(&text, &file)),
                        Err(e) => tracing::debug!(error = %e, "config no legible todavía"),
                    }
                }
            })
            .map_err(|e| notify::Error::generic(&e.to_string()))?;
        Ok(Self { _watcher: watcher })
    }
}

/// Vigila una carpeta (temas, idiomas) y llama a `on_change` cuando cambia algún `.toml`.
pub struct DirWatcher {
    _watcher: notify::RecommendedWatcher,
}

impl DirWatcher {
    /// Crea la carpeta si no existe y la vigila (sin subcarpetas).
    pub fn spawn(dir: &Path, on_change: impl Fn() + Send + 'static) -> notify::Result<Self> {
        let _ = std::fs::create_dir_all(dir);
        let (tx, rx) = crossbeam_channel::unbounded::<()>();
        let mut watcher = notify::recommended_watcher(move |ev: notify::Result<notify::Event>| {
            if let Ok(ev) = ev
                && ev.paths.iter().any(|p| {
                    p.extension()
                        .is_some_and(|e| e.eq_ignore_ascii_case("toml"))
                })
            {
                let _ = tx.send(());
            }
        })?;
        watcher.watch(dir, RecursiveMode::NonRecursive)?;
        std::thread::Builder::new()
            .name("rukes-dir-watch".into())
            .spawn(move || {
                while rx.recv().is_ok() {
                    while rx.recv_timeout(DEBOUNCE).is_ok() {}
                    on_change();
                }
            })
            .map_err(|e| notify::Error::generic(&e.to_string()))?;
        Ok(Self { _watcher: watcher })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn dir_watcher_sees_toml_changes() {
        let dir = std::env::temp_dir().join(format!("rukes-dirwatch-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        let (tx, rx) = crossbeam_channel::unbounded();
        let _w = DirWatcher::spawn(&dir, move || {
            let _ = tx.send(());
        })
        .unwrap();
        std::thread::sleep(Duration::from_millis(100));
        std::fs::write(dir.join("notas.txt"), "x").unwrap();
        assert!(rx.recv_timeout(Duration::from_millis(500)).is_err());
        std::fs::write(
            dir.join("mio.toml"),
            "[theme]
",
        )
        .unwrap();
        assert!(rx.recv_timeout(Duration::from_secs(3)).is_ok());
        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn reloads_on_save_and_reports_errors() {
        let dir = std::env::temp_dir().join(format!("rukes-watch-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let p = dir.join("config.toml");
        std::fs::write(&p, "").unwrap();
        let (tx, rx) = crossbeam_channel::unbounded();
        let _w = ConfigWatcher::spawn(&p, move |r| {
            let _ = tx.send(r.map(|c| c.window.max_results).map_err(|e| e.line));
        })
        .unwrap();
        std::thread::sleep(Duration::from_millis(100));

        std::fs::write(&p, "[window]\nmax_results = 3\n").unwrap();
        assert_eq!(rx.recv_timeout(Duration::from_secs(3)).unwrap(), Ok(3));

        std::fs::write(&p, "[window]\nmax_results = \n").unwrap();
        assert_eq!(rx.recv_timeout(Duration::from_secs(3)).unwrap(), Err(2));
        let _ = std::fs::remove_dir_all(dir);
    }
}
