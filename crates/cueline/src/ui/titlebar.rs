//! Custom window chrome: the window is frameless and draws a macOS-style
//! title bar (traffic lights, menus, centred document title), handles window
//! dragging and resizing from its edges.

use eframe::egui::{self, pos2, Align2, CursorIcon, ResizeDirection, Sense, ViewportCommand};

use super::widgets::{traffic_lights, WindowAction};
use super::{fonts, menus, theme};
use crate::app::CueLineApp;

pub const HEIGHT: f32 = 34.0;
const EDGE: f32 = 5.0;

pub fn draw(app: &mut CueLineApp, ui: &mut egui::Ui) {
    let rect = ui.max_rect();
    let ctx = ui.ctx().clone();
    let maximized = ctx.input(|i| i.viewport().maximized.unwrap_or(false));

    // Background first so every widget drawn later sits on top of the drag area.
    let bg = ui.interact(rect, ui.id().with("titlebar"), Sense::click_and_drag());
    if bg.double_clicked() {
        ctx.send_viewport_cmd(ViewportCommand::Maximized(!maximized));
    } else if bg.drag_started_by(egui::PointerButton::Primary) {
        ctx.send_viewport_cmd(ViewportCommand::StartDrag);
    }

    let title =
        if app.ui.screen == super::Screen::Welcome { ("CueLine".to_string(), false) } else { app.title_parts() };
    let p = ui.painter();
    let name_font = fonts::semibold(13.0);
    let name_w = p.layout_no_wrap(title.0.clone(), name_font.clone(), theme::TEXT).size().x;
    let center = rect.center();
    if title.1 {
        p.text(pos2(center.x - name_w / 2.0, center.y), Align2::LEFT_CENTER, &title.0, name_font, theme::TEXT);
        p.text(
            pos2(center.x + name_w / 2.0 + 6.0, center.y),
            Align2::LEFT_CENTER,
            "— Edited",
            fonts::text(13.0),
            theme::TEXT_FAINT,
        );
    } else {
        p.text(center, Align2::CENTER_CENTER, &title.0, name_font, theme::TEXT);
    }

    ui.horizontal_centered(|ui| {
        ui.add_space(14.0);
        match traffic_lights(ui) {
            Some(WindowAction::Close) => ctx.send_viewport_cmd(ViewportCommand::Close),
            Some(WindowAction::Minimize) => ctx.send_viewport_cmd(ViewportCommand::Minimized(true)),
            Some(WindowAction::Zoom) => ctx.send_viewport_cmd(ViewportCommand::Maximized(!maximized)),
            None => {}
        }
        ui.add_space(18.0);
        menus::menu_bar(app, ui);
    });
}

/// Lets the user resize the frameless window from its borders.
pub fn resize_edges(ctx: &egui::Context) {
    let (maximized, rect, pos, pressed) = ctx.input(|i| {
        (i.viewport().maximized.unwrap_or(false), i.content_rect(), i.pointer.hover_pos(), i.pointer.primary_pressed())
    });
    let Some(pos) = pos else { return };
    if maximized {
        return;
    }
    let (l, r) = (pos.x - rect.left() < EDGE, rect.right() - pos.x < EDGE);
    let (t, b) = (pos.y - rect.top() < EDGE, rect.bottom() - pos.y < EDGE);
    let dir = match (l, r, t, b) {
        (true, _, true, _) => Some((ResizeDirection::NorthWest, CursorIcon::ResizeNorthWest)),
        (_, true, true, _) => Some((ResizeDirection::NorthEast, CursorIcon::ResizeNorthEast)),
        (true, _, _, true) => Some((ResizeDirection::SouthWest, CursorIcon::ResizeSouthWest)),
        (_, true, _, true) => Some((ResizeDirection::SouthEast, CursorIcon::ResizeSouthEast)),
        (true, ..) => Some((ResizeDirection::West, CursorIcon::ResizeWest)),
        (_, true, ..) => Some((ResizeDirection::East, CursorIcon::ResizeEast)),
        (_, _, true, _) => Some((ResizeDirection::North, CursorIcon::ResizeNorth)),
        (_, _, _, true) => Some((ResizeDirection::South, CursorIcon::ResizeSouth)),
        _ => None,
    };
    if let Some((dir, cursor)) = dir {
        ctx.set_cursor_icon(cursor);
        if pressed {
            ctx.send_viewport_cmd(ViewportCommand::BeginResize(dir));
        }
    }
}
