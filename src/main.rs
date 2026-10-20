// Hide the console window in release builds on Windows.
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

use explorist::app::ExploristApp;

fn main() -> eframe::Result {
    match std::env::args().nth(1).as_deref() {
        Some("--version") => {
            println!("{}", explorist::update::CURRENT);
            return Ok(());
        }
        Some("--update-now") => std::process::exit(explorist::update::update_now()),
        // Opened again while it sits in the tray: show that window instead.
        None if explorist::app::instance::hand_off() => return Ok(()),
        _ => {}
    }
    let viewport = egui::ViewportBuilder::default()
        .with_title("Explorist")
        .with_icon(icon())
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

/// The window and taskbar icon.
fn icon() -> egui::IconData {
    let png = include_bytes!("../assets/icon/icon-256.png");
    match image::load_from_memory_with_format(png, image::ImageFormat::Png) {
        Ok(img) => {
            let rgba = img.to_rgba8();
            egui::IconData {
                width: rgba.width(),
                height: rgba.height(),
                rgba: rgba.into_raw(),
            }
        }
        Err(_) => egui::IconData::default(),
    }
}
