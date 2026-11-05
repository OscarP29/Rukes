//! Panel visual de ajustes: pestañas con los ajustes más usados de `config.toml`, editados con
//! interruptores, listas y deslizadores. Al guardar, la aplicación escribe solo lo cambiado
//! (conserva los comentarios) y la configuración se recarga sola.

use std::fmt;

use iced::widget::{
    button, checkbox, column, container, pick_list, row, scrollable, slider, space, text,
    text_input, toggler,
};
use iced::{Background, Border, Color, Element, Font, Length};
use rukes_core::i18n::tr;

use crate::app::Message;
use crate::theme::Look;
use crate::{SettingKind, SettingValue, SettingsForm};

/// Fuente de iconos de Windows (Segoe MDL2 Assets existe en Windows 10 y 11).
pub fn icon_font() -> Font {
    Font::with_name("Segoe MDL2 Assets")
}

/// Engranaje.
pub const ICON_SETTINGS: &str = "\u{E713}";
/// Cerrar.
pub const ICON_CLOSE: &str = "\u{E711}";
/// Añadir.
pub const ICON_ADD: &str = "\u{E710}";

/// Alto del cuerpo del panel (las pestañas y los ajustes).
const BODY_H: f32 = 380.0;
/// Alto del pie con los botones.
const FOOTER_H: f32 = 52.0;
/// Ancho mínimo del panel.
pub const MIN_WIDTH: f32 = 640.0;
/// Ancho de la columna de pestañas.
const TABS_W: f32 = 150.0;
/// Ancho de los controles.
const CONTROL_W: f32 = 230.0;

/// Alto total del panel con una cabecera de `header` píxeles.
pub fn height(header: f32) -> f32 {
    header + 1.0 + BODY_H + 1.0 + FOOTER_H
}

/// Estado del panel abierto.
#[derive(Debug, Clone)]
pub struct Panel {
    /// Formulario con los valores al abrirlo.
    pub form: SettingsForm,
    /// Valores editados, en el mismo orden que `form`.
    pub values: Vec<Vec<SettingValue>>,
    /// Pestaña visible.
    pub tab: usize,
    /// Guardando (botones desactivados).
    pub saving: bool,
    /// Error al guardar.
    pub error: Option<String>,
}

impl Panel {
    /// Panel recién abierto.
    pub fn new(form: SettingsForm) -> Self {
        let values = form
            .sections
            .iter()
            .map(|s| s.fields.iter().map(|f| f.value.clone()).collect())
            .collect();
        Self {
            form,
            values,
            tab: 0,
            saving: false,
            error: None,
        }
    }

    /// Cambia un valor (los índices fuera de rango se ignoran).
    pub fn set(&mut self, section: usize, field: usize, value: SettingValue) {
        if let Some(v) = self.values.get_mut(section).and_then(|s| s.get_mut(field)) {
            *v = value;
            self.error = None;
        }
    }

    /// Ajustes cambiados: `(ruta en config.toml, valor nuevo)`.
    pub fn changes(&self) -> Vec<(String, SettingValue)> {
        let mut out = Vec::new();
        for (s, section) in self.form.sections.iter().enumerate() {
            for (f, field) in section.fields.iter().enumerate() {
                if let Some(v) = self.values.get(s).and_then(|s| s.get(f))
                    && *v != field.value
                {
                    out.push((field.key.clone(), v.clone()));
                }
            }
        }
        out
    }
}

/// Opción de una lista desplegable.
#[derive(Debug, Clone, PartialEq)]
struct Opt {
    value: String,
    label: String,
}

impl fmt::Display for Opt {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.label)
    }
}

/// Botón plano que se resalta al pasar el ratón.
fn flat<'a>(l: &Look, content: impl Into<Element<'a, Message>>) -> button::Button<'a, Message> {
    let (fg, hover, radius) = (l.fg, l.selection, l.radius);
    button(content).style(move |_, status| button::Style {
        background: matches!(status, button::Status::Hovered | button::Status::Pressed)
            .then_some(Background::Color(hover)),
        text_color: fg,
        border: Border {
            radius: radius.into(),
            ..Border::default()
        },
        ..button::Style::default()
    })
}

fn control<'a>(
    l: &Look,
    s: usize,
    f: usize,
    kind: &'a SettingKind,
    value: &'a SettingValue,
) -> Element<'a, Message> {
    match (kind, value) {
        (SettingKind::Toggle, SettingValue::Bool(b)) => toggler(*b)
            .on_toggle(move |v| Message::PanelSet(s, f, SettingValue::Bool(v)))
            .size(20)
            .into(),
        (SettingKind::Choice(options), SettingValue::Text(current)) => {
            let opts: Vec<Opt> = options
                .iter()
                .map(|(value, label)| Opt {
                    value: value.clone(),
                    label: label.clone(),
                })
                .collect();
            let selected = opts
                .iter()
                .find(|o| o.value == *current)
                .cloned()
                .or_else(|| {
                    // Un valor que no está en la lista (p. ej. un tema propio) se muestra igual.
                    (!current.is_empty()).then(|| Opt {
                        value: current.clone(),
                        label: current.clone(),
                    })
                });
            pick_list(opts, selected, move |o: Opt| {
                Message::PanelSet(s, f, SettingValue::Text(o.value))
            })
            .text_size(l.small())
            .width(Length::Fill)
            .into()
        }
        (SettingKind::Number { min, max, step }, SettingValue::Int(v)) => {
            let to_i32 = |n: i64| i32::try_from(n).unwrap_or(0);
            let (lo, hi) = (to_i32(*min), to_i32(*max));
            row![
                slider(lo..=hi, to_i32(*v).clamp(lo, hi), move |n| {
                    Message::PanelSet(s, f, SettingValue::Int(i64::from(n)))
                })
                .step(to_i32(*step).max(1))
                .width(Length::Fill),
                text(v.to_string())
                    .size(l.small())
                    .font(l.mono)
                    .color(l.fg)
                    .width(Length::Fixed(44.0))
                    .align_x(iced::alignment::Horizontal::Right),
            ]
            .spacing(10)
            .align_y(iced::Alignment::Center)
            .into()
        }
        (SettingKind::Text, SettingValue::Text(t)) => text_input("", t)
            .on_input(move |v| Message::PanelSet(s, f, SettingValue::Text(v)))
            .size(l.small())
            .padding([5, 8])
            .into(),
        (SettingKind::Multi(options), SettingValue::List(current)) => {
            multi(l, s, f, options, current)
        }
        _ => space().into(),
    }
}

/// `current` con `value` marcado o desmarcado, en el orden de `options`.
pub fn toggle_in(
    options: &[(String, String)],
    current: &[String],
    value: &str,
    on: bool,
) -> Vec<String> {
    options
        .iter()
        .map(|(v, _)| v)
        .filter(|v| {
            if v.as_str() == value {
                on
            } else {
                current.contains(v)
            }
        })
        .cloned()
        .collect()
}

/// Casillas en dos columnas.
fn multi<'a>(
    l: &Look,
    s: usize,
    f: usize,
    options: &'a [(String, String)],
    current: &'a [String],
) -> Element<'a, Message> {
    let mut grid = column![].spacing(6);
    for pair in options.chunks(2) {
        let mut r = row![].spacing(12);
        for (value, label) in pair {
            let checked = current.contains(value);
            r = r.push(
                checkbox(checked)
                    .label(label.as_str())
                    .size(16)
                    .text_size(l.small())
                    .on_toggle(move |on| {
                        Message::PanelSet(
                            s,
                            f,
                            SettingValue::List(toggle_in(options, current, value, on)),
                        )
                    })
                    .width(Length::FillPortion(1)),
            );
        }
        if pair.len() < 2 {
            r = r.push(space().width(Length::FillPortion(1)));
        }
        grid = grid.push(r);
    }
    grid.into()
}

/// Vista del panel.
pub fn view<'a>(p: &'a Panel, l: &Look, header_h: f32) -> Element<'a, Message> {
    let (fg, muted, divider, accent) = (l.fg, l.muted, l.divider, l.accent);

    let header = row![
        text(ICON_SETTINGS)
            .font(icon_font())
            .size(l.size + 2.0)
            .color(accent),
        text(tr("settings.title")).size(l.input_size()).color(fg),
        space().width(Length::Fill),
        flat(
            l,
            text(tr("settings.open_file")).size(l.small()).color(muted)
        )
        .padding([4.0, 10.0])
        .on_press(Message::PanelOpenFile),
        flat(
            l,
            text(ICON_CLOSE)
                .font(icon_font())
                .size(l.small())
                .color(muted)
        )
        .padding([6.0, 8.0])
        .on_press(Message::PanelClose),
    ]
    .spacing(12)
    .align_y(iced::Alignment::Center)
    .padding([0.0, 20.0])
    .height(header_h);

    let mut tabs = column![].spacing(2).width(TABS_W);
    for (i, section) in p.form.sections.iter().enumerate() {
        let active = i == p.tab;
        let (sel, radius) = (l.selection, l.radius);
        tabs = tabs.push(
            button(
                text(section.title.as_str())
                    .size(l.small())
                    .color(if active { fg } else { muted }),
            )
            .width(Length::Fill)
            .padding([7.0, 12.0])
            .on_press(Message::PanelTab(i))
            .style(move |_, status| button::Style {
                background: (active
                    || matches!(status, button::Status::Hovered | button::Status::Pressed))
                .then_some(Background::Color(if active {
                    sel
                } else {
                    Color {
                        a: sel.a * 0.5,
                        ..sel
                    }
                })),
                text_color: fg,
                border: Border {
                    radius: radius.into(),
                    ..Border::default()
                },
                ..button::Style::default()
            }),
        );
    }

    let mut fields = column![].spacing(14).padding([4.0, 16.0]);
    if let Some(section) = p.form.sections.get(p.tab) {
        for (f, field) in section.fields.iter().enumerate() {
            let Some(value) = p.values.get(p.tab).and_then(|v| v.get(f)) else {
                continue;
            };
            let mut label = column![text(field.label.as_str()).size(l.small() + 1.0).color(fg)]
                .spacing(2)
                .width(Length::Fill);
            if !field.help.is_empty() {
                label = label.push(text(field.help.as_str()).size(l.small() - 1.0).color(muted));
            }
            // Las casillas ocupan todo el ancho, debajo del nombre.
            if matches!(field.kind, SettingKind::Multi(_)) {
                fields = fields
                    .push(column![label, control(l, p.tab, f, &field.kind, value)].spacing(8));
                continue;
            }
            fields = fields.push(
                row![
                    label,
                    container(control(l, p.tab, f, &field.kind, value))
                        .width(CONTROL_W)
                        .align_x(iced::alignment::Horizontal::Right),
                ]
                .spacing(16)
                .align_y(iced::Alignment::Center),
            );
        }
    }

    let body = row![
        container(tabs).padding([12.0, 8.0]).height(Length::Fill),
        line(divider, true),
        scrollable(container(fields).padding([12.0, 0.0])).height(Length::Fill),
    ]
    .height(BODY_H);

    let pending = p.changes().len();
    let status = match &p.error {
        Some(e) => text(e.as_str())
            .size(l.small())
            .color(Color::from_rgb8(0xf7, 0x76, 0x8e)),
        None if pending > 0 => text(rukes_core::i18n::trf(
            "settings.pending",
            &[("n", &pending)],
        ))
        .size(l.small())
        .color(muted),
        None => text(tr("settings.hint")).size(l.small()).color(muted),
    };
    let save = button(text(tr("settings.save")).size(l.small()))
        .padding([6.0, 16.0])
        .style(button::primary)
        .on_press_maybe((pending > 0 && !p.saving).then_some(Message::PanelSave));
    let cancel = flat(l, text(tr("settings.cancel")).size(l.small()).color(fg))
        .padding([6.0, 14.0])
        .on_press(Message::PanelClose);
    let footer = row![status, space().width(Length::Fill), cancel, save]
        .spacing(8)
        .align_y(iced::Alignment::Center)
        .padding([0.0, 16.0])
        .height(FOOTER_H);

    column![
        header,
        line(divider, false),
        body,
        line(divider, false),
        footer
    ]
    .into()
}

fn line<'a>(c: Color, vertical: bool) -> Element<'a, Message> {
    let l = if vertical {
        space().width(1).height(Length::Fill)
    } else {
        space().height(1).width(Length::Fill)
    };
    container(l)
        .style(move |_| container::Style {
            background: Some(Background::Color(c)),
            ..container::Style::default()
        })
        .into()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{SettingField, SettingsSection};

    fn form() -> SettingsForm {
        SettingsForm {
            sections: vec![SettingsSection {
                title: "Ventana".into(),
                fields: vec![
                    SettingField {
                        key: "window.hints".into(),
                        label: "Leyenda".into(),
                        help: String::new(),
                        kind: SettingKind::Toggle,
                        value: SettingValue::Bool(true),
                    },
                    SettingField {
                        key: "window.width".into(),
                        label: "Ancho".into(),
                        help: String::new(),
                        kind: SettingKind::Number {
                            min: 400,
                            max: 1200,
                            step: 20,
                        },
                        value: SettingValue::Int(680),
                    },
                ],
            }],
        }
    }

    #[test]
    fn only_changed_values_are_saved() {
        let mut p = Panel::new(form());
        assert!(p.changes().is_empty());
        p.set(0, 1, SettingValue::Int(720));
        p.set(0, 0, SettingValue::Bool(true)); // igual que antes
        p.set(3, 0, SettingValue::Bool(false)); // fuera de rango
        assert_eq!(
            p.changes(),
            vec![("window.width".to_owned(), SettingValue::Int(720))]
        );
        p.set(0, 1, SettingValue::Int(680));
        assert!(p.changes().is_empty());
    }

    #[test]
    fn multi_keeps_the_order_of_the_options() {
        let o: Vec<(String, String)> = ["a", "b", "c"]
            .iter()
            .map(|v| ((*v).to_owned(), v.to_uppercase()))
            .collect();
        let cur = vec!["c".to_owned()];
        assert_eq!(toggle_in(&o, &cur, "a", true), ["a", "c"]);
        assert!(toggle_in(&o, &cur, "c", false).is_empty());
        assert_eq!(
            toggle_in(&o, &["a".into(), "b".into()], "b", true),
            ["a", "b"]
        );
    }
}
