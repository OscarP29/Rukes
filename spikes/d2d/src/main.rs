//! Spike: ventana Win32 sin bordes + Direct2D/DirectWrite + Mica.
#![windows_subsystem = "windows"]

use std::cell::RefCell;

use spike_common::{self as common, color};
use windows::Win32::Foundation::{HWND, LPARAM, LRESULT, POINT, RECT, WPARAM};
use windows::Win32::Graphics::Direct2D::Common::{
    D2D_RECT_F, D2D_SIZE_U, D2D1_ALPHA_MODE_PREMULTIPLIED, D2D1_COLOR_F, D2D1_PIXEL_FORMAT,
};
use windows::Win32::Graphics::Direct2D::{
    D2D1_DRAW_TEXT_OPTIONS_CLIP, D2D1_FACTORY_TYPE_SINGLE_THREADED,
    D2D1_HWND_RENDER_TARGET_PROPERTIES, D2D1_PRESENT_OPTIONS_NONE, D2D1_RENDER_TARGET_PROPERTIES,
    D2D1_RENDER_TARGET_TYPE_DEFAULT, D2D1_ROUNDED_RECT, D2D1CreateFactory, ID2D1Factory,
    ID2D1HwndRenderTarget, ID2D1SolidColorBrush,
};
use windows::Win32::Graphics::DirectWrite::{
    DWRITE_FACTORY_TYPE_SHARED, DWRITE_FONT_STRETCH_NORMAL, DWRITE_FONT_STYLE_NORMAL,
    DWRITE_FONT_WEIGHT_NORMAL, DWRITE_FONT_WEIGHT_SEMI_BOLD, DWRITE_MEASURING_MODE_NATURAL,
    DWRITE_PARAGRAPH_ALIGNMENT_CENTER, DWriteCreateFactory, IDWriteFactory, IDWriteTextFormat,
};
use windows::Win32::Graphics::Dxgi::Common::DXGI_FORMAT_B8G8R8A8_UNORM;
use windows::Win32::Graphics::Gdi::{
    GetMonitorInfoW, InvalidateRect, MONITOR_DEFAULTTONEAREST, MONITORINFO, MonitorFromPoint,
    ValidateRect,
};
use windows::Win32::System::LibraryLoader::GetModuleHandleW;
use windows::Win32::UI::HiDpi::{
    DPI_AWARENESS_CONTEXT_PER_MONITOR_AWARE_V2, GetDpiForWindow, SetProcessDpiAwarenessContext,
};
use windows::Win32::UI::Input::KeyboardAndMouse::VK_ESCAPE;
use windows::Win32::UI::WindowsAndMessaging::{
    CS_HREDRAW, CS_VREDRAW, CreateWindowExW, DefWindowProcW, DispatchMessageW, GetClientRect,
    GetCursorPos, GetMessageW, IDC_ARROW, LoadCursorW, MSG, PostQuitMessage, RegisterClassW,
    SW_SHOW, SWP_NOZORDER, SetForegroundWindow, SetWindowPos, ShowWindow, TranslateMessage,
    WM_DESTROY, WM_DPICHANGED, WM_KEYDOWN, WM_PAINT, WM_SIZE, WNDCLASSW, WS_EX_TOOLWINDOW,
    WS_EX_TOPMOST, WS_POPUP,
};
use windows::core::{PCWSTR, Result, w};

struct Renderer {
    rt: ID2D1HwndRenderTarget,
    brush: ID2D1SolidColorBrush,
    title: IDWriteTextFormat,
    sub: IDWriteTextFormat,
    search: IDWriteTextFormat,
}

thread_local! {
    static FACTORIES: RefCell<Option<(ID2D1Factory, IDWriteFactory)>> = const { RefCell::new(None) };
    static RENDERER: RefCell<Option<Renderer>> = const { RefCell::new(None) };
}

fn rgba(c: [f32; 3], a: f32) -> D2D1_COLOR_F {
    // Premultiplicado.
    D2D1_COLOR_F { r: c[0] * a, g: c[1] * a, b: c[2] * a, a }
}

fn rect(l: f32, t: f32, r: f32, b: f32) -> D2D_RECT_F {
    D2D_RECT_F { left: l, top: t, right: r, bottom: b }
}

fn create_renderer(hwnd: HWND) -> Result<Renderer> {
    FACTORIES.with_borrow_mut(|f| {
        if f.is_none() {
            // SAFETY: creación estándar de factorías; sin punteros externos.
            let d2d: ID2D1Factory =
                unsafe { D2D1CreateFactory(D2D1_FACTORY_TYPE_SINGLE_THREADED, None)? };
            // SAFETY: idem.
            let dw: IDWriteFactory = unsafe { DWriteCreateFactory(DWRITE_FACTORY_TYPE_SHARED)? };
            *f = Some((d2d, dw));
        }
        let Some((d2d, dw)) = f.as_ref() else { unreachable!() };
        let mut rc = RECT::default();
        // SAFETY: hwnd válido, rc local.
        unsafe { GetClientRect(hwnd, &mut rc)? };
        // SAFETY: hwnd válido.
        let dpi = unsafe { GetDpiForWindow(hwnd) } as f32;
        let props = D2D1_RENDER_TARGET_PROPERTIES {
            r#type: D2D1_RENDER_TARGET_TYPE_DEFAULT,
            pixelFormat: D2D1_PIXEL_FORMAT {
                format: DXGI_FORMAT_B8G8R8A8_UNORM,
                alphaMode: D2D1_ALPHA_MODE_PREMULTIPLIED,
            },
            dpiX: dpi,
            dpiY: dpi,
            ..Default::default()
        };
        let hprops = D2D1_HWND_RENDER_TARGET_PROPERTIES {
            hwnd,
            pixelSize: D2D_SIZE_U {
                width: (rc.right - rc.left) as u32,
                height: (rc.bottom - rc.top) as u32,
            },
            presentOptions: D2D1_PRESENT_OPTIONS_NONE,
        };
        // SAFETY: estructuras locales válidas.
        unsafe {
            let rt = d2d.CreateHwndRenderTarget(&props, &hprops)?;
            let brush = rt.CreateSolidColorBrush(&rgba(color::FG, 1.0), None)?;
            let fmt = |size: f32, weight| {
                dw.CreateTextFormat(
                    w!("Segoe UI Variable Text"),
                    None,
                    weight,
                    DWRITE_FONT_STYLE_NORMAL,
                    DWRITE_FONT_STRETCH_NORMAL,
                    size,
                    w!("es-es"),
                )
            };
            let title = fmt(15.0, DWRITE_FONT_WEIGHT_SEMI_BOLD)?;
            let sub = fmt(12.5, DWRITE_FONT_WEIGHT_NORMAL)?;
            let search = fmt(20.0, DWRITE_FONT_WEIGHT_NORMAL)?;
            search.SetParagraphAlignment(DWRITE_PARAGRAPH_ALIGNMENT_CENTER)?;
            Ok(Renderer { rt, brush, title, sub, search })
        }
    })
}

fn draw_text(r: &Renderer, fmt: &IDWriteTextFormat, text: &str, rc: D2D_RECT_F) {
    let wide: Vec<u16> = text.encode_utf16().collect();
    // SAFETY: rt/brush/fmt viven en el Renderer; `wide` vive durante la llamada.
    unsafe {
        r.rt.DrawText(
            &wide,
            fmt,
            &rc,
            &r.brush,
            D2D1_DRAW_TEXT_OPTIONS_CLIP,
            DWRITE_MEASURING_MODE_NATURAL,
        );
    }
}

fn paint(r: &Renderer) -> Result<()> {
    let a = common::fade_alpha();
    let sel = common::selection_pos();
    let w = common::WIDTH;
    // SAFETY: llamadas Direct2D sobre objetos vivos, entre BeginDraw y EndDraw.
    unsafe {
        r.rt.BeginDraw();
        let t = color::TINT;
        r.rt.Clear(Some(&rgba([t[0], t[1], t[2]], t[3] * a)));

        // Campo de búsqueda.
        r.brush.SetColor(&rgba(color::FG, a));
        draw_text(r, &r.search, "chr", rect(24.0, 0.0, w - 24.0, common::SEARCH_H));
        r.brush.SetColor(&rgba(color::ACCENT, a));
        r.rt.FillRectangle(&rect(58.0, 18.0, 60.0, common::SEARCH_H - 18.0), &r.brush);
        r.brush.SetColor(&rgba(color::MUTED, a * 0.5));
        r.rt.FillRectangle(
            &rect(0.0, common::SEARCH_H, w, common::SEARCH_H + 1.0),
            &r.brush,
        );

        // Selección animada.
        let top = common::SEARCH_H + 6.0 + sel * common::ROW_H;
        r.brush.SetColor(&rgba(color::SELECTION, a));
        r.rt.FillRoundedRectangle(
            &D2D1_ROUNDED_RECT {
                rect: rect(8.0, top, w - 8.0, top + common::ROW_H),
                radiusX: 8.0,
                radiusY: 8.0,
            },
            &r.brush,
        );

        for (i, (title, sub)) in common::ITEMS.iter().enumerate() {
            let y = common::SEARCH_H + 6.0 + i as f32 * common::ROW_H;
            r.brush.SetColor(&rgba(color::ACCENT, a));
            r.rt.FillRoundedRectangle(
                &D2D1_ROUNDED_RECT {
                    rect: rect(20.0, y + 10.0, 48.0, y + 38.0),
                    radiusX: 6.0,
                    radiusY: 6.0,
                },
                &r.brush,
            );
            r.brush.SetColor(&rgba(color::FG, a));
            draw_text(r, &r.title, title, rect(62.0, y + 6.0, w - 20.0, y + 26.0));
            r.brush.SetColor(&rgba(color::MUTED, a));
            draw_text(r, &r.sub, sub, rect(62.0, y + 26.0, w - 20.0, y + 44.0));
        }
        r.rt.EndDraw(None, None)
    }
}

extern "system" fn wndproc(hwnd: HWND, msg: u32, wp: WPARAM, lp: LPARAM) -> LRESULT {
    match msg {
        WM_PAINT => {
            RENDERER.with_borrow_mut(|slot| {
                if slot.is_none() {
                    *slot = create_renderer(hwnd).ok();
                }
                if let Some(r) = slot.as_ref()
                    && paint(r).is_err()
                {
                    // Dispositivo perdido: se recrea en el próximo WM_PAINT.
                    *slot = None;
                }
            });
            // SAFETY: hwnd válido.
            unsafe {
                let _ = ValidateRect(Some(hwnd), None);
            }
            common::frame_rendered();
            if common::needs_animation() {
                // SAFETY: hwnd válido.
                unsafe {
                    let _ = InvalidateRect(Some(hwnd), None, false);
                }
            }
            LRESULT(0)
        }
        WM_SIZE => {
            RENDERER.with_borrow(|slot| {
                if let Some(r) = slot.as_ref() {
                    let w = (lp.0 & 0xFFFF) as u32;
                    let h = ((lp.0 >> 16) & 0xFFFF) as u32;
                    // SAFETY: rt vivo.
                    unsafe {
                        let _ = r.rt.Resize(&D2D_SIZE_U { width: w, height: h });
                    }
                }
            });
            LRESULT(0)
        }
        WM_DPICHANGED => {
            RENDERER.with_borrow_mut(|slot| *slot = None);
            // SAFETY: lParam de WM_DPICHANGED apunta a un RECT sugerido válido.
            unsafe {
                let r = &*(lp.0 as *const RECT);
                let _ = SetWindowPos(
                    hwnd,
                    None,
                    r.left,
                    r.top,
                    r.right - r.left,
                    r.bottom - r.top,
                    SWP_NOZORDER,
                );
            }
            LRESULT(0)
        }
        WM_KEYDOWN if wp.0 as u16 == VK_ESCAPE.0 => {
            // SAFETY: sin precondiciones.
            unsafe { PostQuitMessage(0) };
            LRESULT(0)
        }
        WM_DESTROY => {
            // SAFETY: sin precondiciones.
            unsafe { PostQuitMessage(0) };
            LRESULT(0)
        }
        // SAFETY: delegación estándar.
        _ => unsafe { DefWindowProcW(hwnd, msg, wp, lp) },
    }
}

fn main() -> Result<()> {
    // SAFETY: llamadas Win32 estándar de arranque; todos los punteros son locales o estáticos.
    unsafe {
        let _ = SetProcessDpiAwarenessContext(DPI_AWARENESS_CONTEXT_PER_MONITOR_AWARE_V2);
        let instance = GetModuleHandleW(None)?;
        let class = w!("RukesSpikeD2D");
        let wc = WNDCLASSW {
            style: CS_HREDRAW | CS_VREDRAW,
            lpfnWndProc: Some(wndproc),
            hInstance: instance.into(),
            hCursor: LoadCursorW(None, IDC_ARROW)?,
            lpszClassName: class,
            ..Default::default()
        };
        RegisterClassW(&wc);
        let hwnd = CreateWindowExW(
            WS_EX_TOOLWINDOW | WS_EX_TOPMOST,
            class,
            w!("Rukes spike D2D"),
            WS_POPUP,
            0,
            0,
            100,
            100,
            None,
            None,
            Some(instance.into()),
            None,
        )?;

        // Centrar en el monitor del cursor, escalando por DPI.
        let mut pt = POINT::default();
        let _ = GetCursorPos(&mut pt);
        let mon = MonitorFromPoint(pt, MONITOR_DEFAULTTONEAREST);
        let mut mi = MONITORINFO { cbSize: size_of::<MONITORINFO>() as u32, ..Default::default() };
        let _ = GetMonitorInfoW(mon, &mut mi);
        let scale = GetDpiForWindow(hwnd) as f32 / 96.0;
        let (w, h) = ((common::WIDTH * scale) as i32, (common::HEIGHT * scale) as i32);
        let wa = mi.rcWork;
        let x = wa.left + (wa.right - wa.left - w) / 2;
        let y = wa.top + (wa.bottom - wa.top - h) / 3;
        SetWindowPos(hwnd, None, x, y, w, h, SWP_NOZORDER)?;

        common::apply_backdrop(hwnd.0 as isize, common::backdrop_from_env());
        let raw = hwnd.0 as isize;
        common::register(raw, move || {
            // SAFETY: InvalidateRect es seguro desde cualquier hilo con un HWND vivo.
            let _ = InvalidateRect(Some(HWND(raw as *mut _)), None, false);
        });
        let _ = ShowWindow(hwnd, SW_SHOW);
        let _ = SetForegroundWindow(hwnd);

        let mut msg = MSG::default();
        while GetMessageW(&mut msg, None, 0, 0).as_bool() {
            let _ = TranslateMessage(&msg);
            DispatchMessageW(&msg);
        }
        let _ = PCWSTR::null();
    }
    Ok(())
}
