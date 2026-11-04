//! Configuración, temas y recarga en caliente de Rukes.
#![forbid(unsafe_code)]

mod hotkey;
pub mod paths;
mod schema;
pub mod theme;
mod watch;

use std::path::{Path, PathBuf};

pub use hotkey::{Hotkey, HotkeyError, Modifiers};
pub use schema::*;
pub use watch::{ConfigWatcher, DirWatcher};

/// Error al cargar la configuración, con posición para mostrarlo al usuario (RF-201).
#[derive(Debug, Clone, thiserror::Error)]
#[error("{path}:{line}:{column}: {message}")]
pub struct ConfigError {
    /// Archivo.
    pub path: PathBuf,
    /// Línea (desde 1; 0 si se desconoce).
    pub line: usize,
    /// Columna (desde 1; 0 si se desconoce).
    pub column: usize,
    /// Mensaje.
    pub message: String,
}

/// Plantilla escrita la primera vez que se arranca sin configuración.
pub const DEFAULT_CONFIG: &str = include_str!("default_config.toml");

/// Interpreta el texto TOML de una configuración.
pub fn parse(text: &str, path: &Path) -> Result<Config, ConfigError> {
    toml::from_str::<Config>(text).map_err(|e| {
        let (line, column) = e.span().map(|s| line_col(text, s.start)).unwrap_or((0, 0));
        ConfigError {
            path: path.to_owned(),
            line,
            column,
            message: e.message().trim().to_owned(),
        }
    })
}

/// Carga la configuración. Si el archivo no existe, devuelve los valores por defecto y escribe
/// la plantilla comentada para que el usuario la edite (RF-200).
pub fn load(path: &Path) -> Result<Config, ConfigError> {
    match std::fs::read_to_string(path) {
        Ok(text) => parse(&text, path),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
            if let Some(dir) = path.parent()
                && let Err(e) =
                    std::fs::create_dir_all(dir).and_then(|()| std::fs::write(path, DEFAULT_CONFIG))
            {
                tracing::warn!(path = %path.display(), error = %e, "no se pudo escribir la configuración por defecto");
            }
            Ok(Config::default())
        }
        Err(e) => Err(ConfigError {
            path: path.to_owned(),
            line: 0,
            column: 0,
            message: e.to_string(),
        }),
    }
}

fn line_col(text: &str, offset: usize) -> (usize, usize) {
    let before = &text[..offset.min(text.len())];
    let line = before.matches('\n').count() + 1;
    let column = before.rsplit('\n').next().map_or(0, |l| l.chars().count()) + 1;
    (line, column)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_template_parses_to_defaults() {
        let c = parse(DEFAULT_CONFIG, Path::new("config.toml")).unwrap();
        let d = Config::default();
        assert_eq!(c.general.hotkey, d.general.hotkey);
        assert_eq!(c.window.max_results, d.window.max_results);
        assert_eq!(c.search.frecency_weight, d.search.frecency_weight);
        // Las secciones de plugins de la plantilla coinciden con los valores por defecto.
        assert_eq!(c.general, d.general);
        assert_eq!(c.window, d.window);
        assert_eq!(c.plugins, d.plugins);
        assert_eq!(c.scripts, d.scripts);
    }

    #[test]
    fn empty_file_gives_defaults() {
        let c = parse("", Path::new("c.toml")).unwrap();
        assert_eq!(c.general.hotkey, "Alt+Space");
        assert!(c.plugins.apps.enabled);
    }

    #[test]
    fn partial_file_overrides_only_given_fields() {
        let c = parse(
            "[window]\nmax_results = 5\n[plugins.apps.aliases]\nc = \"Google Chrome\"\n",
            Path::new("c.toml"),
        )
        .unwrap();
        assert_eq!(c.window.max_results, 5);
        assert!(c.window.hide_on_blur);
        assert_eq!(
            c.plugins.apps.aliases.get("c").map(String::as_str),
            Some("Google Chrome")
        );
    }

    #[test]
    fn syntax_error_reports_line_and_column() {
        let e = parse("[window]\nmax_results = = 5\n", Path::new("c.toml")).unwrap_err();
        assert_eq!(e.line, 2);
        assert!(e.column > 1);
    }

    #[test]
    fn type_error_reports_position() {
        let e = parse("[window]\nwidth = \"ancho\"\n", Path::new("c.toml")).unwrap_err();
        assert_eq!(e.line, 2);
    }

    #[test]
    fn enums_parse_snake_case() {
        let c = parse(
            "[window]\non_show = \"select_all\"\nposition = \"top_center\"\n",
            Path::new("c.toml"),
        )
        .unwrap();
        assert_eq!(c.window.on_show, OnShow::SelectAll);
        assert_eq!(c.window.position, WindowPosition::TopCenter);
    }

    #[test]
    fn load_missing_writes_template() {
        let dir = std::env::temp_dir().join(format!("rukes-cfg-{}", std::process::id()));
        let p = dir.join("config.toml");
        let _ = std::fs::remove_dir_all(&dir);
        let c = load(&p).unwrap();
        assert_eq!(c.general.hotkey, "Alt+Space");
        assert!(p.exists());
        let _ = std::fs::remove_dir_all(dir);
    }
}
