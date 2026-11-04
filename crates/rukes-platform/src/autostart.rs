//! Iniciar con Windows mediante la clave `Run` del usuario (RF-008).

use windows::Win32::Foundation::ERROR_FILE_NOT_FOUND;
use windows::Win32::System::Registry::{
    HKEY_CURRENT_USER, REG_SZ, RRF_RT_REG_SZ, RegDeleteKeyValueW, RegGetValueW, RegSetKeyValueW,
};
use windows::core::PCWSTR;

use crate::{PlatformError, Result, wide};

const RUN: &str = r"Software\Microsoft\Windows\CurrentVersion\Run";

fn get(subkey: &str, name: &str) -> Option<String> {
    let (k, n) = (wide(subkey), wide(name));
    let mut buf = vec![0u16; 2048];
    let mut bytes = (buf.len() * 2) as u32;
    // SAFETY: buffer local con su tamaño en bytes.
    let err = unsafe {
        RegGetValueW(
            HKEY_CURRENT_USER,
            PCWSTR(k.as_ptr()),
            PCWSTR(n.as_ptr()),
            RRF_RT_REG_SZ,
            None,
            Some(buf.as_mut_ptr().cast()),
            Some(&mut bytes),
        )
    };
    if err.is_err() {
        return None;
    }
    let len = (bytes as usize / 2).saturating_sub(1).min(buf.len());
    Some(String::from_utf16_lossy(&buf[..len]))
}

fn set_value(subkey: &str, name: &str, command: Option<&str>) -> Result<()> {
    let (k, n) = (wide(subkey), wide(name));
    let err = match command {
        Some(cmd) => {
            if get(subkey, name).as_deref() == Some(cmd) {
                return Ok(());
            }
            let data = wide(cmd);
            // SAFETY: cadenas locales terminadas en cero; el tamaño incluye el terminador.
            unsafe {
                RegSetKeyValueW(
                    HKEY_CURRENT_USER,
                    PCWSTR(k.as_ptr()),
                    PCWSTR(n.as_ptr()),
                    REG_SZ.0,
                    Some(data.as_ptr().cast()),
                    (data.len() * 2) as u32,
                )
            }
        }
        // SAFETY: cadenas locales terminadas en cero.
        None => match unsafe {
            RegDeleteKeyValueW(HKEY_CURRENT_USER, PCWSTR(k.as_ptr()), PCWSTR(n.as_ptr()))
        } {
            e if e == ERROR_FILE_NOT_FOUND => return Ok(()),
            e => e,
        },
    };
    err.ok().map_err(|source| PlatformError::Win32 {
        context: "registro (Run)",
        source,
    })
}

/// Comando registrado para `name`, si lo hay.
pub fn current(name: &str) -> Option<String> {
    get(RUN, name)
}

/// Activa (`Some(comando)`) o desactiva (`None`) el inicio con Windows. No escribe si el valor
/// ya es el mismo.
pub fn set(name: &str, command: Option<&str>) -> Result<()> {
    set_value(RUN, name, command)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn set_get_delete_in_a_test_key() {
        // Clave propia de pruebas: no toca la clave Run real.
        let key = r"Software\Rukes-tests";
        let name = format!("autostart-{}", std::process::id());
        set_value(key, &name, Some("\"C:\\x\\rukes.exe\" --hidden")).unwrap();
        assert_eq!(
            get(key, &name).as_deref(),
            Some("\"C:\\x\\rukes.exe\" --hidden")
        );
        set_value(key, &name, None).unwrap();
        assert_eq!(get(key, &name), None);
        // Borrar lo que no existe no es un error.
        set_value(key, &name, None).unwrap();
        // SAFETY: borra la clave de pruebas creada arriba.
        let _ = unsafe {
            windows::Win32::System::Registry::RegDeleteKeyW(
                HKEY_CURRENT_USER,
                PCWSTR(wide(key).as_ptr()),
            )
        };
    }
}
