//! The drive menu's dialogs: renaming a drive and its properties.

use egui::{CornerRadius, Frame, Id, Margin, Modal, Pos2, Rect, Sense, Stroke, vec2};

use crate::app::{ExploristApp, Notice};
use crate::fs::places::{Drive, DriveKind};
use crate::fs::{format, volumes};
use crate::ui::theme::{radius, text};
use crate::ui::widgets::{self, ButtonStyle};

pub enum DriveDialog {
    Rename {
        drive: Drive,
        name: String,
        focus: bool,
    },
    Properties {
        drive: Drive,
        file_system: Option<String>,
    },
}

impl DriveDialog {
    pub fn rename(drive: Drive) -> Self {
        Self::Rename {
            name: drive.volume_label.clone(),
            drive,
            focus: true,
        }
    }

    pub fn properties(drive: Drive) -> Self {
        let file_system = volumes::file_system(&drive.root);
        Self::Properties { drive, file_system }
    }
}

pub fn show(ctx: &egui::Context, app: &mut ExploristApp) {
    let Some(dialog) = &mut app.view.drive_dialog else {
        return;
    };
    let p = app.palette;
    let mut close = false;
    let mut rename_to: Option<(Drive, String)> = None;

    let modal = Modal::new(Id::new("drive-dialog"))
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
            ui.set_width(380.0);
            ui.spacing_mut().item_spacing = vec2(0.0, 8.0);
            match dialog {
                DriveDialog::Rename { drive, name, focus } => {
                    ui.label(
                        egui::RichText::new(format!("Rename {}", drive.display_name()))
                            .font(text::heading())
                            .color(p.text),
                    );
                    let field = widgets::text_field(ui, &p, name, 380.0, false, "Drive name");
                    if std::mem::take(focus) {
                        field.request_focus();
                    }
                    let enter = field.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter));
                    ui.add_space(8.0);
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        ui.spacing_mut().item_spacing.x = 8.0;
                        let ok =
                            widgets::button(ui, &p, "Rename", ButtonStyle::Primary, Some("Enter"));
                        let cancel =
                            widgets::button(ui, &p, "Cancel", ButtonStyle::Ghost, Some("Esc"));
                        if ok.clicked() || enter {
                            rename_to = Some((drive.clone(), name.trim().to_owned()));
                        }
                        if cancel.clicked() {
                            close = true;
                        }
                    });
                }
                DriveDialog::Properties { drive, file_system } => {
                    ui.label(
                        egui::RichText::new(drive.display_name())
                            .font(text::heading())
                            .color(p.text),
                    );
                    let kind = match drive.kind {
                        DriveKind::Fixed => "Local disk",
                        DriveKind::Removable => "Removable drive",
                        DriveKind::Network => "Network drive",
                        DriveKind::Optical => "Disc drive",
                        DriveKind::Other => "Drive",
                    };
                    row(ui, &p, "Type", kind);
                    row(ui, &p, "Location", &drive.root.display().to_string());
                    if let Some(fs) = file_system {
                        row(ui, &p, "File system", fs);
                    }
                    if let (Some(total), Some(free)) = (drive.total_bytes, drive.free_bytes) {
                        let used = total.saturating_sub(free);
                        row(ui, &p, "Capacity", &bytes(total));
                        row(ui, &p, "Used", &bytes(used));
                        row(ui, &p, "Free", &bytes(free));
                        // Used space as a bar.
                        let (r, _) = ui.allocate_exact_size(vec2(380.0, 10.0), Sense::hover());
                        ui.painter()
                            .rect_filled(r, CornerRadius::same(4), p.scroll_track);
                        if total > 0 {
                            let mut fill = r;
                            fill.set_width(
                                r.width() * (used as f32 / total as f32).clamp(0.0, 1.0),
                            );
                            ui.painter()
                                .rect_filled(fill, CornerRadius::same(4), p.accent);
                        }
                    }
                    ui.add_space(8.0);
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        let done =
                            widgets::button(ui, &p, "Close", ButtonStyle::Primary, Some("Esc"));
                        if done.clicked() {
                            close = true;
                        }
                    });
                }
            }
        });
    if modal.should_close() {
        close = true;
    }
    if let Some((drive, name)) = rename_to {
        close = true;
        let label = drive.display_name();
        app.background(move || match volumes::rename(&drive.root, &name) {
            Ok(()) => Notice {
                text: format!("Renamed {label} to “{name}”"),
                error: false,
            },
            Err(err) => Notice {
                text: format!("Couldn’t rename {label}: {err}"),
                error: true,
            },
        });
    }
    if close {
        app.view.drive_dialog = None;
    }
}

/// "1.2 TB (1,234,567,890,123 bytes)".
fn bytes(n: u64) -> String {
    format!("{} ({})", format::size(n), format::exact_size(n))
}

fn row(ui: &mut egui::Ui, p: &crate::ui::theme::Palette, label: &str, value: &str) {
    let (r, _) = ui.allocate_exact_size(vec2(380.0, 20.0), Sense::hover());
    ui.painter().text(
        Pos2::new(r.left(), r.center().y),
        egui::Align2::LEFT_CENTER,
        label,
        text::body(),
        p.muted,
    );
    let value_rect = Rect::from_min_max(Pos2::new(r.left() + 100.0, r.top()), r.max);
    let g = widgets::truncated(
        ui,
        value,
        crate::ui::fonts::mono(12.0),
        p.text_2,
        value_rect.width(),
    );
    widgets::paint_aligned(
        ui.painter(),
        value_rect,
        g,
        egui::Align2::LEFT_CENTER,
        p.text_2,
    );
}

/// Ejects `drive` in the background, leaving it first if it's open.
pub fn eject(app: &mut ExploristApp, drive: Drive) {
    if app.browser.location().starts_with(&drive.root) {
        // Our own folder handle would keep the drive busy.
        let home = dirs::home_dir().unwrap_or_else(|| std::path::PathBuf::from("/"));
        app.browser.navigate(home);
    }
    let label = drive.display_name();
    app.background(move || match volumes::eject(&drive.root) {
        Ok(()) => Notice {
            text: format!("{label} can be removed safely"),
            error: false,
        },
        Err(err) => Notice {
            text: format!("Couldn’t eject {label}: {err}"),
            error: true,
        },
    });
}
