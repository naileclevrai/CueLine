//! macOS-style sheets (modal dialogs) and System Settings-like form groups.

use eframe::egui::{self, pos2, vec2, Align2, Color32, CornerRadius, Margin, Sense, Stroke, Ui};

use super::{fonts, theme};

const SHEET_FILL: Color32 = Color32::from_rgb(0x26, 0x26, 0x29);
const GROUP_FILL: Color32 = Color32::from_rgb(0x30, 0x30, 0x33);

/// Shows a modal sheet. Clicking outside or pressing Escape closes it.
pub fn show<R>(
    ctx: &egui::Context,
    title: &str,
    open: &mut bool,
    width: f32,
    add: impl FnOnce(&mut Ui) -> R,
) -> Option<R> {
    if !*open {
        return None;
    }
    let frame = egui::Frame::new()
        .fill(SHEET_FILL)
        .stroke(Stroke::new(0.5, Color32::from_white_alpha(30)))
        .corner_radius(CornerRadius::same(14))
        .shadow(egui::Shadow { offset: [0, 22], blur: 60, spread: 0, color: Color32::from_black_alpha(170) })
        .inner_margin(Margin { left: 20, right: 20, top: 0, bottom: 20 });
    let resp = egui::Modal::new(egui::Id::new(("sheet", title)))
        .frame(frame)
        .backdrop_color(Color32::from_black_alpha(110))
        .show(ctx, |ui| {
            ui.set_width(width);
            // Header: close light on the left, centred title.
            let (header, _) = ui.allocate_exact_size(vec2(width, 44.0), Sense::hover());
            let c = pos2(header.left() + 6.0, header.center().y);
            let close =
                ui.interact(egui::Rect::from_center_size(c, vec2(14.0, 14.0)), ui.id().with("close"), Sense::click());
            let p = ui.painter();
            p.circle_filled(c, 6.0, Color32::from_rgb(0xff, 0x5f, 0x57));
            if close.hovered() {
                let g = Color32::from_black_alpha(150);
                p.line_segment([c + vec2(-2.5, -2.5), c + vec2(2.5, 2.5)], Stroke::new(1.2, g));
                p.line_segment([c + vec2(-2.5, 2.5), c + vec2(2.5, -2.5)], Stroke::new(1.2, g));
            }
            p.text(header.center(), Align2::CENTER_CENTER, title, fonts::semibold(13.5), theme::TEXT);
            let r = add(ui);
            (r, close.clicked())
        });
    let should_close = resp.should_close() && !resp.any_popup_open;
    let (inner, closed) = resp.inner;
    if closed || should_close {
        *open = false;
    }
    Some(inner)
}

/// Rounded inset group of form rows.
pub fn group<R>(ui: &mut Ui, add: impl FnOnce(&mut Ui) -> R) -> R {
    egui::Frame::new()
        .fill(GROUP_FILL)
        .stroke(Stroke::new(0.5, Color32::from_white_alpha(14)))
        .corner_radius(CornerRadius::same(10))
        .inner_margin(Margin::symmetric(14, 8))
        .show(ui, |ui| {
            ui.set_width(ui.available_width());
            ui.spacing_mut().item_spacing.y = 6.0;
            add(ui)
        })
        .inner
}

pub fn row_separator(ui: &mut Ui) {
    let (r, _) = ui.allocate_exact_size(vec2(ui.available_width(), 1.0), Sense::hover());
    ui.painter().hline(r.x_range(), r.center().y, Stroke::new(1.0, Color32::from_white_alpha(16)));
}

/// A labelled row with the control right-aligned.
pub fn row<R>(ui: &mut Ui, label: &str, add: impl FnOnce(&mut Ui) -> R) -> R {
    ui.horizontal(|ui| {
        ui.set_min_height(30.0);
        ui.label(egui::RichText::new(label).font(fonts::text(13.0)).color(theme::TEXT));
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), add).inner
    })
    .inner
}

pub fn section(ui: &mut Ui, title: &str) {
    ui.add_space(10.0);
    ui.label(egui::RichText::new(title).font(fonts::semibold(12.5)).color(theme::TEXT));
    ui.add_space(2.0);
}

pub fn footnote(ui: &mut Ui, text: &str) {
    ui.add_space(2.0);
    ui.label(egui::RichText::new(text).font(fonts::text(11.5)).color(theme::TEXT_DIM));
}

/// Primary (blue) push button.
pub fn primary_button(ui: &mut Ui, text: &str, enabled: bool) -> egui::Response {
    let fill = if enabled { theme::BLUE } else { theme::BG_WIDGET };
    let color = if enabled { Color32::WHITE } else { theme::TEXT_FAINT };
    ui.add_enabled(
        enabled,
        egui::Button::new(egui::RichText::new(text).font(fonts::medium(13.0)).color(color))
            .fill(fill)
            .corner_radius(CornerRadius::same(7))
            .min_size(vec2(84.0, 26.0)),
    )
}

/// Secondary push button.
pub fn button(ui: &mut Ui, text: &str) -> egui::Response {
    ui.add(
        egui::Button::new(egui::RichText::new(text).font(fonts::text(13.0)))
            .corner_radius(CornerRadius::same(7))
            .min_size(vec2(84.0, 26.0)),
    )
}
