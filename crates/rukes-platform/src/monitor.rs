//! Monitores y colocación de ventanas de otras apps (RF-151).

use std::ffi::c_void;

use windows::Win32::Foundation::{HWND, LPARAM, RECT};
use windows::Win32::Graphics::Dwm::{DWMWA_EXTENDED_FRAME_BOUNDS, DwmGetWindowAttribute};
use windows::Win32::Graphics::Gdi::{
    EnumDisplayMonitors, GetMonitorInfoW, HDC, HMONITOR, MONITORINFO,
};
use windows::Win32::UI::WindowsAndMessaging::{
    GetWindowRect, IsIconic, IsZoomed, SW_MAXIMIZE, SW_RESTORE, SWP_NOACTIVATE, SWP_NOZORDER,
    SetWindowPos, ShowWindow,
};
use windows::core::BOOL;

use crate::Hwnd;

/// `MONITORINFOF_PRIMARY`.
const MONITORINFOF_PRIMARY: u32 = 1;

/// Rectángulo en píxeles físicos de pantalla.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct Rect {
    /// Izquierda.
    pub left: i32,
    /// Arriba.
    pub top: i32,
    /// Derecha (exclusiva).
    pub right: i32,
    /// Abajo (exclusiva).
    pub bottom: i32,
}

impl Rect {
    /// Ancho.
    pub fn width(&self) -> i32 {
        self.right - self.left
    }

    /// Alto.
    pub fn height(&self) -> i32 {
        self.bottom - self.top
    }
}

impl From<RECT> for Rect {
    fn from(r: RECT) -> Self {
        Self {
            left: r.left,
            top: r.top,
            right: r.right,
            bottom: r.bottom,
        }
    }
}

/// Un monitor.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Monitor {
    /// Área completa.
    pub rect: Rect,
    /// Área de trabajo (sin la barra de tareas).
    pub work: Rect,
    /// Es el principal.
    pub primary: bool,
}

unsafe extern "system" fn collect(mon: HMONITOR, _: HDC, _: *mut RECT, lparam: LPARAM) -> BOOL {
    // SAFETY: `lparam` es el puntero al Vec que pasa `monitors` y vive durante la enumeración.
    let out = unsafe { &mut *(lparam.0 as *mut Vec<Monitor>) };
    let mut mi = MONITORINFO {
        cbSize: size_of::<MONITORINFO>() as u32,
        ..Default::default()
    };
    // SAFETY: `mi` local con su tamaño.
    if unsafe { GetMonitorInfoW(mon, &mut mi) }.as_bool() {
        out.push(Monitor {
            rect: mi.rcMonitor.into(),
            work: mi.rcWork.into(),
            primary: mi.dwFlags & MONITORINFOF_PRIMARY != 0,
        });
    }
    true.into()
}

/// Monitores: el principal primero y luego de izquierda a derecha (y de arriba abajo).
pub fn monitors() -> Vec<Monitor> {
    let mut v: Vec<Monitor> = Vec::new();
    // SAFETY: el callback solo usa el puntero a `v` durante la llamada.
    let _ =
        unsafe { EnumDisplayMonitors(None, None, Some(collect), LPARAM((&raw mut v) as isize)) };
    v.sort_by_key(|m| (!m.primary, m.rect.left, m.rect.top));
    v
}

fn h(hwnd: Hwnd) -> HWND {
    HWND(hwnd as *mut _)
}

/// Borde invisible que Windows 10/11 añade alrededor de las ventanas (sombra de redimensionado):
/// `(izquierda, arriba, derecha, abajo)`.
fn invisible_border(hwnd: HWND) -> (i32, i32, i32, i32) {
    let mut outer = RECT::default();
    let mut visible = RECT::default();
    // SAFETY: estructuras locales con su tamaño; un hwnd no válido devuelve error.
    let ok = unsafe {
        GetWindowRect(hwnd, &mut outer).is_ok()
            && DwmGetWindowAttribute(
                hwnd,
                DWMWA_EXTENDED_FRAME_BOUNDS,
                (&raw mut visible).cast::<c_void>(),
                size_of::<RECT>() as u32,
            )
            .is_ok()
    };
    if !ok {
        return (0, 0, 0, 0);
    }
    (
        (visible.left - outer.left).max(0),
        (visible.top - outer.top).max(0),
        (outer.right - visible.right).max(0),
        (outer.bottom - visible.bottom).max(0),
    )
}

/// Coloca una ventana para que su parte visible ocupe `target`. Con `maximize`, la mueve al
/// monitor de `target` y la maximiza. Devuelve `false` si la ventana ya no existe.
pub fn place(hwnd: Hwnd, target: Rect, maximize: bool) -> bool {
    let hwnd = h(hwnd);
    // SAFETY: si el hwnd ya no existe, las llamadas fallan sin efectos.
    unsafe {
        if IsIconic(hwnd).as_bool() || IsZoomed(hwnd).as_bool() {
            let _ = ShowWindow(hwnd, SW_RESTORE);
        }
        let (l, t, r, b) = invisible_border(hwnd);
        let ok = SetWindowPos(
            hwnd,
            None,
            target.left - l,
            target.top - t,
            (target.width() + l + r).max(1),
            (target.height() + t + b).max(1),
            SWP_NOZORDER | SWP_NOACTIVATE,
        )
        .is_ok();
        if ok && maximize {
            let _ = ShowWindow(hwnd, SW_MAXIMIZE);
        }
        ok
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn lists_at_least_one_monitor_with_the_primary_first() {
        let m = monitors();
        assert!(!m.is_empty());
        assert!(m[0].primary);
        assert!(m[0].work.width() > 0 && m[0].work.height() > 0);
        assert_eq!(m.iter().filter(|m| m.primary).count(), 1);
    }
}
