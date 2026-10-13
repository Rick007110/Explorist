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
use crate::fs::index::FileIndex;
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
    index: Option<FileIndex>,
    /// Results for `cached_query` with `cached_items` files indexed.
    cache: Vec<Item>,
    cached_query: Option<String>,
    cached_items: usize,
}

impl PaletteState {
    pub fn new(query: &str) -> Self {
        Self {
            query: query.to_owned(),
            selected: 0,
            just_opened: true,
            index: None,
            cache: Vec::new(),
            cached_query: None,
            cached_items: 0,
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

    // Start indexing files the first time they're needed.
    if matches!(mode, Mode::All | Mode::Files)
        && state
            .index
            .as_ref()
            .is_none_or(|i| i.root() != app.browser.location())
    {
        let ctx = app.ctx.clone();
        state.index = Some(FileIndex::start(app.browser.location(), move || {
            ctx.request_repaint()
        }));
    }
    let indexed = state.index.as_mut().map_or(0, |i| i.items().len());
    if state.cached_query.as_deref() == Some(state.query.as_str()) && state.cached_items == indexed
    {
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

    if matches!(mode, Mode::All | Mode::Files) && (!query.is_empty() || mode == Mode::Files) {
        let group = format!("Files in {folder_name}");
        let items = state
            .index
            .as_mut()
            .map(|i| i.items().to_vec())
            .unwrap_or_default();
        let mut found: Vec<Item> = items
            .into_iter()
            .filter_map(|f| {
                let m = fuzzy_match(&query, &f.relative)?;
                Some(Item {
                    group: group.clone(),
                    hint: if f.is_dir {
                        "Folder".to_owned()
                    } else {
                        String::new()
                    },
                    mono: true,
                    icon: if f.is_dir { Icon::Folder } else { Icon::File },
                    icon_accent: f.is_dir,
                    keys: None,
                    // Depth costs a little, so nearby matches come first.
                    score: m.score - f.relative.matches('/').count() as i32,
                    indices: m.indices,
                    label: f.relative,
                    target: if f.is_dir {
                        Target::Folder(f.path)
                    } else {
                        Target::File(f.path)
                    },
                })
            })
            .collect();
        found.sort_by_key(|item| std::cmp::Reverse(item.score));
        found.truncate(limit(Mode::Files, 8));
        out.extend(found);
    }

    state.cache = out.clone();
    state.cached_query = Some(state.query.clone());
    state.cached_items = indexed;
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
        Command::OpenSettings => Icon::Config,
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
    let (up, down, enter, escape, tab) = ctx.input_mut(|i| {
        (
            i.consume_key(Modifiers::NONE, Key::ArrowUp),
            i.consume_key(Modifiers::NONE, Key::ArrowDown),
            i.consume_key(Modifiers::NONE, Key::Enter),
            i.consume_key(Modifiers::NONE, Key::Escape),
            i.consume_key(Modifiers::NONE, Key::Tab),
        )
    });
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
                            let message = if state.index.as_ref().is_some_and(|i| !i.is_done()) {
                                "Searching…"
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
                        let mut group = String::new();
                        for (i, item) in items
                            .iter()
                            .enumerate()
                            .take(MAX_ROWS.max(items.len()).min(30))
                        {
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
        let g = widgets::truncated(
            ui,
            &item.hint,
            crate::ui::fonts::regular(12.0),
            p.muted,
            220.0,
        );
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
