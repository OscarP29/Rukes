//! Preferencias de apariencia y accesibilidad del sistema (RF-195, RF-198, RF-214).
//!
//! Son lecturas baratas (registro y `SystemParametersInfo`): la aplicación las consulta cada
//! vez que muestra la ventana, así un cambio del sistema se aplica en la siguiente aparición.

use windows::Win32::Graphics::Gdi::{
    COLOR_GRAYTEXT, COLOR_HIGHLIGHT, COLOR_HIGHLIGHTTEXT, COLOR_WINDOW, COLOR_WINDOWTEXT,
    GetSysColor, SYS_COLOR_INDEX,
};
use windows::Win32::System::Registry::{HKEY_CURRENT_USER, RRF_RT_REG_DWORD, RegGetValueW};
use windows::Win32::UI::Accessibility::{HCF_HIGHCONTRASTON, HIGHCONTRASTW};
use windows::Win32::UI::WindowsAndMessaging::{
    SPI_GETCLIENTAREAANIMATION, SPI_GETHIGHCONTRAST, SYSTEM_PARAMETERS_INFO_UPDATE_FLAGS,
    SystemParametersInfoW,
};
use windows::core::BOOL;

use crate::wide;

fn reg_dword(key: &str, value: &str) -> Option<u32> {
    let (k, v) = (wide(key), wide(value));
    let mut data = 0u32;
    let mut size = size_of::<u32>() as u32;
    // SAFETY: buffer local de 4 bytes con su tamaño.
    let err = unsafe {
        RegGetValueW(
            HKEY_CURRENT_USER,
            windows::core::PCWSTR(k.as_ptr()),
            windows::core::PCWSTR(v.as_ptr()),
            RRF_RT_REG_DWORD,
            None,
            Some((&raw mut data).cast()),
            Some(&mut size),
        )
    };
    err.is_ok().then_some(data)
}

/// `true` si las aplicaciones usan el modo oscuro (Configuración → Personalización → Colores).
pub fn system_dark_mode() -> bool {
    reg_dword(
        r"Software\Microsoft\Windows\CurrentVersion\Themes\Personalize",
        "AppsUseLightTheme",
    )
    .is_some_and(|v| v == 0)
}

/// Factor de «Tamaño del texto» de Accesibilidad (1,0 a 2,25).
pub fn text_scale() -> f32 {
    reg_dword(r"Software\Microsoft\Accessibility", "TextScaleFactor")
        .map_or(1.0, |v| (v as f32 / 100.0).clamp(1.0, 2.25))
}

/// `false` si el usuario desactivó los efectos de animación de Windows.
pub fn animations_enabled() -> bool {
    let mut on = BOOL(1);
    // SAFETY: `on` es un BOOL local, el tipo que espera esta acción.
    let ok = unsafe {
        SystemParametersInfoW(
            SPI_GETCLIENTAREAANIMATION,
            0,
            Some((&raw mut on).cast()),
            SYSTEM_PARAMETERS_INFO_UPDATE_FLAGS(0),
        )
    };
    ok.is_err() || on.as_bool()
}

/// Colores del sistema en modo de alto contraste, como `0xRRGGBB`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct HighContrast {
    /// Fondo de ventana.
    pub window: u32,
    /// Texto de ventana.
    pub text: u32,
    /// Fondo de la selección.
    pub highlight: u32,
    /// Texto de la selección.
    pub highlight_text: u32,
    /// Texto atenuado.
    pub gray_text: u32,
}

fn sys_color(index: SYS_COLOR_INDEX) -> u32 {
    // SAFETY: índice de color válido; devuelve 0x00BBGGRR.
    let c = unsafe { GetSysColor(index) };
    ((c & 0xff) << 16) | (c & 0xff00) | ((c >> 16) & 0xff)
}

/// Colores de alto contraste si ese modo está activo.
pub fn high_contrast() -> Option<HighContrast> {
    let mut hc = HIGHCONTRASTW {
        cbSize: size_of::<HIGHCONTRASTW>() as u32,
        ..Default::default()
    };
    // SAFETY: estructura local con su tamaño.
    unsafe {
        SystemParametersInfoW(
            SPI_GETHIGHCONTRAST,
            hc.cbSize,
            Some((&raw mut hc).cast()),
            SYSTEM_PARAMETERS_INFO_UPDATE_FLAGS(0),
        )
    }
    .ok()?;
    (hc.dwFlags.0 & HCF_HIGHCONTRASTON.0 != 0).then(|| HighContrast {
        window: sys_color(COLOR_WINDOW),
        text: sys_color(COLOR_WINDOWTEXT),
        highlight: sys_color(COLOR_HIGHLIGHT),
        highlight_text: sys_color(COLOR_HIGHLIGHTTEXT),
        gray_text: sys_color(COLOR_GRAYTEXT),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn readings_are_sane() {
        let _ = system_dark_mode();
        let _ = animations_enabled();
        let s = text_scale();
        assert!((1.0..=2.25).contains(&s));
        if let Some(hc) = high_contrast() {
            assert_ne!(hc.window, hc.text);
        }
    }
}
