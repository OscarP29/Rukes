//! Cambios puntuales en `config.toml` desde el panel visual de ajustes: se escribe solo lo
//! cambiado y se conservan los comentarios, el orden y el formato del resto del archivo.

use std::path::Path;

use toml_edit::{DocumentMut, Item, Table, Value};

/// Valor nuevo de un ajuste.
#[derive(Debug, Clone, PartialEq)]
pub enum EditValue {
    /// Booleano.
    Bool(bool),
    /// Entero.
    Int(i64),
    /// Texto.
    Text(String),
}

impl EditValue {
    fn to_value(&self) -> Value {
        match self {
            Self::Bool(b) => Value::from(*b),
            Self::Int(i) => Value::from(*i),
            Self::Text(t) => Value::from(t.as_str()),
        }
    }
}

/// Aplica los cambios (`ruta.con.puntos`, valor) al texto de una configuración. Las tablas que
/// falten se crean al final; un valor existente conserva su comentario de la misma línea.
/// Devuelve el texto nuevo, que se valida antes de devolverlo.
pub fn apply(source: &str, edits: &[(String, EditValue)]) -> Result<String, String> {
    let mut doc: DocumentMut = source.parse().map_err(|e| format!("config.toml: {e}"))?;
    for (key, value) in edits {
        let parts: Vec<&str> = key.split('.').filter(|p| !p.is_empty()).collect();
        let Some((last, tables)) = parts.split_last() else {
            return Err(format!("clave vacía: «{key}»"));
        };
        let mut table: &mut Table = doc.as_table_mut();
        for name in tables {
            if !table.contains_key(name) {
                let mut t = Table::new();
                t.set_implicit(true);
                table.insert(name, Item::Table(t));
            }
            table = match table.get_mut(name) {
                Some(Item::Table(t)) => t,
                _ => return Err(format!("«{key}»: «{name}» no es una tabla")),
            };
        }
        let mut new = value.to_value();
        match table.get_mut(last) {
            // Se sustituye en su sitio: la clave conserva el comentario de encima y el valor el
            // de su línea.
            Some(item) => {
                if let Some(old) = item.as_value() {
                    *new.decor_mut() = old.decor().clone();
                }
                *item = Item::Value(new);
            }
            None => {
                table.insert(last, Item::Value(new));
            }
        }
    }
    let text = doc.to_string();
    crate::parse(&text, Path::new("config.toml")).map_err(|e| {
        if e.line > 0 {
            format!("{}:{}: {}", e.line, e.column, e.message)
        } else {
            e.message
        }
    })?;
    Ok(text)
}

/// Aplica los cambios al archivo (o a la plantilla, si aún no existe) y lo guarda. No toca el
/// archivo si el resultado no es una configuración válida.
pub fn save(path: &Path, edits: &[(String, EditValue)]) -> Result<(), String> {
    let source = match std::fs::read_to_string(path) {
        Ok(s) => s,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => crate::DEFAULT_CONFIG.to_owned(),
        Err(e) => return Err(e.to_string()),
    };
    let text = apply(&source, edits)?;
    if let Some(dir) = path.parent() {
        std::fs::create_dir_all(dir).map_err(|e| e.to_string())?;
    }
    // Se escribe en un temporal y se renombra: el vigilante nunca ve un archivo a medias.
    let tmp = path.with_extension("toml.tmp");
    std::fs::write(&tmp, text).map_err(|e| e.to_string())?;
    std::fs::rename(&tmp, path).map_err(|e| {
        let _ = std::fs::remove_file(&tmp);
        e.to_string()
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn keeps_comments_and_changes_only_the_values() {
        let src = "# Mi configuración\n[general]\n# tema\ntheme = \"nord\" # el mío\n\n[window]\nwidth = 680\n";
        let out = apply(
            src,
            &[
                ("general.theme".into(), EditValue::Text("midnight".into())),
                ("window.hints".into(), EditValue::Bool(false)),
                ("window.width".into(), EditValue::Int(720)),
                ("plugins.calc.currency_online".into(), EditValue::Bool(true)),
            ],
        )
        .unwrap();
        assert!(out.starts_with("# Mi configuración\n[general]\n# tema\n"));
        assert!(out.contains("theme = \"midnight\" # el mío"), "{out}");
        assert!(out.contains("width = 720"));
        assert!(out.contains("hints = false"));
        assert!(
            out.contains("[plugins.calc]\ncurrency_online = true"),
            "{out}"
        );
        let c = crate::parse(&out, Path::new("c.toml")).unwrap();
        assert_eq!(c.general.theme, "midnight");
        assert!(c.plugins.calc.currency_online);
    }

    #[test]
    fn the_default_template_survives_an_edit() {
        let out = apply(
            crate::DEFAULT_CONFIG,
            &[(
                "window.position".into(),
                EditValue::Text("bottom_center".into()),
            )],
        )
        .unwrap();
        let c = crate::parse(&out, Path::new("c.toml")).unwrap();
        assert_eq!(c.window.position, crate::WindowPosition::BottomCenter);
        assert!(out.starts_with("#:schema"));
    }

    #[test]
    fn invalid_values_are_rejected() {
        let e = apply(
            "",
            &[(
                "window.position".into(),
                EditValue::Text("izquierda".into()),
            )],
        )
        .unwrap_err();
        assert!(e.contains("izquierda") || e.contains("variant"), "{e}");
        assert!(
            apply(
                "[general]\ntheme = 3\n",
                &[("general.theme.x".into(), EditValue::Bool(true))]
            )
            .is_err()
        );
    }

    #[test]
    fn save_writes_the_file_atomically() {
        let dir = std::env::temp_dir().join(format!("rukes-edit-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        let file = dir.join("config.toml");
        save(&file, &[("window.max_results".into(), EditValue::Int(10))]).unwrap();
        let c = crate::load(&file).unwrap();
        assert_eq!(c.window.max_results, 10);
        assert!(!dir.join("config.toml.tmp").exists());
        let _ = std::fs::remove_dir_all(&dir);
    }
}
