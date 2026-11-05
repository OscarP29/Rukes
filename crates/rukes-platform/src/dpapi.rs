//! Cifrado ligado al usuario de Windows con DPAPI (historial del portapapeles, RF-104).

use windows::Win32::Foundation::{HLOCAL, LocalFree};
use windows::Win32::Security::Cryptography::{
    CRYPT_INTEGER_BLOB, CRYPTPROTECT_UI_FORBIDDEN, CryptProtectData, CryptUnprotectData,
};
use windows::core::w;

use crate::{Context as _, Result};

/// Entropía adicional fija: otros programas que usen DPAPI con el mismo usuario no descifran
/// estos datos por accidente.
const ENTROPY: &[u8] = b"rukes-clipboard-v1";

fn blob(data: &[u8]) -> CRYPT_INTEGER_BLOB {
    CRYPT_INTEGER_BLOB {
        cbData: data.len() as u32,
        pbData: data.as_ptr().cast_mut(),
    }
}

/// Copia el resultado de DPAPI y libera su memoria.
///
/// SAFETY: `out` debe venir de una llamada correcta a `CryptProtectData`/`CryptUnprotectData`.
unsafe fn take(out: CRYPT_INTEGER_BLOB) -> Vec<u8> {
    // SAFETY: DPAPI garantiza `cbData` bytes válidos en `pbData`.
    let v = unsafe { std::slice::from_raw_parts(out.pbData, out.cbData as usize).to_vec() };
    // SAFETY: memoria reservada por DPAPI con LocalAlloc.
    unsafe {
        let _ = LocalFree(Some(HLOCAL(out.pbData.cast())));
    }
    v
}

/// Cifra `data` para el usuario actual.
pub fn protect(data: &[u8]) -> Result<Vec<u8>> {
    let input = blob(data);
    let entropy = blob(ENTROPY);
    let mut out = CRYPT_INTEGER_BLOB::default();
    // SAFETY: los blobs apuntan a memoria viva durante la llamada; `out` es local.
    unsafe {
        CryptProtectData(
            &input,
            w!("Rukes"),
            Some(&entropy),
            None,
            None,
            CRYPTPROTECT_UI_FORBIDDEN,
            &mut out,
        )
        .ctx("CryptProtectData")?;
        Ok(take(out))
    }
}

/// Descifra datos de [`protect`].
pub fn unprotect(data: &[u8]) -> Result<Vec<u8>> {
    let input = blob(data);
    let entropy = blob(ENTROPY);
    let mut out = CRYPT_INTEGER_BLOB::default();
    // SAFETY: como en `protect`.
    unsafe {
        CryptUnprotectData(
            &input,
            None,
            Some(&entropy),
            None,
            None,
            CRYPTPROTECT_UI_FORBIDDEN,
            &mut out,
        )
        .ctx("CryptUnprotectData")?;
        Ok(take(out))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn roundtrip() {
        let secret = "contraseña de prueba ñ".as_bytes();
        let enc = protect(secret).unwrap();
        assert_ne!(enc, secret);
        assert!(!enc.windows(secret.len()).any(|w| w == secret));
        assert_eq!(unprotect(&enc).unwrap(), secret);
        assert!(unprotect(b"basura").is_err());
        assert_eq!(unprotect(&protect(b"").unwrap()).unwrap(), b"");
    }
}
