//! Resultados y acciones.

use std::path::PathBuf;

/// Identificador de acción. Las acciones estándar están en las constantes asociadas.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct ActionId(pub String);

impl ActionId {
    /// Acción principal (`Enter`).
    pub const OPEN: &'static str = "open";
    /// Abrir como administrador (`Ctrl+Enter`).
    pub const RUN_AS_ADMIN: &'static str = "run_as_admin";
    /// Abrir la carpeta contenedora (`Ctrl+Shift+Enter`).
    pub const OPEN_LOCATION: &'static str = "open_location";
    /// Copiar la ruta (`Ctrl+C`).
    pub const COPY_PATH: &'static str = "copy_path";

    /// Crea un id a partir de un texto.
    pub fn new(s: impl Into<String>) -> Self {
        Self(s.into())
    }

    /// Texto del id.
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

/// Acción disponible sobre un resultado.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Action {
    /// Identificador.
    pub id: ActionId,
    /// Clave de i18n o texto a mostrar.
    pub title: String,
}

impl Action {
    /// Crea una acción.
    pub fn new(id: &str, title: impl Into<String>) -> Self {
        Self {
            id: ActionId::new(id),
            title: title.into(),
        }
    }
}

/// Origen del icono de un resultado. La UI lo resuelve y lo cachea.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum Icon {
    /// Sin icono.
    None,
    /// Icono extraído del archivo, acceso directo o ejecutable.
    File(PathBuf),
    /// Icono de un elemento del shell (por ejemplo `shell:AppsFolder\<AUMID>`).
    Shell(String),
    /// Icono interno con nombre (glifo del tema).
    Named(String),
}

/// Un resultado de búsqueda.
#[derive(Debug, Clone)]
pub struct ResultItem {
    /// Id estable dentro del plugin (se usa para frecency).
    pub id: String,
    /// Id del plugin que lo produjo (lo rellena el motor).
    pub plugin_id: String,
    /// Título.
    pub title: String,
    /// Subtítulo.
    pub subtitle: String,
    /// Icono.
    pub icon: Icon,
    /// Puntuación difusa normalizada (0..1) que calcula el plugin.
    pub fuzzy: f32,
    /// `true` si coincide exactamente con un alias del usuario.
    pub alias_match: bool,
    /// Puntuación final (la calcula el motor).
    pub score: f32,
    /// Acciones disponibles; la primera es la principal.
    pub actions: Vec<Action>,
    /// Datos opacos para el plugin (ruta, AUMID, etc.).
    pub payload: String,
}

impl ResultItem {
    /// Crea un resultado con la acción principal `open`.
    pub fn new(id: impl Into<String>, title: impl Into<String>) -> Self {
        Self {
            id: id.into(),
            plugin_id: String::new(),
            title: title.into(),
            subtitle: String::new(),
            icon: Icon::None,
            fuzzy: 0.0,
            alias_match: false,
            score: 0.0,
            actions: vec![Action::new(ActionId::OPEN, "action.open")],
            payload: String::new(),
        }
    }

    /// Clave global del resultado: `plugin_id:id`.
    pub fn key(&self) -> String {
        format!("{}:{}", self.plugin_id, self.id)
    }

    /// `true` si el resultado admite la acción.
    pub fn has_action(&self, id: &str) -> bool {
        self.actions.iter().any(|a| a.id.as_str() == id)
    }
}
