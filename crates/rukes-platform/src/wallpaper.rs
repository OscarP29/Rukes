//! Fondo de pantalla del escritorio con `IDesktopWallpaper` (RF-180).

use std::path::{Path, PathBuf};

use windows::Win32::System::Com::{CLSCTX_ALL, CoCreateInstance, CoTaskMemFree};
use windows::Win32::UI::Shell::{
    DESKTOP_WALLPAPER_POSITION, DWPOS_CENTER, DWPOS_FILL, DWPOS_FIT, DWPOS_SPAN, DWPOS_STRETCH,
    DWPOS_TILE, DesktopWallpaper, IDesktopWallpaper,
};
use windows::core::{PCWSTR, PWSTR};

use crate::{Context as _, PlatformError, Result, wide};

/// Cómo se ajusta la imagen a la pantalla.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Position {
    /// Rellenar (recorta lo que sobra).
    #[default]
    Fill,
    /// Ajustar (deja franjas).
    Fit,
    /// Estirar.
    Stretch,
    /// Mosaico.
    Tile,
    /// Centrar.
    Center,
    /// Una sola imagen a lo largo de todos los monitores.
    Span,
}

impl Position {
    /// Interpreta `fill`, `fit`, `stretch`, `tile`, `center` o `span`.
    pub fn parse(s: &str) -> Option<Self> {
        Some(match s.trim().to_lowercase().as_str() {
            "fill" | "rellenar" => Self::Fill,
            "fit" | "ajustar" => Self::Fit,
            "stretch" | "estirar" => Self::Stretch,
            "tile" | "mosaico" => Self::Tile,
            "center" | "centrar" => Self::Center,
            "span" | "expandir" => Self::Span,
            _ => return None,
        })
    }

    fn raw(self) -> DESKTOP_WALLPAPER_POSITION {
        match self {
            Self::Fill => DWPOS_FILL,
            Self::Fit => DWPOS_FIT,
            Self::Stretch => DWPOS_STRETCH,
            Self::Tile => DWPOS_TILE,
            Self::Center => DWPOS_CENTER,
            Self::Span => DWPOS_SPAN,
        }
    }
}

fn api() -> Result<IDesktopWallpaper> {
    crate::com::ensure_thread();
    // SAFETY: creación de un objeto COM del shell en un hilo con COM inicializado.
    unsafe { CoCreateInstance(&DesktopWallpaper, None, CLSCTX_ALL) }.ctx("IDesktopWallpaper")
}

/// Toma una cadena reservada por COM y la libera.
fn take(p: PWSTR) -> String {
    if p.is_null() {
        return String::new();
    }
    // SAFETY: `p` es una cadena terminada en cero devuelta por el shell, que se libera una sola
    // vez con CoTaskMemFree tras copiarla.
    unsafe {
        let s = p.to_string().unwrap_or_default();
        CoTaskMemFree(Some(p.0 as *const _));
        s
    }
}

/// Fondo actual de cada monitor (en el orden del sistema; vacío si es un color sólido).
pub fn current() -> Result<Vec<PathBuf>> {
    let w = api()?;
    // SAFETY: métodos de IDesktopWallpaper sobre un objeto válido; las cadenas se liberan en
    // `take`.
    unsafe {
        let n = w
            .GetMonitorDevicePathCount()
            .ctx("GetMonitorDevicePathCount")?;
        let mut out = Vec::new();
        for i in 0..n {
            let Ok(id) = w.GetMonitorDevicePathAt(i) else {
                continue;
            };
            let id = take(id);
            let wid = wide(&id);
            let path = w
                .GetWallpaper(PCWSTR(wid.as_ptr()))
                .map(take)
                .unwrap_or_default();
            out.push(PathBuf::from(path));
        }
        Ok(out)
    }
}

/// Pone `image` de fondo en todos los monitores, con el ajuste `pos`.
pub fn set(image: &Path, pos: Position) -> Result<()> {
    if !image.is_file() {
        return Err(PlatformError::Invalid(format!(
            "no existe: {}",
            image.display()
        )));
    }
    let w = api()?;
    let path = wide(&image.to_string_lossy());
    // SAFETY: `path` vive durante la llamada; un monitor nulo aplica el fondo a todos.
    unsafe {
        w.SetPosition(pos.raw()).ctx("SetPosition")?;
        w.SetWallpaper(PCWSTR::null(), PCWSTR(path.as_ptr()))
            .ctx("SetWallpaper")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_positions() {
        assert_eq!(Position::parse("Fill"), Some(Position::Fill));
        assert_eq!(Position::parse("ajustar"), Some(Position::Fit));
        assert_eq!(Position::parse("span"), Some(Position::Span));
        assert_eq!(Position::parse("x"), None);
    }

    /// Solo lee: no cambia el fondo del usuario.
    #[test]
    fn reads_current_wallpaper() {
        let list = current().unwrap();
        assert!(!list.is_empty());
    }
}
