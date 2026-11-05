//! Teclas sintéticas: pegar en la ventana anterior (RF-103, RF-112).

use windows::Win32::UI::Input::KeyboardAndMouse::{GetAsyncKeyState, VK_LCONTROL, VK_RCONTROL};
use windows::Win32::UI::Input::KeyboardAndMouse::{
    INPUT, INPUT_0, INPUT_KEYBOARD, KEYBD_EVENT_FLAGS, KEYBDINPUT, KEYEVENTF_EXTENDEDKEY,
    KEYEVENTF_KEYUP, SendInput, VIRTUAL_KEY, VK_CONTROL, VK_LEFT, VK_LMENU, VK_LSHIFT, VK_LWIN,
    VK_MENU, VK_RMENU, VK_RSHIFT, VK_RWIN, VK_SHIFT, VK_V,
};

fn key(vk: VIRTUAL_KEY, up: bool) -> INPUT {
    let mut flags = if up {
        KEYEVENTF_KEYUP
    } else {
        KEYBD_EVENT_FLAGS(0)
    };
    if vk == VK_LEFT {
        flags |= KEYEVENTF_EXTENDEDKEY;
    }
    INPUT {
        r#type: INPUT_KEYBOARD,
        Anonymous: INPUT_0 {
            ki: KEYBDINPUT {
                wVk: vk,
                dwFlags: flags,
                ..Default::default()
            },
        },
    }
}

fn send(inputs: &[INPUT]) -> bool {
    // SAFETY: slice local de estructuras INPUT completas.
    let n = unsafe { SendInput(inputs, size_of::<INPUT>() as i32) };
    n as usize == inputs.len()
}

/// Suelta los modificadores que siga pulsando el usuario (p. ej. el Alt del atajo o el Ctrl de
/// `Ctrl+Enter`): si no, el `Ctrl+V` llegaría como `Ctrl+Alt+V`.
fn release_modifiers() {
    let mods = [
        VK_SHIFT,
        VK_LSHIFT,
        VK_RSHIFT,
        VK_CONTROL,
        VK_LCONTROL,
        VK_RCONTROL,
        VK_MENU,
        VK_LMENU,
        VK_RMENU,
        VK_LWIN,
        VK_RWIN,
    ];
    let held: Vec<INPUT> = mods
        .iter()
        // SAFETY: consulta de estado sin punteros.
        .filter(|vk| unsafe { GetAsyncKeyState(i32::from(vk.0)) } < 0)
        .map(|vk| key(*vk, true))
        .collect();
    if !held.is_empty() {
        send(&held);
    }
}

/// Pulsa `Ctrl+V` en la ventana con el foco y luego `←` `left` veces (para dejar el cursor donde
/// marca `{cursor}`). Devuelve `false` si el sistema bloqueó la entrada (UIPI).
pub fn paste(left: usize) -> bool {
    release_modifiers();
    let mut seq = vec![
        key(VK_CONTROL, false),
        key(VK_V, false),
        key(VK_V, true),
        key(VK_CONTROL, true),
    ];
    for _ in 0..left.min(10_000) {
        seq.push(key(VK_LEFT, false));
        seq.push(key(VK_LEFT, true));
    }
    send(&seq)
}
