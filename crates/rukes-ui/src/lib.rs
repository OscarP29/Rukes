//! Ventana, render y animaciones del launcher (iced + tiny-skia, ADR 0001).
//!
//! El resto del proyecto no ve tipos de iced: la UI se comunica con la aplicación a través del
//! trait [`Host`] y de un canal de [`UiEvent`].
#![forbid(unsafe_code)]

mod app;
mod theme;

use std::sync::{Arc, Mutex};

use iced::futures::channel::mpsc;
use rukes_core::{ActionOutcome, Icon, QueryBatch, QueryId, ResultItem};

/// Imagen RGBA de un icono: `(ancho, alto, píxeles)`.
pub type IconRgba = (u32, u32, Vec<u8>);

/// Receptor de lotes de resultados del motor.
pub type Sink = Arc<dyn Fn(QueryBatch) + Send + Sync>;

/// Lo que la UI necesita de la aplicación. Los métodos pueden llamarse desde hilos del pool de
/// iced; los marcados como bloqueantes nunca se llaman desde el hilo de UI.
pub trait Host: Send + Sync + 'static {
    /// Lanza una búsqueda.
    fn query(&self, text: &str, sink: Sink) -> QueryId;
    /// Ejecuta una acción (bloqueante).
    fn execute(
        &self,
        query: &str,
        item: &ResultItem,
        action: &str,
    ) -> Result<ActionOutcome, String>;
    /// La ventana existe. Devuelve `true` si el fondo es translúcido (Mica/Acrílico).
    fn window_ready(&self, hwnd: isize) -> bool;
    /// Ocultar la ventana.
    fn hide(&self);
    /// Carga un icono al tamaño indicado en píxeles físicos (bloqueante).
    fn icon(&self, icon: &Icon, px: u32) -> Option<IconRgba>;
    /// La ventana se acaba de mostrar.
    fn shown(&self) {}
}

/// Qué hacer con el texto al mostrar la ventana (RF-004).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum OnShow {
    /// Vaciar.
    Clear,
    /// Conservar.
    Keep,
    /// Conservar y seleccionar todo.
    #[default]
    SelectAll,
}

/// Ajustes de la UI.
#[derive(Debug, Clone)]
pub struct UiConfig {
    /// Ancho lógico.
    pub width: f32,
    /// Filas visibles.
    pub max_results: usize,
    /// Ocultar al perder el foco.
    pub hide_on_blur: bool,
    /// Ocultar tras una acción.
    pub hide_after_action: bool,
    /// Política de texto al mostrar.
    pub on_show: OnShow,
}

impl Default for UiConfig {
    fn default() -> Self {
        Self {
            width: 680.0,
            max_results: 8,
            hide_on_blur: true,
            hide_after_action: true,
            on_show: OnShow::SelectAll,
        }
    }
}

/// Eventos que la aplicación envía a la UI.
#[derive(Debug, Clone)]
pub enum UiEvent {
    /// La ventana se acaba de mostrar (la aplicación ya la hizo visible y le dio el foco).
    Shown,
    /// Ocultar.
    Hide,
    /// Nueva configuración.
    Config(UiConfig),
    /// Mensaje breve para el usuario (errores de config, avisos).
    Notice(String),
    /// Resultados del motor.
    Batch(QueryBatch),
    /// Sustituye la lista por `items` si el primer resultado mostrado tiene la clave
    /// `replaces` (`plugin:id`). Lo usan las acciones que terminan en segundo plano, como un
    /// script que muestra su salida en el launcher.
    Results {
        /// Clave del resultado provisional («Ejecutando…»).
        replaces: String,
        /// Resultados nuevos (con `plugin_id` ya relleno).
        items: Vec<ResultItem>,
    },
}

/// Extremo emisor del canal de la UI. Se puede clonar y usar desde cualquier hilo.
pub type UiSender = mpsc::UnboundedSender<UiEvent>;

static INBOX: Mutex<Option<mpsc::UnboundedReceiver<UiEvent>>> = Mutex::new(None);

/// Crea el canal de la UI. Debe llamarse una vez antes de [`run`].
pub fn channel() -> UiSender {
    let (tx, rx) = mpsc::unbounded();
    if let Ok(mut inbox) = INBOX.lock() {
        *inbox = Some(rx);
    }
    tx
}

/// Arranca la UI en el hilo actual (bloquea hasta salir). La ventana empieza oculta.
pub fn run(host: Arc<dyn Host>, tx: UiSender, config: UiConfig) -> iced::Result {
    app::run(host, tx, config)
}
