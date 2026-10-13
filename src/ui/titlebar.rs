//! The 40 px top bar: logo, the (single) tab, and window buttons. It ships
//! with one tab so adding tabs later changes no layout.

use egui::{
    Align2, Color32, CornerRadius, Frame, Pos2, Rect, Sense, Stroke, StrokeKind, ViewportCommand,
    vec2,
};

use crate::app::ExploristApp;
use crate::ui::icons::{self, Icon};
use crate::ui::theme::{radius, size};
use crate::ui::widgets;

/// Windows' own hover color for the close button.
const CLOSE_HOVER: Color32 = Color32::from_rgb(0xC4, 0x2B, 0x1C);

pub fn show(ui: &mut egui::Ui, app: &mut ExploristApp) {
    let p = app.palette;
    egui::Panel::top("titlebar")
        .exact_size(size::TAB_BAR)
        .resizable(false)
        .show_separator_line(false)
        .frame(Frame::NONE.fill(p.bg_0))
        .show(ui, |ui| {
            let bar = ui.max_rect();

            // Everything not covered by a control drags the window.
            let drag = ui.interact(bar, ui.id().with("drag"), Sense::click_and_drag());
            if drag.drag_started() {
                ui.ctx().send_viewport_cmd(ViewportCommand::StartDrag);
            }
            if drag.double_clicked() {
                ui.ctx()
                    .send_viewport_cmd(ViewportCommand::Maximized(!app.view.maximized));
            }

            // Logo.
            let logo = Rect::from_center_size(
                Pos2::new(bar.left() + 12.0 + 9.0, bar.center().y),
                vec2(18.0, 18.0),
            );
            paint_logo(ui.painter(), logo, p.accent);

            // The tab.
            let name = app
                .browser
                .location()
                .file_name()
                .map(|n| n.to_string_lossy().into_owned())
                .unwrap_or_else(|| app.browser.location().display().to_string());
            let tab_left = logo.right() + 14.0;
            let max_text = 240.0 - 12.0 - 16.0 - 8.0 - 12.0;
            let galley = widgets::truncated(
                ui,
                &name,
                crate::ui::theme::text::body_medium(),
                p.text,
                max_text,
            );
            let tab = Rect::from_min_size(
                Pos2::new(tab_left, bar.bottom() - 34.0),
                vec2(12.0 + 16.0 + 8.0 + galley.size().x + 12.0, 34.0),
            );
            ui.painter().rect_filled(
                tab,
                CornerRadius {
                    nw: radius::MD,
                    ne: radius::MD,
                    sw: 0,
                    se: 0,
                },
                p.bg_2,
            );
            let icon_rect = Rect::from_min_size(
                Pos2::new(tab.left() + 12.0, tab.center().y - 8.0),
                vec2(16.0, 16.0),
            );
            icons::paint(ui.painter(), icon_rect, Icon::Folder, 16.0, p.accent);
            widgets::paint_aligned(
                ui.painter(),
                Rect::from_min_max(
                    Pos2::new(icon_rect.right() + 8.0, tab.top()),
                    tab.right_bottom(),
                ),
                galley,
                Align2::LEFT_CENTER,
                p.text,
            );

            // Window buttons.
            let buttons = [
                (Icon::Minimize, "Minimize", WindowButton::Minimize),
                (
                    if app.view.maximized {
                        Icon::Restore
                    } else {
                        Icon::Maximize
                    },
                    if app.view.maximized {
                        "Restore"
                    } else {
                        "Maximize"
                    },
                    WindowButton::Maximize,
                ),
                (Icon::CloseWindow, "Close", WindowButton::Close),
            ];
            for (i, (icon, label, which)) in buttons.into_iter().enumerate() {
                let right = bar.right() - size::WINDOW_BUTTON_WIDTH * (2 - i) as f32;
                let rect = Rect::from_min_max(
                    Pos2::new(right - size::WINDOW_BUTTON_WIDTH, bar.top()),
                    Pos2::new(right, bar.bottom()),
                );
                let response = ui
                    .interact(rect, ui.id().with(label), Sense::click())
                    .on_hover_text(label);
                let (fill, fg) = match (which, response.hovered()) {
                    (WindowButton::Close, true) => (CLOSE_HOVER, Color32::WHITE),
                    (_, true) => (p.hover, p.text),
                    _ => (Color32::TRANSPARENT, p.text_2),
                };
                ui.painter().rect_filled(rect, 0.0, fill);
                icons::paint(ui.painter(), rect, icon, 10.0, fg);
                if response.clicked() {
                    let cmd = match which {
                        WindowButton::Minimize => ViewportCommand::Minimized(true),
                        WindowButton::Maximize => ViewportCommand::Maximized(!app.view.maximized),
                        WindowButton::Close => ViewportCommand::Close,
                    };
                    ui.ctx().send_viewport_cmd(cmd);
                }
            }
        });
}

#[derive(Clone, Copy)]
enum WindowButton {
    Minimize,
    Maximize,
    Close,
}

/// The app mark: a rounded square with three lines, in the accent.
fn paint_logo(painter: &egui::Painter, rect: Rect, color: Color32) {
    let s = rect.width() / 18.0;
    let stroke = Stroke::new(1.5 * s, color);
    painter.rect_stroke(
        Rect::from_min_size(rect.min + vec2(1.5, 1.5) * s, vec2(15.0, 15.0) * s),
        4.0 * s,
        stroke,
        StrokeKind::Middle,
    );
    for (y, len) in [(6.5, 7.0), (9.0, 7.0), (11.5, 4.0)] {
        let a = rect.min + vec2(5.5, y) * s;
        painter.line_segment([a, a + vec2(len, 0.0) * s], stroke);
    }
}
