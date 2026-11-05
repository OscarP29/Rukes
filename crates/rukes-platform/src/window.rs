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
    SWP_NOZORDER, SetForegroundWindow, SetWindowPos, ShowWindow,
};

use crate::Hwnd;

/// Posición vertical relativa en el área de trabajo del monitor (0 = arriba, 0,5 = centro).
#[derive(Debug, Clone, Copy)]
pub struct Placement {
    /// Fracción del espacio libre por encima de la ventana.
    pub vertical: f32,
    /// Anclada abajo: el borde inferior queda fijo y la ventana crece hacia arriba.
    pub bottom: bool,
}

impl Placement {
    /// Centrada, algo por encima del centro (como Spotlight/PowerToys Run).
    pub const CENTER: Self = Self {
        vertical: 0.3,
        bottom: false,
    };
    /// Cerca del borde superior.
    pub const TOP_CENTER: Self = Self {
        vertical: 0.12,
        bottom: false,
    };
    /// Cerca del borde inferior (encima de la barra de tareas).
    pub const BOTTOM_CENTER: Self = Self {
        vertical: 1.0,
        bottom: true,
    };
}

/// Separación lógica entre la ventana anclada abajo y el borde del área de trabajo.
const BOTTOM_MARGIN: f32 = 48.0;

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
        let x = work.left + (work.right - work.left - w) / 2;
        let y = if placement.bottom {
            // El borde inferior se ancla; la altura actual decide dónde empieza.
            let margin = (BOTTOM_MARGIN * scale) as i32;
            (work.bottom - margin - (r.bottom - r.top)).max(work.top)
        } else {
            let hgt = ((ref_height * scale) as i32).max(r.bottom - r.top);
            let free = (work.bottom - work.top - hgt).max(0) as f32;
            work.top + (free * placement.vertical) as i32
        };
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

/// Cambia el tamaño (lógico) de la ventana manteniendo fijos su borde inferior y su centro
/// horizontal, en un solo paso para que no parpadee (posición `bottom_center`).
pub fn resize_keep_bottom(hwnd: Hwnd, width: f32, height: f32) {
    let hwnd = h(hwnd);
    let r = rect(hwnd);
    // SAFETY: consulta sin efectos sobre una ventana de este proceso.
    let scale = unsafe { GetDpiForWindow(hwnd) }.max(96) as f32 / 96.0;
    let w = (width * scale).round() as i32;
    let hgt = (height * scale).round() as i32;
    if r.right - r.left == w && r.bottom - r.top == hgt {
        return;
    }
    let x = (r.left + r.right) / 2 - w / 2;
    let y = r.bottom - hgt;
    // SAFETY: hwnd de este proceso.
    let _ = unsafe { SetWindowPos(hwnd, None, x, y, w, hgt, SWP_NOACTIVATE | SWP_NOZORDER) };
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
