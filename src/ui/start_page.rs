//! What a new tab shows before it goes anywhere: favorite places, drives
//! and recent folders to pick from.

use std::path::PathBuf;

use egui::{Align2, CornerRadius, Pos2, Rect, ScrollArea, Sense, Stroke, StrokeKind, vec2};

use crate::app::ExploristApp;
use crate::fs::format;
use crate::fs::places::PlaceIcon;
use crate::ui::icons::{self, Icon};
use crate::ui::theme::{Palette, radius, text};

const CARD: egui::Vec2 = egui::Vec2::new(220.0, 64.0);
const GAP: f32 = 10.0;

pub fn show(ui: &mut egui::Ui, app: &mut ExploristApp) {
    let p = app.palette;
    let mut go: Option<PathBuf> = None;
    let places: Vec<(String, String, PathBuf, Icon)> = app
        .places
        .iter()
        .map(|place| {
            let icon = match place.icon {
                PlaceIcon::Home => Icon::Home,
                PlaceIcon::Desktop => Icon::Desktop,
                PlaceIcon::Downloads => Icon::Downloads,
                PlaceIcon::Documents => Icon::Documents,
                PlaceIcon::Folder => Icon::Folder,
            };
            (
                place.label.clone(),
                place.path.display().to_string(),
                place.path.clone(),
                icon,
            )
        })
        .collect();
    let drives = app.drives.lock().map(|d| d.clone()).unwrap_or_default();
    let recent: Vec<PathBuf> = app.recent.iter().take(8).cloned().collect();

    ScrollArea::vertical()
        .id_salt("start-page")
        .auto_shrink([false, false])
        .show(ui, |ui| {
            egui::Frame::NONE
                .inner_margin(egui::Margin::symmetric(32, 28))
                .show(ui, |ui| {
                    ui.spacing_mut().item_spacing = vec2(GAP, GAP);
                    let width = ui.available_width();

                    heading(ui, &p, "Favorites");
                    cards(ui, width, places.len(), |ui, i, rect| {
                        let (label, detail, path, icon) = &places[i];
                        if card(ui, &p, rect, *icon, label, detail, None, ("place", i)) {
                            go = Some(path.clone());
                        }
                    });

                    ui.add_space(14.0);
                    heading(ui, &p, "Drives");
                    cards(ui, width, drives.len(), |ui, i, rect| {
                        let drive = &drives[i];
                        let detail = match (drive.free_bytes, drive.total_bytes) {
                            (Some(free), Some(total)) => {
                                format!("{} free of {}", format::size(free), format::size(total))
                            }
                            _ => drive.root.display().to_string(),
                        };
                        let name = drive.display_name();
                        if card(
                            ui,
                            &p,
                            rect,
                            Icon::Drive,
                            &name,
                            &detail,
                            drive.used_fraction(),
                            ("drive", i),
                        ) {
                            go = Some(drive.root.clone());
                        }
                    });

                    if !recent.is_empty() {
                        ui.add_space(14.0);
                        heading(ui, &p, "Recent");
                        for path in &recent {
                            let (rect, response) = ui
                                .allocate_exact_size(vec2(width.min(720.0), 30.0), Sense::click());
                            if response.hovered() {
                                ui.painter().rect_filled(
                                    rect,
                                    CornerRadius::same(radius::SM),
                                    p.hover,
                                );
                            }
                            icons::paint(
                                ui.painter(),
                                Rect::from_center_size(
                                    Pos2::new(rect.left() + 18.0, rect.center().y),
                                    vec2(16.0, 16.0),
                                ),
                                Icon::Folder,
                                16.0,
                                p.accent,
                            );
                            let name = path
                                .file_name()
                                .map(|n| n.to_string_lossy().into_owned())
                                .unwrap_or_else(|| path.display().to_string());
                            let label = ui.painter().text(
                                Pos2::new(rect.left() + 36.0, rect.center().y),
                                Align2::LEFT_CENTER,
                                name,
                                text::body_medium(),
                                p.text,
                            );
                            let g = crate::ui::widgets::truncated(
                                ui,
                                &path.display().to_string(),
                                crate::ui::fonts::mono(11.5),
                                p.muted,
                                rect.right() - label.right() - 24.0,
                            );
                            ui.painter().galley(
                                Pos2::new(label.right() + 14.0, rect.center().y - g.size().y / 2.0),
                                g,
                                p.muted,
                            );
                            if response.clicked() {
                                go = Some(path.clone());
                            }
                        }
                    }
                });
        });

    if let Some(path) = go {
        app.browser.navigate(path);
    }
}

fn heading(ui: &mut egui::Ui, p: &Palette, label: &str) {
    ui.label(
        egui::RichText::new(label)
            .font(text::label())
            .color(p.text_2),
    );
}

/// Lays out `count` cards in rows that fill `width`.
fn cards(
    ui: &mut egui::Ui,
    width: f32,
    count: usize,
    mut each: impl FnMut(&mut egui::Ui, usize, Rect),
) {
    let per_row = (((width + GAP) / (CARD.x + GAP)).floor() as usize).max(1);
    for row in 0..count.div_ceil(per_row) {
        let (line, _) = ui.allocate_exact_size(vec2(width, CARD.y), Sense::hover());
        for col in 0..per_row {
            let i = row * per_row + col;
            if i >= count {
                break;
            }
            let rect = Rect::from_min_size(line.min + vec2(col as f32 * (CARD.x + GAP), 0.0), CARD);
            each(ui, i, rect);
        }
    }
}

/// One card: icon, name and a detail line, with an optional usage bar.
/// Returns true when clicked.
#[allow(clippy::too_many_arguments)]
fn card(
    ui: &mut egui::Ui,
    p: &Palette,
    rect: Rect,
    icon: Icon,
    label: &str,
    detail: &str,
    used: Option<f32>,
    id: impl std::hash::Hash + std::fmt::Debug,
) -> bool {
    let response = ui.interact(rect, ui.id().with(id), Sense::click());
    ui.painter().rect(
        rect,
        CornerRadius::same(radius::MD),
        if response.hovered() {
            p.hover
        } else {
            p.surface
        },
        Stroke::new(1.0, p.border_subtle),
        StrokeKind::Inside,
    );
    let tile = Rect::from_min_size(rect.min + vec2(12.0, 14.0), vec2(36.0, 36.0));
    ui.painter()
        .rect_filled(tile, CornerRadius::same(radius::SM), p.bg_2);
    icons::paint(ui.painter(), tile, icon, 18.0, p.accent);
    let text_left = tile.right() + 12.0;
    let max = rect.right() - text_left - 12.0;
    let name = crate::ui::widgets::truncated(ui, label, text::body_medium(), p.text, max);
    ui.painter()
        .galley(Pos2::new(text_left, rect.top() + 14.0), name, p.text);
    let info = crate::ui::widgets::truncated(ui, detail, text::caption(), p.muted, max);
    ui.painter()
        .galley(Pos2::new(text_left, rect.top() + 33.0), info, p.muted);
    if let Some(used) = used {
        let track = Rect::from_min_size(Pos2::new(text_left, rect.bottom() - 11.0), vec2(max, 3.0));
        ui.painter()
            .rect_filled(track, CornerRadius::same(2), p.scroll_track);
        let mut fill = track;
        fill.set_width(track.width() * used.clamp(0.0, 1.0));
        ui.painter()
            .rect_filled(fill, CornerRadius::same(2), p.accent);
    }
    response
        .on_hover_cursor(egui::CursorIcon::PointingHand)
        .clicked()
}
