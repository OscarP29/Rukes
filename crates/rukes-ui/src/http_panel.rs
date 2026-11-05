//! Mini Postman: una barra (método, dirección y Enviar) que con Tab se despliega en pestañas
//! (cabeceras, cuerpo, autenticación y parámetros) y, al enviar, muestra la respuesta: estado,
//! tiempo y tamaño arriba y el cuerpo formateado debajo. Todo se maneja con el teclado.

use iced::widget::text_editor::{self, Binding, Content, KeyPress};
use iced::widget::{
    Id, button, column, container, pick_list, row, space, text, text_editor as editor, text_input,
};
use iced::{Background, Border, Color, Element, Length};
use rukes_core::i18n::{tr, trf};
use rukes_core::{HTTP_METHODS, HttpForm, HttpResponse};

use crate::app::Message;
use crate::theme::Look;

/// Pestañas, en orden.
pub const TABS: [&str; 4] = ["headers", "body", "auth", "params"];

/// Alto de la fila de pestañas.
const TABS_H: f32 = 34.0;
/// Alto del editor de una pestaña.
const EDITOR_H: f32 = 150.0;
/// Alto de la barra de la respuesta.
const INFO_H: f32 = 38.0;
/// Alto mínimo y máximo de la respuesta.
const RESPONSE_MIN: f32 = 90.0;
const RESPONSE_MAX: f32 = 380.0;
/// Alto del pie con las teclas.
const FOOTER_H: f32 = 26.0;
/// Ancho mínimo de la ventana en este modo.
pub const MIN_WIDTH: f32 = 680.0;

pub fn url_id() -> Id {
    Id::new("http-url")
}

pub fn editor_id() -> Id {
    Id::new("http-editor")
}

pub fn response_id() -> Id {
    Id::new("http-response")
}

/// Dónde está el foco (lo que decide qué hacen ↑ ↓ y Enter).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Focus {
    /// La barra de la dirección.
    Url,
    /// El editor de la pestaña.
    Editor,
    /// La respuesta.
    Response,
}

/// Qué se copia.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CopyWhat {
    /// El cuerpo de la respuesta.
    Body,
    /// Las cabeceras de la respuesta.
    Headers,
    /// La petición como `curl`.
    Curl,
}

/// Estado del mini Postman abierto.
pub struct HttpPanel {
    /// Índice en [`HTTP_METHODS`].
    pub method: usize,
    /// Dirección (o una orden `curl` a medio escribir).
    pub url: String,
    /// Pestañas visibles.
    pub expanded: bool,
    /// Pestaña visible.
    pub tab: usize,
    /// Texto de cada pestaña.
    pub editors: [Content; 4],
    /// Petición en curso (su número).
    pub sending: Option<u64>,
    /// Número de la última petición.
    pub seq: u64,
    /// Última respuesta o error.
    pub response: Option<Result<HttpResponse, String>>,
    /// Vista de la respuesta (solo lectura, se puede seleccionar y desplazar).
    pub view: Content,
    /// La vista muestra las cabeceras en vez del cuerpo.
    pub show_headers: bool,
    /// Peticiones recientes (la última primero).
    pub history: Vec<HttpForm>,
    /// Posición en el historial mientras se recorre con ↑ ↓.
    pub hist_pos: Option<usize>,
    /// Lo que había antes de recorrer el historial.
    pub draft: Option<HttpForm>,
    /// Foco.
    pub focus: Focus,
    /// Aviso breve en el pie («Copiado»).
    pub flash: Option<String>,
}

impl std::fmt::Debug for HttpPanel {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("HttpPanel")
            .field("method", &self.method)
            .field("url", &self.url)
            .field("expanded", &self.expanded)
            .field("tab", &self.tab)
            .finish_non_exhaustive()
    }
}

/// Índice de un método por su nombre (`GET` si no se conoce).
pub fn method_index(name: &str) -> usize {
    HTTP_METHODS
        .iter()
        .position(|m| m.eq_ignore_ascii_case(name.trim()))
        .unwrap_or(0)
}

/// `true` si `new` parece una orden `curl` recién pegada sobre `old`.
pub fn pasted_curl(old: &str, new: &str) -> bool {
    let t = new.trim_start().to_ascii_lowercase();
    (t.starts_with("curl ") || t.starts_with("curl.exe "))
        && (new.contains('\n') || new.chars().count() > old.chars().count() + 1)
}

/// Líneas no vacías de un texto (para el contador de una pestaña).
fn count_lines(s: &str) -> usize {
    s.lines()
        .filter(|l| !l.trim().is_empty() && !l.trim_start().starts_with('#'))
        .count()
}

impl HttpPanel {
    /// Panel vacío con el historial dado.
    pub fn new(history: Vec<HttpForm>) -> Self {
        Self {
            method: 0,
            url: String::new(),
            expanded: false,
            tab: 0,
            editors: [
                Content::new(),
                Content::new(),
                Content::new(),
                Content::new(),
            ],
            sending: None,
            seq: 0,
            response: None,
            view: Content::new(),
            show_headers: false,
            history,
            hist_pos: None,
            draft: None,
            focus: Focus::Url,
            flash: None,
        }
    }

    /// El formulario con lo escrito.
    pub fn form(&self) -> HttpForm {
        let t = |i: usize| self.editors[i].text().trim_end_matches('\n').to_owned();
        HttpForm {
            method: HTTP_METHODS[self.method].to_owned(),
            url: self.url.trim().to_owned(),
            headers: t(0),
            body: t(1),
            auth: t(2),
            params: t(3),
        }
    }

    /// Rellena los campos con una petición.
    pub fn load(&mut self, f: &HttpForm) {
        self.method = if f.method.trim().is_empty() {
            method_index(if f.body.trim().is_empty() {
                "GET"
            } else {
                "POST"
            })
        } else {
            method_index(&f.method)
        };
        self.url.clone_from(&f.url);
        for (i, v) in [&f.headers, &f.body, &f.auth, &f.params].iter().enumerate() {
            self.editors[i] = Content::with_text(v);
        }
        // Se despliega en la pestaña que tenga algo, empezando por el cuerpo.
        if let Some(i) = [1, 0, 2, 3]
            .into_iter()
            .find(|&i| !self.editors[i].text().trim().is_empty())
        {
            self.expanded = true;
            self.tab = i;
        }
    }

    /// Mezcla lo que dio la sintaxis corta (`3000/api {..}`) con lo ya escrito: la dirección y
    /// el cuerpo sustituyen, las cabeceras se añaden y el método elegido se respeta (con cuerpo,
    /// `GET` pasa a `POST`).
    pub fn merge(&mut self, f: HttpForm) {
        let mut cur = self.form();
        cur.url = f.url;
        if !f.body.trim().is_empty() {
            cur.body = f.body;
            if cur.method == "GET" {
                cur.method = "POST".into();
            }
        }
        if !f.headers.trim().is_empty() {
            cur.headers = [cur.headers.trim(), f.headers.trim()]
                .into_iter()
                .filter(|h| !h.is_empty())
                .collect::<Vec<_>>()
                .join("\n");
        }
        self.load(&cur);
    }

    /// ↑ (`older`) o ↓ por el historial. Devuelve si cambió algo.
    pub fn browse(&mut self, older: bool) -> bool {
        if self.history.is_empty() {
            return false;
        }
        let next = match (self.hist_pos, older) {
            (None, true) => Some(0),
            (None, false) => return false,
            (Some(i), true) => Some((i + 1).min(self.history.len() - 1)),
            (Some(0), false) => None,
            (Some(i), false) => Some(i - 1),
        };
        if next == self.hist_pos {
            return false;
        }
        if self.hist_pos.is_none() {
            self.draft = Some(self.form());
        }
        self.hist_pos = next;
        let f = match next {
            Some(i) => self.history[i].clone(),
            None => self.draft.take().unwrap_or_default(),
        };
        self.expanded = false;
        self.load(&f);
        true
    }

    /// Tab (o Shift+Tab): plegado → pestaña 1 → … → 4 → plegado.
    pub fn cycle_tab(&mut self, back: bool) {
        let n = TABS.len();
        let step: Option<usize> = match (self.expanded, back) {
            (false, false) => Some(0),
            (false, true) => Some(n - 1),
            (true, false) if self.tab + 1 < n => Some(self.tab + 1),
            (true, true) if self.tab > 0 => Some(self.tab - 1),
            _ => None,
        };
        match step {
            Some(t) => {
                self.expanded = true;
                self.tab = t;
            }
            None => self.expanded = false,
        }
    }

    /// Ctrl+M: el método siguiente (o el anterior).
    pub fn cycle_method(&mut self, back: bool) {
        let n = HTTP_METHODS.len();
        self.method = if back {
            (self.method + n - 1) % n
        } else {
            (self.method + 1) % n
        };
    }

    /// Muestra una respuesta.
    pub fn set_response(&mut self, r: Result<HttpResponse, String>) {
        self.sending = None;
        self.response = Some(r);
        self.refresh_view();
    }

    /// Vuelve a llenar la vista de la respuesta (cuerpo o cabeceras).
    pub fn refresh_view(&mut self) {
        let t = match &self.response {
            Some(Ok(r)) if self.show_headers => r.headers.clone(),
            Some(Ok(r)) => r.body.clone(),
            Some(Err(e)) => e.clone(),
            None => String::new(),
        };
        self.view = Content::with_text(&t);
    }

    /// Texto que se copia.
    pub fn copy_text(&self, what: CopyWhat) -> Option<String> {
        match (&self.response, what) {
            (Some(Ok(r)), CopyWhat::Body) => Some(r.body.clone()),
            (Some(Ok(r)), CopyWhat::Headers) => Some(r.headers.clone()),
            (Some(Ok(r)), CopyWhat::Curl) => Some(r.curl.clone()),
            _ => None,
        }
    }

    /// Alto del panel con la barra de búsqueda de `bar` píxeles y líneas de `line` píxeles.
    pub fn height(&self, bar: f32, line: f32) -> f32 {
        let mut h = bar;
        if self.expanded {
            h += 1.0 + TABS_H + EDITOR_H;
        }
        if self.sending.is_some() || self.response.is_some() {
            h += 1.0 + INFO_H;
            if self.response.is_some() {
                let lines = self.view.line_count().max(1) as f32;
                h += (lines * line + 16.0).clamp(RESPONSE_MIN, RESPONSE_MAX);
            }
        }
        h + 1.0 + FOOTER_H
    }
}

/// Teclas del editor de una pestaña: Ctrl+Enter envía (no salta de línea) y Tab cambia de
/// pestaña (la gestiona la aplicación).
fn tab_keys(k: KeyPress) -> Option<Binding<Message>> {
    use iced::keyboard::{Key, key::Named};
    match k.key.as_ref() {
        Key::Named(Named::Enter) if k.modifiers.control() => None,
        Key::Named(Named::Tab) => None,
        _ => Binding::from_key_press(k),
    }
}

/// Color de un método.
fn method_color(m: &str, l: &Look) -> Color {
    match m {
        "GET" => Color::from_rgb8(0x9e, 0xce, 0x6a),
        "POST" => Color::from_rgb8(0xe0, 0xaf, 0x68),
        "PUT" | "PATCH" => Color::from_rgb8(0x7a, 0xa2, 0xf7),
        "DELETE" => Color::from_rgb8(0xf7, 0x76, 0x8e),
        _ => l.accent,
    }
}

/// Color de un código de estado.
fn status_color(status: u32) -> Color {
    match status {
        200..=299 => Color::from_rgb8(0x9e, 0xce, 0x6a),
        300..=399 => Color::from_rgb8(0x7a, 0xa2, 0xf7),
        400..=499 => Color::from_rgb8(0xe0, 0xaf, 0x68),
        _ => Color::from_rgb8(0xf7, 0x76, 0x8e),
    }
}

/// `1234` → `1,2 KB`.
fn size(n: usize) -> String {
    if n < 1024 {
        format!("{n} B")
    } else if n < 1024 * 1024 {
        format!("{:.1} KB", n as f64 / 1024.0).replace('.', ",")
    } else {
        format!("{:.1} MB", n as f64 / 1_048_576.0).replace('.', ",")
    }
}

fn flat<'a>(
    l: &Look,
    content: impl Into<Element<'a, Message>>,
    active: bool,
) -> button::Button<'a, Message> {
    let (fg, sel, radius) = (l.fg, l.selection, l.radius);
    button(content).style(move |_, status| button::Style {
        background: (active || matches!(status, button::Status::Hovered | button::Status::Pressed))
            .then_some(Background::Color(if active {
                sel
            } else {
                Color {
                    a: sel.a * 0.6,
                    ..sel
                }
            })),
        text_color: fg,
        border: Border {
            radius: radius.into(),
            ..Border::default()
        },
        ..button::Style::default()
    })
}

fn line<'a>(c: Color) -> Element<'a, Message> {
    container(space().height(1).width(Length::Fill))
        .style(move |_| container::Style {
            background: Some(Background::Color(c)),
            ..container::Style::default()
        })
        .into()
}

fn editor_style(
    l: &Look,
) -> impl Fn(&iced::Theme, text_editor::Status) -> text_editor::Style + use<> {
    let (fg, muted, accent) = (l.fg, l.muted, l.accent);
    move |_, _| text_editor::Style {
        background: Background::Color(Color::TRANSPARENT),
        border: Border::default(),
        placeholder: muted,
        value: fg,
        selection: Color { a: 0.45, ..accent },
    }
}

/// Atajo pequeño con su texto: `[⌃⇧C] Cuerpo`.
fn shortcut<'a>(l: &Look, keys: &'a str, label: String, msg: Message) -> Element<'a, Message> {
    let muted = l.muted;
    flat(
        l,
        row![
            text(keys).font(l.mono).size(l.small() - 1.0).color(muted),
            text(label).size(l.small()).color(muted),
        ]
        .spacing(5)
        .align_y(iced::Alignment::Center),
        false,
    )
    .padding([3.0, 7.0])
    .on_press(msg)
    .into()
}

/// Vista del mini Postman.
pub fn view<'a>(p: &'a HttpPanel, l: &Look) -> Element<'a, Message> {
    let (fg, muted, divider, accent) = (l.fg, l.muted, l.divider, l.accent);
    let method = HTTP_METHODS[p.method];
    let mcolor = method_color(method, l);

    // Barra: método · dirección · enviar.
    let picker = pick_list(&HTTP_METHODS[..], Some(method), |m: &str| {
        Message::HttpMethod(method_index(m))
    })
    .font(l.mono)
    .text_size(l.small() + 1.0)
    .padding([6.0, 10.0])
    .width(Length::Fixed(112.0))
    .style(move |t, s| {
        let mut st = pick_list::default(t, s);
        st.text_color = mcolor;
        st.background = Background::Color(Color::TRANSPARENT);
        st.border = Border {
            radius: 6.0.into(),
            width: 1.0,
            color: Color { a: 0.5, ..mcolor },
        };
        st
    });
    let input = text_input(tr("httpui.placeholder"), &p.url)
        .id(url_id())
        .on_input(Message::HttpUrl)
        .size(l.input_size())
        .font(l.font)
        .padding([8.0, 12.0])
        .style(move |_, _| text_input::Style {
            background: Background::Color(Color::TRANSPARENT),
            border: Border::default(),
            icon: muted,
            placeholder: muted,
            value: fg,
            selection: Color { a: 0.45, ..accent },
        });
    let send_label = if p.sending.is_some() {
        tr("httpui.sending")
    } else {
        tr("httpui.send")
    };
    let send = button(
        row![
            text(send_label).size(l.small() + 1.0),
            text("↵").size(l.small()).font(l.mono),
        ]
        .spacing(6)
        .align_y(iced::Alignment::Center),
    )
    .padding([7.0, 14.0])
    .style(button::primary)
    .on_press_maybe(p.sending.is_none().then_some(Message::HttpSend));
    let bar = row![picker, input, send]
        .spacing(8)
        .align_y(iced::Alignment::Center)
        .padding([0.0, 14.0])
        .height(l.search_h);

    let mut col = column![bar];

    // Pestañas.
    if p.expanded {
        let mut tabs = row![].spacing(4).align_y(iced::Alignment::Center);
        for (i, id) in TABS.iter().enumerate() {
            let text_now = p.editors[i].text();
            let n = match i {
                1 | 2 => usize::from(!text_now.trim().is_empty()),
                _ => count_lines(&text_now),
            };
            let label = if n > 0 && i != 1 && i != 2 {
                format!("{} ({n})", tr(&format!("httpui.tab_{id}")))
            } else if n > 0 {
                format!("{} •", tr(&format!("httpui.tab_{id}")))
            } else {
                tr(&format!("httpui.tab_{id}")).to_owned()
            };
            let active = i == p.tab;
            tabs = tabs.push(
                flat(
                    l,
                    text(label)
                        .size(l.small())
                        .color(if active { fg } else { muted }),
                    active,
                )
                .padding([4.0, 12.0])
                .on_press(Message::HttpTab(i)),
            );
        }
        tabs = tabs.push(space().width(Length::Fill));
        tabs = tabs.push(
            text(tr(&format!("httpui.hint_{}", TABS[p.tab])))
                .size(l.small() - 1.0)
                .color(muted),
        );
        let tab = p.tab;
        let ed = editor(&p.editors[tab])
            .id(editor_id())
            .placeholder(tr(&format!("httpui.placeholder_{}", TABS[tab])))
            .on_action(move |a| Message::HttpEdit(tab, a))
            .key_binding(tab_keys)
            .font(l.mono)
            .size(l.small())
            .height(Length::Fixed(EDITOR_H - 8.0))
            .padding([4.0, 14.0])
            .style(editor_style(l));
        col = col
            .push(line(divider))
            .push(
                container(tabs)
                    .height(TABS_H)
                    .padding([0.0, 12.0])
                    .align_y(iced::alignment::Vertical::Center),
            )
            .push(container(ed).height(EDITOR_H));
    }

    // Respuesta.
    if p.sending.is_some() || p.response.is_some() {
        let info: Element<'a, Message> = match (&p.response, p.sending) {
            (_, Some(_)) => text(tr("httpui.waiting"))
                .size(l.small())
                .color(muted)
                .into(),
            (Some(Ok(r)), None) => {
                let sc = status_color(r.status);
                let mut sz = size(r.size);
                if r.truncated {
                    sz.push('+');
                }
                let badge = container(
                    text(format!("{} {}", r.status, r.reason).trim().to_owned())
                        .font(l.mono)
                        .size(l.small())
                        .color(sc),
                )
                .padding([2.0, 8.0])
                .style(move |_| container::Style {
                    background: Some(Background::Color(Color { a: 0.14, ..sc })),
                    border: Border {
                        radius: 4.0.into(),
                        ..Border::default()
                    },
                    ..container::Style::default()
                });
                let view_label = if p.show_headers {
                    tr("httpui.view_body")
                } else {
                    tr("httpui.view_headers")
                };
                row![
                    badge,
                    text(format!("{} ms", r.ms))
                        .font(l.mono)
                        .size(l.small())
                        .color(fg),
                    text(sz).font(l.mono).size(l.small()).color(fg),
                    text(if r.content_type.is_empty() {
                        String::new()
                    } else {
                        r.content_type.split(';').next().unwrap_or("").to_owned()
                    })
                    .size(l.small() - 1.0)
                    .color(muted),
                    space().width(Length::Fill),
                    shortcut(
                        l,
                        "⌃⇧C",
                        tr("httpui.copy_body").into(),
                        Message::HttpCopy(CopyWhat::Body)
                    ),
                    shortcut(l, "⌃H", view_label.into(), Message::HttpToggleView),
                    shortcut(l, "⌃⇧X", "cURL".into(), Message::HttpCopy(CopyWhat::Curl)),
                    shortcut(l, "Esc", tr("httpui.close").into(), Message::HttpClose),
                ]
                .spacing(10)
                .align_y(iced::Alignment::Center)
                .into()
            }
            (Some(Err(_)), None) => row![
                text("✕").size(l.small()).color(status_color(500)),
                text(tr("httpui.error"))
                    .size(l.small())
                    .color(status_color(500)),
                space().width(Length::Fill),
                shortcut(l, "Esc", tr("httpui.close").into(), Message::HttpClose),
            ]
            .spacing(8)
            .align_y(iced::Alignment::Center)
            .into(),
            (None, None) => space().into(),
        };
        col = col.push(line(divider)).push(
            container(info)
                .height(INFO_H)
                .padding([0.0, 14.0])
                .align_y(iced::alignment::Vertical::Center),
        );
        if p.response.is_some() {
            let body = editor(&p.view)
                .id(response_id())
                .on_action(Message::HttpView)
                .font(l.mono)
                .size(l.small())
                .height(Length::Fill)
                .padding([6.0, 14.0])
                .style(editor_style(l));
            col = col.push(container(body).height(Length::Fill));
        }
    }

    // Teclas.
    let keys = if p.response.is_some() {
        tr("httpui.keys_response")
    } else {
        tr("httpui.keys")
    };
    let hist = match p.hist_pos {
        _ if p.flash.is_some() => p.flash.clone().unwrap_or_default(),
        Some(i) => trf(
            "httpui.history_pos",
            &[("n", &(i + 1)), ("total", &p.history.len())],
        ),
        None => String::new(),
    };
    col = col.push(line(divider)).push(
        container(
            row![
                text(keys)
                    .size(l.small() - 1.0)
                    .color(Color { a: 0.85, ..muted }),
                space().width(Length::Fill),
                text(hist).size(l.small() - 1.0).color(accent),
            ]
            .align_y(iced::Alignment::Center),
        )
        .height(FOOTER_H)
        .padding([0.0, 14.0])
        .align_y(iced::alignment::Vertical::Center),
    );
    col.into()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn form(url: &str, body: &str) -> HttpForm {
        HttpForm {
            method: if body.is_empty() { "GET" } else { "POST" }.into(),
            url: url.into(),
            body: body.into(),
            ..HttpForm::default()
        }
    }

    #[test]
    fn tab_expands_cycles_and_folds() {
        let mut p = HttpPanel::new(vec![]);
        p.cycle_tab(false);
        assert!(p.expanded && p.tab == 0);
        for want in 1..TABS.len() {
            p.cycle_tab(false);
            assert_eq!(p.tab, want);
        }
        p.cycle_tab(false);
        assert!(!p.expanded);
        p.cycle_tab(true);
        assert!(p.expanded && p.tab == TABS.len() - 1);
        p.tab = 0;
        p.cycle_tab(true);
        assert!(!p.expanded);
    }

    #[test]
    fn history_with_arrows_keeps_the_draft() {
        let mut p = HttpPanel::new(vec![form("3000/a", ""), form("3000/b", "{}")]);
        p.url = "a medio escribir".into();
        assert!(!p.browse(false), "abajo sin haber subido no hace nada");
        assert!(p.browse(true));
        assert_eq!(p.url, "3000/a");
        assert!(p.browse(true));
        assert_eq!((p.url.as_str(), HTTP_METHODS[p.method]), ("3000/b", "POST"));
        assert!(p.expanded && p.tab == 1, "con cuerpo se abre su pestaña");
        assert!(!p.browse(true), "no hay más");
        assert!(p.browse(false));
        assert!(p.browse(false));
        assert_eq!(p.url, "a medio escribir");
        assert_eq!(p.hist_pos, None);
    }

    #[test]
    fn form_round_trips_and_methods_cycle() {
        let mut p = HttpPanel::new(vec![]);
        let f = HttpForm {
            method: "delete".into(),
            url: "3000/u/1".into(),
            headers: "X-A: 1".into(),
            body: String::new(),
            auth: "tok".into(),
            params: "a=1".into(),
        };
        p.load(&f);
        assert_eq!(
            p.form(),
            HttpForm {
                method: "DELETE".into(),
                ..f
            }
        );
        assert_eq!(p.tab, 0, "sin cuerpo se abren las cabeceras");
        p.cycle_method(false);
        assert_eq!(HTTP_METHODS[p.method], "HEAD");
        p.cycle_method(false);
        assert_eq!(HTTP_METHODS[p.method], "GET");
        p.cycle_method(true);
        assert_eq!(HTTP_METHODS[p.method], "HEAD");
        assert_eq!(method_index("nada"), 0);
    }

    #[test]
    fn detects_a_pasted_curl() {
        assert!(pasted_curl("", "curl https://x.com -H 'a: b'"));
        assert!(pasted_curl("curl", "curl \\\n -X POST x"));
        assert!(!pasted_curl("curl", "curl "), "escribiéndolo a mano");
        assert!(!pasted_curl("", "https://curl.se"));
    }

    #[test]
    fn response_view_and_height() {
        let mut p = HttpPanel::new(vec![]);
        let h0 = p.height(56.0, 18.0);
        p.cycle_tab(false);
        let h1 = p.height(56.0, 18.0);
        assert!(h1 > h0);
        p.set_response(Ok(HttpResponse {
            status: 200,
            reason: "OK".into(),
            ms: 12,
            size: 10,
            truncated: false,
            headers: "A: 1".into(),
            body: "{\n  \"a\": 1\n}".into(),
            content_type: "application/json".into(),
            curl: "curl x".into(),
        }));
        assert!(p.height(56.0, 18.0) > h1);
        assert_eq!(p.view.text().trim_end(), "{\n  \"a\": 1\n}");
        p.show_headers = true;
        p.refresh_view();
        assert_eq!(p.view.text().trim_end(), "A: 1");
        assert_eq!(p.copy_text(CopyWhat::Curl).as_deref(), Some("curl x"));
        p.set_response(Err("sin conexión".into()));
        assert_eq!(p.copy_text(CopyWhat::Body), None);
        assert_eq!(size(2048), "2,0 KB");
    }
}
