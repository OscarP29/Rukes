//! Decodificación de la salida de programas de consola.

use windows::Win32::Globalization::{CP_OEMCP, MULTI_BYTE_TO_WIDE_CHAR_FLAGS, MultiByteToWideChar};

/// Decodifica la salida de un programa de consola: UTF-8 si es válido; si no, la página de
/// códigos OEM (la que usan `cmd` y PowerShell al escribir en una tubería).
pub fn decode_console(bytes: &[u8]) -> String {
    let bytes = bytes.strip_prefix(b"\xEF\xBB\xBF").unwrap_or(bytes);
    if let Ok(s) = std::str::from_utf8(bytes) {
        return s.to_owned();
    }
    if bytes.len() > i32::MAX as usize {
        return String::from_utf8_lossy(bytes).into_owned();
    }
    // SAFETY: primero se pide el tamaño y luego se convierte en un buffer de ese tamaño.
    unsafe {
        let n = MultiByteToWideChar(CP_OEMCP, MULTI_BYTE_TO_WIDE_CHAR_FLAGS(0), bytes, None);
        if n <= 0 {
            return String::from_utf8_lossy(bytes).into_owned();
        }
        let mut buf = vec![0u16; n as usize];
        let n = MultiByteToWideChar(
            CP_OEMCP,
            MULTI_BYTE_TO_WIDE_CHAR_FLAGS(0),
            bytes,
            Some(&mut buf),
        );
        String::from_utf16_lossy(&buf[..n.max(0) as usize])
    }
}

#[cfg(test)]
mod tests {
    use super::decode_console;

    #[test]
    fn utf8_passes_through_and_oem_is_decoded() {
        assert_eq!(decode_console("añó".as_bytes()), "añó");
        assert_eq!(decode_console(b"\xEF\xBB\xBFhola"), "hola");
        // 0xA4 es «ñ» en CP850 y CP437 (las OEM habituales en español e inglés).
        let s = decode_console(b"a\xA4o");
        assert_eq!(s.chars().count(), 3);
        assert!(s.starts_with('a') && s.ends_with('o'));
    }
}
