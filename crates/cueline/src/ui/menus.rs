//! Menu bar, context menus and file dialogs.

use eframe::egui::{self, Ui};

use super::timeline::Map;
use crate::app::CueLineApp;
use crate::project::EXTENSION;

pub const AUDIO_EXTENSIONS: [&str; 10] = ["wav", "wave", "aif", "aiff", "flac", "mp3", "m4a", "aac", "ogg", "caf"];

pub fn import_dialog(app: &mut CueLineApp) {
    if let Some(files) = rfd::FileDialog::new()
        .set_title("Import audio")
        .add_filter("Audio", &AUDIO_EXTENSIONS)
        .add_filter("All files", &["*"])
        .pick_files()
    {
        app.import_files(files);
    }
}

pub fn open_dialog(app: &mut CueLineApp) {
    if !app.confirm_discard() {
        return;
    }
    if let Some(path) =
        rfd::FileDialog::new().set_title("Open project").add_filter("CueLine project", &[EXTENSION]).pick_file()
    {
        app.open_project(&path);
    }
}

pub fn save(app: &mut CueLineApp) -> bool {
    match app.project_path.clone() {
        Some(p) => app.save_project_to(&p),
        None => save_as(app),
    }
}

pub fn save_as(app: &mut CueLineApp) -> bool {
    let name = app
        .project_path
        .as_ref()
        .and_then(|p| p.file_name())
        .map_or("Untitled.cueline".into(), |n| n.to_string_lossy().into_owned());
    match rfd::FileDialog::new()
        .set_title("Save project")
        .set_file_name(name)
        .add_filter("CueLine project", &[EXTENSION])
        .save_file()
    {
        Some(mut path) => {
            if path.extension().is_none() {
                path.set_extension(EXTENSION);
            }
            app.save_project_to(&path)
        }
        None => false,
    }
}

fn shortcut(ui: &mut Ui, label: &str, keys: &str) -> egui::Response {
    ui.add(egui::Button::new(label).shortcut_text(keys))
}

pub fn menu_bar(app: &mut CueLineApp, ui: &mut Ui) {
    egui::MenuBar::new().ui(ui, |ui| {
        ui.menu_button("File", |ui| {
            if shortcut(ui, "New project", "Ctrl+N").clicked() && app.confirm_discard() {
                app.new_project();
            }
            if shortcut(ui, "Open…", "Ctrl+O").clicked() {
                open_dialog(app);
            }
            ui.menu_button("Open recent", |ui| {
                let recent = app.settings.recent.clone();
                if recent.is_empty() {
                    ui.weak("No recent projects");
                }
                for p in recent {
                    if ui.button(p.display().to_string()).clicked() && app.confirm_discard() {
                        app.open_project(&p);
                        ui.close();
                    }
                }
            });
            ui.separator();
            if shortcut(ui, "Save", "Ctrl+S").clicked() {
                save(app);
            }
            if shortcut(ui, "Save as…", "Ctrl+Shift+S").clicked() {
                save_as(app);
            }
            ui.separator();
            if shortcut(ui, "Import audio…", "Ctrl+I").clicked() {
                import_dialog(app);
            }
            if shortcut(ui, "Export WAV…", "Ctrl+E").clicked() {
                app.ui.export.open = true;
            }
            ui.separator();
            if ui.button("Quit").clicked() {
                ui.ctx().send_viewport_cmd(egui::ViewportCommand::Close);
            }
        });
        ui.menu_button("Edit", |ui| {
            if ui.add_enabled(app.history.can_undo(), egui::Button::new("Undo").shortcut_text("Ctrl+Z")).clicked() {
                app.undo();
            }
            if ui.add_enabled(app.history.can_redo(), egui::Button::new("Redo").shortcut_text("Ctrl+Y")).clicked() {
                app.redo();
            }
            ui.separator();
            if shortcut(ui, "Add marker at playhead", "M").clicked() {
                app.add_marker_at(app.position_secs());
            }
            let sel = app.view.selected_track;
            if ui.add_enabled(sel.is_some(), egui::Button::new("Remove selected track").shortcut_text("Del")).clicked()
            {
                app.remove_track(sel.unwrap());
            }
            ui.separator();
            ui.checkbox(&mut app.settings.snap_to_frames, "Snap to frames (hold Shift to bypass)");
        });
        ui.menu_button("Transport", |ui| {
            if shortcut(ui, "Play / Stop", "Space").clicked() {
                app.toggle_play();
            }
            if shortcut(ui, "Pause", "Shift+Space").clicked() {
                app.pause();
            }
            if shortcut(ui, "Go to start", "Home").clicked() {
                app.seek(0.0);
            }
            if shortcut(ui, "Go to end", "End").clicked() {
                app.seek(app.project_end_secs());
            }
            if shortcut(ui, "Go to timecode…", "G").clicked() {
                app.ui.goto_text = Some(app.timecode_at(app.position_secs()).to_string());
            }
            ui.separator();
            if shortcut(ui, "Previous marker", "[").clicked() {
                app.goto_adjacent_marker(false);
            }
            if shortcut(ui, "Next marker", "]").clicked() {
                app.goto_adjacent_marker(true);
            }
        });
        ui.menu_button("View", |ui| {
            if shortcut(ui, "Zoom in", "Ctrl+Wheel / +").clicked() {
                app.zoom_by(1.5);
            }
            if shortcut(ui, "Zoom out", "Ctrl+Wheel / −").clicked() {
                app.zoom_by(1.0 / 1.5);
            }
            if shortcut(ui, "Zoom to project", "Z").clicked() {
                app.ui.zoom_to_fit = true;
            }
            ui.separator();
            ui.checkbox(&mut app.settings.follow_playhead, "Follow playhead (F)");
            ui.checkbox(&mut app.ui.show_markers, "Markers panel");
            ui.add(egui::Slider::new(&mut app.view.track_height, 44.0..=200.0).text("Track height"));
        });
        ui.menu_button("Options", |ui| {
            if shortcut(ui, "Preferences…", "Ctrl+,").clicked() {
                app.ui.prefs.open = true;
            }
        });
        ui.menu_button("Help", |ui| {
            if ui.button("Keyboard shortcuts").clicked() {
                app.ui.show_help = true;
            }
            ui.hyperlink_to("CueLine on GitHub", "https://github.com/naileclevrai/CueLine");
            ui.separator();
            ui.weak(format!("CueLine {}", env!("CARGO_PKG_VERSION")));
        });
    });
}

pub fn track_context_menu(app: &mut CueLineApp, ui: &mut Ui) {
    let Some(id) = app.view.context_track else {
        if ui.button("Import audio…").clicked() {
            import_dialog(app);
            ui.close();
        }
        return;
    };
    if ui.button("Move clip start to cursor").clicked() {
        app.checkpoint();
        let at = app.cursor_secs;
        if let Some(t) = app.tracks.iter_mut().find(|t| t.id == id) {
            t.def.offset_secs = at;
        }
        app.track_changed(id);
        ui.close();
    }
    if ui.button("Move clip start to project start").clicked() {
        app.checkpoint();
        if let Some(t) = app.tracks.iter_mut().find(|t| t.id == id) {
            t.def.offset_secs = 0.0;
        }
        app.track_changed(id);
        ui.close();
    }
    if ui.button("Rename").clicked() {
        if let Some(t) = app.tracks.iter().find(|t| t.id == id) {
            app.ui.rename_track = Some((id, t.def.name.clone()));
        }
        ui.close();
    }
    ui.menu_button("Colour", |ui| {
        for c in super::theme::TRACK_COLORS {
            let (r, resp) = ui.allocate_exact_size(egui::vec2(60.0, 16.0), egui::Sense::click());
            ui.painter().rect_filled(r, 2.0, super::theme::rgb(c));
            if resp.clicked() {
                app.checkpoint();
                if let Some(t) = app.tracks.iter_mut().find(|t| t.id == id) {
                    t.def.color = c;
                }
                app.dirty = true;
                ui.close();
            }
        }
    });
    ui.separator();
    if ui.button("Remove track").clicked() {
        app.remove_track(id);
        ui.close();
    }
}

pub fn ruler_context_menu(app: &mut CueLineApp, ui: &mut Ui, _map: Map) {
    if let Some(idx) = app.view.context_marker {
        if ui.button("Rename marker").clicked() {
            if let Some(m) = app.project.markers.get(idx) {
                app.ui.rename_marker = Some((idx, m.name.clone()));
            }
            ui.close();
        }
        if ui.button("Delete marker").clicked() {
            app.remove_marker(idx);
            ui.close();
        }
        return;
    }
    if ui.button("Add marker here").clicked() {
        app.add_marker_at(app.view.context_time);
        ui.close();
    }
    if ui.button("Add marker at cursor").clicked() {
        app.add_marker_at(app.cursor_secs);
        ui.close();
    }
}
