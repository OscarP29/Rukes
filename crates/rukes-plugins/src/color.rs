//! Conversor de colores con muestra (RF-075): `#ff5733`, `rgb(255, 87, 51)`, `hsl(11, 100%, 60%)`.

use rukes_core::i18n::tr;
use rukes_core::{
    ActionId, ActionOutcome, CancelToken, Icon, Plugin, PluginError, PluginMetadata, Query,
    ResultItem,
};

use crate::util;

/// Color RGB.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Rgb(pub u8, pub u8, pub u8);

impl Rgb {
    /// `#rrggbb`.
    pub fn hex(self) -> String {
        format!("#{:02x}{:02x}{:02x}", self.0, self.1, self.2)
    }

    /// Tono (0..360), saturación y luminosidad (0..100).
    pub fn hsl(self) -> (f64, f64, f64) {
        let (r, g, b) = (
            f64::from(self.0) / 255.0,
            f64::from(self.1) / 255.0,
            f64::from(self.2) / 255.0,
        );
        let (max, min) = (r.max(g).max(b), r.min(g).min(b));
        let l = (max + min) / 2.0;
        let d = max - min;
        if d == 0.0 {
            return (0.0, 0.0, l * 100.0);
        }
        let s = d / (1.0 - (2.0 * l - 1.0).abs());
        (hue(r, g, b, max, d), s * 100.0, l * 100.0)
    }

    /// Tono, saturación y valor (0..100).
    pub fn hsv(self) -> (f64, f64, f64) {
        let (r, g, b) = (
            f64::from(self.0) / 255.0,
            f64::from(self.1) / 255.0,
            f64::from(self.2) / 255.0,
        );
        let (max, min) = (r.max(g).max(b), r.min(g).min(b));
        let d = max - min;
        let h = if d == 0.0 { 0.0 } else { hue(r, g, b, max, d) };
        let s = if max == 0.0 { 0.0 } else { d / max };
        (h, s * 100.0, max * 100.0)
    }
}

fn hue(r: f64, g: f64, b: f64, max: f64, d: f64) -> f64 {
    if max == r {
        60.0 * ((g - b) / d).rem_euclid(6.0)
    } else if max == g {
        60.0 * ((b - r) / d + 2.0)
    } else {
        60.0 * ((r - g) / d + 4.0)
    }
}

fn from_hsl(h: f64, s: f64, l: f64) -> Rgb {
    let (s, l) = (s.clamp(0.0, 100.0) / 100.0, l.clamp(0.0, 100.0) / 100.0);
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
    Rgb(f(r), f(g), f(b))
}

fn numbers(s: &str) -> Vec<f64> {
    s.split(|c: char| !(c.is_ascii_digit() || c == '.'))
        .filter(|p| !p.is_empty())
        .filter_map(|p| p.parse().ok())
        .collect()
}

/// Interpreta un color escrito: `#rgb`, `#rrggbb`, `rgb(r, g, b)` o `hsl(h, s%, l%)`. Exige el
/// formato completo para no confundirse con otras búsquedas.
pub fn parse(text: &str) -> Option<Rgb> {
    let t = text.trim().to_ascii_lowercase();
    if let Some(hex) = t.strip_prefix('#') {
        if !matches!(hex.len(), 3 | 6) || !hex.chars().all(|c| c.is_ascii_hexdigit()) {
            return None;
        }
        let full: String = if hex.len() == 3 {
            hex.chars().flat_map(|c| [c, c]).collect()
        } else {
            hex.to_owned()
        };
        let v = u32::from_str_radix(&full, 16).ok()?;
        return Some(Rgb((v >> 16) as u8, (v >> 8) as u8, v as u8));
    }
    let (kind, rest) = t.split_once('(')?;
    let inner = rest.strip_suffix(')')?;
    let n = numbers(inner);
    let [a, b, c] = n[..] else { return None };
    match kind.trim() {
        "rgb" if [a, b, c].iter().all(|v| *v <= 255.0) => Some(Rgb(a as u8, b as u8, c as u8)),
        "hsl" if b <= 100.0 && c <= 100.0 => Some(from_hsl(a, b, c)),
        _ => None,
    }
}

/// Plugin de colores: participa en la búsqueda global solo si la consulta es un color.
pub struct ColorPlugin;

impl ColorPlugin {
    /// Resultados para un texto.
    pub fn search(text: &str) -> Vec<ResultItem> {
        let Some(c) = parse(text) else {
            return Vec::new();
        };
        let (h, s, l) = c.hsl();
        let (hv, sv, v) = c.hsv();
        let forms = [
            ("hex", c.hex()),
            ("rgb", format!("rgb({}, {}, {})", c.0, c.1, c.2)),
            ("hsl", format!("hsl({h:.0}, {s:.0}%, {l:.0}%)")),
            ("hsv", format!("hsv({hv:.0}, {sv:.0}%, {v:.0}%)")),
        ];
        forms
            .iter()
            .enumerate()
            .map(|(i, (id, value))| {
                let mut it = ResultItem::new(*id, value.clone());
                it.subtitle = format!("{} · {}", id.to_uppercase(), tr("calc.enter_copies"));
                it.icon = Icon::Named(format!("swatch:{}", c.hex()));
                it.fuzzy = 1.0 - i as f32 * 0.01;
                it.payload = value.clone();
                it.detail = Some(
                    forms
                        .iter()
                        .map(|(k, v)| format!("{}: {v}", k.to_uppercase()))
                        .collect::<Vec<_>>()
                        .join("\n"),
                );
                it
            })
            .collect()
    }
}

impl Plugin for ColorPlugin {
    fn metadata(&self) -> PluginMetadata {
        PluginMetadata {
            id: "color".into(),
            name: tr("color.name").into(),
            prefix: None,
            keywords: Vec::new(),
            priority: 0.5,
            global: true,
            list_on_keyword: false,
        }
    }

    fn query(&self, q: &Query, _c: &CancelToken, sink: &mut dyn FnMut(Vec<ResultItem>)) {
        sink(Self::search(&q.text));
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

    #[test]
    fn parses_and_converts() {
        assert_eq!(parse("#ff5733"), Some(Rgb(255, 87, 51)));
        assert_eq!(parse("#F53"), Some(Rgb(255, 85, 51)));
        assert_eq!(parse("rgb(255, 87, 51)"), Some(Rgb(255, 87, 51)));
        assert_eq!(parse("hsl(11, 100%, 60%)"), Some(Rgb(255, 88, 51)));
        assert_eq!(parse("ff5733"), None, "sin # no es un color");
        assert_eq!(parse("#hola"), None);
        assert_eq!(parse("rgb(300, 0, 0)"), None);
        let (h, s, l) = Rgb(255, 87, 51).hsl();
        assert_eq!((h.round(), s.round(), l.round()), (11.0, 100.0, 60.0));
        let (h, s, v) = Rgb(255, 87, 51).hsv();
        assert_eq!((h.round(), s.round(), v.round()), (11.0, 80.0, 100.0));
    }

    #[test]
    fn results_carry_a_swatch() {
        let _ = rukes_core::i18n::set_language("es", "");
        let r = ColorPlugin::search("#ff5733");
        assert_eq!(r.len(), 4);
        assert_eq!(r[1].title, "rgb(255, 87, 51)");
        assert_eq!(r[0].icon, Icon::Named("swatch:#ff5733".into()));
        assert!(ColorPlugin::search("hola").is_empty());
    }
}
