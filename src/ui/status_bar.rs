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
            let painter = ui.painter();
            painter.hline(
                rect.x_range(),
                rect.top() + 0.5,
                Stroke::new(1.0, p.border_subtle),
            );
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
                if selected.iter().any(|e| !e.is_dir) {
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
            painter.text(
                Pos2::new(rect.right() - 14.0, y),
                Align2::RIGHT_CENTER,
                right,
                font,
                p.muted,
            );
        });
}

fn items(n: usize) -> String {
    match n {
        1 => "1 item".to_owned(),
        n => format!("{} items", format::count(n as u64)),
    }
}
