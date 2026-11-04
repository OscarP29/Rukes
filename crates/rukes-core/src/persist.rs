//! Lectura y escritura atómica de JSON versionado (ADR 0004).

use std::io;
use std::path::Path;

use serde::Serialize;
use serde::de::DeserializeOwned;

/// Lee un JSON. Devuelve `None` si no existe o está corrupto (se registra en el log).
pub fn load_json<T: DeserializeOwned>(path: &Path) -> Option<T> {
    let bytes = match std::fs::read(path) {
        Ok(b) => b,
        Err(e) if e.kind() == io::ErrorKind::NotFound => return None,
        Err(e) => {
            tracing::warn!(path = %path.display(), error = %e, "no se pudo leer");
            return None;
        }
    };
    match serde_json::from_slice(&bytes) {
        Ok(v) => Some(v),
        Err(e) => {
            tracing::warn!(path = %path.display(), error = %e, "archivo corrupto; se descarta");
            None
        }
    }
}

/// Escribe un JSON de forma atómica: archivo temporal y renombrado.
pub fn save_json<T: Serialize>(path: &Path, value: &T) -> io::Result<()> {
    if let Some(dir) = path.parent() {
        std::fs::create_dir_all(dir)?;
    }
    let tmp = path.with_extension("json.tmp");
    let bytes = serde_json::to_vec(value).map_err(io::Error::other)?;
    std::fs::write(&tmp, bytes)?;
    std::fs::rename(&tmp, path)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn roundtrip_and_corrupt() {
        let dir = std::env::temp_dir().join(format!("rukes-persist-{}", std::process::id()));
        let p = dir.join("x.json");
        save_json(&p, &vec![1, 2, 3]).unwrap();
        assert_eq!(load_json::<Vec<i32>>(&p), Some(vec![1, 2, 3]));
        std::fs::write(&p, b"{no json").unwrap();
        assert_eq!(load_json::<Vec<i32>>(&p), None);
        assert_eq!(load_json::<Vec<i32>>(&dir.join("missing.json")), None);
        let _ = std::fs::remove_dir_all(dir);
    }
}
