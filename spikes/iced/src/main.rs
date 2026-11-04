//! Spike: iced (wgpu por defecto, `--features tiny-skia` para render por CPU) + Mica.
#![windows_subsystem = "windows"]

use std::sync::Mutex;

use iced::futures::channel::mpsc;
use iced::futures::{SinkExt as _, StreamExt as _};
use iced::widget::canvas::{self, Canvas, Frame, Geometry, Path, Text};
use iced::{
    Color, Element, Font, Length, Point, Rectangle, Renderer, Size, Subscription, Task, Theme,
    keyboard, mouse, window,
};
use raw_window_handle::RawWindowHandle;
use spike_common::{self as common, color};

static WAKE_TX: Mutex<Option<mpsc::UnboundedSender<()>>> = Mutex::new(None);
static WAKE_RX: Mutex<Option<mpsc::UnboundedReceiver<()>>> = Mutex::new(None);

#[derive(Debug, Clone)]
enum Message {
    Registered,
    Wake,
    Tick,
    Close,
}

struct Spike;

fn c(rgb: [f32; 3], a: f32) -> Color {
    Color::from_rgba(rgb[0], rgb[1], rgb[2], a)
}

const FONT: Font = Font::with_name("Segoe UI");

impl canvas::Program<Message> for Spike {
    type State = ();

    fn draw(
        &self,
        _state: &(),
        renderer: &Renderer,
        _theme: &Theme,
        bounds: Rectangle,
        _cursor: mouse::Cursor,
    ) -> Vec<Geometry> {
        let a = common::fade_alpha();
        let sel = common::selection_pos();
        let w = common::WIDTH;
        let mut f = Frame::new(renderer, bounds.size());
        let text = |content: &str, x: f32, y: f32, size: f32, col: Color| Text {
            content: content.to_owned(),
            position: Point::new(x, y),
            color: col,
            size: size.into(),
            font: FONT,
            ..Text::default()
        };

        f.fill_text(text("chr", 24.0, 16.0, 20.0, c(color::FG, a)));
        f.fill_rectangle(
            Point::new(58.0, 18.0),
            Size::new(2.0, common::SEARCH_H - 36.0),
            c(color::ACCENT, a),
        );
        f.fill_rectangle(
            Point::new(0.0, common::SEARCH_H),
            Size::new(w, 1.0),
            c(color::MUTED, a * 0.5),
        );

        let top = common::SEARCH_H + 6.0 + sel * common::ROW_H;
        f.fill(
            &Path::rounded_rectangle(
                Point::new(8.0, top),
                Size::new(w - 16.0, common::ROW_H),
                8.0.into(),
            ),
            c(color::SELECTION, a),
        );

        for (i, (title, sub)) in common::ITEMS.iter().enumerate() {
            let y = common::SEARCH_H + 6.0 + i as f32 * common::ROW_H;
            f.fill(
                &Path::rounded_rectangle(
                    Point::new(20.0, y + 10.0),
                    Size::new(28.0, 28.0),
                    6.0.into(),
                ),
                c(color::ACCENT, a),
            );
            f.fill_text(text(title, 62.0, y + 6.0, 15.0, c(color::FG, a)));
            f.fill_text(text(sub, 62.0, y + 27.0, 12.5, c(color::MUTED, a)));
        }
        common::frame_rendered();
        vec![f.into_geometry()]
    }
}

fn boot() -> (Spike, Task<Message>) {
    let register = window::oldest().then(|id| {
        let Some(id) = id else { return Task::none() };
        window::run(id, |w| {
            if let Ok(h) = w.window_handle()
                && let RawWindowHandle::Win32(h) = h.as_raw()
            {
                let hwnd = h.hwnd.get();
                common::apply_backdrop(hwnd, common::backdrop_from_env());
                common::register(hwnd, || {
                    if let Ok(tx) = WAKE_TX.lock()
                        && let Some(tx) = tx.as_ref()
                    {
                        let _ = tx.unbounded_send(());
                    }
                });
            }
        })
        .map(|()| Message::Registered)
    });
    (Spike, register)
}

fn update(_: &mut Spike, msg: Message) -> Task<Message> {
    match msg {
        Message::Close => iced::exit(),
        Message::Registered | Message::Wake | Message::Tick => Task::none(),
    }
}

fn view(state: &Spike) -> Element<'_, Message> {
    Canvas::new(state)
        .width(Length::Fill)
        .height(Length::Fill)
        .into()
}

fn wake_stream() -> impl iced::futures::Stream<Item = Message> {
    iced::stream::channel(8, async |mut out: mpsc::Sender<Message>| {
        let rx = WAKE_RX.lock().ok().and_then(|mut r| r.take());
        if let Some(mut rx) = rx {
            while rx.next().await.is_some() {
                let _ = out.send(Message::Wake).await;
            }
        }
        std::future::pending::<()>().await;
    })
}

fn subscription(_: &Spike) -> Subscription<Message> {
    let mut subs = vec![
        Subscription::run(wake_stream),
        keyboard::listen().filter_map(|e| match e {
            keyboard::Event::KeyPressed {
                key: keyboard::Key::Named(keyboard::key::Named::Escape),
                ..
            } => Some(Message::Close),
            _ => None,
        }),
    ];
    if common::needs_animation() {
        subs.push(window::frames().map(|_| Message::Tick));
    }
    Subscription::batch(subs)
}

fn main() -> iced::Result {
    let (tx, rx) = mpsc::unbounded();
    if let Ok(mut t) = WAKE_TX.lock() {
        *t = Some(tx);
    }
    if let Ok(mut r) = WAKE_RX.lock() {
        *r = Some(rx);
    }
    iced::application(boot, update, view)
        .title("Rukes spike iced")
        .subscription(subscription)
        .default_font(FONT)
        .window(window::Settings {
            size: Size::new(common::WIDTH, common::HEIGHT),
            position: window::Position::Centered,
            decorations: false,
            transparent: true,
            resizable: false,
            level: window::Level::AlwaysOnTop,
            platform_specific: window::settings::PlatformSpecific {
                skip_taskbar: true,
                ..Default::default()
            },
            ..Default::default()
        })
        .style(|_, _| iced::theme::Style {
            background_color: {
                let t = color::TINT;
                Color::from_rgba(t[0], t[1], t[2], t[3] * common::fade_alpha())
            },
            text_color: c(color::FG, 1.0),
        })
        .run()
}
