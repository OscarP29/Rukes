//! Atajo global con `RegisterHotKey` en un hilo propio (RF-001).

use std::sync::{Arc, Mutex};

use windows::Win32::Foundation::{LPARAM, WPARAM};
use windows::Win32::System::Threading::GetCurrentThreadId;
use windows::Win32::UI::Input::KeyboardAndMouse::{
    HOT_KEY_MODIFIERS, MOD_ALT, MOD_CONTROL, MOD_NOREPEAT, MOD_SHIFT, MOD_WIN, RegisterHotKey,
    UnregisterHotKey,
};
use windows::Win32::UI::WindowsAndMessaging::{
    GetMessageW, MSG, PM_NOREMOVE, PeekMessageW, PostThreadMessageW, WM_APP, WM_HOTKEY, WM_QUIT,
};

use crate::{PlatformError, Result};

const HOTKEY_ID: i32 = 1;
const WM_SET_HOTKEY: u32 = WM_APP + 1;

/// Atajo ya traducido a códigos de Windows.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct VkHotkey {
    /// Modificadores `MOD_*`.
    pub mods: u32,
    /// Código de tecla virtual.
    pub vk: u32,
}

/// Traduce un atajo de configuración (`ctrl`, `alt`, `shift`, `win`, nombre de tecla).
pub fn to_vk(ctrl: bool, alt: bool, shift: bool, win: bool, key: &str) -> Result<VkHotkey> {
    let mut mods = MOD_NOREPEAT;
    if ctrl {
        mods |= MOD_CONTROL;
    }
    if alt {
        mods |= MOD_ALT;
    }
    if shift {
        mods |= MOD_SHIFT;
    }
    if win {
        mods |= MOD_WIN;
    }
    let vk = match key {
        "SPACE" => 0x20,
        "ENTER" => 0x0D,
        "TAB" => 0x09,
        "ESC" => 0x1B,
        "BACKSPACE" => 0x08,
        "BACKQUOTE" => 0xC0,
        k if k.len() == 1 && k.as_bytes()[0].is_ascii_alphanumeric() => u32::from(k.as_bytes()[0]),
        k if k.starts_with('F') => match k[1..].parse::<u32>() {
            Ok(n @ 1..=24) => 0x70 + n - 1,
            _ => return Err(PlatformError::Invalid(format!("tecla no soportada: {k}"))),
        },
        k => return Err(PlatformError::Invalid(format!("tecla no soportada: {k}"))),
    };
    Ok(VkHotkey { mods: mods.0, vk })
}

type Reply = crossbeam_channel::Sender<Result<()>>;

/// Gestor del atajo global. Al soltarse, el hilo termina y el atajo se libera.
pub struct HotkeyManager {
    thread_id: u32,
    pending: Arc<Mutex<Option<(VkHotkey, Reply)>>>,
}

impl HotkeyManager {
    /// Arranca el hilo del atajo. `on_press` se llama en ese hilo, que tiene permiso para poner
    /// ventanas en primer plano.
    pub fn spawn(on_press: impl Fn() + Send + 'static) -> Result<Self> {
        let pending: Arc<Mutex<Option<(VkHotkey, Reply)>>> = Arc::default();
        let (tid_tx, tid_rx) = crossbeam_channel::bounded(1);
        let slot = Arc::clone(&pending);
        std::thread::Builder::new()
            .name("rukes-hotkey".into())
            .spawn(move || {
                let mut msg = MSG::default();
                // SAFETY: crea la cola de mensajes del hilo antes de anunciar su id.
                unsafe {
                    let _ = PeekMessageW(&mut msg, None, 0, 0, PM_NOREMOVE);
                }
                // SAFETY: sin precondiciones.
                let _ = tid_tx.send(unsafe { GetCurrentThreadId() });
                let mut registered = false;
                // SAFETY: bucle de mensajes estándar del hilo; `msg` es local.
                while unsafe { GetMessageW(&mut msg, None, 0, 0) }.as_bool() {
                    match msg.message {
                        WM_HOTKEY if msg.wParam.0 as i32 == HOTKEY_ID => on_press(),
                        WM_SET_HOTKEY => {
                            let job = slot.lock().ok().and_then(|mut s| s.take());
                            if let Some((hk, reply)) = job {
                                // SAFETY: (des)registro de un atajo propio de este hilo.
                                let res = unsafe {
                                    if registered {
                                        let _ = UnregisterHotKey(None, HOTKEY_ID);
                                    }
                                    RegisterHotKey(
                                        None,
                                        HOTKEY_ID,
                                        HOT_KEY_MODIFIERS(hk.mods),
                                        hk.vk,
                                    )
                                };
                                registered = res.is_ok();
                                let _ = reply.send(res.map_err(|source| PlatformError::Win32 {
                                    context: "RegisterHotKey (¿atajo ya en uso por otra app?)",
                                    source,
                                }));
                            }
                        }
                        _ => {}
                    }
                }
                if registered {
                    // SAFETY: libera el atajo de este hilo.
                    let _ = unsafe { UnregisterHotKey(None, HOTKEY_ID) };
                }
            })
            .map_err(|e| PlatformError::Invalid(e.to_string()))?;
        let thread_id = tid_rx
            .recv()
            .map_err(|e| PlatformError::Invalid(e.to_string()))?;
        Ok(Self { thread_id, pending })
    }

    /// Registra (o cambia) el atajo. Falla si otra aplicación ya lo usa.
    pub fn set(&self, hk: VkHotkey) -> Result<()> {
        let (tx, rx) = crossbeam_channel::bounded(1);
        if let Ok(mut s) = self.pending.lock() {
            *s = Some((hk, tx));
        }
        // SAFETY: mensaje a un hilo propio y vivo.
        unsafe { PostThreadMessageW(self.thread_id, WM_SET_HOTKEY, WPARAM(0), LPARAM(0)) }
            .map_err(|source| PlatformError::Win32 {
                context: "PostThreadMessageW",
                source,
            })?;
        rx.recv()
            .map_err(|e| PlatformError::Invalid(e.to_string()))?
    }
}

impl Drop for HotkeyManager {
    fn drop(&mut self) {
        // SAFETY: mensaje a un hilo propio.
        let _ = unsafe { PostThreadMessageW(self.thread_id, WM_QUIT, WPARAM(0), LPARAM(0)) };
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn maps_keys() {
        assert_eq!(to_vk(false, true, false, false, "SPACE").unwrap().vk, 0x20);
        assert_eq!(
            to_vk(true, false, false, false, "K").unwrap().vk,
            u32::from(b'K')
        );
        assert_eq!(to_vk(false, false, false, false, "F13").unwrap().vk, 0x7C);
        assert!(to_vk(false, true, false, false, "F25").is_err());
        assert!(to_vk(false, true, false, false, "PAUSE").is_err());
        let hk = to_vk(true, true, false, false, "A").unwrap();
        assert_eq!(hk.mods, (MOD_NOREPEAT | MOD_CONTROL | MOD_ALT).0);
    }
}
