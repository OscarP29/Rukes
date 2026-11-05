//! Leyenda de comandos con la búsqueda vacía: ejemplos clicables para no tener que recordar
//! los prefijos y un recordatorio de las teclas.

use iced::widget::{button, column, container, row, space, text};
use iced::{Background, Border, Color, Element, Length};
use rukes_core::i18n::tr;

use crate::app::Message;
use crate::theme::Look;

/// `(lo que se muestra, lo que se escribe al pulsarlo, clave de la descripción)`.
const HINTS: &[(&str, &str, &str)] = &[
    ("= 5 km a mi", "= ", "hints.calc"),
    ("f: informe", "f: ", "hints.files"),
    ("g rust", "g ", "hints.web"),
    ("> dir", "> ", "hints.shell"),
    ("clip", "clip ", "hints.clip"),
    ("snip", "snip ", "hints.snip"),
    ("todo", "todo ", "hints.todo"),
    ("remind 15m", "remind ", "hints.remind"),
    ("vol 50", "vol ", "hints.controls"),
    ("win", "win ", "hints.windows"),
    ("wall", "wall ", "hints.wall"),
    ("workflows", "workflows", "hints.workflows"),
];

/// Columnas de la cuadrícula.
const COLUMNS: usize = 2;
/// Alto de cada ejemplo.
const ROW_H: f32 = 30.0;
/// Alto de la cabecera y del pie.
const BAR_H: f32 = 28.0;
/// Margen vertical.
const PAD: f32 = 6.0;

/// Alto total de la leyenda.
pub fn height() -> f32 {
    let rows = HINTS.len().div_ceil(COLUMNS) as f32;
    PAD * 2.0 + BAR_H * 2.0 + rows * ROW_H
}

fn hint<'a>(l: &Look, shown: &'a str, insert: &'a str, key: &str) -> Element<'a, Message> {
    let (accent, muted, hover, radius) = (l.accent, l.muted, l.selection, l.radius);
    button(
        row![
            text(shown)
                .font(l.mono)
                .size(l.small())
                .color(accent)
                .width(Length::Fixed(120.0)),
            text(tr(key)).size(l.small()).color(muted),
        ]
        .spacing(8)
        .align_y(iced::Alignment::Center),
    )
    .padding([0.0, 10.0])
    .height(ROW_H)
    .width(Length::Fill)
    .on_press(Message::Hint(insert.to_owned()))
    .style(move |_, status| button::Style {
        background: matches!(status, button::Status::Hovered | button::Status::Pressed)
            .then_some(Background::Color(hover)),
        text_color: muted,
        border: Border {
            radius: radius.into(),
            ..Border::default()
        },
        ..button::Style::default()
    })
    .into()
}

/// Leyenda: título con el botón de ajustes, ejemplos en dos columnas y teclas.
pub fn view<'a>(l: &Look) -> Element<'a, Message> {
    let (fg, muted, hover, radius) = (l.fg, l.muted, l.selection, l.radius);
    let settings = button(
        row![
            text(crate::panel::ICON_SETTINGS)
                .font(crate::panel::icon_font())
                .size(l.small())
                .color(muted),
            text(tr("hints.settings")).size(l.small()).color(muted),
        ]
        .spacing(6)
        .align_y(iced::Alignment::Center),
    )
    .padding([3.0, 8.0])
    .on_press(Message::OpenSettings)
    .style(move |_, status| button::Style {
        background: matches!(status, button::Status::Hovered | button::Status::Pressed)
            .then_some(Background::Color(hover)),
        text_color: fg,
        border: Border {
            radius: radius.into(),
            ..Border::default()
        },
        ..button::Style::default()
    });
    let header = row![
        text(tr("hints.title")).size(l.small()).color(fg),
        space().width(Length::Fill),
        settings,
    ]
    .align_y(iced::Alignment::Center)
    .height(BAR_H)
    .padding([0.0, 10.0]);

    let mut grid = column![].spacing(0);
    for pair in HINTS.chunks(COLUMNS) {
        let mut r = row![].spacing(4);
        for (shown, insert, key) in pair {
            r = r.push(hint(l, shown, insert, key));
        }
        for _ in pair.len()..COLUMNS {
            r = r.push(space().width(Length::Fill));
        }
        grid = grid.push(r);
    }

    let footer = container(
        text(tr("hints.keys"))
            .size(l.small() - 1.0)
            .color(Color { a: 0.85, ..muted }),
    )
    .height(BAR_H)
    .padding([0.0, 10.0])
    .align_y(iced::alignment::Vertical::Center);

    container(column![header, grid, footer])
        .padding([PAD, 8.0])
        .width(Length::Fill)
        .height(height())
        .into()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_hint_has_text_in_both_languages() {
        let es = include_str!("../../rukes-core/lang/es.toml");
        let en = include_str!("../../rukes-core/lang/en.toml");
        for (_, _, key) in HINTS {
            let name = key.trim_start_matches("hints.");
            for lang in [es, en] {
                assert!(
                    lang.lines().any(|l| l.starts_with(&format!("{name} = "))),
                    "{key}"
                );
            }
        }
        assert!(height() > 200.0);
    }
}
