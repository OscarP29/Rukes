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
}

impl Default for General {
    fn default() -> Self {
        Self {
            hotkey: "Alt+Space".into(),
            language: "auto".into(),
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
