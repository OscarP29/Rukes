//! Ventana activa y archivos seleccionados en el Explorador (RF-141).

use std::path::PathBuf;

use windows::Win32::Foundation::HWND;
use windows::Win32::System::Com::{CLSCTX_ALL, CoCreateInstance, CoTaskMemFree, IServiceProvider};
use windows::Win32::System::Variant::VARIANT;
use windows::Win32::UI::Shell::{
    IFolderView, IShellBrowser, IShellItemArray, IShellWindows, IWebBrowser2, SID_STopLevelBrowser,
    SIGDN_FILESYSPATH, SVGIO_SELECTION, ShellWindows,
};
use windows::Win32::UI::WindowsAndMessaging::{
    GetClassNameW, GetWindowTextLengthW, GetWindowTextW, GetWindowThreadProcessId,
};
use windows::core::Interface;

use crate::Hwnd;

/// Datos de una ventana de nivel superior.
#[derive(Debug, Clone, Default)]
pub struct WindowContext {
    /// Título.
    pub title: String,
    /// Clase de ventana.
    pub class: String,
    /// Ejecutable del proceso, si se puede leer.
    pub exe: Option<PathBuf>,
}

/// Título, clase y ejecutable de la ventana. Vacío si el handle ya no existe.
pub fn window_context(hwnd: Hwnd) -> WindowContext {
    if hwnd == 0 {
        return WindowContext::default();
    }
    let h = HWND(hwnd as *mut _);
    let mut pid = 0u32;
    // SAFETY: consultas sin efectos; los buffers tienen el tamaño indicado.
    let (title, class) = unsafe {
        GetWindowThreadProcessId(h, Some(&mut pid));
        let len = GetWindowTextLengthW(h).max(0) as usize;
        let mut buf = vec![0u16; len + 1];
        let n = GetWindowTextW(h, &mut buf).max(0) as usize;
        let mut cls = [0u16; 256];
        let c = GetClassNameW(h, &mut cls).max(0) as usize;
        (
            String::from_utf16_lossy(&buf[..n]),
            String::from_utf16_lossy(&cls[..c]),
        )
    };
    WindowContext {
        title,
        class,
        exe: (pid != 0).then(|| crate::process::exe_path(pid)).flatten(),
    }
}

/// Archivos seleccionados en la ventana del Explorador `hwnd` (vacío si no es una).
pub fn selection(hwnd: Hwnd) -> Vec<PathBuf> {
    if hwnd == 0 {
        return Vec::new();
    }
    crate::com::ensure_thread();
    // SAFETY: llamadas COM estándar en un hilo con COM inicializado; las cadenas devueltas por
    // GetDisplayName se liberan con CoTaskMemFree.
    unsafe {
        let Ok(windows) = CoCreateInstance::<_, IShellWindows>(&ShellWindows, None, CLSCTX_ALL)
        else {
            return Vec::new();
        };
        let count = windows.Count().unwrap_or(0);
        for i in 0..count {
            let Ok(disp) = windows.Item(&VARIANT::from(i)) else {
                continue;
            };
            let Ok(browser) = disp.cast::<IWebBrowser2>() else {
                continue;
            };
            if browser.HWND().map(|h| h.0) != Ok(hwnd) {
                continue;
            }
            let Ok(sp) = browser.cast::<IServiceProvider>() else {
                return Vec::new();
            };
            let Ok(sb) = sp.QueryService::<IShellBrowser>(&SID_STopLevelBrowser) else {
                return Vec::new();
            };
            let Ok(view) = sb.QueryActiveShellView() else {
                return Vec::new();
            };
            let Ok(fv) = view.cast::<IFolderView>() else {
                return Vec::new();
            };
            let Ok(items) = fv.Items::<IShellItemArray>(SVGIO_SELECTION) else {
                return Vec::new();
            };
            let n = items.GetCount().unwrap_or(0);
            let mut out = Vec::new();
            for j in 0..n {
                let Ok(item) = items.GetItemAt(j) else {
                    continue;
                };
                if let Ok(name) = item.GetDisplayName(SIGDN_FILESYSPATH) {
                    if let Ok(s) = name.to_string() {
                        out.push(PathBuf::from(s));
                    }
                    CoTaskMemFree(Some(name.0.cast()));
                }
            }
            return out;
        }
        Vec::new()
    }
}

#[cfg(test)]
mod tests {
    #[test]
    fn unknown_window_has_no_selection() {
        assert!(super::selection(0).is_empty());
        assert!(super::selection(0x1234).is_empty());
        assert!(super::window_context(0).title.is_empty());
    }
}
