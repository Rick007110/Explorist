//! Application state and the top-level eframe app.

pub mod browser;
pub mod commands;
pub mod drag;
pub mod files;
pub mod fuzzy;
pub mod instance;
pub mod integrations;
pub mod keymap;
pub mod selection;
pub mod sizes;
pub mod tabs;
pub mod tray;
pub mod undo;

use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use crate::app::browser::Browser;
use crate::app::commands::{Command, MoveTarget};
use crate::app::keymap::Mods;
use crate::app::selection::MoveMode;
use crate::app::undo::{Action, UndoStack};
use crate::config::{LoadOutcome, Settings, StartIn, Theme};
use crate::fs::entry::Entry;
use crate::fs::ops;
use crate::fs::path_input::{self, Target};
use crate::fs::places::{self, Drive, Place};
use crate::fs::shell;
use crate::fs::sort::SortColumn;
use crate::ui::{self, fonts, theme::Palette};

/// How long a toast stays up, unless hovered.
const TOAST_DWELL: Duration = Duration::from_secs(6);

pub struct ExploristApp {
    pub(crate) ctx: egui::Context,
    pub(crate) settings: Settings,
    settings_path: Option<PathBuf>,
    pub(crate) browser: Browser,
    pub(crate) places: Vec<Place>,
    /// Filled in by a background thread; drive queries can block.
    pub(crate) drives: Arc<Mutex<Vec<Drive>>>,
    pub(crate) palette: Palette,
    pub(crate) view: ViewState,
    undo: UndoStack,
    /// The file operation in progress, if any.
    pub(crate) job: Option<files::RunningJob>,
    pub(crate) preview: crate::fs::preview::Loader,
    /// Folders visited, most recent first, for the palette.
    pub(crate) recent: Vec<PathBuf>,
    /// All tabs; the active one's slot is `None` because its state is in
    /// `browser` and `view`.
    pub(crate) tabs: Vec<Option<tabs::TabSlot>>,
    pub(crate) active_tab: usize,
    pub(crate) git: crate::fs::git::GitWatcher,
    pub(crate) update: crate::ui::update::UpdateState,
    /// The folder contents' generation git last saw, and when.
    git_seen: (u64, Instant),
    git_folder: PathBuf,
    pub(crate) preview_image: Option<ui::preview::PreviewImage>,
    pub(crate) thumbs: ui::grid::ThumbCache,
    /// Results of background tasks like ejecting a drive, shown as toasts.
    notices: (
        std::sync::mpsc::Sender<Notice>,
        std::sync::mpsc::Receiver<Notice>,
    ),
    /// When the drive list was last read, to notice drives coming and going.
    drives_read: Instant,
    pub(crate) folder_sizes: sizes::FolderSizes,
    /// Every name on the device, for searching Everywhere; built on first
    /// use and kept while Explorist runs.
    pub(crate) device_index: Option<(Instant, Arc<crate::fs::index::NameIndex>)>,
    /// A fresh list being made to replace `device_index` when it's done,
    /// so searches keep using the old one meanwhile.
    device_index_next: Option<Arc<crate::fs::index::NameIndex>>,
    /// The list saved last time, being read at startup.
    device_index_loading: Option<std::sync::mpsc::Receiver<Option<crate::fs::index::NameIndex>>>,
    /// `device_index` is finished but not saved yet.
    device_index_unsaved: bool,
    /// Other apps for the right-click menu.
    pub(crate) integrations: integrations::Integrations,
    /// Listens for Explorist being opened again, if this is the first copy.
    instance: Option<instance::Listener>,
    /// The tray icon, while Close to tray is on (first copy only).
    tray: Option<tray::Tray>,
    /// The desktop has no tray area (some Linux desktops).
    pub(crate) tray_unavailable: bool,
    /// Closing really quits now (Quit in the tray menu, restarting to update).
    pub(crate) quitting: bool,
    /// The search-anywhere shortcut and its box.
    pub(crate) quick: crate::ui::quick_search::QuickSearch,
}

/// A background task's outcome, for a toast.
pub(crate) struct Notice {
    pub text: String,
    pub error: bool,
}

/// How often the drive list is read again.
const DRIVE_POLL: std::time::Duration = std::time::Duration::from_secs(4);

/// Transient UI state: what's open, being edited or shown.
#[derive(Default)]
pub struct ViewState {
    pub path_edit: Option<PathEdit>,
    pub filter_open: bool,
    pub filter_text: String,
    pub focus_filter: bool,
    pub rename: Option<Rename>,
    pub toast: Option<Toast>,
    pub confirm_delete: Option<Vec<PathBuf>>,
    pub properties: Option<crate::ui::properties::PropertiesState>,
    pub open_with: Option<crate::ui::open_with::OpenWithState>,
    pub drive_dialog: Option<crate::ui::drive_dialog::DriveDialog>,
    pub folder_icon: Option<crate::ui::folder_icon::FolderIconState>,
    /// Quick Look is showing the focused item.
    pub quick_look: bool,
    /// Rename this item once the running job ends (New folder with
    /// selection).
    pub rename_after_job: Option<PathBuf>,
    /// Files being dragged inside the window.
    pub drag: Option<drag::FileDrag>,
    /// A selection box being dragged, where the list or grid sits, and
    /// how far to scroll it next frame.
    pub marquee: Option<crate::ui::marquee::Marquee>,
    pub viewport: crate::ui::marquee::Viewport,
    pub marquee_scroll: f32,
    /// A drag out of the window ended; the system kept the mouse release,
    /// so send egui one.
    pub release_pointer: bool,
    /// Scroll the focused row into view on the next frame.
    pub scroll_to_focus: bool,
    /// Rows that fit in the list, for Page Up / Page Down.
    pub page_rows: usize,
    /// Tiles per line in the grid; 1 in the list.
    pub columns: usize,
    /// The list's scroll offset and height last frame.
    pub list_offset: f32,
    pub list_height: f32,
    pub maximized: bool,
    /// Items cut to the clipboard, shown dimmed until pasted.
    pub cut: std::collections::HashSet<PathBuf>,
    /// Files are being dragged over the window from outside.
    pub drop_hover: bool,
    /// The Settings page is showing instead of the folder.
    pub settings_open: bool,
    pub settings: crate::ui::settings::SettingsState,
    pub palette: Option<crate::ui::palette::PaletteState>,
    pub batch: Option<crate::ui::batch_rename::BatchState>,
    /// The folder shown last frame, to notice navigation.
    pub last_location: PathBuf,
    /// When Paste last ran from the keyboard, to paste once per press.
    pub last_paste: Option<Instant>,
    pub paste_detector: keymap::PasteDetector,
}

#[derive(Default)]
pub struct PathEdit {
    pub text: String,
    pub error: Option<String>,
    pub just_opened: bool,
    /// Folders completing `text`, and the one picked with the arrow keys.
    pub completions: Vec<String>,
    pub chosen: Option<usize>,
    /// The text `completions` was worked out for.
    pub completed_for: Option<String>,
}

pub struct Rename {
    pub path: PathBuf,
    pub is_dir: bool,
    pub text: String,
    pub error: Option<String>,
    /// Select the name without its extension on the first frame.
    pub just_opened: bool,
}

pub struct Toast {
    pub message: String,
    pub kind: ToastKind,
    /// Show an Undo button.
    pub undo: bool,
    pub shown_at: Instant,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ToastKind {
    Success,
    Error,
}

impl ExploristApp {
    pub fn new(cc: &eframe::CreationContext<'_>) -> Self {
        fonts::install(&cc.egui_ctx);

        let settings_path = Settings::default_path();
        let (settings, outcome) = match &settings_path {
            Some(path) => Settings::load(path).unwrap_or_else(|err| {
                (
                    Settings::default(),
                    LoadOutcome::Invalid {
                        error: err.to_string(),
                        backup: path.clone(),
                    },
                )
            }),
            None => (Settings::default(), LoadOutcome::Missing),
        };

        let last = match settings.start_in {
            StartIn::LastFolder => settings.last_folder.clone().filter(|p| p.is_dir()),
            StartIn::Home => None,
        };
        let start = std::env::args_os()
            .nth(1)
            .map(PathBuf::from)
            .filter(|p| p.is_dir())
            .or(last)
            .or_else(dirs::home_dir)
            .unwrap_or_else(|| PathBuf::from("/"));
        keymap::install(keymap::Keymap::new(&settings.shortcuts));

        let ctx = cc.egui_ctx.clone();
        let browser = Browser::new(
            start.clone(),
            settings.sort_for(&start),
            settings.show_hidden,
            Arc::new(move || ctx.request_repaint()),
        );

        let drives = Arc::new(Mutex::new(Vec::new()));
        refresh_drives(&drives, &cc.egui_ctx);

        let mut places = places::standard_places();
        places.extend(settings.pinned.iter().map(|p| places::pinned_place(p)));

        let palette = Palette::dark(settings.accent);
        let mut app = Self {
            ctx: cc.egui_ctx.clone(),
            settings,
            settings_path,
            browser,
            places,
            drives,
            palette,
            view: ViewState::default(),
            undo: UndoStack::default(),
            job: None,
            preview: Default::default(),
            preview_image: None,
            thumbs: Default::default(),
            recent: Vec::new(),
            tabs: vec![None],
            active_tab: 0,
            git: Default::default(),
            update: Default::default(),
            git_seen: (u64::MAX, Instant::now()),
            git_folder: PathBuf::new(),
            notices: std::sync::mpsc::channel(),
            drives_read: Instant::now(),
            folder_sizes: Default::default(),
            device_index: None,
            device_index_next: None,
            device_index_loading: None,
            device_index_unsaved: false,
            integrations: integrations::Integrations::detect(),
            instance: instance::Listener::start(cc.egui_ctx.clone()),
            tray: None,
            tray_unavailable: false,
            quitting: false,
            quick: Default::default(),
        };
        // Reopen the other tabs from last time.
        let folder_given = std::env::args_os().nth(1).is_some_and(|a| a != "--search");
        if app.settings.start_in == StartIn::LastFolder && !folder_given {
            let saved = app.settings.open_tabs.clone();
            let active = app.settings.active_tab;
            for (i, path) in saved.iter().enumerate() {
                if i == active || !path.is_dir() {
                    continue;
                }
                let ctx = app.ctx.clone();
                let browser = Browser::new(
                    path.clone(),
                    app.settings.sort_for(path),
                    app.settings.show_hidden,
                    Arc::new(move || ctx.request_repaint()),
                );
                let slot = tabs::TabSlot {
                    browser,
                    filter_open: false,
                    filter_text: String::new(),
                    list_offset: 0.0,
                };
                if i < active {
                    app.tabs.insert(app.active_tab, Some(slot));
                    app.active_tab += 1;
                } else {
                    app.tabs.push(Some(slot));
                }
            }
        }
        app.check_for_updates_at_start();
        // Quick search: have the device's list ready before it's needed.
        if app.settings.quick_search && app.is_first_instance() {
            if Self::device_index_file().is_some_and(|f| f.is_file()) {
                app.load_saved_device_index();
            } else {
                app.ensure_device_index();
            }
            app.list_apps_if_old();
        }
        if let LoadOutcome::Invalid { error, .. } = outcome {
            app.toast_error(format!(
                "Settings couldn’t be read, so defaults are in use: {error}"
            ));
        }
        app
    }

    pub(crate) fn save_settings(&mut self) {
        self.settings.show_hidden = self.browser.filter().show_hidden;
        self.settings.last_folder = Some(self.browser.location().to_path_buf());
        self.settings.open_tabs = self.tab_locations();
        self.settings.active_tab = self.active_tab;
        if let Some(path) = &self.settings_path
            && let Err(err) = self.settings.save(path)
        {
            self.toast_error(format!("Couldn’t save settings: {err}"));
        }
    }

    /// A click on a column header: sorts, and remembers it for this folder.
    pub(crate) fn sort_by(&mut self, column: SortColumn) {
        self.browser.sort_by(column);
        let folder = self.browser.location().to_path_buf();
        self.settings.remember_sort(&folder, self.browser.sort());
        self.save_settings();
    }

    /// Re-checks git status when the folder or its contents change, at
    /// most once a second.
    fn refresh_git(&mut self, ctx: &egui::Context) {
        if !self.settings.git_status {
            self.git.clear();
            return;
        }
        let generation = self.browser.generation();
        let moved = self.git_folder != self.browser.location();
        if generation == self.git_seen.0 && !moved {
            return;
        }
        let since = self.git_seen.1.elapsed();
        if !moved && since < Duration::from_secs(1) {
            ctx.request_repaint_after(Duration::from_secs(1) - since);
            return;
        }
        self.git_seen = (generation, Instant::now());
        let wake = ctx.clone();
        self.git
            .refresh(self.browser.location(), move || wake.request_repaint());
        self.git_folder = self.browser.location().to_path_buf();
    }

    /// Whether to skip movement: the user's choice, else Windows'.
    pub(crate) fn reduce_motion(&self) -> bool {
        self.settings
            .reduce_motion
            .unwrap_or_else(|| !shell::animations_enabled())
    }

    /// Re-applies settings that affect the open folder and shortcuts.
    pub(crate) fn apply_settings(&mut self) {
        keymap::install(keymap::Keymap::new(&self.settings.shortcuts));
        let sort = self.settings.sort_for(self.browser.location());
        self.browser.set_sort(sort);
        self.browser.set_show_hidden(self.settings.show_hidden);
        self.save_settings();
    }

    /// Adds a folder to Favorites in the sidebar.
    pub(crate) fn pin(&mut self, path: PathBuf) {
        if self.settings.pinned.contains(&path) {
            return;
        }
        self.places.push(places::pinned_place(&path));
        self.settings.pinned.push(path);
        self.save_settings();
    }

    pub(crate) fn unpin(&mut self, path: &Path) {
        self.settings.pinned.retain(|p| p != path);
        self.places
            .retain(|place| !(place.icon == places::PlaceIcon::Folder && place.path == path));
        self.save_settings();
    }

    pub(crate) fn push_undo(&mut self, action: Action) {
        self.undo.push(action);
    }

    pub(crate) fn toast(&mut self, message: String, undo: bool) {
        self.view.toast = Some(Toast {
            message,
            kind: ToastKind::Success,
            undo,
            shown_at: Instant::now(),
        });
    }

    /// Runs `task` on a background thread and shows its outcome as a
    /// toast; the drive list is read again afterwards.
    pub(crate) fn background(&self, task: impl FnOnce() -> Notice + Send + 'static) {
        let tx = self.notices.0.clone();
        let ctx = self.ctx.clone();
        std::thread::Builder::new()
            .name("explorist-task".into())
            .spawn(move || {
                let _ = tx.send(task());
                ctx.request_repaint();
            })
            .ok();
    }

    /// Where the device's list of names is saved between runs.
    fn device_index_file() -> Option<PathBuf> {
        dirs::cache_dir().map(|d| d.join("Explorist").join("device-index.bin"))
    }

    /// Reads last run's list of names in the background, so searching
    /// everywhere works at once after starting.
    pub(crate) fn load_saved_device_index(&mut self) {
        if self.device_index.is_some() || self.device_index_loading.is_some() {
            return;
        }
        let Some(file) = Self::device_index_file().filter(|f| f.is_file()) else {
            return;
        };
        let (tx, rx) = std::sync::mpsc::channel();
        let ctx = self.ctx.clone();
        std::thread::Builder::new()
            .name("explorist-index-load".into())
            .spawn(move || {
                let _ = tx.send(crate::fs::index::NameIndex::load(&file));
                ctx.request_repaint();
            })
            .ok();
        self.device_index_loading = Some(rx);
    }

    /// Each frame: takes in a loaded or freshly made list, and saves a
    /// finished one.
    pub(crate) fn poll_device_index(&mut self) {
        const MAX_AGE: Duration = Duration::from_secs(15 * 60);
        if let Some(rx) = &self.device_index_loading
            && let Ok(loaded) = rx.try_recv()
        {
            self.device_index_loading = None;
            if self.device_index.is_none()
                && let Some(loaded) = loaded
            {
                // Old by definition: the next search starts a fresh one.
                let built = Instant::now()
                    .checked_sub(MAX_AGE)
                    .unwrap_or_else(Instant::now);
                self.device_index = Some((built, Arc::new(loaded)));
            }
        }
        if self.device_index_next.as_ref().is_some_and(|n| n.is_done()) {
            let next = self.device_index_next.take().expect("checked above");
            self.device_index = Some((Instant::now(), next));
            self.device_index_unsaved = true;
        }
        if self.device_index_unsaved
            && let Some((_, index)) = &self.device_index
            && index.is_done()
        {
            self.device_index_unsaved = false;
            if let Some(file) = Self::device_index_file() {
                let index = Arc::clone(index);
                std::thread::Builder::new()
                    .name("explorist-index-save".into())
                    .spawn(move || {
                        let _ = index.save(&file);
                    })
                    .ok();
            }
        }
    }

    /// Starts listing every name on the device, or again when the list is
    /// old, for searching Everywhere. A list that's being replaced keeps
    /// answering until the new one is done.
    pub(crate) fn ensure_device_index(&mut self) {
        const MAX_AGE: Duration = Duration::from_secs(15 * 60);
        if self.device_index_next.is_some() {
            return;
        }
        let refresh = match &self.device_index {
            Some((built, index)) => {
                if !index.is_done() || built.elapsed() < MAX_AGE {
                    return;
                }
                true
            }
            // Read last run's list for now, and make a fresh one behind it.
            None if self.device_index_loading.is_some() => true,
            None if Self::device_index_file().is_some_and(|f| f.is_file()) => {
                self.load_saved_device_index();
                true
            }
            None => false,
        };
        let drives = self.drives.lock().map(|d| d.clone()).unwrap_or_default();
        // Network and disc drives are slow and rarely what's wanted.
        let roots: Vec<PathBuf> = drives
            .iter()
            .filter(|d| {
                !matches!(
                    d.kind,
                    places::DriveKind::Network | places::DriveKind::Optical
                )
            })
            .map(|d| d.root.clone())
            .collect();
        let roots = if roots.is_empty() {
            vec![PathBuf::from(if cfg!(windows) { "C:\\" } else { "/" })]
        } else {
            roots
        };
        // System areas that are virtual, or the same files again.
        let skip: Vec<PathBuf> = if cfg!(target_os = "macos") {
            [
                "/System/Volumes",
                "/Volumes",
                "/dev",
                "/private/var/vm",
                "/cores",
            ]
            .iter()
            .map(PathBuf::from)
            .collect()
        } else if cfg!(windows) {
            Vec::new()
        } else {
            [
                "/proc",
                "/sys",
                "/dev",
                "/run",
                "/snap",
                "/var/lib/docker",
                "/var/lib/snapd",
            ]
            .iter()
            .map(PathBuf::from)
            .collect()
        };
        let ctx = self.ctx.clone();
        let index = Arc::new(crate::fs::index::NameIndex::start(roots, skip, move || {
            ctx.request_repaint()
        }));
        if refresh {
            self.device_index_next = Some(index);
        } else {
            self.device_index = Some((Instant::now(), index));
            self.device_index_unsaved = true;
        }
    }

    /// Reads the drive list again on the next frame, e.g. after a copy
    /// changed the free space.
    pub(crate) fn refresh_drives_soon(&mut self) {
        self.drives_read = Instant::now() - DRIVE_POLL;
    }

    fn poll_background(&mut self, ctx: &egui::Context) {
        while let Ok(notice) = self.notices.1.try_recv() {
            if notice.error {
                self.toast_error(notice.text);
            } else {
                self.toast(notice.text, false);
            }
            refresh_drives(&self.drives, ctx);
            self.drives_read = Instant::now();
        }
        if self.drives_read.elapsed() >= DRIVE_POLL {
            refresh_drives(&self.drives, ctx);
            self.drives_read = Instant::now();
        }
        ctx.request_repaint_after(DRIVE_POLL);
    }

    pub(crate) fn toast_error(&mut self, message: String) {
        self.view.toast = Some(Toast {
            message,
            kind: ToastKind::Error,
            undo: false,
            shown_at: Instant::now(),
        });
    }

    /// Whether a text field (path, filter, rename) has keyboard focus.
    fn typing(ctx: &egui::Context) -> bool {
        ctx.egui_wants_keyboard_input()
    }

    /// Turns key presses into commands. Runs before the UI is drawn so
    /// shortcuts win over widgets that would otherwise react to the keys.
    fn handle_keys(&mut self, ctx: &egui::Context) {
        // Recording a shortcut ends when Settings isn't showing, or the next
        // keys pressed anywhere would become the shortcut.
        if !self.view.settings_open {
            self.view.settings.recording = None;
            self.view.settings.recording_quick = false;
        }
        // The mouse's back and forward buttons.
        let (back, forward) = ctx.input(|i| {
            (
                i.pointer.button_pressed(egui::PointerButton::Extra1),
                i.pointer.button_pressed(egui::PointerButton::Extra2),
            )
        });
        if back {
            self.browser.go_back();
        }
        if forward {
            self.browser.go_forward();
        }
        if self.view.quick_look {
            self.quick_look_keys(ctx);
            return;
        }
        // Dialogs and the palette handle their own keys.
        if self.view.confirm_delete.is_some()
            || self
                .job
                .as_ref()
                .is_some_and(|running| running.job.asking().is_some())
            || self.view.properties.is_some()
            || self.view.open_with.is_some()
            || self.view.drive_dialog.is_some()
            || self.view.folder_icon.is_some()
            || self.view.palette.is_some()
            || self.view.batch.is_some()
        {
            return;
        }
        if self.view.settings.recording_quick {
            self.record_quick_keys(ctx);
            return;
        }
        if let Some(command) = self.view.settings.recording {
            self.record_shortcut(ctx, command);
            return;
        }
        // The start page has no list, so list keys (Enter, arrows, typing
        // to jump) do nothing there; shortcuts with Ctrl or Alt still work.
        let typing = Self::typing(ctx) || self.browser.start_page;
        let (events, mods) = ctx.input(|i| (i.events.clone(), Mods::from_egui(i.modifiers)));
        let mut consumed = Vec::new();
        for (index, event) in events.iter().enumerate() {
            let swallowed_paste = self.view.paste_detector.feed(event);
            let clip = if swallowed_paste {
                Some(Command::Paste)
            } else {
                keymap::clipboard_event(event, mods)
            };
            if let Some(command) = clip {
                // Text fields handle their own copy and paste.
                if typing {
                    continue;
                }
                consumed.push(index);
                if command == Command::Paste {
                    // A press can give both a Paste event and, on release, a
                    // V key-up; paste once.
                    let recent = self
                        .view
                        .last_paste
                        .is_some_and(|t| t.elapsed() < Duration::from_millis(800));
                    self.view.last_paste = Some(Instant::now());
                    if recent {
                        continue;
                    }
                }
                self.run(ctx, command);
                continue;
            }
            match event {
                egui::Event::Key {
                    key,
                    pressed: true,
                    modifiers,
                    ..
                } => {
                    let Some(command) = keymap::lookup(Mods::from_egui(*modifiers), *key) else {
                        continue;
                    };
                    if typing
                        && !keymap::allowed_while_typing(command, Mods::from_egui(*modifiers), *key)
                    {
                        continue;
                    }
                    consumed.push(index);
                    self.run(ctx, command);
                }
                egui::Event::Text(text) if !typing => {
                    let mods = ctx.input(|i| i.modifiers);
                    if mods.ctrl || mods.alt || mods.command {
                        continue;
                    }
                    let now = Instant::now();
                    for ch in text.chars().filter(|c| !c.is_control() && *c != ' ') {
                        self.browser
                            .select(|sel, rows| sel.type_ahead(rows, ch, now));
                        self.view.scroll_to_focus = true;
                    }
                }
                _ => {}
            }
        }
        if !consumed.is_empty() {
            ctx.input_mut(|i| {
                let mut index = 0;
                i.events.retain(|_| {
                    let keep = !consumed.contains(&index);
                    index += 1;
                    keep
                });
            });
        }
    }

    /// While Quick Look shows: Space or Esc closes it, the arrows move on.
    fn quick_look_keys(&mut self, ctx: &egui::Context) {
        use egui::{Key, Modifiers};
        let pressed = |key| ctx.input_mut(|i| i.consume_key(Modifiers::NONE, key));
        if pressed(Key::Space) || pressed(Key::Escape) {
            self.view.quick_look = false;
            return;
        }
        for (key, target) in [
            (Key::ArrowUp, MoveTarget::Up),
            (Key::ArrowDown, MoveTarget::Down),
            (Key::ArrowLeft, MoveTarget::Left),
            (Key::ArrowRight, MoveTarget::Right),
        ] {
            if pressed(key) {
                // In the list, left and right step through items too.
                let target = match (target, self.view.columns > 1) {
                    (MoveTarget::Left, false) => MoveTarget::Up,
                    (MoveTarget::Right, false) => MoveTarget::Down,
                    (t, _) => t,
                };
                self.move_focus(target, MoveMode::Replace);
            }
        }
    }

    /// Takes the next key press as the new shortcut for `command`.
    fn record_shortcut(&mut self, ctx: &egui::Context, command: Command) {
        let pressed = ctx.input_mut(|i| {
            let found = i.events.iter().find_map(|e| match e {
                egui::Event::Key {
                    key,
                    pressed: true,
                    modifiers,
                    ..
                } if !keymap::is_modifier(*key) => Some((*key, *modifiers)),
                _ => None,
            });
            if found.is_some() {
                i.events.clear();
            }
            found
        });
        let Some((key, modifiers)) = pressed else {
            return;
        };
        self.view.settings.recording = None;
        if key == egui::Key::Escape {
            return;
        }
        let mods = Mods::from_egui(modifiers);
        let mut map = keymap::Keymap::new(&self.settings.shortcuts);
        let lost = map.bind(command, mods, key);
        self.settings.shortcuts = map.overrides();
        self.apply_settings();
        let keys = keymap::format_shortcut(mods, key);
        match lost {
            Some(other) => self.toast(
                format!(
                    "{keys} now does “{}” instead of “{}”",
                    command.label(),
                    other.label()
                ),
                false,
            ),
            None => self.toast(format!("{keys} now does “{}”", command.label()), false),
        }
    }

    /// Does what a command asks.
    pub(crate) fn run(&mut self, ctx: &egui::Context, command: Command) {
        match command {
            Command::Open => self.open_selection(),
            Command::GoBack => self.browser.go_back(),
            Command::GoForward => self.browser.go_forward(),
            Command::GoUp => self.browser.go_up(),
            Command::Refresh => {
                self.browser.refresh();
                self.folder_sizes.restart = true;
                refresh_drives(&self.drives, ctx);
            }
            Command::EditPath => {
                self.view.path_edit = Some(PathEdit {
                    text: if self.browser.start_page {
                        String::new()
                    } else {
                        self.browser.location().display().to_string()
                    },
                    just_opened: true,
                    ..PathEdit::default()
                });
            }
            Command::Filter => {
                self.view.filter_open = true;
                self.view.focus_filter = true;
            }
            Command::Rename => self.start_rename(),
            Command::NewFolder => self.create(true),
            Command::NewFile => self.create(false),
            Command::Trash => self.trash_selection(),
            Command::Cut => self.clip_selection(true),
            Command::Copy => self.clip_selection(false),
            Command::Paste => self.paste(),
            Command::DeletePermanently => {
                let paths = self.selected_paths();
                if !paths.is_empty() {
                    self.view.confirm_delete = Some(paths);
                }
            }
            Command::CopyPath => {
                let paths: Vec<String> = self
                    .browser
                    .selected_entries()
                    .iter()
                    .map(|e| e.path.display().to_string())
                    .collect();
                let text = if paths.is_empty() {
                    self.browser.location().display().to_string()
                } else {
                    paths.join("\n")
                };
                ctx.copy_text(text);
                self.toast("Path copied".to_owned(), false);
            }
            Command::SelectAll => self.browser.select(|sel, rows| sel.select_all(rows)),
            Command::ToggleHidden => {
                let show = !self.browser.filter().show_hidden;
                self.browser.set_show_hidden(show);
                self.save_settings();
            }
            Command::Undo => self.undo(),
            Command::OpenTerminal => {
                if let Err(err) = shell::open_terminal(self.browser.location()) {
                    self.toast_error(format!("Couldn’t open a terminal: {err}"));
                }
            }
            Command::JumpToPlace(n) => {
                if let Some(place) = self.places.get(usize::from(n)) {
                    let path = place.path.clone();
                    self.browser.navigate(path);
                }
            }
            Command::Escape => self.escape(),
            Command::Move(target, mode) => self.move_focus(target, mode),
            Command::ToggleFocused => self.browser.select(|sel, rows| sel.toggle_focused(rows)),
            Command::OpenSettings => self.view.settings_open = true,
            Command::BatchRename => self.open_batch_rename(),
            // A fresh tab shows the start page, not a copy of this one.
            Command::NewTab => self.new_start_tab(),
            Command::CloseTab => self.close_tab(self.active_tab),
            Command::NextTab => self.cycle_tab(true),
            Command::PreviousTab => self.cycle_tab(false),
            Command::OpenInNewTab => {
                let folders: Vec<PathBuf> = self
                    .browser
                    .selected_entries()
                    .iter()
                    .filter(|e| e.is_dir)
                    .map(|e| e.path.clone())
                    .collect();
                for folder in folders {
                    self.new_tab(folder);
                }
            }
            Command::CommandPalette => {
                self.view.palette = Some(crate::ui::palette::PaletteState::new(""));
            }
            Command::RecentPlaces => {
                self.view.palette = Some(crate::ui::palette::PaletteState::new("@"));
            }
            Command::TogglePreview => {
                self.settings.show_preview = !self.settings.show_preview;
                self.save_settings();
            }
            Command::OpenAsAdmin => {
                #[cfg(windows)]
                if let Err(err) = crate::fs::shell::open_as_admin(self.browser.location()) {
                    self.toast_error(format!("Couldn’t open as administrator: {err}"));
                }
            }
            Command::FolderIcon => {
                let folders: Vec<PathBuf> = self
                    .browser
                    .selected_entries()
                    .iter()
                    .filter(|e| e.is_dir)
                    .map(|e| e.path.clone())
                    .collect();
                if folders.is_empty() {
                    self.toast("Select a folder to change its icon".to_owned(), false);
                } else {
                    self.view.folder_icon =
                        Some(ui::folder_icon::FolderIconState::new(self, folders));
                }
            }
            Command::Duplicate => self.duplicate_selection(),
            Command::Compress => self.compress_selection(),
            Command::Extract => self.extract_selection(),
            Command::NewFolderWithSelection => self.new_folder_with_selection(),
            Command::SelectNone => self.browser.select(|sel, _| sel.clear()),
            Command::InvertSelection => self.browser.select(|sel, rows| sel.invert(rows)),
            Command::QuickLook => {
                self.view.quick_look = !self.view.quick_look
                    && self
                        .browser
                        .selection
                        .focused_row(self.browser.rows())
                        .is_some();
            }
            Command::OpenInNewWindow => {
                let folders: Vec<PathBuf> = self
                    .browser
                    .selected_entries()
                    .iter()
                    .filter(|e| e.is_dir)
                    .map(|e| e.path.clone())
                    .collect();
                let target = folders
                    .first()
                    .cloned()
                    .unwrap_or_else(|| self.browser.location().to_path_buf());
                let started = std::env::current_exe()
                    .and_then(|exe| std::process::Command::new(exe).arg(&target).spawn());
                if let Err(err) = started {
                    self.toast_error(format!("Couldn’t open a new window: {err}"));
                }
            }
            Command::OpenWith => {
                let files: Vec<PathBuf> = self
                    .browser
                    .selected_entries()
                    .iter()
                    .filter(|e| !e.is_dir)
                    .map(|e| e.path.clone())
                    .collect();
                if files.is_empty() {
                    self.toast("Select a file to open".to_owned(), false);
                } else if crate::fs::open_with::SYSTEM_CHOOSER {
                    #[cfg(windows)]
                    crate::fs::open_with::choose(&files[0]);
                } else {
                    self.view.open_with = Some(ui::open_with::OpenWithState::new(files));
                }
            }
            Command::Properties => {
                let entries = ui::properties::targets(self);
                if !entries.is_empty() {
                    self.view.properties = Some(ui::properties::PropertiesState::new(ctx, entries));
                }
            }
            Command::ToggleGrid => {
                self.settings.grid_view = !self.settings.grid_view;
                self.view.scroll_to_focus = true;
                self.save_settings();
            }
            Command::PinFolder => {
                let folders: Vec<PathBuf> = self
                    .browser
                    .selected_entries()
                    .iter()
                    .filter(|e| e.is_dir)
                    .map(|e| e.path.clone())
                    .collect();
                for folder in folders {
                    self.pin(folder);
                }
            }
        }
    }

    /// Esc closes the innermost thing that's open.
    fn escape(&mut self) {
        if self.view.settings_open {
            self.view.settings_open = false;
            return;
        }
        if self.view.rename.take().is_some() || self.view.path_edit.take().is_some() {
            return;
        }
        if self.browser.is_loading() {
            self.browser.cancel_loading();
        } else if let Some(running) = &self.job {
            running.job.cancel();
        } else if self.view.filter_open {
            self.view.filter_open = false;
            self.view.filter_text.clear();
            self.browser.clear_filter();
        } else {
            self.browser.select(|sel, _| sel.clear());
        }
    }

    fn move_focus(&mut self, target: MoveTarget, mode: MoveMode) {
        // In the grid, up and down jump a whole line of tiles.
        let columns = self.view.columns.max(1) as isize;
        let page = self.view.page_rows.max(1) as isize * columns;
        if matches!(target, MoveTarget::Left | MoveTarget::Right) && columns == 1 {
            return;
        }
        self.browser.select(|sel, rows| match target {
            MoveTarget::Up => sel.move_by(rows, -columns, mode),
            MoveTarget::Down => sel.move_by(rows, columns, mode),
            MoveTarget::Left => sel.move_by(rows, -1, mode),
            MoveTarget::Right => sel.move_by(rows, 1, mode),
            MoveTarget::PageUp => sel.move_by(rows, -page, mode),
            MoveTarget::PageDown => sel.move_by(rows, page, mode),
            MoveTarget::First => sel.move_to(rows, 0, mode),
            MoveTarget::Last => sel.move_to(rows, usize::MAX, mode),
        });
        self.view.scroll_to_focus = true;
    }

    /// Enter or double-click: a single folder opens in place; files open in
    /// their default apps.
    pub(crate) fn open_selection(&mut self) {
        let selected: Vec<Entry> = self
            .browser
            .selected_entries()
            .into_iter()
            .cloned()
            .collect();
        if let [only] = selected.as_slice()
            && only.is_dir
        {
            self.browser.navigate(only.path.clone());
            return;
        }
        for entry in selected.iter().filter(|e| !e.is_dir) {
            if let Err(err) = shell::open_default(&entry.path) {
                self.toast_error(format!("Couldn’t open “{}”: {err}", entry.name));
            }
        }
    }

    /// Navigates to what was typed in the path bar.
    pub(crate) fn submit_path(&mut self) {
        let Some(edit) = &mut self.view.path_edit else {
            return;
        };
        // Nothing typed: just close the field.
        if edit.text.trim().is_empty() {
            self.view.path_edit = None;
            return;
        }
        match path_input::resolve(&edit.text, self.browser.location()) {
            Ok(Target::Folder(path)) => {
                self.view.path_edit = None;
                self.browser.navigate(path);
            }
            Ok(Target::File(path)) => {
                self.view.path_edit = None;
                if let Some(parent) = path.parent() {
                    self.browser.navigate(parent.to_path_buf());
                    self.browser.select_when_loaded(path);
                }
            }
            Err(_) if !path_input::looks_like_path(&edit.text) => {
                // Words rather than a path: search for files by that name.
                let query = format!("/{}", edit.text.trim());
                self.view.path_edit = None;
                let mut palette = crate::ui::palette::PaletteState::new(&query);
                palette.selected = 0;
                self.view.palette = Some(palette);
            }
            Err(message) => edit.error = Some(message),
        }
    }

    fn start_rename(&mut self) {
        let focus = self.browser.selection.focus().map(Path::to_path_buf);
        let entry = focus
            .and_then(|f| {
                self.browser
                    .selected_entries()
                    .into_iter()
                    .find(|e| e.path == f)
                    .cloned()
            })
            .or_else(|| {
                self.browser
                    .selected_entries()
                    .first()
                    .map(|e| (*e).clone())
            });
        if let Some(entry) = entry {
            self.begin_rename(&entry);
        }
    }

    fn begin_rename(&mut self, entry: &Entry) {
        self.browser.selection.select_only(&entry.path);
        self.view.rename = Some(Rename {
            path: entry.path.clone(),
            is_dir: entry.is_dir,
            text: entry.name.clone(),
            error: None,
            just_opened: true,
        });
        self.view.scroll_to_focus = true;
    }

    /// Applies the name typed in the rename field. Keeps the field open
    /// with an error message if the name can't be used.
    pub(crate) fn commit_rename(&mut self) -> bool {
        let Some(rename) = &mut self.view.rename else {
            return true;
        };
        let current = rename
            .path
            .file_name()
            .map(|n| n.to_string_lossy().into_owned())
            .unwrap_or_default();
        if rename.text == current {
            self.view.rename = None;
            return true;
        }
        match ops::rename(&rename.path, &rename.text) {
            Ok(to) => {
                let from = rename.path.clone();
                self.view.rename = None;
                match Entry::from_path(&to) {
                    Ok(entry) => self
                        .browser
                        .apply_change(std::slice::from_ref(&from), vec![entry]),
                    Err(_) => self.browser.refresh(),
                }
                self.browser.selection.select_only(&to);
                self.carry_folder_icons(&[(from.clone(), to.clone())]);
                let action = Action::Rename { from, to };
                self.toast(action.done_message(), true);
                self.undo.push(action);
                self.view.scroll_to_focus = true;
                true
            }
            Err(err) => {
                rename.error = Some(err.to_string());
                false
            }
        }
    }

    fn create(&mut self, folder: bool) {
        let dir = self.browser.location().to_path_buf();
        let result = if folder {
            ops::create_folder(&dir, &ops::unique_name(&dir, "New folder", ""))
        } else {
            ops::create_file(&dir, &ops::unique_name(&dir, "New file", "txt"))
        };
        match result.and_then(|path| Ok(Entry::from_path(&path)?)) {
            Ok(entry) => {
                self.browser.apply_change(&[], vec![entry.clone()]);
                self.undo.push(Action::Create {
                    path: entry.path.clone(),
                    is_dir: folder,
                });
                self.begin_rename(&entry);
            }
            Err(err) => self.toast_error(format!("Couldn’t create it: {err}")),
        }
    }

    fn current_palette(&self, ctx: &egui::Context) -> Palette {
        let dark = match self.settings.theme {
            Theme::Dark => true,
            Theme::Light => false,
            Theme::System => ctx.system_theme() != Some(egui::Theme::Light),
        };
        if dark {
            Palette::dark(self.settings.accent)
        } else {
            Palette::light(self.settings.accent)
        }
    }

    fn expire_toast(&mut self, ctx: &egui::Context, hovered: bool) {
        if let Some(toast) = &mut self.view.toast {
            if hovered {
                toast.shown_at = Instant::now();
            }
            let age = toast.shown_at.elapsed();
            if age >= TOAST_DWELL {
                self.view.toast = None;
            } else {
                ctx.request_repaint_after(TOAST_DWELL - age);
            }
        }
    }
}

impl eframe::App for ExploristApp {
    /// Runs before every frame, and also while the window is hidden in the
    /// tray (when `ui` doesn't run at all), so the tray icon, a second
    /// launch and the search shortcut can still bring things up.
    fn logic(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        self.update_tray(ctx);
        self.check_for_updates_regularly();
        self.poll_device_index();
        self.quick_search_logic(ctx);
    }

    fn raw_input_hook(&mut self, ctx: &egui::Context, raw_input: &mut egui::RawInput) {
        if std::mem::take(&mut self.view.release_pointer) {
            let (pos, modifiers) = ctx.input(|i| {
                (
                    i.pointer.latest_pos().unwrap_or(egui::Pos2::ZERO),
                    i.modifiers,
                )
            });
            raw_input.events.push(egui::Event::PointerButton {
                pos,
                button: egui::PointerButton::Primary,
                pressed: false,
                modifiers,
            });
            raw_input.events.push(egui::Event::PointerGone);
        }
    }

    fn ui(&mut self, ui: &mut egui::Ui, frame: &mut eframe::Frame) {
        let ctx = ui.ctx().clone();
        let palette = self.current_palette(&ctx);
        if palette != self.palette || ctx.cumulative_frame_nr() == 0 {
            self.palette = palette;
            ui::theme::apply(&ctx, &palette);
        }
        let animation_time = if self.reduce_motion() { 0.0 } else { 0.08 };
        if ctx.global_style().animation_time != animation_time {
            ctx.global_style_mut(|s| s.animation_time = animation_time);
        }
        self.view.maximized = ctx.input(|i| i.viewport().maximized.unwrap_or(false));

        self.browser.poll();
        self.poll_background_tabs();
        if self.view.last_location != self.browser.location() {
            // Each folder opens with its own remembered sort.
            self.view.last_location = self.browser.location().to_path_buf();
            let here = self.view.last_location.clone();
            self.recent.retain(|p| *p != here);
            self.recent.insert(0, here);
            self.recent.truncate(20);
            let sort = self.settings.sort_for(&self.view.last_location);
            if sort != self.browser.sort() {
                self.browser.set_sort(sort);
            }
        }
        self.poll_job();
        self.update_folder_sizes();
        self.poll_background(&ctx);
        self.refresh_git(&ctx);
        self.handle_dropped_files(&ctx);
        if let Some(wait) = self.browser.repaint_in() {
            ctx.request_repaint_after(wait);
        }
        self.handle_keys(&ctx);

        ui::window::resize_handles(&ctx, self.view.maximized);
        ui::titlebar::show(ui, self);
        if self.view.settings_open {
            ui::settings::show(ui, self);
        } else {
            ui::toolbar::show(ui, self);
            ui::status_bar::show(ui, self);
            ui::sidebar::show(ui, self);
            if self.settings.show_preview {
                ui::preview::show(ui, self);
            }
            ui::file_list::show(ui, self);
        }
        self.finish_drag(&ctx, frame);

        let toast_hovered = ui::toast::show(&ctx, self);
        self.expire_toast(&ctx, toast_hovered);
        ui::dialogs::confirm_delete(&ctx, self);
        ui::dialogs::existing_files(&ctx, self);
        ui::properties::show(&ctx, self);
        ui::open_with::show(&ctx, self);
        ui::drive_dialog::show(&ctx, self);
        ui::folder_icon::show(&ctx, self);
        ui::quick_look::show(&ctx, self);
        ui::palette::show(&ctx, self);
        ui::batch_rename::show(&ctx, self);
        ui::update::show(&ctx, self);
        self.quick_search_ui(&ctx);
    }

    fn on_exit(&mut self, _gl: Option<&eframe::glow::Context>) {
        self.save_settings();
    }
}

impl ExploristApp {
    /// Close to tray: keeps the icon in step with the setting, hides the
    /// window instead of closing it, and handles the icon's clicks.
    fn update_tray(&mut self, ctx: &egui::Context) {
        let wanted = self.settings.close_to_tray && self.instance.is_some();
        if wanted && self.tray.is_none() && !self.tray_unavailable {
            self.tray = tray::Tray::new(ctx);
            self.tray_unavailable = self.tray.is_none();
        } else if !wanted {
            self.tray = None;
        }
        if let Some(listener) = &self.instance {
            listener.set_accepting(self.tray.is_some(), self.settings.quick_search);
            if listener.take_search() {
                crate::ui::quick_search::open_soon();
            }
        }

        let clicked = self.tray.as_ref().is_some_and(tray::Tray::take_show);
        let reopened = self
            .instance
            .as_ref()
            .is_some_and(instance::Listener::take_show);
        if clicked || reopened {
            // Like starting Explorist: see if there's an update.
            self.check_for_updates_on_reopen();
            ctx.send_viewport_cmd(egui::ViewportCommand::Visible(true));
            ctx.send_viewport_cmd(egui::ViewportCommand::Minimized(false));
            ctx.send_viewport_cmd(egui::ViewportCommand::Focus);
            tray::set_dock_icon(true);
        }
        if let Some(tray) = &self.tray {
            tray.show_search(
                self.settings.quick_search,
                &crate::ui::quick_search::display_keys(&self.settings.quick_search_keys),
            );
            if tray.take_search() && self.settings.quick_search {
                crate::ui::quick_search::open_soon();
            }
        }
        if self.tray.as_ref().is_some_and(tray::Tray::take_quit) {
            self.quit(ctx);
        }
        if ctx.input(|i| i.viewport().close_requested()) && self.tray.is_some() && !self.quitting {
            ctx.send_viewport_cmd(egui::ViewportCommand::CancelClose);
            ctx.send_viewport_cmd(egui::ViewportCommand::Visible(false));
            tray::set_dock_icon(false);
            // Saved as if closing, in case it's never opened again.
            self.save_settings();
        }
    }

    /// This is the first Explorist running (later ones hand over to it, or
    /// are extra windows).
    pub(crate) fn is_first_instance(&self) -> bool {
        self.instance.is_some()
    }

    /// The device's list of names is being made again behind the scenes.
    pub(crate) fn device_index_refreshing(&self) -> bool {
        self.device_index_next.is_some()
    }

    /// Closes the window for real, even with Close to tray on.
    pub(crate) fn quit(&mut self, ctx: &egui::Context) {
        self.quitting = true;
        ctx.send_viewport_cmd(egui::ViewportCommand::Close);
    }
}

/// Reads the drive list on a background thread.
fn refresh_drives(drives: &Arc<Mutex<Vec<Drive>>>, ctx: &egui::Context) {
    let drives = Arc::clone(drives);
    let ctx = ctx.clone();
    std::thread::Builder::new()
        .name("explorist-drives".into())
        .spawn(move || {
            let list = places::drives();
            if let Ok(mut guard) = drives.lock()
                && *guard != list
            {
                *guard = list;
                ctx.request_repaint();
            }
        })
        .ok();
}
