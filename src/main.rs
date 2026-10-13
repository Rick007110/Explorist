// Hide the console window in release builds on Windows.
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

use explorist::app::ExploristApp;

fn main() -> eframe::Result {
    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_title("Explorist")
            .with_inner_size([1440.0, 900.0])
            .with_min_inner_size([1280.0, 720.0])
            .with_decorations(false),
        ..Default::default()
    };

    eframe::run_native(
        "Explorist",
        options,
        Box::new(|cc| Ok(Box::new(ExploristApp::new(cc)))),
    )
}
