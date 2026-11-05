//! Lanzar programas, abrir ubicaciones y resolver accesos directos (RF-032).

use std::path::{Path, PathBuf};

use windows::Win32::Foundation::CloseHandle;
use windows::Win32::System::Com::{
    CLSCTX_INPROC_SERVER, CoCreateInstance, IPersistFile, STGM_READ,
};
use windows::Win32::System::Threading::GetProcessId;
use windows::Win32::UI::Shell::{
    FO_DELETE, FOF_ALLOWUNDO, FOF_NOCONFIRMATION, FOF_NOERRORUI, FOF_SILENT, IShellLinkW,
    SEE_MASK_FLAG_NO_UI, SEE_MASK_NOASYNC, SEE_MASK_NOCLOSEPROCESS, SHELLEXECUTEINFOW,
    SHFILEOPSTRUCTW, SHFileOperationW, ShellExecuteExW, ShellLink,
};
use windows::Win32::UI::WindowsAndMessaging::{SW_HIDE, SW_SHOWNORMAL};
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
    launch_with(target, args, cwd, verb, true)
}

/// Como [`launch`], pero permite lanzar con la ventana oculta (`visible = false`).
pub fn launch_with(
    target: &str,
    args: Option<&str>,
    cwd: Option<&Path>,
    verb: Verb,
    visible: bool,
) -> Result<()> {
    exec(target, args, cwd, verb, visible, false).map(|_| ())
}

/// Como [`launch`], pero devuelve el PID del proceso creado, si ShellExecute creó uno (al abrir
/// una URL en un navegador ya abierto, o un documento, puede no haberlo).
pub fn launch_pid(target: &str, args: Option<&str>, cwd: Option<&Path>) -> Result<Option<u32>> {
    exec(target, args, cwd, Verb::Open, true, true)
}

fn exec(
    target: &str,
    args: Option<&str>,
    cwd: Option<&Path>,
    verb: Verb,
    visible: bool,
    want_pid: bool,
) -> Result<Option<u32>> {
    crate::com::ensure_thread();
    let file = wide(target);
    let params = args.map(wide);
    let dir = cwd.map(|d| wide(&d.to_string_lossy()));
    let mut info = SHELLEXECUTEINFOW {
        cbSize: size_of::<SHELLEXECUTEINFOW>() as u32,
        fMask: SEE_MASK_NOASYNC
            | SEE_MASK_FLAG_NO_UI
            | if want_pid { SEE_MASK_NOCLOSEPROCESS } else { 0 },
        lpVerb: match verb {
            Verb::Open => w!("open"),
            Verb::RunAs => w!("runas"),
        },
        lpFile: PCWSTR(file.as_ptr()),
        lpParameters: params
            .as_ref()
            .map_or(PCWSTR::null(), |p| PCWSTR(p.as_ptr())),
        lpDirectory: dir.as_ref().map_or(PCWSTR::null(), |d| PCWSTR(d.as_ptr())),
        nShow: if visible { SW_SHOWNORMAL.0 } else { SW_HIDE.0 },
        ..Default::default()
    };
    // SAFETY: todos los punteros de `info` apuntan a buffers locales vivos durante la llamada.
    unsafe { ShellExecuteExW(&mut info) }.ctx("ShellExecuteExW")?;
    if info.hProcess.is_invalid() {
        return Ok(None);
    }
    // SAFETY: handle devuelto por ShellExecuteExW con SEE_MASK_NOCLOSEPROCESS; se cierra aquí.
    let pid = unsafe {
        let pid = GetProcessId(info.hProcess);
        let _ = CloseHandle(info.hProcess);
        pid
    };
    Ok((pid != 0).then_some(pid))
}

/// Mueve un archivo o carpeta a la papelera de reciclaje, sin diálogos.
pub fn recycle(path: &Path) -> Result<()> {
    // La lista de rutas termina en doble cero.
    let mut from = wide(&path.to_string_lossy());
    from.push(0);
    let mut op = SHFILEOPSTRUCTW {
        wFunc: FO_DELETE,
        pFrom: PCWSTR(from.as_ptr()),
        fFlags: (FOF_ALLOWUNDO | FOF_NOCONFIRMATION | FOF_SILENT | FOF_NOERRORUI).0 as u16,
        ..Default::default()
    };
    // SAFETY: `from` vive durante la llamada y termina en doble cero.
    let code = unsafe { SHFileOperationW(&mut op) };
    if code != 0 || op.fAnyOperationsAborted.as_bool() {
        return Err(crate::PlatformError::Invalid(format!(
            "no se pudo mover a la papelera (código {code})"
        )));
    }
    Ok(())
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

#[cfg(test)]
mod tests {
    // Ignorado por defecto: deja un archivo en la papelera real del usuario.
    #[test]
    #[ignore]
    fn recycle_moves_a_temp_file_away() {
        let path = std::env::temp_dir().join(format!("rukes-recycle-{}.txt", std::process::id()));
        std::fs::write(&path, "x").unwrap();
        super::recycle(&path).unwrap();
        assert!(!path.exists());
    }
}
