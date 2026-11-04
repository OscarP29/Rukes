//! Caché de iconos en disco (RF-196). Cada icono es un archivo `<hash>.rgba` con ancho y alto
//! (u32 LE) seguidos de los píxeles. La clave incluye la fecha y el tamaño del archivo de
//! origen, así un ejecutable actualizado vuelve a extraerse.

use std::path::{Path, PathBuf};

use rukes_ui::IconRgba;

/// Archivos a partir de los cuales la caché se vacía al arrancar.
const MAX_FILES: usize = 6000;
const EXT: &str = "rgba";

/// FNV-1a de 64 bits.
fn fnv(bytes: &[u8]) -> u64 {
    let mut h: u64 = 0xcbf2_9ce4_8422_2325;
    for b in bytes {
        h ^= u64::from(*b);
        h = h.wrapping_mul(0x0100_0000_01b3);
    }
    h
}

pub struct IconCache {
    dir: PathBuf,
}

impl IconCache {
    /// Abre la caché y la vacía si ha crecido demasiado.
    pub fn open(dir: PathBuf) -> Self {
        if let Ok(rd) = std::fs::read_dir(&dir)
            && rd.take(MAX_FILES + 1).count() > MAX_FILES
        {
            tracing::info!("caché de iconos llena; se vacía");
            let _ = std::fs::remove_dir_all(&dir);
        }
        Self { dir }
    }

    /// Clave: origen, tamaño y, si el origen es un archivo, su fecha y longitud.
    pub fn key(source: &str, px: u32) -> u64 {
        let mut s = format!("{}|{px}", source.to_lowercase());
        if let Ok(m) = std::fs::metadata(Path::new(source)) {
            let t = m
                .modified()
                .ok()
                .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
                .map_or(0, |d| d.as_secs());
            s.push_str(&format!("|{t}|{}", m.len()));
        }
        fnv(s.as_bytes())
    }

    fn file(&self, key: u64) -> PathBuf {
        self.dir.join(format!("{key:016x}.{EXT}"))
    }

    pub fn get(&self, key: u64) -> Option<IconRgba> {
        let b = std::fs::read(self.file(key)).ok()?;
        let w = u32::from_le_bytes(b.get(0..4)?.try_into().ok()?);
        let h = u32::from_le_bytes(b.get(4..8)?.try_into().ok()?);
        let px = b.get(8..)?;
        (w > 0 && w <= 1024 && h > 0 && h <= 1024 && px.len() == (w * h * 4) as usize)
            .then(|| (w, h, px.to_vec()))
    }

    pub fn put(&self, key: u64, (w, h, px): &IconRgba) {
        let mut b = Vec::with_capacity(8 + px.len());
        b.extend_from_slice(&w.to_le_bytes());
        b.extend_from_slice(&h.to_le_bytes());
        b.extend_from_slice(px);
        let file = self.file(key);
        let tmp = file.with_extension("tmp");
        let res = std::fs::create_dir_all(&self.dir)
            .and_then(|()| std::fs::write(&tmp, &b))
            .and_then(|()| std::fs::rename(&tmp, &file));
        if let Err(e) = res {
            tracing::debug!(%e, "no se pudo guardar un icono en la caché");
            let _ = std::fs::remove_file(tmp);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn roundtrip_and_key_changes_with_file() {
        let dir = std::env::temp_dir().join(format!("rukes-iconcache-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        let c = IconCache::open(dir.join("icons"));
        let src = dir.join("app.exe");
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(&src, b"v1").unwrap();
        let s = src.to_string_lossy();
        let k = IconCache::key(&s, 56);
        assert_ne!(k, IconCache::key(&s, 32));
        assert!(c.get(k).is_none());
        let icon = (2, 1, vec![1, 2, 3, 4, 5, 6, 7, 8]);
        c.put(k, &icon);
        assert_eq!(c.get(k), Some(icon));
        std::fs::write(&src, b"version 2").unwrap();
        assert_ne!(k, IconCache::key(&s, 56));
        // Archivo corrupto: se ignora.
        std::fs::write(c.file(7), b"xx").unwrap();
        assert!(c.get(7).is_none());
        let _ = std::fs::remove_dir_all(dir);
    }
}
