//! Aplicación iced: estado, mensajes, actualización y vista.

use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Arc;
use std::time::{Duration, Instant};

use iced::futures::channel::mpsc;
use iced::futures::{SinkExt as _, StreamExt as _};
use iced::keyboard::{self, Key, Modifiers, key::Named};
use iced::widget::text::{Shaping, Wrapping};
use iced::widget::{
    Id, column, container, image, mouse_area, operation, rich_text, row, space, span, text,
    text_input,
};
use iced::{
    Background, Border, Color, ContentFit, Element, Event, Length, Size, Subscription, Task, event,
    font, mouse, window,
};
use rukes_core::i18n::tr;
use rukes_core::{ActionId, ActionOutcome, Icon, QueryBatch, QueryId, ResultItem, ranking};

use crate::theme::{self, Look};
use crate::{Host, OnShow, Preview, UiConfig, UiEvent, UiSender};

/// Resultados que se conservan en memoria (la vista muestra `max_results`).
const KEEP_RESULTS: usize = 200;
/// Iconos en memoria; al pasar de aquí se vacía la caché (los del disco cargan rápido).
const KEEP_ICONS: usize = 512;
/// Previsualizaciones en memoria.
const KEEP_PREVIEWS: usize = 48;
/// Duraciones de las animaciones (RF-195: 80–150 ms).
const FADE: Duration = Duration::from_millis(110);
const SELECT: Duration = Duration::from_millis(90);
const RESIZE: Duration = Duration::from_millis(100);
/// Fotogramas extra tras redimensionar.
const SETTLE: Duration = Duration::from_millis(120);
/// Filas mínimas con el panel de previsualización abierto (para que quepa).
const PREVIEW_ROWS: usize = 6;
/// Líneas de texto en la previsualización.
const PREVIEW_LINES: usize = 40;

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
    PreviewLoaded(PathBuf, Option<Arc<Loaded>>),
    Frame(Instant),
    Unfocused,
}

/// Previsualización con la imagen ya convertida para iced.
#[derive(Debug)]
struct Loaded {
    preview: Preview,
    image: Option<image::Handle>,
}

/// Animación en curso.
#[derive(Debug, Clone, Copy)]
struct Anim {
    start: Instant,
    dur: Duration,
}

impl Anim {
    fn new(dur: Duration) -> Self {
        Self {
            start: Instant::now(),
            dur,
        }
    }

    /// Progreso 0..1 con salida suave (ease-out cúbico).
    fn at(&self, now: Instant) -> f32 {
        let t = now.saturating_duration_since(self.start).as_secs_f32() / self.dur.as_secs_f32();
        let t = t.clamp(0.0, 1.0);
        1.0 - (1.0 - t).powi(3)
    }

    fn done(&self, now: Instant) -> bool {
        now.saturating_duration_since(self.start) >= self.dur
    }
}

struct State {
    host: Arc<dyn Host>,
    tx: UiSender,
    cfg: UiConfig,
    look: Look,
    window: Option<window::Id>,
    translucent: bool,
    visible: bool,
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
    preview_on: bool,
    previews: HashMap<PathBuf, Option<Arc<Loaded>>>,
    /// Tamaño pedido por última vez y tamaño actual (distinto mientras se anima).
    target: Option<Size>,
    current: Size,
    now: Instant,
    fade: Option<Anim>,
    select_anim: Option<(usize, Anim)>,
    resize_anim: Option<(f32, Anim)>,
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

/// Texto con modelado avanzado: ligaduras, emoji en color y respaldo de fuentes (RF-197).
fn txt<'a>(s: impl text::IntoFragment<'a>) -> text::Text<'a> {
    text(s).shaping(Shaping::Advanced)
}

impl State {
    fn visible_rows(&self) -> usize {
        self.results.len().min(self.cfg.max_results)
    }

    /// Filas que ocupa la lista (con el panel abierto, al menos unas cuantas).
    fn list_rows(&self) -> usize {
        let rows = self.visible_rows();
        if self.preview_on && rows > 0 {
            rows.max(PREVIEW_ROWS.min(self.cfg.max_results))
        } else {
            rows
        }
    }

    fn width(&self) -> f32 {
        if self.preview_on {
            self.cfg.width + 1.0 + self.cfg.preview_width
        } else {
            self.cfg.width
        }
    }

    fn height(&self) -> f32 {
        let rows = self.list_rows();
        let mut h = self.look.search_h;
        if rows > 0 {
            h += 1.0 + rows as f32 * self.look.row_h + theme::LIST_PAD * 2.0;
        }
        if self.notice.is_some() {
            h += theme::NOTICE_H;
        }
        h.round()
    }

    fn animating(&self) -> bool {
        self.fade.is_some() || self.select_anim.is_some() || self.resize_anim.is_some()
    }

    fn resize(&mut self) -> Task<Message> {
        let Some(id) = self.window else {
            return Task::none();
        };
        let target = Size::new(self.width(), self.height());
        if self.target == Some(target) {
            return Task::none();
        }
        self.target = Some(target);
        // Solo se anima la altura con la ventana visible y el mismo ancho.
        if self.cfg.animations
            && self.visible
            && (self.current.width - target.width).abs() < 0.5
            && (self.current.height - target.height).abs() >= 1.0
        {
            self.resize_anim = Some((self.current.height, Anim::new(RESIZE)));
            return Task::none();
        }
        self.resize_anim = None;
        self.current = target;
        window::resize(id, target)
    }

    fn set_style(&mut self) {
        self.look = Look::new(&self.cfg.style, self.translucent);
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
        if self.icons.len() > KEEP_ICONS {
            self.icons.clear();
        }
        let end = (self.offset + self.cfg.max_results).min(self.results.len());
        let px = self.look.icon_px();
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
                        .icon(&icon, px)
                        .map(|(w, h, px)| image::Handle::from_rgba(w, h, px));
                    (icon, h)
                },
                |(icon, h)| Message::IconLoaded(icon, h),
            ));
        }
        Task::batch(tasks)
    }

    fn load_preview(&mut self) -> Task<Message> {
        if !self.preview_on {
            return Task::none();
        }
        let Some(path) = self
            .results
            .get(self.selected)
            .and_then(|i| i.preview.clone())
        else {
            return Task::none();
        };
        if self.previews.contains_key(&path) {
            return Task::none();
        }
        if self.previews.len() > KEEP_PREVIEWS {
            self.previews.clear();
        }
        self.previews.insert(path.clone(), None);
        let host = Arc::clone(&self.host);
        Task::perform(
            async move {
                let loaded = host.preview(&path).map(|p| {
                    let image = p
                        .image
                        .clone()
                        .map(|(w, h, px)| image::Handle::from_rgba(w, h, px));
                    Arc::new(Loaded { preview: p, image })
                });
                (path, loaded)
            },
            |(p, l)| Message::PreviewLoaded(p, l),
        )
    }

    /// Cambia la selección (con animación) y carga lo que haga falta.
    fn select(&mut self, sel: usize, offset: usize) -> Task<Message> {
        if sel != self.selected && self.cfg.animations && self.visible {
            self.select_anim = Some((self.selected, Anim::new(SELECT)));
        }
        self.selected = sel;
        self.offset = offset;
        Task::batch([self.load_visible_icons(), self.load_preview()])
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
        self.visible = false;
        self.fade = None;
        self.select_anim = None;
        self.host.hide();
    }

    /// Sustituye la lista (confirmaciones, submenús, salida de scripts).
    fn show_results(&mut self, items: Vec<ResultItem>) -> Task<Message> {
        // Cancela la consulta en curso para que sus lotes no pisen esta lista.
        self.qid = self.host.query("", Arc::new(|_| {}));
        self.results = items;
        self.selected = 0;
        self.offset = 0;
        self.stale = false;
        Task::batch([
            self.load_visible_icons(),
            self.load_preview(),
            self.resize(),
        ])
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
        // El tamaño se fija sin animar (aún «oculta» para `resize`); luego aparece el contenido.
        self.target = None;
        tasks.push(self.resize());
        self.visible = true;
        if self.cfg.animations {
            self.fade = Some(Anim::new(FADE));
            self.now = Instant::now();
        }
        tasks.push(self.load_preview());
        Task::batch(tasks)
    }

    fn on_frame(&mut self, now: Instant) -> Task<Message> {
        self.now = now;
        if self.fade.is_some_and(|a| a.done(now)) {
            self.fade = None;
        }
        if self.select_anim.is_some_and(|(_, a)| a.done(now)) {
            self.select_anim = None;
        }
        let (Some((from, a)), Some(target), Some(id)) =
            (self.resize_anim, self.target, self.window)
        else {
            return Task::none();
        };
        let h = if a.done(now) {
            // Se siguen pidiendo fotogramas un poco más: iced a veces no redibuja tras el
            // último cambio de tamaño de una serie rápida y quedaría una franja sin pintar.
            if now.saturating_duration_since(a.start) >= RESIZE + SETTLE {
                self.resize_anim = None;
            }
            target.height
        } else {
            (from + (target.height - from) * a.at(now)).round()
        };
        if (h - self.current.height).abs() < 0.5 {
            return Task::none();
        }
        self.current = Size::new(target.width, h);
        window::resize(id, self.current)
    }
}

fn update(s: &mut State, msg: Message) -> Task<Message> {
    match msg {
        Message::Ready(id, hwnd) => {
            s.window = Some(id);
            if let Some(h) = hwnd {
                s.translucent = s.host.window_ready(h);
                s.set_style();
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
                let icons_changed = (c.style.icon_size - s.cfg.style.icon_size).abs() > 0.1;
                let panel_changed = c.preview_panel != s.cfg.preview_panel;
                if s.window.is_some() {
                    s.translucent = c.style.translucent;
                }
                s.cfg = c;
                s.set_style();
                if icons_changed {
                    s.icons.clear();
                }
                if panel_changed {
                    s.preview_on = s.cfg.preview_panel;
                }
                if !s.cfg.animations {
                    s.fade = None;
                    s.select_anim = None;
                }
                s.offset = scroll_to(s.selected, s.offset, s.results.len(), s.cfg.max_results);
                Task::batch([s.resize(), s.load_visible_icons(), s.load_preview()])
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
                Task::batch([s.load_visible_icons(), s.load_preview(), s.resize()])
            }
            UiEvent::Results { replaces, items } => {
                if s.results.first().map(ResultItem::key).as_deref() != Some(&replaces) {
                    return Task::none();
                }
                s.show_results(items)
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
                let (sel, off) = move_selection(s.selected, s.offset, len, visible, d);
                s.select(sel, off)
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
                Key::Character("o") if m.control() => s.execute(s.selected, ActionId::OPEN_WITH),
                Key::Character("t") if m.control() => {
                    s.execute(s.selected, ActionId::TERMINAL_HERE)
                }
                Key::Character("e") if m.control() => s.execute(s.selected, ActionId::EDIT),
                Key::Character("d") if m.control() => s.execute(s.selected, ActionId::DELETE),
                Key::Character("s") if m.control() => s.execute(s.selected, ActionId::PIN),
                Key::Character("p") if m.control() => {
                    s.preview_on = !s.preview_on;
                    Task::batch([s.resize(), s.load_preview()])
                }
                _ => Task::none(),
            }
        }
        Message::Clicked(i) => {
            let off = s.offset;
            let load = s.select(i, off);
            let action = s
                .results
                .get(i)
                .and_then(|r| r.actions.first())
                .map(|a| a.id.0.clone())
                .unwrap_or_default();
            Task::batch([load, s.execute(i, &action)])
        }
        Message::Scrolled(lines) => {
            let max_off = s.results.len().saturating_sub(s.cfg.max_results);
            let off = (s.offset as f32 - lines).round().clamp(0.0, max_off as f32) as usize;
            let sel = s
                .selected
                .clamp(off, off + s.cfg.max_results.saturating_sub(1));
            s.select(sel, off)
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
                Ok(ActionOutcome::ShowResults(items)) => s.show_results(items),
                Ok(ActionOutcome::Paste {
                    text,
                    cursor_left,
                    restore,
                }) => {
                    // Primero se oculta (vuelve el foco a la ventana anterior) y luego se pega.
                    s.hide();
                    s.host.paste(&text, cursor_left, restore);
                    Task::none()
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
        Message::PreviewLoaded(path, loaded) => {
            s.previews.insert(path, loaded);
            Task::none()
        }
        Message::Frame(now) => s.on_frame(now),
        Message::Unfocused => {
            if s.cfg.hide_on_blur && !s.busy {
                s.hide();
            }
            Task::none()
        }
    }
}

/// Icono de texto para los plugins integrados (`Icon::Named`).
fn glyph<'a>(l: &Look, name: &str) -> Element<'a, Message> {
    let symbol = match name {
        "calc" => "=",
        "web" => "@",
        "shell" => ">_",
        "window" => "[]",
        "process" => "ps",
        "generator" => "#",
        "folder" => "/",
        "system" => "sys",
        "script" => "{}",
        "new" => "+",
        "log" => "log",
        "running" => "..",
        "error" => "!",
        "ok" => "ok",
        "clipboard" => "cb",
        "snippet" => "<>",
        "note" => "md",
        "todo" => "[ ]",
        "remind" => "!t",
        _ => "·",
    };
    let (bg, radius) = (l.selection, (l.radius - 2.0).max(0.0));
    container(
        text(symbol)
            .size((l.icon * 0.46).round())
            .font(l.mono)
            .color(l.accent),
    )
    .width(l.icon)
    .height(l.icon)
    .center_x(l.icon)
    .center_y(l.icon)
    .style(move |_| container::Style {
        background: Some(Background::Color(bg)),
        border: Border {
            radius: radius.into(),
            ..Border::default()
        },
        ..container::Style::default()
    })
    .into()
}

/// Opacidad del fondo de selección de la fila `i` (anima el cambio de fila).
fn selection_alpha(s: &State, i: usize) -> f32 {
    match s.select_anim {
        Some((prev, a)) => {
            let t = a.at(s.now);
            if i == s.selected {
                t
            } else if i == prev {
                1.0 - t
            } else {
                0.0
            }
        }
        None => f32::from(u8::from(i == s.selected)),
    }
}

fn result_row<'a>(s: &'a State, l: &Look, i: usize, item: &'a ResultItem) -> Element<'a, Message> {
    let fade = s.fade.map_or(1.0, |a| a.at(s.now));
    let icon: Element<'_, Message> = match s.icons.get(&item.icon) {
        Some(Some(h)) => image(h.clone())
            .width(l.icon)
            .height(l.icon)
            .opacity(fade)
            .into(),
        _ => match &item.icon {
            Icon::Named(name) => glyph(l, name),
            _ => space().width(l.icon).height(l.icon).into(),
        },
    };
    let sel = selection_alpha(s, i);
    let selected = sel > 0.5;
    let texts = column![
        txt(item.title.as_str())
            .size(l.size)
            .font(l.font)
            .color(if selected { l.sel_fg } else { l.fg })
            .wrapping(Wrapping::None),
        txt(item.subtitle.as_str())
            .size(l.small())
            .font(l.font)
            .color(if selected { l.sel_muted } else { l.muted })
            .wrapping(Wrapping::None),
    ]
    .spacing(1);
    let bg = Color {
        a: l.selection.a * sel,
        ..l.selection
    };
    let radius = l.radius;
    let body = container(
        row![icon, texts]
            .spacing(l.spacing)
            .align_y(iced::Alignment::Center),
    )
    .padding([0.0, l.pad])
    .width(Length::Fill)
    .height(l.row_h)
    .align_y(iced::alignment::Vertical::Center)
    .clip(true)
    .style(move |_| container::Style {
        background: (sel > 0.0).then_some(Background::Color(bg)),
        border: Border {
            radius: radius.into(),
            ..Border::default()
        },
        ..container::Style::default()
    });
    mouse_area(body)
        .on_press(Message::Clicked(i))
        .interaction(mouse::Interaction::Pointer)
        .into()
}

/// Markdown sencillo: títulos, viñetas y bloques de código (RF-017). Se dibuja como un solo
/// texto enriquecido (un widget por línea se recortaba al ajustar líneas largas).
fn markdown<'a>(l: &Look, src: &str) -> Element<'a, Message> {
    let mut spans: Vec<text::Span<'a, (), iced::Font>> = Vec::new();
    let mut code = false;
    let bold = iced::Font {
        weight: font::Weight::Semibold,
        ..l.font
    };
    for line in src.lines().take(PREVIEW_LINES) {
        let t = line.trim_end();
        if t.trim_start().starts_with("```") {
            code = !code;
            continue;
        }
        let span = if code {
            span(format!(
                "{t}
"
            ))
            .font(l.mono)
            .size(l.small() - 0.5)
            .color(l.fg)
        } else if let Some(h) = t.strip_prefix('#') {
            let level = 1 + h.chars().take_while(|c| *c == '#').count();
            let h = h.trim_start_matches('#').trim();
            span(format!(
                "{h}
"
            ))
            .font(bold)
            .size(l.size + (4.0 - level as f32).max(0.0))
            .color(l.fg)
        } else if let Some(b) = t
            .trim_start()
            .strip_prefix("- ")
            .or_else(|| t.trim_start().strip_prefix("* "))
        {
            span(format!(
                "•  {}
",
                b.replace("**", "").replace('`', "")
            ))
            .font(l.font)
            .size(l.small())
            .color(l.fg)
        } else {
            span(format!(
                "{}
",
                t.replace("**", "").replace('`', "")
            ))
            .font(l.font)
            .size(l.small())
            .color(l.muted)
        };
        spans.push(span);
    }
    rich_text(spans).width(Length::Fill).into()
}

fn preview_panel<'a>(s: &'a State, l: &Look) -> Element<'a, Message> {
    let mut col = column![].spacing(10).width(Length::Fill);
    let item = s.results.get(s.selected);
    let loaded = item
        .and_then(|i| i.preview.as_ref())
        .and_then(|p| s.previews.get(p))
        .and_then(Option::as_ref);
    if let Some(item) = item {
        if let Some(h) = loaded.and_then(|p| p.image.as_ref()) {
            col = col.push(
                container(
                    image(h.clone())
                        .content_fit(ContentFit::Contain)
                        .width(Length::Fill)
                        .height(Length::Fill),
                )
                .height(Length::Fixed(
                    (s.cfg.preview_width * 0.6).min(l.row_h * 4.0),
                ))
                .center_x(Length::Fill),
            );
        }
        col = col.push(
            txt(item.title.as_str())
                .size(l.size)
                .font(iced::Font {
                    weight: font::Weight::Semibold,
                    ..l.font
                })
                .color(l.fg),
        );
        match loaded {
            Some(p) => {
                for (k, v) in &p.preview.info {
                    col = col.push(
                        column![
                            txt(k.as_str()).size(l.small() - 1.0).color(l.muted),
                            txt(v.as_str()).size(l.small()).color(l.fg),
                        ]
                        .spacing(1),
                    );
                }
                if let Some(t) = &p.preview.text {
                    let body: Element<'_, Message> = if p.preview.markdown {
                        markdown(l, t)
                    } else {
                        txt(t.as_str())
                            .font(l.mono)
                            .size(l.small() - 0.5)
                            .color(l.fg)
                            .into()
                    };
                    let line = l.divider;
                    col = col.push(container(space().height(1).width(Length::Fill)).style(
                        move |_| container::Style {
                            background: Some(Background::Color(line)),
                            ..container::Style::default()
                        },
                    ));
                    col = col.push(body);
                }
            }
            None if item.preview.is_none() => {
                if !item.subtitle.is_empty() {
                    col = col.push(txt(item.subtitle.as_str()).size(l.small()).color(l.muted));
                }
                if let Some(d) = &item.detail {
                    col = col.push(divider(l.divider, false));
                    col = col.push(
                        txt(d.as_str())
                            .font(l.mono)
                            .size(l.small() - 0.5)
                            .color(l.fg)
                            .width(Length::Fill),
                    );
                } else {
                    col = col.push(txt(tr("preview.hint")).size(l.small() - 1.0).color(l.muted));
                }
            }
            None => {}
        }
    }
    container(col)
        .padding([theme::LIST_PAD + 6.0, l.pad + 4.0])
        .width(s.cfg.preview_width)
        .height(Length::Fill)
        .clip(true)
        .into()
}

fn divider<'a>(c: Color, vertical: bool) -> Element<'a, Message> {
    let line = if vertical {
        space().width(1).height(Length::Fill)
    } else {
        space().height(1).width(Length::Fill)
    };
    container(line)
        .style(move |_| container::Style {
            background: Some(Background::Color(c)),
            ..container::Style::default()
        })
        .into()
}

fn view(s: &State) -> Element<'_, Message> {
    let fade = s.fade.map_or(1.0, |a| a.at(s.now));
    let l = s.look.faded(fade);
    let (fg, muted, accent) = (l.fg, l.muted, l.accent);
    let input = text_input(tr("search.placeholder"), &s.query)
        .id(input_id())
        .on_input(Message::Input)
        .size(l.input_size())
        .padding([((l.search_h - l.input_size() * 1.3) / 2.0).max(4.0), 20.0])
        .font(l.font)
        .style(move |_, _| text_input::Style {
            background: Background::Color(Color::TRANSPARENT),
            border: Border::default(),
            icon: muted,
            placeholder: muted,
            value: fg,
            selection: Color { a: 0.45, ..accent },
        });

    let mut col = column![
        container(input)
            .height(l.search_h)
            .align_y(iced::alignment::Vertical::Center)
    ];
    let rows = s.visible_rows();
    if rows > 0 {
        col = col.push(divider(l.divider, false));
        let list = s
            .results
            .iter()
            .enumerate()
            .skip(s.offset)
            .take(rows)
            .fold(column![].spacing(0), |c, (i, item)| {
                c.push(result_row(s, &l, i, item))
            });
        let row_h = l.row_h;
        let list = mouse_area(
            container(list)
                .padding([theme::LIST_PAD, 8.0])
                .width(s.cfg.width)
                .height(Length::Fill),
        )
        .on_scroll(move |d| {
            Message::Scrolled(match d {
                mouse::ScrollDelta::Lines { y, .. } => y,
                mouse::ScrollDelta::Pixels { y, .. } => y / row_h,
            })
        });
        if s.preview_on {
            col = col.push(
                row![list, divider(l.divider, true), preview_panel(s, &l)]
                    .height(s.list_rows() as f32 * l.row_h + theme::LIST_PAD * 2.0),
            );
        } else {
            col = col.push(list);
        }
    }
    if let Some(n) = &s.notice {
        col = col.push(
            container(txt(n.as_str()).size(l.small()).color(l.muted))
                .padding([6, 20])
                .height(theme::NOTICE_H),
        );
    }
    let bg = l.bg;
    container(col)
        .width(Length::Fill)
        .height(Length::Fill)
        .clip(true)
        .style(move |_| container::Style {
            background: Some(Background::Color(bg)),
            text_color: Some(fg),
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

fn subscription(s: &State) -> Subscription<Message> {
    let mut subs = vec![
        Subscription::run(inbox_stream),
        event::listen_with(on_event),
    ];
    // Solo se piden fotogramas mientras hay una animación: en reposo, ningún coste.
    if s.animating() {
        subs.push(window::frames().map(Message::Frame));
    }
    Subscription::batch(subs)
}

pub(crate) fn run(host: Arc<dyn Host>, tx: UiSender, config: UiConfig) -> iced::Result {
    let look = Look::new(&config.style, false);
    let initial = Size::new(config.width, look.search_h);
    let boot = move || {
        let state = State {
            host: Arc::clone(&host),
            tx: tx.clone(),
            preview_on: config.preview_panel,
            cfg: config.clone(),
            look,
            window: None,
            translucent: false,
            visible: false,
            query: String::new(),
            qid: QueryId::default(),
            stale: false,
            results: Vec::new(),
            selected: 0,
            offset: 0,
            icons: HashMap::new(),
            notice: None,
            busy: false,
            previews: HashMap::new(),
            target: None,
            current: initial,
            now: Instant::now(),
            fade: None,
            select_anim: None,
            resize_anim: None,
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
        .default_font(look.font)
        .window(window::Settings {
            size: initial,
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
        .style(move |_, _| iced::theme::Style {
            background_color: Color::TRANSPARENT,
            text_color: look.fg,
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

    #[test]
    fn animations_ease_and_finish() {
        let a = Anim::new(Duration::from_millis(100));
        assert_eq!(a.at(a.start), 0.0);
        let mid = a.at(a.start + Duration::from_millis(50));
        assert!(mid > 0.5 && mid < 1.0, "{mid}");
        assert_eq!(a.at(a.start + Duration::from_millis(200)), 1.0);
        assert!(a.done(a.start + Duration::from_millis(100)));
        assert!(!a.done(a.start + Duration::from_millis(99)));
    }
}
