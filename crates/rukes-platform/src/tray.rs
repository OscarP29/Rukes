//! Icono en la bandeja del sistema con menú (RF-007).
//!
//! Corre en un hilo propio con una ventana oculta que recibe los mensajes del icono. Si el
//! Explorador se reinicia (`TaskbarCreated`), el icono se vuelve a añadir.

use std::cell::RefCell;
use std::sync::Arc;
use std::sync::atomic::{AtomicIsize, Ordering};

use windows::Win32::Foundation::{HWND, LPARAM, LRESULT, POINT, WPARAM};
use windows::Win32::Graphics::Gdi::{
    BI_RGB, BITMAPINFO, BITMAPINFOHEADER, CreateBitmap, CreateDIBSection, DIB_RGB_COLORS,
    DeleteObject, HBITMAP,
};
use windows::Win32::System::LibraryLoader::GetModuleHandleW;
use windows::Win32::System::Threading::GetCurrentThreadId;
use windows::Win32::UI::Shell::{
    NIF_ICON, NIF_INFO, NIF_MESSAGE, NIF_TIP, NIIF_ERROR, NIIF_INFO, NIIF_NOSOUND, NIM_ADD,
    NIM_DELETE, NIM_MODIFY, NOTIFYICONDATAW, Shell_NotifyIconW,
};
use windows::Win32::UI::WindowsAndMessaging::{
    AppendMenuW, CreateIconIndirect, CreatePopupMenu, CreateWindowExW, DefWindowProcW, DestroyIcon,
    DestroyMenu, DestroyWindow, DispatchMessageW, GetCursorPos, GetMessageW, HICON, ICONINFO,
    MF_GRAYED, MF_SEPARATOR, MF_STRING, MSG, PostMessageW, PostThreadMessageW, RegisterClassW,
    RegisterWindowMessageW, SetForegroundWindow, TPM_NONOTIFY, TPM_RETURNCMD, TPM_RIGHTBUTTON,
    TrackPopupMenu, TranslateMessage, WINDOW_EX_STYLE, WM_LBUTTONUP, WM_NULL, WM_QUIT,
    WM_RBUTTONUP, WNDCLASSW, WS_OVERLAPPED,
};
use windows::core::{PCWSTR, w};

use crate::{Context as _, PlatformError, Result, wide};

const WM_TRAY: u32 = windows::Win32::UI::WindowsAndMessaging::WM_APP + 10;
const ICON_ID: u32 = 1;

/// Entrada del menú.
#[derive(Debug, Clone)]
pub enum MenuEntry {
    /// Opción con id (> 0) y texto.
    Item(u16, String),
    /// Opción desactivada.
    Disabled(String),
    /// Separador.
    Separator,
}

type MenuFn = Box<dyn Fn() -> Vec<MenuEntry> + Send>;
type ClickFn = Box<dyn Fn() + Send>;
type CommandFn = Box<dyn Fn(u16) + Send>;

struct State {
    menu: MenuFn,
    on_click: ClickFn,
    on_command: CommandFn,
    icon: HICON,
    tip: String,
    taskbar_created: u32,
}

thread_local! {
    static STATE: RefCell<Option<State>> = const { RefCell::new(None) };
}

/// Icono de la bandeja. Al soltarlo se quita el icono.
pub struct Tray {
    thread_id: u32,
    hwnd: Arc<AtomicIsize>,
}

/// Logo de Rukes en RGBA: cuadrado redondeado azul con un `>` oscuro.
pub fn logo(size: u32) -> Vec<u8> {
    let s = size as f32;
    let mut px = vec![0u8; (size * size * 4) as usize];
    let radius = s * 0.22;
    let thick = (s * 0.11).max(1.5);
    // Segmentos del chevron: (x0, y0) → (x1, y1) en fracciones del tamaño.
    let segs = [((0.32, 0.28), (0.58, 0.5)), ((0.58, 0.5), (0.32, 0.72))];
    for y in 0..size {
        for x in 0..size {
            let (fx, fy) = (x as f32 + 0.5, y as f32 + 0.5);
            // Distancia al cuadrado redondeado.
            let qx = (fx - s / 2.0).abs() - (s / 2.0 - radius);
            let qy = (fy - s / 2.0).abs() - (s / 2.0 - radius);
            let outside = (qx.max(0.0).powi(2) + qy.max(0.0).powi(2)).sqrt() - radius;
            let cover = (0.5 - outside).clamp(0.0, 1.0);
            if cover <= 0.0 {
                continue;
            }
            let d = segs
                .iter()
                .map(|&((x0, y0), (x1, y1))| {
                    let (ax, ay, bx, by) = (x0 * s, y0 * s, x1 * s, y1 * s);
                    let (dx, dy) = (bx - ax, by - ay);
                    let t =
                        (((fx - ax) * dx + (fy - ay) * dy) / (dx * dx + dy * dy)).clamp(0.0, 1.0);
                    ((fx - ax - t * dx).powi(2) + (fy - ay - t * dy).powi(2)).sqrt()
                })
                .fold(f32::MAX, f32::min);
            let ink = (thick / 2.0 + 0.5 - d).clamp(0.0, 1.0);
            let (bg, fg): ([u8; 3], [u8; 3]) = ([0x7a, 0xa2, 0xf7], [0x1a, 0x1b, 0x26]);
            let i = ((y * size + x) * 4) as usize;
            for c in 0..3 {
                px[i + c] = (f32::from(bg[c]) * (1.0 - ink) + f32::from(fg[c]) * ink) as u8;
            }
            px[i + 3] = (cover * 255.0) as u8;
        }
    }
    px
}

/// Crea un `HICON` a partir de píxeles RGBA.
fn icon_from_rgba(size: u32, rgba: &[u8]) -> Result<HICON> {
    let header = BITMAPINFOHEADER {
        biSize: size_of::<BITMAPINFOHEADER>() as u32,
        biWidth: size as i32,
        biHeight: -(size as i32),
        biPlanes: 1,
        biBitCount: 32,
        biCompression: BI_RGB.0,
        ..Default::default()
    };
    let info = BITMAPINFO {
        bmiHeader: header,
        ..Default::default()
    };
    let mut bits: *mut core::ffi::c_void = std::ptr::null_mut();
    // SAFETY: `info` describe un DIB de 32 bpp; `bits` recibe el puntero al buffer del DIB.
    let color: HBITMAP =
        unsafe { CreateDIBSection(None, &info, DIB_RGB_COLORS, &mut bits, None, 0) }
            .ctx("CreateDIBSection")?;
    if bits.is_null() {
        return Err(PlatformError::Invalid("DIB sin memoria".into()));
    }
    let n = (size * size) as usize;
    // SAFETY: el DIB tiene `size*size` píxeles de 4 bytes y nadie más lo usa todavía.
    let dst = unsafe { std::slice::from_raw_parts_mut(bits.cast::<u8>(), n * 4) };
    for (d, s) in dst.chunks_exact_mut(4).zip(rgba.chunks_exact(4)) {
        // BGRA premultiplicado no es necesario para iconos: alfa directo.
        d.copy_from_slice(&[s[2], s[1], s[0], s[3]]);
    }
    let mask_bits = vec![0u8; (size.div_ceil(16) * 2 * size) as usize];
    // SAFETY: buffer de máscara monocromo del tamaño adecuado (filas alineadas a 16 bits).
    let mask = unsafe {
        CreateBitmap(
            size as i32,
            size as i32,
            1,
            1,
            Some(mask_bits.as_ptr().cast()),
        )
    };
    let ii = ICONINFO {
        fIcon: true.into(),
        hbmMask: mask,
        hbmColor: color,
        ..Default::default()
    };
    // SAFETY: los dos bitmaps son válidos; CreateIconIndirect los copia.
    let icon = unsafe { CreateIconIndirect(&ii) }.ctx("CreateIconIndirect");
    // SAFETY: bitmaps creados arriba y ya copiados.
    unsafe {
        let _ = DeleteObject(color.into());
        let _ = DeleteObject(mask.into());
    }
    icon
}

fn notify_data(hwnd: HWND, icon: HICON, tip: &str) -> NOTIFYICONDATAW {
    let mut nid = NOTIFYICONDATAW {
        cbSize: size_of::<NOTIFYICONDATAW>() as u32,
        hWnd: hwnd,
        uID: ICON_ID,
        uFlags: NIF_ICON | NIF_MESSAGE | NIF_TIP,
        uCallbackMessage: WM_TRAY,
        hIcon: icon,
        ..Default::default()
    };
    for (d, s) in nid.szTip.iter_mut().zip(tip.encode_utf16().take(127)) {
        *d = s;
    }
    nid
}

/// Copia `s` en un buffer UTF-16 fijo, cortando y dejando el terminador.
fn copy_truncated(dst: &mut [u16], s: &str) {
    let max = dst.len().saturating_sub(1);
    let mut n = 0;
    for (d, c) in dst.iter_mut().zip(s.encode_utf16().take(max)) {
        *d = c;
        n += 1;
    }
    if let Some(z) = dst.get_mut(n) {
        *z = 0;
    }
}

fn add_icon(hwnd: HWND, st: &State) {
    let nid = notify_data(hwnd, st.icon, &st.tip);
    // SAFETY: estructura local completa.
    let _ = unsafe { Shell_NotifyIconW(NIM_ADD, &nid) };
}

fn remove_icon(hwnd: HWND) {
    let nid = NOTIFYICONDATAW {
        cbSize: size_of::<NOTIFYICONDATAW>() as u32,
        hWnd: hwnd,
        uID: ICON_ID,
        ..Default::default()
    };
    // SAFETY: estructura local; si el icono no existe la llamada falla sin efectos.
    let _ = unsafe { Shell_NotifyIconW(NIM_DELETE, &nid) };
}

fn show_menu(hwnd: HWND, entries: &[MenuEntry]) -> u16 {
    // SAFETY: menú creado y destruido aquí; los textos viven durante AppendMenuW.
    unsafe {
        let Ok(menu) = CreatePopupMenu() else {
            return 0;
        };
        for e in entries {
            let _ = match e {
                MenuEntry::Item(id, label) => {
                    let t = wide(label);
                    AppendMenuW(menu, MF_STRING, usize::from(*id), PCWSTR(t.as_ptr()))
                }
                MenuEntry::Disabled(label) => {
                    let t = wide(label);
                    AppendMenuW(menu, MF_STRING | MF_GRAYED, 0, PCWSTR(t.as_ptr()))
                }
                MenuEntry::Separator => AppendMenuW(menu, MF_SEPARATOR, 0, PCWSTR::null()),
            };
        }
        let mut pt = POINT::default();
        let _ = GetCursorPos(&mut pt);
        // Necesario para que el menú se cierre al hacer clic fuera.
        let _ = SetForegroundWindow(hwnd);
        let cmd = TrackPopupMenu(
            menu,
            TPM_RETURNCMD | TPM_RIGHTBUTTON | TPM_NONOTIFY,
            pt.x,
            pt.y,
            Some(0),
            hwnd,
            None,
        );
        let _ = PostMessageW(Some(hwnd), WM_NULL, WPARAM(0), LPARAM(0));
        let _ = DestroyMenu(menu);
        cmd.0 as u16
    }
}

unsafe extern "system" fn wndproc(hwnd: HWND, msg: u32, wp: WPARAM, lp: LPARAM) -> LRESULT {
    if msg == WM_TRAY {
        let event = (lp.0 & 0xffff) as u32;
        // Se saca lo necesario del estado antes de llamar a los callbacks (pueden tardar).
        match event {
            WM_LBUTTONUP => STATE.with_borrow(|s| {
                if let Some(s) = s {
                    (s.on_click)();
                }
            }),
            WM_RBUTTONUP => {
                let entries = STATE.with_borrow(|s| s.as_ref().map(|s| (s.menu)()));
                if let Some(entries) = entries {
                    let cmd = show_menu(hwnd, &entries);
                    if cmd != 0 {
                        STATE.with_borrow(|s| {
                            if let Some(s) = s {
                                (s.on_command)(cmd);
                            }
                        });
                    }
                }
            }
            _ => {}
        }
        return LRESULT(0);
    }
    let recreate = STATE.with_borrow(|s| s.as_ref().is_some_and(|s| s.taskbar_created == msg));
    if recreate {
        STATE.with_borrow(|s| {
            if let Some(s) = s {
                add_icon(hwnd, s);
            }
        });
        return LRESULT(0);
    }
    // SAFETY: procedimiento por defecto con los mismos argumentos recibidos.
    unsafe { DefWindowProcW(hwnd, msg, wp, lp) }
}

impl Tray {
    /// Crea el icono. `menu` se llama cada vez que se abre el menú (clic derecho);
    /// `on_click` con el clic izquierdo y `on_command` con el id elegido.
    pub fn spawn(
        tooltip: &str,
        menu: impl Fn() -> Vec<MenuEntry> + Send + 'static,
        on_click: impl Fn() + Send + 'static,
        on_command: impl Fn(u16) + Send + 'static,
    ) -> Result<Self> {
        let (tx, rx) = crossbeam_channel::bounded::<Result<u32>>(1);
        let hwnd_slot = Arc::new(AtomicIsize::new(0));
        let slot = Arc::clone(&hwnd_slot);
        let tip = tooltip.to_owned();
        let menu: MenuFn = Box::new(menu);
        let on_click: ClickFn = Box::new(on_click);
        let on_command: CommandFn = Box::new(on_command);
        std::thread::Builder::new()
            .name("rukes-tray".into())
            .spawn(move || {
                let setup = || -> Result<HWND> {
                    // SAFETY: registro de clase y creación de ventana con datos estáticos o
                    // locales; la ventana nunca se muestra.
                    unsafe {
                        let inst = GetModuleHandleW(None).ctx("GetModuleHandleW")?;
                        let wc = WNDCLASSW {
                            lpfnWndProc: Some(wndproc),
                            hInstance: inst.into(),
                            lpszClassName: w!("RukesTray"),
                            ..Default::default()
                        };
                        RegisterClassW(&wc);
                        CreateWindowExW(
                            WINDOW_EX_STYLE(0),
                            w!("RukesTray"),
                            w!("Rukes"),
                            WS_OVERLAPPED,
                            0,
                            0,
                            0,
                            0,
                            None,
                            None,
                            Some(inst.into()),
                            None,
                        )
                        .ctx("CreateWindowExW")
                    }
                };
                let hwnd = match setup() {
                    Ok(h) => h,
                    Err(e) => {
                        let _ = tx.send(Err(e));
                        return;
                    }
                };
                let icon = match icon_from_rgba(32, &logo(32)) {
                    Ok(i) => i,
                    Err(e) => {
                        let _ = tx.send(Err(e));
                        return;
                    }
                };
                // SAFETY: cadena estática.
                let taskbar_created = unsafe { RegisterWindowMessageW(w!("TaskbarCreated")) };
                let state = State {
                    menu,
                    on_click,
                    on_command,
                    icon,
                    tip,
                    taskbar_created,
                };
                slot.store(hwnd.0 as isize, Ordering::SeqCst);
                // Se responde antes de añadir el icono: `Shell_NotifyIconW` habla con el
                // Explorador y puede tardar decenas de ms; el arranque no debe esperarlo.
                // SAFETY: sin precondiciones.
                let _ = tx.send(Ok(unsafe { GetCurrentThreadId() }));
                add_icon(hwnd, &state);
                STATE.set(Some(state));
                let mut msg = MSG::default();
                // SAFETY: bucle de mensajes estándar del hilo.
                unsafe {
                    while GetMessageW(&mut msg, None, 0, 0).as_bool() {
                        let _ = TranslateMessage(&msg);
                        DispatchMessageW(&msg);
                    }
                }
                remove_icon(hwnd);
                if let Some(s) = STATE.take() {
                    // SAFETY: icono creado por este hilo.
                    let _ = unsafe { DestroyIcon(s.icon) };
                }
                // SAFETY: ventana de este hilo.
                let _ = unsafe { DestroyWindow(hwnd) };
            })
            .map_err(|e| PlatformError::Invalid(e.to_string()))?;
        let thread_id = rx
            .recv()
            .map_err(|_| PlatformError::Invalid("el hilo de la bandeja terminó".into()))??;
        Ok(Self {
            thread_id,
            hwnd: hwnd_slot,
        })
    }

    /// Muestra una notificación del sistema desde el icono (en Windows 10/11 aparece como toast).
    pub fn notify(&self, title: &str, body: &str, error: bool) {
        let hwnd = self.hwnd.load(Ordering::SeqCst);
        if hwnd == 0 {
            return;
        }
        let mut nid = NOTIFYICONDATAW {
            cbSize: size_of::<NOTIFYICONDATAW>() as u32,
            hWnd: HWND(hwnd as *mut _),
            uID: ICON_ID,
            uFlags: NIF_INFO,
            dwInfoFlags: (if error { NIIF_ERROR } else { NIIF_INFO }) | NIIF_NOSOUND,
            ..Default::default()
        };
        copy_truncated(&mut nid.szInfoTitle, title);
        copy_truncated(&mut nid.szInfo, if body.is_empty() { " " } else { body });
        // SAFETY: estructura local completa; el icono pertenece a esta ventana.
        let _ = unsafe { Shell_NotifyIconW(NIM_MODIFY, &nid) };
    }

    /// Quita el icono ya (para salir con `process::exit` sin dejar un icono fantasma).
    pub fn remove_now(&self) {
        let hwnd = self.hwnd.load(Ordering::SeqCst);
        if hwnd != 0 {
            remove_icon(HWND(hwnd as *mut _));
        }
    }
}

impl Drop for Tray {
    fn drop(&mut self) {
        // SAFETY: id de un hilo con cola de mensajes; si ya terminó, falla sin efectos.
        let _ = unsafe { PostThreadMessageW(self.thread_id, WM_QUIT, WPARAM(0), LPARAM(0)) };
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn logo_has_transparent_corners_and_ink() {
        let px = logo(32);
        assert_eq!(px.len(), 32 * 32 * 4);
        assert_eq!(px[3], 0, "esquina transparente");
        let center = ((16 * 32 + 16) * 4) as usize;
        assert_eq!(px[center + 3], 255);
        // Hay píxeles del chevron (oscuros).
        assert!(px.chunks_exact(4).any(|p| p[3] == 255 && p[0] < 0x40));
    }

    #[test]
    fn icon_can_be_created() {
        let icon = icon_from_rgba(32, &logo(32)).unwrap();
        // SAFETY: icono creado arriba.
        unsafe { DestroyIcon(icon) }.unwrap();
    }
}
