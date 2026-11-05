//! Aspecto: tema, modo claro/oscuro, contraste alto, escalado de texto y animaciones del
//! sistema (RF-192 a RF-198, RF-214).

use std::path::Path;

use rukes_config::theme::{self, Rgba, Theme};
use rukes_config::{Config, ConfigError};
use rukes_platform::appearance::{self, HighContrast};
use rukes_platform::dwm::Backdrop;
use rukes_ui::UiStyle;

/// Estado del sistema que cambia el aspecto. Se vuelve a leer cada vez que se muestra la
/// ventana (son lecturas de registro baratas).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct System {
    pub dark: bool,
    pub high_contrast: Option<HighContrast>,
    pub text_scale: f32,
    pub animations: bool,
}

impl System {
    pub fn read() -> Self {
        Self {
            dark: appearance::system_dark_mode(),
            high_contrast: appearance::high_contrast(),
            text_scale: appearance::text_scale(),
            animations: appearance::animations_enabled(),
        }
    }
}

impl Default for System {
    fn default() -> Self {
        Self {
            dark: true,
            high_contrast: None,
            text_scale: 1.0,
            animations: true,
        }
    }
}

/// Nombre del tema que toca según la config y el modo del sistema.
pub fn theme_name<'a>(c: &'a Config, sys: &System) -> &'a str {
    theme::pick(
        &c.general.theme,
        &c.general.light_theme,
        &c.general.dark_theme,
        sys.dark,
    )
}

/// Carga el tema que toca.
pub fn load_theme(c: &Config, sys: &System, themes_dir: &Path) -> Result<Theme, ConfigError> {
    theme::load(theme_name(c, sys), themes_dir)
}

/// Fondo de la ventana: el de la config, el del tema (`auto`) o sólido con contraste alto.
pub fn backdrop(c: &Config, t: &Theme, sys: &System) -> Backdrop {
    if sys.high_contrast.is_some() {
        return Backdrop::Solid;
    }
    match c.window.backdrop.trim() {
        "" | "auto" => Backdrop::from_config(&t.backdrop),
        b => Backdrop::from_config(b),
    }
}

/// `true` si la ventana debe pedir a DWM el modo oscuro.
pub fn dark(t: &Theme, sys: &System) -> bool {
    match sys.high_contrast {
        // Con contraste alto manda el color de fondo del sistema.
        Some(hc) => luminance(Rgba::from_rgb(hc.window)) < 0.5,
        None => t.dark,
    }
}

fn luminance(Rgba([r, g, b, _]): Rgba) -> f32 {
    (0.2126 * f32::from(r) + 0.7152 * f32::from(g) + 0.0722 * f32::from(b)) / 255.0
}

/// Aspecto para la UI.
pub fn ui_style(t: &Theme, sys: &System, translucent: bool) -> UiStyle {
    let k = sys.text_scale.clamp(1.0, 2.25);
    let scaled = |v: f32| (v * k).round();
    let c = &t.colors;
    let mut s = UiStyle {
        background: c.background.0,
        opacity: t.opacity,
        solid_opacity: t.solid_opacity,
        translucent,
        foreground: c.foreground.0,
        muted: c.muted.0,
        accent: c.accent.0,
        selection: c.selection.0,
        selection_foreground: c.selection_foreground.0,
        divider: c.divider.0,
        surface: c.surface.map(|c| c.0),
        surface_hover: c.surface_hover.map(|c| c.0),
        text_faint: c.text_faint.map(|c| c.0),
        accent_soft: c.accent_soft.map(|c| c.0),
        font: t.font.family.clone(),
        mono: t.font.mono.clone(),
        font_size: (t.font.size * k * 2.0).round() / 2.0,
        row_height: scaled(t.layout.row_height),
        icon_size: scaled(t.layout.icon_size),
        search_height: scaled(t.layout.search_height),
        padding: t.layout.padding,
        spacing: t.layout.spacing,
        radius: t.radius,
    };
    // Contraste alto (RF-214): colores del sistema y fondo opaco.
    if let Some(hc) = sys.high_contrast {
        let text = Rgba::from_rgb(hc.text);
        s.background = Rgba::from_rgb(hc.window).0;
        s.foreground = text.0;
        s.muted = Rgba::from_rgb(hc.gray_text).0;
        s.accent = Rgba::from_rgb(hc.highlight).0;
        s.selection = Rgba::from_rgb(hc.highlight).0;
        s.selection_foreground = Rgba::from_rgb(hc.highlight_text).0;
        s.divider = text.0;
        // En contraste alto nada se deriva de los colores del tema.
        s.surface = Some(s.background);
        s.surface_hover = Some(s.selection);
        s.text_faint = Some(s.muted);
        s.accent_soft = Some(s.selection);
        s.opacity = 1.0;
        s.solid_opacity = 1.0;
        s.translucent = false;
    }
    s
}

#[cfg(test)]
mod tests {
    use super::*;

    fn hc() -> HighContrast {
        HighContrast {
            window: 0x000000,
            text: 0xffffff,
            highlight: 0x1aebff,
            highlight_text: 0x000000,
            gray_text: 0x3ff23f,
        }
    }

    #[test]
    fn system_theme_follows_mode_and_backdrop_auto_uses_theme() {
        let c = Config::default();
        let light = System {
            dark: false,
            ..System::default()
        };
        assert_eq!(theme_name(&c, &light), "rukes-light");
        assert_eq!(theme_name(&c, &System::default()), "rukes-dark");
        let t = Theme::default();
        assert_eq!(backdrop(&c, &t, &System::default()), Backdrop::Mica);
        let mut c2 = Config::default();
        c2.window.backdrop = "solid".into();
        assert_eq!(backdrop(&c2, &t, &System::default()), Backdrop::Solid);
    }

    #[test]
    fn high_contrast_overrides_colors_and_backdrop() {
        let sys = System {
            high_contrast: Some(hc()),
            ..System::default()
        };
        let t = Theme::default();
        let s = ui_style(&t, &sys, true);
        assert_eq!(s.background, [0, 0, 0, 255]);
        assert_eq!(s.foreground, [255, 255, 255, 255]);
        assert_eq!(s.selection_foreground, [0, 0, 0, 255]);
        assert!(!s.translucent);
        assert_eq!(s.solid_opacity, 1.0);
        assert_eq!(backdrop(&Config::default(), &t, &sys), Backdrop::Solid);
        assert!(dark(&t, &sys));
    }

    #[test]
    fn text_scale_grows_text_and_rows() {
        let sys = System {
            text_scale: 1.5,
            ..System::default()
        };
        let s = ui_style(&Theme::default(), &sys, false);
        assert_eq!(s.font_size, 22.5);
        assert_eq!(s.row_height, 72.0);
        assert_eq!(s.search_height, 84.0);
    }
}
