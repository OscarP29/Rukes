//! Interpretación de atajos como `Alt+Space` o `Ctrl+Shift+K`.

use std::fmt;

/// Modificadores de un atajo.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Hash)]
pub struct Modifiers {
    /// Ctrl.
    pub ctrl: bool,
    /// Alt.
    pub alt: bool,
    /// Shift.
    pub shift: bool,
    /// Tecla Windows.
    pub win: bool,
}

/// Atajo global: modificadores y una tecla (nombre normalizado en mayúsculas).
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct Hotkey {
    /// Modificadores.
    pub mods: Modifiers,
    /// Tecla: `SPACE`, `A`..`Z`, `0`..`9`, `F1`..`F24`, `ENTER`, `TAB`, `ESC`, etc.
    pub key: String,
}

/// Error al interpretar un atajo.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum HotkeyError {
    /// Texto vacío.
    #[error("el atajo está vacío")]
    Empty,
    /// Falta la tecla principal.
    #[error("falta la tecla principal en «{0}»")]
    NoKey(String),
    /// Hay más de una tecla principal.
    #[error("hay más de una tecla principal en «{0}»")]
    MultipleKeys(String),
    /// Sin modificadores: capturaría la tecla en todo el sistema.
    #[error("el atajo «{0}» necesita al menos un modificador")]
    NoModifier(String),
}

impl Hotkey {
    /// Interpreta un texto como `Alt+Space`. Insensible a mayúsculas y espacios.
    pub fn parse(s: &str) -> Result<Self, HotkeyError> {
        if s.trim().is_empty() {
            return Err(HotkeyError::Empty);
        }
        let mut mods = Modifiers::default();
        let mut key: Option<String> = None;
        for part in s.split('+').map(str::trim).filter(|p| !p.is_empty()) {
            match part.to_ascii_lowercase().as_str() {
                "ctrl" | "control" => mods.ctrl = true,
                "alt" => mods.alt = true,
                "shift" => mods.shift = true,
                "win" | "super" | "meta" | "cmd" => mods.win = true,
                other => {
                    if key.is_some() {
                        return Err(HotkeyError::MultipleKeys(s.into()));
                    }
                    key = Some(normalize_key(other));
                }
            }
        }
        let key = key.ok_or_else(|| HotkeyError::NoKey(s.into()))?;
        let is_fkey = key.starts_with('F') && key.len() > 1 && key[1..].parse::<u8>().is_ok();
        if mods == Modifiers::default() && !is_fkey {
            return Err(HotkeyError::NoModifier(s.into()));
        }
        Ok(Self { mods, key })
    }
}

fn normalize_key(k: &str) -> String {
    match k {
        "space" | "espacio" => "SPACE".into(),
        "enter" | "return" | "intro" => "ENTER".into(),
        "esc" | "escape" => "ESC".into(),
        "tab" => "TAB".into(),
        "backspace" => "BACKSPACE".into(),
        "`" | "backquote" | "grave" => "BACKQUOTE".into(),
        other => other.to_ascii_uppercase(),
    }
}

impl fmt::Display for Hotkey {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let m = self.mods;
        for (on, name) in [
            (m.ctrl, "Ctrl"),
            (m.alt, "Alt"),
            (m.shift, "Shift"),
            (m.win, "Win"),
        ] {
            if on {
                write!(f, "{name}+")?;
            }
        }
        write!(f, "{}", self.key)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_default() {
        let h = Hotkey::parse("Alt+Space").unwrap();
        assert!(h.mods.alt && !h.mods.ctrl);
        assert_eq!(h.key, "SPACE");
    }

    #[test]
    fn case_and_spaces_insensitive() {
        let h = Hotkey::parse(" ctrl + shift + k ").unwrap();
        assert!(h.mods.ctrl && h.mods.shift);
        assert_eq!(h.key, "K");
        assert_eq!(h.to_string(), "Ctrl+Shift+K");
    }

    #[test]
    fn errors() {
        assert_eq!(Hotkey::parse(""), Err(HotkeyError::Empty));
        assert!(matches!(Hotkey::parse("Alt+"), Err(HotkeyError::NoKey(_))));
        assert!(matches!(
            Hotkey::parse("Alt+A+B"),
            Err(HotkeyError::MultipleKeys(_))
        ));
        assert!(matches!(
            Hotkey::parse("A"),
            Err(HotkeyError::NoModifier(_))
        ));
    }

    #[test]
    fn function_key_alone_is_allowed() {
        assert_eq!(Hotkey::parse("F13").unwrap().key, "F13");
    }
}
