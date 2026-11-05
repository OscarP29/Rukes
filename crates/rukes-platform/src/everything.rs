//! Búsqueda con *Everything* (voidtools) por su IPC de `WM_COPYDATA`, si está abierto (RF-041).
//!
//! Formato según `everything_ipc.h` del SDK: se envía `EVERYTHING_IPC_QUERY2` seguido del texto
//! en UTF-16 y Everything responde con `EVERYTHING_IPC_LIST2` a una ventana nuestra.

use std::cell::RefCell;
use std::path::PathBuf;
use std::time::{Duration, Instant};

use windows::Win32::Foundation::{HWND, LPARAM, LRESULT, WPARAM};
use windows::Win32::System::DataExchange::COPYDATASTRUCT;
use windows::Win32::System::LibraryLoader::GetModuleHandleW;
use windows::Win32::UI::WindowsAndMessaging::{
    CreateWindowExW, DefWindowProcW, DestroyWindow, DispatchMessageW, FindWindowW, HWND_MESSAGE,
    MSG, MsgWaitForMultipleObjects, PM_REMOVE, PeekMessageW, QS_ALLINPUT, RegisterClassW,
    SMTO_ABORTIFHUNG, SendMessageTimeoutW, TranslateMessage, WINDOW_EX_STYLE, WM_COPYDATA,
    WNDCLASSW, WS_OVERLAPPED,
};
use windows::core::w;

use crate::{Context as _, PlatformError, Result};

const COPYDATA_QUERY2W: usize = 18;
const REQUEST_FULL_PATH_AND_NAME: u32 = 0x0000_0004;
const SORT_NAME_ASCENDING: u32 = 1;
const ITEM_FOLDER: u32 = 0x1;
/// Id con que Everything nos devuelve la respuesta.
const REPLY_ID: u32 = 0x5255_4b45;
/// Tiempo máximo de una búsqueda.
const TIMEOUT: Duration = Duration::from_millis(800);

thread_local! {
    static REPLY: RefCell<Option<Vec<u8>>> = const { RefCell::new(None) };
}

/// Ventana de Everything, si está abierto.
fn everything_window() -> Option<HWND> {
    // SAFETY: búsqueda de una ventana por clase con una cadena estática.
    unsafe { FindWindowW(w!("EVERYTHING_TASKBAR_NOTIFICATION"), None) }.ok()
}

/// `true` si Everything está abierto y acepta consultas.
pub fn available() -> bool {
    everything_window().is_some()
}

extern "system" fn wndproc(hwnd: HWND, msg: u32, wp: WPARAM, lp: LPARAM) -> LRESULT {
    if msg == WM_COPYDATA && lp.0 != 0 {
        // SAFETY: con WM_COPYDATA, `lp` apunta a un COPYDATASTRUCT válido durante el mensaje.
        let cds = unsafe { &*(lp.0 as *const COPYDATASTRUCT) };
        if cds.dwData == REPLY_ID as usize && !cds.lpData.is_null() {
            // SAFETY: `lpData` tiene `cbData` bytes válidos durante el mensaje; se copian.
            let bytes =
                unsafe { std::slice::from_raw_parts(cds.lpData as *const u8, cds.cbData as usize) }
                    .to_vec();
            REPLY.with_borrow_mut(|r| *r = Some(bytes));
            return LRESULT(1);
        }
    }
    // SAFETY: procedimiento por defecto con los mismos argumentos.
    unsafe { DefWindowProcW(hwnd, msg, wp, lp) }
}

/// Mensaje `EVERYTHING_IPC_QUERY2` para `search`.
fn query_bytes(reply: HWND, search: &str, max: u32) -> Vec<u8> {
    let mut out = Vec::new();
    for v in [
        reply.0 as usize as u32,
        REPLY_ID,
        0,
        0,
        max,
        REQUEST_FULL_PATH_AND_NAME,
        SORT_NAME_ASCENDING,
    ] {
        out.extend_from_slice(&v.to_le_bytes());
    }
    for u in search.encode_utf16().chain(std::iter::once(0)) {
        out.extend_from_slice(&u.to_le_bytes());
    }
    out
}

fn u32_at(b: &[u8], at: usize) -> Option<u32> {
    b.get(at..at + 4)
        .map(|s| u32::from_le_bytes([s[0], s[1], s[2], s[3]]))
}

/// Lee `EVERYTHING_IPC_LIST2`: rutas completas y si son carpetas.
pub fn parse_reply(b: &[u8]) -> Vec<(PathBuf, bool)> {
    let n = u32_at(b, 4).unwrap_or(0) as usize;
    let mut out = Vec::with_capacity(n.min(1000));
    for i in 0..n {
        let base = 20 + i * 8;
        let (Some(flags), Some(off)) = (u32_at(b, base), u32_at(b, base + 4)) else {
            break;
        };
        let off = off as usize;
        let Some(len) = u32_at(b, off) else { break };
        let start = off + 4;
        let Some(raw) = b.get(start..start + len as usize * 2) else {
            break;
        };
        let units: Vec<u16> = raw
            .chunks_exact(2)
            .map(|c| u16::from_le_bytes([c[0], c[1]]))
            .collect();
        out.push((
            PathBuf::from(String::from_utf16_lossy(&units)),
            flags & ITEM_FOLDER != 0,
        ));
    }
    out
}

/// Busca con la sintaxis de Everything (`file:`, `folder:`, `ext:pdf`…). Hasta `max` resultados.
pub fn search(query: &str, max: u32) -> Result<Vec<(PathBuf, bool)>> {
    let target = everything_window()
        .ok_or_else(|| PlatformError::Invalid("Everything no está abierto".into()))?;
    // SAFETY: clase y ventana de solo mensajes con datos estáticos; la ventana se destruye al
    // terminar. El COPYDATASTRUCT apunta a `msg`, que vive durante el envío síncrono.
    unsafe {
        let inst = GetModuleHandleW(None).ctx("GetModuleHandleW")?;
        let wc = WNDCLASSW {
            lpfnWndProc: Some(wndproc),
            hInstance: inst.into(),
            lpszClassName: w!("RukesEverything"),
            ..Default::default()
        };
        RegisterClassW(&wc);
        let hwnd = CreateWindowExW(
            WINDOW_EX_STYLE(0),
            w!("RukesEverything"),
            w!("Rukes Everything"),
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
        REPLY.with_borrow_mut(|r| *r = None);
        let msg = query_bytes(hwnd, query, max);
        let cds = COPYDATASTRUCT {
            dwData: COPYDATA_QUERY2W,
            cbData: msg.len() as u32,
            lpData: msg.as_ptr() as *mut _,
        };
        let mut result = 0usize;
        let sent = SendMessageTimeoutW(
            target,
            WM_COPYDATA,
            WPARAM(hwnd.0 as usize),
            LPARAM(&raw const cds as isize),
            SMTO_ABORTIFHUNG,
            TIMEOUT.as_millis() as u32,
            Some(&mut result),
        );
        let start = Instant::now();
        let mut reply = None;
        if sent.0 != 0 && result != 0 {
            // La respuesta llega como otro WM_COPYDATA: se bombean mensajes hasta recibirla.
            while start.elapsed() < TIMEOUT {
                reply = REPLY.with_borrow_mut(Option::take);
                if reply.is_some() {
                    break;
                }
                let left = TIMEOUT.saturating_sub(start.elapsed()).as_millis() as u32;
                let _ = MsgWaitForMultipleObjects(None, false, left.max(1), QS_ALLINPUT);
                let mut m = MSG::default();
                while PeekMessageW(&mut m, Some(hwnd), 0, 0, PM_REMOVE).as_bool() {
                    let _ = TranslateMessage(&m);
                    DispatchMessageW(&m);
                }
            }
        }
        let _ = DestroyWindow(hwnd);
        let reply =
            reply.ok_or_else(|| PlatformError::Invalid("Everything no respondió".into()))?;
        Ok(parse_reply(&reply))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn builds_query_and_reads_reply() {
        let q = query_bytes(HWND(0x1234 as *mut _), "ab", 10);
        assert_eq!(q.len(), 7 * 4 + 3 * 2);
        assert_eq!(u32_at(&q, 0), Some(0x1234));
        assert_eq!(u32_at(&q, 16), Some(10));
        // LIST2 con dos elementos: un archivo y una carpeta.
        let mut b = Vec::new();
        for v in [2u32, 2, 0, REQUEST_FULL_PATH_AND_NAME, 1] {
            b.extend_from_slice(&v.to_le_bytes());
        }
        let data_start = 20 + 2 * 8;
        let a: Vec<u16> = "C:\\a.txt".encode_utf16().collect();
        let d: Vec<u16> = "C:\\dir".encode_utf16().collect();
        let off_b = data_start + 4 + (a.len() + 1) * 2;
        for v in [0u32, data_start as u32, ITEM_FOLDER, off_b as u32] {
            b.extend_from_slice(&v.to_le_bytes());
        }
        for s in [&a, &d] {
            b.extend_from_slice(&(s.len() as u32).to_le_bytes());
            for u in s.iter().chain(std::iter::once(&0)) {
                b.extend_from_slice(&u.to_le_bytes());
            }
        }
        let r = parse_reply(&b);
        assert_eq!(
            r,
            [
                (PathBuf::from("C:\\a.txt"), false),
                (PathBuf::from("C:\\dir"), true)
            ]
        );
        assert!(parse_reply(&[1, 2]).is_empty());
    }
}
