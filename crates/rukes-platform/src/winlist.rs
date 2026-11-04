//! Ventanas de nivel superior abiertas (RF-050).

use std::ffi::c_void;
use std::path::PathBuf;

use windows::Win32::Foundation::{HWND, LPARAM};
use windows::Win32::Graphics::Dwm::{DWMWA_CLOAKED, DwmGetWindowAttribute};
use windows::Win32::UI::WindowsAndMessaging::{
    EnumWindows, GW_OWNER, GWL_EXSTYLE, GetWindow, GetWindowLongPtrW, GetWindowTextLengthW,
    GetWindowTextW, GetWindowThreadProcessId, IsIconic, IsWindowVisible, IsZoomed, SW_RESTORE,
    SWP_NOACTIVATE, SWP_NOZORDER, SetForegroundWindow, SetWindowPos, ShowWindow, WS_EX_APPWINDOW,
    WS_EX_TOOLWINDOW,
};
use windows::core::BOOL;

use crate::Hwnd;

/// Ventana visible en la barra de tareas o en Alt+Tab.
#[derive(Debug, Clone)]
pub struct WindowInfo {
    /// Handle.
    pub hwnd: Hwnd,
    /// Título.
    pub title: String,
    /// Proceso propietario.
    pub pid: u32,
    /// Ejecutable del proceso, si se puede leer.
    pub exe: Option<PathBuf>,
    /// Minimizada.
    pub minimized: bool,
}

fn is_alt_tab_window(hwnd: HWND) -> bool {
    // SAFETY: consultas sin efectos sobre un hwnd recibido de EnumWindows.
    unsafe {
        if !IsWindowVisible(hwnd).as_bool() || GetWindowTextLengthW(hwnd) == 0 {
            return false;
        }
        let ex = GetWindowLongPtrW(hwnd, GWL_EXSTYLE) as u32;
        if ex & WS_EX_TOOLWINDOW.0 != 0 {
            return false;
        }
        let owned = GetWindow(hwnd, GW_OWNER).is_ok_and(|o| !o.is_invalid());
        if owned && ex & WS_EX_APPWINDOW.0 == 0 {
            return false;
        }
        // Ventanas «ocultas» por DWM (apps UWP suspendidas, otros escritorios virtuales).
        let mut cloaked = 0u32;
        let _ = DwmGetWindowAttribute(
            hwnd,
            DWMWA_CLOAKED,
            (&raw mut cloaked).cast::<c_void>(),
            size_of::<u32>() as u32,
        );
        cloaked == 0
    }
}

unsafe extern "system" fn collect(hwnd: HWND, lparam: LPARAM) -> BOOL {
    // SAFETY: `lparam` es el puntero al Vec que pasa `list` y vive durante EnumWindows.
    let out = unsafe { &mut *(lparam.0 as *mut Vec<HWND>) };
    if is_alt_tab_window(hwnd) {
        out.push(hwnd);
    }
    true.into()
}

/// Ventanas abiertas en orden Z (la más reciente primero), sin las del proceso actual.
pub fn list() -> Vec<WindowInfo> {
    let mut hwnds: Vec<HWND> = Vec::new();
    // SAFETY: el callback solo usa el puntero a `hwnds` durante la llamada.
    let _ = unsafe { EnumWindows(Some(collect), LPARAM((&raw mut hwnds) as isize)) };
    let me = std::process::id();
    hwnds
        .into_iter()
        .filter_map(|hwnd| {
            let mut pid = 0u32;
            // SAFETY: consultas sin efectos; el buffer tiene el tamaño indicado.
            let (title, minimized) = unsafe {
                GetWindowThreadProcessId(hwnd, Some(&mut pid));
                let len = GetWindowTextLengthW(hwnd).max(0) as usize;
                let mut buf = vec![0u16; len + 1];
                let n = GetWindowTextW(hwnd, &mut buf).max(0) as usize;
                (
                    String::from_utf16_lossy(&buf[..n]),
                    IsIconic(hwnd).as_bool(),
                )
            };
            (pid != me).then(|| WindowInfo {
                hwnd: hwnd.0 as Hwnd,
                title,
                pid,
                exe: crate::process::exe_path(pid),
                minimized,
            })
        })
        .collect()
}

/// Trae la ventana al frente, restaurándola si está minimizada.
pub fn activate(hwnd: Hwnd) -> bool {
    let hwnd = HWND(hwnd as *mut _);
    // SAFETY: si el hwnd ya no existe, las llamadas fallan sin efectos.
    unsafe {
        if IsIconic(hwnd).as_bool() {
            let _ = ShowWindow(hwnd, SW_RESTORE);
        }
        SetForegroundWindow(hwnd).as_bool()
    }
}

/// Mueve y redimensiona la ventana (coordenadas de pantalla en píxeles físicos). Si está
/// minimizada o maximizada, primero se restaura.
pub fn set_rect(hwnd: Hwnd, x: i32, y: i32, width: i32, height: i32) -> bool {
    let hwnd = HWND(hwnd as *mut _);
    // SAFETY: si el hwnd ya no existe, las llamadas fallan sin efectos.
    unsafe {
        if IsIconic(hwnd).as_bool() || IsZoomed(hwnd).as_bool() {
            let _ = ShowWindow(hwnd, SW_RESTORE);
        }
        SetWindowPos(
            hwnd,
            None,
            x,
            y,
            width.max(1),
            height.max(1),
            SWP_NOZORDER | SWP_NOACTIVATE,
        )
        .is_ok()
    }
}

#[cfg(test)]
mod tests {
    #[test]
    fn listing_does_not_panic() {
        // En CI puede no haber ventanas; solo se comprueba que la enumeración funciona.
        for w in super::list() {
            assert!(!w.title.is_empty());
        }
    }
}
