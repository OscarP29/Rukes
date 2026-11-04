//! Esquema de `config.toml`. Todo tiene valor por defecto: un archivo vacío es válido.

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

/// Configuración principal.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Config {
    /// Ajustes generales.
    pub general: General,
    /// Ventana.
    pub window: Window,
    /// Búsqueda y ranking.
    pub search: Search,
    /// Plugins.
    pub plugins: Plugins,
}

/// `[general]`
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct General {
    /// Atajo global (RF-001).
    pub hotkey: String,
    /// Idioma de la interfaz: `auto`, `es`, `en`.
    pub language: String,
    /// Iniciar con Windows (RF-008).
    pub start_with_windows: bool,
    /// Terminal para los comandos de shell y «abrir terminal aquí»: `wt` (Windows Terminal) o
    /// `none` (la consola del propio shell).
    pub terminal: String,
}

impl Default for General {
    fn default() -> Self {
        Self {
            hotkey: "Alt+Space".into(),
            language: "auto".into(),
            start_with_windows: false,
            terminal: "wt".into(),
        }
    }
}

/// Posición de la ventana (RF-002).
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum WindowPosition {
    /// Centrada (un poco por encima del centro).
    #[default]
    Center,
    /// Centrada horizontalmente, cerca del borde superior.
    TopCenter,
}

/// Qué hacer con la búsqueda anterior al mostrar la ventana (RF-004).
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum OnShow {
    /// Vaciar.
    Clear,
    /// Conservar.
    Keep,
    /// Conservar y seleccionar todo.
    #[default]
    SelectAll,
}

/// `[window]`
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Window {
    /// Posición.
    pub position: WindowPosition,
    /// Ancho lógico en píxeles.
    pub width: u32,
    /// Resultados visibles a la vez (RF-020).
    pub max_results: usize,
    /// Ocultar al perder el foco (RF-003).
    pub hide_on_blur: bool,
    /// Ocultar tras ejecutar una acción (RF-003).
    pub hide_after_action: bool,
    /// Comportamiento del texto al mostrar (RF-004).
    pub on_show: OnShow,
    /// Fondo: `mica`, `acrylic` o `solid` (RF-190).
    pub backdrop: String,
}

impl Default for Window {
    fn default() -> Self {
        Self {
            position: WindowPosition::default(),
            width: 680,
            max_results: 8,
            hide_on_blur: true,
            hide_after_action: true,
            on_show: OnShow::default(),
            backdrop: "mica".into(),
        }
    }
}

/// `[search]`
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Search {
    /// Peso de la frecency en el ranking (0..1).
    pub frecency_weight: f32,
    /// Vida media de la frecency en días.
    pub frecency_half_life_days: f32,
}

impl Default for Search {
    fn default() -> Self {
        Self {
            frecency_weight: 0.35,
            frecency_half_life_days: 7.0,
        }
    }
}

/// `[plugins]`
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Plugins {
    /// Plugin de aplicaciones.
    pub apps: Apps,
    /// Calculadora, unidades y fechas.
    pub calc: Calc,
    /// URLs y búsquedas web.
    pub web: Web,
    /// Comandos de shell y del sistema.
    pub shell: Shell,
    /// Ventanas abiertas.
    pub windows: PluginCommon,
    /// Procesos.
    pub processes: PluginCommon,
    /// Generadores, hashes y codificadores.
    pub generators: Generators,
    /// Archivos y carpetas.
    pub files: Files,
}

/// `[plugins.calc]`
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Calc {
    /// Activo.
    pub enabled: bool,
    /// Prefijo (RF-015).
    pub prefix: String,
    /// Evaluar también sin prefijo si la consulta parece una expresión.
    pub auto: bool,
}

impl Default for Calc {
    fn default() -> Self {
        Self {
            enabled: true,
            prefix: "=".into(),
            auto: true,
        }
    }
}

/// `[plugins.web]`
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Web {
    /// Activo.
    pub enabled: bool,
    /// Palabra clave → URL con `{q}` (RF-091).
    pub engines: BTreeMap<String, String>,
}

impl Default for Web {
    fn default() -> Self {
        let engines = [
            ("g", "https://www.google.com/search?q={q}"),
            ("ddg", "https://duckduckgo.com/?q={q}"),
            ("yt", "https://www.youtube.com/results?search_query={q}"),
            ("gh", "https://github.com/search?q={q}"),
            ("wiki", "https://es.wikipedia.org/w/index.php?search={q}"),
            ("crate", "https://crates.io/search?q={q}"),
            ("docs", "https://docs.rs/releases/search?query={q}"),
        ]
        .into_iter()
        .map(|(k, v)| (k.to_owned(), v.to_owned()))
        .collect();
        Self {
            enabled: true,
            engines,
        }
    }
}

/// `[plugins.shell]`
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Shell {
    /// Activo.
    pub enabled: bool,
    /// Prefijo (RF-060).
    pub prefix: String,
    /// `pwsh`, `powershell`, `cmd` o `nu`.
    pub shell: String,
    /// Mantener la terminal abierta tras el comando.
    pub keep_open: bool,
    /// Pedir confirmación para apagar, reiniciar, etc. (RF-062).
    pub confirm_system: bool,
}

impl Default for Shell {
    fn default() -> Self {
        Self {
            enabled: true,
            prefix: ">".into(),
            shell: "pwsh".into(),
            keep_open: true,
            confirm_system: true,
        }
    }
}

/// `[plugins.generators]`
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Generators {
    /// Activo.
    pub enabled: bool,
    /// Longitud por defecto de las contraseñas.
    pub password_length: usize,
    /// Incluir símbolos en las contraseñas.
    pub password_symbols: bool,
}

impl Default for Generators {
    fn default() -> Self {
        Self {
            enabled: true,
            password_length: 20,
            password_symbols: true,
        }
    }
}

/// `[plugins.files]`
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Files {
    /// Activo.
    pub enabled: bool,
    /// Prioridad.
    pub priority: f32,
    /// Carpetas indexadas (`~` = carpeta del usuario).
    pub roots: Vec<String>,
    /// Patrones glob excluidos (RF-042).
    pub exclude: Vec<String>,
    /// Indexar archivos y carpetas ocultos.
    pub include_hidden: bool,
    /// Caracteres mínimos para buscar archivos sin prefijo.
    pub min_chars: usize,
}

impl Default for Files {
    fn default() -> Self {
        Self {
            enabled: true,
            priority: -0.15,
            roots: [
                "~/Desktop",
                "~/Documents",
                "~/Downloads",
                "~/Pictures",
                "~/Videos",
                "~/Music",
            ]
            .into_iter()
            .map(Into::into)
            .collect(),
            exclude: [
                "**/node_modules",
                "**/.git",
                "**/target",
                "**/__pycache__",
                "**/.venv",
                "**/AppData",
                "**/$RECYCLE.BIN",
            ]
            .into_iter()
            .map(Into::into)
            .collect(),
            include_hidden: false,
            min_chars: 3,
        }
    }
}

/// Ajustes comunes a todos los plugins (RF-205).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct PluginCommon {
    /// Activo.
    pub enabled: bool,
    /// Prioridad: se suma a la puntuación (−1..1).
    pub priority: f32,
}

impl Default for PluginCommon {
    fn default() -> Self {
        Self {
            enabled: true,
            priority: 0.0,
        }
    }
}

/// `[plugins.apps]`
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Apps {
    /// Activo.
    pub enabled: bool,
    /// Prioridad.
    pub priority: f32,
    /// Alias → nombre de la app (RF-031).
    pub aliases: BTreeMap<String, String>,
    /// Incluir ejecutables del `PATH` (RF-030).
    pub include_path: bool,
}

impl Default for Apps {
    fn default() -> Self {
        Self {
            enabled: true,
            priority: 0.0,
            aliases: BTreeMap::new(),
            include_path: true,
        }
    }
}

impl Apps {
    /// Ajustes comunes.
    pub fn common(&self) -> PluginCommon {
        PluginCommon {
            enabled: self.enabled,
            priority: self.priority,
        }
    }
}
