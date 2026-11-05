//! Ventana, render y animaciones del launcher (iced + tiny-skia, ADR 0001).
//!
//! El resto del proyecto no ve tipos de iced: la UI se comunica con la aplicación a través del
//! trait [`Host`] y de un canal de [`UiEvent`].
#![forbid(unsafe_code)]

mod app;
mod hints;
mod http_panel;
mod json_highlight;
mod panel;
mod theme;

pub use hints::options as hint_options;

use std::path::Path;
use std::sync::{Arc, Mutex};

use iced::futures::channel::mpsc;
use rukes_core::{ActionOutcome, HttpForm, HttpResponse, Icon, QueryBatch, QueryId, ResultItem};

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
    /// Pega `text` en la ventana que tenía el foco (la ventana ya se ocultó). No bloquea.
    fn paste(&self, _text: &str, _cursor_left: usize, _restore: bool) {}
    /// Contenido del panel de previsualización de un archivo o carpeta (bloqueante, RF-017).
    fn preview(&self, _path: &Path) -> Option<Preview> {
        None
    }
    /// Cambia el tamaño lógico de la ventana manteniendo fijo su borde inferior (posición
    /// `bottom_center`).
    fn resize_keep_bottom(&self, _width: f32, _height: f32) {}
    /// Formulario del panel de ajustes con los valores actuales (bloqueante).
    fn settings_form(&self) -> Option<SettingsForm> {
        None
    }
    /// Guarda los ajustes cambiados en `config.toml` (bloqueante). Se aplican al recargarse.
    fn save_settings(&self, _changes: &[(String, SettingValue)]) -> Result<(), String> {
        Err("sin soporte".into())
    }
    /// Abre `config.toml` en el editor.
    fn open_config_file(&self) {}
    /// Envía una petición del mini Postman (bloqueante) y la guarda en el historial.
    fn http_send(&self, _form: &HttpForm) -> Result<HttpResponse, String> {
        Err("sin soporte".into())
    }
    /// Interpreta una orden `curl` (o `POST 3000/api {..}`) pegada en la barra.
    fn http_parse(&self, _text: &str) -> Option<HttpForm> {
        None
    }
    /// Peticiones recientes, la última primero.
    fn http_history(&self) -> Vec<HttpForm> {
        Vec::new()
    }
    /// Elementos fijados, en orden: se muestran con la búsqueda vacía.
    fn pinned(&self) -> Vec<ResultItem> {
        Vec::new()
    }
    /// Mueve un fijado `delta` puestos. Devuelve si se movió.
    fn pin_shift(&self, _item: &ResultItem, _delta: isize) -> bool {
        false
    }
}

/// Valor de un ajuste del panel visual.
#[derive(Debug, Clone, PartialEq)]
pub enum SettingValue {
    /// Sí o no.
    Bool(bool),
    /// Número entero.
    Int(i64),
    /// Texto o una de las opciones de una lista.
    Text(String),
    /// Varias opciones de una lista, en orden.
    List(Vec<String>),
}

/// Tipo de control de un ajuste.
#[derive(Debug, Clone, PartialEq)]
pub enum SettingKind {
    /// Interruptor (`SettingValue::Bool`).
    Toggle,
    /// Lista de opciones `(valor, etiqueta)` (`SettingValue::Text`).
    Choice(Vec<(String, String)>),
    /// Número entre `min` y `max` (`SettingValue::Int`).
    Number {
        /// Mínimo.
        min: i64,
        /// Máximo.
        max: i64,
        /// Paso.
        step: i64,
    },
    /// Texto libre (`SettingValue::Text`).
    Text,
    /// Varias opciones `(valor, etiqueta)` con casillas (`SettingValue::List`). El orden del
    /// valor es el de las opciones. Una etiqueta `comando — descripción` se muestra en dos
    /// columnas.
    Multi(Vec<(String, String)>),
    /// Título de una tarjeta: agrupa los ajustes que le siguen. No es un valor (clave vacía).
    Heading,
}

/// Un ajuste del panel visual.
#[derive(Debug, Clone, PartialEq)]
pub struct SettingField {
    /// Ruta en `config.toml`, p. ej. `window.position`.
    pub key: String,
    /// Nombre visible.
    pub label: String,
    /// Explicación breve (puede estar vacía).
    pub help: String,
    /// Control.
    pub kind: SettingKind,
    /// Valor actual.
    pub value: SettingValue,
}

/// Grupo de ajustes (una pestaña del panel).
#[derive(Debug, Clone, PartialEq)]
pub struct SettingsSection {
    /// Título de la pestaña.
    pub title: String,
    /// Ajustes.
    pub fields: Vec<SettingField>,
}

/// Formulario completo del panel de ajustes.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct SettingsForm {
    /// Pestañas.
    pub sections: Vec<SettingsSection>,
}

/// Contenido del panel de previsualización.
#[derive(Debug, Clone, Default)]
pub struct Preview {
    /// Miniatura o icono grande.
    pub image: Option<IconRgba>,
    /// Primeras líneas de un archivo de texto.
    pub text: Option<String>,
    /// `true` si `text` es Markdown.
    pub markdown: bool,
    /// Datos del archivo: `(etiqueta, valor)`.
    pub info: Vec<(String, String)>,
}

/// Color RGBA de 8 bits.
pub type Rgba8 = [u8; 4];

/// Aspecto de la interfaz: el tema ya resuelto, con el escalado de texto y el contraste alto
/// aplicados por la aplicación (RF-192, RF-214).
#[derive(Debug, Clone, PartialEq)]
pub struct UiStyle {
    /// Fondo (se usa con `opacity` o `solid_opacity`).
    pub background: Rgba8,
    /// Opacidad del fondo sobre Mica/Acrílico.
    pub opacity: f32,
    /// Opacidad del fondo sólido.
    pub solid_opacity: f32,
    /// `true` si la ventana tiene Mica/Acrílico debajo.
    pub translucent: bool,
    /// Texto.
    pub foreground: Rgba8,
    /// Texto secundario.
    pub muted: Rgba8,
    /// Acento.
    pub accent: Rgba8,
    /// Fondo de la fila seleccionada.
    pub selection: Rgba8,
    /// Texto de la fila seleccionada.
    pub selection_foreground: Rgba8,
    /// Separadores y bordes.
    pub divider: Rgba8,
    /// Fondo de tarjetas y controles (`None`: derivado).
    pub surface: Option<Rgba8>,
    /// Fondo al pasar el ratón (`None`: derivado).
    pub surface_hover: Option<Rgba8>,
    /// Texto terciario (`None`: derivado).
    pub text_faint: Option<Rgba8>,
    /// Acento suave (`None`: derivado).
    pub accent_soft: Option<Rgba8>,
    /// Fuente principal.
    pub font: String,
    /// Fuente monoespaciada.
    pub mono: String,
    /// Tamaño del título de las filas.
    pub font_size: f32,
    /// Alto de fila.
    pub row_height: f32,
    /// Lado del icono.
    pub icon_size: f32,
    /// Alto del campo de búsqueda.
    pub search_height: f32,
    /// Margen interior horizontal.
    pub padding: f32,
    /// Separación icono-texto.
    pub spacing: f32,
    /// Radio de filas e iconos.
    pub radius: f32,
}

impl Default for UiStyle {
    /// Tokyo Night.
    fn default() -> Self {
        Self {
            background: [0x1a, 0x1b, 0x26, 255],
            opacity: 0.55,
            solid_opacity: 1.0,
            translucent: false,
            foreground: [0xc0, 0xca, 0xf5, 255],
            muted: [0x73, 0x7a, 0xa2, 255],
            accent: [0x7a, 0xa2, 0xf7, 255],
            selection: [0x28, 0x34, 0x57, 217],
            selection_foreground: [0xc0, 0xca, 0xf5, 255],
            divider: [0x73, 0x7a, 0xa2, 89],
            surface: None,
            surface_hover: None,
            text_faint: None,
            accent_soft: None,
            font: "Segoe UI Variable Text".into(),
            mono: "Cascadia Mono".into(),
            font_size: 15.0,
            row_height: 48.0,
            icon_size: 28.0,
            search_height: 56.0,
            padding: 12.0,
            spacing: 12.0,
            radius: 8.0,
        }
    }
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
    /// Aspecto.
    pub style: UiStyle,
    /// Animaciones (ya combinado con el ajuste de Windows, RF-195).
    pub animations: bool,
    /// Panel de previsualización visible al mostrar la ventana.
    pub preview_panel: bool,
    /// Ancho del panel.
    pub preview_width: f32,
    /// Ventana anclada abajo: la búsqueda va debajo y la lista crece hacia arriba.
    pub anchor_bottom: bool,
    /// Leyenda de comandos con la búsqueda vacía.
    pub hints: bool,
    /// Cuántos comandos muestra la leyenda.
    pub hints_count: usize,
    /// Cuáles y en qué orden (ids de [`hint_options`]); vacío: los de siempre.
    pub hints_items: Vec<String>,
}

impl Default for UiConfig {
    fn default() -> Self {
        Self {
            width: 680.0,
            max_results: 8,
            hide_on_blur: true,
            hide_after_action: true,
            on_show: OnShow::SelectAll,
            style: UiStyle::default(),
            animations: true,
            preview_panel: false,
            preview_width: 340.0,
            anchor_bottom: false,
            hints: true,
            hints_count: 6,
            hints_items: Vec::new(),
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
    /// Abrir el panel de ajustes (desde la bandeja). La aplicación muestra antes la ventana.
    OpenSettings,
    /// Abrir el mini Postman (atajo global). La aplicación muestra antes la ventana.
    OpenHttp,
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
