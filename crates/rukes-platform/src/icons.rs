//! Extracción de iconos del shell como RGBA (RF-196).

use windows::Win32::Foundation::SIZE;
use windows::Win32::Graphics::Gdi::{
    BI_RGB, BITMAP, BITMAPINFO, BITMAPINFOHEADER, CreateCompatibleDC, DIB_RGB_COLORS, DeleteDC,
    DeleteObject, GetDIBits, GetObjectW, HGDIOBJ,
};
use windows::Win32::UI::Shell::{
    IShellItemImageFactory, SHCreateItemFromParsingName, SIIGBF, SIIGBF_BIGGERSIZEOK,
    SIIGBF_ICONONLY, SIIGBF_THUMBNAILONLY,
};
use windows::core::PCWSTR;

/// Imagen RGBA (no premultiplicada), filas de arriba abajo.
#[derive(Debug, Clone)]
pub struct Rgba {
    /// Ancho.
    pub width: u32,
    /// Alto.
    pub height: u32,
    /// Píxeles RGBA.
    pub pixels: Vec<u8>,
}

/// Icono de un archivo, acceso directo o `shell:` URI al tamaño pedido (en píxeles físicos).
/// Debe llamarse fuera del hilo de UI: puede tardar varios milisegundos.
pub fn load(source: &str, size: u32) -> Option<Rgba> {
    image(source, size, SIIGBF_ICONONLY | SIIGBF_BIGGERSIZEOK)
}

/// Miniatura de un archivo (imágenes, vídeos, PDF... según los proveedores instalados) que
/// cabe en `size`×`size` conservando la proporción. `None` si el archivo no tiene miniatura.
pub fn thumbnail(path: &str, size: u32) -> Option<Rgba> {
    image(path, size, SIIGBF_THUMBNAILONLY)
}

fn image(source: &str, size: u32, flags: SIIGBF) -> Option<Rgba> {
    crate::com::ensure_thread();
    let name = crate::wide(source);
    let size_i = i32::try_from(size).ok()?;
    // SAFETY: COM inicializado en este hilo; `name` vive durante la llamada; el HBITMAP devuelto
    // es nuestro y se libera con DeleteObject; el DC se crea y destruye aquí.
    unsafe {
        let factory: IShellItemImageFactory =
            SHCreateItemFromParsingName(PCWSTR(name.as_ptr()), None).ok()?;
        let hbmp = factory
            .GetImage(
                SIZE {
                    cx: size_i,
                    cy: size_i,
                },
                flags,
            )
            .ok()?;
        let obj = HGDIOBJ(hbmp.0);
        let mut bm = BITMAP::default();
        if GetObjectW(obj, size_of::<BITMAP>() as i32, Some((&raw mut bm).cast())) == 0 {
            let _ = DeleteObject(obj);
            return None;
        }
        let (w, h) = (bm.bmWidth, bm.bmHeight.abs());
        let mut info = BITMAPINFO {
            bmiHeader: BITMAPINFOHEADER {
                biSize: size_of::<BITMAPINFOHEADER>() as u32,
                biWidth: w,
                biHeight: -h,
                biPlanes: 1,
                biBitCount: 32,
                biCompression: BI_RGB.0,
                ..Default::default()
            },
            ..Default::default()
        };
        let mut buf = vec![0u8; (w * h * 4) as usize];
        let dc = CreateCompatibleDC(None);
        let lines = GetDIBits(
            dc,
            hbmp,
            0,
            h as u32,
            Some(buf.as_mut_ptr().cast()),
            &mut info,
            DIB_RGB_COLORS,
        );
        let _ = DeleteDC(dc);
        let _ = DeleteObject(obj);
        if lines == 0 {
            return None;
        }
        Some(Rgba {
            width: w as u32,
            height: h as u32,
            pixels: bgra_premul_to_rgba(buf),
        })
    }
}

/// BGRA premultiplicado → RGBA. Si ningún píxel tiene alfa, la imagen es opaca.
fn bgra_premul_to_rgba(mut px: Vec<u8>) -> Vec<u8> {
    let has_alpha = px.chunks_exact(4).any(|p| p[3] != 0);
    for p in px.chunks_exact_mut(4) {
        let (b, g, r, a) = (p[0], p[1], p[2], if has_alpha { p[3] } else { 255 });
        let un = |c: u8| {
            if a == 0 || a == 255 {
                c
            } else {
                ((u16::from(c) * 255 + u16::from(a) / 2) / u16::from(a)).min(255) as u8
            }
        };
        p[0] = un(r);
        p[1] = un(g);
        p[2] = un(b);
        p[3] = a;
    }
    px
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn converts_and_unpremultiplies() {
        // B=0 G=64 R=128 A=128 premultiplicado → R=255 G=128 B=0.
        let out = bgra_premul_to_rgba(vec![0, 64, 128, 128]);
        assert_eq!(out, vec![255, 128, 0, 128]);
    }

    #[test]
    fn no_alpha_means_opaque() {
        let out = bgra_premul_to_rgba(vec![1, 2, 3, 0, 4, 5, 6, 0]);
        assert_eq!(out, vec![3, 2, 1, 255, 6, 5, 4, 255]);
    }

    #[test]
    fn loads_an_icon() {
        let windir = std::env::var("WINDIR").unwrap_or_else(|_| "C:\\Windows".into());
        let img = load(&format!("{windir}\\explorer.exe"), 32).unwrap();
        assert!(img.width >= 32 && img.height >= 32);
        assert_eq!(img.pixels.len(), (img.width * img.height * 4) as usize);
    }
}
