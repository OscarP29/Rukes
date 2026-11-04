//! Efectos de ventana de DWM: Mica/Acrílico, esquinas redondeadas, modo oscuro (RF-190, RF-191).

use windows::Win32::Foundation::HWND;
use windows::Win32::Graphics::Dwm::{
    DWM_SYSTEMBACKDROP_TYPE, DWMSBT_MAINWINDOW, DWMSBT_NONE, DWMSBT_TRANSIENTWINDOW,
    DWMWA_SYSTEMBACKDROP_TYPE, DWMWA_USE_IMMERSIVE_DARK_MODE, DWMWA_WINDOW_CORNER_PREFERENCE,
    DWMWCP_ROUND, DwmExtendFrameIntoClientArea, DwmSetWindowAttribute,
};
use windows::Win32::UI::Controls::MARGINS;
use windows::Win32::UI::WindowsAndMessaging::{
    GWL_STYLE, GetWindowLongPtrW, SWP_FRAMECHANGED, SWP_NOMOVE, SWP_NOSIZE, SWP_NOZORDER,
    SetWindowLongPtrW, SetWindowPos, WS_CAPTION, WS_MAXIMIZEBOX, WS_MINIMIZEBOX, WS_SYSMENU,
    WS_THICKFRAME,
};

use crate::Hwnd;

/// Fondo de la ventana.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Backdrop {
    /// Mica (Windows 11).
    Mica,
    /// Acrílico (Windows 11).
    Acrylic,
    /// Color sólido (o Windows 10).
    Solid,
}

impl Backdrop {
    /// Interpreta el valor de configuración; desconocido = Mica.
    pub fn from_config(s: &str) -> Self {
        match s {
            "acrylic" => Self::Acrylic,
            "solid" => Self::Solid,
            _ => Self::Mica,
        }
    }
}

fn set_attr<T>(hwnd: HWND, attr: windows::Win32::Graphics::Dwm::DWMWINDOWATTRIBUTE, v: &T) -> bool {
    // SAFETY: `v` apunta a un valor vivo de tamaño `size_of::<T>()`; hwnd es una ventana válida
    // (si no lo es, la API devuelve error sin efectos).
    unsafe {
        DwmSetWindowAttribute(hwnd, attr, (&raw const *v).cast(), size_of::<T>() as u32).is_ok()
    }
}

/// Quita los estilos de marco: si no, DWM pinta botones de título al extender el marco.
fn strip_frame(hwnd: HWND) {
    let strip = (WS_CAPTION | WS_SYSMENU | WS_MINIMIZEBOX | WS_MAXIMIZEBOX | WS_THICKFRAME).0;
    // SAFETY: lectura/escritura de estilo sobre una ventana de este proceso.
    unsafe {
        let style = GetWindowLongPtrW(hwnd, GWL_STYLE);
        if style & strip as isize != 0 {
            SetWindowLongPtrW(hwnd, GWL_STYLE, style & !(strip as isize));
            let _ = SetWindowPos(
                hwnd,
                None,
                0,
                0,
                0,
                0,
                SWP_FRAMECHANGED | SWP_NOMOVE | SWP_NOSIZE | SWP_NOZORDER,
            );
        }
    }
}

/// Aplica modo oscuro, esquinas redondeadas y el fondo. Devuelve el fondo realmente aplicado:
/// en Windows 10 (sin `DWMWA_SYSTEMBACKDROP_TYPE`) cae a [`Backdrop::Solid`] y la UI debe pintar
/// un fondo opaco.
pub fn apply(hwnd: Hwnd, backdrop: Backdrop, dark: bool) -> Backdrop {
    let hwnd = HWND(hwnd as *mut _);
    strip_frame(hwnd);
    let dark: i32 = dark.into();
    set_attr(hwnd, DWMWA_USE_IMMERSIVE_DARK_MODE, &dark);
    set_attr(hwnd, DWMWA_WINDOW_CORNER_PREFERENCE, &DWMWCP_ROUND);

    let kind: DWM_SYSTEMBACKDROP_TYPE = match backdrop {
        Backdrop::Mica => DWMSBT_MAINWINDOW,
        Backdrop::Acrylic => DWMSBT_TRANSIENTWINDOW,
        Backdrop::Solid => DWMSBT_NONE,
    };
    let applied = set_attr(hwnd, DWMWA_SYSTEMBACKDROP_TYPE, &kind);
    let margins = if applied && backdrop != Backdrop::Solid {
        MARGINS {
            cxLeftWidth: -1,
            cxRightWidth: -1,
            cyTopHeight: -1,
            cyBottomHeight: -1,
        }
    } else {
        MARGINS::default()
    };
    // SAFETY: hwnd válido y `margins` local.
    let _ = unsafe { DwmExtendFrameIntoClientArea(hwnd, &margins) };
    if applied { backdrop } else { Backdrop::Solid }
}
