//! Exportar e importar la configuración (RF-204) y esquema JSON de `config.toml` (RF-202).

use std::io;
use std::path::{Path, PathBuf};

use crate::paths::Paths;

/// Nombre del esquema junto a `config.toml`.
pub const SCHEMA_FILE: &str = "config.schema.json";

/// Carpetas que no se copian: estado, cachés y logs (en el modo portable están dentro).
const SKIP: &[&str] = &["state", "cache", "logs"];

/// Esquema JSON de `config.toml` para autocompletado en editores (Taplo / Even Better TOML).
pub fn json_schema() -> String {
    let schema = schemars::schema_for!(crate::Config);
    serde_json::to_string_pretty(&schema).unwrap_or_default()
}

/// Escribe `config.schema.json` junto a `config.toml` si falta o está desfasado.
pub fn write_schema(paths: &Paths) -> io::Result<PathBuf> {
    let path = paths.config_dir.join(SCHEMA_FILE);
    let text = json_schema();
    if std::fs::read_to_string(&path).ok().as_deref() != Some(text.as_str()) {
        std::fs::create_dir_all(&paths.config_dir)?;
        std::fs::write(&path, text)?;
    }
    Ok(path)
}

fn copy_tree(from: &Path, to: &Path, skip_root: bool) -> io::Result<usize> {
    std::fs::create_dir_all(to)?;
    let mut n = 0;
    for e in std::fs::read_dir(from)? {
        let e = e?;
        let name = e.file_name();
        if skip_root
            && SKIP
                .iter()
                .any(|s| name.to_string_lossy().eq_ignore_ascii_case(s))
        {
            continue;
        }
        let (src, dst) = (e.path(), to.join(&name));
        let t = e.file_type()?;
        if t.is_dir() {
            n += copy_tree(&src, &dst, false)?;
        } else if t.is_file() {
            std::fs::copy(&src, &dst)?;
            n += 1;
        }
    }
    Ok(n)
}

/// Copia la configuración (config.toml, temas, scripts, snippets, workflows, notas, plugins,
/// idiomas) a una carpeta nueva `Rukes-config-<fecha>` dentro de `dest`. Devuelve la carpeta.
///
/// Renombrada a `data` junto a `rukes.exe`, sirve como instalación portable.
pub fn export(paths: &Paths, dest: &Path, stamp: &str) -> io::Result<PathBuf> {
    let out = dest.join(format!("Rukes-config-{stamp}"));
    if out.exists() {
        return Err(io::Error::new(
            io::ErrorKind::AlreadyExists,
            out.display().to_string(),
        ));
    }
    let n = copy_tree(&paths.config_dir, &out, true)?;
    tracing::info!(files = n, dest = %out.display(), "configuración exportada");
    Ok(out)
}

/// Importa una configuración exportada (o la carpeta `data` de un Rukes portable): comprueba
/// que su `config.toml` es válido, guarda una copia de la actual en
/// `Rukes-backup-<fecha>` junto a la carpeta de configuración y copia encima los archivos.
/// Devuelve la carpeta de la copia de seguridad.
pub fn import(paths: &Paths, src: &Path, stamp: &str) -> Result<PathBuf, String> {
    let cfg = src.join("config.toml");
    if !cfg.is_file() {
        return Err(format!("{} no tiene config.toml", src.display()));
    }
    crate::load(&cfg).map_err(|e| e.to_string())?;
    if same_dir(src, &paths.config_dir) {
        return Err("es la misma carpeta que la configuración actual".into());
    }
    let parent = paths
        .config_dir
        .parent()
        .map_or_else(|| paths.config_dir.clone(), Path::to_path_buf);
    let backup = parent.join(format!("Rukes-backup-{stamp}"));
    if paths.config_dir.is_dir() {
        copy_tree(&paths.config_dir, &backup, true).map_err(|e| e.to_string())?;
    }
    let n = copy_tree(src, &paths.config_dir, true).map_err(|e| e.to_string())?;
    tracing::info!(files = n, from = %src.display(), backup = %backup.display(), "configuración importada");
    Ok(backup)
}

fn same_dir(a: &Path, b: &Path) -> bool {
    match (a.canonicalize(), b.canonicalize()) {
        (Ok(x), Ok(y)) => x == y,
        _ => false,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tmp(tag: &str) -> PathBuf {
        let d = std::env::temp_dir().join(format!("rukes-transfer-{tag}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&d);
        std::fs::create_dir_all(&d).unwrap();
        d
    }

    #[test]
    fn export_then_import_roundtrip_with_backup() {
        let root = tmp("rt");
        let a = Paths::under(&root.join("a"), true);
        std::fs::create_dir_all(a.config_dir.join("scripts")).unwrap();
        std::fs::write(a.config_file(), "[general]\nhotkey = \"Alt+R\"\n").unwrap();
        std::fs::write(a.config_dir.join("scripts/hola.ps1"), "echo hola").unwrap();
        std::fs::create_dir_all(&a.cache_dir).unwrap();
        std::fs::write(a.cache_dir.join("apps.json"), "{}").unwrap();

        let out = export(&a, &root, "1").unwrap();
        assert!(out.join("scripts/hola.ps1").is_file());
        assert!(!out.join("cache").exists(), "la caché no se exporta");
        assert!(export(&a, &root, "1").is_err(), "no sobrescribe");

        let b = Paths::under(&root.join("b"), true);
        std::fs::create_dir_all(&b.config_dir).unwrap();
        std::fs::write(b.config_file(), "[general]\nhotkey = \"Alt+B\"\n").unwrap();
        let backup = import(&b, &out, "2").unwrap();
        assert!(
            std::fs::read_to_string(backup.join("config.toml"))
                .unwrap()
                .contains("Alt+B")
        );
        assert!(
            std::fs::read_to_string(b.config_file())
                .unwrap()
                .contains("Alt+R")
        );
        assert!(b.config_dir.join("scripts/hola.ps1").is_file());
        let _ = std::fs::remove_dir_all(root);
    }

    #[test]
    fn import_rejects_invalid_config() {
        let root = tmp("bad");
        let src = root.join("src");
        std::fs::create_dir_all(&src).unwrap();
        std::fs::write(src.join("config.toml"), "[general\n").unwrap();
        let p = Paths::under(&root.join("dst"), true);
        assert!(import(&p, &src, "x").is_err());
        assert!(import(&p, &root.join("nada"), "x").is_err());
        let _ = std::fs::remove_dir_all(root);
    }

    #[test]
    fn schema_describes_the_config() {
        let s = json_schema();
        assert!(s.contains("\"hotkey\""));
        assert!(s.contains("\"wall\""));
        assert!(
            s.contains("Atajo") || s.contains("atajo"),
            "usa los comentarios de doc"
        );
    }
}
