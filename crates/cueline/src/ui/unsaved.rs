//! "Save changes?" sheet, shown before an action would discard edits.

use eframe::egui::{self, pos2, vec2, Align2, Color32, CornerRadius, RichText, Sense};

use super::sheet;
use super::{fonts, theme};
use crate::app::{CueLineApp, Discarding};

pub fn window(app: &mut CueLineApp, ctx: &egui::Context) {
    let Some(action) = app.ui.pending.clone() else { return };
    let name = app.title_parts().0;
    let mut open = true;
    let mut choice = None;
    let mut cancel = false;
    sheet::show(ctx, "Unsaved Changes", &mut open, 420.0, |ui| {
        ui.horizontal(|ui| {
            // Document badge, like the macOS alert icon.
            let (r, _) = ui.allocate_exact_size(vec2(48.0, 56.0), Sense::hover());
            let doc = egui::Rect::from_center_size(r.center(), vec2(40.0, 50.0));
            let p = ui.painter();
            p.rect_filled(doc, CornerRadius::same(7), theme::ORANGE);
            p.text(
                doc.center(),
                Align2::CENTER_CENTER,
                "TC",
                fonts::semibold(14.0),
                Color32::from_rgb(0x1a, 0x12, 0x02),
            );
            p.circle_filled(pos2(doc.right() - 2.0, doc.top() + 4.0), 7.0, theme::YELLOW);
            p.text(
                pos2(doc.right() - 2.0, doc.top() + 4.0),
                Align2::CENTER_CENTER,
                "!",
                fonts::semibold(11.0),
                Color32::BLACK,
            );
            ui.add_space(6.0);
            ui.vertical(|ui| {
                ui.spacing_mut().item_spacing.y = 4.0;
                ui.add(
                    egui::Label::new(
                        RichText::new(format!("Do you want to save the changes you made to “{name}”?"))
                            .font(fonts::semibold(13.5))
                            .color(theme::TEXT),
                    )
                    .wrap(),
                );
                let consequence = match action {
                    Discarding::Quit => "Your changes will be lost if you quit without saving.",
                    _ => "Your changes will be lost if you don't save them.",
                };
                ui.label(RichText::new(consequence).font(fonts::text(12.0)).color(theme::TEXT_DIM));
            });
        });
        ui.add_space(16.0);
        ui.horizontal(|ui| {
            if sheet::button(ui, "Don't Save").clicked() {
                choice = Some(false);
            }
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                if sheet::primary_button(ui, "Save", true).clicked() || ui.input(|i| i.key_pressed(egui::Key::Enter)) {
                    choice = Some(true);
                }
                if sheet::button(ui, "Cancel").clicked() {
                    cancel = true;
                }
            });
        });
    });

    match choice {
        Some(save) => {
            app.ui.pending = None;
            // Saving an untitled show opens the save dialog; cancelling it keeps the show open.
            if !save || super::menus::save(app) {
                app.perform(action);
            }
        }
        None if !open || cancel => app.ui.pending = None,
        None => {}
    }
}
