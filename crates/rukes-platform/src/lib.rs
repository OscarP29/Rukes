//! Envoltorios seguros sobre la API de Windows (DWM, shell, hotkeys, iconos).
//!
//! Es el único crate del proyecto con código `unsafe`; cada bloque lleva su `// SAFETY:`.
//! Las funciones públicas son seguras y reciben los `HWND` como [`Hwnd`] (un `isize`).

pub mod appearance;
pub mod apps;
pub mod autostart;
pub mod clipboard;
pub mod clipwatch;
pub mod com;
pub mod dpapi;
pub mod dwm;
pub mod explorer;
pub mod hotkey;
pub mod http;
pub mod icons;
pub mod input;
pub mod instance;
pub mod job;
pub mod monitor;
pub mod power;
pub mod process;
pub mod shell;
pub mod sysinfo;
pub mod text;
pub mod time;
pub mod tray;
pub mod window;
pub mod winlist;

/// Handle de ventana como entero, para poder enviarlo entre hilos.
pub type Hwnd = isize;

/// Error de la plataforma.
#[derive(Debug, thiserror::Error)]
pub enum PlatformError {
    /// Error de la API de Windows.
    #[error("{context}: {source}")]
    Win32 {
        /// Qué se intentaba hacer.
        context: &'static str,
        /// Error original.
        source: windows::core::Error,
    },
    /// Entrada no válida.
    #[error("{0}")]
    Invalid(String),
}

/// Resultado de la plataforma.
pub type Result<T> = std::result::Result<T, PlatformError>;

pub(crate) trait Context<T> {
    fn ctx(self, context: &'static str) -> Result<T>;
}

impl<T> Context<T> for windows::core::Result<T> {
    fn ctx(self, context: &'static str) -> Result<T> {
        self.map_err(|source| PlatformError::Win32 { context, source })
    }
}

/// Convierte un texto a UTF-16 terminado en cero.
pub(crate) fn wide(s: &str) -> Vec<u16> {
    s.encode_utf16().chain(std::iter::once(0)).collect()
}
