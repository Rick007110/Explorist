//! The 52 px toolbar: back/forward/up, the path bar and view toggles.
//!
//! The path bar has two modes: breadcrumbs, and (Ctrl L or a click on
//! empty space) an editable field with the full path selected.

use std::path::{Path, PathBuf};

use egui::text::{CCursor, CCursorRange};
use egui::{
    Align2, CornerRadius, Frame, Key, Margin, Pos2, Rect, Sense, Stroke, StrokeKind, TextEdit, vec2,
};

use crate::app::commands::Command;
use crate::app::keymap;
use crate::app::{ExploristApp, PathEdit};
use crate::ui::icons::{self, Icon};
use crate::ui::theme::{Palette, radius, size, text};
use crate::ui::widgets;

pub fn show(ui: &mut egui::Ui, app: &mut ExploristApp) {
    let p = app.palette;
    egui::Panel::top("toolbar")
        .exact_size(size::TOOLBAR)
        .resizable(false)
        .show_separator_line(false)
        .frame(
            Frame::NONE
                .fill(p.bg_2)
                .inner_margin(Margin::symmetric(12, 0)),
        )
        .show(ui, |ui| {
            let full = ui.max_rect();
            ui.painter().hline(
                full.x_range().expand(12.0),
                full.bottom() - 0.5,
                Stroke::new(1.0, p.border_subtle),
            );

            ui.horizontal_centered(|ui| {
                ui.spacing_mut().item_spacing.x = 2.0;
                let ctx = ui.ctx().clone();
                let nav = [
                    (Icon::Back, Command::GoBack, app.browser.can_go_back()),
                    (
                        Icon::Forward,
                        Command::GoForward,
                        app.browser.can_go_forward(),
                    ),
                    (Icon::Up, Command::GoUp, app.browser.can_go_up()),
                ];
                for (icon, command, enabled) in nav {
                    let tip = tooltip(command);
                    if widgets::icon_button(ui, &p, icon, &tip, enabled, false).clicked() {
                        app.run(&ctx, command);
                    }
                }
                ui.add_space(10.0);

                // Path bar takes what's left after the right-hand buttons.
                let right_width = 3.0 * size::BUTTON + 2.0 * 2.0;
                let width = (ui.available_width() - right_width - 10.0).max(120.0);
                path_bar(ui, app, width);

                ui.add_space(10.0);
                let filter_on = app.view.filter_open;
                if widgets::icon_button(
                    ui,
                    &p,
                    Icon::Filter,
                    &tooltip(Command::Filter),
                    true,
                    filter_on,
                )
                .clicked()
                {
                    if filter_on {
                        app.run(&ctx, Command::Escape);
                        app.view.filter_open = false;
                    } else {
                        app.run(&ctx, Command::Filter);
                    }
                }
                if widgets::icon_button(
                    ui,
                    &p,
                    Icon::Refresh,
                    &tooltip(Command::Refresh),
                    true,
                    false,
                )
                .clicked()
                {
                    app.run(&ctx, Command::Refresh);
                }
                if widgets::icon_button(
                    ui,
                    &p,
                    Icon::Config,
                    &tooltip(Command::OpenSettings),
                    true,
                    false,
                )
                .clicked()
                {
                    app.run(&ctx, Command::OpenSettings);
                }
            });
        });
}

fn tooltip(command: Command) -> String {
    match keymap::shortcut_label(command) {
        Some(keys) => format!("{} ({keys})", command.label()),
        None => command.label().to_owned(),
    }
}

fn path_bar(ui: &mut egui::Ui, app: &mut ExploristApp, width: f32) {
    let p = app.palette;
    let (rect, response) = ui.allocate_exact_size(vec2(width, size::PATH_BAR), Sense::click());
    let editing = app.view.path_edit.is_some();
    let border = if editing { p.accent } else { p.border };
    ui.painter().rect(
        rect,
        CornerRadius::same(radius::MD),
        p.surface,
        Stroke::new(1.0, border),
        StrokeKind::Inside,
    );
    if editing {
        ui.painter().rect_stroke(
            rect,
            CornerRadius::same(radius::MD),
            Stroke::new(3.0, p.accent_alpha(if p.dark { 0.22 } else { 0.18 })),
            StrokeKind::Outside,
        );
        edit_field(ui, app, rect);
        return;
    }

    let drive_icon = Rect::from_min_size(
        Pos2::new(rect.left() + 10.0, rect.center().y - 8.0),
        vec2(16.0, 16.0),
    );
    icons::paint(ui.painter(), drive_icon, Icon::Drive, 16.0, p.muted);

    // "Edit or search  Ctrl L" on the right.
    let kbd = widgets::kbd_galley(ui, &p, "Ctrl L");
    let kbd_left = rect.right() - 6.0 - widgets::kbd_width(&kbd);
    let hint = ui
        .painter()
        .layout_no_wrap("Edit or search".to_owned(), text::caption(), p.muted);
    let hint_left = kbd_left - 6.0 - hint.size().x;
    let show_hint = width > 420.0;
    if show_hint {
        ui.painter().galley(
            Pos2::new(hint_left, rect.center().y - hint.size().y / 2.0),
            hint,
            p.muted,
        );
        widgets::paint_kbd(ui.painter(), &p, kbd, kbd_left, rect.center().y, false);
    }

    let crumbs_left = drive_icon.right() + 6.0;
    let crumbs_right = if show_hint {
        hint_left - 12.0
    } else {
        rect.right() - 6.0
    };
    let clicked_crumb = breadcrumbs(
        ui,
        &p,
        app.browser.location(),
        crumbs_left,
        crumbs_right,
        rect.center().y,
    );
    if let Some(path) = clicked_crumb {
        app.browser.navigate(path);
    } else if response.clicked() {
        app.view.path_edit = Some(PathEdit {
            text: app.browser.location().display().to_string(),
            error: None,
            just_opened: true,
        });
    }
}

/// Draws the path as clickable segments. Leading segments that don't fit
/// collapse into "…". Returns the folder clicked, if any.
fn breadcrumbs(
    ui: &mut egui::Ui,
    p: &Palette,
    location: &Path,
    left: f32,
    right: f32,
    center_y: f32,
) -> Option<PathBuf> {
    let segments: Vec<(String, PathBuf)> = {
        let mut list: Vec<&Path> = location.ancestors().collect();
        list.reverse();
        list.into_iter()
            .map(|path| {
                let label = match path.file_name() {
                    Some(name) => name.to_string_lossy().into_owned(),
                    None => {
                        let s = path.display().to_string();
                        let trimmed = s.trim_end_matches(['\\', '/']);
                        if trimmed.is_empty() {
                            s
                        } else {
                            trimmed.to_owned()
                        }
                    }
                };
                (label, path.to_path_buf())
            })
            .collect()
    };

    let font = text::mono();
    let sep_width = 12.0;
    let galleys: Vec<_> = segments
        .iter()
        .enumerate()
        .map(|(i, (label, _))| {
            let last = i + 1 == segments.len();
            let f = if last {
                crate::ui::fonts::mono_medium(12.5)
            } else {
                font.clone()
            };
            ui.painter()
                .layout_no_wrap(label.clone(), f, if last { p.text } else { p.text_2 })
        })
        .collect();
    let widths: Vec<f32> = galleys.iter().map(|g| g.size().x + 12.0).collect();

    // Drop segments from the front until the rest fits.
    let available = right - left;
    let mut first = 0;
    let total =
        |from: usize| widths[from..].iter().sum::<f32>() + sep_width * (widths.len() - from) as f32;
    while first + 1 < widths.len() && total(first) + 20.0 > available {
        first += 1;
    }

    let mut x = left;
    let mut clicked = None;
    if first > 0 {
        ui.painter().text(
            Pos2::new(x + 4.0, center_y),
            Align2::LEFT_CENTER,
            "…",
            font.clone(),
            p.faint,
        );
        x += 20.0;
    }
    for i in first..segments.len() {
        // No separator right after a root that already ends in one ("/").
        let after_root = i > 0 && segments[i - 1].0.ends_with(['/', '\\']);
        if i > 0 && !after_root {
            ui.painter().text(
                Pos2::new(x + sep_width / 2.0, center_y),
                Align2::CENTER_CENTER,
                "/",
                font.clone(),
                p.faint,
            );
            x += sep_width;
        }
        let w = widths[i].min((right - x).max(0.0));
        let rect = Rect::from_min_size(Pos2::new(x, center_y - 12.0), vec2(w, 24.0));
        let last = i + 1 == segments.len();
        let response = ui.interact(rect, ui.id().with(("crumb", i)), Sense::click());
        if response.hovered() {
            ui.painter()
                .rect_filled(rect, CornerRadius::same(radius::XS), p.hover);
        }
        let color = if last { p.text } else { p.text_2 };
        ui.painter().with_clip_rect(rect).galley(
            Pos2::new(rect.left() + 6.0, center_y - galleys[i].size().y / 2.0),
            galleys[i].clone(),
            color,
        );
        if response.clicked() && !last {
            clicked = Some(segments[i].1.clone());
        }
        x += w;
    }
    clicked
}

fn edit_field(ui: &mut egui::Ui, app: &mut ExploristApp, rect: Rect) {
    let p = app.palette;
    let Some(edit) = &mut app.view.path_edit else {
        return;
    };
    let field_rect = Rect::from_min_max(
        Pos2::new(rect.left() + 10.0, rect.center().y - 9.0),
        Pos2::new(rect.right() - 10.0, rect.center().y + 9.0),
    );
    let error_galley = edit.error.as_ref().map(|e| {
        ui.painter()
            .layout_no_wrap(e.clone(), text::caption(), p.error)
    });
    let field_rect = match &error_galley {
        Some(g) => Rect::from_min_max(
            field_rect.min,
            Pos2::new(field_rect.right() - g.size().x - 12.0, field_rect.bottom()),
        ),
        None => field_rect,
    };

    let mut field_ui = ui.new_child(egui::UiBuilder::new().max_rect(field_rect));
    let mut output = {
        let ui = &mut field_ui;
        TextEdit::singleline(&mut edit.text)
            .font(text::mono())
            .text_color(p.text)
            .frame(Frame::NONE)
            .desired_width(field_rect.width())
            .vertical_align(egui::Align::Center)
            .show(ui)
    };
    let response = output.response.clone();

    if edit.just_opened {
        edit.just_opened = false;
        response.request_focus();
        let end = edit.text.chars().count();
        output
            .state
            .cursor
            .set_char_range(Some(CCursorRange::two(CCursor::new(0), CCursor::new(end))));
        output.state.store(ui.ctx(), response.id);
    }
    if response.changed() {
        edit.error = None;
    }
    if let Some(g) = error_galley {
        ui.painter().galley(
            Pos2::new(
                rect.right() - 10.0 - g.size().x,
                rect.center().y - g.size().y / 2.0,
            ),
            g,
            p.error,
        );
    }

    let enter = response.lost_focus() && ui.input(|i| i.key_pressed(Key::Enter));
    let escape = ui.input(|i| i.key_pressed(Key::Escape));
    if enter {
        app.submit_path();
        if app.view.path_edit.is_some() {
            response.request_focus();
        }
    } else if escape || (response.lost_focus() && !enter) {
        app.view.path_edit = None;
    }
}
