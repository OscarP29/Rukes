//! Comandos de energía y sesión (RF-062).

use windows::Win32::Foundation::{CloseHandle, HANDLE, LUID};
use windows::Win32::Security::{
    AdjustTokenPrivileges, LUID_AND_ATTRIBUTES, LookupPrivilegeValueW, SE_PRIVILEGE_ENABLED,
    SE_SHUTDOWN_NAME, TOKEN_ADJUST_PRIVILEGES, TOKEN_PRIVILEGES, TOKEN_QUERY,
};
use windows::Win32::System::Power::SetSuspendState;
use windows::Win32::System::Shutdown::{
    EWX_LOGOFF, EWX_POWEROFF, EWX_REBOOT, EXIT_WINDOWS_FLAGS, ExitWindowsEx, LockWorkStation,
    SHTDN_REASON_FLAG_PLANNED, SHTDN_REASON_MAJOR_OTHER,
};
use windows::Win32::System::Threading::{GetCurrentProcess, OpenProcessToken};
use windows::Win32::UI::Shell::{SHERB_NOCONFIRMATION, SHEmptyRecycleBinW};
use windows::core::PCWSTR;

use crate::{Context as _, PlatformError, Result};

/// Comando del sistema.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Command {
    /// Bloquear la sesión.
    Lock,
    /// Suspender.
    Sleep,
    /// Hibernar.
    Hibernate,
    /// Apagar.
    Shutdown,
    /// Reiniciar.
    Restart,
    /// Cerrar sesión.
    LogOff,
    /// Vaciar la papelera de reciclaje.
    EmptyRecycleBin,
}

/// Ejecuta el comando.
pub fn run(cmd: Command) -> Result<()> {
    match cmd {
        // SAFETY: sin precondiciones.
        Command::Lock => unsafe { LockWorkStation() }.ctx("LockWorkStation"),
        Command::Sleep | Command::Hibernate => {
            // SAFETY: sin punteros.
            let ok = unsafe { SetSuspendState(cmd == Command::Hibernate, false, false) };
            if ok {
                Ok(())
            } else {
                Err(PlatformError::Win32 {
                    context: "SetSuspendState",
                    source: windows::core::Error::from_thread(),
                })
            }
        }
        Command::Shutdown => exit_windows(EWX_POWEROFF),
        Command::Restart => exit_windows(EWX_REBOOT),
        Command::LogOff => exit_windows(EWX_LOGOFF),
        Command::EmptyRecycleBin => {
            // SAFETY: ruta nula = todas las unidades. Con la papelera ya vacía devuelve un error
            // que se ignora.
            let _ = unsafe { SHEmptyRecycleBinW(None, PCWSTR::null(), SHERB_NOCONFIRMATION) };
            Ok(())
        }
    }
}

fn exit_windows(flags: EXIT_WINDOWS_FLAGS) -> Result<()> {
    if flags != EWX_LOGOFF {
        enable_shutdown_privilege()?;
    }
    // SAFETY: sin punteros.
    unsafe { ExitWindowsEx(flags, SHTDN_REASON_MAJOR_OTHER | SHTDN_REASON_FLAG_PLANNED) }
        .ctx("ExitWindowsEx")
}

fn enable_shutdown_privilege() -> Result<()> {
    let mut token = HANDLE::default();
    // SAFETY: `token` es una variable local; se cierra abajo.
    unsafe {
        OpenProcessToken(
            GetCurrentProcess(),
            TOKEN_ADJUST_PRIVILEGES | TOKEN_QUERY,
            &mut token,
        )
    }
    .ctx("OpenProcessToken")?;
    let mut luid = LUID::default();
    // SAFETY: `SE_SHUTDOWN_NAME` es una constante estática y `luid` es local.
    let res = unsafe { LookupPrivilegeValueW(PCWSTR::null(), SE_SHUTDOWN_NAME, &mut luid) }
        .ctx("LookupPrivilegeValueW")
        .and_then(|()| {
            let tp = TOKEN_PRIVILEGES {
                PrivilegeCount: 1,
                Privileges: [LUID_AND_ATTRIBUTES {
                    Luid: luid,
                    Attributes: SE_PRIVILEGE_ENABLED,
                }],
            };
            // SAFETY: `tp` vive durante la llamada; no se pide el estado anterior.
            unsafe { AdjustTokenPrivileges(token, false, Some(&tp), 0, None, None) }
                .ctx("AdjustTokenPrivileges")
        });
    // SAFETY: handle abierto arriba.
    let _ = unsafe { CloseHandle(token) };
    res
}
