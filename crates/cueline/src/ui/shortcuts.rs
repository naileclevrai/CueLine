//! Global keyboard shortcuts.

use eframe::egui::{self, Key, KeyboardShortcut, Modifiers};

use super::menus;
use crate::app::CueLineApp;

pub const HELP: &[(&str, &str)] = &[
    ("Space", "Play / stop (returns to start position)"),
    ("Shift+Space", "Pause at current position"),
    ("Home / End", "Go to project start / end"),
    ("← / →", "Nudge one frame (Shift: one second)"),
    ("G", "Go to timecode"),
    ("M", "Add marker at playhead"),
    ("[ / ]", "Previous / next marker"),
    ("1 … 9", "Jump to marker 1 … 9"),
    ("F", "Toggle follow playhead"),
    ("Z", "Zoom to project"),
    ("+ / −, Ctrl+Wheel", "Zoom in / out"),
    ("Shift+Wheel", "Scroll horizontally"),
    ("Delete", "Remove selected track / marker"),
    ("Ctrl+Z / Ctrl+Y", "Undo / redo"),
    ("Ctrl+N / O / S", "New / open / save project"),
    ("Ctrl+Shift+S", "Save as"),
    ("Ctrl+I", "Import audio"),
    ("Ctrl+E", "Export WAV"),
    ("Ctrl+,", "Preferences"),
    ("Shift (while dragging)", "Bypass frame snapping"),
];

fn cmd(key: Key) -> KeyboardShortcut {
    KeyboardShortcut::new(Modifiers::COMMAND, key)
}

pub fn handle(app: &mut CueLineApp, ctx: &egui::Context) {
    // Shortcuts that work even while typing.
    let consume = |s: KeyboardShortcut| ctx.input_mut(|i| i.consume_shortcut(&s));
    if consume(KeyboardShortcut::new(Modifiers::COMMAND | Modifiers::SHIFT, Key::S)) {
        menus::save_as(app);
    }
    if consume(cmd(Key::S)) {
        menus::save(app);
    }
    if consume(cmd(Key::O)) {
        menus::open_dialog(app);
    }
    if consume(cmd(Key::N)) && app.confirm_discard() {
        app.new_project();
    }
    if consume(cmd(Key::I)) {
        menus::import_dialog(app);
    }
    if consume(cmd(Key::E)) {
        app.ui.export.open = true;
    }
    if consume(cmd(Key::Comma)) {
        app.ui.prefs.open = true;
    }

    if ctx.egui_wants_keyboard_input() {
        return;
    }
    if consume(KeyboardShortcut::new(Modifiers::COMMAND | Modifiers::SHIFT, Key::Z)) || consume(cmd(Key::Y)) {
        app.redo();
    }
    if consume(cmd(Key::Z)) {
        app.undo();
    }

    let (pressed, mods) = ctx.input(|i| {
        let keys: Vec<Key> = i
            .events
            .iter()
            .filter_map(|e| match e {
                egui::Event::Key { key, pressed: true, repeat, .. }
                    if !*repeat || matches!(key, Key::ArrowLeft | Key::ArrowRight) =>
                {
                    Some(*key)
                }
                _ => None,
            })
            .collect();
        (keys, i.modifiers)
    });
    if mods.command || mods.alt {
        return;
    }
    let fps = app.project.frame_rate.fps();
    for key in pressed {
        match key {
            Key::Space if mods.shift => {
                if app.is_playing() {
                    app.pause();
                } else {
                    app.play();
                }
            }
            Key::Space => app.toggle_play(),
            Key::Home => app.seek(0.0),
            Key::End => app.seek(app.project_end_secs()),
            Key::ArrowLeft | Key::ArrowRight => {
                let dir = if key == Key::ArrowLeft { -1.0 } else { 1.0 };
                let step = if mods.shift { 1.0 } else { 1.0 / fps };
                let t = super::timeline::snap_to_frame(app.position_secs(), app.project.frame_rate) + dir * step;
                app.seek(t);
            }
            Key::G => app.ui.goto_text = Some(app.timecode_at(app.position_secs()).to_string()),
            Key::M => app.add_marker_at(app.position_secs()),
            Key::OpenBracket => app.goto_adjacent_marker(false),
            Key::CloseBracket => app.goto_adjacent_marker(true),
            Key::F => app.settings.follow_playhead = !app.settings.follow_playhead,
            Key::Z => app.ui.zoom_to_fit = true,
            Key::Plus | Key::Equals => app.zoom_by(1.5),
            Key::Minus => app.zoom_by(1.0 / 1.5),
            Key::Delete | Key::Backspace => {
                if let Some(idx) = app.view.selected_marker {
                    app.remove_marker(idx);
                } else if let Some(id) = app.view.selected_track {
                    app.remove_track(id);
                }
            }
            Key::Escape => {
                app.view.selected_marker = None;
                app.view.selected_track = None;
            }
            k => {
                let digits =
                    [Key::Num1, Key::Num2, Key::Num3, Key::Num4, Key::Num5, Key::Num6, Key::Num7, Key::Num8, Key::Num9];
                if let Some(n) = digits.iter().position(|d| *d == k) {
                    app.goto_marker(n);
                }
            }
        }
    }
}
