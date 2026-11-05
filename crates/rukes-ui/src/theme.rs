//! Colores, fuentes y medidas en tipos de iced, a partir de [`UiStyle`] (RF-192).

use std::collections::HashMap;
use std::sync::Mutex;

use iced::{Color, Font};

use crate::{Rgba8, UiStyle};

/// Margen vertical de la lista.
pub const LIST_PAD: f32 = 8.0;
/// Alto de la línea de aviso.
pub const NOTICE_H: f32 = 28.0;

fn color([r, g, b, a]: Rgba8) -> Color {
    Color::from_rgba8(r, g, b, f32::from(a) / 255.0)
}

/// Caracteres que caben en `width` píxeles con texto de `size` (estimación para cortar con
/// «…»: iced no tiene elipsis).
pub fn fit_chars(width: f32, size: f32) -> usize {
    (width / (size * 0.53)).floor().max(4.0) as usize
}

/// `s` cortado al final con «…» si pasa de `max` caracteres.
pub fn ellipsis(s: &str, max: usize) -> String {
    if s.chars().count() <= max {
        return s.to_owned();
    }
    let mut out: String = s.chars().take(max.saturating_sub(1)).collect();
    out.push('…');
    out
}

/// `s` cortado por el medio con «…» (las rutas conservan el principio y el nombre final).
pub fn ellipsis_middle(s: &str, max: usize) -> String {
    let n = s.chars().count();
    if n <= max {
        return s.to_owned();
    }
    let keep = max.saturating_sub(1);
    let head = keep * 2 / 5;
    let tail = keep - head;
    let mut out: String = s.chars().take(head).collect();
    out.push('…');
    out.extend(s.chars().skip(n - tail));
    out
}

/// `true` si el texto parece una ruta (se corta por el medio).
pub fn looks_like_path(s: &str) -> bool {
    let b = s.as_bytes();
    s.starts_with("\\\\")
        || s.starts_with('/')
        || s.starts_with('~')
        || (b.len() > 2 && b[1] == b':' && (b[2] == b'\\' || b[2] == b'/'))
}

/// `a` mezclado con `b` en proporción `k` (0: `a`, 1: `b`), opaco.
fn mix(a: Color, b: Color, k: f32) -> Color {
    let m = |x: f32, y: f32| x + (y - x) * k;
    Color::from_rgb(m(a.r, b.r), m(a.g, b.g), m(a.b, b.b))
}

/// Fuente por nombre. iced pide `&'static str`: los nombres se guardan una sola vez (solo
/// cambian al editar el tema).
pub fn font(name: &str) -> Font {
    static NAMES: Mutex<Option<HashMap<String, &'static str>>> = Mutex::new(None);
    let mut names = NAMES
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner);
    let names = names.get_or_insert_with(HashMap::new);
    let s: &'static str = match names.get(name) {
        Some(s) => s,
        None => {
            let s: &'static str = Box::leak(name.to_owned().into_boxed_str());
            names.insert(name.to_owned(), s);
            s
        }
    };
    Font::with_name(s)
}

/// Aspecto listo para la vista.
#[derive(Debug, Clone, Copy)]
pub struct Look {
    pub bg: Color,
    pub fg: Color,
    pub muted: Color,
    pub accent: Color,
    pub selection: Color,
    pub sel_fg: Color,
    /// Texto secundario de la fila seleccionada.
    pub sel_muted: Color,
    /// Separadores y bordes (token `border`).
    pub divider: Color,
    /// Fondo de tarjetas y controles.
    pub surface: Color,
    /// Fondo al pasar el ratón.
    pub surface_hover: Color,
    /// Texto terciario: etiquetas pequeñas en mayúsculas.
    pub faint: Color,
    /// Acento suave: fondos que llevan el acento.
    pub accent_soft: Color,
    /// Error (derivado del tema: no hay token propio).
    pub danger: Color,
    /// Aviso.
    pub warning: Color,
    /// Correcto.
    pub success: Color,
    pub font: Font,
    pub mono: Font,
    pub size: f32,
    pub row_h: f32,
    pub icon: f32,
    pub search_h: f32,
    pub pad: f32,
    pub spacing: f32,
    pub radius: f32,
}

impl Look {
    pub fn new(s: &UiStyle, translucent: bool) -> Self {
        let base = color(s.background);
        let bg = Color {
            a: if translucent {
                s.opacity
            } else {
                s.solid_opacity
            },
            ..base
        };
        let fg = color(s.foreground);
        let sel_fg = color(s.selection_foreground);
        let muted = color(s.muted);
        let accent = color(s.accent);
        // Un token fijado en el tema lleva la opacidad del fondo (para no tapar Mica); uno
        // derivado es el texto con transparencia, que funciona sobre cualquier fondo.
        let token = |v: Option<Rgba8>, derived: Color| match v {
            Some(c) => {
                let c = color(c);
                Color { a: c.a * bg.a, ..c }
            }
            None => derived,
        };
        let dark = base.r * 0.2126 + base.g * 0.7152 + base.b * 0.0722 < 0.5;
        let (danger, warning, success) = if dark {
            (
                Color::from_rgb8(0xf7, 0x76, 0x8e),
                Color::from_rgb8(0xe0, 0xaf, 0x68),
                Color::from_rgb8(0x9e, 0xce, 0x6a),
            )
        } else {
            (
                Color::from_rgb8(0xc6, 0x28, 0x3c),
                Color::from_rgb8(0xa8, 0x5d, 0x00),
                Color::from_rgb8(0x2a, 0x7d, 0x2e),
            )
        };
        Self {
            bg,
            fg,
            muted,
            accent,
            selection: color(s.selection),
            sel_fg,
            // Con un texto de selección propio (temas claros con acento oscuro, contraste
            // alto) el subtítulo usa ese color; si no, el secundario de siempre.
            sel_muted: if s.selection_foreground == s.foreground {
                color(s.muted)
            } else {
                Color { a: 0.85, ..sel_fg }
            },
            divider: color(s.divider),
            surface: token(s.surface, Color { a: 0.045, ..fg }),
            surface_hover: token(s.surface_hover, Color { a: 0.085, ..fg }),
            faint: s.text_faint.map_or_else(|| mix(muted, base, 0.35), color),
            accent_soft: s.accent_soft.map_or(Color { a: 0.16, ..accent }, color),
            danger,
            warning,
            success,
            font: font(&s.font),
            mono: font(&s.mono),
            size: s.font_size,
            row_h: s.row_height,
            icon: s.icon_size,
            search_h: s.search_height,
            pad: s.padding,
            spacing: s.spacing,
            radius: s.radius,
        }
    }

    /// Lado del icono pedido al sistema en píxeles físicos (cubre escalados de hasta 200 %).
    pub fn icon_px(&self) -> u32 {
        (self.icon * 2.0).round().clamp(16.0, 256.0) as u32
    }

    /// Tamaño del texto secundario: rutas, pie y etiquetas (12 px con el tamaño de 15).
    pub fn small(&self) -> f32 {
        (self.size * 0.8).round().max(9.0)
    }

    /// Tamaño de las descripciones (13 px con el tamaño de 15).
    pub fn desc(&self) -> f32 {
        (self.size * 0.87).round().max(10.0)
    }

    /// Tamaño del campo de búsqueda.
    pub fn input_size(&self) -> f32 {
        (self.size * 1.33).round()
    }

    /// El mismo aspecto con el contenido a una opacidad (animación de aparición).
    pub fn faded(mut self, k: f32) -> Self {
        if k >= 1.0 {
            return self;
        }
        for c in [
            &mut self.fg,
            &mut self.muted,
            &mut self.accent,
            &mut self.selection,
            &mut self.sel_fg,
            &mut self.sel_muted,
            &mut self.divider,
            &mut self.surface,
            &mut self.surface_hover,
            &mut self.faint,
            &mut self.accent_soft,
            &mut self.danger,
            &mut self.warning,
            &mut self.success,
        ] {
            c.a *= k;
        }
        self
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn background_alpha_depends_on_backdrop() {
        let s = UiStyle::default();
        assert!((Look::new(&s, true).bg.a - 0.55).abs() < 1e-3);
        assert!((Look::new(&s, false).bg.a - 1.0).abs() < 1e-3);
        let f = Look::new(&s, true).faded(0.5);
        assert!((f.fg.a - 0.5).abs() < 1e-3);
        assert!((f.bg.a - 0.55).abs() < 1e-3);
        assert_eq!(Look::new(&s, true).icon_px(), 56);
    }

    #[test]
    fn missing_tokens_are_derived() {
        let mut s = UiStyle::default();
        let l = Look::new(&s, false);
        assert!(l.surface.a > 0.0 && l.surface.a < 0.2);
        assert!(l.surface_hover.a > l.surface.a);
        assert!(l.accent_soft.a < 0.5);
        // El terciario queda entre el secundario y el fondo.
        assert!(l.faint.r < l.muted.r);
        s.surface = Some([0x17, 0x1a, 0x21, 255]);
        let t = Look::new(&s, true);
        assert!(
            (t.surface.a - 0.55).abs() < 1e-3,
            "lleva la opacidad del fondo"
        );
        assert_eq!((l.small(), l.desc(), l.input_size()), (12.0, 13.0, 20.0));
    }

    #[test]
    fn ellipsis_cuts_at_the_end_or_the_middle() {
        assert_eq!(ellipsis("hola", 10), "hola");
        assert_eq!(ellipsis("abcdefghij", 5), "abcd…");
        let p = r"C:\Users\ana\Documentos\informe-final.docx";
        let m = ellipsis_middle(p, 20);
        assert_eq!(m.chars().count(), 20);
        assert!(m.starts_with("C:\\") && m.ends_with("final.docx"), "{m}");
        assert!(looks_like_path(p) && looks_like_path("~/x") && !looks_like_path("Tareas"));
        assert!(fit_chars(160.0, 13.0) > 20);
    }

    #[test]
    fn fonts_are_interned() {
        assert_eq!(font("Cascadia Mono"), font("Cascadia Mono"));
    }
}
