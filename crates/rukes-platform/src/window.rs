//! Mostrar, ocultar y posicionar la ventana del launcher (RF-002, RF-003, RF-005).

use windows::Win32::Foundation::{HWND, POINT, RECT};
use windows::Win32::Graphics::Gdi::{
    GetMonitorInfoW, MONITOR_DEFAULTTONEAREST, MONITORINFO, MonitorFromPoint,
};
use windows::Win32::System::Threading::AttachThreadInput;
use windows::Win32::System::Threading::GetCurrentThreadId;
use windows::Win32::UI::HiDpi::GetDpiForWindow;
use windows::Win32::UI::Input::KeyboardAndMouse::SetFocus;
use windows::Win32::UI::WindowsAndMessaging::{
    BringWindowToTop, GetCursorPos, GetForegroundWindow, GetWindowRect, GetWindowThreadProcessId,
    HWND_TOPMOST, IsWindow, IsWindowVisible, SW_HIDE, SW_SHOW, SWP_NOACTIVATE, SWP_NOSIZE,
    SetForegroundWindow, SetWindowPos, ShowWindow,
};

use crate::Hwnd;

/// Posición vertical relativa en el área de trabajo del monitor (0 = arriba, 0,5 = centro).
#[derive(Debug, Clone, Copy)]
pub struct Placement {
    /// Fracción del espacio libre por encima de la ventana.
    pub vertical: f32,
}

impl Placement {
    /// Centrada, algo por encima del centro (como Spotlight/PowerToys Run).
    pub const CENTER: Self = Self { vertical: 0.3 };
    /// Cerca del borde superior.
    pub const TOP_CENTER: Self = Self { vertical: 0.12 };
}

fn h(hwnd: Hwnd) -> HWND {
    HWND(hwnd as *mut _)
}

fn rect(hwnd: HWND) -> RECT {
    let mut r = RECT::default();
    // SAFETY: r local; un hwnd no válido devuelve error y deja r a cero.
    let _ = unsafe { GetWindowRect(hwnd, &mut r) };
    r
}

/// Coloca la ventana en el monitor del cursor según `placement`. La posición vertical se calcula
/// con `ref_height` (alto lógico de la ventana con la lista llena) para que no salte según el
/// tamaño actual. Se hace en dos pasos porque al cambiar a un monitor con otro DPI la ventana se
/// redimensiona (WM_DPICHANGED).
pub fn place_on_cursor_monitor(hwnd: Hwnd, placement: Placement, ref_height: f32) {
    let hwnd = h(hwnd);
    let mut pt = POINT::default();
    // SAFETY: punteros locales; funciones sin otras precondiciones.
    let work = unsafe {
        let _ = GetCursorPos(&mut pt);
        let mon = MonitorFromPoint(pt, MONITOR_DEFAULTTONEAREST);
        let mut mi = MONITORINFO {
            cbSize: size_of::<MONITORINFO>() as u32,
            ..Default::default()
        };
        let _ = GetMonitorInfoW(mon, &mut mi);
        mi.rcWork
    };
    for _ in 0..2 {
        let r = rect(hwnd);
        // SAFETY: consulta sin efectos sobre una ventana de este proceso.
        let scale = unsafe { GetDpiForWindow(hwnd) }.max(96) as f32 / 96.0;
        let w = r.right - r.left;
        let hgt = ((ref_height * scale) as i32).max(r.bottom - r.top);
        let x = work.left + (work.right - work.left - w) / 2;
        let free = (work.bottom - work.top - hgt).max(0) as f32;
        let y = work.top + (free * placement.vertical) as i32;
        if r.left == x && r.top == y {
            break;
        }
        // SAFETY: hwnd de este proceso.
        let _ = unsafe {
            SetWindowPos(
                hwnd,
                Some(HWND_TOPMOST),
                x,
                y,
                0,
                0,
                SWP_NOSIZE | SWP_NOACTIVATE,
            )
        };
    }
}

/// Muestra la ventana y le da el foco aunque otra app esté en primer plano (RF-001). Devuelve la
/// ventana que tenía el foco antes, para devolvérselo al ocultar ([`hide_and_restore`]).
///
/// Debe llamarse desde el hilo que recibió el atajo (tiene permiso de primer plano); si
/// `SetForegroundWindow` se rechaza, se usa `AttachThreadInput` con el hilo en primer plano.
pub fn show_and_focus(hwnd: Hwnd) -> Hwnd {
    let hwnd = h(hwnd);
    // SAFETY: llamadas sobre una ventana de este proceso; AttachThreadInput se deshace siempre.
    unsafe {
        let previous = GetForegroundWindow();
        let previous = if previous == hwnd {
            0
        } else {
            previous.0 as Hwnd
        };
        let _ = ShowWindow(hwnd, SW_SHOW);
        if SetForegroundWindow(hwnd).as_bool() {
            return previous;
        }
        let fg = GetForegroundWindow();
        let fg_thread = GetWindowThreadProcessId(fg, None);
        let me = GetCurrentThreadId();
        let attached =
            fg_thread != 0 && fg_thread != me && AttachThreadInput(me, fg_thread, true).as_bool();
        let _ = BringWindowToTop(hwnd);
        let _ = SetForegroundWindow(hwnd);
        let _ = SetFocus(Some(hwnd));
        if attached {
            let _ = AttachThreadInput(me, fg_thread, false);
        }
        previous
    }
}

/// Oculta la ventana y, si tenía el foco, se lo devuelve a `previous` (la app desde la que se
/// invocó el launcher). Si `previous` ya no existe, Windows elige la siguiente.
pub fn hide_and_restore(hwnd: Hwnd, previous: Hwnd) {
    let had_focus = is_foreground(hwnd);
    hide(hwnd);
    if had_focus && previous != 0 {
        // SAFETY: si `previous` ya no es una ventana válida la llamada falla sin efectos.
        unsafe {
            let p = h(previous);
            if IsWindow(Some(p)).as_bool() {
                let _ = SetForegroundWindow(p);
            }
        }
    }
}

/// Oculta la ventana.
pub fn hide(hwnd: Hwnd) {
    // SAFETY: ventana de este proceso.
    let _ = unsafe { ShowWindow(h(hwnd), SW_HIDE) };
}

/// `true` si la ventana está visible.
pub fn is_visible(hwnd: Hwnd) -> bool {
    // SAFETY: consulta sin efectos.
    unsafe { IsWindowVisible(h(hwnd)).as_bool() }
}

/// `true` si la ventana tiene el primer plano.
pub fn is_foreground(hwnd: Hwnd) -> bool {
    // SAFETY: consulta sin efectos.
    unsafe { GetForegroundWindow() == h(hwnd) }
}
