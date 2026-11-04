//! Textos de la interfaz en archivos de recursos (RF-210, RF-211).
//!
//! Los idiomas incluidos (`lang/es.toml`, `lang/en.toml`) van dentro del binario. Un archivo
//! `<código>.toml` en la carpeta `lang` de la configuración cambia textos de un idioma incluido o
//! añade uno nuevo (p. ej. `fr.toml`) sin recompilar; las claves que falten salen en inglés.
//!
//! ```toml
//! [search]
//! placeholder = "Rechercher…"
//! ```

use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU8, Ordering};
use std::sync::{Mutex, PoisonError, RwLock};

/// Idioma para la lógica que depende del idioma (calculadora, fechas): español o el resto.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Lang {
    /// Español.
    Es,
    /// Inglés (y cualquier otro idioma añadido por el usuario).
    En,
}

/// Idiomas incluidos: `(código, TOML)`.
pub const BUILTIN: &[(&str, &str)] = &[
    ("es", include_str!("../lang/es.toml")),
    ("en", include_str!("../lang/en.toml")),
];

static LANG: AtomicU8 = AtomicU8::new(0);
static TABLE: RwLock<Option<HashMap<String, &'static str>>> = RwLock::new(None);
static CODE: RwLock<String> = RwLock::new(String::new());
static DIR: RwLock<Option<PathBuf>> = RwLock::new(None);
static INTERN: Mutex<Option<HashSet<&'static str>>> = Mutex::new(None);

/// Cadena con vida estática sin duplicar las que ya existen (las tablas se recargan al
/// guardar un archivo de idioma; sin esto cada recarga gastaría memoria otra vez).
fn intern(s: &str) -> &'static str {
    let mut set = INTERN.lock().unwrap_or_else(PoisonError::into_inner);
    let set = set.get_or_insert_with(HashSet::new);
    if let Some(v) = set.get(s) {
        return v;
    }
    let v: &'static str = Box::leak(s.to_owned().into_boxed_str());
    set.insert(v);
    v
}

/// Aplana `[a] b = "x"` en `a.b → x`.
fn flatten(prefix: &str, t: &toml::Table, out: &mut HashMap<String, String>) {
    for (k, v) in t {
        let key = if prefix.is_empty() {
            k.clone()
        } else {
            format!("{prefix}.{k}")
        };
        match v {
            toml::Value::Table(t) => flatten(&key, t, out),
            toml::Value::String(s) => {
                out.insert(key, s.clone());
            }
            _ => {}
        }
    }
}

/// Lee un archivo de idioma en `clave → texto`.
pub fn parse(text: &str) -> Result<HashMap<String, String>, String> {
    let t: toml::Table = toml::from_str(text).map_err(|e| e.to_string())?;
    let mut out = HashMap::new();
    flatten("", &t, &mut out);
    Ok(out)
}

fn builtin(code: &str) -> Option<HashMap<String, String>> {
    BUILTIN
        .iter()
        .find(|(c, _)| *c == code)
        .and_then(|(_, t)| parse(t).ok())
}

/// Carpeta de idiomas del usuario (`<config>/lang`).
pub fn set_lang_dir(dir: Option<&Path>) {
    *DIR.write().unwrap_or_else(PoisonError::into_inner) = dir.map(Path::to_path_buf);
}

/// Código del idioma elegido: `auto` usa el del sistema (`es-CO` → `es`).
fn resolve(config: &str, sys_lang: &str, dir: Option<&Path>) -> String {
    let config = config.trim().to_ascii_lowercase();
    if config != "auto" && !config.is_empty() {
        return config;
    }
    let sys = sys_lang.trim().to_ascii_lowercase();
    let exists = |c: &str| {
        BUILTIN.iter().any(|(b, _)| *b == c)
            || dir.is_some_and(|d| d.join(format!("{c}.toml")).is_file())
    };
    if !sys.is_empty() && exists(&sys) {
        return sys;
    }
    let base = sys.split(['-', '_']).next().unwrap_or("");
    if !base.is_empty() && exists(base) {
        base.to_owned()
    } else {
        "en".to_owned()
    }
}

/// Fija el idioma (`auto`, `es`, `en` o el código de un archivo de la carpeta `lang`) y carga
/// sus textos. Devuelve un error legible si el archivo del usuario no es válido (se usan
/// entonces los textos incluidos).
pub fn set_language(config: &str, sys_lang: &str) -> Result<(), String> {
    let dir = DIR.read().unwrap_or_else(PoisonError::into_inner).clone();
    let code = resolve(config, sys_lang, dir.as_deref());
    let base = code.split(['-', '_']).next().unwrap_or("en").to_owned();
    let mut merged = builtin("en").unwrap_or_default();
    if base != "en"
        && let Some(t) = builtin(&base)
    {
        merged.extend(t);
    }
    let mut result = Ok(());
    if let Some(d) = &dir {
        let mut files = vec![d.join(format!("{base}.toml"))];
        if code != base {
            files.push(d.join(format!("{code}.toml")));
        }
        for f in files {
            match std::fs::read_to_string(&f) {
                Ok(text) => match parse(&text) {
                    Ok(t) => merged.extend(t),
                    Err(e) => result = Err(format!("{}: {}", f.display(), e.trim())),
                },
                Err(e) if e.kind() == std::io::ErrorKind::NotFound => {}
                Err(e) => result = Err(format!("{}: {e}", f.display())),
            }
        }
    }
    let table = merged.into_iter().map(|(k, v)| (k, intern(&v))).collect();
    *TABLE.write().unwrap_or_else(PoisonError::into_inner) = Some(table);
    let lang = if base == "es" { Lang::Es } else { Lang::En };
    LANG.store(lang as u8, Ordering::Relaxed);
    *CODE.write().unwrap_or_else(PoisonError::into_inner) = code;
    result
}

/// Idioma actual para la lógica que depende de él.
pub fn language() -> Lang {
    if LANG.load(Ordering::Relaxed) == Lang::En as u8 {
        Lang::En
    } else {
        Lang::Es
    }
}

/// Código del idioma actual (`es`, `en`, `fr`…).
pub fn code() -> String {
    let c = CODE.read().unwrap_or_else(PoisonError::into_inner).clone();
    if c.is_empty() { "es".into() } else { c }
}

/// Traduce una clave. Si no existe, devuelve la propia clave (así las etiquetas que ya son
/// texto, como las de los scripts, pasan tal cual).
pub fn tr(key: &str) -> &'static str {
    {
        let t = TABLE.read().unwrap_or_else(PoisonError::into_inner);
        if let Some(v) = t.as_ref().and_then(|t| t.get(key)) {
            return v;
        }
        if t.is_none() {
            drop(t);
            // Sin inicializar (pruebas, arranque): español.
            let _ = set_language("es", "");
            return tr(key);
        }
    }
    intern(key)
}

/// Traduce y sustituye `{nombre}` por cada valor.
pub fn trf(key: &str, args: &[(&str, &dyn std::fmt::Display)]) -> String {
    let mut s = tr(key).to_owned();
    for (k, v) in args {
        s = s.replace(&format!("{{{k}}}"), &v.to_string());
    }
    s
}

#[cfg(test)]
mod tests {
    use super::*;

    fn keys(code: &str) -> std::collections::BTreeSet<String> {
        builtin(code).unwrap().into_keys().collect()
    }

    #[test]
    fn included_languages_parse_and_have_the_same_keys() {
        let es = keys("es");
        let en = keys("en");
        assert!(es.len() > 100);
        let only_es: Vec<_> = es.difference(&en).collect();
        let only_en: Vec<_> = en.difference(&es).collect();
        assert!(only_es.is_empty(), "solo en es: {only_es:?}");
        assert!(only_en.is_empty(), "solo en en: {only_en:?}");
        for (code, text) in BUILTIN {
            for (k, v) in parse(text).unwrap() {
                assert!(!v.trim().is_empty(), "{code}: {k} vacío");
            }
        }
    }

    /// Cada `tr("…")`/`trf("…"` del código existe en los archivos de idioma.
    #[test]
    fn every_used_key_exists() {
        let en = keys("en");
        let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("..");
        let mut missing = Vec::new();
        let mut stack = vec![root];
        while let Some(d) = stack.pop() {
            let Ok(rd) = std::fs::read_dir(&d) else {
                continue;
            };
            for e in rd.flatten() {
                let p = e.path();
                if p.is_dir() {
                    if !p.ends_with("target") {
                        stack.push(p);
                    }
                    continue;
                }
                if p.extension().is_none_or(|x| x != "rs") || p.ends_with("i18n.rs") {
                    continue;
                }
                let text = std::fs::read_to_string(&p).unwrap_or_default();
                for pat in ["tr(\"", "trf(\""] {
                    for (i, _) in text.match_indices(pat) {
                        // `xtr("` u otras funciones que acaban en `tr`.
                        let prev = text[..i].chars().next_back();
                        if prev.is_some_and(|c| c.is_alphanumeric() || c == '_') {
                            continue;
                        }
                        let rest = &text[i + pat.len()..];
                        let key = &rest[..rest.find('"').unwrap_or(0)];
                        if !en.contains(key) {
                            missing.push(format!("{}: {key}", p.display()));
                        }
                    }
                }
            }
        }
        assert!(missing.is_empty(), "claves sin traducir: {missing:#?}");
    }

    #[test]
    fn translates_overrides_and_falls_back() {
        set_language("en", "es-CO").unwrap();
        assert_eq!(tr("action.open"), "Open");
        assert_eq!(code(), "en");
        set_language("auto", "es-CO").unwrap();
        assert_eq!(tr("action.open"), "Abrir");
        assert_eq!(language(), Lang::Es);
        assert_eq!(tr("missing.key"), "missing.key");
        assert_eq!(
            trf("calc.days", &[("n", &3)]),
            tr("calc.days").replace("{n}", "3")
        );

        let dir = std::env::temp_dir().join(format!("rukes-lang-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(dir.join("fr.toml"), "[action]\nopen = \"Ouvrir\"\n").unwrap();
        std::fs::write(dir.join("es.toml"), "[action]\nopen = \"Abrir ya\"\n").unwrap();
        set_lang_dir(Some(&dir));
        set_language("fr", "").unwrap();
        assert_eq!(tr("action.open"), "Ouvrir");
        assert_eq!(tr("action.copy_path"), "Copy path");
        assert_eq!(language(), Lang::En);
        set_language("auto", "fr-FR").unwrap();
        assert_eq!(code(), "fr");
        set_language("es", "").unwrap();
        assert_eq!(tr("action.open"), "Abrir ya");
        std::fs::write(dir.join("es.toml"), "[action\n").unwrap();
        assert!(set_language("es", "").is_err());
        assert_eq!(tr("action.open"), "Abrir");
        set_lang_dir(None);
        set_language("es", "").unwrap();
        let _ = std::fs::remove_dir_all(dir);
    }
}
