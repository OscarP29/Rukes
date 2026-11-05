//! Temas en TOML (RF-192, RF-194).
//!
//! Un tema puede heredar de otro con `extends` y cambiar solo algunos valores. Los temas del
//! usuario están en la carpeta `themes` de la configuración y sustituyen a los incluidos con el
//! mismo nombre de archivo.
//!
//! ```toml
//! [theme]
//! name = "Mi tema"
//! extends = "tokyo-night"
//! radius = 12
//!
//! [theme.colors]
//! accent = "#ff9e64"
//! ```

use std::fmt;
use std::path::Path;

use serde::Deserialize;
use serde::de::{self, Deserializer};

use crate::ConfigError;

/// Color RGBA.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Rgba(pub [u8; 4]);

impl Rgba {
    /// Interpreta `#rgb`, `#rrggbb` o `#rrggbbaa`.
    pub fn parse(s: &str) -> Option<Self> {
        let h = s.trim().strip_prefix('#')?;
        let byte = |i: usize| u8::from_str_radix(h.get(i..i + 2)?, 16).ok();
        let nib = |i: usize| {
            let c = u8::from_str_radix(h.get(i..=i)?, 16).ok()?;
            Some(c * 17)
        };
        Some(Self(match h.len() {
            3 => [nib(0)?, nib(1)?, nib(2)?, 255],
            6 => [byte(0)?, byte(2)?, byte(4)?, 255],
            8 => [byte(0)?, byte(2)?, byte(4)?, byte(6)?],
            _ => return None,
        }))
    }

    /// Desde `0xRRGGBB`.
    pub fn from_rgb(v: u32) -> Self {
        Self([(v >> 16) as u8, (v >> 8) as u8, v as u8, 255])
    }

    /// El mismo color con otra opacidad (0..1).
    pub fn with_alpha(self, a: f32) -> Self {
        let [r, g, b, _] = self.0;
        Self([r, g, b, (a.clamp(0.0, 1.0) * 255.0).round() as u8])
    }
}

impl fmt::Display for Rgba {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let [r, g, b, a] = self.0;
        write!(f, "#{r:02x}{g:02x}{b:02x}{a:02x}")
    }
}

impl<'de> Deserialize<'de> for Rgba {
    fn deserialize<D: Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        let s = String::deserialize(d)?;
        Self::parse(&s).ok_or_else(|| {
            de::Error::custom(format!("color no válido «{s}» (usa #rrggbb o #rrggbbaa)"))
        })
    }
}

/// Colores de un tema.
#[derive(Debug, Clone, PartialEq)]
pub struct Colors {
    /// Fondo.
    pub background: Rgba,
    /// Texto.
    pub foreground: Rgba,
    /// Texto secundario.
    pub muted: Rgba,
    /// Acento (cursor, iconos de texto).
    pub accent: Rgba,
    /// Fondo de la fila seleccionada.
    pub selection: Rgba,
    /// Texto de la fila seleccionada.
    pub selection_foreground: Rgba,
    /// Separadores.
    pub divider: Rgba,
}

/// Fuentes.
#[derive(Debug, Clone, PartialEq)]
pub struct Fonts {
    /// Fuente principal (cualquier fuente instalada).
    pub family: String,
    /// Tamaño del título de los resultados (el resto escala a partir de él).
    pub size: f32,
    /// Fuente monoespaciada (iconos de texto, código en la previsualización).
    pub mono: String,
}

/// Medidas.
#[derive(Debug, Clone, PartialEq)]
pub struct Layout {
    /// Alto de fila.
    pub row_height: f32,
    /// Lado del icono.
    pub icon_size: f32,
    /// Alto del campo de búsqueda.
    pub search_height: f32,
    /// Margen interior horizontal.
    pub padding: f32,
    /// Separación entre icono y texto.
    pub spacing: f32,
}

/// Tema resuelto, con todos los valores.
#[derive(Debug, Clone, PartialEq)]
pub struct Theme {
    /// Nombre visible.
    pub name: String,
    /// `true` si es oscuro (modo oscuro de DWM y de Mica).
    pub dark: bool,
    /// Fondo preferido: `mica`, `acrylic` o `solid`.
    pub backdrop: String,
    /// Opacidad del fondo sobre Mica/Acrílico.
    pub opacity: f32,
    /// Opacidad del fondo cuando es sólido (Windows 10 o `solid`).
    pub solid_opacity: f32,
    /// Radio de filas e iconos.
    pub radius: f32,
    /// Colores.
    pub colors: Colors,
    /// Fuentes.
    pub font: Fonts,
    /// Medidas.
    pub layout: Layout,
}

impl Default for Theme {
    /// Tokyo Night con las medidas por defecto.
    fn default() -> Self {
        let muted = Rgba::parse("#737aa2").unwrap_or(Rgba([115, 122, 162, 255]));
        let fg = Rgba([0xc0, 0xca, 0xf5, 255]);
        Self {
            name: "Tokyo Night".into(),
            dark: true,
            backdrop: "mica".into(),
            opacity: 0.55,
            solid_opacity: 1.0,
            radius: 8.0,
            colors: Colors {
                background: Rgba([0x1a, 0x1b, 0x26, 255]),
                foreground: fg,
                muted,
                accent: Rgba([0x7a, 0xa2, 0xf7, 255]),
                selection: Rgba([0x28, 0x34, 0x57, 217]),
                selection_foreground: fg,
                divider: muted.with_alpha(0.35),
            },
            font: Fonts {
                family: "Segoe UI Variable Text".into(),
                size: 15.0,
                mono: "Cascadia Mono".into(),
            },
            layout: Layout {
                row_height: 48.0,
                icon_size: 28.0,
                search_height: 56.0,
                padding: 12.0,
                spacing: 12.0,
            },
        }
    }
}

#[derive(Deserialize, Default)]
#[serde(deny_unknown_fields)]
struct FileColors {
    background: Option<Rgba>,
    foreground: Option<Rgba>,
    muted: Option<Rgba>,
    accent: Option<Rgba>,
    selection: Option<Rgba>,
    selection_foreground: Option<Rgba>,
    divider: Option<Rgba>,
}

#[derive(Deserialize, Default)]
#[serde(deny_unknown_fields)]
struct FileFont {
    family: Option<String>,
    size: Option<f32>,
    mono: Option<String>,
}

#[derive(Deserialize, Default)]
#[serde(deny_unknown_fields)]
struct FileLayout {
    row_height: Option<f32>,
    icon_size: Option<f32>,
    search_height: Option<f32>,
    padding: Option<f32>,
    spacing: Option<f32>,
}

#[derive(Deserialize, Default)]
#[serde(deny_unknown_fields)]
struct FileTheme {
    name: Option<String>,
    extends: Option<String>,
    dark: Option<bool>,
    backdrop: Option<String>,
    opacity: Option<f32>,
    solid_opacity: Option<f32>,
    radius: Option<f32>,
    #[serde(default)]
    colors: FileColors,
    #[serde(default)]
    font: FileFont,
    #[serde(default)]
    layout: FileLayout,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct ThemeDoc {
    theme: FileTheme,
}

/// Temas incluidos: `(nombre de archivo, TOML)`.
pub const BUILTIN: &[(&str, &str)] = &[
    (
        "tokyo-night",
        include_str!("../../../themes/tokyo-night.toml"),
    ),
    (
        "catppuccin-latte",
        include_str!("../../../themes/catppuccin-latte.toml"),
    ),
    (
        "catppuccin-frappe",
        include_str!("../../../themes/catppuccin-frappe.toml"),
    ),
    (
        "catppuccin-macchiato",
        include_str!("../../../themes/catppuccin-macchiato.toml"),
    ),
    (
        "catppuccin-mocha",
        include_str!("../../../themes/catppuccin-mocha.toml"),
    ),
    ("gruvbox", include_str!("../../../themes/gruvbox.toml")),
    ("nord", include_str!("../../../themes/nord.toml")),
    ("dracula", include_str!("../../../themes/dracula.toml")),
    ("midnight", include_str!("../../../themes/midnight.toml")),
    (
        "rukes-light",
        include_str!("../../../themes/rukes-light.toml"),
    ),
];

/// Nombre especial: sigue el modo claro/oscuro del sistema (RF-198).
pub const SYSTEM: &str = "system";

/// Temas que se pueden elegir: `(nombre de archivo, nombre visible, oscuro)`. Primero los
/// incluidos y después los de `user_dir`; los que no cargan no aparecen.
pub fn available(user_dir: &Path) -> Vec<(String, String, bool)> {
    let mut ids: Vec<String> = BUILTIN.iter().map(|(k, _)| (*k).to_owned()).collect();
    if let Ok(entries) = std::fs::read_dir(user_dir) {
        let mut user: Vec<String> = entries
            .flatten()
            .map(|e| e.path())
            .filter(|p| {
                p.extension()
                    .is_some_and(|x| x.eq_ignore_ascii_case("toml"))
            })
            .filter_map(|p| p.file_stem().map(|s| normalize(&s.to_string_lossy())))
            .filter(|id| !ids.contains(id))
            .collect();
        user.sort();
        ids.extend(user);
    }
    ids.into_iter()
        .filter_map(|id| load(&id, user_dir).ok().map(|t| (id, t.name, t.dark)))
        .collect()
}

/// Normaliza un nombre de tema: `Tokyo Night` → `tokyo-night`.
pub fn normalize(name: &str) -> String {
    name.trim()
        .to_lowercase()
        .replace([' ', '_'], "-")
        .replace('é', "e")
}

/// Elige el tema: `system` usa `light`/`dark` según el modo del sistema.
pub fn pick<'a>(theme: &'a str, light: &'a str, dark: &'a str, system_dark: bool) -> &'a str {
    if normalize(theme) == SYSTEM {
        if system_dark { dark } else { light }
    } else {
        theme
    }
}

fn source(name: &str, user_dir: &Path) -> Option<(String, std::path::PathBuf)> {
    let n = normalize(name);
    let user = user_dir.join(format!("{n}.toml"));
    if let Ok(text) = std::fs::read_to_string(&user) {
        return Some((text, user));
    }
    BUILTIN
        .iter()
        .find(|(k, _)| *k == n)
        .map(|(k, t)| ((*t).to_owned(), format!("<{k}>").into()))
}

fn error(path: &Path, text: &str, e: &toml::de::Error) -> ConfigError {
    let (line, column) = e
        .span()
        .map(|s| crate::line_col(text, s.start))
        .unwrap_or((0, 0));
    ConfigError {
        path: path.to_owned(),
        line,
        column,
        message: e.message().trim().to_owned(),
    }
}

fn plain_error(path: &Path, message: String) -> ConfigError {
    ConfigError {
        path: path.to_owned(),
        line: 0,
        column: 0,
        message,
    }
}

/// Carga un tema por nombre (con su cadena de `extends`).
pub fn load(name: &str, user_dir: &Path) -> Result<Theme, ConfigError> {
    load_depth(name, user_dir, 0)
}

fn load_depth(name: &str, user_dir: &Path, depth: usize) -> Result<Theme, ConfigError> {
    let (text, path) = source(name, user_dir).ok_or_else(|| {
        let mut names: Vec<&str> = BUILTIN.iter().map(|(k, _)| *k).collect();
        names.push(SYSTEM);
        plain_error(
            &user_dir.join(format!("{}.toml", normalize(name))),
            format!(
                "tema «{name}» no encontrado (incluidos: {})",
                names.join(", ")
            ),
        )
    })?;
    let doc: ThemeDoc = toml::from_str(&text).map_err(|e| error(&path, &text, &e))?;
    let f = doc.theme;
    let base_name = f.extends.as_deref().map(normalize);
    let mut t = match &base_name {
        Some(b) if depth >= 4 || *b == normalize(name) => {
            return Err(plain_error(
                &path,
                format!("«extends = \"{b}\"» forma un ciclo"),
            ));
        }
        Some(b) => load_depth(b, user_dir, depth + 1)?,
        None => Theme::default(),
    };
    let explicit_divider = f.colors.divider.is_some();
    let explicit_sel_fg = f.colors.selection_foreground.is_some();
    if let Some(v) = f.name {
        t.name = v;
    }
    if let Some(v) = f.dark {
        t.dark = v;
    }
    if let Some(v) = f.backdrop {
        if !matches!(v.as_str(), "mica" | "acrylic" | "solid") {
            return Err(plain_error(
                &path,
                format!("backdrop «{v}» no válido (mica, acrylic o solid)"),
            ));
        }
        t.backdrop = v;
    }
    let unit = |v: f32| v.clamp(0.0, 1.0);
    if let Some(v) = f.opacity {
        t.opacity = unit(v);
    }
    if let Some(v) = f.solid_opacity {
        t.solid_opacity = unit(v);
    }
    if let Some(v) = f.radius {
        t.radius = v.clamp(0.0, 24.0);
    }
    let c = f.colors;
    let fg_changed = c.foreground.is_some();
    let muted_changed = c.muted.is_some();
    macro_rules! set {
        ($($field:ident),*) => { $( if let Some(v) = c.$field { t.colors.$field = v; } )* };
    }
    set!(
        background,
        foreground,
        muted,
        accent,
        selection,
        selection_foreground,
        divider
    );
    // Colores derivados que el tema no fija explícitamente.
    if !explicit_divider && (muted_changed || base_name.is_none()) {
        t.colors.divider = t.colors.muted.with_alpha(0.35);
    }
    if !explicit_sel_fg && (fg_changed || base_name.is_none()) {
        t.colors.selection_foreground = t.colors.foreground;
    }
    if let Some(v) = f.font.family.filter(|s| !s.trim().is_empty()) {
        t.font.family = v;
    }
    if let Some(v) = f.font.size {
        t.font.size = v.clamp(9.0, 32.0);
    }
    if let Some(v) = f.font.mono.filter(|s| !s.trim().is_empty()) {
        t.font.mono = v;
    }
    let l = f.layout;
    if let Some(v) = l.row_height {
        t.layout.row_height = v.clamp(28.0, 96.0);
    }
    if let Some(v) = l.icon_size {
        t.layout.icon_size = v.clamp(12.0, 64.0);
    }
    if let Some(v) = l.search_height {
        t.layout.search_height = v.clamp(36.0, 96.0);
    }
    if let Some(v) = l.padding {
        t.layout.padding = v.clamp(0.0, 40.0);
    }
    if let Some(v) = l.spacing {
        t.layout.spacing = v.clamp(0.0, 40.0);
    }
    Ok(t)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn no_user_dir() -> std::path::PathBuf {
        std::env::temp_dir().join("rukes-no-themes")
    }

    #[test]
    fn colors_parse() {
        assert_eq!(Rgba::parse("#fff"), Some(Rgba([255, 255, 255, 255])));
        assert_eq!(Rgba::parse("#1a1b26"), Some(Rgba([0x1a, 0x1b, 0x26, 255])));
        assert_eq!(
            Rgba::parse("#1a1b2680"),
            Some(Rgba([0x1a, 0x1b, 0x26, 0x80]))
        );
        assert_eq!(Rgba::parse("1a1b26"), None);
        assert_eq!(Rgba::parse("#zzzzzz"), None);
        assert_eq!(Rgba::from_rgb(0x123456), Rgba([0x12, 0x34, 0x56, 255]));
    }

    #[test]
    fn available_lists_builtin_and_user_themes() {
        let dir = std::env::temp_dir().join(format!("rukes-avail-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(
            dir.join("mi-tema.toml"),
            "[theme]\nname = \"Mi tema\"\nextends = \"midnight\"\n",
        )
        .unwrap();
        std::fs::write(dir.join("roto.toml"), "esto no es toml [").unwrap();
        let list = available(&dir);
        assert_eq!(list.len(), BUILTIN.len() + 1);
        assert!(list.contains(&("midnight".into(), "Midnight".into(), true)));
        assert_eq!(
            list.last().unwrap(),
            &("mi-tema".into(), "Mi tema".into(), true)
        );
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn every_builtin_theme_loads() {
        for (k, _) in BUILTIN {
            let t = load(k, &no_user_dir()).unwrap_or_else(|e| panic!("{k}: {e}"));
            assert!(!t.name.is_empty());
            assert_ne!(t.colors.background, t.colors.foreground, "{k}");
            assert_eq!(t.colors.divider, t.colors.muted.with_alpha(0.35), "{k}");
            assert_eq!(t.colors.selection_foreground, t.colors.foreground, "{k}");
        }
        assert!(!load("catppuccin-latte", &no_user_dir()).unwrap().dark);
        assert_eq!(
            load("Tokyo Night", &no_user_dir()).unwrap().name,
            "Tokyo Night"
        );
        assert_eq!(
            load("catppuccin frappé", &no_user_dir()).unwrap().name,
            "Catppuccin Frappé"
        );
    }

    #[test]
    fn user_themes_extend_and_override() {
        let dir = std::env::temp_dir().join(format!("rukes-themes-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(
            dir.join("mio.toml"),
            "[theme]\nname = \"Mío\"\nextends = \"dracula\"\nradius = 12\n[theme.colors]\naccent = \"#ff9e64\"\n[theme.font]\nsize = 17\n",
        )
        .unwrap();
        let t = load("mio", &dir).unwrap();
        let d = load("dracula", &dir).unwrap();
        assert_eq!(t.name, "Mío");
        assert_eq!(t.radius, 12.0);
        assert_eq!(t.colors.accent, Rgba::parse("#ff9e64").unwrap());
        assert_eq!(t.colors.background, d.colors.background);
        assert_eq!(t.colors.divider, d.colors.divider);
        assert_eq!(t.font.size, 17.0);
        // Un archivo del usuario sustituye al incluido.
        std::fs::write(dir.join("nord.toml"), "[theme]\nname = \"Nord propio\"\n").unwrap();
        assert_eq!(load("nord", &dir).unwrap().name, "Nord propio");
        // Errores con posición.
        std::fs::write(
            dir.join("malo.toml"),
            "[theme]\n[theme.colors]\naccent = \"azul\"\n",
        )
        .unwrap();
        let e = load("malo", &dir).unwrap_err();
        assert_eq!(e.line, 3);
        assert!(e.message.contains("azul"));
        std::fs::write(dir.join("raro.toml"), "[theme]\ncolour = 1\n").unwrap();
        assert!(load("raro", &dir).is_err());
        std::fs::write(dir.join("ciclo.toml"), "[theme]\nextends = \"ciclo\"\n").unwrap();
        assert!(load("ciclo", &dir).unwrap_err().message.contains("ciclo"));
        assert!(
            load("no-existe", &dir)
                .unwrap_err()
                .message
                .contains("tokyo-night")
        );
        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn system_picks_by_mode() {
        assert_eq!(pick("system", "latte", "mocha", true), "mocha");
        assert_eq!(pick("System", "latte", "mocha", false), "latte");
        assert_eq!(pick("nord", "latte", "mocha", false), "nord");
    }
}
