//! Rutas de datos (sección 7.4). En modo portable todo vive en `data\` junto al ejecutable.

use std::path::{Path, PathBuf};

/// Directorios de Rukes.
#[derive(Debug, Clone)]
pub struct Paths {
    /// Configuración, temas, scripts (`%APPDATA%\Rukes`).
    pub config_dir: PathBuf,
    /// Estado: frecency, historiales (`%LOCALAPPDATA%\Rukes\state`).
    pub state_dir: PathBuf,
    /// Cachés regenerables (`%LOCALAPPDATA%\Rukes\cache`).
    pub cache_dir: PathBuf,
    /// Logs (`%LOCALAPPDATA%\Rukes\logs`).
    pub logs_dir: PathBuf,
    /// `true` si se usa el modo portable.
    pub portable: bool,
}

impl Paths {
    /// Detecta las rutas. Es portable si existe `portable.txt` o la carpeta `data` junto al
    /// ejecutable (RF-204).
    pub fn detect() -> Self {
        let exe_dir = std::env::current_exe()
            .ok()
            .and_then(|p| p.parent().map(Path::to_path_buf));
        if let Some(dir) = exe_dir
            && (dir.join("portable.txt").exists() || dir.join("data").is_dir())
        {
            return Self::under(&dir.join("data"), true);
        }
        let roaming = std::env::var_os("APPDATA").map(PathBuf::from);
        let local = std::env::var_os("LOCALAPPDATA").map(PathBuf::from);
        match (roaming, local) {
            (Some(r), Some(l)) => {
                let local = l.join("Rukes");
                Self {
                    config_dir: r.join("Rukes"),
                    state_dir: local.join("state"),
                    cache_dir: local.join("cache"),
                    logs_dir: local.join("logs"),
                    portable: false,
                }
            }
            _ => Self::under(Path::new("data"), true),
        }
    }

    /// Todas las rutas bajo una raíz.
    pub fn under(root: &Path, portable: bool) -> Self {
        Self {
            config_dir: root.to_path_buf(),
            state_dir: root.join("state"),
            cache_dir: root.join("cache"),
            logs_dir: root.join("logs"),
            portable,
        }
    }

    /// `config.toml`.
    pub fn config_file(&self) -> PathBuf {
        self.config_dir.join("config.toml")
    }

    /// Archivo de frecency.
    pub fn frecency_file(&self) -> PathBuf {
        self.state_dir.join("frecency.json")
    }
}
