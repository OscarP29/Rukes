//! Escucha de cambios del portapapeles (RF-100, RF-102).
//!
//! Una ventana de solo mensajes recibe `WM_CLIPBOARDUPDATE`. Se descarta el contenido que el
//! programa de origen marca como sensible:
//!
//! - `ExcludeClipboardContentFromMonitorProcessing` (cualquier dato),
//! - `CanIncludeInClipboardHistory` con valor 0,
//! - `Clipboard Viewer Ignore` (KeePass y otros gestores de contraseñas).

use std::cell::RefCell;

use windows::Win32::Foundation::{HGLOBAL, HWND, LPARAM, LRESULT, WPARAM};
use windows::Win32::System::DataExchange::{
    AddClipboardFormatListener, CloseClipboard, GetClipboardData, GetClipboardOwner,
    IsClipboardFormatAvailable, OpenClipboard, RegisterClipboardFormatW,
    RemoveClipboardFormatListener,
};
use windows::Win32::System::LibraryLoader::GetModuleHandleW;
use windows::Win32::System::Memory::{GlobalLock, GlobalSize, GlobalUnlock};
use windows::Win32::System::Ole::CF_UNICODETEXT;
use windows::Win32::System::Threading::GetCurrentThreadId;
use windows::Win32::UI::WindowsAndMessaging::{
    CreateWindowExW, DefWindowProcW, DestroyWindow, DispatchMessageW, GetMessageW,
    GetWindowThreadProcessId, HWND_MESSAGE, MSG, PostThreadMessageW, RegisterClassW,
    WINDOW_EX_STYLE, WM_CLIPBOARDUPDATE, WM_QUIT, WNDCLASSW, WS_OVERLAPPED,
};
use windows::core::w;

use crate::{Context as _, PlatformError, Result};

/// Texto copiado.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Copied {
    /// Texto.
    pub text: String,
    /// Ejecutable que lo copió (`keepass.exe`), si se conoce.
    pub source: Option<String>,
}

/// Lo que hay en el portapapeles al cambiar.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Change {
    /// Texto normal.
    Text(Copied),
    /// Marcado como sensible: no debe guardarse.
    Sensitive,
    /// Sin texto (imagen, archivos...).
    Other,
}

type OnChange = Box<dyn Fn(Change)>;

thread_local! {
    static HANDLER: RefCell<Option<(OnChange, Formats)>> = const { RefCell::new(None) };
}

#[derive(Clone, Copy)]
struct Formats {
    exclude: u32,
    history: u32,
    ignore: u32,
}

impl Formats {
    fn register() -> Self {
        // SAFETY: cadenas estáticas.
        unsafe {
            Self {
                exclude: RegisterClipboardFormatW(w!(
                    "ExcludeClipboardContentFromMonitorProcessing"
                )),
                history: RegisterClipboardFormatW(w!("CanIncludeInClipboardHistory")),
                ignore: RegisterClipboardFormatW(w!("Clipboard Viewer Ignore")),
            }
        }
    }
}

/// Ejecutable del dueño actual del portapapeles.
fn owner_exe() -> Option<String> {
    // SAFETY: sin punteros salvo `pid`, local.
    let pid = unsafe {
        let owner = GetClipboardOwner().ok()?;
        let mut pid = 0u32;
        GetWindowThreadProcessId(owner, Some(&mut pid));
        pid
    };
    let path = crate::process::exe_path(pid)?;
    Some(path.file_name()?.to_string_lossy().into_owned())
}

/// Lee un bloque global del portapapeles abierto.
///
/// SAFETY: el portapapeles debe estar abierto por este hilo.
unsafe fn read_global(format: u32) -> Option<Vec<u8>> {
    // SAFETY: según el contrato de la función; se lee como mucho el tamaño del bloque.
    unsafe {
        let h = GetClipboardData(format).ok()?;
        let mem = HGLOBAL(h.0);
        let ptr = GlobalLock(mem).cast::<u8>();
        if ptr.is_null() {
            return None;
        }
        let v = std::slice::from_raw_parts(ptr, GlobalSize(mem)).to_vec();
        let _ = GlobalUnlock(mem);
        Some(v)
    }
}

/// Clasifica el contenido actual del portapapeles. `max_chars` limita el texto leído.
fn snapshot(f: Formats, max_chars: usize) -> Change {
    // SAFETY: consultas sin punteros.
    let has = |fmt: u32| unsafe { IsClipboardFormatAvailable(fmt).is_ok() };
    if has(f.exclude) || has(f.ignore) {
        return Change::Sensitive;
    }
    if !has(u32::from(CF_UNICODETEXT.0)) && !has(f.history) {
        return Change::Other;
    }
    // Quien copia suele tener el portapapeles abierto unos ms: se reintenta.
    let mut opened = false;
    for _ in 0..10 {
        // SAFETY: sin punteros.
        if unsafe { OpenClipboard(None) }.is_ok() {
            opened = true;
            break;
        }
        std::thread::sleep(std::time::Duration::from_millis(15));
    }
    if !opened {
        return Change::Other;
    }
    // SAFETY: portapapeles abierto por este hilo hasta `CloseClipboard`.
    let result = unsafe {
        let excluded_from_history = read_global(f.history).and_then(|b| {
            b.get(0..4)
                .map(|d| u32::from_le_bytes([d[0], d[1], d[2], d[3]]))
        }) == Some(0);
        if excluded_from_history {
            Change::Sensitive
        } else {
            match read_global(u32::from(CF_UNICODETEXT.0)) {
                Some(b) => {
                    let units: Vec<u16> = b
                        .chunks_exact(2)
                        .map(|c| u16::from_le_bytes([c[0], c[1]]))
                        .take(max_chars)
                        .take_while(|c| *c != 0)
                        .collect();
                    Change::Text(Copied {
                        text: String::from_utf16_lossy(&units),
                        source: None,
                    })
                }
                None => Change::Other,
            }
        }
    };
    // SAFETY: abierto arriba.
    let _ = unsafe { CloseClipboard() };
    match result {
        Change::Text(mut c) => {
            c.source = owner_exe();
            Change::Text(c)
        }
        other => other,
    }
}

/// Caracteres que se leen como mucho de cada copia.
const MAX_CHARS: usize = 200_000;

unsafe extern "system" fn wndproc(hwnd: HWND, msg: u32, wp: WPARAM, lp: LPARAM) -> LRESULT {
    if msg == WM_CLIPBOARDUPDATE {
        let formats = HANDLER.with_borrow(|h| h.as_ref().map(|(_, f)| *f));
        if let Some(f) = formats {
            let change = snapshot(f, MAX_CHARS);
            HANDLER.with_borrow(|h| {
                if let Some((cb, _)) = h {
                    cb(change);
                }
            });
        }
        return LRESULT(0);
    }
    // SAFETY: procedimiento por defecto con los mismos argumentos.
    unsafe { DefWindowProcW(hwnd, msg, wp, lp) }
}

/// Escucha activa. Al soltarse deja de escuchar.
pub struct ClipboardWatcher {
    thread_id: u32,
}

impl ClipboardWatcher {
    /// Empieza a escuchar en un hilo propio.
    pub fn spawn(on_change: impl Fn(Change) + Send + 'static) -> Result<Self> {
        let (tx, rx) = crossbeam_channel::bounded::<Result<u32>>(1);
        let on_change: Box<dyn Fn(Change) + Send> = Box::new(on_change);
        std::thread::Builder::new()
            .name("rukes-clipboard".into())
            .spawn(move || {
                let setup = || -> Result<HWND> {
                    // SAFETY: registro de clase y ventana de solo mensajes con datos estáticos.
                    unsafe {
                        let inst = GetModuleHandleW(None).ctx("GetModuleHandleW")?;
                        let wc = WNDCLASSW {
                            lpfnWndProc: Some(wndproc),
                            hInstance: inst.into(),
                            lpszClassName: w!("RukesClipboard"),
                            ..Default::default()
                        };
                        RegisterClassW(&wc);
                        let hwnd = CreateWindowExW(
                            WINDOW_EX_STYLE(0),
                            w!("RukesClipboard"),
                            w!("Rukes clipboard"),
                            WS_OVERLAPPED,
                            0,
                            0,
                            0,
                            0,
                            Some(HWND_MESSAGE),
                            None,
                            Some(inst.into()),
                            None,
                        )
                        .ctx("CreateWindowExW")?;
                        AddClipboardFormatListener(hwnd).ctx("AddClipboardFormatListener")?;
                        Ok(hwnd)
                    }
                };
                let hwnd = match setup() {
                    Ok(h) => h,
                    Err(e) => {
                        let _ = tx.send(Err(e));
                        return;
                    }
                };
                HANDLER.set(Some((on_change, Formats::register())));
                // SAFETY: sin precondiciones.
                let _ = tx.send(Ok(unsafe { GetCurrentThreadId() }));
                let mut msg = MSG::default();
                // SAFETY: bucle de mensajes estándar del hilo; la ventana es de este hilo.
                unsafe {
                    while GetMessageW(&mut msg, None, 0, 0).as_bool() {
                        DispatchMessageW(&msg);
                    }
                    let _ = RemoveClipboardFormatListener(hwnd);
                    let _ = DestroyWindow(hwnd);
                }
                HANDLER.set(None);
            })
            .map_err(|e| PlatformError::Invalid(e.to_string()))?;
        let thread_id = rx
            .recv()
            .map_err(|_| PlatformError::Invalid("el hilo del portapapeles terminó".into()))??;
        Ok(Self { thread_id })
    }
}

impl Drop for ClipboardWatcher {
    fn drop(&mut self) {
        // SAFETY: mensaje sin punteros a un hilo de este proceso.
        let _ = unsafe { PostThreadMessageW(self.thread_id, WM_QUIT, WPARAM(0), LPARAM(0)) };
    }
}
