//! Confirmation for the one action that can't be undone, and the question
//! about files that are already where others are going.

use std::path::{Path, PathBuf};

use egui::{CornerRadius, Frame, Id, Margin, Modal, Stroke};

use crate::app::ExploristApp;
use crate::fs::jobs::Existing;
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

/// A copy or move found files of the same name where it's going (also
/// inside folders it puts into folders of the same name): replace them,
/// skip them, or keep both.
pub fn existing_files(ctx: &egui::Context, app: &mut ExploristApp) {
    let Some(existing) = app
        .job
        .as_ref()
        .and_then(|running| running.job.asking())
        .map(<[PathBuf]>::to_vec)
    else {
        return;
    };
    let p = app.palette;
    let title = match existing.as_slice() {
        [one] => format!("“{}” is already there", name(one)),
        many => format!("{} files are already there", many.len()),
    };
    let note = if cfg!(target_os = "macos") {
        "Replacing sends the ones there to the Recycle Bin."
    } else {
        "Replacing sends the ones there to the Recycle Bin, and Undo puts them back."
    };
    const LISTED: usize = 6;

    let mut decision: Option<Option<Existing>> = None;
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
            ui.set_width(460.0);
            ui.spacing_mut().item_spacing.y = 8.0;
            ui.label(
                egui::RichText::new(title)
                    .font(text::heading())
                    .color(p.text),
            );
            for path in existing.iter().take(LISTED) {
                let folder = path.parent().map(name).unwrap_or_default();
                ui.horizontal(|ui| {
                    ui.spacing_mut().item_spacing.x = 6.0;
                    ui.label(
                        egui::RichText::new(name(path))
                            .font(text::body())
                            .color(p.text),
                    );
                    ui.label(
                        egui::RichText::new(format!("in {folder}"))
                            .font(text::caption())
                            .color(p.muted),
                    );
                });
            }
            if existing.len() > LISTED {
                ui.label(
                    egui::RichText::new(format!("…and {} more", existing.len() - LISTED))
                        .font(text::caption())
                        .color(p.muted),
                );
            }
            ui.label(egui::RichText::new(note).font(text::body()).color(p.text_2));
            ui.add_space(12.0);
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                ui.spacing_mut().item_spacing.x = 8.0;
                let replace =
                    widgets::button(ui, &p, "Replace", ButtonStyle::Primary, Some("Enter"));
                let both = widgets::button(ui, &p, "Keep both", ButtonStyle::Secondary, None);
                let skip = widgets::button(ui, &p, "Skip", ButtonStyle::Secondary, None);
                let cancel = widgets::button(ui, &p, "Cancel", ButtonStyle::Ghost, Some("Esc"));
                if replace.clicked() || ui.input(|i| i.key_pressed(egui::Key::Enter)) {
                    decision = Some(Some(Existing::Replace));
                } else if both.clicked() {
                    decision = Some(Some(Existing::KeepBoth));
                } else if skip.clicked() {
                    decision = Some(Some(Existing::Skip));
                } else if cancel.clicked() {
                    decision = Some(None);
                }
            });
        });
    if modal.should_close() && decision.is_none() {
        decision = Some(None);
    }
    let (Some(decision), Some(running)) = (decision, app.job.as_mut()) else {
        return;
    };
    match decision {
        Some(choice) => running.job.answer(choice),
        None => running.job.cancel(),
    }
}

fn name(path: &Path) -> String {
    path.file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_else(|| path.display().to_string())
}
