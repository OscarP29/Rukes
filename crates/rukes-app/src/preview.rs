//! Contenido del panel de previsualización (RF-017): miniatura, texto, Markdown y datos.

use std::io::Read as _;
use std::path::Path;

use rukes_core::i18n::{tr, trf};
use rukes_platform::{icons, time};
use rukes_ui::Preview;

/// Bytes que se leen para la vista de texto.
const TEXT_BYTES: usize = 32 * 1024;
/// Líneas y caracteres por línea que se muestran.
const TEXT_LINES: usize = 40;
const LINE_CHARS: usize = 160;
/// Elementos de una carpeta que se cuentan como mucho.
const MAX_COUNT: usize = 10_000;
/// Lado de la miniatura en píxeles físicos.
const THUMB_PX: u32 = 256;
const ICON_PX: u32 = 96;

/// Extensiones que se muestran como imagen (miniatura del shell).
const IMAGE_EXT: &[&str] = &[
    "png", "jpg", "jpeg", "gif", "bmp", "webp", "tif", "tiff", "ico", "heic", "avif", "svg", "mp4",
    "mkv", "mov", "avi", "webm", "pdf",
];

/// `1,5 MB`, `320 KB`, `12 bytes`.
pub fn fmt_size(n: u64) -> String {
    const UNITS: [&str; 4] = ["KB", "MB", "GB", "TB"];
    if n < 1024 {
        return format!("{n} bytes");
    }
    let mut v = n as f64 / 1024.0;
    let mut u = 0;
    while v >= 1024.0 && u < UNITS.len() - 1 {
        v /= 1024.0;
        u += 1;
    }
    let s = if v >= 100.0 {
        format!("{v:.0}")
    } else {
        format!("{v:.1}")
    };
    let s = if rukes_core::i18n::language() == rukes_core::i18n::Lang::Es {
        s.replace('.', ",")
    } else {
        s
    };
    format!("{s} {}", UNITS[u])
}

/// Texto legible de los primeros bytes, o `None` si parece binario.
pub fn text_of(bytes: &[u8]) -> Option<String> {
    let bytes = bytes.strip_prefix(b"\xEF\xBB\xBF").unwrap_or(bytes);
    // UTF-16 con BOM.
    if let Some(rest) = bytes.strip_prefix(b"\xFF\xFE") {
        let units: Vec<u16> = rest
            .chunks_exact(2)
            .map(|c| u16::from_le_bytes([c[0], c[1]]))
            .collect();
        return Some(clip(&String::from_utf16_lossy(&units)));
    }
    if bytes.iter().take(8192).any(|b| *b == 0) {
        return None;
    }
    let s = String::from_utf8_lossy(bytes);
    // Demasiados caracteres de reemplazo: binario o una codificación que no es UTF-8.
    let bad = s.chars().filter(|c| *c == '\u{fffd}').count();
    if bad > 8 && bad * 50 > s.len() {
        return None;
    }
    Some(clip(&s))
}

fn clip(s: &str) -> String {
    s.lines()
        .take(TEXT_LINES)
        .map(|l| {
            let l = l.replace('\t', "    ");
            if l.chars().count() > LINE_CHARS {
                l.chars().take(LINE_CHARS).chain(['…']).collect()
            } else {
                l
            }
        })
        .collect::<Vec<_>>()
        .join("\n")
}

fn modified(m: &std::fs::Metadata) -> Option<String> {
    let t = time::local_from(m.modified().ok()?)?;
    Some(format!(
        "{:04}-{:02}-{:02} {:02}:{:02}",
        t.year, t.month, t.day, t.hour, t.minute
    ))
}

/// Previsualización de un archivo o carpeta.
pub fn build(path: &Path) -> Option<Preview> {
    let meta = std::fs::metadata(path).ok()?;
    let source = path.to_string_lossy();
    let mut p = Preview::default();
    let location = path
        .parent()
        .map(|d| d.display().to_string())
        .unwrap_or_default();
    if meta.is_dir() {
        let count = std::fs::read_dir(path)
            .map(|r| r.take(MAX_COUNT).count())
            .ok();
        p.image = icons::load(&source, ICON_PX).map(|i| (i.width, i.height, i.pixels));
        p.info
            .push((tr("preview.kind").into(), tr("preview.folder").into()));
        if let Some(n) = count {
            let n = if n >= MAX_COUNT {
                format!("{MAX_COUNT}+")
            } else {
                n.to_string()
            };
            p.info.push((tr("preview.items").into(), n));
        }
    } else {
        let ext = path
            .extension()
            .map(|e| e.to_string_lossy().to_lowercase())
            .unwrap_or_default();
        let kind = match ext.as_str() {
            "" => tr("preview.file_noext").to_owned(),
            "exe" | "lnk" | "appref-ms" => tr("preview.app").to_owned(),
            e => trf("preview.file", &[("ext", &e.to_uppercase())]),
        };
        p.info.push((tr("preview.kind").into(), kind));
        p.info
            .push((tr("preview.size").into(), fmt_size(meta.len())));
        let is_image = IMAGE_EXT.contains(&ext.as_str());
        if !is_image && meta.len() > 0 && !matches!(ext.as_str(), "exe" | "dll" | "lnk") {
            let mut buf = Vec::with_capacity(TEXT_BYTES.min(meta.len() as usize));
            if let Ok(f) = std::fs::File::open(path) {
                let _ = f.take(TEXT_BYTES as u64).read_to_end(&mut buf);
            }
            p.text = text_of(&buf);
            p.markdown = p.text.is_some() && matches!(ext.as_str(), "md" | "markdown");
        }
        if p.text.is_none() {
            p.image = icons::thumbnail(&source, THUMB_PX)
                .or_else(|| icons::load(&source, ICON_PX))
                .map(|i| (i.width, i.height, i.pixels));
        }
    }
    if let Some(m) = modified(&meta) {
        p.info.push((tr("preview.modified").into(), m));
    }
    if !location.is_empty() {
        p.info.push((tr("preview.location").into(), location));
    }
    Some(p)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sizes() {
        let _ = rukes_core::i18n::set_language("en", "");
        assert_eq!(fmt_size(12), "12 bytes");
        assert_eq!(fmt_size(1536), "1.5 KB");
        assert_eq!(fmt_size(300 * 1024 * 1024), "300 MB");
    }

    #[test]
    fn text_detection() {
        assert_eq!(text_of(b"hola\nmundo").as_deref(), Some("hola\nmundo"));
        assert_eq!(text_of(b"\xEF\xBB\xBF# T\n").as_deref(), Some("# T"));
        assert!(text_of(b"MZ\x90\x00\x03\x00").is_none());
        let utf16: Vec<u8> = [0xFF, 0xFE]
            .into_iter()
            .chain("año".encode_utf16().flat_map(u16::to_le_bytes))
            .collect();
        assert_eq!(text_of(&utf16).as_deref(), Some("año"));
        let long = "x".repeat(500);
        assert_eq!(
            text_of(long.as_bytes()).unwrap().chars().count(),
            LINE_CHARS + 1
        );
        let many = "l\n".repeat(100);
        assert_eq!(
            text_of(many.as_bytes()).unwrap().lines().count(),
            TEXT_LINES
        );
    }

    #[test]
    fn builds_for_files_and_folders() {
        let dir = std::env::temp_dir().join(format!("rukes-preview-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let md = dir.join("notas.md");
        std::fs::write(&md, "# Título\n- uno\n").unwrap();
        let p = build(&md).unwrap();
        assert!(p.markdown);
        assert_eq!(p.text.as_deref(), Some("# Título\n- uno"));
        assert!(p.info.iter().any(|(_, v)| v.contains("MD")));
        let d = build(&dir).unwrap();
        assert!(d.info.iter().any(|(_, v)| v == "1"));
        assert!(build(&dir.join("no-existe")).is_none());
        let _ = std::fs::remove_dir_all(dir);
    }
}
