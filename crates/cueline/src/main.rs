#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod app;
mod audio;
mod devshot;
mod engine;
mod export;
mod history;
mod markers_io;
mod platform;
mod project;
mod settings;
mod ui;

use eframe::egui;

fn main() -> eframe::Result {
    env_logger::Builder::from_env(env_logger::Env::default().default_filter_or("info")).init();
    let _timer = platform::TimerResolution::acquire();

    // Documentation screenshots must not steal focus from the desktop.
    let background = std::env::var_os("CUELINE_SCREENSHOT").is_some();
    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_active(!background)
            .with_decorations(false)
            .with_resizable(true)
            .with_title("CueLine")
            .with_inner_size([1280.0, 760.0])
            .with_min_inner_size([820.0, 480.0])
            .with_drag_and_drop(true)
            .with_icon(egui::IconData {
                rgba: include_bytes!("../../../assets/icon-64.rgba").to_vec(),
                width: 64,
                height: 64,
            }),
        ..Default::default()
    };
    let open = std::env::args_os().nth(1).map(std::path::PathBuf::from);
    eframe::run_native(
        "CueLine",
        options,
        Box::new(move |cc| {
            ui::theme::apply(&cc.egui_ctx);
            Ok(Box::new(app::CueLineApp::new(cc, open)))
        }),
    )
}
