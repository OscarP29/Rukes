//! Generadores (UUID, contraseñas, lorem ipsum, timestamps), hashes y codificadores
//! (RF-080, RF-081, RF-082).
//!
//! Se activan con la primera palabra: `uuid`, `pass 16`, `lorem 3`, `ts 1700000000`,
//! `sha256 hola`, `sha256 C:\ruta\archivo`, `b64 texto`, `b64d dGV4dG8=`, `url`, `urld`, `html`,
//! `htmld`, `hex`, `unhex`.

use std::fs::File;
use std::io::{BufReader, Read};
use std::path::Path;
use std::time::{SystemTime, UNIX_EPOCH};

use base64::Engine as _;
use rukes_core::i18n::{tr, trf};
use rukes_core::{
    ActionId, ActionOutcome, CancelToken, Icon, Plugin, PluginError, PluginMetadata, Query,
    ResultItem,
};
use sha2::Digest;

use crate::calc::civil_from_days;
use crate::util::{self, ellipsis, fold, percent_decode, percent_encode};

/// Plugin de generadores.
pub struct GeneratorsPlugin {
    /// Longitud de contraseña por defecto.
    pub password_length: usize,
    /// Incluir símbolos en las contraseñas.
    pub password_symbols: bool,
}

/// Archivos más grandes no se hashean al escribir (se haría en cada pulsación).
const MAX_HASH_FILE: u64 = 64 * 1024 * 1024;

fn result(id: &str, value: String, label: &str) -> ResultItem {
    let mut it = ResultItem::new(id, ellipsis(&value, 90));
    it.subtitle = format!("{label} · {}", tr("calc.enter_copies"));
    it.icon = Icon::Named("generator".into());
    it.fuzzy = 1.0;
    it.payload = value;
    it
}

/// Número aleatorio uniforme en `0..n` (muestreo por rechazo, sin sesgo).
fn random_below(n: u32) -> Option<u32> {
    let zone = u32::MAX - (u32::MAX % n);
    loop {
        let v = getrandom::u32().ok()?;
        if v < zone {
            return Some(v % n);
        }
    }
}

/// Contraseña aleatoria con al menos una minúscula, una mayúscula, un dígito y (si se piden)
/// un símbolo.
pub fn password(len: usize, symbols: bool) -> Option<String> {
    const LOWER: &str = "abcdefghijkmnopqrstuvwxyz";
    const UPPER: &str = "ABCDEFGHJKLMNPQRSTUVWXYZ";
    const DIGITS: &str = "23456789";
    const SYMBOLS: &str = "!#$%&*+-=?@^_~";
    let mut classes = vec![LOWER, UPPER, DIGITS];
    if symbols {
        classes.push(SYMBOLS);
    }
    let len = len.clamp(classes.len(), 256);
    let all: Vec<char> = classes.concat().chars().collect();
    loop {
        let mut out = String::with_capacity(len);
        for _ in 0..len {
            out.push(all[random_below(all.len() as u32)? as usize]);
        }
        if classes.iter().all(|c| out.chars().any(|ch| c.contains(ch))) {
            return Some(out);
        }
    }
}

const LOREM: &str = "Lorem ipsum dolor sit amet, consectetur adipiscing elit, sed do eiusmod tempor \
incididunt ut labore et dolore magna aliqua. Ut enim ad minim veniam, quis nostrud exercitation \
ullamco laboris nisi ut aliquip ex ea commodo consequat. Duis aute irure dolor in reprehenderit in \
voluptate velit esse cillum dolore eu fugiat nulla pariatur. Excepteur sint occaecat cupidatat non \
proident, sunt in culpa qui officia deserunt mollit anim id est laborum.";

/// `n` párrafos de lorem ipsum.
pub fn lorem(n: usize) -> String {
    vec![LOREM; n.clamp(1, 20)].join("\n\n")
}

fn iso_utc(secs: i64) -> String {
    let days = secs.div_euclid(86_400);
    let rem = secs.rem_euclid(86_400);
    let (y, m, d) = civil_from_days(days);
    format!(
        "{y:04}-{m:02}-{d:02}T{:02}:{:02}:{:02}Z",
        rem / 3600,
        rem % 3600 / 60,
        rem % 60
    )
}

fn to_hex(bytes: &[u8]) -> String {
    bytes.iter().map(|b| format!("{b:02x}")).collect()
}

fn from_hex(s: &str) -> Option<Vec<u8>> {
    let s: String = s.chars().filter(|c| !c.is_whitespace()).collect();
    let s = s.strip_prefix("0x").unwrap_or(&s);
    if s.len() % 2 != 0 {
        return None;
    }
    (0..s.len())
        .step_by(2)
        .map(|i| u8::from_str_radix(s.get(i..i + 2)?, 16).ok())
        .collect()
}

/// Algoritmo de hash.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Hash {
    /// MD5.
    Md5,
    /// SHA-1.
    Sha1,
    /// SHA-256.
    Sha256,
    /// SHA-512.
    Sha512,
    /// CRC32.
    Crc32,
}

impl Hash {
    fn parse(word: &str) -> Option<Self> {
        Some(match word {
            "md5" => Self::Md5,
            "sha1" => Self::Sha1,
            "sha256" | "sha" => Self::Sha256,
            "sha512" => Self::Sha512,
            "crc32" | "crc" => Self::Crc32,
            _ => return None,
        })
    }

    fn label(self) -> &'static str {
        match self {
            Self::Md5 => "MD5",
            Self::Sha1 => "SHA-1",
            Self::Sha256 => "SHA-256",
            Self::Sha512 => "SHA-512",
            Self::Crc32 => "CRC32",
        }
    }

    /// Hash en hexadecimal de todo lo que se lea de `r`.
    pub fn hex(self, mut r: impl Read, cancel: &CancelToken) -> Option<String> {
        fn run<D: Digest>(r: &mut dyn Read, cancel: &CancelToken) -> Option<String> {
            let mut d = D::new();
            feed(r, cancel, |chunk| d.update(chunk))?;
            Some(to_hex(&d.finalize()))
        }
        match self {
            Self::Md5 => run::<md5::Md5>(&mut r, cancel),
            Self::Sha1 => run::<sha1::Sha1>(&mut r, cancel),
            Self::Sha256 => run::<sha2::Sha256>(&mut r, cancel),
            Self::Sha512 => run::<sha2::Sha512>(&mut r, cancel),
            Self::Crc32 => {
                let mut h = crc32fast::Hasher::new();
                feed(&mut r, cancel, |chunk| h.update(chunk))?;
                Some(format!("{:08x}", h.finalize()))
            }
        }
    }
}

fn feed(r: &mut dyn Read, cancel: &CancelToken, mut f: impl FnMut(&[u8])) -> Option<()> {
    let mut buf = vec![0u8; 64 * 1024];
    loop {
        if cancel.is_cancelled() {
            return None;
        }
        let n = r.read(&mut buf).ok()?;
        if n == 0 {
            return Some(());
        }
        f(&buf[..n]);
    }
}

fn html_encode(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for c in s.chars() {
        match c {
            '&' => out.push_str("&amp;"),
            '<' => out.push_str("&lt;"),
            '>' => out.push_str("&gt;"),
            '"' => out.push_str("&quot;"),
            '\'' => out.push_str("&#39;"),
            c => out.push(c),
        }
    }
    out
}

fn html_decode(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    let mut rest = s;
    while let Some(p) = rest.find('&') {
        out.push_str(&rest[..p]);
        rest = &rest[p..];
        let decoded = rest.find(';').filter(|&e| e <= 10).and_then(|e| {
            let ent = &rest[1..e];
            let c = match ent {
                "amp" => Some('&'),
                "lt" => Some('<'),
                "gt" => Some('>'),
                "quot" => Some('"'),
                "apos" => Some('\''),
                "nbsp" => Some('\u{a0}'),
                _ => ent
                    .strip_prefix("#x")
                    .or_else(|| ent.strip_prefix("#X"))
                    .and_then(|h| u32::from_str_radix(h, 16).ok())
                    .or_else(|| ent.strip_prefix('#').and_then(|d| d.parse().ok()))
                    .and_then(char::from_u32),
            };
            c.map(|c| (c, e))
        });
        match decoded {
            Some((c, e)) => {
                out.push(c);
                rest = &rest[e + 1..];
            }
            None => {
                out.push('&');
                rest = &rest[1..];
            }
        }
    }
    out.push_str(rest);
    out
}

fn now_unix() -> (i64, u128) {
    let d = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default();
    (d.as_secs() as i64, d.as_millis())
}

impl GeneratorsPlugin {
    /// Resultados para una consulta (sin prefijo).
    pub fn compute(&self, text: &str, cancel: &CancelToken) -> Vec<ResultItem> {
        let text = text.trim();
        let (word, arg) = match text.split_once(char::is_whitespace) {
            Some((w, a)) => (fold(w), a.trim()),
            None => (fold(text), ""),
        };
        let mut out = Vec::new();
        match word.as_str() {
            "uuid" | "guid" | "uuid4" | "uuid7" => {
                let v4 = uuid::Uuid::new_v4().to_string();
                let v7 = uuid::Uuid::now_v7().to_string();
                let (a, b) = if word == "uuid7" {
                    (("uuid7", v7, "UUID v7"), ("uuid4", v4, "UUID v4"))
                } else {
                    (("uuid4", v4, "UUID v4"), ("uuid7", v7, "UUID v7"))
                };
                out.push(result(a.0, a.1, a.2));
                let mut second = result(b.0, b.1, b.2);
                second.fuzzy = 0.95;
                out.push(second);
            }
            "pass" | "password" | "contrasena" | "pwd" => {
                let len = arg.parse().unwrap_or(self.password_length);
                let label = tr("generators.password");
                if let Some(p) = password(len, self.password_symbols) {
                    out.push(result("password", p, &format!("{label} · {len}")));
                }
                if self.password_symbols
                    && let Some(p) = password(len, false)
                {
                    let mut it = result(
                        "password_alnum",
                        p,
                        &format!("{label} · {len} · {}", tr("generators.no_symbols")),
                    );
                    it.fuzzy = 0.95;
                    out.push(it);
                }
            }
            "lorem" | "ipsum" => {
                let n = arg.parse().unwrap_or(1);
                let label = trf("generators.lorem", &[("n", &n)]);
                out.push(result("lorem", lorem(n), &label));
            }
            "ts" | "timestamp" | "unix" | "epoch" => {
                if let Ok(v) = arg.parse::<i64>() {
                    // Más de 11 dígitos: milisegundos.
                    let secs = if v.abs() > 99_999_999_999 {
                        v / 1000
                    } else {
                        v
                    };
                    out.push(result("ts_iso", iso_utc(secs), "UTC"));
                } else if arg.is_empty() {
                    let (s, ms) = now_unix();
                    out.push(result("ts_s", s.to_string(), tr("generators.unix_s")));
                    let mut it = result("ts_ms", ms.to_string(), tr("generators.unix_ms"));
                    it.fuzzy = 0.95;
                    out.push(it);
                    let mut it = result("ts_iso", iso_utc(s), "ISO 8601 UTC");
                    it.fuzzy = 0.9;
                    out.push(it);
                }
            }
            w => {
                if arg.is_empty() {
                    return out;
                }
                if let Some(h) = Hash::parse(w) {
                    let path = Path::new(arg.trim_matches('"'));
                    let file = path
                        .metadata()
                        .ok()
                        .filter(|m| m.is_file() && m.len() <= MAX_HASH_FILE)
                        .and_then(|_| File::open(path).ok());
                    let (value, label) = match file {
                        Some(f) => (
                            h.hex(BufReader::new(f), cancel),
                            format!("{} · {}", h.label(), path.display()),
                        ),
                        None => (h.hex(arg.as_bytes(), cancel), h.label().to_owned()),
                    };
                    if let Some(v) = value {
                        out.push(result("hash", v, &label));
                    }
                    return out;
                }
                let b64 = base64::engine::general_purpose::STANDARD;
                let encoded = match w {
                    "b64" | "base64" => Some((b64.encode(arg), "Base64")),
                    "b64d" | "unbase64" | "base64d" => b64
                        .decode(arg.trim())
                        .ok()
                        .and_then(|b| String::from_utf8(b).ok())
                        .map(|s| (s, tr("generators.b64_decoded"))),
                    "url" | "urlencode" => Some((percent_encode(arg), "URL")),
                    "urld" | "urldecode" => {
                        percent_decode(arg).map(|s| (s, tr("generators.url_decoded")))
                    }
                    "html" | "htmlencode" => Some((html_encode(arg), "HTML")),
                    "htmld" | "htmldecode" => {
                        Some((html_decode(arg), tr("generators.html_decoded")))
                    }
                    "hex" => Some((to_hex(arg.as_bytes()), "Hex")),
                    "unhex" | "hexd" => from_hex(arg)
                        .and_then(|b| String::from_utf8(b).ok())
                        .map(|s| (s, tr("generators.hex_decoded"))),
                    _ => None,
                };
                if let Some((v, label)) = encoded {
                    out.push(result("encode", v, label));
                }
            }
        }
        out
    }
}

impl Plugin for GeneratorsPlugin {
    fn metadata(&self) -> PluginMetadata {
        PluginMetadata {
            id: "generators".into(),
            name: tr("generators.name").into(),
            prefix: None,
            keywords: Vec::new(),
            priority: 0.4,
            global: true,
            list_on_keyword: false,
        }
    }

    fn query(&self, q: &Query, cancel: &CancelToken, sink: &mut dyn FnMut(Vec<ResultItem>)) {
        sink(self.compute(&q.text, cancel));
    }

    fn execute(&self, item: &ResultItem, action: &str) -> Result<ActionOutcome, PluginError> {
        match action {
            ActionId::OPEN => util::copy(&item.payload),
            other => Err(PluginError::UnsupportedAction(other.into())),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn first(q: &str) -> String {
        let p = GeneratorsPlugin {
            password_length: 20,
            password_symbols: true,
        };
        p.compute(q, &CancelToken::new()).remove(0).payload
    }

    #[test]
    fn hashes() {
        assert_eq!(first("md5 hola"), "4d186321c1a7f0f354b297e8914ab240");
        assert_eq!(
            first("sha1 hola"),
            "99800b85d3383e3a2fb45eb7d0066a4879a9dad0"
        );
        assert_eq!(
            first("sha256 hola"),
            "b221d9dbb083a7f33428d7c2a3c3198ae925614d70210e28716ccaa7cd4ddb79"
        );
        assert_eq!(first("crc32 hola").len(), 8);
        assert_eq!(first("sha512 hola").len(), 128);
    }

    #[test]
    fn hash_of_file() {
        let dir = std::env::temp_dir().join(format!("rukes-gen-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let f = dir.join("a.txt");
        std::fs::write(&f, "hola").unwrap();
        assert_eq!(
            first(&format!("md5 {}", f.display())),
            "4d186321c1a7f0f354b297e8914ab240"
        );
        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn encoders() {
        assert_eq!(first("b64 hola mundo"), "aG9sYSBtdW5kbw==");
        assert_eq!(first("b64d aG9sYSBtdW5kbw=="), "hola mundo");
        assert_eq!(first("url a b&c"), "a%20b%26c");
        assert_eq!(first("urld a%20b%26c"), "a b&c");
        assert_eq!(first("html <a href=\"x\">"), "&lt;a href=&quot;x&quot;&gt;");
        assert_eq!(
            first("htmld &lt;b&gt; &#241; &#xF1; &amp &zz;"),
            "<b> ñ ñ &amp &zz;"
        );
        assert_eq!(first("hex hi"), "6869");
        assert_eq!(first("unhex 6869"), "hi");
    }

    #[test]
    fn generators() {
        let u = first("uuid");
        assert_eq!(u.len(), 36);
        assert_eq!(&u[14..15], "4");
        assert_eq!(&first("uuid7")[14..15], "7");
        let p = first("pass 32");
        assert_eq!(p.chars().count(), 32);
        assert!(p.chars().any(|c| c.is_ascii_digit()));
        assert!(first("lorem 2").contains("\n\n"));
        assert_eq!(first("ts 0"), "1970-01-01T00:00:00Z");
        assert_eq!(first("ts 1700000000000"), "2023-11-14T22:13:20Z");
    }

    #[test]
    fn ignores_other_queries() {
        let p = GeneratorsPlugin {
            password_length: 20,
            password_symbols: true,
        };
        assert!(p.compute("chrome", &CancelToken::new()).is_empty());
        assert!(p.compute("sha256", &CancelToken::new()).is_empty());
    }
}
