//! Aplicación iced: estado, mensajes, actualización y vista.

use std::collections::HashMap;
use std::sync::Arc;

use iced::futures::channel::mpsc;
use iced::futures::{SinkExt as _, StreamExt as _};
use iced::keyboard::{self, Key, Modifiers, key::Named};
use iced::widget::text::Wrapping;
use iced::widget::{
    Id, column, container, image, mouse_area, operation, row, space, text, text_input,
};
use iced::{
    Background, Border, Color, Element, Event, Font, Length, Size, Subscription, Task, event,
    mouse, window,
};
use rukes_core::i18n::tr;
use rukes_core::{ActionId, ActionOutcome, Icon, QueryBatch, QueryId, ResultItem, ranking};

use crate::theme;
use crate::{Host, OnShow, UiConfig, UiEvent, UiSender};

/// Resultados que se conservan en memoria (la vista muestra `max_results`).
const KEEP_RESULTS: usize = 200;
const FONT: Font = Font::with_name("Segoe UI Variable Text");

fn input_id() -> Id {
    Id::new("search")
}

#[derive(Debug, Clone)]
enum Message {
    Ready(window::Id, Option<isize>),
    Inbox(UiEvent),
    Input(String),
    Key(Key, Modifiers),
    Clicked(usize),
    Scrolled(f32),
    ActionDone(Result<ActionOutcome, String>),
    IconLoaded(Icon, Option<image::Handle>),
    Unfocused,
}

struct State {
    host: Arc<dyn Host>,
    tx: UiSender,
    cfg: UiConfig,
    window: Option<window::Id>,
    translucent: bool,
    query: String,
    qid: QueryId,
    /// `true` hasta que llegue el primer lote de la consulta actual: entonces se sustituyen los
    /// resultados (evita parpadeo entre pulsaciones).
    stale: bool,
    results: Vec<ResultItem>,
    selected: usize,
    offset: usize,
    icons: HashMap<Icon, Option<image::Handle>>,
    notice: Option<String>,
    busy: bool,
    rows_shown: Option<usize>,
}

/// Mueve la selección `delta` posiciones (con vuelta al principio/fin) y ajusta el desplazamiento
/// para que la selección quede visible. Devuelve `(seleccionado, desplazamiento)`.
fn move_selection(
    sel: usize,
    offset: usize,
    len: usize,
    visible: usize,
    delta: isize,
) -> (usize, usize) {
    if len == 0 {
        return (0, 0);
    }
    let len_i = len as isize;
    let sel = (sel as isize + delta).rem_euclid(len_i) as usize;
    (sel, scroll_to(sel, offset, len, visible))
}

fn scroll_to(sel: usize, offset: usize, len: usize, visible: usize) -> usize {
    let visible = visible.max(1);
    let max_off = len.saturating_sub(visible);
    let off = if sel < offset {
        sel
    } else if sel >= offset + visible {
        sel + 1 - visible
    } else {
        offset
    };
    off.min(max_off)
}

impl State {
    fn visible_rows(&self) -> usize {
        self.results.len().min(self.cfg.max_results)
    }

    fn height(&self) -> f32 {
        let rows = self.visible_rows();
        let mut h = theme::SEARCH_H;
        if rows > 0 {
            h += 1.0 + rows as f32 * theme::ROW_H + theme::LIST_PAD * 2.0;
        }
        if self.notice.is_some() {
            h += theme::NOTICE_H;
        }
        h
    }

    fn resize(&mut self) -> Task<Message> {
        let Some(id) = self.window else {
            return Task::none();
        };
        let key = self.visible_rows() + if self.notice.is_some() { 1000 } else { 0 };
        if self.rows_shown == Some(key) {
            return Task::none();
        }
        self.rows_shown = Some(key);
        window::resize(id, Size::new(self.cfg.width, self.height()))
    }

    fn run_query(&mut self) -> Task<Message> {
        if self.query.trim().is_empty() {
            self.results.clear();
            self.selected = 0;
            self.offset = 0;
            self.stale = false;
            self.qid = self.host.query("", Arc::new(|_| {}));
            return self.resize();
        }
        let tx = self.tx.clone();
        self.stale = true;
        self.qid = self.host.query(
            &self.query,
            Arc::new(move |b: QueryBatch| {
                let _ = tx.unbounded_send(UiEvent::Batch(b));
            }),
        );
        Task::none()
    }

    fn load_visible_icons(&mut self) -> Task<Message> {
        let end = (self.offset + self.cfg.max_results).min(self.results.len());
        let mut tasks = Vec::new();
        for item in &self.results[self.offset..end] {
            if matches!(item.icon, Icon::None) || self.icons.contains_key(&item.icon) {
                continue;
            }
            self.icons.insert(item.icon.clone(), None);
            let host = Arc::clone(&self.host);
            let icon = item.icon.clone();
            tasks.push(Task::perform(
                async move {
                    let h = host
                        .icon(&icon, theme::ICON_PX)
                        .map(|(w, h, px)| image::Handle::from_rgba(w, h, px));
                    (icon, h)
                },
                |(icon, h)| Message::IconLoaded(icon, h),
            ));
        }
        Task::batch(tasks)
    }

    fn execute(&mut self, idx: usize, action: &str) -> Task<Message> {
        let Some(item) = self.results.get(idx).cloned() else {
            return Task::none();
        };
        if self.busy || !item.has_action(action) {
            return Task::none();
        }
        self.busy = true;
        let host = Arc::clone(&self.host);
        let query = self.query.clone();
        let action = action.to_owned();
        Task::perform(
            async move { host.execute(&query, &item, &action) },
            Message::ActionDone,
        )
    }

    fn hide(&mut self) {
        self.host.hide();
    }

    fn on_shown(&mut self) -> Task<Message> {
        // El aviso (p. ej. un error de config llegado con la ventana oculta) se mantiene hasta
        // que el usuario escribe.
        self.busy = false;
        self.host.shown();
        let mut tasks = vec![operation::focus(input_id())];
        match self.cfg.on_show {
            OnShow::Clear => {
                self.query.clear();
                tasks.push(self.run_query());
            }
            OnShow::Keep => tasks.push(operation::move_cursor_to_end(input_id())),
            OnShow::SelectAll => {
                tasks.push(operation::select_all(input_id()));
                // Vuelve a buscar: la frecency pudo cambiar el orden.
                tasks.push(self.run_query());
            }
        }
        self.rows_shown = None;
        tasks.push(self.resize());
        Task::batch(tasks)
    }
}

fn update(s: &mut State, msg: Message) -> Task<Message> {
    match msg {
        Message::Ready(id, hwnd) => {
            s.window = Some(id);
            if let Some(h) = hwnd {
                s.translucent = s.host.window_ready(h);
            }
            s.resize()
        }
        Message::Inbox(ev) => match ev {
            UiEvent::Shown => s.on_shown(),
            UiEvent::Hide => {
                s.hide();
                Task::none()
            }
            UiEvent::Config(c) => {
                s.cfg = c;
                s.rows_shown = None;
                s.offset = scroll_to(s.selected, s.offset, s.results.len(), s.cfg.max_results);
                s.resize()
            }
            UiEvent::Notice(n) => {
                s.notice = Some(n);
                s.resize()
            }
            UiEvent::Batch(b) => {
                if b.id != s.qid {
                    return Task::none();
                }
                if s.stale {
                    s.results.clear();
                    s.selected = 0;
                    s.offset = 0;
                    s.stale = false;
                }
                ranking::merge(&mut s.results, b.items, KEEP_RESULTS);
                s.selected = s.selected.min(s.results.len().saturating_sub(1));
                Task::batch([s.load_visible_icons(), s.resize()])
            }
        },
        Message::Input(q) => {
            s.query = q;
            s.notice = None;
            s.run_query()
        }
        Message::Key(key, m) => {
            let visible = s.cfg.max_results;
            let len = s.results.len();
            let mv = |s: &mut State, d: isize| {
                (s.selected, s.offset) = move_selection(s.selected, s.offset, len, visible, d);
                s.load_visible_icons()
            };
            match key.as_ref() {
                Key::Named(Named::Escape) => {
                    s.hide();
                    Task::none()
                }
                Key::Named(Named::ArrowDown) => mv(s, 1),
                Key::Named(Named::ArrowUp) => mv(s, -1),
                Key::Named(Named::Tab) => mv(s, if m.shift() { -1 } else { 1 }),
                Key::Named(Named::PageDown) => mv(s, visible as isize),
                Key::Named(Named::PageUp) => mv(s, -(visible as isize)),
                Key::Character("j") if m.control() => mv(s, 1),
                Key::Character("k") if m.control() => mv(s, -1),
                Key::Named(Named::Enter) => {
                    let action = if m.control() && m.shift() {
                        ActionId::OPEN_LOCATION.to_owned()
                    } else if m.control() {
                        ActionId::RUN_AS_ADMIN.to_owned()
                    } else {
                        s.results
                            .get(s.selected)
                            .and_then(|i| i.actions.first())
                            .map(|a| a.id.0.clone())
                            .unwrap_or_default()
                    };
                    s.execute(s.selected, &action)
                }
                Key::Character("c") if m.control() => s.execute(s.selected, ActionId::COPY_PATH),
                _ => Task::none(),
            }
        }
        Message::Clicked(i) => {
            s.selected = i;
            let action = s
                .results
                .get(i)
                .and_then(|r| r.actions.first())
                .map(|a| a.id.0.clone())
                .unwrap_or_default();
            s.execute(i, &action)
        }
        Message::Scrolled(lines) => {
            let max_off = s.results.len().saturating_sub(s.cfg.max_results);
            let off = (s.offset as f32 - lines).round().clamp(0.0, max_off as f32) as usize;
            s.offset = off;
            s.selected = s
                .selected
                .clamp(off, off + s.cfg.max_results.saturating_sub(1));
            s.load_visible_icons()
        }
        Message::ActionDone(res) => {
            s.busy = false;
            match res {
                Ok(ActionOutcome::Hide) => {
                    if s.cfg.hide_after_action {
                        s.hide();
                    }
                    Task::none()
                }
                Ok(ActionOutcome::KeepOpen) => Task::none(),
                Ok(ActionOutcome::ReplaceQuery(q)) => {
                    s.query = q;
                    Task::batch([s.run_query(), operation::move_cursor_to_end(input_id())])
                }
                Ok(ActionOutcome::Notify(n)) => {
                    s.notice = Some(n);
                    s.resize()
                }
                Err(e) => {
                    s.notice = Some(format!("{}: {e}", tr("error.action")));
                    s.resize()
                }
            }
        }
        Message::IconLoaded(icon, handle) => {
            if handle.is_some() {
                s.icons.insert(icon, handle);
            }
            Task::none()
        }
        Message::Unfocused => {
            if s.cfg.hide_on_blur && !s.busy {
                s.hide();
            }
            Task::none()
        }
    }
}

fn result_row<'a>(s: &'a State, i: usize, item: &'a ResultItem) -> Element<'a, Message> {
    let icon: Element<'_, Message> = match s.icons.get(&item.icon) {
        Some(Some(h)) => image(h.clone())
            .width(theme::ICON)
            .height(theme::ICON)
            .into(),
        _ => space().width(theme::ICON).height(theme::ICON).into(),
    };
    let texts = column![
        text(item.title.as_str())
            .size(15)
            .color(theme::FG)
            .wrapping(Wrapping::None),
        text(item.subtitle.as_str())
            .size(12.5)
            .color(theme::MUTED)
            .wrapping(Wrapping::None),
    ]
    .spacing(1);
    let selected = i == s.selected;
    let body = container(
        row![icon, texts]
            .spacing(12)
            .align_y(iced::Alignment::Center),
    )
    .padding([0, 12])
    .width(Length::Fill)
    .height(theme::ROW_H)
    .align_y(iced::alignment::Vertical::Center)
    .clip(true)
    .style(move |_| container::Style {
        background: selected.then_some(Background::Color(theme::SELECTION)),
        border: Border {
            radius: 8.0.into(),
            ..Border::default()
        },
        ..container::Style::default()
    });
    mouse_area(body)
        .on_press(Message::Clicked(i))
        .interaction(mouse::Interaction::Pointer)
        .into()
}

fn view(s: &State) -> Element<'_, Message> {
    let input = text_input(tr("search.placeholder"), &s.query)
        .id(input_id())
        .on_input(Message::Input)
        .size(20)
        .padding([16, 20])
        .font(FONT)
        .style(|_, _| text_input::Style {
            background: Background::Color(Color::TRANSPARENT),
            border: Border::default(),
            icon: theme::MUTED,
            placeholder: theme::MUTED,
            value: theme::FG,
            selection: Color {
                a: 0.45,
                ..theme::ACCENT
            },
        });

    let mut col = column![container(input).height(theme::SEARCH_H)];
    let rows = s.visible_rows();
    if rows > 0 {
        col = col.push(container(space().height(1).width(Length::Fill)).style(|_| {
            container::Style {
                background: Some(Background::Color(theme::DIVIDER)),
                ..container::Style::default()
            }
        }));
        let list = s
            .results
            .iter()
            .enumerate()
            .skip(s.offset)
            .take(rows)
            .fold(column![].spacing(0), |c, (i, item)| {
                c.push(result_row(s, i, item))
            });
        col = col.push(
            mouse_area(container(list).padding([theme::LIST_PAD, 8.0])).on_scroll(|d| {
                Message::Scrolled(match d {
                    mouse::ScrollDelta::Lines { y, .. } => y,
                    mouse::ScrollDelta::Pixels { y, .. } => y / theme::ROW_H,
                })
            }),
        );
    }
    if let Some(n) = &s.notice {
        col = col.push(
            container(text(n.as_str()).size(12.5).color(theme::MUTED))
                .padding([6, 20])
                .height(theme::NOTICE_H),
        );
    }
    let bg = theme::background(s.translucent);
    container(col)
        .width(Length::Fill)
        .height(Length::Fill)
        .style(move |_| container::Style {
            background: Some(Background::Color(bg)),
            text_color: Some(theme::FG),
            ..container::Style::default()
        })
        .into()
}

fn inbox_stream() -> impl iced::futures::Stream<Item = Message> {
    iced::stream::channel(64, async |mut out: mpsc::Sender<Message>| {
        let rx = super::INBOX.lock().ok().and_then(|mut r| r.take());
        if let Some(mut rx) = rx {
            while let Some(ev) = rx.next().await {
                if out.send(Message::Inbox(ev)).await.is_err() {
                    break;
                }
            }
        }
        std::future::pending::<()>().await;
    })
}

fn on_event(ev: Event, _status: event::Status, _id: window::Id) -> Option<Message> {
    match ev {
        Event::Keyboard(keyboard::Event::KeyPressed { key, modifiers, .. }) => {
            Some(Message::Key(key, modifiers))
        }
        Event::Window(window::Event::Unfocused) => Some(Message::Unfocused),
        _ => None,
    }
}

fn subscription(_: &State) -> Subscription<Message> {
    Subscription::batch([
        Subscription::run(inbox_stream),
        event::listen_with(on_event),
    ])
}

pub(crate) fn run(host: Arc<dyn Host>, tx: UiSender, config: UiConfig) -> iced::Result {
    let width = config.width;
    let boot = move || {
        let state = State {
            host: Arc::clone(&host),
            tx: tx.clone(),
            cfg: config.clone(),
            window: None,
            translucent: false,
            query: String::new(),
            qid: QueryId::default(),
            stale: false,
            results: Vec::new(),
            selected: 0,
            offset: 0,
            icons: HashMap::new(),
            notice: None,
            busy: false,
            rows_shown: None,
        };
        let ready = window::oldest().then(|id| {
            let Some(id) = id else { return Task::none() };
            window::run(id, move |w| {
                use raw_window_handle::RawWindowHandle;
                let hwnd = w.window_handle().ok().and_then(|h| match h.as_raw() {
                    RawWindowHandle::Win32(h) => Some(h.hwnd.get()),
                    _ => None,
                });
                (id, hwnd)
            })
            .map(|(id, hwnd)| Message::Ready(id, hwnd))
        });
        (state, ready)
    };
    iced::application(boot, update, view)
        .title("Rukes")
        .subscription(subscription)
        .default_font(FONT)
        .window(window::Settings {
            size: Size::new(width, theme::SEARCH_H),
            position: window::Position::Centered,
            visible: false,
            decorations: false,
            transparent: true,
            resizable: false,
            level: window::Level::AlwaysOnTop,
            exit_on_close_request: false,
            platform_specific: window::settings::PlatformSpecific {
                skip_taskbar: true,
                ..Default::default()
            },
            ..Default::default()
        })
        .style(|_, _| iced::theme::Style {
            background_color: Color::TRANSPARENT,
            text_color: theme::FG,
        })
        .run()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn selection_wraps_and_scrolls() {
        // 20 resultados, 8 visibles.
        assert_eq!(move_selection(0, 0, 20, 8, 1), (1, 0));
        assert_eq!(move_selection(7, 0, 20, 8, 1), (8, 1));
        assert_eq!(move_selection(0, 0, 20, 8, -1), (19, 12));
        assert_eq!(move_selection(19, 12, 20, 8, 1), (0, 0));
        assert_eq!(move_selection(0, 0, 20, 8, 8), (8, 1));
        assert_eq!(move_selection(0, 0, 0, 8, 1), (0, 0));
    }

    #[test]
    fn scroll_keeps_selection_visible() {
        assert_eq!(scroll_to(3, 5, 20, 8), 3);
        assert_eq!(scroll_to(15, 0, 20, 8), 8);
        assert_eq!(scroll_to(4, 0, 5, 8), 0);
    }
}
