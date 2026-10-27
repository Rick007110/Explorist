//! Quick search (Settings → General): a shortcut that works in any app and
//! pops up a box over everything for finding any file on the device, like
//! Spotlight or Raycast.
//!
//! The box is its own small window, made once (hidden) on the first frame
//! and then only shown and hidden, so it appears at once even while the
//! main window sits in the tray. It searches the device's list of names
//! (see [`crate::fs::index`]), which is kept in memory and on disk.
//!
//! The box draws on its own (a "deferred" viewport), so what it shares with
//! the app goes through [`Shared`].

use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{Receiver, Sender};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use egui::text::{CCursor, CCursorRange, LayoutJob, TextFormat};
use egui::{
    Color32, ColorImage, CornerRadius, Event, Id, Key, Modifiers, Pos2, Rect, ScrollArea, Sense,
    Stroke, StrokeKind, TextureHandle, ViewportBuilder, ViewportCommand, ViewportId, vec2,
};
use global_hotkey::hotkey::HotKey;
use global_hotkey::{GlobalHotKeyEvent, GlobalHotKeyManager, HotKeyState};

use crate::app::ExploristApp;
use crate::fs::apps::{self, App, Icon as AppIcon};
use crate::fs::index::NameIndex;
use crate::ui::icons::{self, Icon};
use crate::ui::theme::{Palette, radius};
use crate::ui::widgets;

const WIDTH: f32 = 700.0;
const HEIGHT: f32 = 470.0;
/// Results shown at most.
const LIMIT: usize = 60;
/// Apps shown at most, above the files.
const APPS: usize = 5;
/// The app list is read again when the box opens after this long.
const APPS_MAX_AGE: Duration = Duration::from_secs(120);
const ROW: f32 = 46.0;

/// Asked for from outside a frame: `--search` (handed over, or nothing
/// was running to hand it to), or the tray menu.
static OPEN_SOON: AtomicBool = AtomicBool::new(false);

/// Opens the box on the next frame, or once Explorist has started.
pub fn open_soon() {
    OPEN_SOON.store(true, Ordering::Relaxed);
}

fn viewport() -> ViewportId {
    ViewportId::from_hash_of("explorist-quick-search")
}

/// What the box and the app share.
#[derive(Default)]
struct Shared {
    open: bool,
    /// Focus the field and select what's in it.
    just_opened: bool,
    /// The query is still selected only because the box selected it on
    /// opening, not because the person picked it out.
    auto_selected: bool,
    /// The box has had focus since opening, so losing it closes the box.
    had_focus: bool,
    query: String,
    /// The query and list generation the results are for.
    searched: Option<(String, u64)>,
    results: Vec<Row>,
    selected: usize,
    index: Option<Arc<NameIndex>>,
    /// The list is being replaced by a fresh one.
    refreshing: bool,
    recent: Vec<PathBuf>,
    palette: Option<Palette>,
    requests: Vec<Request>,
    error: Option<String>,
    /// Installed apps, when they're searched too.
    apps: Arc<Vec<App>>,
    apps_enabled: bool,
    /// Bumped when the apps or whether they're searched change.
    apps_serial: u64,
    apps_listed: Option<Instant>,
    apps_listing: bool,
    /// App icons by app, `None` while loading or when there's none.
    icons: HashMap<String, Option<TextureHandle>>,
    icon_requests: Option<Sender<(String, AppIcon)>>,
    icon_results: Option<Receiver<(String, Option<ColorImage>)>>,
}

#[derive(Clone)]
struct Row {
    kind: RowKind,
    /// Matched parts of the name, as char ranges.
    matches: Vec<(usize, usize)>,
}

#[derive(Clone)]
enum RowKind {
    App(App),
    Path { path: PathBuf, is_dir: bool },
}

impl Row {
    /// What the row stands for, to find it again among new results.
    fn identity(&self) -> String {
        match &self.kind {
            RowKind::App(app) => format!("app:{}", app.target),
            RowKind::Path { path, .. } => format!("path:{}", path.display()),
        }
    }

    fn name(&self) -> String {
        match &self.kind {
            RowKind::App(app) => app.name.clone(),
            RowKind::Path { path, .. } => name_of(path),
        }
    }
}

/// What the box asks the app to do.
enum Request {
    /// Show this in Explorist's window: a folder opened, a file selected.
    Reveal(PathBuf, bool),
    /// The box closed.
    Closed,
}

impl Shared {
    fn close(&mut self, ctx: &egui::Context) {
        if !self.open {
            return;
        }
        self.open = false;
        self.requests.push(Request::Closed);
        ctx.send_viewport_cmd_to(viewport(), ViewportCommand::Visible(false));
        ctx.request_repaint_of(ViewportId::ROOT);
    }
}

/// The shortcut, the box and their state, kept by the app.
#[derive(Default)]
pub struct QuickSearch {
    shared: Arc<Mutex<Shared>>,
    /// The shortcut was pressed.
    pressed: Arc<AtomicBool>,
    manager: Option<GlobalHotKeyManager>,
    /// The shortcut registered now.
    registered: Option<HotKey>,
    /// The keys last tried (from the settings), worked or not, so a
    /// failure isn't retried every frame.
    attempted: Option<String>,
    /// Why the shortcut couldn't be set up, for Settings.
    pub error: Option<String>,
    /// Fixed once the box exists, so it isn't changed by accident.
    builder: Option<ViewportBuilder>,
}

impl ExploristApp {
    /// Runs in `logic`, so also while the main window is hidden: keeps the
    /// shortcut in step with the settings and opens or closes the box.
    pub(crate) fn quick_search_logic(&mut self, ctx: &egui::Context) {
        self.sync_shortcut(ctx);
        let enabled = self.settings.quick_search && self.quick.builder.is_some();
        if self.quick.pressed.swap(false, Ordering::Relaxed) && enabled {
            let open = self.quick.shared.lock().unwrap().open;
            if open {
                self.quick.shared.lock().unwrap().close(ctx);
            } else {
                self.open_quick_search(ctx);
            }
        }
        if enabled && OPEN_SOON.swap(false, Ordering::Relaxed) {
            self.open_quick_search(ctx);
        }

        let requests = {
            let mut shared = self.quick.shared.lock().unwrap();
            if shared.open || shared.palette.is_none() {
                shared.index = self.device_index.as_ref().map(|(_, i)| Arc::clone(i));
                shared.refreshing = self.device_index_refreshing();
                shared.recent = self.recent.iter().take(8).cloned().collect();
            }
            shared.palette = Some(self.palette);
            if shared.apps_enabled != self.settings.quick_search_apps {
                shared.apps_enabled = self.settings.quick_search_apps;
                shared.apps_serial += 1;
            }
            std::mem::take(&mut shared.requests)
        };
        let main_shown = ctx.input(|i| {
            i.raw
                .viewports
                .get(&ViewportId::ROOT)
                .and_then(|v| v.visible())
                .unwrap_or(true)
        });
        // Showing something in the window brings Explorist forward, so the
        // keyboard shouldn't be handed back to another app after it.
        let revealing = requests.iter().any(|r| matches!(r, Request::Reveal(..)));
        for request in requests {
            match request {
                Request::Reveal(path, is_dir) => {
                    ctx.send_viewport_cmd(ViewportCommand::Visible(true));
                    ctx.send_viewport_cmd(ViewportCommand::Minimized(false));
                    ctx.send_viewport_cmd(ViewportCommand::Focus);
                    crate::app::tray::set_dock_icon(true);
                    self.view.settings_open = false;
                    if is_dir {
                        self.browser.navigate(path);
                    } else if let Some(parent) = path.parent() {
                        self.browser.navigate(parent.to_path_buf());
                        self.browser.select_when_loaded(path);
                    }
                }
                // Give the keyboard back to the app that had it.
                Request::Closed if !main_shown && !revealing => crate::app::tray::hide_app(),
                Request::Closed => {}
            }
        }
        if !self.settings.quick_search {
            self.quick.shared.lock().unwrap().close(ctx);
        }
    }

    /// Runs on shown frames: keeps the box's window in existence (hidden
    /// until the shortcut is pressed), which only a shown frame can do.
    pub(crate) fn quick_search_ui(&mut self, ctx: &egui::Context) {
        // Only the first Explorist gets the shortcut and `--search`, so
        // other copies (Open in new window) don't need a box.
        if !self.settings.quick_search || !self.is_first_instance() {
            self.quick.builder = None;
            return;
        }
        let builder = self
            .quick
            .builder
            .get_or_insert_with(|| {
                ViewportBuilder::default()
                    .with_title("Explorist search")
                    .with_inner_size([WIDTH, HEIGHT])
                    .with_position(box_position(ctx))
                    .with_decorations(false)
                    .with_resizable(false)
                    .with_always_on_top()
                    .with_taskbar(false)
                    .with_visible(false)
            })
            .clone();
        let shared = Arc::clone(&self.quick.shared);
        ctx.show_viewport_deferred(viewport(), builder, move |ui, _| {
            let mut shared = shared.lock().unwrap_or_else(|e| e.into_inner());
            draw(ui, &mut shared);
        });
    }

    /// Reads the installed apps in the background, if they're searched and
    /// the list is missing or old.
    pub(crate) fn list_apps_if_old(&mut self) {
        if !self.settings.quick_search_apps {
            return;
        }
        let mut shared = self.quick.shared.lock().unwrap();
        let old = shared
            .apps_listed
            .is_none_or(|t| t.elapsed() > APPS_MAX_AGE);
        if !old || shared.apps_listing {
            return;
        }
        shared.apps_listing = true;
        shared.apps_listed = Some(Instant::now());
        drop(shared);
        let (shared, ctx) = (Arc::clone(&self.quick.shared), self.ctx.clone());
        std::thread::Builder::new()
            .name("explorist-apps".into())
            .spawn(move || {
                let list = apps::list();
                let mut shared = shared.lock().unwrap_or_else(|e| e.into_inner());
                shared.apps = Arc::new(list);
                shared.apps_serial += 1;
                shared.apps_listing = false;
                drop(shared);
                ctx.request_repaint_of(viewport());
            })
            .ok();
    }

    fn open_quick_search(&mut self, ctx: &egui::Context) {
        self.ensure_device_index();
        self.list_apps_if_old();
        {
            let mut shared = self.quick.shared.lock().unwrap();
            shared.open = true;
            shared.just_opened = true;
            shared.had_focus = false;
            shared.selected = 0;
            shared.error = None;
            shared.searched = None;
            shared.index = self.device_index.as_ref().map(|(_, i)| Arc::clone(i));
        }
        let id = viewport();
        ctx.send_viewport_cmd_to(id, ViewportCommand::OuterPosition(box_position(ctx)));
        ctx.send_viewport_cmd_to(id, ViewportCommand::Visible(true));
        ctx.send_viewport_cmd_to(id, ViewportCommand::Focus);
        crate::app::tray::activate_app();
        ctx.request_repaint_of(id);
    }

    /// Registers the shortcut from the settings, or drops it when turned
    /// off. Only the first Explorist running holds it.
    fn sync_shortcut(&mut self, ctx: &egui::Context) {
        let wanted = (self.settings.quick_search && self.is_first_instance())
            .then(|| self.settings.quick_search_keys.clone());
        let quick = &mut self.quick;
        if wanted == quick.attempted {
            return;
        }
        quick.attempted.clone_from(&wanted);
        if let (Some(manager), Some(old)) = (&quick.manager, quick.registered.take()) {
            let _ = manager.unregister(old);
        }
        quick.error = None;
        let Some(keys) = wanted else { return };
        let Some(hotkey) = parse_keys(&keys) else {
            quick.error = Some(format!("“{keys}” isn't a shortcut Explorist can use."));
            return;
        };
        if quick.manager.is_none() {
            match GlobalHotKeyManager::new() {
                Ok(manager) => {
                    let (pressed, ctx) = (Arc::clone(&quick.pressed), ctx.clone());
                    GlobalHotKeyEvent::set_event_handler(Some(move |event: GlobalHotKeyEvent| {
                        if event.state() == HotKeyState::Pressed {
                            pressed.store(true, Ordering::Relaxed);
                            ctx.request_repaint();
                        }
                    }));
                    quick.manager = Some(manager);
                }
                Err(err) => {
                    quick.error = Some(format!(
                        "Shortcuts that work in any app aren't available here ({err})."
                    ));
                    return;
                }
            }
        }
        let manager = quick.manager.as_ref().expect("made above");
        match manager.register(hotkey) {
            Ok(()) => quick.registered = Some(hotkey),
            Err(_) => {
                quick.error = Some(format!(
                    "Couldn't use {}: another app may have it. Choose other keys.",
                    display_keys(&keys)
                ));
            }
        }
    }

    /// Records the next key press as the quick search shortcut (Settings).
    pub(crate) fn record_quick_keys(&mut self, ctx: &egui::Context) {
        let events = ctx.input(|i| i.events.clone());
        for event in events {
            let Event::Key {
                key,
                pressed: true,
                modifiers,
                ..
            } = event
            else {
                continue;
            };
            if key == Key::Escape && modifiers.is_none() {
                self.view.settings.recording_quick = false;
                return;
            }
            if let Some(keys) = keys_from(key, modifiers) {
                self.settings.quick_search_keys = keys;
                self.view.settings.recording_quick = false;
                self.save_settings();
                return;
            }
        }
    }
}

/// Where the box goes: centered, a little above the middle of the screen.
fn box_position(ctx: &egui::Context) -> Pos2 {
    let monitor = ctx
        .input(|i| {
            i.raw
                .viewports
                .get(&ViewportId::ROOT)
                .and_then(|v| v.monitor_size)
        })
        .unwrap_or(vec2(1920.0, 1080.0));
    Pos2::new(
        ((monitor.x - WIDTH) / 2.0).max(0.0),
        ((monitor.y - HEIGHT) * 0.3).max(0.0),
    )
}

/// Settings text ("Alt+Space") as a shortcut, if it is one.
pub fn parse_keys(keys: &str) -> Option<HotKey> {
    let parts: Vec<&str> = keys.split('+').map(str::trim).collect();
    if parts.iter().any(|p| p.is_empty()) {
        return None;
    }
    parts.join("+").parse().ok()
}

/// A pressed key and modifiers as settings text, if they make a usable
/// shortcut: a modifier is needed, except for F-keys.
pub fn keys_from(key: Key, modifiers: Modifiers) -> Option<String> {
    // Shift, Ctrl and the like arrive as keys of their own on the way to
    // the real one.
    if crate::app::keymap::is_modifier(key) {
        return None;
    }
    let mut parts = Vec::new();
    if modifiers.ctrl {
        parts.push("Ctrl");
    }
    if modifiers.alt {
        parts.push("Alt");
    }
    if modifiers.shift {
        parts.push("Shift");
    }
    if modifiers.mac_cmd {
        parts.push("Cmd");
    }
    let name = key.name();
    let function_key = name.len() > 1 && name.starts_with('F') && name[1..].parse::<u8>().is_ok();
    if parts.is_empty() && !function_key {
        return None;
    }
    let keys = parts
        .into_iter()
        .chain(std::iter::once(name))
        .collect::<Vec<_>>()
        .join("+");
    parse_keys(&keys).map(|_| keys)
}

/// Settings text as shown: "Alt Space", or "Option Space" on macOS.
pub fn display_keys(keys: &str) -> String {
    keys.split('+')
        .map(|part| match part.trim() {
            "Alt" if cfg!(target_os = "macos") => "Option",
            "Cmd" => "⌘",
            other => other,
        })
        .collect::<Vec<_>>()
        .join(" ")
}

/// Whether global shortcuts can't reach Explorist here: on Wayland only
/// the desktop can bind them.
pub fn wayland() -> bool {
    cfg!(all(unix, not(target_os = "macos")))
        && std::env::var("XDG_SESSION_TYPE").is_ok_and(|t| t == "wayland")
}

/// The box: a field, the results and key hints.
fn draw(ui: &mut egui::Ui, shared: &mut Shared) {
    let Some(p) = shared.palette else { return };
    if !shared.open {
        return;
    }
    let ctx = ui.ctx().clone();
    let (focused, close_asked) =
        ui.input(|i| (i.viewport().focused, i.viewport().close_requested()));
    if close_asked {
        ctx.send_viewport_cmd(ViewportCommand::CancelClose);
    }
    if focused == Some(true) {
        shared.had_focus = true;
    }
    // Ctrl C copies the result's path too unless the person picked out
    // text in the field: the query the box selects on opening doesn't
    // count. It's also the one sure way on macOS, where a quick Cmd Shift C
    // can arrive without its Shift.
    let field_selection = egui::TextEdit::load_state(&ctx, Id::new("quick-search-field"))
        .and_then(|state| state.cursor.char_range())
        .is_some_and(|range| !range.is_empty());
    let (plain_copy, select_all) = ui.input(|i| {
        (
            i.events.iter().any(|e| matches!(e, Event::Copy)),
            i.events.iter().any(|e| {
                matches!(e, Event::Key { key: Key::A, pressed: true, modifiers, .. } if modifiers.command)
            }),
        )
    });
    let picked_text = field_selection && (!shared.auto_selected || select_all);
    let command = Modifiers::COMMAND;
    let (escape, up, down, enter, reveal, copy) = ui.input_mut(|i| {
        let copy = copy_path_pressed(&i.events, i.modifiers.shift);
        (
            i.consume_key(Modifiers::NONE, Key::Escape),
            i.consume_key(Modifiers::NONE, Key::ArrowUp),
            i.consume_key(Modifiers::NONE, Key::ArrowDown),
            i.consume_key(Modifiers::NONE, Key::Enter),
            i.consume_key(command, Key::Enter),
            copy,
        )
    });
    if escape || close_asked || (shared.had_focus && focused == Some(false)) {
        shared.close(&ctx);
        return;
    }

    search(shared);
    let count = shared.results.len();
    if count > 0 {
        if down {
            shared.selected = (shared.selected + 1) % count;
        }
        if up {
            shared.selected = (shared.selected + count - 1) % count;
        }
        shared.selected = shared.selected.min(count - 1);
    }

    let full = ui.max_rect();
    ui.painter().rect(
        full,
        CornerRadius::ZERO,
        p.surface,
        Stroke::new(1.0, p.border),
        StrokeKind::Inside,
    );

    // The field.
    let field = Rect::from_min_size(full.min + vec2(18.0, 14.0), vec2(full.width() - 36.0, 40.0));
    icons::paint(
        ui.painter(),
        Rect::from_center_size(
            Pos2::new(field.left() + 11.0, field.center().y),
            vec2(20.0, 20.0),
        ),
        Icon::Search,
        20.0,
        p.text_2,
    );
    let status = status_text(shared);
    let status_galley =
        ui.painter()
            .layout_no_wrap(status, crate::ui::fonts::regular(12.0), p.muted);
    ui.painter().galley(
        Pos2::new(
            field.right() - status_galley.size().x,
            field.center().y - status_galley.size().y / 2.0,
        ),
        status_galley.clone(),
        p.muted,
    );
    let text_rect = Rect::from_min_max(
        Pos2::new(field.left() + 34.0, field.center().y - 13.0),
        Pos2::new(
            field.right() - status_galley.size().x - 16.0,
            field.center().y + 13.0,
        ),
    );
    let id = Id::new("quick-search-field");
    let mut child = ui.new_child(egui::UiBuilder::new().max_rect(text_rect));
    let output = egui::TextEdit::singleline(&mut shared.query)
        .id(id)
        .hint_text("Search apps and files")
        .font(crate::ui::fonts::regular(19.0))
        .text_color(p.text)
        .frame(egui::Frame::NONE)
        .margin(egui::Margin::ZERO)
        .desired_width(text_rect.width())
        .show(&mut child);
    if shared.just_opened {
        shared.just_opened = false;
        output.response.request_focus();
        let mut state = output.state.clone();
        let end = shared.query.chars().count();
        state
            .cursor
            .set_char_range(Some(CCursorRange::two(CCursor::new(0), CCursor::new(end))));
        state.store(&ctx, id);
        shared.auto_selected = end > 0;
    } else {
        if !output.response.has_focus() {
            output.response.request_focus();
        }
        // Typing, clicking, Ctrl A or moving the cursor makes it theirs.
        let whole = output.state.cursor.char_range().is_some_and(|range| {
            let [start, end] = range.sorted_cursors();
            start.index == CCursor::new(0).index
                && end.index == CCursor::new(shared.query.chars().count()).index
        });
        if output.response.changed()
            || output.response.clicked()
            || output.response.drag_started()
            || select_all
            || !whole
        {
            shared.auto_selected = false;
        }
    }
    if output.response.changed() {
        shared.selected = 0;
        search(shared);
    }
    ui.painter().hline(
        full.x_range(),
        field.bottom() + 12.0,
        Stroke::new(1.0, p.border_subtle),
    );

    // The results.
    let footer = 38.0;
    let list = Rect::from_min_max(
        Pos2::new(full.left() + 8.0, field.bottom() + 18.0),
        Pos2::new(full.right() - 8.0, full.bottom() - footer - 4.0),
    );
    let mut chosen: Option<(usize, bool)> = None;
    request_icons(shared, &ctx);
    let mut list_ui = ui.new_child(egui::UiBuilder::new().max_rect(list));
    if shared.results.is_empty() {
        let text = if shared.query.trim().is_empty() {
            "Type a name to find any app or file on this device."
        } else if shared.index.as_ref().is_some_and(|i| !i.is_done()) {
            "Still listing files…"
        } else {
            "No matches"
        };
        list_ui.painter().text(
            list.center_top() + vec2(0.0, 40.0),
            egui::Align2::CENTER_CENTER,
            text,
            crate::ui::fonts::regular(13.0),
            p.muted,
        );
    } else {
        if shared.query.trim().is_empty() {
            list_ui.label(
                egui::RichText::new("Recent folders")
                    .font(crate::ui::fonts::medium(11.5))
                    .color(p.muted),
            );
        }
        ScrollArea::vertical()
            .auto_shrink([false, false])
            .show(&mut list_ui, |ui| {
                ui.spacing_mut().item_spacing.y = 2.0;
                for (i, row) in shared.results.iter().enumerate() {
                    let selected = i == shared.selected;
                    if row_ui(ui, &p, row, selected, &shared.icons) {
                        chosen = Some((i, false));
                    }
                    if selected && (up || down) {
                        ui.scroll_to_rect(
                            ui.min_rect().with_min_y(ui.min_rect().bottom() - ROW),
                            None,
                        );
                    }
                }
            });
    }
    if let Some(err) = &shared.error {
        ui.painter().text(
            Pos2::new(full.left() + 18.0, full.bottom() - footer - 14.0),
            egui::Align2::LEFT_CENTER,
            err,
            crate::ui::fonts::regular(12.0),
            p.error,
        );
    }

    // Key hints.
    ui.painter().hline(
        full.x_range(),
        full.bottom() - footer,
        Stroke::new(1.0, p.border_subtle),
    );
    let mut x = full.left() + 18.0;
    let y = full.bottom() - footer / 2.0;
    let cmd = if cfg!(target_os = "macos") {
        "⌘"
    } else {
        "Ctrl"
    };
    for (keys, label) in [
        ("Enter".to_owned(), "Open"),
        (format!("{cmd} Enter"), "Show in Explorist"),
        (format!("{cmd} Shift C"), "Copy path"),
        ("Esc".to_owned(), "Close"),
    ] {
        let g = widgets::kbd_galley(ui, &p, &keys);
        let w = widgets::kbd_width(&g);
        widgets::paint_kbd(ui.painter(), &p, g, x, y, false);
        x += w + 6.0;
        let text = ui.painter().layout_no_wrap(
            label.to_owned(),
            crate::ui::fonts::regular(12.0),
            p.text_2,
        );
        let tw = text.size().x;
        ui.painter()
            .galley(Pos2::new(x, y - text.size().y / 2.0), text, p.text_2);
        x += tw + 18.0;
    }

    if enter && !shared.results.is_empty() {
        chosen = Some((shared.selected, false));
    }
    if reveal && !shared.results.is_empty() {
        chosen = Some((shared.selected, true));
    }
    let copy = copy || (plain_copy && !picked_text);
    if copy && let Some(row) = shared.results.get(shared.selected) {
        let text = match &row.kind {
            RowKind::App(app) => app
                .file
                .as_ref()
                .map_or_else(|| app.target.clone(), |f| f.display().to_string()),
            RowKind::Path { path, .. } => path.display().to_string(),
        };
        ctx.copy_text(text);
        shared.close(&ctx);
        return;
    }
    if let Some((i, reveal)) = chosen {
        match shared.results[i].kind.clone() {
            RowKind::App(app) => match (&app.file, reveal) {
                (Some(file), true) => {
                    shared.requests.push(Request::Reveal(file.clone(), false));
                    shared.close(&ctx);
                }
                _ => match apps::launch(&app) {
                    Ok(()) => shared.close(&ctx),
                    Err(err) => {
                        shared.error = Some(format!("Couldn't start {}: {err}", app.name));
                    }
                },
            },
            RowKind::Path { path, is_dir } => {
                if reveal || is_dir {
                    shared.requests.push(Request::Reveal(path, is_dir));
                    shared.close(&ctx);
                } else if !path.exists() {
                    shared.error = Some(format!("“{}” isn't there any more.", name_of(&path)));
                } else {
                    match crate::fs::shell::open_default(&path) {
                        Ok(()) => shared.close(&ctx),
                        Err(err) => shared.error = Some(format!("Couldn't open it: {err}")),
                    }
                }
            }
        }
    }
}

/// Whether Ctrl Shift C (⌘ Shift C) was pressed. Ctrl C arrives as a Copy
/// event that doesn't say whether Shift was down, and a quick press can be
/// over (Shift up again) by the time the frame sees it, so Shift is
/// followed through its own press and release events instead.
fn copy_path_pressed(events: &[Event], shift_now: bool) -> bool {
    let shift_event = |e: &Event| match e {
        Event::Key {
            key: Key::ShiftLeft | Key::ShiftRight,
            pressed,
            ..
        } => Some(*pressed),
        _ => None,
    };
    // Down at the start if its first event this frame lets it go.
    let mut shift = events
        .iter()
        .find_map(shift_event)
        .map_or(shift_now, |pressed| !pressed);
    for event in events {
        if let Some(pressed) = shift_event(event) {
            shift = pressed;
            continue;
        }
        match event {
            Event::Key {
                key: Key::C,
                pressed: true,
                modifiers,
                ..
            } if modifiers.command && modifiers.shift => return true,
            Event::Copy if shift => return true,
            _ => {}
        }
    }
    false
}

/// Searches again when the query, the list or the apps changed.
fn search(shared: &mut Shared) {
    let generation = shared.index.as_ref().map_or(0, |i| i.generation());
    let key = (
        shared.query.clone(),
        generation ^ (shared.apps_serial << 48),
    );
    if shared.searched.as_ref() == Some(&key) {
        return;
    }
    // When only the list changed (it's still filling in, or a fresh one
    // replaced it), keep the same item picked, not the same position.
    let same_query = shared
        .searched
        .as_ref()
        .is_some_and(|(q, _)| *q == shared.query);
    let picked = shared
        .results
        .get(shared.selected)
        .filter(|_| same_query)
        .map(Row::identity);
    let query = shared.query.trim();
    shared.results =
        if query.is_empty() {
            shared
                .recent
                .iter()
                .map(|path| Row {
                    kind: RowKind::Path {
                        path: path.clone(),
                        is_dir: true,
                    },
                    matches: Vec::new(),
                })
                .collect()
        } else {
            let mut rows = Vec::new();
            if shared.apps_enabled {
                rows.extend(apps::find(&shared.apps, query, APPS).into_iter().map(
                    |(i, matches)| Row {
                        kind: RowKind::App(shared.apps[i].clone()),
                        matches,
                    },
                ));
            }
            if let Some(index) = &shared.index {
                rows.extend(index.search(query, LIMIT).into_iter().map(|hit| Row {
                    kind: RowKind::Path {
                        path: hit.path,
                        is_dir: hit.is_dir,
                    },
                    matches: hit.matches,
                }));
            }
            rows
        };
    if let Some(picked) = picked {
        shared.selected = shared
            .results
            .iter()
            .position(|r| r.identity() == picked)
            .unwrap_or(0);
    }
    shared.searched = Some(key);
}

/// Asks for the icons of the apps in the results, loaded off this thread.
fn request_icons(shared: &mut Shared, ctx: &egui::Context) {
    if let Some(results) = &shared.icon_results {
        for (key, image) in results.try_iter() {
            let texture =
                image.map(|i| ctx.load_texture(format!("app-icon:{key}"), i, Default::default()));
            shared.icons.insert(key, texture);
        }
    }
    let wanted: Vec<(String, AppIcon)> = shared
        .results
        .iter()
        .filter_map(|row| match &row.kind {
            RowKind::App(app) if !shared.icons.contains_key(&app.target) => {
                Some((app.target.clone(), app.icon.clone()))
            }
            _ => None,
        })
        .collect();
    if wanted.is_empty() {
        return;
    }
    let sender = shared.icon_requests.get_or_insert_with(|| {
        let (requests, inbox) = std::sync::mpsc::channel::<(String, AppIcon)>();
        let (outbox, results) = std::sync::mpsc::channel();
        let ctx = ctx.clone();
        std::thread::Builder::new()
            .name("explorist-app-icons".into())
            .spawn(move || {
                for (key, icon) in inbox {
                    if outbox.send((key, app_icon(&icon))).is_err() {
                        return;
                    }
                    ctx.request_repaint_of(viewport());
                }
            })
            .ok();
        shared.icon_results = Some(results);
        requests
    });
    for (key, icon) in wanted {
        let _ = sender.send((key.clone(), icon));
        shared.icons.insert(key, None);
    }
}

/// An app's icon, as an image.
fn app_icon(icon: &AppIcon) -> Option<ColorImage> {
    let rgba: Option<image::RgbaImage> = match icon {
        AppIcon::File(path) => return crate::ui::image_icon::load(path),
        #[cfg(windows)]
        AppIcon::Shell(name) => crate::fs::system_thumbs::icon(std::ffi::OsStr::new(name), 64),
        #[cfg(target_os = "macos")]
        AppIcon::Bundle(path) => crate::fs::system_thumbs::app_icon(path, 64)
            .or_else(|| crate::fs::system_thumbs::thumbnail(path, 64)),
        _ => None,
    };
    let rgba = rgba?;
    Some(ColorImage::from_rgba_unmultiplied(
        [rgba.width() as usize, rgba.height() as usize],
        rgba.as_raw(),
    ))
}

fn status_text(shared: &Shared) -> String {
    match &shared.index {
        None => "Getting ready…".to_owned(),
        Some(index) if !index.is_done() => {
            format!("Listing… {}", crate::fs::format::count(index.len() as u64))
        }
        Some(index) => {
            let count = crate::fs::format::count(index.len() as u64);
            if shared.refreshing {
                format!("{count} items · updating")
            } else {
                format!("{count} items")
            }
        }
    }
}

fn name_of(path: &std::path::Path) -> String {
    path.file_name().map_or_else(
        || path.display().to_string(),
        |n| n.to_string_lossy().into_owned(),
    )
}

/// One result: icon, name with the matches picked out, and its folder
/// (or "App").
fn row_ui(
    ui: &mut egui::Ui,
    p: &Palette,
    row: &Row,
    selected: bool,
    icons: &HashMap<String, Option<TextureHandle>>,
) -> bool {
    let (rect, response) = ui.allocate_exact_size(vec2(ui.available_width(), ROW), Sense::click());
    if selected {
        ui.painter()
            .rect_filled(rect, CornerRadius::same(radius::MD), p.accent_alpha(0.16));
        if ui.input(|i| i.key_pressed(Key::ArrowDown) || i.key_pressed(Key::ArrowUp)) {
            ui.scroll_to_rect(rect, None);
        }
    } else if response.hovered() {
        ui.painter()
            .rect_filled(rect, CornerRadius::same(radius::MD), p.hover);
    }
    let name = row.name();
    let icon_rect = Rect::from_center_size(
        Pos2::new(rect.left() + 22.0, rect.center().y),
        vec2(20.0, 20.0),
    );
    let picture = match &row.kind {
        RowKind::App(app) => icons.get(&app.target).cloned().flatten(),
        RowKind::Path { .. } => None,
    };
    if let Some(texture) = picture {
        let size = texture.size_vec2();
        let side = 26.0;
        let scale = side / size.x.max(size.y);
        egui::Image::new(&texture)
            .paint_at(ui, Rect::from_center_size(icon_rect.center(), size * scale));
    } else {
        let (icon, color) = match &row.kind {
            RowKind::App(_) => (Icon::Exe, p.text_2),
            RowKind::Path { is_dir: true, .. } => (Icon::Folder, p.accent),
            RowKind::Path { .. } => {
                let kind = crate::fs::kind::classify(&name, false).0;
                (Icon::for_kind(kind), p.text_2)
            }
        };
        icons::paint(ui.painter(), icon_rect, icon, 20.0, color);
    }
    let left = rect.left() + 46.0;
    let width = rect.right() - left - 12.0;

    let font = crate::ui::fonts::regular(14.0);
    let bold = crate::ui::fonts::semibold(14.0);
    let hit = |i: usize| row.matches.iter().any(|&(a, b)| i >= a && i < b);
    let mut job = LayoutJob::default();
    for (i, ch) in name.chars().enumerate() {
        let on = hit(i);
        job.append(
            ch.encode_utf8(&mut [0; 4]),
            0.0,
            TextFormat {
                font_id: if on { bold.clone() } else { font.clone() },
                color: if on { p.accent } else { p.text },
                ..Default::default()
            },
        );
    }
    job.wrap = egui::text::TextWrapping::truncate_at_width(width);
    let galley = ui.painter().layout_job(job);
    ui.painter().galley(
        Pos2::new(left, rect.top() + 6.0),
        galley,
        Color32::PLACEHOLDER,
    );

    let detail = match &row.kind {
        RowKind::App(_) => {
            ui.painter()
                .layout_no_wrap("App".to_owned(), crate::ui::fonts::regular(12.0), p.muted)
        }
        RowKind::Path { path, .. } => crate::ui::palette::tail_truncated(
            ui,
            &path
                .parent()
                .map(|d| d.display().to_string())
                .unwrap_or_default(),
            crate::ui::fonts::mono(11.5),
            p.muted,
            width,
        ),
    };
    ui.painter()
        .galley(Pos2::new(left, rect.top() + 26.0), detail, p.muted);
    response.clicked()
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Results changing under the same query (the list filling in or being
    /// replaced) keep the same item picked; a new query starts at the top.
    #[test]
    fn the_picked_result_stays_when_the_list_changes() {
        let paths = |names: &[&str]| -> Vec<PathBuf> { names.iter().map(PathBuf::from).collect() };
        let mut shared = Shared {
            recent: paths(&["/a", "/b", "/c"]),
            ..Default::default()
        };
        search(&mut shared);
        shared.selected = 2;
        shared.recent = paths(&["/c", "/a", "/b"]);
        shared.apps_serial += 1;
        search(&mut shared);
        assert_eq!(shared.results[shared.selected].identity(), "path:/c");

        // Gone from the new results: back to the top.
        shared.recent = paths(&["/a", "/b"]);
        shared.apps_serial += 1;
        search(&mut shared);
        assert_eq!(shared.selected, 0);
    }

    #[test]
    fn copy_path_needs_shift_even_when_pressed_quickly() {
        let shift = |pressed| Event::Key {
            key: Key::ShiftLeft,
            physical_key: None,
            pressed,
            repeat: false,
            modifiers: Modifiers::NONE,
        };
        // Held across frames.
        assert!(copy_path_pressed(&[Event::Copy], true));
        // Pressed and let go within one frame.
        assert!(copy_path_pressed(
            &[shift(true), Event::Copy, shift(false)],
            false
        ));
        // Let go in the frame of the copy, having been held before.
        assert!(copy_path_pressed(&[Event::Copy, shift(false)], false));
        // Plain Ctrl C is for the text in the field.
        assert!(!copy_path_pressed(&[Event::Copy], false));
        assert!(!copy_path_pressed(
            &[shift(true), shift(false), Event::Copy],
            false
        ));
    }

    #[test]
    fn shortcuts_from_settings_text() {
        assert!(parse_keys("Alt+Space").is_some());
        assert!(parse_keys("Ctrl + Shift + F").is_some());
        assert!(parse_keys("Alt+").is_none());
        assert!(parse_keys("Banana+Q").is_none());
    }

    #[test]
    fn pressed_keys_become_settings_text() {
        let alt = Modifiers {
            alt: true,
            ..Default::default()
        };
        assert_eq!(keys_from(Key::Space, alt).as_deref(), Some("Alt+Space"));
        let ctrl_shift = Modifiers {
            ctrl: true,
            shift: true,
            ..Default::default()
        };
        assert_eq!(
            keys_from(Key::F, ctrl_shift).as_deref(),
            Some("Ctrl+Shift+F")
        );
        // A plain letter would stop it being typed anywhere.
        assert_eq!(keys_from(Key::F, Modifiers::NONE), None);
        assert_eq!(keys_from(Key::F12, Modifiers::NONE).as_deref(), Some("F12"));
        // Holding Ctrl, then pressing Shift on the way to K: not yet.
        let ctrl = Modifiers {
            ctrl: true,
            ..Default::default()
        };
        assert_eq!(keys_from(Key::ShiftLeft, ctrl), None);
        assert_eq!(display_keys("Ctrl+Shift+F"), "Ctrl Shift F");
    }
}
