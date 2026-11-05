//! Coloreado de la respuesta del mini Postman: JSON (claves, textos, números, `true`/`false`/
//! `null` y signos) y cabeceras (`Nombre: valor`). Va línea a línea y sin estado entre líneas:
//! el cuerpo llega ya formateado, así que ningún texto ocupa más de una línea.

use std::ops::Range;

use iced::Color;
use iced::advanced::text::highlighter::{Format, Highlighter};

/// Qué se colorea.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Mode {
    /// Nada (texto, HTML, errores).
    Plain,
    /// Cuerpo JSON.
    Json,
    /// Cabeceras `Nombre: valor`.
    Headers,
}

/// Colores de cada pieza, tomados del tema.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Palette {
    pub mode: Mode,
    pub key: Color,
    pub string: Color,
    pub number: Color,
    pub literal: Color,
    pub punct: Color,
}

/// Elige el modo para una respuesta: JSON si lo dice el `Content-Type` o si el cuerpo empieza
/// por `{` o `[`.
pub fn mode_for(content_type: &str, body: &str, headers: bool) -> Mode {
    if headers {
        return Mode::Headers;
    }
    let t = body.trim_start();
    if content_type.to_ascii_lowercase().contains("json")
        || t.starts_with('{')
        || t.starts_with('[')
    {
        Mode::Json
    } else {
        Mode::Plain
    }
}

/// Resaltador para `text_editor::highlight_with`.
pub struct JsonHighlighter {
    palette: Palette,
    line: usize,
}

/// Formato de un trozo: solo el color.
pub fn to_format<F>(c: &Color, _theme: &iced::Theme) -> Format<F> {
    Format {
        color: Some(*c),
        font: None,
    }
}

impl Highlighter for JsonHighlighter {
    type Settings = Palette;
    type Highlight = Color;
    type Iterator<'a> = std::vec::IntoIter<(Range<usize>, Color)>;

    fn new(settings: &Palette) -> Self {
        Self {
            palette: *settings,
            line: 0,
        }
    }

    fn update(&mut self, new_settings: &Palette) {
        self.palette = *new_settings;
        self.line = 0;
    }

    fn change_line(&mut self, line: usize) {
        self.line = self.line.min(line);
    }

    fn highlight_line(&mut self, line: &str) -> Self::Iterator<'_> {
        self.line += 1;
        let spans = match self.palette.mode {
            Mode::Plain => Vec::new(),
            Mode::Json => json_line(line, &self.palette),
            Mode::Headers => header_line(line, &self.palette),
        };
        spans.into_iter()
    }

    fn current_line(&self) -> usize {
        self.line
    }
}

/// Trozos coloreados de una línea de JSON.
pub fn json_line(line: &str, p: &Palette) -> Vec<(Range<usize>, Color)> {
    let b = line.as_bytes();
    let mut out = Vec::new();
    let mut i = 0;
    while i < b.len() {
        let c = b[i];
        match c {
            b'"' => {
                let start = i;
                i += 1;
                while i < b.len() && b[i] != b'"' {
                    i += if b[i] == b'\\' { 2 } else { 1 };
                }
                i = (i + 1).min(b.len());
                // Una cadena seguida de `:` es una clave.
                let is_key = b[i..]
                    .iter()
                    .find(|c| !c.is_ascii_whitespace())
                    .is_some_and(|&c| c == b':');
                out.push((start..i, if is_key { p.key } else { p.string }));
            }
            b'-' | b'0'..=b'9' => {
                let start = i;
                i += 1;
                while i < b.len() && matches!(b[i], b'0'..=b'9' | b'.' | b'e' | b'E' | b'+' | b'-')
                {
                    i += 1;
                }
                out.push((start..i, p.number));
            }
            b'a'..=b'z' | b'A'..=b'Z' => {
                let start = i;
                while i < b.len() && b[i].is_ascii_alphanumeric() {
                    i += 1;
                }
                if matches!(&line[start..i], "true" | "false" | "null") {
                    out.push((start..i, p.literal));
                }
            }
            b'{' | b'}' | b'[' | b']' | b',' | b':' => {
                out.push((i..i + 1, p.punct));
                i += 1;
            }
            _ => i += 1,
        }
    }
    out
}

/// `Nombre: valor` → el nombre y los dos puntos coloreados.
pub fn header_line(line: &str, p: &Palette) -> Vec<(Range<usize>, Color)> {
    match line.find(':') {
        Some(n) if n > 0 && !line[..n].contains(' ') => {
            vec![(0..n, p.key), (n..n + 1, p.punct)]
        }
        // La primera línea (`HTTP/1.1 200 OK`).
        _ if line.starts_with("HTTP/") => vec![(0..line.len(), p.number)],
        _ => Vec::new(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn pal() -> Palette {
        let c = |r| Color::from_rgb(r, 0.0, 0.0);
        Palette {
            mode: Mode::Json,
            key: c(0.1),
            string: c(0.2),
            number: c(0.3),
            literal: c(0.4),
            punct: c(0.5),
        }
    }

    fn kinds(line: &str) -> Vec<(&str, f32)> {
        json_line(line, &pal())
            .into_iter()
            .map(|(r, c)| (&line[r], c.r))
            .collect()
    }

    #[test]
    fn colors_keys_values_and_punctuation() {
        assert_eq!(
            kinds(r#"  "nombre": "Ana \"A\" Ñú", "n": -1.5e3, "ok": true, "x": null"#),
            vec![
                (r#""nombre""#, 0.1),
                (":", 0.5),
                (r#""Ana \"A\" Ñú""#, 0.2),
                (",", 0.5),
                (r#""n""#, 0.1),
                (":", 0.5),
                ("-1.5e3", 0.3),
                (",", 0.5),
                (r#""ok""#, 0.1),
                (":", 0.5),
                ("true", 0.4),
                (",", 0.5),
                (r#""x""#, 0.1),
                (":", 0.5),
                ("null", 0.4),
            ]
        );
        assert_eq!(kinds(r#"["a", 1]"#).len(), 5);
        // Cadena sin cerrar: hasta el final, sin salirse.
        assert_eq!(kinds(r#""abc\"#), vec![(r#""abc\"#, 0.2)]);
    }

    #[test]
    fn headers_and_mode() {
        let p = pal();
        assert_eq!(header_line("Content-Type: text/html", &p).len(), 2);
        assert!(header_line("sin cabecera", &p).is_empty());
        assert_eq!(
            mode_for("application/json; charset=utf-8", "x", false),
            Mode::Json
        );
        assert_eq!(mode_for("text/plain", "  [1]", false), Mode::Json);
        assert_eq!(mode_for("text/html", "<html>", false), Mode::Plain);
        assert_eq!(mode_for("application/json", "{}", true), Mode::Headers);
    }
}
