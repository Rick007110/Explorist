//! The Open with picker, on Linux and macOS (Windows uses its own).

use std::path::PathBuf;

use egui::{
    Align2, CornerRadius, Frame, Id, Key, Margin, Modal, Pos2, ScrollArea, Sense, Stroke, vec2,
};

use crate::app::ExploristApp;
use crate::fs::open_with::{self, App};
use crate::ui::icons::{self, Icon};
use crate::ui::theme::{radius, text};
use crate::ui::widgets::{self, ButtonStyle};

const ROW: f32 = 32.0;

pub struct OpenWithState {
    files: Vec<PathBuf>,
    apps: Vec<App>,
    query: String,
    /// Index into the filtered list.
    chosen: usize,
    focus: bool,
    /// The Enter that opened it (from the palette) mustn't pick an app.
    fresh: bool,
}

impl OpenWithState {
    pub fn new(files: Vec<PathBuf>) -> Self {
        let apps = files
            .first()
            .map(|f| open_with::apps_for(f))
            .unwrap_or_default();
        Self {
            files,
            apps,
            query: String::new(),
            chosen: 0,
            focus: true,
            fresh: true,
        }
    }

    fn filtered(&self) -> Vec<&App> {
        let query = self.query.to_lowercase();
        self.apps
            .iter()
            .filter(|a| query.is_empty() || a.name.to_lowercase().contains(&query))
            .collect()
    }
}

pub fn show(ctx: &egui::Context, app: &mut ExploristApp) {
    let Some(state) = &mut app.view.open_with else {
        return;
    };
    let p = app.palette;
    let mut close = false;
    let mut launch: Option<App> = None;
    let fresh = std::mem::take(&mut state.fresh);

    let title = match state.files.as_slice() {
        [one] => format!(
            "Open “{}” with",
            one.file_name()
                .map(|n| n.to_string_lossy().into_owned())
                .unwrap_or_default()
        ),
        many => format!("Open {} items with", many.len()),
    };

    let modal = Modal::new(Id::new("open-with"))
        .backdrop_color(p.scrim)
        .frame(
            Frame::NONE
                .fill(p.surface)
                .stroke(Stroke::new(1.0, p.border))
                .corner_radius(CornerRadius::same(radius::LG))
                .shadow(p.shadow_modal())
                .inner_margin(Margin::same(20)),
        )
        .show(ctx, |ui| {
            ui.set_width(420.0);
            ui.spacing_mut().item_spacing = vec2(0.0, 0.0);
            let heading = widgets::truncated(ui, &title, text::heading(), p.text, 420.0);
            let (r, _) = ui.allocate_exact_size(vec2(420.0, heading.size().y), Sense::hover());
            ui.painter().galley(r.min, heading, p.text);
            ui.add_space(14.0);

            let field = widgets::text_field(ui, &p, &mut state.query, 420.0, false, "Search apps");
            if std::mem::take(&mut state.focus) {
                field.request_focus();
            }
            if field.changed() {
                state.chosen = 0;
            }
            ui.add_space(10.0);

            let apps: Vec<App> = state.filtered().into_iter().cloned().collect();
            let (up, down, enter) = ui.input_mut(|i| {
                (
                    i.consume_key(egui::Modifiers::NONE, Key::ArrowUp),
                    i.consume_key(egui::Modifiers::NONE, Key::ArrowDown),
                    !fresh && i.key_pressed(Key::Enter),
                )
            });
            let mut scroll_to = None;
            if !apps.is_empty() {
                if down {
                    state.chosen = (state.chosen + 1).min(apps.len() - 1);
                    scroll_to = Some(state.chosen);
                }
                if up {
                    state.chosen = state.chosen.saturating_sub(1);
                    scroll_to = Some(state.chosen);
                }
                state.chosen = state.chosen.min(apps.len() - 1);
                if enter {
                    launch = Some(apps[state.chosen].clone());
                }
            }

            ScrollArea::vertical()
                .max_height(340.0)
                .auto_shrink([false, true])
                .show(ui, |ui| {
                    if apps.is_empty() {
                        let (r, _) = ui.allocate_exact_size(vec2(420.0, 60.0), Sense::hover());
                        let message = if state.apps.is_empty() {
                            "No apps found"
                        } else {
                            "No apps match"
                        };
                        ui.painter().text(
                            r.center(),
                            Align2::CENTER_CENTER,
                            message,
                            text::body(),
                            p.muted,
                        );
                    }
                    let mut last_group = None;
                    for (i, entry) in apps.iter().enumerate() {
                        if last_group != Some(entry.recommended) {
                            last_group = Some(entry.recommended);
                            let label = if entry.recommended {
                                "Suggested"
                            } else {
                                "Other apps"
                            };
                            let (r, _) = ui.allocate_exact_size(vec2(420.0, 26.0), Sense::hover());
                            ui.painter().text(
                                Pos2::new(r.left() + 10.0, r.center().y + 2.0),
                                Align2::LEFT_CENTER,
                                label,
                                text::caption(),
                                p.faint,
                            );
                        }
                        let (r, response) =
                            ui.allocate_exact_size(vec2(420.0, ROW), Sense::click());
                        if scroll_to == Some(i) {
                            ui.scroll_to_rect(r, None);
                        }
                        if response.hovered() && ui.input(|i| i.pointer.delta() != egui::Vec2::ZERO)
                        {
                            state.chosen = i;
                        }
                        if i == state.chosen {
                            ui.painter().rect_filled(
                                r,
                                CornerRadius::same(radius::SM),
                                p.accent_alpha(if p.dark { 0.16 } else { 0.10 }),
                            );
                        }
                        icons::paint(
                            ui.painter(),
                            egui::Rect::from_center_size(
                                Pos2::new(r.left() + 20.0, r.center().y),
                                vec2(16.0, 16.0),
                            ),
                            Icon::Exe,
                            16.0,
                            p.text_2,
                        );
                        let name = widgets::truncated(
                            ui,
                            &entry.name,
                            text::body(),
                            p.text,
                            r.width() - 50.0,
                        );
                        widgets::paint_aligned(
                            ui.painter(),
                            egui::Rect::from_min_max(Pos2::new(r.left() + 38.0, r.top()), r.max),
                            name,
                            Align2::LEFT_CENTER,
                            p.text,
                        );
                        if response.clicked() {
                            launch = Some(entry.clone());
                        }
                    }
                });

            ui.add_space(14.0);
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                ui.spacing_mut().item_spacing.x = 8.0;
                let open = widgets::button(ui, &p, "Open", ButtonStyle::Primary, Some("Enter"));
                let cancel = widgets::button(ui, &p, "Cancel", ButtonStyle::Ghost, Some("Esc"));
                if open.clicked()
                    && let Some(a) = apps.get(state.chosen)
                {
                    launch = Some(a.clone());
                }
                if cancel.clicked() {
                    close = true;
                }
            });
        });
    if modal.should_close() {
        close = true;
    }
    if let Some(chosen) = launch {
        let files = state.files.clone();
        close = true;
        if let Err(err) = open_with::launch(&chosen, &files) {
            app.toast_error(format!("Couldn’t open {}: {err}", chosen.name));
        }
    }
    if close {
        app.view.open_with = None;
    }
}
