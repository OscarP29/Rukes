//! Portapapeles (texto).

use windows::Win32::Foundation::{GlobalFree, HANDLE, HGLOBAL};
use windows::Win32::System::DataExchange::{
    CloseClipboard, EmptyClipboard, OpenClipboard, SetClipboardData,
};
use windows::Win32::System::Memory::{GMEM_MOVEABLE, GlobalAlloc, GlobalLock, GlobalUnlock};
use windows::Win32::System::Ole::CF_UNICODETEXT;

use crate::{Context as _, Result};

/// Copia texto al portapapeles.
pub fn set_text(text: &str) -> Result<()> {
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
            Ok(())
        })();
        let _ = CloseClipboard();
        res
    }
}
