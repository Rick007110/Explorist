//! Quick Look (Space): a large preview of the focused item over the window.
//! The arrow keys move to the next item while it's open; Space or Esc
//! closes it.

use egui::{Align2, Color32, CornerRadius, Frame, Margin, Pos2, Rect, Sense, Stroke, vec2};

use crate::app::ExploristApp;
use crate::fs::format;
use crate::fs::preview::Content;
use crate::ui::icons::{self, Icon};
use crate::ui::theme::{radius, text};

pub fn show(ctx: &egui::Context, app: &mut ExploristApp) {
    if !app.view.quick_look {
        return;
    }
    let Some(entry) = app
        .browser
        .selection
        .focused_row(app.browser.rows())
        .and_then(|row| app.browser.rows().get(row))
        .cloned()
    else {
        app.view.quick_look = false;
        return;
    };
    let p = app.palette;
    let screen = ctx.content_rect();

    // Dim everything behind; a click there closes.
    let scrim = egui::Area::new(egui::Id::new("quick-look-scrim"))
        .order(egui::Order::Foreground)
        .fixed_pos(screen.min)
        .show(ctx, |ui| {
            let (rect, response) = ui.allocate_exact_size(screen.size(), Sense::click());
            ui.painter().rect_filled(rect, 0.0, p.scrim);
            response
        })
        .inner;
    if scrim.clicked() {
        app.view.quick_look = false;
        return;
    }

    let wake = ctx.clone();
    app.preview
        .request(&entry.path, entry.kind, entry.is_dir, move || {
            wake.request_repaint()
        });
    let content = app.preview.poll().cloned();

    let size = vec2(
        (screen.width() * 0.72).clamp(420.0, 1100.0),
        (screen.height() * 0.78).clamp(320.0, 820.0),
    );
    let panel = Rect::from_center_size(screen.center(), size);
    egui::Area::new(egui::Id::new("quick-look"))
        .order(egui::Order::Foreground)
        .fixed_pos(panel.min)
        .show(ctx, |ui| {
            Frame::NONE
                .fill(p.surface)
                .stroke(Stroke::new(1.0, p.border))
                .corner_radius(CornerRadius::same(radius::LG))
                .shadow(p.shadow_modal())
                .inner_margin(Margin::same(16))
                .show(ui, |ui| {
                    ui.set_min_size(size - vec2(32.0, 32.0));
                    ui.set_max_size(size - vec2(32.0, 32.0));
                    ui.spacing_mut().item_spacing.y = 10.0;
                    title(ui, &p, &entry);
                    let body = ui.available_rect_before_wrap();
                    match &content {
                        Some(Content::Image { pixels, scaled, .. }) => {
                            crate::ui::preview::ensure_image(
                                ui.ctx(),
                                app,
                                &entry.path,
                                pixels,
                                *scaled,
                            );
                            if let Some(img) = &app.preview_image {
                                let s = vec2(scaled[0] as f32, scaled[1] as f32);
                                let fit = (body.width() / s.x).min(body.height() / s.y);
                                let shown = Rect::from_center_size(body.center(), s * fit);
                                ui.painter().rect_filled(
                                    body,
                                    CornerRadius::same(radius::MD),
                                    p.bg_0,
                                );
                                ui.painter().image(
                                    img.texture.id(),
                                    shown,
                                    Rect::from_min_max(Pos2::ZERO, Pos2::new(1.0, 1.0)),
                                    Color32::WHITE,
                                );
                            }
                        }
                        Some(Content::Text { lines, truncated }) => {
                            egui::ScrollArea::vertical()
                                .id_salt(("quick-look-text", &entry.path))
                                .max_height(body.height())
                                .show(ui, |ui| {
                                    crate::ui::preview::code_block(ui, &p, lines, *truncated);
                                });
                        }
                        other => {
                            let message = match other {
                                Some(Content::Folder { items }) => {
                                    format!("{} items", format::count(*items as u64))
                                }
                                Some(Content::Error(err)) => format!("Couldn’t preview: {err}"),
                                None => "Loading…".to_owned(),
                                _ => "No preview for this kind of file".to_owned(),
                            };
                            let center = body.center();
                            icons::paint_stroked(
                                ui.painter(),
                                Rect::from_center_size(center - vec2(0.0, 30.0), vec2(96.0, 96.0)),
                                Icon::for_kind(entry.kind),
                                96.0,
                                2.5,
                                if entry.is_dir { p.accent } else { p.text_2 },
                            );
                            ui.painter().text(
                                center + vec2(0.0, 44.0),
                                Align2::CENTER_CENTER,
                                message,
                                text::body(),
                                p.muted,
                            );
                        }
                    }
                });
        });
}

fn title(ui: &mut egui::Ui, p: &crate::ui::theme::Palette, entry: &crate::fs::entry::Entry) {
    let (rect, _) = ui.allocate_exact_size(vec2(ui.available_width(), 24.0), Sense::hover());
    let mut detail = entry.type_label.to_string();
    if let Some(size) = entry.size {
        detail.push_str(&format!(" · {}", format::size(size)));
    }
    let detail_galley = ui
        .painter()
        .layout_no_wrap(detail, text::caption(), p.muted);
    let name = crate::ui::widgets::truncated(
        ui,
        &entry.name,
        text::heading(),
        p.text,
        rect.width() - detail_galley.size().x - 24.0,
    );
    ui.painter().galley(
        Pos2::new(rect.left(), rect.center().y - name.size().y / 2.0),
        name,
        p.text,
    );
    ui.painter().galley(
        Pos2::new(
            rect.right() - detail_galley.size().x,
            rect.center().y - detail_galley.size().y / 2.0,
        ),
        detail_galley,
        p.muted,
    );
}
