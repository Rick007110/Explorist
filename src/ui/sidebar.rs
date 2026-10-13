//! The 224 px sidebar: favorites and drives.

use egui::{Align2, CornerRadius, Frame, Margin, Pos2, Rect, Sense, Stroke, vec2};

use crate::app::ExploristApp;
use crate::fs::format;
use crate::fs::places::PlaceIcon;
use crate::ui::icons::{self, Icon};
use crate::ui::theme::{radius, size, text};
use crate::ui::widgets;

/// Free space below this fraction shows in the warning color.
const LOW_SPACE: f32 = 0.10;

pub fn show(ui: &mut egui::Ui, app: &mut ExploristApp) {
    let p = app.palette;
    egui::Panel::left("sidebar")
        .exact_size(size::SIDEBAR)
        .resizable(false)
        .show_separator_line(false)
        .frame(
            Frame::NONE
                .fill(p.bg_1)
                .inner_margin(Margin::symmetric(8, 12)),
        )
        .show(ui, |ui| {
            let full = ui.max_rect();
            ui.painter().vline(
                full.right() + 8.0 - 0.5,
                full.y_range().expand(12.0),
                Stroke::new(1.0, p.border_subtle),
            );
            ui.spacing_mut().item_spacing.y = 1.0;

            let location = app.browser.location().to_path_buf();
            let mut go = None;
            let drives = app.drives.lock().map(|d| d.clone()).unwrap_or_default();
            // Highlight the most specific place containing the current folder.
            let active_path = app
                .places
                .iter()
                .map(|place| &place.path)
                .chain(drives.iter().map(|d| &d.root))
                .filter(|path| location.starts_with(path))
                .max_by_key(|path| path.components().count())
                .cloned();

            widgets::section_label(ui, &p, "Favorites");
            for (i, place) in app.places.iter().enumerate() {
                let icon = match place.icon {
                    PlaceIcon::Home => Icon::Home,
                    PlaceIcon::Desktop => Icon::Desktop,
                    PlaceIcon::Downloads => Icon::Downloads,
                    PlaceIcon::Documents => Icon::Documents,
                    PlaceIcon::Folder => Icon::Folder,
                };
                let active = active_path.as_ref() == Some(&place.path);
                let tip = if i < 9 {
                    format!("{} (Alt {})", place.path.display(), i + 1)
                } else {
                    place.path.display().to_string()
                };
                let (rect, response) =
                    ui.allocate_exact_size(vec2(ui.available_width(), 30.0), Sense::click());
                paint_row_bg(ui, rect, active, response.hovered(), &p);
                let fg = if active { p.text } else { p.text_2 };
                let icon_color = if active && icon == Icon::Folder {
                    p.accent
                } else {
                    fg
                };
                icons::paint(
                    ui.painter(),
                    Rect::from_min_size(
                        Pos2::new(rect.left() + 10.0, rect.center().y - 8.0),
                        vec2(16.0, 16.0),
                    ),
                    icon,
                    16.0,
                    icon_color,
                );
                let font = if active {
                    text::body_medium()
                } else {
                    text::body()
                };
                let label = widgets::truncated(ui, &place.label, font, fg, rect.width() - 46.0);
                ui.painter().galley(
                    Pos2::new(rect.left() + 36.0, rect.center().y - label.size().y / 2.0),
                    label,
                    fg,
                );
                if response.on_hover_text(tip).clicked() {
                    go = Some(place.path.clone());
                }
            }

            ui.add_space(18.0);
            widgets::section_label(ui, &p, "Drives");
            for drive in &drives {
                let active = active_path.as_ref() == Some(&drive.root);
                let has_bar = drive.used_fraction().is_some();
                let height = if has_bar { 44.0 } else { 30.0 };
                let (rect, response) =
                    ui.allocate_exact_size(vec2(ui.available_width(), height), Sense::click());
                paint_row_bg(ui, rect, active, response.hovered(), &p);
                let fg = if active { p.text } else { p.text_2 };
                let line_y = if has_bar {
                    rect.top() + 7.0 + 10.0
                } else {
                    rect.center().y
                };
                icons::paint(
                    ui.painter(),
                    Rect::from_min_size(
                        Pos2::new(rect.left() + 10.0, line_y - 8.0),
                        vec2(16.0, 16.0),
                    ),
                    Icon::Drive,
                    16.0,
                    fg,
                );

                let low = drive.used_fraction().is_some_and(|u| 1.0 - u < LOW_SPACE);
                let free = drive
                    .free_bytes
                    .map(|b| format!("{} free", format::size(b)));
                let free_w = free.as_ref().map_or(0.0, |f| {
                    let g = ui.painter().layout_no_wrap(
                        f.clone(),
                        crate::ui::fonts::mono(11.0),
                        p.muted,
                    );
                    let w = g.size().x;
                    ui.painter().galley(
                        Pos2::new(rect.right() - 10.0 - w, line_y - g.size().y / 2.0),
                        g,
                        if low { p.warning } else { p.muted },
                    );
                    w
                });
                let name = widgets::truncated(
                    ui,
                    &drive.display_name(),
                    text::body(),
                    fg,
                    rect.width() - 46.0 - free_w - 8.0,
                );
                ui.painter().galley(
                    Pos2::new(rect.left() + 36.0, line_y - name.size().y / 2.0),
                    name,
                    fg,
                );

                if let Some(used) = drive.used_fraction() {
                    let track = Rect::from_min_size(
                        Pos2::new(rect.left() + 10.0, rect.bottom() - 9.0 - 3.0),
                        vec2(rect.width() - 20.0, 3.0),
                    );
                    ui.painter()
                        .rect_filled(track, CornerRadius::same(2), p.scroll_track);
                    let mut fill = track;
                    fill.set_width(track.width() * used.clamp(0.0, 1.0));
                    ui.painter().rect_filled(
                        fill,
                        CornerRadius::same(2),
                        if low { p.warning } else { p.scroll_thumb },
                    );
                }
                if response.clicked() {
                    go = Some(drive.root.clone());
                }
            }

            // "Jump to place  Alt 1–9" at the bottom.
            let bottom = Rect::from_min_max(
                Pos2::new(full.left(), full.bottom() - 16.0),
                full.right_bottom(),
            );
            let hint = ui.painter().text(
                Pos2::new(bottom.left() + 10.0, bottom.center().y),
                Align2::LEFT_CENTER,
                "Jump to place",
                text::caption(),
                p.muted,
            );
            let kbd = widgets::kbd_galley(ui, &p, "Alt 1–9");
            widgets::paint_kbd(
                ui.painter(),
                &p,
                kbd,
                hint.right() + 6.0,
                bottom.center().y,
                false,
            );

            if let Some(path) = go {
                app.browser.navigate(path);
            }
        });
}

fn paint_row_bg(
    ui: &egui::Ui,
    rect: Rect,
    active: bool,
    hovered: bool,
    p: &crate::ui::theme::Palette,
) {
    let fill = if active {
        p.pressed
    } else if hovered {
        p.hover
    } else {
        return;
    };
    ui.painter()
        .rect_filled(rect, CornerRadius::same(radius::SM), fill);
}
