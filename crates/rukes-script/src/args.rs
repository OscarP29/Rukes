//! Argumentos escritos en línea tras el alias (`gh issue Mi bug`) (RF-133).
//!
//! Cada argumento toma una palabra (o un texto entre comillas), salvo el último si es de tipo
//! texto, que se queda con el resto de la línea: con un único argumento `title`, `Mi bug` es el
//! título completo.

use std::path::PathBuf;

use crate::header::{ArgKind, ArgSpec};

/// Resultado de interpretar los argumentos.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct Parsed {
    /// `(nombre, valor)` en el orden declarado; los opcionales sin valor quedan vacíos.
    pub values: Vec<(String, String)>,
    /// Obligatorios que faltan.
    pub missing: Vec<String>,
    /// Primer error de validación.
    pub error: Option<String>,
    /// Si el usuario está escribiendo una opción: `(índice del argumento, opciones que
    /// encajan)`.
    pub choices: Option<(usize, Vec<String>)>,
}

impl Parsed {
    /// `true` si se puede ejecutar.
    pub fn is_complete(&self) -> bool {
        self.missing.is_empty() && self.error.is_none()
    }
}

/// Separa la siguiente palabra (admite `"comillas"`). Devuelve `(palabra, resto)`.
fn next_token(s: &str) -> Option<(String, &str)> {
    let s = s.trim_start();
    if s.is_empty() {
        return None;
    }
    if let Some(inner) = s.strip_prefix('"') {
        return Some(match inner.find('"') {
            Some(end) => (inner[..end].to_owned(), &inner[end + 1..]),
            None => (inner.to_owned(), ""),
        });
    }
    let end = s.find(char::is_whitespace).unwrap_or(s.len());
    Some((s[..end].to_owned(), &s[end..]))
}

fn unquote(s: &str) -> String {
    let t = s.trim();
    t.strip_prefix('"')
        .and_then(|x| x.strip_suffix('"'))
        .unwrap_or(t)
        .to_owned()
}

/// Expande `~` y `%VAR%` en una ruta.
pub fn expand_path(s: &str) -> PathBuf {
    let mut out = String::new();
    let mut rest = s.trim();
    if let Some(r) = rest.strip_prefix('~')
        && (r.is_empty() || r.starts_with(['/', '\\']))
        && let Some(home) = std::env::var_os("USERPROFILE")
    {
        out.push_str(&home.to_string_lossy());
        rest = r;
    }
    while let Some(start) = rest.find('%') {
        let after = &rest[start + 1..];
        let Some(len) = after.find('%') else {
            break;
        };
        let name = &after[..len];
        match std::env::var(name) {
            Ok(v) if !name.is_empty() => {
                out.push_str(&rest[..start]);
                out.push_str(&v);
            }
            _ => out.push_str(&rest[..start + len + 2]),
        }
        rest = &after[len + 1..];
    }
    out.push_str(rest);
    PathBuf::from(out)
}

fn validate(spec: &ArgSpec, raw: &str) -> Result<String, String> {
    match &spec.kind {
        ArgKind::Text => Ok(raw.to_owned()),
        ArgKind::Number => {
            let n = raw.replace(',', ".");
            n.parse::<f64>()
                .ok()
                .filter(|v| v.is_finite())
                .map(|_| n)
                .ok_or_else(|| format!("«{}» debe ser un número", spec.name))
        }
        ArgKind::Bool => match raw.to_lowercase().as_str() {
            "true" | "yes" | "y" | "si" | "sí" | "s" | "1" | "on" => Ok("true".into()),
            "false" | "no" | "n" | "0" | "off" => Ok("false".into()),
            _ => Err(format!("«{}» debe ser sí o no", spec.name)),
        },
        ArgKind::Choice(options) => {
            let lower = raw.to_lowercase();
            if let Some(o) = options.iter().find(|o| o.to_lowercase() == lower) {
                return Ok(o.clone());
            }
            let matching: Vec<&String> = options
                .iter()
                .filter(|o| o.to_lowercase().starts_with(&lower))
                .collect();
            match matching.as_slice() {
                [one] => Ok((*one).clone()),
                _ => Err(format!(
                    "«{}» debe ser una de: {}",
                    spec.name,
                    options.join(", ")
                )),
            }
        }
        ArgKind::File => {
            let p = expand_path(raw);
            if p.exists() {
                Ok(p.to_string_lossy().into_owned())
            } else {
                Err(format!("no existe: {}", p.display()))
            }
        }
    }
}

/// Interpreta `input` según los argumentos declarados.
pub fn parse(specs: &[ArgSpec], input: &str) -> Parsed {
    let mut out = Parsed::default();
    let mut rest = input.trim();
    let ends_with_space = input.ends_with(char::is_whitespace);
    for (i, spec) in specs.iter().enumerate() {
        let last = i + 1 == specs.len();
        let token = if last && spec.kind == ArgKind::Text {
            let v = unquote(rest);
            rest = "";
            (!v.is_empty()).then_some(v)
        } else {
            next_token(rest).map(|(t, r)| {
                rest = r;
                t
            })
        };
        // ¿Es la palabra que el usuario está escribiendo ahora?
        let typing = rest.trim().is_empty() && !ends_with_space;
        let value = match token {
            Some(t) => match validate(spec, &t) {
                Ok(v) => v,
                Err(e) => {
                    if let ArgKind::Choice(options) = &spec.kind
                        && typing
                    {
                        let lower = t.to_lowercase();
                        let fit: Vec<String> = options
                            .iter()
                            .filter(|o| o.to_lowercase().contains(&lower))
                            .cloned()
                            .collect();
                        out.choices = Some((i, if fit.is_empty() { options.clone() } else { fit }));
                    }
                    if out.error.is_none() {
                        out.error = Some(e);
                    }
                    t
                }
            },
            None => match &spec.default {
                Some(d) => d.clone(),
                None => {
                    if spec.required {
                        out.missing.push(spec.name.clone());
                        if let ArgKind::Choice(options) = &spec.kind
                            && out.choices.is_none()
                        {
                            out.choices = Some((i, options.clone()));
                        }
                    }
                    String::new()
                }
            },
        };
        out.values.push((spec.name.clone(), value));
    }
    out
}

/// Sustituye el valor del argumento `index` en el texto escrito por `value` (para completar
/// una opción). Los argumentos anteriores se conservan tal cual.
pub fn complete(specs: &[ArgSpec], input: &str, index: usize, value: &str) -> String {
    let mut parts = Vec::new();
    let mut rest = input.trim();
    for _ in 0..index.min(specs.len()) {
        match next_token(rest) {
            Some((t, r)) => {
                parts.push(if t.contains(char::is_whitespace) {
                    format!("\"{t}\"")
                } else {
                    t
                });
                rest = r;
            }
            None => break,
        }
    }
    parts.push(value.to_owned());
    parts.join(" ")
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::header::parse as header;

    fn specs(src: &str) -> Vec<ArgSpec> {
        header(src).unwrap().args
    }

    #[test]
    fn single_text_arg_takes_the_whole_rest() {
        let s = specs("# @rukes.arg title:text:required");
        let p = parse(&s, "Mi bug");
        assert!(p.is_complete());
        assert_eq!(p.values, [("title".into(), "Mi bug".into())]);
        let p = parse(&s, "  ");
        assert_eq!(p.missing, ["title"]);
        assert!(!p.is_complete());
        // Comillas alrededor de todo: se quitan.
        assert_eq!(parse(&s, "\"a b\"").values[0].1, "a b");
    }

    #[test]
    fn several_args_with_defaults_and_validation() {
        let s = specs(
            "# @rukes.arg n:number:default=3\n# @rukes.arg env:option(dev|prod)\n\
             # @rukes.arg force:bool:default=no\n# @rukes.arg msg:text",
        );
        let p = parse(&s, "5 pro sí hola mundo");
        assert!(p.is_complete(), "{p:?}");
        assert_eq!(
            p.values,
            [
                ("n".into(), "5".into()),
                ("env".into(), "prod".into()),
                ("force".into(), "true".into()),
                ("msg".into(), "hola mundo".into()),
            ]
        );
        let p = parse(&s, "");
        assert!(p.is_complete());
        assert_eq!(p.values[0].1, "3");
        assert_eq!(p.values[2].1, "no");
        let p = parse(&s, "x");
        assert_eq!(p.error.as_deref(), Some("«n» debe ser un número"));
        assert_eq!(parse(&s, "2,5").values[0].1, "2.5");
        assert_eq!(parse(&s, "\"1\" dev").values[1].1, "dev");
    }

    #[test]
    fn choice_completion() {
        let s = specs("# @rukes.arg env:option(dev|prod|preprod):required\n# @rukes.arg x:text");
        let p = parse(&s, "");
        assert_eq!(p.missing, ["env"]);
        assert_eq!(
            p.choices,
            Some((0, vec!["dev".into(), "prod".into(), "preprod".into()]))
        );
        // «pr» es ambiguo: se ofrecen las que lo contienen.
        let p = parse(&s, "pr");
        assert_eq!(p.choices, Some((0, vec!["prod".into(), "preprod".into()])));
        assert!(p.error.is_some());
        // «pre» es único.
        let p = parse(&s, "pre algo");
        assert!(p.is_complete());
        assert_eq!(p.values[0].1, "preprod");
        assert_eq!(complete(&s, "pr", 0, "prod"), "prod");
        let s2 = specs("# @rukes.arg a:text\n# @rukes.arg env:option(dev|prod)");
        assert_eq!(
            complete(&s2, "\"hola mundo\" d", 1, "dev"),
            "\"hola mundo\" dev"
        );
    }

    #[test]
    fn file_args_and_expansion() {
        let s = specs("# @rukes.arg f:file:required");
        let tmp = std::env::temp_dir();
        let p = parse(&s, &format!("\"{}\"", tmp.display()));
        assert!(p.is_complete(), "{p:?}");
        assert!(parse(&s, "C:\\no\\existe\\nunca.txt").error.is_some());
        let home = std::env::var("USERPROFILE").unwrap();
        assert_eq!(expand_path("~/x"), PathBuf::from(format!("{home}/x")));
        assert_eq!(
            expand_path("%USERPROFILE%\\y"),
            PathBuf::from(format!("{home}\\y"))
        );
        assert_eq!(
            expand_path("%NO_EXISTE_RUKES%\\z"),
            PathBuf::from("%NO_EXISTE_RUKES%\\z")
        );
        assert_eq!(expand_path("50%"), PathBuf::from("50%"));
    }
}
