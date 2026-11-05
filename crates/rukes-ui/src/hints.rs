//! Leyenda de comandos con la búsqueda vacía: ejemplos clicables para no tener que recordar
//! los prefijos y un recordatorio de las teclas. Cada ejemplo es una fila con el comando y su
//! descripción; cuántos y cuáles se eligen en `[window] hints_count` / `hints_items`.

use iced::widget::text::Wrapping;
use iced::widget::{button, column, container, row, text};
use iced::{Background, Border, Element, Length};
use rukes_core::i18n::tr;

use crate::app::Message;
use crate::theme::{self, Look};

/// Un ejemplo de la leyenda.
#[derive(Debug, PartialEq, Eq)]
pub struct Hint {
    /// Id para `hints_items`.
    pub id: &'static str,
    /// Lo que se muestra.
    shown: &'static str,
    /// Lo que se escribe al pulsarlo.
    insert: &'static str,
    /// Clave de la descripción.
    key: &'static str,
}

const fn h(id: &'static str, shown: &'static str, insert: &'static str, key: &'static str) -> Hint {
    Hint {
        id,
        shown,
        insert,
        key,
    }
}

/// Todos los ejemplos, en el orden por defecto.
const HINTS: &[Hint] = &[
    h("calc", "= 5 km a mi", "= ", "hints.calc"),
    h("files", "f: informe", "f: ", "hints.files"),
    h("web", "g rust", "g ", "hints.web"),
    h("clip", "clip", "clip ", "hints.clip"),
    h("todo", "todo", "todo ", "hints.todo"),
    h("remind", "remind 15m", "remind ", "hints.remind"),
    h("new", "new", "new ", "hints.new"),
    h("help", "help vol", "help ", "hints.help"),
    h("shell", "> dir", "> ", "hints.shell"),
    h("snip", "snip", "snip ", "hints.snip"),
    h("note", "note idea", "note ", "hints.note"),
    h("controls", "vol 50", "vol ", "hints.controls"),
    h("windows", "win", "win ", "hints.windows"),
    h("wall", "wall", "wall ", "hints.wall"),
    h("workflows", "workflows", "workflows", "hints.workflows"),
    h("http", "http 3000/api", "http ", "hints.http"),
];

/// Ejemplos para elegir en los ajustes: `(id, "ejemplo — descripción")`.
pub fn options() -> Vec<(String, String)> {
    HINTS
        .iter()
        .map(|h| (h.id.to_owned(), format!("{} — {}", h.shown, tr(h.key))))
        .collect()
}

/// Los ejemplos que se muestran: los de `items` (ids, en su orden; los desconocidos se
/// ignoran) o, si no queda ninguno, los de siempre; como mucho `count` (al menos uno).
pub fn selected(items: &[String], count: usize) -> Vec<&'static Hint> {
    let mut out: Vec<&'static Hint> = Vec::new();
    for id in items {
        if let Some(h) = HINTS.iter().find(|h| h.id.eq_ignore_ascii_case(id.trim()))
            && !out.contains(&h)
        {
            out.push(h);
        }
    }
    if out.is_empty() {
        out = HINTS.iter().collect();
    }
    out.truncate(count.clamp(1, HINTS.len()));
    out
}

/// Alto de cada ejemplo.
const ROW_H: f32 = 32.0;
/// Alto de la etiqueta «SUGERENCIAS».
const LABEL_H: f32 = 26.0;
/// Alto del pie con las teclas.
const FOOTER_H: f32 = 28.0;
/// Margen vertical.
const PAD: f32 = 8.0;
/// Margen horizontal exterior.
const SIDE: f32 = 12.0;
/// Margen horizontal dentro de cada fila.
const ROW_PAD: f32 = 10.0;
/// Ancho de la columna de comandos.
const SHOWN_W: f32 = 140.0;
/// Separación entre comando y descripción.
const GAP: f32 = 12.0;

/// Alto total de la leyenda con `rows` ejemplos.
pub fn height(rows: usize) -> f32 {
    PAD * 2.0 + LABEL_H + FOOTER_H + rows as f32 * ROW_H
}

fn hint<'a>(l: &Look, h: &'static Hint, desc_chars: usize) -> Element<'a, Message> {
    let (accent, muted, hover, radius) = (l.accent, l.muted, l.surface_hover, l.radius);
    button(
        row![
            text(h.shown)
                .font(l.mono)
                .size(l.desc())
                .color(accent)
                .wrapping(Wrapping::None)
                .width(Length::Fixed(SHOWN_W)),
            text(theme::ellipsis(tr(h.key), desc_chars))
                .size(l.desc())
                .color(muted)
                .wrapping(Wrapping::None)
                .width(Length::Fill),
        ]
        .spacing(GAP)
        .align_y(iced::Alignment::Center),
    )
    .padding([0.0, ROW_PAD])
    .height(ROW_H)
    .width(Length::Fill)
    .on_press(Message::Hint(h.insert.to_owned()))
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

/// Leyenda: la etiqueta «SUGERENCIAS», los ejemplos (comando y descripción en dos columnas,
/// cada fila clicable) y un recordatorio de las teclas. `width`: ancho de la ventana.
pub fn view<'a>(l: &Look, hints: &[&'static Hint], width: f32) -> Element<'a, Message> {
    let faint = l.faint;
    let label = container(
        text(tr("hints.title").to_uppercase())
            .size(l.small() - 1.0)
            .font(iced::Font {
                weight: iced::font::Weight::Semibold,
                ..l.font
            })
            .color(faint),
    )
    .height(LABEL_H)
    .padding([0.0, ROW_PAD])
    .align_y(iced::alignment::Vertical::Center);

    let desc_w = width - SIDE * 2.0 - ROW_PAD * 2.0 - SHOWN_W - GAP;
    let desc_chars = theme::fit_chars(desc_w, l.desc());
    let mut list = column![].spacing(0);
    for h in hints {
        list = list.push(hint(l, h, desc_chars));
    }

    let footer = container(
        text(tr("hints.keys"))
            .size(l.small())
            .color(faint)
            .wrapping(Wrapping::None),
    )
    .height(FOOTER_H)
    .padding([0.0, ROW_PAD])
    .align_y(iced::alignment::Vertical::Center)
    .clip(true);

    container(column![label, list, footer])
        .padding([PAD, SIDE])
        .width(Length::Fill)
        .height(height(hints.len()))
        .into()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_hint_has_text_in_both_languages() {
        let es = include_str!("../../rukes-core/lang/es.toml");
        let en = include_str!("../../rukes-core/lang/en.toml");
        let keys = HINTS
            .iter()
            .map(|h| h.key)
            .chain(["hints.new_button", "hints.settings"]);
        for key in keys {
            let name = key.trim_start_matches("hints.");
            for lang in [es, en] {
                assert!(
                    lang.lines().any(|l| l.starts_with(&format!("{name} = "))),
                    "{key}"
                );
            }
        }
        let mut ids: Vec<&str> = HINTS.iter().map(|h| h.id).collect();
        ids.sort_unstable();
        ids.dedup();
        assert_eq!(ids.len(), HINTS.len(), "ids repetidos");
    }

    #[test]
    fn selection_follows_the_config() {
        let ids = |v: Vec<&Hint>| v.iter().map(|h| h.id).collect::<Vec<_>>();
        // Por defecto, los primeros.
        assert_eq!(ids(selected(&[], 3)), ["calc", "files", "web"]);
        assert_eq!(selected(&[], 0).len(), 1);
        assert_eq!(selected(&[], 99).len(), HINTS.len());
        // Los elegidos, en su orden, sin repetidos ni desconocidos.
        let items = ["new", "x", " Calc ", "new", "todo"].map(String::from);
        assert_eq!(ids(selected(&items, 8)), ["new", "calc", "todo"]);
        assert_eq!(ids(selected(&items, 2)), ["new", "calc"]);
        // Solo desconocidos: los de siempre.
        assert_eq!(selected(&["nada".into()], 2).len(), 2);
        assert_eq!(options().len(), HINTS.len());
        assert!(height(8) > height(2));
    }
}
