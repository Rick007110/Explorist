//! The 28 px status bar: item counts, selection size, sort or progress.

use egui::{Align2, Frame, Pos2, Stroke};

use crate::app::ExploristApp;
use crate::app::browser::LoadState;
use crate::fs::format;
use crate::fs::sort::{SortColumn, SortDirection};
use crate::ui::theme::size;

pub fn show(ui: &mut egui::Ui, app: &mut ExploristApp) {
    let p = app.palette;
    egui::Panel::bottom("status")
        .exact_size(size::STATUS_BAR)
        .resizable(false)
        .show_separator_line(false)
        .frame(Frame::NONE.fill(p.bg_0))
        .show(ui, |ui| {
            let rect = ui.max_rect();
            ui.painter().hline(
                rect.x_range(),
                rect.top() + 0.5,
                Stroke::new(1.0, p.border_subtle),
            );
            if let Some(running) = &app.job {
                job_status(ui, &p, running, rect);
                return;
            }
            let painter = ui.painter();
            let font = crate::ui::fonts::mono(11.5);
            let y = rect.center().y;

            let browser = &app.browser;
            let shown = browser.rows().len();
            let total = browser.shown_total();
            let left = match browser.load_state() {
                LoadState::Loading { read } => {
                    format!("Reading… {} items", format::count(*read as u64))
                }
                LoadState::Failed(_) => "Couldn’t read this folder".to_owned(),
                LoadState::Ready {
                    cancelled: true, ..
                } => {
                    format!("{} items · stopped early", format::count(total as u64))
                }
                LoadState::Ready { .. } if browser.filter().is_active() => {
                    format!(
                        "{} of {} items · filtered",
                        format::count(shown as u64),
                        format::count(total as u64)
                    )
                }
                LoadState::Ready { .. } => items(shown),
            };
            let r = painter.text(
                Pos2::new(rect.left() + 14.0, y),
                Align2::LEFT_CENTER,
                left,
                font.clone(),
                p.muted,
            );

            let selected = browser.selected_entries();
            if !selected.is_empty() {
                let bytes: u64 = selected.iter().filter_map(|e| e.size).sum();
                let mut text = format!("{} selected", format::count(selected.len() as u64));
                if selected.iter().any(|e| e.size.is_some()) {
                    text.push_str(&format!(" · {}", format::size(bytes)));
                }
                painter.text(
                    Pos2::new(r.right() + 16.0, y),
                    Align2::LEFT_CENTER,
                    text,
                    font.clone(),
                    p.text_2,
                );
            }

            let right = if browser.is_loading() {
                "Esc to cancel".to_owned()
            } else {
                let sort = browser.sort();
                let column = match sort.column {
                    SortColumn::Name => "name",
                    SortColumn::Size => "size",
                    SortColumn::Modified => "date modified",
                    SortColumn::Type => "type",
                };
                let arrow = match sort.direction {
                    SortDirection::Ascending => "↑",
                    SortDirection::Descending => "↓",
                };
                format!("Sorted by {column} {arrow}")
            };
            let sort_rect = painter.text(
                Pos2::new(rect.right() - 14.0, y),
                Align2::RIGHT_CENTER,
                right,
                font.clone(),
                p.muted,
            );

            // Git: branch, ahead/behind, and how many files changed.
            if let Some(git) = app.git.poll().cloned() {
                let mut x = sort_rect.left() - 16.0;
                let changed = git.changed_count();
                if changed > 0 {
                    let r = painter.text(
                        Pos2::new(x, y),
                        Align2::RIGHT_CENTER,
                        format!("{changed} modified"),
                        font.clone(),
                        p.warning,
                    );
                    x = r.left() - 12.0;
                }
                if git.ahead > 0 || git.behind > 0 {
                    let mut sync = String::new();
                    if git.ahead > 0 {
                        sync.push_str(&format!("↑{}", git.ahead));
                    }
                    if git.behind > 0 {
                        if !sync.is_empty() {
                            sync.push(' ');
                        }
                        sync.push_str(&format!("↓{}", git.behind));
                    }
                    let r = painter.text(
                        Pos2::new(x, y),
                        Align2::RIGHT_CENTER,
                        sync,
                        font.clone(),
                        p.muted,
                    );
                    x = r.left() - 12.0;
                }
                let branch = git.branch.clone().unwrap_or_else(|| "detached".to_owned());
                let r = painter.text(Pos2::new(x, y), Align2::RIGHT_CENTER, branch, font, p.muted);
                let icon = egui::Rect::from_center_size(
                    Pos2::new(r.left() - 10.0, y),
                    egui::vec2(14.0, 14.0),
                );
                crate::ui::icons::paint(
                    painter,
                    icon,
                    crate::ui::icons::Icon::Branch,
                    14.0,
                    p.muted,
                );
            }
        });
}

fn items(n: usize) -> String {
    match n {
        1 => "1 item".to_owned(),
        n => format!("{} items", format::count(n as u64)),
    }
}

/// While a file operation runs: what it's doing, a progress bar, and
/// Cancel.
fn job_status(
    ui: &mut egui::Ui,
    p: &crate::ui::theme::Palette,
    running: &crate::app::files::RunningJob,
    rect: egui::Rect,
) {
    let progress = running.job.progress();
    let font = crate::ui::fonts::mono(11.5);
    let y = rect.center().y;

    let cancel_rect = egui::Rect::from_center_size(
        egui::Pos2::new(rect.right() - 40.0, y),
        egui::vec2(56.0, 22.0),
    );
    let cancel = ui.interact(
        cancel_rect,
        ui.id().with("cancel-job"),
        egui::Sense::click(),
    );
    if cancel.hovered() {
        ui.painter().rect_filled(cancel_rect, 4.0, p.hover);
    }
    ui.painter().text(
        cancel_rect.center(),
        Align2::CENTER_CENTER,
        "Cancel",
        crate::ui::fonts::medium(11.5),
        p.text_2,
    );
    if cancel.clicked() {
        running.job.cancel();
    }

    let bar = egui::Rect::from_min_size(
        egui::Pos2::new(cancel_rect.left() - 12.0 - 120.0, y - 2.0),
        egui::vec2(120.0, 4.0),
    );
    ui.painter().rect_filled(bar, 2.0, p.scroll_track);
    let mut fill = bar;
    fill.set_width(bar.width() * progress.fraction().clamp(0.0, 1.0));
    ui.painter().rect_filled(fill, 2.0, p.accent);

    let mut label = format!(
        "{} {} of {}",
        running.job.work.verb(),
        format::count((progress.items_done + 1).min(progress.items_total.max(1)) as u64),
        format::count(progress.items_total as u64)
    );
    if progress.bytes_total > 0 {
        label.push_str(&format!(
            " · {} of {}",
            format::size(progress.bytes_done),
            format::size(progress.bytes_total)
        ));
    }
    if !progress.current.is_empty() {
        label.push_str(&format!(" · {}", progress.current));
    }
    let max = bar.left() - 12.0 - rect.left() - 14.0;
    let galley = crate::ui::widgets::truncated(ui, &label, font, p.text_2, max);
    ui.painter().galley(
        egui::Pos2::new(rect.left() + 14.0, y - galley.size().y / 2.0),
        galley,
        p.text_2,
    );
}
