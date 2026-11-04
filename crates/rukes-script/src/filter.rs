//! Resultados en JSON que devuelve un script (*script filters*, RF-140).
//!
//! El script escribe en stdout una lista o un objeto con `items`:
//!
//! ```json
//! { "items": [
//!   { "title": "Abrir el repo", "subtitle": "github.com/x/y", "open": "https://github.com/x/y" },
//!   { "title": "Copiar el id", "copy": "1234" },
//!   { "title": "Buscar otra cosa", "query": "gh issue " }
//! ] }
//! ```
//!
//! `open` abre una ruta o URL, `copy` copia un texto y `query` sustituye la búsqueda. Sin
//! ninguna de las tres, Enter copia el título.

use serde::Deserialize;

/// Un resultado devuelto por el script.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
pub struct FilterItem {
    /// Título.
    pub title: String,
    /// Subtítulo.
    #[serde(default, alias = "description")]
    pub subtitle: String,
    /// Icono (ruta).
    #[serde(default)]
    pub icon: Option<String>,
    /// Ruta, URL o URI que se abre con Enter.
    #[serde(default, alias = "url", alias = "path")]
    pub open: Option<String>,
    /// Texto que se copia con Enter.
    #[serde(default, alias = "arg", alias = "text")]
    pub copy: Option<String>,
    /// Nueva búsqueda.
    #[serde(default)]
    pub query: Option<String>,
}

/// Acción principal de un resultado.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ItemAction {
    /// Abrir.
    Open(String),
    /// Copiar.
    Copy(String),
    /// Sustituir la búsqueda.
    Query(String),
}

impl FilterItem {
    /// Acción de Enter.
    pub fn action(&self) -> ItemAction {
        if let Some(o) = &self.open {
            ItemAction::Open(o.clone())
        } else if let Some(c) = &self.copy {
            ItemAction::Copy(c.clone())
        } else if let Some(q) = &self.query {
            ItemAction::Query(q.clone())
        } else {
            ItemAction::Copy(self.title.clone())
        }
    }
}

#[derive(Deserialize)]
#[serde(untagged)]
enum Doc {
    List(Vec<FilterItem>),
    Wrapped { items: Vec<FilterItem> },
}

/// Interpreta la salida como resultados JSON. `None` si no es JSON con esa forma.
pub fn parse(stdout: &str) -> Option<Vec<FilterItem>> {
    let t = stdout.trim().trim_start_matches('\u{feff}');
    if !(t.starts_with('[') || t.starts_with('{')) {
        return None;
    }
    match serde_json::from_str::<Doc>(t).ok()? {
        Doc::List(v) | Doc::Wrapped { items: v } => Some(v),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn list_and_wrapped_forms() {
        let a = parse(r#"[{"title":"a","url":"https://x"},{"title":"b","arg":"1"}]"#).unwrap();
        assert_eq!(a.len(), 2);
        assert_eq!(a[0].action(), ItemAction::Open("https://x".into()));
        assert_eq!(a[1].action(), ItemAction::Copy("1".into()));
        let b =
            parse("\n{ \"items\": [ {\"title\":\"c\",\"subtitle\":\"s\",\"query\":\"q \"} ] }\n")
                .unwrap();
        assert_eq!(b[0].subtitle, "s");
        assert_eq!(b[0].action(), ItemAction::Query("q ".into()));
        assert_eq!(
            parse(r#"[{"title":"solo"}]"#).unwrap()[0].action(),
            ItemAction::Copy("solo".into())
        );
    }

    #[test]
    fn plain_text_is_not_json() {
        assert!(parse("hola").is_none());
        assert!(parse("[no json").is_none());
        assert!(parse(r#"{"otra":"cosa"}"#).is_none());
        assert!(parse(r#"[{"sin":"titulo"}]"#).is_none());
    }
}
