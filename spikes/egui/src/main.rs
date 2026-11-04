//! Spike: eframe/egui (glow por defecto, `--features wgpu` para wgpu) + Mica.
#![windows_subsystem = "windows"]

use eframe::egui::{self, Color32, CornerRadius, FontId, Pos2, Rect, Vec2, pos2, vec2};
use raw_window_handle::{HasWindowHandle, RawWindowHandle};
use spike_common::{self as common, color};

fn c32(c: [f32; 3], a: f32) -> Color32 {
    Color32::from_rgba_unmultiplied(
        (c[0] * 255.0) as u8,
        (c[1] * 255.0) as u8,
        (c[2] * 255.0) as u8,
        (a * 255.0) as u8,
    )
}

struct App;

impl eframe::App for App {
    fn clear_color(&self, _: &egui::Visuals) -> [f32; 4] {
        let a = common::fade_alpha();
        let t = color::TINT;
        c32([t[0], t[1], t[2]], t[3] * a).to_normalized_gamma_f32()
    }

    fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
        let a = common::fade_alpha();
        let sel = common::selection_pos();
        let w = common::WIDTH;
        let p = ui.painter();

        p.text(
            pos2(24.0, common::SEARCH_H / 2.0),
            egui::Align2::LEFT_CENTER,
            "chr",
            FontId::proportional(20.0),
            c32(color::FG, a),
        );
        p.rect_filled(
            Rect::from_min_max(pos2(58.0, 18.0), pos2(60.0, common::SEARCH_H - 18.0)),
            0.0,
            c32(color::ACCENT, a),
        );
        p.rect_filled(
            Rect::from_min_size(pos2(0.0, common::SEARCH_H), vec2(w, 1.0)),
            0.0,
            c32(color::MUTED, a * 0.5),
        );

        let top = common::SEARCH_H + 6.0 + sel * common::ROW_H;
        p.rect_filled(
            Rect::from_min_max(pos2(8.0, top), pos2(w - 8.0, top + common::ROW_H)),
            CornerRadius::same(8),
            c32(color::SELECTION, a),
        );

        for (i, (title, sub)) in common::ITEMS.iter().enumerate() {
            let y = common::SEARCH_H + 6.0 + i as f32 * common::ROW_H;
            p.rect_filled(
                Rect::from_min_size(pos2(20.0, y + 10.0), Vec2::splat(28.0)),
                CornerRadius::same(6),
                c32(color::ACCENT, a),
            );
            p.text(
                Pos2::new(62.0, y + 6.0),
                egui::Align2::LEFT_TOP,
                *title,
                FontId::proportional(15.0),
                c32(color::FG, a),
            );
            p.text(
                Pos2::new(62.0, y + 27.0),
                egui::Align2::LEFT_TOP,
                *sub,
                FontId::proportional(12.5),
                c32(color::MUTED, a),
            );
        }

        if ui.input(|i| i.key_pressed(egui::Key::Escape)) {
            ui.ctx().send_viewport_cmd(egui::ViewportCommand::Close);
        }
        common::frame_rendered();
        if common::needs_animation() {
            ui.ctx().request_repaint();
        }
    }
}

fn load_segoe(ctx: &egui::Context) {
    let Ok(bytes) = std::fs::read("C:\\Windows\\Fonts\\segoeui.ttf") else {
        return;
    };
    let mut fonts = egui::FontDefinitions::default();
    fonts.font_data.insert(
        "segoe".into(),
        std::sync::Arc::new(egui::FontData::from_owned(bytes)),
    );
    if let Some(f) = fonts.families.get_mut(&egui::FontFamily::Proportional) {
        f.insert(0, "segoe".into());
    }
    ctx.set_fonts(fonts);
}

fn main() -> eframe::Result {
    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_title("Rukes spike egui")
            .with_inner_size([common::WIDTH, common::HEIGHT])
            .with_decorations(false)
            .with_transparent(true)
            .with_always_on_top()
            .with_taskbar(false)
            .with_resizable(false),
        centered: true,
        #[cfg(feature = "wgpu")]
        renderer: eframe::Renderer::Wgpu,
        #[cfg(not(feature = "wgpu"))]
        renderer: eframe::Renderer::Glow,
        ..Default::default()
    };
    eframe::run_native(
        "rukes-spike-egui",
        options,
        Box::new(|cc| {
            load_segoe(&cc.egui_ctx);
            if let Ok(h) = cc.window_handle()
                && let RawWindowHandle::Win32(h) = h.as_raw()
            {
                let hwnd = h.hwnd.get();
                common::apply_backdrop(hwnd, common::backdrop_from_env());
                let ctx = cc.egui_ctx.clone();
                common::register(hwnd, move || ctx.request_repaint());
            }
            Ok(Box::new(App))
        }),
    )
}
