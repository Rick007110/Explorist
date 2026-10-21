//! Close to tray (Settings → General): closing the window hides it, and
//! an icon in the notification area (the menu bar on macOS) brings it
//! back or quits.

use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};

use tray_icon::menu::{Menu, MenuEvent, MenuItem, PredefinedMenuItem};
use tray_icon::{MouseButton, MouseButtonState, TrayIcon, TrayIconBuilder, TrayIconEvent};

const OPEN: &str = "open";
const QUIT: &str = "quit";

pub struct Tray {
    _icon: TrayIcon,
    show: Arc<AtomicBool>,
    quit: Arc<AtomicBool>,
}

impl Tray {
    /// Puts the icon up. `None` when the desktop has nowhere to show it.
    pub fn new(ctx: &egui::Context) -> Option<Self> {
        if !available() {
            return None;
        }
        let menu = Menu::new();
        menu.append_items(&[
            &MenuItem::with_id(OPEN, "Open Explorist", true, None),
            &PredefinedMenuItem::separator(),
            &MenuItem::with_id(QUIT, "Quit Explorist", true, None),
        ])
        .ok()?;
        let icon = TrayIconBuilder::new()
            .with_icon(icon()?)
            .with_tooltip("Explorist")
            .with_menu(Box::new(menu))
            .with_menu_on_left_click(false)
            .build()
            .ok()?;

        let show = Arc::new(AtomicBool::new(false));
        let quit = Arc::new(AtomicBool::new(false));
        {
            let (show, ctx) = (Arc::clone(&show), ctx.clone());
            TrayIconEvent::set_event_handler(Some(move |event| {
                let open = matches!(
                    event,
                    TrayIconEvent::Click {
                        button: MouseButton::Left,
                        button_state: MouseButtonState::Up,
                        ..
                    } | TrayIconEvent::DoubleClick {
                        button: MouseButton::Left,
                        ..
                    }
                );
                if open {
                    show.store(true, Ordering::Relaxed);
                    ctx.request_repaint();
                }
            }));
        }
        {
            let (show, quit, ctx) = (Arc::clone(&show), Arc::clone(&quit), ctx.clone());
            MenuEvent::set_event_handler(Some(move |event: MenuEvent| {
                match event.id.as_ref() {
                    OPEN => show.store(true, Ordering::Relaxed),
                    QUIT => quit.store(true, Ordering::Relaxed),
                    _ => return,
                }
                ctx.request_repaint();
            }));
        }
        Some(Self {
            _icon: icon,
            show,
            quit,
        })
    }

    /// The icon or "Open Explorist" was clicked since the last call.
    pub fn take_show(&self) -> bool {
        self.show.swap(false, Ordering::Relaxed)
    }

    /// "Quit Explorist" was clicked since the last call.
    pub fn take_quit(&self) -> bool {
        self.quit.swap(false, Ordering::Relaxed)
    }
}

/// The app icon, small.
fn icon() -> Option<tray_icon::Icon> {
    let png = include_bytes!("../../assets/icon/icon-256.png");
    let image = image::load_from_memory_with_format(png, image::ImageFormat::Png)
        .ok()?
        .resize(64, 64, image::imageops::FilterType::Lanczos3)
        .to_rgba8();
    let (w, h) = image.dimensions();
    tray_icon::Icon::from_rgba(image.into_raw(), w, h).ok()
}

/// Whether the desktop shows tray icons. Windows and macOS always do; on
/// Linux it takes a StatusNotifier host (KDE, Cinnamon, XFCE and most
/// others have one; GNOME needs the AppIndicator extension).
pub fn available() -> bool {
    #[cfg(all(unix, not(target_os = "macos")))]
    {
        use std::sync::OnceLock;
        static AVAILABLE: OnceLock<bool> = OnceLock::new();
        *AVAILABLE.get_or_init(|| {
            let ask = || -> zbus::Result<bool> {
                let bus = zbus::blocking::Connection::session()?;
                let dbus = zbus::blocking::fdo::DBusProxy::new(&bus)?;
                let name = "org.kde.StatusNotifierWatcher".try_into()?;
                Ok(dbus.name_has_owner(name)?)
            };
            ask().unwrap_or(false)
        })
    }
    #[cfg(not(all(unix, not(target_os = "macos"))))]
    true
}

/// Hides or shows the Dock icon, so Explorist in the menu bar doesn't also
/// sit in the Dock with no window.
#[cfg(target_os = "macos")]
pub fn set_dock_icon(shown: bool) {
    use objc2_app_kit::{NSApplication, NSApplicationActivationPolicy};
    let Some(mtm) = objc2::MainThreadMarker::new() else {
        return;
    };
    let app = NSApplication::sharedApplication(mtm);
    let policy = if shown {
        NSApplicationActivationPolicy::Regular
    } else {
        NSApplicationActivationPolicy::Accessory
    };
    app.setActivationPolicy(policy);
    if shown {
        #[allow(deprecated)]
        app.activateIgnoringOtherApps(true);
    }
}

#[cfg(not(target_os = "macos"))]
pub fn set_dock_icon(_shown: bool) {}

/// Brings Explorist to the front without showing its Dock icon, for the
/// quick search box. Elsewhere, focusing the window is enough.
#[cfg(target_os = "macos")]
pub fn activate_app() {
    use objc2_app_kit::NSApplication;
    if let Some(mtm) = objc2::MainThreadMarker::new() {
        #[allow(deprecated)]
        NSApplication::sharedApplication(mtm).activateIgnoringOtherApps(true);
    }
}

#[cfg(not(target_os = "macos"))]
pub fn activate_app() {}

/// Hands the keyboard back to the app used before the quick search box,
/// when no Explorist window is showing. Windows and Linux do this
/// themselves when the box hides.
#[cfg(target_os = "macos")]
pub fn hide_app() {
    use objc2_app_kit::NSApplication;
    if let Some(mtm) = objc2::MainThreadMarker::new() {
        NSApplication::sharedApplication(mtm).hide(None);
    }
}

#[cfg(not(target_os = "macos"))]
pub fn hide_app() {}
