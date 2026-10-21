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
        None if explorist::app::instance::hand_off(explorist::app::instance::Ask::Show) => {
            return Ok(());
        }
        // A desktop shortcut for quick search (where apps can't set one up
        // themselves, like on Wayland): ask the running Explorist, or start
        // with the box open.
        Some("--search") => {
            if explorist::app::instance::hand_off(explorist::app::instance::Ask::Search) {
                return Ok(());
            }
            explorist::ui::quick_search::open_soon();
        }
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
    run(viewport)
}

/// Opens the window and runs Explorist until it quits.
#[cfg(not(windows))]
fn run(viewport: egui::ViewportBuilder) -> eframe::Result {
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

/// Opens the window and runs Explorist until it quits. It draws with OpenGL,
/// but PCs without a graphics driver, and some virtual machines and Remote
/// Desktop sessions, only have OpenGL 1.1, which is too old. There it draws
/// with Direct3D 12 instead, which Windows runs in software when it has to.
/// A file in the cache remembers which one worked, so later starts try that
/// one first.
#[cfg(windows)]
fn run(viewport: egui::ViewportBuilder) -> eframe::Result {
    use eframe::Renderer;
    use std::sync::atomic::{AtomicBool, Ordering};

    // Set once the window and its graphics are up and the app is made, so
    // an error after that isn't taken for missing graphics.
    static STARTED: AtomicBool = AtomicBool::new(false);
    let marker = dirs::cache_dir().map(|d| d.join("Explorist").join("use-direct3d"));
    let direct3d_before = marker.as_ref().is_some_and(|m| m.exists());
    let order = if direct3d_before {
        [Renderer::Wgpu, Renderer::Glow]
    } else {
        [Renderer::Glow, Renderer::Wgpu]
    };
    let mut details = String::new();
    let mut last = None;
    for renderer in order {
        let options = eframe::NativeOptions {
            viewport: viewport.clone(),
            renderer,
            ..Default::default()
        };
        let marker = marker.as_deref();
        let result = eframe::run_native(
            "Explorist",
            options,
            Box::new(move |cc| {
                STARTED.store(true, Ordering::Relaxed);
                let direct3d = renderer == Renderer::Wgpu;
                if let Some(marker) = marker
                    && direct3d != direct3d_before
                {
                    let _ = if direct3d {
                        marker
                            .parent()
                            .map_or(Ok(()), std::fs::create_dir_all)
                            .and_then(|()| std::fs::write(marker, b""))
                    } else {
                        std::fs::remove_file(marker)
                    };
                }
                Ok(Box::new(ExploristApp::new(cc)))
            }),
        );
        match result {
            Err(err) if !STARTED.load(Ordering::Relaxed) => {
                let name = match renderer {
                    Renderer::Glow => "OpenGL",
                    Renderer::Wgpu => "Direct3D 12",
                };
                details.push_str(&format!("\n{name}: {err}"));
                last = Some(err);
            }
            result => return result,
        }
    }
    graphics_too_old(&details);
    last.map_or(Ok(()), Err)
}

/// Says why Explorist can't open: there's no console to show the error in.
#[cfg(windows)]
fn graphics_too_old(details: &str) {
    use windows_sys::Win32::UI::WindowsAndMessaging::{MB_ICONERROR, MB_OK, MessageBoxW};

    let wide = |text: &str| text.encode_utf16().chain([0]).collect::<Vec<u16>>();
    let text = wide(&format!(
        "Explorist can't open because this PC's graphics can't draw it: it needs \
         OpenGL 2.0 or Direct3D 12.\n\nInstalling or updating the graphics driver \
         usually fixes this.\n{details}"
    ));
    let title = wide("Explorist");
    unsafe {
        MessageBoxW(
            std::ptr::null_mut(),
            text.as_ptr(),
            title.as_ptr(),
            MB_OK | MB_ICONERROR,
        );
    }
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
