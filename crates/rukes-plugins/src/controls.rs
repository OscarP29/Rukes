//! Controles del sistema (RF-063): `vol 50`, `brillo 70`, `wifi`, `bluetooth`, `avion`,
//! `oscuro` / `claro`.

use rukes_core::i18n::{tr, trf};
use rukes_core::{
    Action as UiAction, ActionId, ActionOutcome, CancelToken, Icon, Plugin, PluginError,
    PluginMetadata, Query, ResultItem,
};
use rukes_platform::syscontrol::{self, RadioType};

/// Qué controla una consulta.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Control {
    /// Volumen.
    Volume,
    /// Brillo.
    Brightness,
    /// Wi-Fi.
    WiFi,
    /// Bluetooth.
    Bluetooth,
    /// Modo avión (todas las radios).
    Airplane,
    /// Modo oscuro de Windows.
    Dark,
    /// Modo claro de Windows.
    Light,
}

const KEYWORDS: &[(&str, Control)] = &[
    ("vol", Control::Volume),
    ("volumen", Control::Volume),
    ("volume", Control::Volume),
    ("brillo", Control::Brightness),
    ("brightness", Control::Brightness),
    ("wifi", Control::WiFi),
    ("bluetooth", Control::Bluetooth),
    ("bt", Control::Bluetooth),
    ("avion", Control::Airplane),
    ("avión", Control::Airplane),
    ("airplane", Control::Airplane),
    ("oscuro", Control::Dark),
    ("dark", Control::Dark),
    ("claro", Control::Light),
    ("light", Control::Light),
];

/// Lo que pide el texto tras la palabra clave.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Ask {
    /// Nada: mostrar estado y opciones.
    Show,
    /// Un valor (0..=100).
    Set(u32),
    /// Subir o bajar.
    Step(i32),
    /// Encender (o quitar el silencio).
    On,
    /// Apagar (o silenciar).
    Off,
}

/// Interpreta `50`, `+10`, `-5`, `on`, `off`, `encender`, `apagar`, `mute`, `silencio`.
pub fn parse_ask(text: &str) -> Option<Ask> {
    let t = crate::util::fold(text.trim().trim_end_matches('%').trim());
    if t.is_empty() {
        return Some(Ask::Show);
    }
    if let Some(n) = t.strip_prefix('+') {
        return n.trim().parse().ok().map(Ask::Step);
    }
    if let Some(n) = t.strip_prefix('-') {
        return n.trim().parse::<i32>().ok().map(|v| Ask::Step(-v));
    }
    if let Ok(n) = t.parse::<u32>() {
        return Some(Ask::Set(n.min(100)));
    }
    match t.as_str() {
        "on" | "encender" | "activar" | "si" | "unmute" | "sonido" => Some(Ask::On),
        "off" | "apagar" | "desactivar" | "no" | "mute" | "silencio" | "silenciar" => {
            Some(Ask::Off)
        }
        "subir" | "up" | "mas" => Some(Ask::Step(10)),
        "bajar" | "down" | "menos" => Some(Ask::Step(-10)),
        _ => None,
    }
}

/// Plugin de controles del sistema.
pub struct ControlsPlugin;

fn item(id: &str, title: String, subtitle: String, payload: String) -> ResultItem {
    let mut it = ResultItem::new(id, title);
    it.subtitle = subtitle;
    it.icon = Icon::Named("system".into());
    it.payload = payload;
    it.actions = vec![UiAction::new(ActionId::OPEN, tr("controls.apply"))];
    it
}

fn on_off(on: bool) -> &'static str {
    if on {
        tr("controls.on")
    } else {
        tr("controls.off")
    }
}

impl ControlsPlugin {
    fn level(&self, c: Control, ask: Ask) -> Vec<ResultItem> {
        let (name, current, muted) = match c {
            Control::Volume => match syscontrol::volume() {
                Ok((v, m)) => (tr("controls.volume"), v, m),
                Err(e) => {
                    return vec![item(
                        "err",
                        tr("controls.no_audio").into(),
                        e.to_string(),
                        String::new(),
                    )];
                }
            },
            _ => match syscontrol::brightness() {
                Some(b) => (tr("controls.brightness"), b, false),
                None => {
                    return vec![item(
                        "err",
                        tr("controls.no_brightness").into(),
                        tr("controls.no_brightness_hint").into(),
                        String::new(),
                    )];
                }
            },
        };
        let key = if c == Control::Volume { "vol" } else { "bri" };
        let state = if muted {
            format!("{current} % · {}", tr("controls.muted"))
        } else {
            format!("{current} %")
        };
        let set = |v: i64| {
            let v = v.clamp(0, 100) as u32;
            item(
                &format!("{key}:{v}"),
                trf("controls.set_to", &[("what", &name), ("v", &v)]),
                trf("controls.now", &[("state", &state)]),
                format!("{key}:{v}"),
            )
        };
        let cur = i64::from(current);
        let mut out = match ask {
            Ask::Set(v) => vec![set(i64::from(v))],
            Ask::Step(d) => vec![set(cur + i64::from(d))],
            Ask::Show | Ask::On | Ask::Off => vec![set(cur + 10), set(cur - 10)],
        };
        if c == Control::Volume {
            let mute = match ask {
                Ask::On => Some(false),
                Ask::Off => Some(true),
                Ask::Show => Some(!muted),
                _ => None,
            };
            if let Some(m) = mute {
                let title = if m {
                    tr("controls.mute")
                } else {
                    tr("controls.unmute")
                };
                let it = item(
                    "mute",
                    title.into(),
                    trf("controls.now", &[("state", &state)]),
                    format!("mute:{m}"),
                );
                if ask == Ask::Show {
                    out.push(it);
                } else {
                    out.insert(0, it);
                }
            }
        }
        out
    }

    fn radio(&self, c: Control, ask: Ask) -> Vec<ResultItem> {
        let list = match syscontrol::radios() {
            Ok(l) => l,
            Err(e) => {
                return vec![item(
                    "err",
                    tr("controls.no_radios").into(),
                    e.to_string(),
                    String::new(),
                )];
            }
        };
        let (kind, name, key) = match c {
            Control::WiFi => (Some(RadioType::WiFi), "Wi-Fi", "wifi"),
            Control::Bluetooth => (Some(RadioType::Bluetooth), "Bluetooth", "bt"),
            _ => (None, tr("controls.airplane"), "air"),
        };
        let mine: Vec<_> = list
            .iter()
            .filter(|r| kind.is_none_or(|k| r.kind == k) && r.kind != RadioType::Other)
            .collect();
        if mine.is_empty() {
            return vec![item(
                "err",
                trf("controls.no_radio", &[("what", &name)]),
                String::new(),
                String::new(),
            )];
        }
        let any_on = mine.iter().any(|r| r.on);
        // Modo avión: activo = todas apagadas.
        let (active, turn_on) = if kind.is_none() {
            let active = !any_on;
            let want = match ask {
                Ask::On => true,
                Ask::Off => false,
                _ => !active,
            };
            (active, !want)
        } else {
            let want = match ask {
                Ask::On => true,
                Ask::Off => false,
                _ => !any_on,
            };
            (any_on, want)
        };
        let title = if kind.is_none() {
            if turn_on {
                tr("controls.airplane_off").to_owned()
            } else {
                tr("controls.airplane_on").to_owned()
            }
        } else if turn_on {
            trf("controls.turn_on", &[("what", &name)])
        } else {
            trf("controls.turn_off", &[("what", &name)])
        };
        let state = trf(
            "controls.state",
            &[("what", &name), ("state", &on_off(active))],
        );
        vec![item(key, title, state, format!("{key}:{turn_on}"))]
    }

    fn theme(&self, dark: bool) -> Vec<ResultItem> {
        let now = rukes_platform::appearance::system_dark_mode();
        let title = if dark {
            tr("controls.dark")
        } else {
            tr("controls.light")
        };
        let state = if now {
            tr("controls.now_dark")
        } else {
            tr("controls.now_light")
        };
        vec![item(
            "theme",
            title.into(),
            state.into(),
            format!("dark:{dark}"),
        )]
    }

    /// Resultados para la palabra clave `c` y el texto que la sigue.
    pub fn search(&self, c: Control, text: &str) -> Vec<ResultItem> {
        let Some(ask) = parse_ask(text) else {
            return Vec::new();
        };
        let mut out = match c {
            Control::Volume | Control::Brightness => self.level(c, ask),
            Control::WiFi | Control::Bluetooth | Control::Airplane => self.radio(c, ask),
            Control::Dark => self.theme(true),
            Control::Light => self.theme(false),
        };
        for (i, it) in out.iter_mut().enumerate() {
            it.fuzzy = 1.0 - i as f32 * 0.01;
        }
        out
    }
}

/// Palabra clave al principio de la consulta.
pub fn control_of(raw: &str) -> Option<Control> {
    let first = raw.split_whitespace().next()?.to_lowercase();
    KEYWORDS.iter().find(|(k, _)| *k == first).map(|(_, c)| *c)
}

impl Plugin for ControlsPlugin {
    fn metadata(&self) -> PluginMetadata {
        PluginMetadata {
            id: "controls".into(),
            name: tr("controls.name").into(),
            prefix: None,
            keywords: KEYWORDS.iter().map(|(k, _)| (*k).to_owned()).collect(),
            priority: 0.3,
            global: false,
            list_on_keyword: false,
        }
    }

    fn query(&self, q: &Query, _c: &CancelToken, sink: &mut dyn FnMut(Vec<ResultItem>)) {
        if let Some(c) = control_of(&q.raw) {
            sink(self.search(c, &q.text));
        }
    }

    fn execute(&self, item: &ResultItem, action: &str) -> Result<ActionOutcome, PluginError> {
        if action != ActionId::OPEN {
            return Err(PluginError::UnsupportedAction(action.into()));
        }
        let Some((key, value)) = item.payload.split_once(':') else {
            return Ok(ActionOutcome::KeepOpen);
        };
        let fail = |e: rukes_platform::PlatformError| PluginError::Failed(e.to_string());
        let flag = value == "true";
        let msg = match key {
            "vol" => {
                let v = value.parse().unwrap_or(0);
                syscontrol::set_volume(v).map_err(fail)?;
                trf("controls.done_volume", &[("v", &v)])
            }
            "bri" => {
                let v = value.parse().unwrap_or(0);
                syscontrol::set_brightness(v).map_err(fail)?;
                trf("controls.done_brightness", &[("v", &v)])
            }
            "mute" => {
                syscontrol::set_mute(flag).map_err(fail)?;
                if flag {
                    tr("controls.muted").to_owned()
                } else {
                    tr("controls.unmuted").to_owned()
                }
            }
            "wifi" | "bt" | "air" => {
                let kind = match key {
                    "wifi" => Some(RadioType::WiFi),
                    "bt" => Some(RadioType::Bluetooth),
                    _ => None,
                };
                syscontrol::set_radios(kind, flag).map_err(fail)?;
                item.title.clone()
            }
            "dark" => {
                syscontrol::set_dark_mode(flag).map_err(fail)?;
                item.title.clone()
            }
            _ => return Ok(ActionOutcome::KeepOpen),
        };
        Ok(ActionOutcome::Notify(format!("✓ {msg}")))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_requests() {
        assert_eq!(parse_ask(""), Some(Ask::Show));
        assert_eq!(parse_ask("50"), Some(Ask::Set(50)));
        assert_eq!(parse_ask("150 %"), Some(Ask::Set(100)));
        assert_eq!(parse_ask("+5"), Some(Ask::Step(5)));
        assert_eq!(parse_ask("-10"), Some(Ask::Step(-10)));
        assert_eq!(parse_ask("Apagar"), Some(Ask::Off));
        assert_eq!(parse_ask("silencio"), Some(Ask::Off));
        assert_eq!(parse_ask("encender"), Some(Ask::On));
        assert_eq!(parse_ask("hola"), None);
        assert_eq!(control_of("vol 50"), Some(Control::Volume));
        assert_eq!(control_of("Avión"), Some(Control::Airplane));
        assert_eq!(control_of("volcan"), None);
    }

    /// Solo lee el estado; no ejecuta ninguna acción.
    #[test]
    fn shows_state_without_changing_it() {
        let _ = rukes_core::i18n::set_language("es", "");
        let p = ControlsPlugin;
        let r = p.search(Control::Volume, "50");
        assert!(!r.is_empty());
        if r[0].id != "err" {
            assert_eq!(r[0].payload, "vol:50");
        }
        let r = p.search(Control::Dark, "");
        assert_eq!(r[0].payload, "dark:true");
    }
}
