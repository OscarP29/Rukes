//! Lanzar programas, abrir ubicaciones y resolver accesos directos (RF-032).

use std::path::{Path, PathBuf};

use windows::Win32::System::Com::{
    CLSCTX_INPROC_SERVER, CoCreateInstance, IPersistFile, STGM_READ,
};
use windows::Win32::UI::Shell::{
    IShellLinkW, SEE_MASK_FLAG_NO_UI, SEE_MASK_NOASYNC, SHELLEXECUTEINFOW, ShellExecuteExW,
    ShellLink,
};
use windows::Win32::UI::WindowsAndMessaging::SW_SHOWNORMAL;
use windows::core::{Interface, PCWSTR, w};

use crate::{Context as _, Result, wide};

/// Cómo lanzar.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Verb {
    /// Abrir normalmente.
    Open,
    /// Abrir como administrador (UAC).
    RunAs,
}

/// Abre un archivo, acceso directo, URL o `shell:` URI con `ShellExecuteExW`.
pub fn launch(target: &str, args: Option<&str>, cwd: Option<&Path>, verb: Verb) -> Result<()> {
    crate::com::ensure_thread();
    let file = wide(target);
    let params = args.map(wide);
    let dir = cwd.map(|d| wide(&d.to_string_lossy()));
    let mut info = SHELLEXECUTEINFOW {
        cbSize: size_of::<SHELLEXECUTEINFOW>() as u32,
        fMask: SEE_MASK_NOASYNC | SEE_MASK_FLAG_NO_UI,
        lpVerb: match verb {
            Verb::Open => w!("open"),
            Verb::RunAs => w!("runas"),
        },
        lpFile: PCWSTR(file.as_ptr()),
        lpParameters: params
            .as_ref()
            .map_or(PCWSTR::null(), |p| PCWSTR(p.as_ptr())),
        lpDirectory: dir.as_ref().map_or(PCWSTR::null(), |d| PCWSTR(d.as_ptr())),
        nShow: SW_SHOWNORMAL.0,
        ..Default::default()
    };
    // SAFETY: todos los punteros de `info` apuntan a buffers locales vivos durante la llamada.
    unsafe { ShellExecuteExW(&mut info) }.ctx("ShellExecuteExW")
}

/// Abre el Explorador con el archivo seleccionado.
pub fn open_location(path: &Path) -> Result<()> {
    let args = format!("/select,\"{}\"", path.display());
    launch("explorer.exe", Some(&args), None, Verb::Open)
}

/// Devuelve la ruta de destino de un `.lnk`, si apunta a un archivo.
pub fn resolve_shortcut(lnk: &Path) -> Option<PathBuf> {
    crate::com::ensure_thread();
    let path = wide(&lnk.to_string_lossy());
    // SAFETY: creación COM estándar en un hilo con COM inicializado; buffers locales.
    unsafe {
        let link: IShellLinkW = CoCreateInstance(&ShellLink, None, CLSCTX_INPROC_SERVER).ok()?;
        let file: IPersistFile = link.cast().ok()?;
        file.Load(PCWSTR(path.as_ptr()), STGM_READ).ok()?;
        let mut buf = [0u16; 1024];
        link.GetPath(&mut buf, std::ptr::null_mut(), 0).ok()?;
        let len = buf.iter().position(|&c| c == 0).unwrap_or(buf.len());
        if len == 0 {
            return None;
        }
        Some(PathBuf::from(String::from_utf16_lossy(&buf[..len])))
    }
}
