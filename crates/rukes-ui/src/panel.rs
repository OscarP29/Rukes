//! Panel visual de ajustes: pestañas con los ajustes más usados de `config.toml`, editados con
//! interruptores, listas y deslizadores. Al guardar, la aplicación escribe solo lo cambiado
//! (conserva los comentarios) y la configuración se recarga sola.

use std::fmt;

use iced::widget::text::Wrapping;
use iced::widget::{
    button, checkbox, column, container, pick_list, row, scrollable, slider, space, text,
    text_input, toggler,
};
use iced::{Background, Border, Color, Element, Font, Length, Padding};
use rukes_core::i18n::{tr, trf};

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
const BODY_H: f32 = 420.0;
/// Alto del pie con los botones.
const FOOTER_H: f32 = 52.0;
/// Ancho mínimo del panel.
pub const MIN_WIDTH: f32 = 640.0;
/// Ancho de la columna de pestañas.
const TABS_W: f32 = 160.0;
/// Ancho de la columna de comandos en las casillas de la leyenda.
const CMD_W: f32 = 130.0;
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
    let (fg, hover, radius) = (l.fg, l.surface_hover, l.radius);
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

/// Casillas en una columna: casilla, comando (monoespaciado, acento) y descripción. Toda la
/// fila es clicable.
fn multi<'a>(
    l: &Look,
    s: usize,
    f: usize,
    options: &'a [(String, String)],
    current: &'a [String],
) -> Element<'a, Message> {
    let (accent, muted, hover, radius) = (l.accent, l.muted, l.surface_hover, l.radius);
    let mut list = column![].spacing(0);
    for (value, label) in options {
        let (cmd, desc) = label.split_once(" — ").unwrap_or((label.as_str(), ""));
        let checked = current.contains(value);
        let set = move |on: bool| {
            Message::PanelSet(
                s,
                f,
                SettingValue::List(toggle_in(options, current, value, on)),
            )
        };
        list = list.push(
            button(
                row![
                    checkbox(checked).size(16).on_toggle(set),
                    text(cmd)
                        .font(l.mono)
                        .size(l.desc())
                        .color(accent)
                        .wrapping(Wrapping::None)
                        .width(Length::Fixed(CMD_W)),
                    text(desc)
                        .size(l.desc())
                        .color(muted)
                        .wrapping(Wrapping::None)
                        .width(Length::Fill),
                ]
                .spacing(12)
                .align_y(iced::Alignment::Center),
            )
            .width(Length::Fill)
            .padding([6.0, 8.0])
            .on_press(set(!checked))
            .style(move |_, status| button::Style {
                background: matches!(status, button::Status::Hovered | button::Status::Pressed)
                    .then_some(Background::Color(hover)),
                text_color: muted,
                border: Border {
                    radius: radius.into(),
                    ..Border::default()
                },
                ..button::Style::default()
            }),
        );
    }
    list.into()
}

/// Pestaña de la navegación lateral; la activa lleva una barra de acento a la izquierda.
fn nav_item<'a>(l: &Look, i: usize, title: &'a str, active: bool) -> Element<'a, Message> {
    let (fg, muted, accent) = (l.fg, l.muted, l.accent);
    let (surface, hover, radius) = (l.surface, l.surface_hover, l.radius);
    let bar = container(space().width(3).height(18)).style(move |_| container::Style {
        background: active.then_some(Background::Color(accent)),
        border: Border {
            radius: 2.0.into(),
            ..Border::default()
        },
        ..container::Style::default()
    });
    button(
        row![
            bar,
            text(title)
                .size(l.desc())
                .color(if active { fg } else { muted })
                .wrapping(Wrapping::None),
        ]
        .spacing(9)
        .align_y(iced::Alignment::Center),
    )
    .width(Length::Fill)
    .padding([7.0, 6.0])
    .on_press(Message::PanelTab(i))
    .style(move |_, status| button::Style {
        background: if active {
            Some(Background::Color(hover))
        } else if matches!(status, button::Status::Hovered | button::Status::Pressed) {
            Some(Background::Color(surface))
        } else {
            None
        },
        text_color: fg,
        border: Border {
            radius: radius.into(),
            ..Border::default()
        },
        ..button::Style::default()
    })
    .into()
}

/// Tarjeta: título opcional y filas separadas por líneas finas.
fn card<'a>(
    l: &Look,
    title: Option<&'a str>,
    rows: Vec<Element<'a, Message>>,
) -> Element<'a, Message> {
    let (surface, border) = (l.surface, l.divider);
    let mut col = column![].spacing(0);
    if let Some(t) = title {
        col = col.push(
            container(
                text(t.to_uppercase())
                    .size(l.small() - 1.0)
                    .font(Font {
                        weight: iced::font::Weight::Semibold,
                        ..l.font
                    })
                    .color(l.faint),
            )
            .padding(Padding {
                top: 12.0,
                bottom: 2.0,
                ..Padding::ZERO
            }),
        );
    }
    for (k, r) in rows.into_iter().enumerate() {
        if k > 0 {
            col = col.push(line(border, false));
        }
        col = col.push(r);
    }
    container(col)
        .padding([2.0, 16.0])
        .width(Length::Fill)
        .style(move |_| container::Style {
            background: Some(Background::Color(surface)),
            border: Border {
                radius: 10.0.into(),
                width: 1.0,
                color: border,
            },
            ..container::Style::default()
        })
        .into()
}

/// Un ajuste: nombre y descripción a la izquierda y el control a la derecha (las casillas,
/// debajo y a todo el ancho).
fn setting_row<'a>(
    l: &Look,
    s: usize,
    f: usize,
    field: &'a crate::SettingField,
    value: &'a SettingValue,
) -> Element<'a, Message> {
    let (fg, muted, faint) = (l.fg, l.muted, l.faint);
    let mut label = column![text(field.label.as_str()).size(l.size - 1.0).color(fg)]
        .spacing(3)
        .width(Length::Fill);
    if !field.help.is_empty() {
        label = label.push(text(field.help.as_str()).size(l.small()).color(muted));
    }
    if let (SettingKind::Multi(options), SettingValue::List(current)) = (&field.kind, value) {
        let n = options.iter().filter(|(v, _)| current.contains(v)).count();
        let count = trf(
            "settings.multi_count",
            &[("n", &n), ("total", &options.len())],
        );
        return column![
            row![label, text(count).size(l.small()).color(faint)]
                .spacing(12)
                .align_y(iced::Alignment::Center),
            control(l, s, f, &field.kind, value),
        ]
        .spacing(8)
        .padding([12.0, 0.0])
        .into();
    }
    row![
        label,
        container(control(l, s, f, &field.kind, value))
            .width(CONTROL_W)
            .align_x(iced::alignment::Horizontal::Right),
    ]
    .spacing(16)
    .align_y(iced::Alignment::Center)
    .padding([12.0, 0.0])
    .into()
}

/// Ajustes de una pestaña en tarjetas: cada título empieza una.
fn cards<'a>(l: &Look, p: &'a Panel) -> Element<'a, Message> {
    let mut out = column![].spacing(18);
    let Some(section) = p.form.sections.get(p.tab) else {
        return out.into();
    };
    let mut title: Option<&str> = None;
    let mut rows: Vec<Element<'a, Message>> = Vec::new();
    for (f, field) in section.fields.iter().enumerate() {
        if matches!(field.kind, SettingKind::Heading) {
            if !rows.is_empty() {
                out = out.push(card(l, title, std::mem::take(&mut rows)));
            }
            title = Some(field.label.as_str());
            continue;
        }
        if let Some(value) = p.values.get(p.tab).and_then(|v| v.get(f)) {
            rows.push(setting_row(l, p.tab, f, field, value));
        }
    }
    if !rows.is_empty() {
        out = out.push(card(l, title, rows));
    }
    out.into()
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
        tabs = tabs.push(nav_item(l, i, section.title.as_str(), i == p.tab));
    }

    let body = row![
        container(tabs).padding([14.0, 10.0]).height(Length::Fill),
        line(divider, true),
        scrollable(container(cards(l, p)).padding([16.0, 18.0])).height(Length::Fill),
    ]
    .height(BODY_H);

    let pending = p.changes().len();
    let status = match &p.error {
        Some(e) => text(e.as_str()).size(l.small()).color(l.danger),
        None if pending > 0 => text(trf("settings.pending", &[("n", &pending)]))
            .size(l.small())
            .color(muted),
        None => text(tr("settings.hint")).size(l.small()).color(l.faint),
    };
    // Guardar: acento con el texto del color del fondo (contrasta en temas claros y oscuros);
    // desactivado sigue legible sobre el acento suave.
    let (soft, on_accent, radius) = (l.accent_soft, Color { a: 1.0, ..l.bg }, l.radius);
    let save = button(text(tr("settings.save")).size(l.desc()))
        .padding([7.0, 18.0])
        .style(move |_, status| {
            let (bg, text_color) = match status {
                button::Status::Disabled => (soft, Color { a: 0.7, ..fg }),
                button::Status::Hovered | button::Status::Pressed => {
                    (Color { a: 0.88, ..accent }, on_accent)
                }
                button::Status::Active => (accent, on_accent),
            };
            button::Style {
                background: Some(Background::Color(bg)),
                text_color,
                border: Border {
                    radius: radius.into(),
                    ..Border::default()
                },
                ..button::Style::default()
            }
        })
        .on_press_maybe((pending > 0 && !p.saving).then_some(Message::PanelSave));
    let cancel = flat(l, text(tr("settings.cancel")).size(l.desc()).color(fg))
        .padding([7.0, 16.0])
        .on_press(Message::PanelClose);
    let footer = row![status, space().width(Length::Fill), cancel, save]
        .spacing(10)
        .align_y(iced::Alignment::Center)
        .padding([0.0, 18.0])
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
