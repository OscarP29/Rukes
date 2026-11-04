//! Plantillas de `new script` (RF-135).

use std::io;
use std::path::{Path, PathBuf};

/// Plantilla de un lenguaje.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Template {
    /// Extensión del archivo.
    pub ext: &'static str,
    /// Nombre del lenguaje.
    pub label: &'static str,
    body: &'static str,
}

/// Plantillas disponibles, en el orden en que se ofrecen.
pub const TEMPLATES: &[Template] = &[
    Template {
        ext: "ps1",
        label: "PowerShell",
        body: include_str!("../templates/template.ps1"),
    },
    Template {
        ext: "py",
        label: "Python",
        body: include_str!("../templates/template.py"),
    },
    Template {
        ext: "js",
        label: "JavaScript (Node, Deno o Bun)",
        body: include_str!("../templates/template.js"),
    },
    Template {
        ext: "cmd",
        label: "Batch (cmd)",
        body: include_str!("../templates/template.cmd"),
    },
    Template {
        ext: "rhai",
        label: "Rhai (embebido, sin dependencias)",
        body: include_str!("../templates/template.rhai"),
    },
];

/// Plantilla por extensión.
pub fn by_ext(ext: &str) -> Option<&'static Template> {
    TEMPLATES.iter().find(|t| t.ext.eq_ignore_ascii_case(ext))
}

fn fold_char(c: char) -> char {
    match c {
        'á' | 'à' | 'ä' | 'â' | 'Á' | 'À' | 'Ä' | 'Â' => 'a',
        'é' | 'è' | 'ë' | 'ê' | 'É' | 'È' | 'Ë' | 'Ê' => 'e',
        'í' | 'ì' | 'ï' | 'î' | 'Í' | 'Ì' | 'Ï' | 'Î' => 'i',
        'ó' | 'ò' | 'ö' | 'ô' | 'Ó' | 'Ò' | 'Ö' | 'Ô' => 'o',
        'ú' | 'ù' | 'ü' | 'û' | 'Ú' | 'Ù' | 'Ü' | 'Û' => 'u',
        'ñ' | 'Ñ' => 'n',
        c => c.to_ascii_lowercase(),
    }
}

/// Palabras en minúsculas, sin tildes ni símbolos.
fn words(name: &str) -> Vec<String> {
    name.chars()
        .map(fold_char)
        .map(|c| if c.is_ascii_alphanumeric() { c } else { ' ' })
        .collect::<String>()
        .split_whitespace()
        .map(str::to_owned)
        .collect()
}

/// Nombre de archivo a partir del nombre del script: `Backup de fotos` → `backup-de-fotos`.
pub fn slug(name: &str) -> String {
    let w = words(name);
    if w.is_empty() {
        "nuevo-script".into()
    } else {
        w.join("-").chars().take(60).collect()
    }
}

/// Alias por defecto: las palabras del nombre (como mucho tres).
pub fn default_alias(name: &str) -> String {
    let w = words(name);
    if w.is_empty() {
        "nuevo script".into()
    } else {
        w.into_iter().take(3).collect::<Vec<_>>().join(" ")
    }
}

/// Texto de la plantilla con el nombre y el alias puestos. Los saltos de línea son `\r\n` en
/// los `.cmd` (cmd.exe los necesita).
pub fn render(t: &Template, name: &str, alias: &str) -> String {
    // El nombre solo va en las líneas de cabecera: se quitan los saltos de línea.
    let clean = |s: &str| s.replace(['\r', '\n'], " ");
    let text = t
        .body
        .replace("{{name}}", &clean(name))
        .replace("{{alias}}", &clean(alias));
    let text = text.replace("\r\n", "\n");
    if t.ext == "cmd" {
        text.replace('\n', "\r\n")
    } else {
        text
    }
}

/// Crea el archivo del script en `dir` con un nombre libre. Devuelve su ruta.
pub fn create(dir: &Path, t: &Template, name: &str) -> io::Result<PathBuf> {
    std::fs::create_dir_all(dir)?;
    let name = if name.trim().is_empty() {
        "Nuevo script"
    } else {
        name.trim()
    };
    let base = slug(name);
    let mut text = render(t, name, &default_alias(name));
    // Windows PowerShell 5.1 lee los .ps1 sin BOM como ANSI.
    if t.ext == "ps1" {
        text.insert(0, '\u{feff}');
    }
    for n in 1..1000 {
        let file = if n == 1 {
            format!("{base}.{}", t.ext)
        } else {
            format!("{base}-{n}.{}", t.ext)
        };
        let path = dir.join(file);
        match std::fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&path)
        {
            Ok(mut f) => {
                use std::io::Write as _;
                f.write_all(text.as_bytes())?;
                return Ok(path);
            }
            Err(e) if e.kind() == io::ErrorKind::AlreadyExists => {}
            Err(e) => return Err(e),
        }
    }
    Err(io::Error::other("no hay un nombre de archivo libre"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn slugs_and_aliases() {
        assert_eq!(slug("Backup de fotos"), "backup-de-fotos");
        assert_eq!(slug("¡Año nuevo! 2026"), "ano-nuevo-2026");
        assert_eq!(slug("  "), "nuevo-script");
        assert_eq!(default_alias("Crear issue en GitHub"), "crear issue en");
    }

    #[test]
    fn every_template_is_a_valid_script() {
        for t in TEMPLATES {
            let text = render(t, "Mi script", "mi script");
            let meta = crate::header::parse(&text).unwrap_or_else(|| panic!("{}", t.ext));
            assert_eq!(meta.name, "Mi script", "{}", t.ext);
            assert_eq!(meta.aliases, ["mi script"], "{}", t.ext);
            assert!(meta.warnings.is_empty(), "{}: {:?}", t.ext, meta.warnings);
            assert_eq!(meta.args.len(), 1, "{}", t.ext);
        }
        assert!(render(by_ext("cmd").unwrap(), "x", "x").contains("\r\n"));
    }

    #[test]
    fn powershell_template_runs() {
        let dir = std::env::temp_dir().join(format!("rukes-tpl-run-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        let file = create(&dir, by_ext("ps1").unwrap(), "Saludo").unwrap();
        let out = std::process::Command::new(crate::lang::powershell())
            .args([
                "-NoProfile",
                "-NonInteractive",
                "-ExecutionPolicy",
                "Bypass",
                "-File",
            ])
            .arg(&file)
            .arg("Ana")
            .env("RUKES_SCRIPT_NAME", "Saludo")
            .output()
            .unwrap();
        assert_eq!(
            String::from_utf8_lossy(&out.stdout).trim(),
            "Hola desde Saludo: Ana"
        );
        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn create_picks_a_free_name() {
        let dir = std::env::temp_dir().join(format!("rukes-tpl-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        let t = by_ext("py").unwrap();
        let a = create(&dir, t, "Hola").unwrap();
        let b = create(&dir, t, "Hola").unwrap();
        assert_eq!(a.file_name().unwrap(), "hola.py");
        assert_eq!(b.file_name().unwrap(), "hola-2.py");
        let ps = create(&dir, by_ext("ps1").unwrap(), "").unwrap();
        assert!(std::fs::read(&ps).unwrap().starts_with(b"\xEF\xBB\xBF"));
        let _ = std::fs::remove_dir_all(dir);
    }
}
