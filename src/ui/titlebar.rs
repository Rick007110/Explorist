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

            // Tabs: the folder, plus Settings while it's open.
            let mut x = logo.right() + 14.0;
            let folder_name = app
                .browser
                .location()
                .file_name()
                .map(|n| n.to_string_lossy().into_owned())
                .unwrap_or_else(|| app.browser.location().display().to_string());
            let settings_open = app.view.settings_open;
            let (rect, folder) = tab(
                ui,
                &p,
                x,
                &folder_name,
                Icon::Folder,
                !settings_open,
                false,
                "folder-tab",
            );
            x = rect.right() + 2.0;
            if folder.clicked {
                app.view.settings_open = false;
            }
            if settings_open {
                let (_, settings) = tab(
                    ui,
                    &p,
                    x,
                    "Settings",
                    Icon::Config,
                    true,
                    true,
                    "settings-tab",
                );
                if settings.closed {
                    app.view.settings_open = false;
                }
            }

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

/// What happened to a tab this frame.
#[derive(Default)]
pub struct TabResponse {
    pub clicked: bool,
    pub closed: bool,
}

/// Draws a 34 px tab at `left`. The active tab shares the toolbar color
/// and "opens" into it; no underline, no accent fill.
#[allow(clippy::too_many_arguments)]
pub fn tab(
    ui: &mut egui::Ui,
    p: &crate::ui::theme::Palette,
    left: f32,
    label: &str,
    icon: Icon,
    active: bool,
    closable: bool,
    id: impl std::hash::Hash + std::fmt::Debug,
) -> (Rect, TabResponse) {
    let bar = ui.max_rect();
    let font = if active {
        crate::ui::theme::text::body_medium()
    } else {
        crate::ui::theme::text::body()
    };
    let color = if active { p.text } else { p.text_2 };
    let close_w = if closable { 22.0 + 4.0 } else { 0.0 };
    let galley = widgets::truncated(
        ui,
        label,
        font,
        color,
        240.0 - 12.0 - 16.0 - 8.0 - 12.0 - close_w,
    );
    let rect = Rect::from_min_size(
        Pos2::new(left, bar.bottom() - 34.0),
        vec2(12.0 + 16.0 + 8.0 + galley.size().x + 12.0 + close_w, 34.0),
    );
    let response = ui.interact(rect, ui.id().with(&id), Sense::click());
    let fill = if active {
        p.bg_2
    } else if response.hovered() {
        p.bg_1
    } else {
        Color32::TRANSPARENT
    };
    let radius = CornerRadius {
        nw: radius::MD,
        ne: radius::MD,
        sw: 0,
        se: 0,
    };
    ui.painter().rect_filled(rect, radius, fill);
    let icon_rect = Rect::from_min_size(
        Pos2::new(rect.left() + 12.0, rect.center().y - 8.0),
        vec2(16.0, 16.0),
    );
    let icon_color = if icon == Icon::Folder {
        p.accent
    } else {
        color
    };
    icons::paint(ui.painter(), icon_rect, icon, 16.0, icon_color);
    widgets::paint_aligned(
        ui.painter(),
        Rect::from_min_max(
            Pos2::new(icon_rect.right() + 8.0, rect.top()),
            rect.right_bottom(),
        ),
        galley,
        Align2::LEFT_CENTER,
        color,
    );
    let mut out = TabResponse {
        clicked: response.clicked(),
        closed: response.middle_clicked(),
    };
    if closable {
        let close = Rect::from_center_size(
            Pos2::new(rect.right() - 8.0 - 11.0, rect.center().y),
            vec2(22.0, 22.0),
        );
        let r = ui
            .interact(close, ui.id().with((&id, "close")), Sense::click())
            .on_hover_text("Close tab");
        if r.hovered() {
            ui.painter()
                .rect_filled(close, CornerRadius::same(5), p.hover);
        }
        icons::paint(
            ui.painter(),
            close,
            Icon::Close,
            12.0,
            if r.hovered() { p.text } else { p.muted },
        );
        if r.clicked() {
            out.closed = true;
            out.clicked = false;
        }
    }
    (rect, out)
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
