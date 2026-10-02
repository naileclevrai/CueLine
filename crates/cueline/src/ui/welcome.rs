//! Launch screen: create a show, open one, or pick a recent project.

use std::path::PathBuf;

use eframe::egui::{self, pos2, vec2, Align2, Color32, CornerRadius, Rect, Sense, Stroke, StrokeKind, Ui};

use super::widgets::{paint_icon, Icon};
use super::{fonts, menus, theme};
use crate::app::CueLineApp;

fn logo(app: &mut CueLineApp, ctx: &egui::Context) -> egui::TextureHandle {
    app.ui
        .logo
        .get_or_insert_with(|| {
            let img = egui::ColorImage::from_rgba_unmultiplied(
                [128, 128],
                include_bytes!("../../../../assets/icon-128.rgba"),
            );
            ctx.load_texture("cueline-logo", img, egui::TextureOptions::LINEAR)
        })
        .clone()
}

/// A large list-style action button (icon, title, subtitle).
fn action(ui: &mut Ui, icon: Icon, tint: Color32, title: &str, subtitle: &str, shortcut: &str) -> egui::Response {
    let (rect, resp) = ui.allocate_exact_size(vec2(ui.available_width(), 52.0), Sense::click());
    let p = ui.painter();
    if resp.hovered() {
        p.rect_filled(rect, CornerRadius::same(10), Color32::from_white_alpha(10));
    }
    let badge = Rect::from_center_size(pos2(rect.left() + 26.0, rect.center().y), vec2(34.0, 34.0));
    p.rect_filled(badge, CornerRadius::same(9), tint.gamma_multiply(0.22));
    paint_icon(p, icon, badge.center(), 12.0, tint);
    p.text(
        pos2(rect.left() + 54.0, rect.center().y - 8.0),
        Align2::LEFT_CENTER,
        title,
        fonts::medium(13.5),
        theme::TEXT,
    );
    let sub_clip = Rect::from_min_max(rect.min, pos2(rect.right() - 58.0, rect.bottom()));
    p.with_clip_rect(sub_clip).text(
        pos2(rect.left() + 54.0, rect.center().y + 9.0),
        Align2::LEFT_CENTER,
        subtitle,
        fonts::text(11.5),
        theme::TEXT_DIM,
    );
    p.text(
        pos2(rect.right() - 10.0, rect.center().y),
        Align2::RIGHT_CENTER,
        shortcut,
        fonts::text(11.5),
        theme::TEXT_FAINT,
    );
    resp
}

/// Shows paths under the user's home folder as `~\...`.
fn short_path(p: &std::path::Path) -> String {
    let full = p.display().to_string();
    match std::env::var("USERPROFILE") {
        Ok(home) if full.starts_with(&home) => format!("~{}", &full[home.len()..]),
        _ => full,
    }
}

pub fn draw(app: &mut CueLineApp, ui: &mut Ui) {
    let ctx = ui.ctx().clone();
    let full = ui.max_rect();
    ui.painter().rect_filled(full, CornerRadius::ZERO, theme::BG_CONTENT);

    let card_size = vec2(820.0_f32.min(full.width() - 40.0), 480.0_f32.min(full.height() - 60.0));
    let card = Rect::from_center_size(full.center() - vec2(0.0, 10.0), card_size);
    let p = ui.painter();
    p.add(
        egui::Shadow { offset: [0, 20], blur: 60, spread: 0, color: Color32::from_black_alpha(140) }
            .as_shape(card, CornerRadius::same(16)),
    );
    p.rect_filled(card, CornerRadius::same(16), Color32::from_rgb(0x22, 0x22, 0x25));
    let left = Rect::from_min_size(card.min, vec2(330.0, card.height()));
    p.rect_filled(left, CornerRadius { nw: 16, sw: 16, ne: 0, se: 0 }, Color32::from_rgb(0x2a, 0x2a, 0x2e));
    p.vline(left.right(), card.y_range(), Stroke::new(1.0, theme::HAIRLINE));
    p.rect_stroke(card, CornerRadius::same(16), Stroke::new(0.5, Color32::from_white_alpha(26)), StrokeKind::Inside);

    // Left: identity and actions.
    let tex = logo(app, &ctx);
    let mut l = ui.new_child(egui::UiBuilder::new().max_rect(left.shrink2(vec2(22.0, 26.0))));
    l.vertical_centered(|ui| {
        ui.add(egui::Image::new(&tex).fit_to_exact_size(vec2(84.0, 84.0)));
        ui.add_space(6.0);
        ui.label(egui::RichText::new("CueLine").font(fonts::display_light(30.0)).color(theme::TEXT));
        ui.label(
            egui::RichText::new(format!("Version {}", env!("CARGO_PKG_VERSION")))
                .font(fonts::text(12.0))
                .color(theme::TEXT_FAINT),
        );
    });
    l.add_space(22.0);
    let outputs = app.output_channels();
    if action(&mut l, Icon::Plus, theme::BLUE, "New Show…", "Timecode, outputs & MIDI", "Ctrl+N").clicked() {
        app.ui.new_show.begin(outputs);
    }
    if action(&mut l, Icon::Folder, theme::ORANGE, "Open Show…", "Open a .cueline project", "Ctrl+O").clicked() {
        menus::open_dialog(app);
    }
    if action(&mut l, Icon::Gear, theme::TEXT_DIM, "Audio & MIDI Settings…", "Choose your interface", "Ctrl+,")
        .clicked()
    {
        app.ui.prefs.open = true;
    }

    // Device status at the bottom of the left pane.
    let status = match &app.engine {
        Some(e) => (
            theme::GREEN,
            format!("{} · {:.1} kHz · {} outputs", e.device_name, e.sample_rate as f32 / 1000.0, e.channels),
        ),
        None => (theme::RED, "No audio device".to_string()),
    };
    let sy = left.bottom() - 22.0;
    let p = ui.painter();
    p.circle_filled(pos2(left.left() + 26.0, sy), 3.5, status.0);
    p.with_clip_rect(left.shrink(8.0)).text(
        pos2(left.left() + 36.0, sy),
        Align2::LEFT_CENTER,
        status.1,
        fonts::text(11.0),
        theme::TEXT_DIM,
    );

    // Right: recent shows.
    let right = Rect::from_min_max(pos2(left.right(), card.top()), card.max).shrink2(vec2(18.0, 20.0));
    let mut r = ui.new_child(egui::UiBuilder::new().max_rect(right));
    r.label(egui::RichText::new("Recent Shows").font(fonts::semibold(13.0)).color(theme::TEXT));
    r.add_space(8.0);
    let recent: Vec<PathBuf> = app.settings.recent.clone();
    if recent.is_empty() {
        r.add_space(60.0);
        r.vertical_centered(|ui| {
            ui.label(egui::RichText::new("No recent shows").font(fonts::semibold(14.0)).color(theme::TEXT_DIM));
            ui.add_space(4.0);
            ui.label(
                egui::RichText::new("Create a show, or drop a .cueline file here.")
                    .font(fonts::text(12.0))
                    .color(theme::TEXT_FAINT),
            );
        });
    }
    let mut forget = None;
    egui::ScrollArea::vertical().auto_shrink([false, false]).show(&mut r, |ui| {
        for path in recent {
            let exists = path.exists();
            let (rect, resp) = ui.allocate_exact_size(vec2(ui.available_width(), 50.0), Sense::click());
            let p = ui.painter();
            if resp.hovered() {
                p.rect_filled(rect, CornerRadius::same(9), Color32::from_white_alpha(10));
            }
            let doc = Rect::from_center_size(pos2(rect.left() + 22.0, rect.center().y), vec2(26.0, 32.0));
            p.rect_filled(
                doc,
                CornerRadius::same(5),
                if exists { theme::ORANGE.gamma_multiply(0.85) } else { theme::BG_WIDGET },
            );
            p.text(
                doc.center(),
                Align2::CENTER_CENTER,
                "TC",
                fonts::semibold(9.5),
                Color32::from_rgb(0x1a, 0x12, 0x02),
            );
            let name = path.file_stem().map_or(String::new(), |s| s.to_string_lossy().into_owned());
            let folder = path.parent().map_or(String::new(), short_path);
            let clip =
                Rect::from_min_max(pos2(rect.left() + 44.0, rect.top()), pos2(rect.right() - 8.0, rect.bottom()));
            let pc = p.with_clip_rect(clip);
            pc.text(
                pos2(clip.left(), rect.center().y - 8.0),
                Align2::LEFT_CENTER,
                &name,
                fonts::medium(13.0),
                if exists { theme::TEXT } else { theme::TEXT_FAINT },
            );
            let sub = if exists { folder } else { format!("Missing — {folder}") };
            pc.text(
                pos2(clip.left(), rect.center().y + 9.0),
                Align2::LEFT_CENTER,
                sub,
                fonts::text(11.0),
                theme::TEXT_FAINT,
            );
            if resp.clicked() && exists {
                app.open_project(&path);
            }
            resp.context_menu(|ui| {
                if ui.add_enabled(exists, egui::Button::new("Open")).clicked() {
                    app.open_project(&path);
                    ui.close();
                }
                if ui.button("Remove from list").clicked() {
                    forget = Some(path.clone());
                    ui.close();
                }
            });
        }
    });
    if let Some(p) = forget {
        app.settings.recent.retain(|r| *r != p);
        app.settings.save();
    }

    let hint_y = card.bottom() + 22.0;
    if hint_y < full.bottom() - 8.0 {
        ui.painter().text(
            pos2(full.center().x, hint_y),
            Align2::CENTER_CENTER,
            "Tip: drop audio files on this window to start a quick untitled show.",
            fonts::text(11.5),
            theme::TEXT_FAINT,
        );
    }
}
