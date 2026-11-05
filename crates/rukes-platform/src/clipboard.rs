//! Portapapeles (texto).

use windows::Win32::Foundation::{GlobalFree, HANDLE, HGLOBAL};
use windows::Win32::System::DataExchange::{
    CloseClipboard, EmptyClipboard, GetClipboardData, IsClipboardFormatAvailable, OpenClipboard,
    RegisterClipboardFormatW, SetClipboardData,
};
use windows::Win32::System::Memory::{
    GMEM_MOVEABLE, GlobalAlloc, GlobalLock, GlobalSize, GlobalUnlock,
};
use windows::Win32::System::Ole::CF_UNICODETEXT;
use windows::core::w;

use crate::{Context as _, Result};

/// Copia texto al portapapeles.
pub fn set_text(text: &str) -> Result<()> {
    set(text, false)
}

/// Copia texto marcándolo para que los historiales (el de Windows y el de Rukes) no lo
/// registren: se usa al pegar desde el historial o al insertar un snippet.
pub fn set_text_private(text: &str) -> Result<()> {
    set(text, true)
}

fn set(text: &str, private: bool) -> Result<()> {
    let data = crate::wide(text);
    let bytes = data.len() * 2;
    // SAFETY: secuencia estándar Open/Empty/Set/Close. La memoria global pasa a ser del sistema
    // si SetClipboardData tiene éxito; si falla, se libera aquí.
    unsafe {
        OpenClipboard(None).ctx("OpenClipboard")?;
        let res = (|| {
            EmptyClipboard().ctx("EmptyClipboard")?;
            let mem: HGLOBAL = GlobalAlloc(GMEM_MOVEABLE, bytes).ctx("GlobalAlloc")?;
            let ptr = GlobalLock(mem).cast::<u16>();
            if ptr.is_null() {
                let _ = GlobalFree(Some(mem));
                return Err(crate::PlatformError::Invalid("GlobalLock".into()));
            }
            std::ptr::copy_nonoverlapping(data.as_ptr(), ptr, data.len());
            let _ = GlobalUnlock(mem);
            if let Err(e) = SetClipboardData(u32::from(CF_UNICODETEXT.0), Some(HANDLE(mem.0))) {
                let _ = GlobalFree(Some(mem));
                return Err(e).ctx("SetClipboardData");
            }
            if private {
                let fmt =
                    RegisterClipboardFormatW(w!("ExcludeClipboardContentFromMonitorProcessing"));
                let flag: HGLOBAL = GlobalAlloc(GMEM_MOVEABLE, 4).ctx("GlobalAlloc")?;
                if SetClipboardData(fmt, Some(HANDLE(flag.0))).is_err() {
                    let _ = GlobalFree(Some(flag));
                }
            }
            Ok(())
        })();
        let _ = CloseClipboard();
        res
    }
}

/// Texto del portapapeles, si lo hay (como mucho `max_chars` caracteres UTF-16).
pub fn get_text(max_chars: usize) -> Option<String> {
    // SAFETY: secuencia estándar Open/Get/Lock/Unlock/Close; se lee como mucho el tamaño del
    // bloque global y hasta el primer cero.
    unsafe {
        IsClipboardFormatAvailable(u32::from(CF_UNICODETEXT.0)).ok()?;
        OpenClipboard(None).ok()?;
        let res = (|| {
            let h = GetClipboardData(u32::from(CF_UNICODETEXT.0)).ok()?;
            let mem = HGLOBAL(h.0);
            let ptr = GlobalLock(mem).cast::<u16>();
            if ptr.is_null() {
                return None;
            }
            let cap = (GlobalSize(mem) / 2).min(max_chars);
            let slice = std::slice::from_raw_parts(ptr, cap);
            let len = slice.iter().position(|&c| c == 0).unwrap_or(cap);
            let text = String::from_utf16_lossy(&slice[..len]);
            let _ = GlobalUnlock(mem);
            Some(text)
        })();
        let _ = CloseClipboard();
        res
    }
}
