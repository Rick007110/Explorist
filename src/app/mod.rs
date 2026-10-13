//! Application state and the top-level eframe app.

pub mod browser;
pub mod commands;
pub mod files;
pub mod fuzzy;
pub mod keymap;
pub mod selection;
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
    pub(crate) preview_image: Option<ui::preview::PreviewImage>,
}

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
    /// Scroll the focused row into view on the next frame.
    pub scroll_to_focus: bool,
    /// Rows that fit in the list, for Page Up / Page Down.
    pub page_rows: usize,
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

pub struct PathEdit {
    pub text: String,
    pub error: Option<String>,
    pub just_opened: bool,
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
            recent: Vec::new(),
        };
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
        // Dialogs and the palette handle their own keys.
        if self.view.confirm_delete.is_some()
            || self.view.palette.is_some()
            || self.view.batch.is_some()
        {
            return;
        }
        if let Some(command) = self.view.settings.recording {
            self.record_shortcut(ctx, command);
            return;
        }
        let typing = Self::typing(ctx);
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
                    if typing && !command.works_while_typing() {
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
                refresh_drives(&self.drives, ctx);
            }
            Command::EditPath => {
                self.view.path_edit = Some(PathEdit {
                    text: self.browser.location().display().to_string(),
                    error: None,
                    just_opened: true,
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
            self.browser.set_filter_query("");
        } else {
            self.browser.select(|sel, _| sel.clear());
        }
    }

    fn move_focus(&mut self, target: MoveTarget, mode: MoveMode) {
        let page = self.view.page_rows.max(1) as isize;
        self.browser.select(|sel, rows| match target {
            MoveTarget::Up => sel.move_by(rows, -1, mode),
            MoveTarget::Down => sel.move_by(rows, 1, mode),
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
    fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
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

        let toast_hovered = ui::toast::show(&ctx, self);
        self.expire_toast(&ctx, toast_hovered);
        ui::dialogs::confirm_delete(&ctx, self);
        ui::palette::show(&ctx, self);
        ui::batch_rename::show(&ctx, self);
    }

    fn on_exit(&mut self, _gl: Option<&eframe::glow::Context>) {
        self.save_settings();
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
            if let Ok(mut guard) = drives.lock() {
                *guard = list;
            }
            ctx.request_repaint();
        })
        .ok();
}
