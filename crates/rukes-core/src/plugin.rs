//! Trait que implementan todos los plugins, integrados o externos (RF-174).

use crate::{CancelToken, Query, ResultItem};

/// Metadatos de un plugin.
#[derive(Debug, Clone)]
pub struct PluginMetadata {
    /// Id único y estable (`apps`, `calc`...).
    pub id: String,
    /// Nombre visible.
    pub name: String,
    /// Prefijo que restringe la búsqueda a este plugin (`=`, `>`...).
    pub prefix: Option<String>,
    /// Palabras clave sin prefijo (`kill`, `clip`...).
    pub keywords: Vec<String>,
    /// Prioridad: se suma a la puntuación (escala 0..1, normalmente 0).
    pub priority: f32,
    /// `true` si participa en las búsquedas sin prefijo.
    pub global: bool,
    /// `true`: si la consulta es justo una de sus palabras clave (`clip`, sin espacio), además
    /// de la búsqueda global se muestra su lista, arriba.
    pub list_on_keyword: bool,
}

/// Qué hacer con la ventana tras ejecutar una acción.
#[derive(Debug, Clone)]
pub enum ActionOutcome {
    /// Ocultar el launcher.
    Hide,
    /// Mantenerlo abierto.
    KeepOpen,
    /// Sustituir el texto de búsqueda.
    ReplaceQuery(String),
    /// Mostrar un mensaje breve y mantenerlo abierto.
    Notify(String),
    /// Sustituir la lista por estos resultados (confirmaciones, submenús). El motor rellena su
    /// `plugin_id`.
    ShowResults(Vec<crate::ResultItem>),
    /// Ocultar el launcher y pegar `text` en la ventana que tenía el foco (RF-103, RF-112).
    Paste {
        /// Texto.
        text: String,
        /// Pulsaciones de `←` tras pegar (posición de `{cursor}` en un snippet).
        cursor_left: usize,
        /// Devolver después al portapapeles lo que había antes (snippets).
        restore: bool,
    },
    /// Abrir el panel visual de ajustes del launcher.
    OpenSettings,
}

/// Error de un plugin.
#[derive(Debug, thiserror::Error)]
pub enum PluginError {
    /// La acción no existe para este resultado.
    #[error("acción no soportada: {0}")]
    UnsupportedAction(String),
    /// Fallo al ejecutar.
    #[error("{0}")]
    Failed(String),
}

/// Un plugin de búsqueda.
///
/// `query` se ejecuta en un hilo del pool del motor; puede llamarse en paralelo para consultas
/// distintas. Debe respetar `cancel` y entregar resultados con `sink` (puede llamarlo varias veces
/// para resultados incrementales).
pub trait Plugin: Send + Sync {
    /// Metadatos.
    fn metadata(&self) -> PluginMetadata;

    /// Busca y entrega resultados.
    fn query(&self, query: &Query, cancel: &CancelToken, sink: &mut dyn FnMut(Vec<ResultItem>));

    /// Ejecuta una acción sobre un resultado propio.
    fn execute(&self, item: &ResultItem, action: &str) -> Result<ActionOutcome, PluginError>;

    /// Reconstruye índices (llamado en segundo plano). Por defecto no hace nada.
    fn refresh(&self) {}
}
