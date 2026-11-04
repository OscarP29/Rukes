//! Colores, fuentes y medidas en tipos de iced, a partir de [`UiStyle`] (RF-192).

use std::collections::HashMap;
use std::sync::Mutex;

use iced::{Color, Font};

use crate::{Rgba8, UiStyle};

/// Margen vertical de la lista.
pub const LIST_PAD: f32 = 6.0;
/// Alto de la línea de aviso.
pub const NOTICE_H: f32 = 28.0;

fn color([r, g, b, a]: Rgba8) -> Color {
    Color::from_rgba8(r, g, b, f32::from(a) / 255.0)
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
    pub divider: Color,
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
        Self {
            bg,
            fg,
            muted: color(s.muted),
            accent: color(s.accent),
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

    /// Tamaño del texto secundario.
    pub fn small(&self) -> f32 {
        (self.size * 0.83).round().max(9.0)
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
    fn fonts_are_interned() {
        assert_eq!(font("Cascadia Mono"), font("Cascadia Mono"));
    }
}
