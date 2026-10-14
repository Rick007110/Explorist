// Hide the console window in release builds on Windows.
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

use explorist::app::ExploristApp;

fn main() -> eframe::Result {
    let viewport = egui::ViewportBuilder::default()
        .with_title("Explorist")
        .with_inner_size([1440.0, 900.0])
        .with_min_inner_size([1280.0, 720.0]);
    // On macOS the native traffic lights sit over our tab bar; elsewhere the
    // app draws its own title bar and window buttons.
    let viewport = if cfg!(target_os = "macos") {
        viewport
            .with_fullsize_content_view(true)
            .with_titlebar_shown(false)
            .with_title_shown(false)
    } else {
        viewport.with_decorations(false)
    };
    let options = eframe::NativeOptions {
        viewport,
        ..Default::default()
    };

    eframe::run_native(
        "Explorist",
        options,
        Box::new(|cc| Ok(Box::new(ExploristApp::new(cc)))),
    )
}
