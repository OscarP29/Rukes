//! Snippets (sección 5.11, RF-110 a RF-112).
//!
//! Se leen de la carpeta `snippets` de la configuración:
//!
//! - `*.toml` con una o varias tablas `[[snippet]]` (`name`, `keyword`, `content`);
//! - `*.md`: un snippet por archivo, con cabecera opcional `---` (`name:`, `keyword:`); sin
//!   cabecera, el nombre y la palabra clave son el nombre del archivo.
//!
//! Variables: `{fecha}`/`{date}`, `{hora}`/`{time}`, `{fechahora}`/`{datetime}`,
//! `{portapapeles}`/`{clipboard}`, `{cursor}` y campos `{campo:Nombre}`/`{field:Name}` que se
//! rellenan escribiendo tras la palabra clave (separados por `;` si hay varios). `{{` y `}}`
//! escriben llaves.

use std::path::{Path, PathBuf};
use std::sync::{Arc, RwLock};

use rukes_core::i18n::{Lang, language, tr, trf};
use rukes_core::{
    Action, ActionId, ActionOutcome, CancelToken, Fuzzy, Icon, Plugin, PluginError, PluginMetadata,
    Query, ResultItem,
};
use serde::Deserialize;

use crate::util::{ellipsis, fold};

/// Un snippet.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Snippet {
    /// Nombre visible.
    pub name: String,
    /// Palabra clave (minúsculas, sin espacios).
    pub keyword: String,
    /// Contenido con variables.
    pub content: String,
    /// Archivo de origen.
    pub path: PathBuf,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct TomlSnippet {
    name: String,
    keyword: Option<String>,
    content: String,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct TomlFile {
    #[serde(default)]
    snippet: Vec<TomlSnippet>,
}

fn keyword_of(s: &str) -> String {
    fold(s.trim())
        .split_whitespace()
        .collect::<Vec<_>>()
        .join("-")
}

/// Snippets de un archivo `.md` (`stem` = nombre del archivo).
pub fn parse_md(text: &str, stem: &str, path: &Path) -> Snippet {
    let text = text.strip_prefix('\u{feff}').unwrap_or(text);
    let (mut name, mut keyword) = (stem.to_owned(), None);
    let mut body = text;
    if let Some(rest) = text
        .strip_prefix("---\r\n")
        .or_else(|| text.strip_prefix("---\n"))
        && let Some(end) = rest.find("\n---")
    {
        for line in rest[..end].lines() {
            if let Some((k, v)) = line.split_once(':') {
                let v = v.trim().trim_matches('"').to_owned();
                match k.trim() {
                    "name" | "nombre" => name = v,
                    "keyword" | "clave" => keyword = Some(v),
                    _ => {}
                }
            }
        }
        body = rest[end + 4..].trim_start_matches(['\r', '-']);
        body = body.strip_prefix('\n').unwrap_or(body);
    }
    Snippet {
        keyword: keyword_of(keyword.as_deref().unwrap_or(stem)),
        name,
        content: body.trim_end_matches(['\r', '\n']).to_owned(),
        path: path.to_owned(),
    }
}

/// Snippets de un archivo `.toml`.
pub fn parse_toml(text: &str, path: &Path) -> Result<Vec<Snippet>, String> {
    let f: TomlFile = toml::from_str(text).map_err(|e| {
        let line = e.span().map_or(0, |s| {
            text[..s.start.min(text.len())].matches('\n').count() + 1
        });
        format!("{}:{line}: {}", path.display(), e.message().trim())
    })?;
    Ok(f.snippet
        .into_iter()
        .map(|s| Snippet {
            keyword: keyword_of(s.keyword.as_deref().unwrap_or(&s.name)),
            name: s.name,
            content: s.content.trim_end_matches(['\r', '\n']).to_owned(),
            path: path.to_owned(),
        })
        .collect())
}

/// Lee todos los snippets de `dir`. Devuelve también los errores (archivo y línea).
pub fn load_dir(dir: &Path) -> (Vec<Snippet>, Vec<String>) {
    let mut out = Vec::new();
    let mut errors = Vec::new();
    let Ok(rd) = std::fs::read_dir(dir) else {
        return (out, errors);
    };
    let mut files: Vec<PathBuf> = rd.flatten().map(|e| e.path()).collect();
    files.sort();
    for p in files {
        let ext = p
            .extension()
            .map(|e| e.to_string_lossy().to_lowercase())
            .unwrap_or_default();
        if ext != "toml" && ext != "md" {
            continue;
        }
        let Ok(text) = std::fs::read_to_string(&p) else {
            continue;
        };
        if ext == "md" {
            let stem = p
                .file_stem()
                .map(|s| s.to_string_lossy().into_owned())
                .unwrap_or_default();
            out.push(parse_md(&text, &stem, &p));
        } else {
            match parse_toml(&text, &p) {
                Ok(v) => out.extend(v),
                Err(e) => errors.push(e),
            }
        }
    }
    (out, errors)
}

/// Resultado de expandir un snippet.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Expanded {
    /// Texto final.
    pub text: String,
    /// Pulsaciones de `←` para dejar el cursor en `{cursor}`.
    pub cursor_left: usize,
    /// Campos que siguen vacíos.
    pub missing: Vec<String>,
}

/// Datos para las variables.
pub struct Vars<'a> {
    /// Fecha y hora: `(año, mes, día, hora, minuto)`.
    pub now: (i32, u32, u32, u32, u32),
    /// Texto del portapapeles (solo se llama si el snippet lo usa).
    pub clipboard: &'a dyn Fn() -> Option<String>,
}

/// Nombres de los campos `{campo:X}` en orden de aparición, sin repetir.
pub fn fields(content: &str) -> Vec<String> {
    let mut out: Vec<String> = Vec::new();
    let mut rest = content;
    while let Some(i) = rest.find('{') {
        rest = &rest[i + 1..];
        if rest.starts_with('{') {
            rest = &rest[1..];
            continue;
        }
        let Some(end) = rest.find('}') else { break };
        let tag = &rest[..end];
        if let Some((k, name)) = tag.split_once(':')
            && matches!(k.trim(), "campo" | "field")
        {
            let name = name.trim().to_owned();
            if !out.contains(&name) {
                out.push(name);
            }
        }
        rest = &rest[end + 1..];
    }
    out
}

/// Expande variables y campos. `values` rellena los campos en orden.
pub fn expand(content: &str, values: &[String], vars: &Vars<'_>) -> Expanded {
    let names = fields(content);
    let (y, mo, d, h, mi) = vars.now;
    let date = if language() == Lang::Es {
        format!("{d:02}/{mo:02}/{y}")
    } else {
        format!("{y}-{mo:02}-{d:02}")
    };
    let time = format!("{h:02}:{mi:02}");
    let mut out = String::with_capacity(content.len());
    let mut cursor: Option<usize> = None;
    let mut missing = Vec::new();
    let mut clip: Option<String> = None;
    let mut chars = content.char_indices().peekable();
    while let Some((i, c)) = chars.next() {
        match c {
            '{' if content[i + 1..].starts_with('{') => {
                chars.next();
                out.push('{');
            }
            '}' if content[i + 1..].starts_with('}') => {
                chars.next();
                out.push('}');
            }
            '{' => {
                let Some(end) = content[i + 1..].find('}') else {
                    out.push(c);
                    continue;
                };
                let tag = &content[i + 1..i + 1 + end];
                let value: Option<String> = match tag.trim() {
                    "fecha" | "date" => Some(date.clone()),
                    "hora" | "time" => Some(time.clone()),
                    "fechahora" | "datetime" => Some(format!("{date} {time}")),
                    "portapapeles" | "clipboard" => Some(
                        clip.get_or_insert_with(|| (vars.clipboard)().unwrap_or_default())
                            .clone(),
                    ),
                    "cursor" => {
                        cursor = Some(out.len());
                        Some(String::new())
                    }
                    t => t.split_once(':').and_then(|(k, name)| {
                        matches!(k.trim(), "campo" | "field").then(|| {
                            let name = name.trim();
                            let idx = names.iter().position(|n| n == name).unwrap_or(0);
                            match values.get(idx).map(|v| v.trim()).filter(|v| !v.is_empty()) {
                                Some(v) => v.to_owned(),
                                None => {
                                    if !missing.iter().any(|m: &String| m == name) {
                                        missing.push(name.to_owned());
                                    }
                                    String::new()
                                }
                            }
                        })
                    }),
                };
                match value {
                    Some(v) => {
                        out.push_str(&v);
                        // Salta hasta la llave de cierre.
                        while let Some((j, _)) = chars.peek() {
                            if *j > i + 1 + end {
                                break;
                            }
                            chars.next();
                        }
                    }
                    None => out.push(c),
                }
            }
            c => out.push(c),
        }
    }
    // `←` mueve un carácter; `\r\n` cuenta como uno.
    let cursor_left = cursor.map_or(0, |at| out[at..].replace("\r\n", "\n").chars().count());
    Expanded {
        text: out,
        cursor_left,
        missing,
    }
}

/// Snippets cargados, compartidos con el vigilante de la carpeta.
pub struct SnippetStore {
    dir: PathBuf,
    snippets: RwLock<Vec<Snippet>>,
}

/// Ejemplo que se copia al crear la carpeta.
const EXAMPLE: &str = include_str!("../snippets/ejemplos.toml");

impl SnippetStore {
    /// Abre la carpeta (la crea con un ejemplo si no existe) y carga los snippets.
    pub fn open(dir: PathBuf) -> (Arc<Self>, Vec<String>) {
        if !dir.exists() && std::fs::create_dir_all(&dir).is_ok() {
            let _ = std::fs::write(dir.join("ejemplos.toml"), EXAMPLE);
        }
        let s = Arc::new(Self {
            dir,
            snippets: RwLock::new(Vec::new()),
        });
        let errors = s.reload();
        (s, errors)
    }

    /// Carpeta.
    pub fn dir(&self) -> &Path {
        &self.dir
    }

    /// Vuelve a leer la carpeta. Devuelve los errores.
    pub fn reload(&self) -> Vec<String> {
        let (v, errors) = load_dir(&self.dir);
        tracing::info!(
            snippets = v.len(),
            errors = errors.len(),
            "snippets cargados"
        );
        *self
            .snippets
            .write()
            .unwrap_or_else(std::sync::PoisonError::into_inner) = v;
        errors
    }

    /// Snippets.
    pub fn all(&self) -> Vec<Snippet> {
        self.snippets
            .read()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .clone()
    }
}

/// Plugin de snippets.
pub struct SnippetsPlugin {
    /// Snippets.
    pub store: Arc<SnippetStore>,
    /// Palabra clave para listarlos todos.
    pub keyword: String,
    /// `true`: Enter pega en la ventana anterior; `false`: copia.
    pub paste: bool,
}

fn vars_now() -> (i32, u32, u32, u32, u32) {
    let t = rukes_platform::time::local_now();
    (t.year, t.month, t.day, t.hour, t.minute)
}

/// Valores escritos tras la palabra clave: `;` separa campos si hay más de uno.
fn split_values(rest: &str, n_fields: usize) -> Vec<String> {
    if n_fields <= 1 {
        vec![rest.trim().to_owned()]
    } else {
        rest.split(';').map(|v| v.trim().to_owned()).collect()
    }
}

impl SnippetsPlugin {
    fn item(&self, s: &Snippet, values: &[String], score: f32, exact: bool) -> ResultItem {
        let clip = || None;
        let e = expand(
            &s.content,
            values,
            &Vars {
                now: vars_now(),
                clipboard: &clip,
            },
        );
        let mut it = ResultItem::new(format!("s:{}", s.keyword), s.name.clone());
        it.subtitle = if !e.missing.is_empty() {
            trf(
                "snippets.type_fields",
                &[("keyword", &s.keyword), ("fields", &e.missing.join("; "))],
            )
        } else {
            format!("{} · {}", s.keyword, ellipsis(e.text.trim(), 80))
        };
        it.icon = Icon::Named("snippet".into());
        it.fuzzy = score;
        it.alias_match = exact;
        it.detail = Some(e.text.chars().take(4000).collect());
        it.actions = vec![
            Action::new(
                ActionId::OPEN,
                tr(if self.paste {
                    "snippets.insert"
                } else {
                    "snippets.copy"
                }),
            ),
            Action::new(ActionId::COPY_PATH, tr("snippets.copy")),
            Action::new(ActionId::EDIT, tr("snippets.edit")),
        ];
        it.payload = serde_json::json!({
            "keyword": s.keyword,
            "values": values,
            "path": s.path,
        })
        .to_string();
        it
    }

    /// Resultados. `targeted`: el usuario escribió la palabra clave (`snip ...`).
    pub fn search(&self, text: &str, targeted: bool) -> Vec<ResultItem> {
        let all = self.store.all();
        let text = text.trim();
        // `firma Ana`: la primera palabra es la clave de un snippet y el resto, sus campos.
        let (first, rest) = text
            .split_once(char::is_whitespace)
            .map_or((text, ""), |(a, b)| (a, b));
        let folded = fold(first);
        if let Some(s) = all.iter().find(|s| s.keyword == folded) {
            let values = split_values(rest, fields(&s.content).len());
            return vec![self.item(s, &values, 1.0, true)];
        }
        if text.is_empty() {
            return if targeted {
                all.iter()
                    .enumerate()
                    .map(|(i, s)| self.item(s, &[], 1.0 - i as f32 * 0.001, false))
                    .collect()
            } else {
                Vec::new()
            };
        }
        let mut f = Fuzzy::new(text);
        all.iter()
            .filter_map(|s| {
                let score = f.best([s.name.as_str(), s.keyword.as_str()])?;
                Some(self.item(s, &[], score, false))
            })
            .collect()
    }
}

impl Plugin for SnippetsPlugin {
    fn metadata(&self) -> PluginMetadata {
        PluginMetadata {
            id: "snippets".into(),
            name: tr("snippets.name").into(),
            prefix: None,
            keywords: vec![self.keyword.clone()],
            priority: 0.2,
            global: true,
            list_on_keyword: true,
        }
    }

    fn query(&self, q: &Query, _cancel: &CancelToken, sink: &mut dyn FnMut(Vec<ResultItem>)) {
        sink(self.search(&q.text, q.targeted));
    }

    fn execute(&self, item: &ResultItem, action: &str) -> Result<ActionOutcome, PluginError> {
        let p: serde_json::Value =
            serde_json::from_str(&item.payload).map_err(|e| PluginError::Failed(e.to_string()))?;
        let keyword = p["keyword"].as_str().unwrap_or_default();
        let values: Vec<String> = p["values"]
            .as_array()
            .map(|a| {
                a.iter()
                    .filter_map(|v| v.as_str().map(str::to_owned))
                    .collect()
            })
            .unwrap_or_default();
        let s = self
            .store
            .all()
            .into_iter()
            .find(|s| s.keyword == keyword)
            .ok_or_else(|| PluginError::Failed(tr("snippets.gone").into()))?;
        if action == ActionId::EDIT {
            rukes_platform::shell::launch(
                &s.path.to_string_lossy(),
                None,
                None,
                rukes_platform::shell::Verb::Open,
            )
            .map_err(|e| PluginError::Failed(e.to_string()))?;
            return Ok(ActionOutcome::Hide);
        }
        let clip = || rukes_platform::clipboard::get_text(crate::clipboard::MAX_CHARS);
        let e = expand(
            &s.content,
            &values,
            &Vars {
                now: vars_now(),
                clipboard: &clip,
            },
        );
        if !e.missing.is_empty() {
            // Faltan campos: se deja escrita la clave para rellenarlos.
            return Ok(ActionOutcome::ReplaceQuery(format!("{} ", s.keyword)));
        }
        match action {
            ActionId::OPEN if self.paste => Ok(ActionOutcome::Paste {
                text: e.text,
                cursor_left: e.cursor_left,
                restore: true,
            }),
            ActionId::OPEN | ActionId::COPY_PATH => {
                rukes_platform::clipboard::set_text_private(&e.text)
                    .map_err(|e| PluginError::Failed(e.to_string()))?;
                Ok(ActionOutcome::Hide)
            }
            other => Err(PluginError::UnsupportedAction(other.into())),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn vars() -> Vars<'static> {
        Vars {
            now: (2026, 11, 4, 9, 5),
            clipboard: &|| Some("PEGADO".into()),
        }
    }

    #[test]
    fn expands_variables_fields_and_cursor() {
        let _ = rukes_core::i18n::set_language("es", "");
        let e = expand(
            "Hola {campo:Nombre}, hoy {fecha} a las {hora}.\n{cursor}\n-- {campo:Cargo} {campo:Nombre} {{x}} {desconocida} {portapapeles}",
            &["Ana".into(), "Gerente".into()],
            &vars(),
        );
        assert_eq!(
            e.text,
            "Hola Ana, hoy 04/11/2026 a las 09:05.\n\n-- Gerente Ana {x} {desconocida} PEGADO"
        );
        assert!(e.missing.is_empty());
        assert_eq!(
            e.cursor_left,
            "\n-- Gerente Ana {x} {desconocida} PEGADO".chars().count()
        );
        let e = expand("A {field:x} {field:y}", &["1".into()], &vars());
        assert_eq!(e.missing, ["y"]);
        assert_eq!(fields("{campo:a} {campo:b} {campo:a}"), ["a", "b"]);
        assert_eq!(expand("sin cierre {", &[], &vars()).text, "sin cierre {");
        assert_eq!(expand("ñ{cursor}ñ\r\nx", &[], &vars()).cursor_left, 3);
    }

    #[test]
    fn parses_toml_and_markdown() {
        let p = Path::new("x");
        let v = parse_toml(
            "[[snippet]]\nname = \"Firma correo\"\ncontent = \"\"\"\nSaludos\n\"\"\"\n[[snippet]]\nname = \"Dir\"\nkeyword = \"dir\"\ncontent = \"Calle 1\"\n",
            p,
        )
        .unwrap();
        assert_eq!(v[0].keyword, "firma-correo");
        assert_eq!(v[0].content, "Saludos");
        assert_eq!(v[1].keyword, "dir");
        assert!(
            parse_toml("[[snippet]]\nname = 1\n", p)
                .unwrap_err()
                .contains(":2:")
        );
        let m = parse_md("---\nname: Firma\nkeyword: fir\n---\nUno\nDos\n", "f", p);
        assert_eq!(
            (m.name.as_str(), m.keyword.as_str(), m.content.as_str()),
            ("Firma", "fir", "Uno\nDos")
        );
        let m = parse_md("Solo texto", "Mi Nota", p);
        assert_eq!(
            (m.name.as_str(), m.keyword.as_str()),
            ("Mi Nota", "mi-nota")
        );
    }

    #[test]
    fn example_file_is_valid() {
        let v = parse_toml(EXAMPLE, Path::new("ejemplos.toml")).unwrap();
        assert!(v.len() >= 3);
    }

    #[test]
    fn search_by_keyword_with_fields() {
        let _ = rukes_core::i18n::set_language("es", "");
        let dir = std::env::temp_dir().join(format!("rukes-snip-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(
            dir.join("a.toml"),
            "[[snippet]]\nname = \"Saludo\"\nkeyword = \"sal\"\ncontent = \"Hola {campo:Quién}{cursor}!\"\n",
        )
        .unwrap();
        let (store, errors) = SnippetStore::open(dir.clone());
        assert!(errors.is_empty());
        let p = SnippetsPlugin {
            store,
            keyword: "snip".into(),
            paste: true,
        };
        let r = p.search("sal", false);
        assert!(r[0].alias_match);
        assert!(r[0].subtitle.contains("Quién"));
        assert!(matches!(
            p.execute(&r[0], ActionId::OPEN).unwrap(),
            ActionOutcome::ReplaceQuery(q) if q == "sal "
        ));
        let r = p.search("sal Ana", false);
        match p.execute(&r[0], ActionId::OPEN).unwrap() {
            ActionOutcome::Paste {
                text,
                cursor_left,
                restore,
            } => {
                assert_eq!(text, "Hola Ana!");
                assert_eq!(cursor_left, 1);
                assert!(restore);
            }
            other => panic!("{other:?}"),
        }
        assert_eq!(p.search("salu", false)[0].title, "Saludo");
        assert!(p.search("", false).is_empty());
        assert_eq!(p.search("", true).len(), 1);
        let _ = std::fs::remove_dir_all(dir);
    }
}
