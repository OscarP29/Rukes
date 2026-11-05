//! Plugin de ejemplo de Rukes en Rust: conversor de colores.
//!
//! Protocolo: JSON, un mensaje por línea, por stdin/stdout (docs/plugin-protocol.md).

use std::io::{BufRead, Write};

use serde_json::{Value, json};

fn hsl_to_rgb(h: f64, s: f64, l: f64) -> (u8, u8, u8) {
    let (s, l) = (s / 100.0, l / 100.0);
    let c = (1.0 - (2.0 * l - 1.0).abs()) * s;
    let hp = h.rem_euclid(360.0) / 60.0;
    let x = c * (1.0 - (hp % 2.0 - 1.0).abs());
    let (r, g, b) = match hp as u32 {
        0 => (c, x, 0.0),
        1 => (x, c, 0.0),
        2 => (0.0, c, x),
        3 => (0.0, x, c),
        4 => (x, 0.0, c),
        _ => (c, 0.0, x),
    };
    let m = l - c / 2.0;
    let f = |v: f64| ((v + m) * 255.0).round().clamp(0.0, 255.0) as u8;
    (f(r), f(g), f(b))
}

fn rgb_to_hsl(r: u8, g: u8, b: u8) -> (f64, f64, f64) {
    let (r, g, b) = (
        f64::from(r) / 255.0,
        f64::from(g) / 255.0,
        f64::from(b) / 255.0,
    );
    let (max, min) = (r.max(g).max(b), r.min(g).min(b));
    let l = (max + min) / 2.0;
    let d = max - min;
    if d == 0.0 {
        return (0.0, 0.0, l * 100.0);
    }
    let s = d / (1.0 - (2.0 * l - 1.0).abs());
    let h = if max == r {
        60.0 * ((g - b) / d).rem_euclid(6.0)
    } else if max == g {
        60.0 * ((b - r) / d + 2.0)
    } else {
        60.0 * ((r - g) / d + 4.0)
    };
    (h, s * 100.0, l * 100.0)
}

/// Interpreta `#rgb`, `#rrggbb`, `r g b`, `rgb(r, g, b)` o `hsl h s l`.
fn parse(text: &str) -> Option<(u8, u8, u8)> {
    let t = text.trim().to_lowercase();
    let hex = t.trim_start_matches('#');
    if matches!(hex.len(), 3 | 6) && hex.chars().all(|c| c.is_ascii_hexdigit()) {
        let full: String = if hex.len() == 3 {
            hex.chars().flat_map(|c| [c, c]).collect()
        } else {
            hex.to_owned()
        };
        let v = u32::from_str_radix(&full, 16).ok()?;
        return Some(((v >> 16) as u8, (v >> 8) as u8, v as u8));
    }
    let is_hsl = t.starts_with("hsl");
    let nums: Vec<f64> = t
        .trim_start_matches("rgb")
        .trim_start_matches("hsl")
        .split(|c: char| !(c.is_ascii_digit() || c == '.'))
        .filter(|s| !s.is_empty())
        .filter_map(|s| s.parse().ok())
        .collect();
    let [a, b, c] = nums[..] else { return None };
    if is_hsl {
        return Some(hsl_to_rgb(a, b.min(100.0), c.min(100.0)));
    }
    if [a, b, c].iter().all(|v| *v <= 255.0) {
        return Some((a as u8, b as u8, c as u8));
    }
    None
}

fn query(text: &str) -> Value {
    let Some((r, g, b)) = parse(text) else {
        return json!([{
            "id": "ayuda",
            "title": "Escribe un color",
            "subtitle": "#ff8800 · 255 136 0 · rgb(255, 136, 0) · hsl 32 100 50",
            "actions": [],
        }]);
    };
    let (h, s, l) = rgb_to_hsl(r, g, b);
    let forms = [
        ("hex", format!("#{r:02x}{g:02x}{b:02x}")),
        ("rgb", format!("rgb({r}, {g}, {b})")),
        ("hsl", format!("hsl({h:.0}, {s:.0}%, {l:.0}%)")),
    ];
    let items: Vec<Value> = forms
        .iter()
        .map(|(id, v)| {
            json!({
                "id": id,
                "title": v,
                "subtitle": format!("{} · Enter copia", id.to_uppercase()),
                "data": v,
                "actions": [{ "id": "open", "label": "Copiar" }],
            })
        })
        .collect();
    json!({ "items": items })
}

fn main() {
    let stdin = std::io::stdin();
    let mut out = std::io::stdout().lock();
    for line in stdin.lock().lines() {
        let Ok(line) = line else { break };
        let Ok(msg) = serde_json::from_str::<Value>(&line) else {
            eprintln!("color: línea no válida");
            continue;
        };
        let params = &msg["params"];
        let result = match msg["method"].as_str() {
            Some("initialize") => json!({ "name": "color" }),
            Some("query") => query(params["text"].as_str().unwrap_or("")),
            Some("action") => json!({ "copy": params["item"]["data"] }),
            // `cancel` y métodos desconocidos: nada que responder.
            _ => continue,
        };
        if let Some(id) = msg.get("id") {
            let reply = json!({ "id": id, "result": result });
            if writeln!(out, "{reply}").and_then(|()| out.flush()).is_err() {
                break;
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_and_converts() {
        assert_eq!(parse("#ff8800"), Some((255, 136, 0)));
        assert_eq!(parse("f80"), Some((255, 136, 0)));
        assert_eq!(parse("rgb(255, 136, 0)"), Some((255, 136, 0)));
        assert_eq!(parse("255 136 0"), Some((255, 136, 0)));
        assert_eq!(parse("hsl 32 100 50"), Some((255, 136, 0)));
        assert_eq!(parse("hola"), None);
        let (h, s, l) = rgb_to_hsl(255, 136, 0);
        assert_eq!((h.round(), s.round(), l.round()), (32.0, 100.0, 50.0));
    }
}
