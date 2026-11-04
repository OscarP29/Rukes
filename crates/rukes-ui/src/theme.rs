//! Colores y medidas. En la fase 4 vendrán de los temas TOML (RF-192).

use iced::Color;

const fn rgb(r: u8, g: u8, b: u8) -> Color {
    Color::from_rgb8(r, g, b)
}

/// Texto principal.
pub const FG: Color = rgb(0xc0, 0xca, 0xf5);
/// Texto secundario.
pub const MUTED: Color = rgb(0x73, 0x7a, 0xa2);
/// Acento (cursor, selección de texto).
pub const ACCENT: Color = rgb(0x7a, 0xa2, 0xf7);
/// Fondo de la fila seleccionada.
pub const SELECTION: Color = Color::from_rgba(0.157, 0.204, 0.341, 0.85);
/// Separador.
pub const DIVIDER: Color = Color::from_rgba(0.337, 0.373, 0.537, 0.35);

/// Fondo sobre Mica/Acrílico (translúcido) o sólido (Windows 10 / `backdrop = "solid"`).
pub fn background(translucent: bool) -> Color {
    if translucent {
        Color::from_rgba(0.102, 0.106, 0.149, 0.55)
    } else {
        rgb(0x1a, 0x1b, 0x26)
    }
}

/// Alto del campo de búsqueda.
pub const SEARCH_H: f32 = 56.0;
/// Alto de fila.
pub const ROW_H: f32 = 48.0;
/// Margen inferior de la lista.
pub const LIST_PAD: f32 = 6.0;
/// Alto de la línea de aviso.
pub const NOTICE_H: f32 = 28.0;
/// Lado del icono (lógico).
pub const ICON: f32 = 28.0;
/// Lado del icono pedido al sistema (físico; cubre escalados de hasta 200 %).
pub const ICON_PX: u32 = 56;
