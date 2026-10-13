//! Confirmation for the one action that can't be undone.

use egui::{CornerRadius, Frame, Id, Margin, Modal, Stroke};

use crate::app::ExploristApp;
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
