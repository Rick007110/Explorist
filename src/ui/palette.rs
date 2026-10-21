//! The command palette (Ctrl P): one box for actions, places and files.
//! Prefixes narrow it: `>` commands, `@` places, `/` files.

use std::path::PathBuf;

use egui::text::LayoutJob;
use egui::{
    Align2, Area, Color32, CornerRadius, Frame, Id, Key, Margin, Modifiers, Order, Pos2, Rect,
    Sense, Stroke, StrokeKind, TextEdit, TextFormat, vec2,
};

use crate::app::ExploristApp;
use crate::app::commands::Command;
use crate::app::fuzzy::fuzzy_match;
use crate::app::keymap;
use crate::fs::index::NameIndex;
use crate::fs::places::PlaceIcon;
use crate::ui::icons::{self, Icon};
use crate::ui::theme::{Palette, radius, text};
use crate::ui::widgets;

const WIDTH: f32 = 640.0;
const MAX_ROWS: usize = 12;

/// Palette state while it's open.
pub struct PaletteState {
    pub query: String,
    pub selected: usize,
    pub just_opened: bool,
    /// Everything in the open folder, for "This folder" searches.
    index: Option<NameIndex>,
    /// Results for a query, scope and index state, to skip searching again
    /// every frame.
    cache: Vec<Item>,
    cached_key: Option<(String, bool, u64)>,
    searched_at: Option<std::time::Instant>,
}

impl PaletteState {
    pub fn new(query: &str) -> Self {
        Self {
            query: query.to_owned(),
            selected: 0,
            just_opened: true,
            index: None,
            cache: Vec::new(),
            cached_key: None,
            searched_at: None,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Mode {
    All,
    Commands,
    Places,
    Files,
}

impl Mode {
    fn of(query: &str) -> (Mode, &str) {
        match query.chars().next() {
            Some('>') => (Mode::Commands, &query[1..]),
            Some('@') => (Mode::Places, &query[1..]),
            Some('/') => (Mode::Files, &query[1..]),
            _ => (Mode::All, query),
        }
    }

    fn chip(self) -> Option<&'static str> {
        match self {
            Mode::All => None,
            Mode::Commands => Some("> Commands"),
            Mode::Places => Some("@ Places"),
            Mode::Files => Some("/ Files"),
        }
    }
}

#[derive(Debug, Clone)]
enum Target {
    Command(Command),
    Folder(PathBuf),
    File(PathBuf),
}

#[derive(Debug, Clone)]
struct Item {
    group: String,
    label: String,
    mono: bool,
    icon: Icon,
    icon_accent: bool,
    hint: String,
    keys: Option<String>,
    indices: Vec<usize>,
    score: i32,
    target: Target,
}

/// Builds the results for the current query.
fn results(app: &mut ExploristApp) -> Vec<Item> {
    let Some(state) = &mut app.view.palette else {
        return Vec::new();
    };
    let (mode, query) = Mode::of(&state.query);
    let query = query.trim().to_owned();
    let folder_name = app
        .browser
        .location()
        .file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_else(|| app.browser.location().display().to_string());

    // Start listing files the first time they're needed: the open folder,
    // or the whole device.
    let everywhere = app.settings.search_everywhere;
    let wants_files = matches!(mode, Mode::All | Mode::Files);
    if wants_files && everywhere {
        app.ensure_device_index();
    }
    let Some(state) = &mut app.view.palette else {
        return Vec::new();
    };
    if wants_files
        && !everywhere
        && state
            .index
            .as_ref()
            .is_none_or(|i| i.roots() != [app.browser.location().to_path_buf()])
    {
        let ctx = app.ctx.clone();
        state.index = Some(NameIndex::start(
            vec![app.browser.location().to_path_buf()],
            Vec::new(),
            move || ctx.request_repaint(),
        ));
    }
    let index: Option<&NameIndex> = if everywhere {
        app.device_index.as_ref().map(|(_, i)| &**i)
    } else {
        state.index.as_ref()
    };
    let generation = index.map_or(0, NameIndex::generation);
    let key = (state.query.clone(), everywhere, generation);
    if state.cached_key.as_ref() == Some(&key) {
        return state.cache.clone();
    }
    // While the list is still growing, search again at most a few times a
    // second; a new query always searches right away.
    let same_query = state
        .cached_key
        .as_ref()
        .is_some_and(|(q, e, _)| *q == key.0 && *e == key.1);
    if same_query
        && state
            .searched_at
            .is_some_and(|t| t.elapsed().as_millis() < 250)
    {
        app.ctx
            .request_repaint_after(std::time::Duration::from_millis(250));
        return state.cache.clone();
    }

    let mut out = Vec::new();
    let selected = app.browser.selection.count();
    let limit = |m: Mode, all: usize| if mode == m { 50 } else { all };

    if matches!(mode, Mode::All | Mode::Commands) {
        let mut found: Vec<Item> = Command::PALETTE
            .into_iter()
            .filter(|c| selected > 0 || !c.needs_selection())
            .filter_map(|c| {
                let m = fuzzy_match(&query, c.label())?;
                let hint = if c.needs_selection() {
                    if selected == 1 {
                        "1 item selected".to_owned()
                    } else {
                        format!("{selected} items selected")
                    }
                } else {
                    String::new()
                };
                Some(Item {
                    group: "Actions".to_owned(),
                    label: c.label().to_owned(),
                    mono: false,
                    icon: command_icon(c),
                    icon_accent: false,
                    hint,
                    keys: keymap::shortcut_label(c),
                    indices: m.indices,
                    score: m.score,
                    target: Target::Command(c),
                })
            })
            .collect();
        if !query.is_empty() {
            found.sort_by_key(|item| std::cmp::Reverse(item.score));
        }
        found.truncate(limit(Mode::Commands, if query.is_empty() { 6 } else { 4 }));
        out.extend(found);
    }

    if matches!(mode, Mode::All | Mode::Places) {
        let drives = app.drives.lock().map(|d| d.clone()).unwrap_or_default();
        let mut places: Vec<(String, PathBuf, Icon)> = Vec::new();
        for path in &app.recent {
            places.push((path.display().to_string(), path.clone(), Icon::Folder));
        }
        for place in &app.places {
            let icon = match place.icon {
                PlaceIcon::Home => Icon::Home,
                PlaceIcon::Desktop => Icon::Desktop,
                PlaceIcon::Downloads => Icon::Downloads,
                PlaceIcon::Documents => Icon::Documents,
                PlaceIcon::Folder => Icon::Folder,
            };
            places.push((place.label.clone(), place.path.clone(), icon));
        }
        for drive in drives {
            places.push((drive.display_name(), drive.root.clone(), Icon::Drive));
        }
        let mut seen = std::collections::HashSet::new();
        let mut found: Vec<Item> = places
            .into_iter()
            .filter(|(_, path, _)| seen.insert(path.clone()))
            .filter_map(|(label, path, icon)| {
                let m = fuzzy_match(&query, &label)?;
                Some(Item {
                    group: "Places".to_owned(),
                    hint: if label == path.display().to_string() {
                        "Recent".to_owned()
                    } else {
                        path.display().to_string()
                    },
                    mono: icon == Icon::Folder,
                    label,
                    icon,
                    icon_accent: false,
                    keys: None,
                    indices: m.indices,
                    score: m.score,
                    target: Target::Folder(path),
                })
            })
            .collect();
        if !query.is_empty() {
            found.sort_by_key(|item| std::cmp::Reverse(item.score));
        }
        found.truncate(limit(
            Mode::Places,
            if mode == Mode::All && query.is_empty() {
                0
            } else {
                5
            },
        ));
        out.extend(found);
    }

    if wants_files && !query.is_empty() {
        let listing = index.is_some_and(|i| !i.is_done());
        let mut group = if everywhere {
            "Files on this device".to_owned()
        } else {
            format!("Files in {folder_name}")
        };
        if listing && let Some(index) = index {
            group.push_str(&format!(
                " · listing, {} so far",
                crate::fs::format::count(index.len() as u64)
            ));
        }
        let hits = index.map_or_else(Vec::new, |i| i.search(&query, limit(Mode::Files, 8)));
        out.extend(hits.into_iter().map(|hit| {
            // The name leads; where it is follows, dimmed: the folder below
            // the open one, or the full folder path when searching
            // everywhere.
            let label = hit.path.file_name().map_or_else(
                || hit.path.display().to_string(),
                |n| n.to_string_lossy().into_owned(),
            );
            let hint = if everywhere {
                hit.path
                    .parent()
                    .map(|p| p.display().to_string())
                    .unwrap_or_default()
            } else {
                hit.relative
                    .trim_end_matches('/')
                    .rsplit_once('/')
                    .map(|(dir, _)| dir.to_owned())
                    .unwrap_or_default()
            };
            let offset = 0;
            Item {
                group: group.clone(),
                hint,
                mono: true,
                icon: if hit.is_dir { Icon::Folder } else { Icon::File },
                icon_accent: hit.is_dir,
                keys: None,
                score: hit.score,
                indices: hit
                    .matches
                    .iter()
                    .flat_map(|&(a, b)| a + offset..b + offset)
                    .collect(),
                label,
                target: if hit.is_dir {
                    Target::Folder(hit.path)
                } else {
                    Target::File(hit.path)
                },
            }
        }));
    }

    let Some(state) = &mut app.view.palette else {
        return out;
    };
    state.cache = out.clone();
    state.cached_key = Some(key);
    state.searched_at = Some(std::time::Instant::now());
    out
}

fn command_icon(command: Command) -> Icon {
    match command {
        Command::Open => Icon::Forward,
        Command::Rename | Command::EditPath => Icon::Text,
        Command::NewFile => Icon::File,
        Command::NewFolder | Command::PinFolder => Icon::Folder,
        Command::Trash | Command::DeletePermanently => Icon::Close,
        Command::Filter => Icon::Filter,
        Command::Refresh | Command::Undo => Icon::Refresh,
        Command::GoBack => Icon::Back,
        Command::GoForward => Icon::Forward,
        Command::GoUp => Icon::Up,
        Command::TogglePreview => Icon::PreviewPanel,
        Command::ToggleGrid => Icon::Grid,
        Command::Properties => Icon::File,
        Command::OpenWith => Icon::Exe,
        Command::FolderIcon => Icon::Folder,
        Command::OpenSettings => Icon::Settings,
        Command::OpenTerminal => Icon::Exe,
        Command::RecentPlaces => Icon::Home,
        _ => Icon::Check,
    }
}

pub fn show(ctx: &egui::Context, app: &mut ExploristApp) {
    if app.view.palette.is_none() {
        return;
    }
    let p = app.palette;
    let items = results(app);

    // Keys the list handles before the text field sees them.
    let (up, down, enter, escape, tab, scope_key) = ctx.input_mut(|i| {
        (
            i.consume_key(Modifiers::NONE, Key::ArrowUp),
            i.consume_key(Modifiers::NONE, Key::ArrowDown),
            i.consume_key(Modifiers::NONE, Key::Enter),
            i.consume_key(Modifiers::NONE, Key::Escape),
            i.consume_key(Modifiers::NONE, Key::Tab),
            i.consume_key(Modifiers::COMMAND, Key::E),
        )
    });
    let everywhere = app.settings.search_everywhere;
    let device_listing = app.device_index.as_ref().is_some_and(|(_, i)| !i.is_done());
    let mut switch_scope = scope_key;
    let Some(state) = &mut app.view.palette else {
        return;
    };
    let shown = items.len().min(MAX_ROWS.max(items.len()));
    if down && shown > 0 {
        state.selected = (state.selected + 1) % shown;
    }
    if up && shown > 0 {
        state.selected = (state.selected + shown - 1) % shown;
    }
    state.selected = state.selected.min(shown.saturating_sub(1));
    if tab && let Some(item) = items.get(state.selected) {
        let (mode, _) = Mode::of(&state.query);
        let prefix = match mode {
            Mode::Commands => ">",
            Mode::Places => "@",
            Mode::Files => "/",
            Mode::All => "",
        };
        state.query = format!("{prefix}{}", item.label);
        state.just_opened = true;
    }
    if escape {
        app.view.palette = None;
        return;
    }
    let mut chosen = if enter {
        items.get(state.selected).cloned()
    } else {
        None
    };

    // Scrim: dims everything behind the palette.
    let screen = ctx.content_rect();
    ctx.layer_painter(egui::LayerId::new(Order::Middle, Id::new("palette-scrim")))
        .rect_filled(screen, 0.0, p.scrim);

    let Some(state) = &mut app.view.palette else {
        return;
    };
    let (mode, query_text) = Mode::of(&state.query);
    let query_text = query_text.to_owned();
    let left = screen.center().x - WIDTH / 2.0;
    let area = Area::new(Id::new("palette"))
        .order(Order::Foreground)
        .fixed_pos(Pos2::new(left, screen.top() + 108.0))
        .show(ctx, |ui| {
            Frame::NONE
                .fill(p.surface)
                .stroke(Stroke::new(1.0, p.border))
                .corner_radius(CornerRadius::same(radius::LG))
                .shadow(p.shadow_modal())
                .show(ui, |ui| {
                    ui.set_width(WIDTH);
                    ui.spacing_mut().item_spacing = vec2(0.0, 0.0);

                    // Input row.
                    let (row, _) = ui.allocate_exact_size(vec2(WIDTH, 52.0), Sense::hover());
                    ui.painter().hline(
                        row.x_range(),
                        row.bottom() - 0.5,
                        Stroke::new(1.0, p.border),
                    );
                    let mut x = row.left() + 16.0;
                    if let Some(chip) = mode.chip() {
                        let g = ui.painter().layout_no_wrap(
                            chip.to_owned(),
                            crate::ui::fonts::mono_medium(12.0),
                            p.accent,
                        );
                        let r = Rect::from_min_size(
                            Pos2::new(x, row.center().y - 11.0),
                            vec2(g.size().x + 14.0, 22.0),
                        );
                        ui.painter()
                            .rect_filled(r, CornerRadius::same(5), p.accent_alpha(0.14));
                        ui.painter().galley(
                            r.min + vec2(7.0, 11.0 - g.size().y / 2.0),
                            g,
                            p.accent,
                        );
                        x = r.right() + 10.0;
                    }
                    let esc = widgets::kbd_galley(ui, &p, "Esc");
                    let esc_left = row.right() - 14.0 - widgets::kbd_width(&esc);
                    widgets::paint_kbd(ui.painter(), &p, esc, esc_left, row.center().y, false);
                    let count = match items.len() {
                        1 => "1 result".to_owned(),
                        n => format!("{n} results"),
                    };
                    let count_rect = ui.painter().text(
                        Pos2::new(esc_left - 10.0, row.center().y),
                        Align2::RIGHT_CENTER,
                        count,
                        text::caption(),
                        p.muted,
                    );
                    // Where files are searched: this folder or everywhere.
                    let mut count_rect = count_rect;
                    if matches!(mode, Mode::All | Mode::Files) {
                        let mut right = count_rect.left() - 12.0;
                        for (label, on) in
                            [("Everywhere", everywhere), ("This folder", !everywhere)]
                        {
                            let g = ui.painter().layout_no_wrap(
                                label.to_owned(),
                                crate::ui::fonts::medium(11.5),
                                if on { p.accent } else { p.text_2 },
                            );
                            let pill = Rect::from_min_max(
                                Pos2::new(right - g.size().x - 16.0, row.center().y - 11.0),
                                Pos2::new(right, row.center().y + 11.0),
                            );
                            let response = ui
                                .interact(pill, ui.id().with(("scope", label)), Sense::click())
                                .on_hover_text("Where to search for files (Ctrl E)");
                            let fill = if on {
                                p.accent_alpha(0.14)
                            } else if response.hovered() {
                                p.hover
                            } else {
                                Color32::TRANSPARENT
                            };
                            ui.painter().rect_filled(pill, CornerRadius::same(5), fill);
                            ui.painter().galley(
                                Pos2::new(pill.left() + 8.0, pill.center().y - g.size().y / 2.0),
                                g,
                                p.text_2,
                            );
                            if response.clicked() && !on {
                                switch_scope = true;
                            }
                            right = pill.left() - 4.0;
                        }
                        count_rect.min.x = right + 4.0;
                    }

                    // The field edits the text after the prefix.
                    let mut typed = query_text.clone();
                    let field = Rect::from_min_max(
                        Pos2::new(x, row.center().y - 10.0),
                        Pos2::new(count_rect.left() - 10.0, row.center().y + 10.0),
                    );
                    let mut child = ui.new_child(egui::UiBuilder::new().max_rect(field));
                    let output = TextEdit::singleline(&mut typed)
                        .hint_text("Search commands, places and files")
                        .font(crate::ui::fonts::regular(15.0))
                        .text_color(p.text)
                        .frame(Frame::NONE)
                        .margin(Margin::ZERO)
                        .desired_width(field.width())
                        .show(&mut child);
                    if state.just_opened {
                        state.just_opened = false;
                        output.response.request_focus();
                        let mut s = output.state.clone();
                        let end = egui::text::CCursor::new(typed.chars().count());
                        s.cursor
                            .set_char_range(Some(egui::text::CCursorRange::one(end)));
                        s.store(ui.ctx(), output.response.id);
                    }
                    if !output.response.has_focus() {
                        output.response.request_focus();
                    }
                    if typed != query_text {
                        // Typing a prefix into an empty box switches mode.
                        state.query = match mode {
                            Mode::All => typed,
                            Mode::Commands => format!(">{typed}"),
                            Mode::Places => format!("@{typed}"),
                            Mode::Files => format!("/{typed}"),
                        };
                        state.selected = 0;
                    } else if typed.is_empty()
                        && mode != Mode::All
                        && ui.input(|i| i.key_pressed(Key::Backspace))
                    {
                        state.query.clear();
                    }

                    // Results.
                    Frame::NONE.inner_margin(Margin::same(6)).show(ui, |ui| {
                        ui.set_width(WIDTH - 12.0);
                        ui.spacing_mut().item_spacing.y = 1.0;
                        if items.is_empty() {
                            let listing = if everywhere {
                                device_listing
                            } else {
                                state.index.as_ref().is_some_and(|i| !i.is_done())
                            };
                            let message = if listing {
                                "Searching…"
                            } else if matches!(mode, Mode::Files) && query_text.trim().is_empty() {
                                "Type a file or folder name"
                            } else {
                                "No matches"
                            };
                            let (r, _) =
                                ui.allocate_exact_size(vec2(WIDTH - 12.0, 40.0), Sense::hover());
                            ui.painter().text(
                                r.center(),
                                Align2::CENTER_CENTER,
                                message,
                                text::body(),
                                p.muted,
                            );
                        }
                        let max_height =
                            (screen.height() - 108.0 - 52.0 - 36.0 - 60.0).clamp(160.0, 460.0);
                        egui::ScrollArea::vertical()
                            .id_salt("palette-results")
                            .max_height(max_height)
                            .auto_shrink([false, true])
                            .show(ui, |ui| {
                                let mut group = String::new();
                                for (i, item) in items.iter().enumerate() {
                                    if item.group != group {
                                        group = item.group.clone();
                                        let (r, _) = ui.allocate_exact_size(
                                            vec2(WIDTH - 12.0, if i == 0 { 26.0 } else { 30.0 }),
                                            Sense::hover(),
                                        );
                                        ui.painter().text(
                                            Pos2::new(r.left() + 10.0, r.bottom() - 9.0),
                                            Align2::LEFT_CENTER,
                                            &group,
                                            crate::ui::fonts::medium(11.5),
                                            p.text_2,
                                        );
                                    }
                                    if row_ui(ui, &p, item, i == state.selected) {
                                        chosen = Some(item.clone());
                                    }
                                }
                            });
                    });

                    // Footer.
                    let (foot, _) = ui.allocate_exact_size(vec2(WIDTH, 36.0), Sense::hover());
                    ui.painter().rect_filled(
                        foot,
                        CornerRadius {
                            nw: 0,
                            ne: 0,
                            sw: radius::LG,
                            se: radius::LG,
                        },
                        p.bg_1,
                    );
                    ui.painter().hline(
                        foot.x_range(),
                        foot.top() + 0.5,
                        Stroke::new(1.0, p.border),
                    );
                    let mut x = foot.left() + 16.0;
                    for (keys, label) in [("↑↓", "Navigate"), ("Enter", "Run"), ("Tab", "Complete")]
                    {
                        let g = widgets::kbd_galley(ui, &p, keys);
                        let r = widgets::paint_kbd(ui.painter(), &p, g, x, foot.center().y, false);
                        let t = ui.painter().text(
                            Pos2::new(r.right() + 6.0, foot.center().y),
                            Align2::LEFT_CENTER,
                            label,
                            text::caption(),
                            p.text_2,
                        );
                        x = t.right() + 16.0;
                    }
                    ui.painter().text(
                        Pos2::new(foot.right() - 16.0, foot.center().y),
                        Align2::RIGHT_CENTER,
                        "> commands   @ places   / files",
                        crate::ui::fonts::mono(11.0),
                        p.text_2,
                    );
                });
        });
    if switch_scope {
        app.settings.search_everywhere = !app.settings.search_everywhere;
        app.save_settings();
        if let Some(state) = &mut app.view.palette {
            state.selected = 0;
        }
        return;
    }
    // A click outside closes it.
    let clicked_outside = ctx.input(|i| {
        i.pointer.any_pressed()
            && i.pointer
                .interact_pos()
                .is_some_and(|pos| !area.response.rect.contains(pos))
    });
    if clicked_outside {
        app.view.palette = None;
        return;
    }

    if let Some(item) = chosen {
        app.view.palette = None;
        match item.target {
            Target::Command(command) => {
                let ctx = app.ctx.clone();
                app.run(&ctx, command);
            }
            Target::Folder(path) => {
                app.view.settings_open = false;
                app.browser.navigate(path);
            }
            Target::File(path) => {
                app.view.settings_open = false;
                if let Some(parent) = path.parent() {
                    app.browser.navigate(parent.to_path_buf());
                    app.browser.select_when_loaded(path);
                    app.view.scroll_to_focus = true;
                }
            }
        }
    }
}

/// One result row. Returns true when clicked.
fn row_ui(ui: &mut egui::Ui, p: &Palette, item: &Item, selected: bool) -> bool {
    let height = if matches!(item.target, Target::Command(_)) {
        40.0
    } else {
        36.0
    };
    let (rect, response) = ui.allocate_exact_size(vec2(WIDTH - 12.0, height), Sense::click());
    if selected {
        ui.painter()
            .rect_filled(rect, CornerRadius::same(7), p.accent_alpha(0.16));
        ui.painter().rect_stroke(
            rect,
            CornerRadius::same(7),
            Stroke::new(1.0, p.accent_alpha(0.6)),
            StrokeKind::Inside,
        );
        ui.scroll_to_rect(rect, None);
    } else if response.hovered() {
        ui.painter()
            .rect_filled(rect, CornerRadius::same(7), p.hover);
    }
    let icon_color = if item.icon_accent || (selected && matches!(item.target, Target::Command(_)))
    {
        p.accent
    } else {
        p.text_2
    };
    icons::paint(
        ui.painter(),
        Rect::from_min_size(
            Pos2::new(rect.left() + 10.0, rect.center().y - 8.0),
            vec2(16.0, 16.0),
        ),
        item.icon,
        16.0,
        icon_color,
    );

    // Right side: shortcut, then hint.
    let mut right = rect.right() - 10.0;
    if let Some(keys) = &item.keys {
        let g = widgets::kbd_galley(ui, p, keys);
        let w = widgets::kbd_width(&g);
        widgets::paint_kbd(ui.painter(), p, g, right - w, rect.center().y, false);
        right -= w + 12.0;
    }
    if !item.hint.is_empty() {
        let font = if item.mono {
            crate::ui::fonts::mono(11.5)
        } else {
            crate::ui::fonts::regular(12.0)
        };
        // Folder paths lose their start, not their end, when too long.
        let g = if item.mono {
            tail_truncated(ui, &item.hint, font, p.muted, 300.0)
        } else {
            widgets::truncated(ui, &item.hint, font, p.muted, 220.0)
        };
        let w = g.size().x;
        ui.painter().galley(
            Pos2::new(right - w, rect.center().y - g.size().y / 2.0),
            g,
            p.muted,
        );
        right -= w + 12.0;
    }

    // Label with matched characters highlighted.
    let font = if item.mono {
        crate::ui::fonts::mono(12.5)
    } else {
        crate::ui::fonts::regular(13.5)
    };
    let bold = if item.mono {
        crate::ui::fonts::mono_medium(12.5)
    } else {
        crate::ui::fonts::semibold(13.5)
    };
    // In paths, the folders leading up to the name are dimmed.
    let dir_chars = if item.mono {
        let trimmed = item.label.trim_end_matches('/');
        trimmed
            .rfind('/')
            .map_or(0, |b| trimmed[..=b].chars().count())
    } else {
        0
    };
    let mut job = LayoutJob::default();
    for (i, ch) in item.label.chars().enumerate() {
        let hit = item.indices.contains(&i);
        let color = if hit {
            p.accent
        } else if i < dir_chars {
            p.muted
        } else {
            p.text
        };
        let font_id = if hit { bold.clone() } else { font.clone() };
        job.append(
            ch.encode_utf8(&mut [0; 4]),
            0.0,
            TextFormat {
                font_id,
                color,
                ..Default::default()
            },
        );
    }
    job.wrap = egui::text::TextWrapping::truncate_at_width(right - rect.left() - 36.0 - 8.0);
    let galley = ui.painter().layout_job(job);
    ui.painter().galley(
        Pos2::new(rect.left() + 36.0, rect.center().y - galley.size().y / 2.0),
        galley,
        Color32::PLACEHOLDER,
    );
    response.clicked()
}

/// `text` in `font`, shortened from the start with "…" to fit `max_width`:
/// "…/photos/2024" rather than "/home/rick/pho…".
pub(crate) fn tail_truncated(
    ui: &egui::Ui,
    text: &str,
    font: egui::FontId,
    color: Color32,
    max_width: f32,
) -> std::sync::Arc<egui::Galley> {
    let layout = |s: String| ui.painter().layout_no_wrap(s, font.clone(), color);
    let full = layout(text.to_owned());
    if full.size().x <= max_width {
        return full;
    }
    let chars: Vec<char> = text.chars().collect();
    // Drop as few leading characters as needed.
    let (mut lo, mut hi) = (1, chars.len());
    while lo < hi {
        let mid = (lo + hi) / 2;
        let candidate: String = std::iter::once('…')
            .chain(chars[mid..].iter().copied())
            .collect();
        if layout(candidate).size().x <= max_width {
            hi = mid;
        } else {
            lo = mid + 1;
        }
    }
    layout(
        std::iter::once('…')
            .chain(chars[lo..].iter().copied())
            .collect(),
    )
}
