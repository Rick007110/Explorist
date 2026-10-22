//! Confirmation for the one action that can't be undone, and the question
//! about files that are already where others are going.

use std::fs;
use std::path::Path;

use egui::{CornerRadius, Frame, Id, Margin, Modal, Stroke};

use crate::app::ExploristApp;
use crate::fs::format::{self, DateFormat};
use crate::fs::jobs::{Answer, Clash, Existing};
use crate::ui::theme::{radius, text};
use crate::ui::widgets::{self, ButtonStyle};

pub fn confirm_delete(ctx: &egui::Context, app: &mut ExploristApp) {
    let Some(paths) = app.view.confirm_delete.clone() else {
        return;
    };
    let p = app.palette;
    let count = paths.len();
    let title = match count {
        1 => format!(
            "Delete “{}” permanently?",
            paths[0]
                .file_name()
                .map(|n| n.to_string_lossy().into_owned())
                .unwrap_or_default()
        ),
        n => format!("Delete {n} items permanently?"),
    };
    let confirm_label = match count {
        1 => "Delete".to_owned(),
        n => format!("Delete {n} items"),
    };

    let mut decision: Option<bool> = None;
    let modal = Modal::new(Id::new("confirm-delete"))
        .backdrop_color(p.scrim)
        .frame(
            Frame::NONE
                .fill(p.surface)
                .stroke(Stroke::new(1.0, p.border))
                .corner_radius(CornerRadius::same(radius::LG))
                .shadow(p.shadow_modal())
                .inner_margin(Margin::same(24)),
        )
        .show(ctx, |ui| {
            ui.set_width(420.0);
            ui.spacing_mut().item_spacing.y = 8.0;
            ui.label(
                egui::RichText::new(title)
                    .font(text::heading())
                    .color(p.text),
            );
            ui.label(
                egui::RichText::new("They won’t go to the Recycle Bin, and this can’t be undone.")
                    .font(text::body())
                    .color(p.text_2),
            );
            ui.add_space(12.0);
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                ui.spacing_mut().item_spacing.x = 8.0;
                let confirm = widgets::button(
                    ui,
                    &p,
                    &confirm_label,
                    ButtonStyle::Destructive,
                    Some("Enter"),
                );
                let cancel = widgets::button(ui, &p, "Cancel", ButtonStyle::Ghost, Some("Esc"));
                if confirm.clicked() || ui.input(|i| i.key_pressed(egui::Key::Enter)) {
                    decision = Some(true);
                } else if cancel.clicked() {
                    decision = Some(false);
                }
            });
        });
    if modal.should_close() && decision.is_none() {
        decision = Some(false);
    }
    match decision {
        Some(true) => {
            app.view.confirm_delete = None;
            app.delete_permanently(paths);
        }
        Some(false) => app.view.confirm_delete = None,
        None => {}
    }
}

/// The "already there" question's progress when choosing file by file.
#[derive(Debug, Default)]
pub struct ExistingDialog {
    /// Choosing for each file rather than for all at once.
    pub each: bool,
    /// The choices so far, one per file in the order asked.
    pub chosen: Vec<Existing>,
    /// Use the next choice for every file still left.
    pub rest: bool,
}

/// A copy or move found files of the same name where it's going (also
/// inside folders it puts into folders of the same name): replace them,
/// skip them or keep both, for all at once or file by file.
pub fn existing_files(ctx: &egui::Context, app: &mut ExploristApp) {
    let Some(clashes) = app
        .job
        .as_ref()
        .and_then(|running| running.job.asking())
        .map(<[Clash]>::to_vec)
    else {
        // Nothing asked: forget a half-finished round.
        if app.view.existing.each || !app.view.existing.chosen.is_empty() {
            app.view.existing = ExistingDialog::default();
        }
        return;
    };
    let p = app.palette;
    let date_format = app.settings.date_format;
    let total = clashes.len();
    let each = app.view.existing.each && total > 1;
    let index = app.view.existing.chosen.len().min(total - 1);
    let left = total - index - 1;
    let note = if cfg!(target_os = "macos") {
        "Replacing sends the ones there to the Recycle Bin."
    } else {
        "Replacing sends the ones there to the Recycle Bin, and Undo puts them back."
    };
    const LISTED: usize = 6;

    enum Click {
        Choose(Existing),
        ForEach,
        Cancel,
    }
    let mut click = None;
    let mut rest = app.view.existing.rest;
    let modal = Modal::new(Id::new("existing-files"))
        .backdrop_color(p.scrim)
        .frame(
            Frame::NONE
                .fill(p.surface)
                .stroke(Stroke::new(1.0, p.border))
                .corner_radius(CornerRadius::same(radius::LG))
                .shadow(p.shadow_modal())
                .inner_margin(Margin::same(24)),
        )
        .show(ctx, |ui| {
            ui.set_width(480.0);
            ui.spacing_mut().item_spacing.y = 8.0;
            let caption = |ui: &mut egui::Ui, text: String| {
                ui.label(
                    egui::RichText::new(text)
                        .font(text::caption())
                        .color(p.muted),
                );
            };
            if each || total == 1 {
                let clash = &clashes[index];
                let title = format!("“{}” is already there", name(&clash.there));
                ui.label(
                    egui::RichText::new(title)
                        .font(text::heading())
                        .color(p.text),
                );
                let folder = clash.there.parent().map(name).unwrap_or_default();
                caption(
                    ui,
                    if each {
                        format!("File {} of {total} · in {folder}", index + 1)
                    } else {
                        format!("In {folder}")
                    },
                );
                compare(ui, &p, clash, date_format);
            } else {
                ui.label(
                    egui::RichText::new(format!("{total} files are already there"))
                        .font(text::heading())
                        .color(p.text),
                );
                ui.spacing_mut().item_spacing.y = 4.0;
                for clash in clashes.iter().take(LISTED) {
                    let folder = clash.there.parent().map(name).unwrap_or_default();
                    let mut line = egui::text::LayoutJob::default();
                    line.append(
                        &name(&clash.there),
                        0.0,
                        egui::TextFormat::simple(text::body(), p.text),
                    );
                    line.append(
                        &format!("in {folder}"),
                        6.0,
                        egui::TextFormat::simple(text::caption(), p.muted),
                    );
                    ui.label(line);
                }
                ui.spacing_mut().item_spacing.y = 8.0;
                if total > LISTED {
                    caption(ui, format!("…and {} more", total - LISTED));
                }
            }
            ui.label(egui::RichText::new(note).font(text::body()).color(p.text_2));
            if each && left > 0 {
                ui.horizontal(|ui| {
                    ui.spacing_mut().item_spacing.x = 10.0;
                    widgets::switch(ui, &p, &mut rest);
                    let files = if left == 1 { "file" } else { "files" };
                    ui.label(
                        egui::RichText::new(format!("Do the same for the {left} {files} left"))
                            .font(text::body())
                            .color(p.text),
                    );
                });
            }
            if !each && total > 1 {
                let for_each =
                    widgets::button(ui, &p, "Choose for each file", ButtonStyle::Secondary, None);
                if for_each.clicked() {
                    click = Some(Click::ForEach);
                }
            }
            ui.add_space(12.0);
            ui.horizontal(|ui| {
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    ui.spacing_mut().item_spacing.x = 8.0;
                    let all = !each && total > 1;
                    let replace = widgets::button(
                        ui,
                        &p,
                        if all { "Replace all" } else { "Replace" },
                        ButtonStyle::Primary,
                        Some("Enter"),
                    );
                    let both = widgets::button(ui, &p, "Keep both", ButtonStyle::Secondary, None);
                    let skip = widgets::button(
                        ui,
                        &p,
                        if all { "Skip all" } else { "Skip" },
                        ButtonStyle::Secondary,
                        None,
                    );
                    let cancel = widgets::button(ui, &p, "Cancel", ButtonStyle::Ghost, Some("Esc"));
                    if replace.clicked() || ui.input(|i| i.key_pressed(egui::Key::Enter)) {
                        click = Some(Click::Choose(Existing::Replace));
                    } else if both.clicked() {
                        click = Some(Click::Choose(Existing::KeepBoth));
                    } else if skip.clicked() {
                        click = Some(Click::Choose(Existing::Skip));
                    } else if cancel.clicked() {
                        click = Some(Click::Cancel);
                    }
                });
            });
        });
    app.view.existing.rest = rest;
    if modal.should_close() && click.is_none() {
        click = Some(Click::Cancel);
    }
    let answer = match click {
        None => return,
        Some(Click::ForEach) => {
            app.view.existing = ExistingDialog {
                each: true,
                ..ExistingDialog::default()
            };
            return;
        }
        Some(Click::Cancel) => None,
        Some(Click::Choose(choice)) if !each => Some(Answer::All(choice)),
        Some(Click::Choose(choice)) => {
            let state = &mut app.view.existing;
            state.chosen.push(choice);
            if state.rest {
                state.chosen.resize(total, choice);
            }
            if state.chosen.len() < total {
                return;
            }
            Some(Answer::Each(std::mem::take(&mut state.chosen)))
        }
    };
    app.view.existing = ExistingDialog::default();
    if let Some(running) = app.job.as_mut() {
        match answer {
            Some(answer) => running.job.answer(answer),
            None => running.job.cancel(),
        }
    }
}

/// The one there and the one coming in, by size and date, marking the
/// newer one.
fn compare(
    ui: &mut egui::Ui,
    p: &crate::ui::theme::Palette,
    clash: &Clash,
    date_format: DateFormat,
) {
    let there = fs::metadata(&clash.there).ok();
    let from = fs::metadata(&clash.from).ok();
    let modified = |m: &Option<fs::Metadata>| m.as_ref().and_then(|m| m.modified().ok());
    let newer_from = match (modified(&there), modified(&from)) {
        (Some(a), Some(b)) if a != b => Some(b > a),
        _ => None,
    };
    let rows = [
        ("There now", &there, newer_from == Some(false)),
        ("Coming in", &from, newer_from == Some(true)),
    ];
    egui::Grid::new(("existing-compare", &clash.there))
        .num_columns(3)
        .spacing([12.0, 4.0])
        .show(ui, |ui| {
            for (label, meta, newer) in rows {
                ui.label(
                    egui::RichText::new(label)
                        .font(text::caption())
                        .color(p.muted),
                );
                let details = match meta {
                    Some(m) => {
                        let date = m
                            .modified()
                            .map(|t| format::date(t, date_format))
                            .unwrap_or_default();
                        format!("{} · {date}", format::size(m.len()))
                    }
                    None => "—".to_owned(),
                };
                ui.label(
                    egui::RichText::new(details)
                        .font(text::mono())
                        .color(p.text),
                );
                ui.label(
                    egui::RichText::new(if newer { "newer" } else { "" })
                        .font(text::caption())
                        .color(p.accent),
                );
                ui.end_row();
            }
        });
}

fn name(path: &Path) -> String {
    path.file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_else(|| path.display().to_string())
}
