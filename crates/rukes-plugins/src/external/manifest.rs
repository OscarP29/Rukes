//! Manifiesto `plugin.toml` de un plugin externo (RF-171).

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use serde::Deserialize;

/// Nombre del manifiesto dentro de la carpeta del plugin.
pub const MANIFEST: &str = "plugin.toml";

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct File {
    plugin: Section,
    #[serde(default)]
    permissions: BTreeMap<String, toml::Value>,
    #[serde(default)]
    config: toml::Table,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct Section {
    name: String,
    version: String,
    executable: String,
    #[serde(default)]
    args: Vec<String>,
    #[serde(default)]
    description: Option<String>,
    #[serde(default)]
    author: Option<String>,
    #[serde(default)]
    keywords: Vec<String>,
    #[serde(default)]
    prefix: Option<String>,
    #[serde(default)]
    global: bool,
    #[serde(default)]
    priority: f32,
    #[serde(default)]
    icon: Option<String>,
    #[serde(default)]
    enabled: Option<bool>,
}

/// Manifiesto leído.
#[derive(Debug, Clone)]
pub struct Manifest {
    /// Carpeta del plugin.
    pub dir: PathBuf,
    /// Id (`ext:<carpeta>`).
    pub id: String,
    /// Nombre visible.
    pub name: String,
    /// Versión.
    pub version: String,
    /// Programa (relativo a la carpeta o del `PATH`).
    pub executable: String,
    /// Argumentos.
    pub args: Vec<String>,
    /// Descripción.
    pub description: Option<String>,
    /// Autor.
    pub author: Option<String>,
    /// Palabras clave (en minúsculas).
    pub keywords: Vec<String>,
    /// Prefijo.
    pub prefix: Option<String>,
    /// Participa en las búsquedas sin palabra clave.
    pub global: bool,
    /// Prioridad (-1..1).
    pub priority: f32,
    /// Icono por defecto de sus resultados.
    pub icon: Option<String>,
    /// `enabled = false` lo ignora.
    pub enabled: bool,
    /// Permisos declarados (informativos en v1.0): `nombre → valor`.
    pub permissions: Vec<(String, String)>,
    /// Tabla `[config]`, que recibe en `initialize`.
    pub config: serde_json::Value,
}

fn show(v: &toml::Value) -> String {
    match v {
        toml::Value::String(s) => s.clone(),
        toml::Value::Array(a) => a.iter().map(show).collect::<Vec<_>>().join(", "),
        other => other.to_string(),
    }
}

impl Manifest {
    /// Lee el manifiesto de `dir`.
    pub fn load(dir: &Path) -> Result<Self, String> {
        let path = dir.join(MANIFEST);
        let text = std::fs::read_to_string(&path).map_err(|e| e.to_string())?;
        Self::parse(dir, &text)
    }

    /// Interpreta el texto de un manifiesto.
    pub fn parse(dir: &Path, text: &str) -> Result<Self, String> {
        let f: File = toml::from_str(text).map_err(|e| e.to_string())?;
        let p = f.plugin;
        if p.name.trim().is_empty() || p.executable.trim().is_empty() {
            return Err("`name` y `executable` no pueden estar vacíos".into());
        }
        if p.keywords.is_empty() && p.prefix.is_none() && !p.global {
            return Err("define `keywords`, `prefix` o `global = true`".into());
        }
        let folder = dir
            .file_name()
            .map(|n| n.to_string_lossy().to_lowercase())
            .unwrap_or_default();
        let permissions = f
            .permissions
            .iter()
            .filter(|(_, v)| !matches!(v, toml::Value::Boolean(false)))
            .map(|(k, v)| {
                let v = if matches!(v, toml::Value::Boolean(true)) {
                    String::new()
                } else {
                    show(v)
                };
                (k.clone(), v)
            })
            .collect();
        let config = serde_json::to_value(&f.config).unwrap_or(serde_json::Value::Null);
        Ok(Self {
            dir: dir.to_owned(),
            id: format!("ext:{folder}"),
            name: p.name.trim().to_owned(),
            version: p.version,
            executable: p.executable,
            args: p.args,
            description: p.description,
            author: p.author,
            keywords: p
                .keywords
                .iter()
                .map(|k| k.trim().to_lowercase())
                .filter(|k| !k.is_empty())
                .collect(),
            prefix: p.prefix.filter(|s| !s.trim().is_empty()),
            global: p.global,
            priority: p.priority.clamp(-1.0, 1.0),
            icon: p.icon,
            enabled: p.enabled.unwrap_or(true),
            permissions,
            config,
        })
    }

    /// Permisos en una línea (`network · filesystem: read`).
    pub fn permissions_label(&self) -> String {
        self.permissions
            .iter()
            .map(|(k, v)| {
                if v.is_empty() {
                    k.clone()
                } else {
                    format!("{k}: {v}")
                }
            })
            .collect::<Vec<_>>()
            .join(" · ")
    }
}

/// Lee todos los plugins de `root` (`<root>/<carpeta>/plugin.toml`), por nombre de carpeta.
/// Devuelve los manifiestos válidos y los errores (`carpeta: error`).
pub fn discover(root: &Path) -> (Vec<Manifest>, Vec<String>) {
    let mut dirs: Vec<PathBuf> = std::fs::read_dir(root)
        .into_iter()
        .flatten()
        .flatten()
        .map(|e| e.path())
        .filter(|p| p.join(MANIFEST).is_file())
        .collect();
    dirs.sort();
    let (mut ok, mut errors) = (Vec::new(), Vec::new());
    for d in dirs {
        match Manifest::load(&d) {
            Ok(m) => ok.push(m),
            Err(e) => errors.push(format!("{}: {e}", d.join(MANIFEST).display())),
        }
    }
    (ok, errors)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_manifest() {
        let m = Manifest::parse(
            Path::new("C:/x/Cuenta"),
            r#"
[plugin]
name = "Cuenta"
version = "1.0.0"
executable = "python"
args = ["main.py"]
keywords = ["Cuenta", "count"]

[permissions]
network = false
clipboard = true
filesystem = "read"

[config]
saludo = "hola"
"#,
        )
        .unwrap();
        assert_eq!(m.id, "ext:cuenta");
        assert_eq!(m.keywords, ["cuenta", "count"]);
        assert_eq!(m.permissions_label(), "clipboard · filesystem: read");
        assert_eq!(m.config["saludo"], "hola");
        assert!(m.enabled && !m.global);
    }

    #[test]
    fn rejects_bad_manifests() {
        let d = Path::new("x");
        assert!(Manifest::parse(d, "[plugin]\nname = \"a\"\n").is_err());
        assert!(
            Manifest::parse(
                d,
                "[plugin]\nname = \"a\"\nversion = \"1\"\nexecutable = \"a.exe\"\n"
            )
            .is_err(),
            "sin keywords ni prefijo"
        );
        assert!(
            Manifest::parse(
                d,
                "[plugin]\nname = \"a\"\nversion = \"1\"\nexecutable = \"a.exe\"\nprefix = \"%\"\nfoo = 1\n"
            )
            .is_err(),
            "campo desconocido"
        );
    }
}
