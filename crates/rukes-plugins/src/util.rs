//! Utilidades compartidas por los plugins.

use rukes_core::i18n::{tr, trf};
use rukes_core::{ActionOutcome, PluginError};

/// Copia texto al portapapeles y devuelve el aviso «Copiado».
pub fn copy(text: &str) -> Result<ActionOutcome, PluginError> {
    rukes_platform::clipboard::set_text(text)
        .map(|()| {
            ActionOutcome::Notify(format!("{} · {}", tr("notify.copied"), ellipsis(text, 60)))
        })
        .map_err(|e| PluginError::Failed(e.to_string()))
}

/// Segundos Unix actuales.
pub fn now_unix() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |d| d.as_secs())
}

/// «hace 5 min», «5 min ago».
pub fn ago(unix: u64) -> String {
    let s = now_unix().saturating_sub(unix);
    if s < 60 {
        return tr("time.just_now").into();
    }
    let (n, unit) = if s < 3600 {
        (s / 60, "min")
    } else if s < 86_400 {
        (s / 3600, "h")
    } else {
        (s / 86_400, "d")
    };
    trf("time.ago", &[("n", &n), ("unit", &unit)])
}

/// Recorta un texto largo para mostrarlo en una línea.
pub fn ellipsis(s: &str, max: usize) -> String {
    let one_line: String = s
        .chars()
        .map(|c| if c.is_control() { ' ' } else { c })
        .collect();
    if one_line.chars().count() <= max {
        one_line
    } else {
        let mut t: String = one_line.chars().take(max.saturating_sub(1)).collect();
        t.push('…');
        t
    }
}

/// Minúsculas y sin tildes (para comparar palabras clave en español).
pub fn fold(s: &str) -> String {
    s.chars()
        .map(|c| match c {
            'á' | 'à' | 'ä' | 'Á' | 'À' | 'Ä' => 'a',
            'é' | 'è' | 'ë' | 'É' | 'È' | 'Ë' => 'e',
            'í' | 'ì' | 'ï' | 'Í' | 'Ì' | 'Ï' => 'i',
            'ó' | 'ò' | 'ö' | 'Ó' | 'Ò' | 'Ö' => 'o',
            'ú' | 'ù' | 'ü' | 'Ú' | 'Ù' | 'Ü' => 'u',
            'ñ' | 'Ñ' => 'n',
            c => c.to_ascii_lowercase(),
        })
        .collect()
}

/// Si `text` empieza por alguna de las frases (comparadas con [`fold`]), devuelve el resto.
pub fn strip_any<'a>(text: &'a str, phrases: &[&str]) -> Option<&'a str> {
    let folded = fold(text);
    for p in phrases {
        if folded.starts_with(p) {
            // `fold` conserva el número de caracteres: se corta por caracteres, no por bytes.
            let n = p.chars().count();
            let byte = text.char_indices().nth(n).map_or(text.len(), |(i, _)| i);
            let rest = &text[byte..];
            if rest.is_empty() || rest.starts_with(char::is_whitespace) {
                return Some(rest.trim());
            }
        }
    }
    None
}

/// Codifica para URL (RFC 3986: solo se dejan sin codificar los caracteres no reservados).
pub fn percent_encode(s: &str) -> String {
    let mut out = String::with_capacity(s.len() * 3);
    for b in s.bytes() {
        if b.is_ascii_alphanumeric() || b"-._~".contains(&b) {
            out.push(char::from(b));
        } else {
            out.push_str(&format!("%{b:02X}"));
        }
    }
    out
}

/// Decodifica `%XX` (y `+` como espacio). Devuelve `None` si el resultado no es UTF-8.
pub fn percent_decode(s: &str) -> Option<String> {
    let bytes = s.as_bytes();
    let mut out = Vec::with_capacity(bytes.len());
    let mut i = 0;
    while i < bytes.len() {
        match bytes[i] {
            b'%' if i + 2 < bytes.len() => {
                let hex = std::str::from_utf8(&bytes[i + 1..i + 3]).ok()?;
                out.push(u8::from_str_radix(hex, 16).ok()?);
                i += 3;
            }
            b'+' => {
                out.push(b' ');
                i += 1;
            }
            b => {
                out.push(b);
                i += 1;
            }
        }
    }
    String::from_utf8(out).ok()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fold_and_strip() {
        assert_eq!(fold("Días Ñandú"), "dias nandu");
        assert_eq!(
            strip_any("Días hasta 25 dic", &["dias hasta"]),
            Some("25 dic")
        );
        assert_eq!(strip_any("diashasta", &["dias hasta"]), None);
        assert_eq!(strip_any("hora en  Tokio ", &["hora en"]), Some("Tokio"));
    }

    #[test]
    fn percent() {
        assert_eq!(
            percent_encode("rust lifetimes ñ&"),
            "rust%20lifetimes%20%C3%B1%26"
        );
        assert_eq!(percent_decode("a%20b+c%C3%B1").as_deref(), Some("a b cñ"));
        assert_eq!(percent_decode("%zz"), None);
        assert_eq!(percent_decode("100%").as_deref(), Some("100%"));
    }

    #[test]
    fn ellipsis_cuts() {
        assert_eq!(ellipsis("abc", 5), "abc");
        assert_eq!(ellipsis("abcdefgh", 5), "abcd…");
        assert_eq!(ellipsis("a\nb", 5), "a b");
    }
}
