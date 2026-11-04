//! Cabecera de metadatos `@rukes.*` en los comentarios del script (RF-132).
//!
//! ```powershell
//! # @rukes.name        Crear issue en GitHub
//! # @rukes.description Abre un issue nuevo en el repo actual
//! # @rukes.alias       gh issue
//! # @rukes.arg         title:text:required
//! # @rukes.output      notification
//! # @rukes.timeout     15s
//! ```
//!
//! Cada línea puede ir tras cualquier marca de comentario (`#`, `//`, `--`, `;`, `::`, `REM`,
//! `'`, o dentro de un bloque `<# … #>` / `/* … */`). Solo se leen las primeras líneas del
//! archivo ([`HEADER_BYTES`]).

use std::time::Duration;

/// Bytes del principio del archivo en los que se busca la cabecera.
pub const HEADER_BYTES: usize = 8 * 1024;

/// Qué hacer con la salida del script (RF-134).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum OutputMode {
    /// No mostrar nada (solo los errores se notifican).
    Silent,
    /// Notificación del sistema con la última línea de la salida.
    #[default]
    Notification,
    /// Mostrar la salida (o los resultados JSON) en el launcher.
    Launcher,
    /// Copiar la salida al portapapeles.
    Clipboard,
    /// Ejecutar en una terminal visible.
    Terminal,
}

impl OutputMode {
    /// Interpreta el valor de `@rukes.output` (en inglés o español).
    pub fn parse(s: &str) -> Option<Self> {
        Some(match s.trim().to_ascii_lowercase().as_str() {
            "silent" | "silencioso" | "none" | "quiet" => Self::Silent,
            "notification" | "notificacion" | "notificación" | "notify" | "toast" => {
                Self::Notification
            }
            "launcher" | "inline" | "list" | "lista" | "results" | "resultados" => Self::Launcher,
            "clipboard" | "portapapeles" | "copy" | "copiar" => Self::Clipboard,
            "terminal" | "console" | "consola" | "visible" => Self::Terminal,
            _ => return None,
        })
    }
}

/// Tipo de un argumento (RF-133).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ArgKind {
    /// Texto libre.
    Text,
    /// Número.
    Number,
    /// Una opción de una lista.
    Choice(Vec<String>),
    /// Ruta a un archivo o carpeta existente.
    File,
    /// Sí/no.
    Bool,
}

impl ArgKind {
    /// Nombre corto para mostrar.
    pub fn label(&self) -> &'static str {
        match self {
            Self::Text => "text",
            Self::Number => "number",
            Self::Choice(_) => "option",
            Self::File => "file",
            Self::Bool => "bool",
        }
    }
}

/// Argumento declarado con `@rukes.arg`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ArgSpec {
    /// Nombre (minúsculas, sin espacios).
    pub name: String,
    /// Tipo.
    pub kind: ArgKind,
    /// Obligatorio.
    pub required: bool,
    /// Valor por defecto.
    pub default: Option<String>,
    /// Descripción para mostrar.
    pub description: Option<String>,
}

/// Metadatos de un script.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct Meta {
    /// Nombre visible.
    pub name: String,
    /// Descripción.
    pub description: String,
    /// Icono: ruta (relativa a la carpeta del script) o nombre.
    pub icon: Option<String>,
    /// Alias que lo invocan con argumentos en línea (`gh issue Mi bug`).
    pub aliases: Vec<String>,
    /// Argumentos.
    pub args: Vec<ArgSpec>,
    /// Modo de salida.
    pub output: OutputMode,
    /// Tiempo máximo (`None` = el de la config; `Some(ZERO)` = sin límite).
    pub timeout: Option<Duration>,
    /// Atajo global propio (se registra en la fase 6, RF-162).
    pub hotkey: Option<String>,
    /// Ejecutar como administrador.
    pub admin: bool,
    /// Directorio de trabajo (por defecto, la carpeta del script).
    pub cwd: Option<String>,
    /// *Script filter*: se ejecuta mientras se escribe y devuelve resultados JSON (RF-140).
    pub filter: bool,
    /// Intérprete propio (sustituye al de la extensión).
    pub interpreter: Option<String>,
    /// Problemas de la cabecera (claves desconocidas, valores no válidos).
    pub warnings: Vec<String>,
}

/// Interpreta una duración: `15s`, `500ms`, `2m`, `1h`, `1.5s` o un número de segundos.
pub fn parse_duration(s: &str) -> Option<Duration> {
    let s = s.trim().to_ascii_lowercase().replace(',', ".");
    let (num, mult) = if let Some(n) = s.strip_suffix("ms") {
        (n, 0.001)
    } else if let Some(n) = s.strip_suffix('s') {
        (n, 1.0)
    } else if let Some(n) = s.strip_suffix("min") {
        (n, 60.0)
    } else if let Some(n) = s.strip_suffix('m') {
        (n, 60.0)
    } else if let Some(n) = s.strip_suffix('h') {
        (n, 3600.0)
    } else {
        (s.as_str(), 1.0)
    };
    let v: f64 = num.trim().parse().ok()?;
    (v.is_finite() && v >= 0.0).then(|| Duration::from_secs_f64(v * mult))
}

fn parse_bool(s: &str) -> Option<bool> {
    match s.trim().to_lowercase().as_str() {
        "" | "true" | "yes" | "si" | "sí" | "1" | "on" => Some(true),
        "false" | "no" | "0" | "off" => Some(false),
        _ => None,
    }
}

/// Texto de la línea a partir de `@rukes.` si lo que hay antes es solo una marca de comentario.
fn header_part(line: &str) -> Option<&str> {
    let idx = line.find("@rukes.")?;
    let before = line[..idx].trim().trim_start_matches('@');
    let before = before
        .strip_prefix("REM")
        .or_else(|| before.strip_prefix("rem"))
        .or_else(|| before.strip_prefix("Rem"))
        .unwrap_or(before);
    before
        .chars()
        .all(|c| c.is_whitespace() || "#/-;:*<'!%\"".contains(c))
        .then(|| &line[idx + "@rukes.".len()..])
}

fn parse_arg(spec: &str) -> Result<ArgSpec, String> {
    const FLAGS: [&str; 5] = ["required", "optional", "default=", "options=", "desc="];
    let is_flag = |seg: &str| {
        let l = seg.trim().to_ascii_lowercase();
        FLAGS.iter().any(|f| {
            if f.ends_with('=') {
                l.starts_with(f)
            } else {
                l == *f
            }
        })
    };
    let segs: Vec<&str> = spec.split(':').collect();
    let name = segs.first().map_or("", |s| s.trim()).to_lowercase();
    if name.is_empty()
        || !name
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || c == '_' || c == '-')
    {
        return Err(format!("nombre de argumento no válido: «{spec}»"));
    }
    let ty = segs.get(1).map_or("text", |s| s.trim());
    let ty_lower = ty.to_ascii_lowercase();
    let mut options: Option<Vec<String>> = None;
    let mut kind = match ty_lower.as_str() {
        "" | "text" | "string" | "texto" => ArgKind::Text,
        "number" | "num" | "int" | "float" | "numero" | "número" => ArgKind::Number,
        "file" | "path" | "archivo" | "ruta" => ArgKind::File,
        "bool" | "boolean" | "booleano" => ArgKind::Bool,
        "option" | "choice" | "opcion" | "opción" => ArgKind::Choice(Vec::new()),
        t if t.starts_with("option(") || t.starts_with("choice(") || t.starts_with("opcion(") => {
            let inner = ty[ty.find('(').unwrap_or(0) + 1..].trim_end_matches(')');
            options = Some(split_options(inner));
            ArgKind::Choice(Vec::new())
        }
        _ => return Err(format!("tipo de argumento desconocido: «{ty}»")),
    };
    let mut required = false;
    let mut default = None;
    let mut description = None;
    let mut i = 2;
    while i < segs.len() {
        let seg = segs[i].trim();
        let lower = seg.to_ascii_lowercase();
        // `default=` y `desc=` pueden contener `:` (rutas): se unen los segmentos que siguen
        // hasta la próxima marca conocida.
        let mut take_rest = |prefix_len: usize| {
            let mut v = seg[prefix_len..].to_owned();
            while i + 1 < segs.len() && !is_flag(segs[i + 1]) {
                i += 1;
                v.push(':');
                v.push_str(segs[i]);
            }
            v.trim().to_owned()
        };
        if lower == "required" {
            required = true;
        } else if lower == "optional" {
            required = false;
        } else if lower.starts_with("default=") {
            default = Some(take_rest("default=".len()));
        } else if lower.starts_with("desc=") {
            description = Some(take_rest("desc=".len()));
        } else if lower.starts_with("options=") {
            options = Some(split_options(&seg["options=".len()..]));
        } else if !seg.is_empty() {
            return Err(format!("opción de argumento desconocida: «{seg}»"));
        }
        i += 1;
    }
    if let ArgKind::Choice(list) = &mut kind {
        *list = options.unwrap_or_default();
        if list.is_empty() {
            return Err(format!(
                "el argumento «{name}» es de tipo opción pero no tiene opciones"
            ));
        }
    }
    Ok(ArgSpec {
        name,
        kind,
        required,
        default,
        description,
    })
}

fn split_options(s: &str) -> Vec<String> {
    s.split(['|', ','])
        .map(str::trim)
        .filter(|o| !o.is_empty())
        .map(str::to_owned)
        .collect()
}

/// Lee la cabecera. Devuelve `None` si el texto no tiene ninguna línea `@rukes.*` (no es un
/// script de Rukes). Si falta `@rukes.name`, el nombre queda vacío y lo rellena el llamador.
pub fn parse(text: &str) -> Option<Meta> {
    let mut meta = Meta::default();
    let mut found = false;
    let text = text.trim_start_matches('\u{feff}');
    for line in text.lines().take(200) {
        let Some(part) = header_part(line) else {
            continue;
        };
        found = true;
        let part = part
            .trim_end()
            .trim_end_matches("#>")
            .trim_end_matches("*/");
        let (key, value) = match part.find(char::is_whitespace) {
            Some(i) => (&part[..i], part[i..].trim()),
            None => (part, ""),
        };
        let key = key.to_ascii_lowercase();
        match key.as_str() {
            "name" | "title" => meta.name = value.to_owned(),
            "description" | "desc" => meta.description = value.to_owned(),
            "icon" => meta.icon = (!value.is_empty()).then(|| value.to_owned()),
            "alias" | "aliases" | "keyword" => {
                for a in value.split(',') {
                    let a = a.split_whitespace().collect::<Vec<_>>().join(" ");
                    if !a.is_empty() && !meta.aliases.iter().any(|x| x.eq_ignore_ascii_case(&a)) {
                        meta.aliases.push(a);
                    }
                }
            }
            "arg" | "argument" => match parse_arg(value) {
                Ok(a) if meta.args.iter().any(|x| x.name == a.name) => meta
                    .warnings
                    .push(format!("argumento repetido: «{}»", a.name)),
                Ok(a) => meta.args.push(a),
                Err(e) => meta.warnings.push(e),
            },
            "output" | "mode" => match OutputMode::parse(value) {
                Some(m) => meta.output = m,
                None if value.eq_ignore_ascii_case("filter") => meta.filter = true,
                None => meta
                    .warnings
                    .push(format!("modo de salida desconocido: «{value}»")),
            },
            "timeout" => match parse_duration(value) {
                Some(d) => meta.timeout = Some(d),
                None => meta.warnings.push(format!("timeout no válido: «{value}»")),
            },
            "hotkey" | "shortcut" => meta.hotkey = (!value.is_empty()).then(|| value.to_owned()),
            "admin" | "requires_admin" | "elevated" => match parse_bool(value) {
                Some(b) => meta.admin = b,
                None => meta.warnings.push(format!("admin no válido: «{value}»")),
            },
            "cwd" | "workdir" | "directory" => {
                meta.cwd = (!value.is_empty()).then(|| value.to_owned());
            }
            "filter" => match parse_bool(value) {
                Some(b) => meta.filter = b,
                None => meta.warnings.push(format!("filter no válido: «{value}»")),
            },
            "interpreter" => meta.interpreter = (!value.is_empty()).then(|| value.to_owned()),
            other => meta
                .warnings
                .push(format!("clave desconocida: «@rukes.{other}»")),
        }
    }
    found.then_some(meta)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn spec_example_header() {
        let m = parse(
            "# @rukes.name        Crear issue en GitHub\n\
             # @rukes.description Abre un issue nuevo en el repo actual\n\
             # @rukes.icon        github\n\
             # @rukes.alias       gh issue\n\
             # @rukes.arg         title:text:required\n\
             # @rukes.output      notification\n\
             # @rukes.timeout     15s\n\
             param($title)\n",
        )
        .unwrap();
        assert_eq!(m.name, "Crear issue en GitHub");
        assert_eq!(m.description, "Abre un issue nuevo en el repo actual");
        assert_eq!(m.icon.as_deref(), Some("github"));
        assert_eq!(m.aliases, ["gh issue"]);
        assert_eq!(m.args.len(), 1);
        assert_eq!(m.args[0].name, "title");
        assert_eq!(m.args[0].kind, ArgKind::Text);
        assert!(m.args[0].required);
        assert_eq!(m.output, OutputMode::Notification);
        assert_eq!(m.timeout, Some(Duration::from_secs(15)));
        assert!(m.warnings.is_empty(), "{:?}", m.warnings);
    }

    #[test]
    fn comment_styles() {
        for (src, name) in [
            ("// @rukes.name JS", "JS"),
            ("-- @rukes.name Lua", "Lua"),
            (":: @rukes.name Bat", "Bat"),
            ("REM @rukes.name Rem", "Rem"),
            ("@REM @rukes.name AtRem", "AtRem"),
            ("<#\n  @rukes.name Bloque\n#>", "Bloque"),
            ("/* @rukes.name C */", "C"),
            ("  # @rukes.name   Espacios  ", "Espacios"),
        ] {
            assert_eq!(parse(src).map(|m| m.name).as_deref(), Some(name), "{src}");
        }
        // Una cadena del código que contiene @rukes. no cuenta.
        assert!(parse("echo \"x\" @rukes.name Falso").is_none());
        assert!(parse("Write-Host 'hola'").is_none());
    }

    #[test]
    fn args_with_types_defaults_and_options() {
        let m = parse(
            "# @rukes.arg count:number:default=3\n\
             # @rukes.arg env:option(dev|prod):required\n\
             # @rukes.arg mode:choice:options=a,b,c:desc=Modo de trabajo\n\
             # @rukes.arg target:file:default=C:\\temp\\x.txt:required\n\
             # @rukes.arg force:bool\n\
             # @rukes.arg Bad Name:text\n\
             # @rukes.arg x:color\n",
        )
        .unwrap();
        assert_eq!(m.args.len(), 5);
        assert_eq!(m.args[0].kind, ArgKind::Number);
        assert_eq!(m.args[0].default.as_deref(), Some("3"));
        assert_eq!(
            m.args[1].kind,
            ArgKind::Choice(vec!["dev".into(), "prod".into()])
        );
        assert!(m.args[1].required);
        assert_eq!(
            m.args[2].kind,
            ArgKind::Choice(vec!["a".into(), "b".into(), "c".into()])
        );
        assert_eq!(m.args[2].description.as_deref(), Some("Modo de trabajo"));
        assert_eq!(m.args[3].default.as_deref(), Some("C:\\temp\\x.txt"));
        assert!(m.args[3].required);
        assert_eq!(m.args[4].kind, ArgKind::Bool);
        assert_eq!(m.warnings.len(), 2);
    }

    #[test]
    fn misc_keys() {
        let m = parse(
            "# @rukes.alias a, b c ,A\n# @rukes.output portapapeles\n# @rukes.admin\n\
             # @rukes.cwd ~/x\n# @rukes.filter true\n# @rukes.timeout 0\n# @rukes.foo 1\n\
             # @rukes.output filter\n",
        )
        .unwrap();
        assert_eq!(m.aliases, ["a", "b c"]);
        assert_eq!(m.output, OutputMode::Clipboard);
        assert!(m.admin);
        assert_eq!(m.cwd.as_deref(), Some("~/x"));
        assert!(m.filter);
        assert_eq!(m.timeout, Some(Duration::ZERO));
        assert_eq!(m.warnings, ["clave desconocida: «@rukes.foo»"]);
    }

    #[test]
    fn durations() {
        assert_eq!(parse_duration("500ms"), Some(Duration::from_millis(500)));
        assert_eq!(parse_duration("2m"), Some(Duration::from_secs(120)));
        assert_eq!(parse_duration("1,5s"), Some(Duration::from_millis(1500)));
        assert_eq!(parse_duration("10"), Some(Duration::from_secs(10)));
        assert_eq!(parse_duration("1h"), Some(Duration::from_secs(3600)));
        assert_eq!(parse_duration("-1s"), None);
        assert_eq!(parse_duration("rápido"), None);
    }
}
