//! The main column: filter bar, column header, and the rows, or the empty,
//! loading and error states.
//!
//! Only the rows on screen are laid out, so folders with 100,000 items
//! scroll as smoothly as small ones.

use std::path::Path;

use egui::text::{CCursor, CCursorRange};
use egui::{
    Align2, Color32, CornerRadius, Frame, Key, Margin, Pos2, Rect, ScrollArea, Sense, Shape,
    Stroke, StrokeKind, TextEdit, vec2,
};

use crate::app::ExploristApp;
use crate::app::browser::LoadState;
use crate::app::commands::Command;
use crate::fs::git::FileState as GitState;
use crate::fs::listing::{ListError, ListErrorKind};
use crate::fs::sort::{SortColumn, SortDirection};
use crate::fs::{format, ops};
use crate::ui::icons::{self, Icon};
use crate::ui::menu;
use crate::ui::theme::{Palette, radius, row_height, size, text};
use crate::ui::widgets::{self, ButtonStyle};

/// Horizontal margin around the header and rows.
const SIDE: f32 = 8.0;
const PAD_LEFT: f32 = 12.0;
const PAD_RIGHT: f32 = 16.0;
const ROW_GAP: f32 = 1.0;

pub fn show(ui: &mut egui::Ui, app: &mut ExploristApp) {
    let p = app.palette;
    let mut commands = Vec::new();
    egui::CentralPanel::default()
        .frame(Frame::NONE.fill(p.bg_2))
        .show(ui, |ui| {
            ui.spacing_mut().item_spacing = vec2(0.0, 0.0);
            if app.view.filter_open {
                filter_bar(ui, app);
            }
            match app.browser.load_state().clone() {
                LoadState::Failed(err) => error_state(ui, app, &err, &mut commands),
                state => {
                    let loading = matches!(state, LoadState::Loading { .. });
                    let grid = app.settings.grid_view;
                    if grid {
                        sort_bar(ui, app);
                    } else {
                        header(ui, app);
                    }
                    if loading {
                        progress_line(ui, &p, app.reduce_motion());
                    }
                    if app.browser.total() == 0 && loading {
                        skeleton(ui, app);
                    } else if app.browser.rows().is_empty() && !loading {
                        if app.browser.filter().is_active() {
                            no_matches(ui, app);
                        } else {
                            empty_state(ui, app, &mut commands);
                        }
                    } else if grid {
                        crate::ui::grid::tiles(ui, app, &mut commands);
                    } else {
                        app.view.columns = 1;
                        rows(ui, app, &mut commands);
                    }
                }
            }
        });
    if app.view.drop_hover {
        drop_overlay(ui, &p, app.browser.location());
    }
    let ctx = ui.ctx().clone();
    for command in commands {
        app.run(&ctx, command);
    }
}

/// Shown while files from outside are dragged over the window.
fn drop_overlay(ui: &egui::Ui, p: &Palette, location: &Path) {
    let rect = ui.ctx().content_rect().shrink(12.0);
    let painter = ui.ctx().layer_painter(egui::LayerId::new(
        egui::Order::Foreground,
        egui::Id::new("drop"),
    ));
    painter.rect(
        rect,
        CornerRadius::same(radius::LG),
        p.accent_alpha(if p.dark { 0.10 } else { 0.08 }),
        Stroke::new(2.0, p.accent),
        StrokeKind::Inside,
    );
    painter.text(
        rect.center(),
        Align2::CENTER_CENTER,
        format!("Drop to copy into “{}”", folder_name(location)),
        text::heading(),
        p.text,
    );
}

/// Splits a row into the Name, Size, Modified and Type cells.
fn columns(row: Rect) -> [Rect; 4] {
    let inner = Rect::from_min_max(
        Pos2::new(row.left() + PAD_LEFT, row.top()),
        Pos2::new(row.right() - PAD_RIGHT, row.bottom()),
    );
    let ty_left = inner.right() - size::COL_TYPE;
    let mod_left = ty_left - size::COL_GAP - size::COL_MODIFIED;
    let size_left = mod_left - size::COL_GAP - size::COL_SIZE;
    let name_right = (size_left - size::COL_GAP).max(inner.left());
    let cell = |l: f32, r: f32| {
        Rect::from_min_max(Pos2::new(l, inner.top()), Pos2::new(r, inner.bottom()))
    };
    [
        cell(inner.left(), name_right),
        cell(size_left, size_left + size::COL_SIZE),
        cell(mod_left, mod_left + size::COL_MODIFIED),
        cell(ty_left, inner.right()),
    ]
}

fn header(ui: &mut egui::Ui, app: &mut ExploristApp) {
    let p = app.palette;
    let (full, _) = ui.allocate_exact_size(
        vec2(ui.available_width(), size::COLUMN_HEADER),
        Sense::hover(),
    );
    let rect = full.shrink2(vec2(SIDE, 0.0));
    ui.painter().hline(
        rect.x_range(),
        rect.bottom() - 0.5,
        Stroke::new(1.0, p.border_subtle),
    );

    let sort = app.browser.sort();
    let cells = columns(rect);
    let labels = [
        ("Name", SortColumn::Name, Align2::LEFT_CENTER),
        ("Size", SortColumn::Size, Align2::RIGHT_CENTER),
        ("Modified", SortColumn::Modified, Align2::LEFT_CENTER),
        ("Type", SortColumn::Type, Align2::LEFT_CENTER),
    ];
    for (cell, (label, column, align)) in cells.into_iter().zip(labels) {
        let active = sort.column == column;
        let font = if active {
            text::label()
        } else {
            crate::ui::fonts::regular(12.0)
        };
        let response = ui.interact(cell, ui.id().with(("header", label)), Sense::click());
        let color = if active || response.hovered() {
            p.text_2
        } else {
            p.muted
        };
        let galley = ui.painter().layout_no_wrap(label.to_owned(), font, color);
        let text_w = galley.size().x;
        let arrow_w = if active { 16.0 } else { 0.0 };
        let left = match align {
            Align2::RIGHT_CENTER => cell.right() - text_w - arrow_w,
            _ => cell.left(),
        };
        ui.painter().galley(
            Pos2::new(left, cell.center().y - galley.size().y / 2.0),
            galley,
            color,
        );
        if active {
            let icon = match sort.direction {
                SortDirection::Ascending => Icon::SortUp,
                SortDirection::Descending => Icon::SortDown,
            };
            let r = Rect::from_center_size(
                Pos2::new(left + text_w + 4.0 + 6.0, cell.center().y),
                vec2(12.0, 12.0),
            );
            icons::paint(ui.painter(), r, icon, 12.0, p.text_2);
        }
        if response.clicked() {
            app.sort_by(column);
        }
    }
}

/// The grid has no columns to click, so its header is a row of sort
/// choices instead.
fn sort_bar(ui: &mut egui::Ui, app: &mut ExploristApp) {
    let p = app.palette;
    let (full, _) = ui.allocate_exact_size(
        vec2(ui.available_width(), size::COLUMN_HEADER),
        Sense::hover(),
    );
    let rect = full.shrink2(vec2(SIDE, 0.0));
    ui.painter().hline(
        rect.x_range(),
        rect.bottom() - 0.5,
        Stroke::new(1.0, p.border_subtle),
    );
    let y = rect.center().y;
    let label = ui.painter().text(
        Pos2::new(rect.left() + PAD_LEFT, y),
        Align2::LEFT_CENTER,
        "Sort by",
        crate::ui::fonts::regular(12.0),
        p.faint,
    );
    let sort = app.browser.sort();
    let mut x = label.right() + 12.0;
    for (name, column) in [
        ("Name", SortColumn::Name),
        ("Size", SortColumn::Size),
        ("Modified", SortColumn::Modified),
        ("Type", SortColumn::Type),
    ] {
        let active = sort.column == column;
        let font = if active {
            text::label()
        } else {
            crate::ui::fonts::regular(12.0)
        };
        let galley = ui.painter().layout_no_wrap(name.to_owned(), font, p.muted);
        let width = galley.size().x + if active { 16.0 } else { 0.0 };
        let cell = Rect::from_min_size(
            Pos2::new(x - 6.0, rect.top() + 4.0),
            vec2(width + 12.0, rect.height() - 8.0),
        );
        let response = ui.interact(cell, ui.id().with(("sort", name)), Sense::click());
        if response.hovered() {
            ui.painter()
                .rect_filled(cell, CornerRadius::same(radius::SM), p.hover);
        }
        let color = if active || response.hovered() {
            p.text_2
        } else {
            p.muted
        };
        let text_w = galley.size().x;
        ui.painter()
            .galley(Pos2::new(x, y - galley.size().y / 2.0), galley, color);
        if active {
            let icon = match sort.direction {
                SortDirection::Ascending => Icon::SortUp,
                SortDirection::Descending => Icon::SortDown,
            };
            let r = Rect::from_center_size(Pos2::new(x + text_w + 10.0, y), vec2(12.0, 12.0));
            icons::paint(ui.painter(), r, icon, 12.0, p.text_2);
        }
        if response.clicked() {
            app.sort_by(column);
        }
        x += width + 18.0;
    }
}

/// While reading there's no total to measure against, so a short bar
/// slides along the line.
fn progress_line(ui: &mut egui::Ui, p: &Palette, still: bool) {
    let (full, _) = ui.allocate_exact_size(vec2(ui.available_width(), 2.0), Sense::hover());
    let rect = full.shrink2(vec2(SIDE, 0.0));
    ui.painter().rect_filled(rect, 0.0, p.scroll_track);
    if still {
        // Reduce motion: a steady partial bar instead of a sliding one.
        let mut bar = rect;
        bar.set_width(rect.width() * 0.3);
        ui.painter().rect_filled(bar, 0.0, p.accent);
        return;
    }
    let t = ui.input(|i| i.time) as f32;
    let width = rect.width() * 0.3;
    let x = rect.left() + (t * 0.8).fract() * (rect.width() + width) - width;
    let bar = Rect::from_min_max(
        Pos2::new(x.max(rect.left()), rect.top()),
        Pos2::new((x + width).min(rect.right()), rect.bottom()),
    );
    ui.painter().rect_filled(bar, 0.0, p.accent);
    ui.ctx().request_repaint();
}

fn rows(ui: &mut egui::Ui, app: &mut ExploristApp, commands: &mut Vec<Command>) {
    let row_h = row_height(app.settings.density);
    let step = row_h + ROW_GAP;
    let count = app.browser.rows().len();

    let area = ui.available_rect_before_wrap();
    // Clicks and right-clicks on empty space below the rows.
    let background = ui.interact(area, ui.id().with("list-bg"), Sense::click_and_drag());
    if background.drag_started() {
        app.begin_marquee(ui.ctx());
    }
    if background.clicked() {
        app.browser.select(|sel, _| sel.clear());
        app.view.rename = None;
    }
    background.context_menu(|ui| background_menu(ui, app, commands));

    let viewport_h = (area.height() - 12.0).max(row_h);
    app.view.page_rows = ((viewport_h / step).floor() as usize).max(1);

    let mut scroll = ScrollArea::vertical()
        .id_salt("rows")
        .auto_shrink([false, false]);
    let marquee_scroll = std::mem::take(&mut app.view.marquee_scroll);
    if marquee_scroll != 0.0 {
        scroll = scroll.vertical_scroll_offset((app.view.list_offset + marquee_scroll).max(0.0));
    }
    if std::mem::take(&mut app.view.scroll_to_focus)
        && let Some(row) = app.browser.selection.focused_row(app.browser.rows())
    {
        let top = row as f32 * step;
        let offset = app.view.list_offset;
        let height = app.view.list_height.max(row_h);
        if top < offset {
            scroll = scroll.vertical_scroll_offset(top);
        } else if top + row_h > offset + height {
            scroll = scroll.vertical_scroll_offset(top + row_h - height);
        }
    }

    let output = Frame::NONE
        .inner_margin(Margin {
            left: SIDE as i8,
            right: SIDE as i8,
            top: 4,
            bottom: 8,
        })
        .show(ui, |ui| {
            ui.spacing_mut().item_spacing.y = ROW_GAP;
            scroll.show_rows(ui, row_h, count, |ui, range| {
                for row in range {
                    draw_row(ui, app, row, row_h, commands);
                }
            })
        })
        .inner;
    app.view.list_offset = output.state.offset.y;
    app.view.list_height = output.inner_rect.height();
    app.view.viewport = crate::ui::marquee::Viewport {
        rect: output.inner_rect,
        offset: output.state.offset.y,
    };
    app.view.marquee_scroll = app.update_marquee(
        ui,
        crate::ui::marquee::Layout::List {
            row_height: row_h,
            step,
        },
    );
}

fn draw_row(
    ui: &mut egui::Ui,
    app: &mut ExploristApp,
    row: usize,
    row_h: f32,
    commands: &mut Vec<Command>,
) {
    let p = app.palette;
    let Some(entry) = app.browser.rows().get(row).cloned() else {
        return;
    };
    let (rect, response) =
        ui.allocate_exact_size(vec2(ui.available_width(), row_h), Sense::click_and_drag());
    if !ui.is_rect_visible(rect) {
        return;
    }

    let selected = app.browser.selection.is_selected(&entry.path);
    let focused = app.browser.selection.is_focused(&entry.path);
    let renaming = app
        .view
        .rename
        .as_ref()
        .is_some_and(|r| r.path == entry.path);
    let fill = if selected || renaming {
        p.selection()
    } else if response.hovered() {
        p.hover
    } else {
        Color32::TRANSPARENT
    };
    let painter = ui.painter();
    painter.rect_filled(rect, CornerRadius::same(radius::SM), fill);
    if entry.is_dir && app.drop_target(ui, rect, &entry.path) {
        crate::app::drag::paint_drop_highlight(ui.painter(), &p, rect);
    }
    let painter = ui.painter();
    if focused {
        painter.rect_stroke(
            rect,
            CornerRadius::same(radius::SM),
            Stroke::new(1.0, p.focus_ring()),
            StrokeKind::Inside,
        );
    }

    let [name_cell, size_cell, mod_cell, type_cell] = columns(rect);
    let git = app
        .git
        .poll()
        .and_then(|g| g.state_of(&entry.path, entry.is_dir));
    let ignored = git == Some(GitState::Ignored);
    // Hidden, git-ignored, and cut items are drawn dimmed.
    let dim = entry.hidden || ignored || app.view.cut.contains(&entry.path);
    let icon_color = if entry.is_dir {
        if dim { p.faint } else { p.accent }
    } else if selected {
        p.text
    } else {
        p.text_2
    };
    let icon_rect = Rect::from_min_size(
        Pos2::new(name_cell.left(), rect.center().y - 8.0),
        vec2(16.0, 16.0),
    );
    icons::paint(
        painter,
        icon_rect,
        Icon::for_kind(entry.kind),
        16.0,
        icon_color,
    );
    let name_rect = Rect::from_min_max(
        Pos2::new(icon_rect.right() + 10.0, rect.top()),
        Pos2::new(name_cell.right(), rect.bottom()),
    );

    // Dragging the icon or name moves files; dragging anywhere else on
    // the row draws a selection box, as in Explorer.
    let mut grab_right = name_rect.left();
    let meta = |s: String| (s, crate::ui::fonts::mono(12.0), p.text_2);
    if renaming {
        rename_field(
            ui,
            app,
            name_rect,
            Rect::from_min_max(size_cell.min, type_cell.max),
            commands,
        );
    } else {
        let font = if entry.is_dir && !dim {
            text::body_medium()
        } else {
            text::body()
        };
        let color = if dim { p.muted } else { p.text };
        let badge = git.filter(|g| *g != GitState::Ignored);
        let badge_w = if badge.is_some() { 30.0 } else { 0.0 };
        let galley = widgets::truncated(ui, &entry.name, font, color, name_rect.width() - badge_w);
        let name_w = galley.size().x;
        grab_right = name_rect.left() + name_w + badge_w;
        widgets::paint_aligned(ui.painter(), name_rect, galley, Align2::LEFT_CENTER, color);
        if let Some(state) = badge {
            let tint = match state {
                GitState::Added | GitState::Untracked => p.success,
                GitState::Conflicted | GitState::Deleted => p.error,
                _ => p.warning,
            };
            let x = name_rect.left() + name_w + 8.0;
            ui.painter()
                .circle_filled(Pos2::new(x + 3.0, rect.center().y), 3.0, tint);
            ui.painter().text(
                Pos2::new(x + 11.0, rect.center().y),
                Align2::LEFT_CENTER,
                state.letter(),
                crate::ui::fonts::mono(11.0),
                tint,
            );
            let badge_rect =
                Rect::from_min_size(Pos2::new(x - 2.0, rect.top()), vec2(24.0, rect.height()));
            ui.interact(badge_rect, ui.id().with(("git", row)), Sense::hover())
                .on_hover_text(state.describe());
        }

        let (size_text, f, c) = meta(entry.size.map_or_else(|| "—".to_owned(), format::size));
        let g = ui.painter().layout_no_wrap(size_text, f, c);
        widgets::paint_aligned(ui.painter(), size_cell, g, Align2::RIGHT_CENTER, c);
        let date_format = app.settings.date_format;
        let (date, f, c) = meta(
            entry
                .modified
                .map(|t| format::date(t, date_format))
                .unwrap_or_default(),
        );
        let g = ui.painter().layout_no_wrap(date, f, c);
        widgets::paint_aligned(ui.painter(), mod_cell, g, Align2::LEFT_CENTER, c);
        let type_label = if ignored {
            format!("{} · ignored", entry.type_label)
        } else {
            entry.type_label.to_string()
        };
        let g = widgets::truncated(
            ui,
            &type_label,
            crate::ui::fonts::regular(12.5),
            p.muted,
            type_cell.width(),
        );
        widgets::paint_aligned(ui.painter(), type_cell, g, Align2::LEFT_CENTER, p.muted);
    }

    let grab = Rect::from_min_max(
        Pos2::new(icon_rect.left() - 6.0, rect.top()),
        Pos2::new(grab_right + 8.0, rect.bottom()),
    );
    let on_name = ui.input(|i| i.pointer.press_origin().is_some_and(|p| grab.contains(p)));
    handle_clicks(
        ui, app, &response, &entry, row, selected, renaming, on_name, commands,
    );
}

/// Selection, opening, and the context menu, for a row or a tile. A drag
/// moves files when it starts on `grab`, else draws a selection box.
#[allow(clippy::too_many_arguments)]
pub(super) fn handle_clicks(
    ui: &egui::Ui,
    app: &mut ExploristApp,
    response: &egui::Response,
    entry: &crate::fs::entry::Entry,
    row: usize,
    selected: bool,
    renaming: bool,
    grab: bool,
    commands: &mut Vec<Command>,
) {
    if response.drag_started() && !renaming && app.view.rename.is_none() {
        if grab {
            app.begin_drag(row, selected);
        } else {
            app.begin_marquee(ui.ctx());
        }
    }
    if response.clicked() || response.secondary_clicked() {
        if renaming {
            return;
        }
        if app.view.rename.is_some() && !app.commit_rename() {
            return;
        }
        let mods = ui.input(|i| i.modifiers);
        if response.secondary_clicked() {
            if !selected {
                app.browser.select(|sel, rows| sel.click(rows, row));
            }
        } else if mods.shift {
            app.browser
                .select(|sel, rows| sel.extend_to(rows, row, mods.ctrl || mods.command));
        } else if mods.ctrl || mods.command {
            app.browser.select(|sel, rows| sel.toggle(rows, row));
        } else {
            app.browser.select(|sel, rows| sel.click(rows, row));
        }
    }
    if response.double_clicked() && !renaming {
        app.browser.select(|sel, rows| sel.click(rows, row));
        commands.push(Command::Open);
    }
    if response.middle_clicked() && entry.is_dir {
        app.browser.select(|sel, rows| sel.click(rows, row));
        commands.push(Command::OpenInNewTab);
    }
    response.context_menu(|ui| row_menu(ui, app, commands));
}

pub(super) fn rename_field(
    ui: &mut egui::Ui,
    app: &mut ExploristApp,
    rect: Rect,
    error_area: Rect,
    commands: &mut Vec<Command>,
) {
    let p = app.palette;
    let Some(rename) = &mut app.view.rename else {
        return;
    };
    let field = Rect::from_min_max(
        Pos2::new(rect.left() - 4.0, rect.top() + 3.0),
        Pos2::new(rect.right(), rect.bottom() - 3.0),
    );
    let bad = rename.error.is_some();
    ui.painter().rect(
        field,
        CornerRadius::same(radius::XS),
        p.surface,
        Stroke::new(1.0, if bad { p.error } else { p.accent }),
        StrokeKind::Inside,
    );

    // TextEdit lays out from the top, so give it a rect one line tall,
    // centered in the field.
    let inner = Rect::from_center_size(field.center(), vec2(field.width() - 8.0, 18.0));
    let mut field_ui = ui.new_child(egui::UiBuilder::new().max_rect(inner));
    let mut output = {
        let ui = &mut field_ui;
        TextEdit::singleline(&mut rename.text)
            .font(text::body())
            .text_color(p.text)
            .frame(Frame::NONE)
            .margin(Margin::ZERO)
            .desired_width(inner.width())
            .vertical_align(egui::Align::Center)
            .show(ui)
    };
    let response = output.response.clone();

    if rename.just_opened {
        rename.just_opened = false;
        response.request_focus();
        let stem = ops::stem_range(&rename.text, rename.is_dir);
        let start = rename.text[..stem.start].chars().count();
        let end = rename.text[..stem.end].chars().count();
        output.state.cursor.set_char_range(Some(CCursorRange::two(
            CCursor::new(start),
            CCursor::new(end),
        )));
        output.state.store(ui.ctx(), response.id);
    }
    if response.changed() {
        rename.error = ops::validate_name(&rename.text)
            .err()
            .map(|e| e.to_string());
    }
    if let Some(err) = &rename.error {
        let g = widgets::truncated(ui, err, text::caption(), p.error, error_area.width());
        widgets::paint_aligned(ui.painter(), error_area, g, Align2::LEFT_CENTER, p.error);
    }

    let (enter, escape, tab) = ui.input(|i| {
        (
            i.key_pressed(Key::Enter),
            i.key_pressed(Key::Escape),
            i.key_pressed(Key::Tab),
        )
    });
    if escape {
        app.view.rename = None;
    } else if response.lost_focus() {
        if app.commit_rename() {
            if tab {
                // Tab moves on to renaming the next row.
                commands.push(Command::Move(
                    crate::app::commands::MoveTarget::Down,
                    crate::app::selection::MoveMode::Replace,
                ));
                commands.push(Command::Rename);
            }
        } else if enter || tab {
            response.request_focus();
        } else {
            // Clicked away with a bad name: give up, as Explorer does.
            app.view.rename = None;
        }
    }
}

fn row_menu(ui: &mut egui::Ui, app: &mut ExploristApp, commands: &mut Vec<Command>) {
    let p = app.palette;
    menu::style(ui);
    let many = app.browser.selection.count() > 1;
    let any_folder = app.browser.selected_entries().iter().any(|e| e.is_dir);
    let any_file = app.browser.selected_entries().iter().any(|e| !e.is_dir);
    let items: &[Option<(Command, &str)>] = &[
        Some((Command::Open, Command::Open.label())),
        Some((Command::OpenWith, Command::OpenWith.label())),
        Some((Command::OpenInNewTab, Command::OpenInNewTab.label())),
        Some((Command::OpenTerminal, Command::OpenTerminal.label())),
        None,
        Some((Command::Cut, Command::Cut.label())),
        Some((Command::Copy, Command::Copy.label())),
        Some((Command::CopyPath, Command::CopyPath.label())),
        None,
        Some((Command::Rename, Command::Rename.label())),
        Some((Command::BatchRename, Command::BatchRename.label())),
        Some((Command::PinFolder, Command::PinFolder.label())),
        None,
        Some((Command::Trash, Command::Trash.label())),
        Some((
            Command::DeletePermanently,
            Command::DeletePermanently.label(),
        )),
        None,
        Some((Command::Properties, Command::Properties.label())),
    ];
    for item in items {
        match item {
            Some((command, label)) => {
                let enabled = match command {
                    Command::Rename => !many,
                    Command::PinFolder | Command::OpenInNewTab => any_folder,
                    Command::OpenWith => any_file,
                    _ => true,
                };
                if menu::item_labeled(ui, &p, label, *command, enabled) {
                    commands.push(*command);
                }
            }
            None => menu::separator(ui, &p),
        }
    }
}

pub(super) fn background_menu(
    ui: &mut egui::Ui,
    app: &mut ExploristApp,
    commands: &mut Vec<Command>,
) {
    let p = app.palette;
    menu::style(ui);
    let can_paste = crate::fs::clipboard::read().is_some_and(|c| !c.paths.is_empty());
    if menu::item(ui, &p, Command::Paste, can_paste) {
        commands.push(Command::Paste);
    }
    menu::separator(ui, &p);
    for command in [Command::NewFolder, Command::NewFile] {
        if menu::item(ui, &p, command, true) {
            commands.push(command);
        }
    }
    menu::separator(ui, &p);
    let hidden_label = if app.browser.filter().show_hidden {
        "Hide hidden files"
    } else {
        "Show hidden files"
    };
    if menu::item_labeled(ui, &p, hidden_label, Command::ToggleHidden, true) {
        commands.push(Command::ToggleHidden);
    }
    for command in [Command::Refresh, Command::OpenTerminal, Command::CopyPath] {
        let label = if command == Command::CopyPath {
            "Copy folder path"
        } else {
            command.label()
        };
        if menu::item_labeled(ui, &p, label, command, true) {
            commands.push(command);
        }
    }
    menu::separator(ui, &p);
    if menu::item(ui, &p, Command::Properties, true) {
        commands.push(Command::Properties);
    }
}

fn filter_bar(ui: &mut egui::Ui, app: &mut ExploristApp) {
    let p = app.palette;
    let (bar, _) = ui.allocate_exact_size(vec2(ui.available_width(), 50.0), Sense::hover());
    ui.painter().hline(
        bar.x_range(),
        bar.bottom() - 0.5,
        Stroke::new(1.0, p.border_subtle),
    );

    let input = Rect::from_min_size(
        Pos2::new(bar.left() + 16.0, bar.center().y - 15.0),
        vec2(240.0, 30.0),
    );
    ui.painter().rect(
        input,
        CornerRadius::same(radius::SM),
        p.surface,
        Stroke::new(1.0, p.accent),
        StrokeKind::Inside,
    );
    ui.painter().rect_stroke(
        input,
        CornerRadius::same(radius::SM),
        Stroke::new(3.0, p.accent_alpha(if p.dark { 0.22 } else { 0.18 })),
        StrokeKind::Outside,
    );
    icons::paint(
        ui.painter(),
        Rect::from_min_size(
            Pos2::new(input.left() + 10.0, input.center().y - 8.0),
            vec2(16.0, 16.0),
        ),
        Icon::Filter,
        16.0,
        p.accent,
    );
    let kbd = widgets::kbd_galley(ui, &p, "Esc");
    let kbd_left = input.right() - 6.0 - widgets::kbd_width(&kbd);
    widgets::paint_kbd(ui.painter(), &p, kbd, kbd_left, input.center().y, false);

    let field = Rect::from_min_max(
        Pos2::new(input.left() + 34.0, input.center().y - 9.0),
        Pos2::new(kbd_left - 6.0, input.center().y + 9.0),
    );
    let mut field_ui = ui.new_child(egui::UiBuilder::new().max_rect(field));
    let output = {
        let ui = &mut field_ui;
        TextEdit::singleline(&mut app.view.filter_text)
            .hint_text("Filter by name…")
            .font(text::body())
            .text_color(p.text)
            .frame(Frame::NONE)
            .margin(Margin::ZERO)
            .desired_width(field.width())
            .vertical_align(egui::Align::Center)
            .show(ui)
    };
    let response = output.response;
    if std::mem::take(&mut app.view.focus_filter) {
        response.request_focus();
    }
    if response.changed() {
        let query = app.view.filter_text.clone();
        app.browser.set_filter_query(&query);
    }
    let (escape, enter, down) = ui.input(|i| {
        (
            i.key_pressed(Key::Escape),
            i.key_pressed(Key::Enter),
            i.key_pressed(Key::ArrowDown),
        )
    });
    if response.lost_focus() && escape {
        app.view.filter_open = false;
        app.view.filter_text.clear();
        app.browser.set_filter_query("");
    } else if (response.lost_focus() && enter) || (response.has_focus() && down) {
        // Hand the keyboard to the list, on the first match.
        response.surrender_focus();
        app.browser
            .select(|sel, rows| sel.move_to(rows, 0, crate::app::selection::MoveMode::Replace));
        app.view.scroll_to_focus = true;
    }

    // "3 of 14" and Clear all on the right.
    let clear_w = 72.0;
    let clear_rect = Rect::from_min_size(
        Pos2::new(bar.right() - 16.0 - clear_w, bar.center().y - 14.0),
        vec2(clear_w, 28.0),
    );
    let clear = ui.interact(clear_rect, ui.id().with("filter-clear"), Sense::click());
    if clear.hovered() {
        ui.painter()
            .rect_filled(clear_rect, CornerRadius::same(radius::SM), p.hover);
    }
    ui.painter().text(
        clear_rect.center(),
        Align2::CENTER_CENTER,
        "Clear all",
        crate::ui::fonts::medium(12.5),
        p.text_2,
    );
    if clear.clicked() {
        app.view.filter_text.clear();
        app.browser.clear_filter();
        app.view.focus_filter = true;
    }
    let counts = format!(
        "{} of {}",
        format::count(app.browser.rows().len() as u64),
        format::count(app.browser.shown_total() as u64)
    );
    let counts_rect = ui.painter().text(
        Pos2::new(clear_rect.left() - 8.0, bar.center().y),
        Align2::RIGHT_CENTER,
        counts,
        crate::ui::fonts::mono(12.0),
        p.text_2,
    );

    // Chips between the field and the counts.
    let area = Rect::from_min_max(
        Pos2::new(input.right() + 8.0, bar.top()),
        Pos2::new(counts_rect.left() - 12.0, bar.bottom()),
    );
    let mut child = ui.new_child(
        egui::UiBuilder::new()
            .max_rect(area)
            .layout(egui::Layout::left_to_right(egui::Align::Center)),
    );
    child.set_clip_rect(area);
    child.spacing_mut().item_spacing.x = 8.0;
    filter_chips(&mut child, app);
}

/// The Type, Size and Modified chips, and "Add filter".
fn filter_chips(ui: &mut egui::Ui, app: &mut ExploristApp) {
    use crate::fs::filter::{Recency, SizeRule};
    let p = app.palette;
    let filter = app.browser.filter().clone();

    if !filter.types.is_empty() {
        let value = filter.types.join(", ");
        let (body, remove) = chip(ui, &p, "Type", &value, false);
        egui::Popup::menu(&body)
            .close_behavior(egui::PopupCloseBehavior::CloseOnClickOutside)
            .show(|ui| type_menu(ui, app));
        if remove {
            app.browser.edit_filter(|f| f.types.clear());
        }
    }
    if let Some(rule) = filter.size {
        let value = format!(
            "{} {}",
            if rule.at_least { ">" } else { "<" },
            format::size(rule.bytes)
        );
        let (body, remove) = chip(ui, &p, "Size", &value, true);
        egui::Popup::menu(&body)
            .close_behavior(egui::PopupCloseBehavior::CloseOnClickOutside)
            .show(|ui| size_menu(ui, app));
        if remove {
            app.browser.edit_filter(|f| f.size = None);
        }
    }
    if let Some(recency) = filter.modified {
        let (body, remove) = chip(ui, &p, "Modified", recency.label(), false);
        egui::Popup::menu(&body).show(|ui| {
            menu::style(ui);
            for r in Recency::ALL {
                if menu::plain_item(ui, &p, r.label()) {
                    app.browser.edit_filter(|f| f.modified = Some(r));
                }
            }
        });
        if remove {
            app.browser.edit_filter(|f| f.modified = None);
        }
    }

    // "Add filter" offers the chips not shown yet.
    let add = widgets::button(ui, &p, "+ Add filter", ButtonStyle::Ghost, None);
    egui::Popup::menu(&add).show(|ui| {
        menu::style(ui);
        if filter.types.is_empty() && menu::plain_item(ui, &p, "Type") {
            // Start with the focused item's type, else the most common one.
            let focused = app
                .browser
                .selection
                .focus()
                .and_then(|f| app.browser.rows().position(f))
                .and_then(|r| app.browser.rows().get(r))
                .map(|e| e.type_label.to_string());
            let first =
                focused.or_else(|| app.browser.type_counts().first().map(|(t, _)| t.clone()));
            if let Some(t) = first {
                app.browser.edit_filter(|f| f.types = vec![t]);
            }
        }
        if filter.size.is_none() && menu::plain_item(ui, &p, "Size") {
            app.browser.edit_filter(|f| {
                f.size = Some(SizeRule {
                    at_least: true,
                    bytes: 1024 * 1024,
                })
            });
        }
        if filter.modified.is_none() && menu::plain_item(ui, &p, "Modified") {
            app.browser
                .edit_filter(|f| f.modified = Some(Recency::LastWeek));
        }
    });
}

/// A filter chip: muted name, value, and a remove button. Returns the
/// body's response (click to edit) and whether × was clicked.
fn chip(
    ui: &mut egui::Ui,
    p: &Palette,
    name: &str,
    value: &str,
    mono: bool,
) -> (egui::Response, bool) {
    let name_g =
        ui.painter()
            .layout_no_wrap(name.to_owned(), crate::ui::fonts::regular(12.5), p.muted);
    let value_font = if mono {
        crate::ui::fonts::mono(12.0)
    } else {
        crate::ui::fonts::regular(12.5)
    };
    let value_g = widgets::truncated(ui, value, value_font, p.text, 220.0);
    let width = 10.0 + name_g.size().x + 6.0 + value_g.size().x + 6.0 + 20.0 + 4.0;
    let (rect, body) = ui.allocate_exact_size(vec2(width, 28.0), Sense::click());
    ui.painter().rect(
        rect,
        CornerRadius::same(radius::SM),
        p.accent_alpha(if p.dark { 0.12 } else { 0.08 }),
        Stroke::new(1.0, p.accent_alpha(if p.dark { 0.35 } else { 0.3 })),
        StrokeKind::Inside,
    );
    let mut x = rect.left() + 10.0;
    let name_w = name_g.size().x;
    ui.painter().galley(
        Pos2::new(x, rect.center().y - name_g.size().y / 2.0),
        name_g,
        p.muted,
    );
    x += name_w + 6.0;
    let value_w = value_g.size().x;
    ui.painter().galley(
        Pos2::new(x, rect.center().y - value_g.size().y / 2.0),
        value_g,
        p.text,
    );
    x += value_w + 6.0;
    let close = Rect::from_min_size(Pos2::new(x, rect.center().y - 10.0), vec2(20.0, 20.0));
    let remove = ui
        .interact(close, body.id.with("remove"), Sense::click())
        .on_hover_text(format!("Remove {} filter", name.to_lowercase()));
    if remove.hovered() {
        ui.painter()
            .rect_filled(close, CornerRadius::same(radius::XS), p.hover);
    }
    icons::paint(ui.painter(), close, Icon::Close, 10.0, p.text_2);
    (body, remove.clicked())
}

fn type_menu(ui: &mut egui::Ui, app: &mut ExploristApp) {
    let p = app.palette;
    menu::style(ui);
    let chosen = app.browser.filter().types.clone();
    for (label, count) in app.browser.type_counts().into_iter().take(20) {
        let on = chosen.contains(&label);
        let (rect, response) =
            ui.allocate_exact_size(vec2(menu::MENU_WIDTH - 8.0, 30.0), Sense::click());
        if response.hovered() {
            ui.painter().rect_filled(
                rect,
                CornerRadius::same(radius::SM - 1),
                p.accent_alpha(if p.dark { 0.16 } else { 0.10 }),
            );
        }
        if on {
            icons::paint(
                ui.painter(),
                Rect::from_center_size(
                    Pos2::new(rect.left() + 18.0, rect.center().y),
                    vec2(14.0, 14.0),
                ),
                Icon::Check,
                14.0,
                p.accent,
            );
        }
        ui.painter().text(
            Pos2::new(rect.left() + 34.0, rect.center().y),
            Align2::LEFT_CENTER,
            &label,
            text::body(),
            p.text,
        );
        ui.painter().text(
            Pos2::new(rect.right() - 10.0, rect.center().y),
            Align2::RIGHT_CENTER,
            count.to_string(),
            crate::ui::fonts::mono(11.5),
            p.muted,
        );
        if response.clicked() {
            app.browser.edit_filter(|f| {
                if on {
                    f.types.retain(|t| *t != label);
                } else {
                    f.types.push(label.clone());
                }
            });
        }
    }
}

fn size_menu(ui: &mut egui::Ui, app: &mut ExploristApp) {
    use crate::fs::filter::SizeRule;
    let p = app.palette;
    menu::style(ui);
    let Some(rule) = app.browser.filter().size else {
        return;
    };
    let selected = if rule.at_least { 0 } else { 1 };
    ui.add_space(4.0);
    ui.horizontal(|ui| {
        ui.add_space(6.0);
        if let Some(i) = widgets::segmented(ui, &p, &["Larger than", "Smaller than"], selected) {
            app.browser.edit_filter(|f| {
                f.size = Some(SizeRule {
                    at_least: i == 0,
                    ..rule
                })
            });
        }
    });
    ui.add_space(4.0);
    for (label, bytes) in [
        ("1 KB", 1u64 << 10),
        ("100 KB", 100 << 10),
        ("1 MB", 1 << 20),
        ("10 MB", 10 << 20),
        ("100 MB", 100 << 20),
        ("1 GB", 1 << 30),
    ] {
        let mark = if rule.bytes == bytes { "✓ " } else { "   " };
        if menu::plain_item(ui, &p, &format!("{mark}{label}")) {
            app.browser
                .edit_filter(|f| f.size = Some(SizeRule { bytes, ..rule }));
        }
    }
}

fn skeleton(ui: &mut egui::Ui, app: &ExploristApp) {
    const WIDTHS: [f32; 22] = [
        62., 48., 71., 55., 40., 66., 58., 44., 69., 52., 61., 47., 73., 50., 64., 43., 57., 68.,
        46., 60., 53., 70.,
    ];
    let p = app.palette;
    let row_h = row_height(app.settings.density);
    let area = ui.available_rect_before_wrap().shrink2(vec2(SIDE, 0.0));
    // Opacity pulses 0.6 ↔ 1 every 1.4 s.
    let pulse = if app.reduce_motion() {
        1.0
    } else {
        ui.ctx().request_repaint();
        let t = ui.input(|i| i.time) as f32;
        0.8 + 0.2 * (t * std::f32::consts::TAU / 1.4).cos()
    };

    let painter = ui.painter();
    let mut y = area.top() + 4.0;
    for (i, w) in WIDTHS.iter().enumerate() {
        if y + row_h > area.bottom() {
            break;
        }
        let rect = Rect::from_min_size(Pos2::new(area.left(), y), vec2(area.width(), row_h));
        let fade = (1.0 - i as f32 * 0.035).max(0.25) * pulse;
        let color = p.skeleton.gamma_multiply(fade);
        let [name, size_c, date, ty] = columns(rect);
        let bar = |l: f32, width: f32| {
            Rect::from_min_size(Pos2::new(l, rect.center().y - 4.0), vec2(width, 8.0))
        };
        painter.rect_filled(
            Rect::from_min_size(
                Pos2::new(name.left(), rect.center().y - 7.0),
                vec2(14.0, 14.0),
            ),
            CornerRadius::same(3),
            color,
        );
        painter.rect_filled(
            bar(name.left() + 24.0, (name.width() - 24.0) * w / 100.0),
            CornerRadius::same(4),
            color,
        );
        let sw = 36.0 + (i * 7 % 20) as f32;
        painter.rect_filled(bar(size_c.right() - sw, sw), CornerRadius::same(4), color);
        painter.rect_filled(bar(date.left(), 112.0), CornerRadius::same(4), color);
        painter.rect_filled(
            bar(ty.left(), 52.0 + (i * 11 % 30) as f32),
            CornerRadius::same(4),
            color,
        );
        y += row_h + ROW_GAP;
    }
}

/// A centered block: icon tile, title, body text and whatever `extra` adds.
fn centered_state(
    ui: &mut egui::Ui,
    p: &Palette,
    icon: Icon,
    tint: Option<Color32>,
    title: &str,
    body: &str,
    extra: impl FnOnce(&mut egui::Ui),
) {
    let area = ui.available_rect_before_wrap();
    let width = 440.0_f32.min(area.width() - 48.0);
    // A child Ui, so the block doesn't push anything else around.
    let mut child = ui.new_child(egui::UiBuilder::new().max_rect(Rect::from_center_size(
        area.center() - vec2(0.0, 40.0),
        vec2(width, 320.0),
    )));
    child.vertical_centered(|ui| {
        ui.spacing_mut().item_spacing.y = 14.0;
        let (tile, _) = ui.allocate_exact_size(vec2(56.0, 56.0), Sense::hover());
        match tint {
            Some(color) => {
                ui.painter().rect_filled(
                    tile,
                    CornerRadius::same(14),
                    color.gamma_multiply(if p.dark { 0.14 } else { 0.10 }),
                );
                icons::paint(ui.painter(), tile, icon, 24.0, color);
            }
            None => {
                dashed_rect(ui.painter(), tile, Stroke::new(1.0, p.border));
                icons::paint(ui.painter(), tile, icon, 24.0, p.muted);
            }
        }
        ui.label(
            egui::RichText::new(title)
                .font(text::heading())
                .color(p.text),
        );
        ui.label(egui::RichText::new(body).font(text::body()).color(p.text_2));
        extra(ui);
    });
}

fn dashed_rect(painter: &egui::Painter, rect: Rect, stroke: Stroke) {
    let r = rect.shrink(0.5);
    let pts = [
        r.left_top(),
        r.right_top(),
        r.right_bottom(),
        r.left_bottom(),
        r.left_top(),
    ];
    painter.extend(Shape::dashed_line(&pts, stroke, 4.0, 3.0));
}

fn empty_state(ui: &mut egui::Ui, app: &mut ExploristApp, commands: &mut Vec<Command>) {
    let p = app.palette;
    let hidden = app.browser.hidden_count();
    centered_state(
        ui,
        &p,
        Icon::Folder,
        None,
        "This folder is empty",
        "Create a file or folder to get started.",
        |ui| {
            ui.horizontal(|ui| {
                ui.spacing_mut().item_spacing.x = 8.0;
                let file_w = widgets::button_width(
                    ui,
                    &p,
                    "New file",
                    ButtonStyle::Secondary,
                    Some("Ctrl N"),
                );
                let folder_w = widgets::button_width(
                    ui,
                    &p,
                    "New folder",
                    ButtonStyle::Ghost,
                    Some("Ctrl Shift N"),
                );
                ui.add_space(((ui.available_width() - file_w - folder_w - 8.0) / 2.0).max(0.0));
                if widgets::button(ui, &p, "New file", ButtonStyle::Secondary, Some("Ctrl N"))
                    .clicked()
                {
                    commands.push(Command::NewFile);
                }
                if widgets::button(
                    ui,
                    &p,
                    "New folder",
                    ButtonStyle::Ghost,
                    Some("Ctrl Shift N"),
                )
                .clicked()
                {
                    commands.push(Command::NewFolder);
                }
            });
            if hidden > 0 {
                let label = match hidden {
                    1 => "1 hidden item · Show hidden (Ctrl H)".to_owned(),
                    n => format!("{n} hidden items · Show hidden (Ctrl H)"),
                };
                let link = ui.add(
                    egui::Label::new(
                        egui::RichText::new(label)
                            .font(crate::ui::fonts::regular(12.0))
                            .color(p.muted)
                            .underline(),
                    )
                    .sense(Sense::click()),
                );
                if link.clicked() {
                    commands.push(Command::ToggleHidden);
                }
            }
        },
    );
}

fn no_matches(ui: &mut egui::Ui, app: &mut ExploristApp) {
    let p = app.palette;
    let query = app.browser.filter().query().to_owned();
    centered_state(
        ui,
        &p,
        Icon::Filter,
        None,
        "Nothing matches",
        &format!("No names contain “{query}” in this folder."),
        |_| {},
    );
}

fn error_state(
    ui: &mut egui::Ui,
    app: &mut ExploristApp,
    err: &ListError,
    commands: &mut Vec<Command>,
) {
    let p = app.palette;
    let name = folder_name(app.browser.location());
    let (title, body) = match err.kind {
        ListErrorKind::AccessDenied => (
            "You don’t have access to this folder".to_owned(),
            if cfg!(windows) {
                format!(
                    "Windows blocked reading “{name}”. It may be managed by the system or need administrator rights."
                )
            } else {
                format!(
                    "You don’t have permission to read “{name}”. Its owner or an administrator can change that."
                )
            },
        ),
        ListErrorKind::NotFound => (
            "This folder doesn’t exist".to_owned(),
            format!("“{name}” may have been moved, renamed or deleted."),
        ),
        ListErrorKind::NotAFolder => (
            "That’s a file, not a folder".to_owned(),
            format!("“{name}” can’t be opened as a folder."),
        ),
        ListErrorKind::Other => ("Couldn’t read this folder".to_owned(), err.message.clone()),
    };
    let code = error_code(err);
    let details = format!(
        "{}\n{}\n{}",
        app.browser.location().display(),
        err.message,
        code
    );
    let can_back = app.browser.can_go_back();
    let elevate = err.kind == ListErrorKind::AccessDenied && crate::fs::shell::CAN_ELEVATE;
    let icon = if err.kind == ListErrorKind::AccessDenied {
        Icon::Lock
    } else {
        Icon::Warning
    };
    centered_state(ui, &p, icon, Some(p.error), &title, &body, |ui| {
        let chip = ui
            .painter()
            .layout_no_wrap(code.clone(), crate::ui::fonts::mono(11.5), p.muted);
        let (rect, _) = ui.allocate_exact_size(chip.size() + vec2(16.0, 4.0), Sense::hover());
        ui.painter()
            .rect_filled(rect, CornerRadius::same(radius::XS), p.surface);
        ui.painter()
            .galley(rect.min + vec2(8.0, 2.0), chip, p.muted);
        ui.horizontal(|ui| {
            ui.spacing_mut().item_spacing.x = 8.0;
            let retry = if elevate {
                "Retry as administrator"
            } else {
                "Try again"
            };
            let mut total = widgets::button_width(ui, &p, retry, ButtonStyle::Primary, None);
            if elevate {
                total +=
                    8.0 + widgets::button_width(ui, &p, "Try again", ButtonStyle::Secondary, None);
            }
            if can_back {
                total += 8.0
                    + widgets::button_width(
                        ui,
                        &p,
                        "Go back",
                        ButtonStyle::Secondary,
                        Some("Alt ←"),
                    );
            }
            ui.add_space(((ui.available_width() - total) / 2.0).max(0.0));
            if widgets::button(ui, &p, retry, ButtonStyle::Primary, None).clicked() {
                if elevate {
                    commands.push(Command::OpenAsAdmin);
                } else {
                    commands.push(Command::Refresh);
                }
            }
            if elevate
                && widgets::button(ui, &p, "Try again", ButtonStyle::Secondary, None).clicked()
            {
                commands.push(Command::Refresh);
            }
            if can_back
                && widgets::button(ui, &p, "Go back", ButtonStyle::Secondary, Some("Alt ←"))
                    .clicked()
            {
                commands.push(Command::GoBack);
            }
        });
        let link = ui.add(
            egui::Label::new(
                egui::RichText::new("Copy error details")
                    .font(crate::ui::fonts::regular(12.0))
                    .color(p.muted)
                    .underline(),
            )
            .sense(Sense::click()),
        );
        if link.clicked() {
            ui.ctx().copy_text(details.clone());
        }
    });
}

fn folder_name(path: &Path) -> String {
    path.file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_else(|| path.display().to_string())
}

/// The Windows name for common error codes, as the design shows them.
fn error_code(err: &ListError) -> String {
    match err.os_code {
        Some(code) if cfg!(windows) => {
            let name = match code {
                2 => "ERROR_FILE_NOT_FOUND",
                3 => "ERROR_PATH_NOT_FOUND",
                5 => "ERROR_ACCESS_DENIED",
                21 => "ERROR_NOT_READY",
                267 => "ERROR_DIRECTORY",
                _ => "Error",
            };
            format!("{name} ({code})")
        }
        Some(code) => format!("os error {code}"),
        None => "no error code".to_owned(),
    }
}
