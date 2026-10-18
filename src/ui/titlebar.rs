//! The 40 px top bar: logo, the (single) tab, and window buttons. It ships
//! with one tab so adding tabs later changes no layout.

use egui::{Align2, Color32, CornerRadius, Frame, Pos2, Rect, Sense, ViewportCommand, vec2};

use crate::app::ExploristApp;
use crate::ui::icons::{self, Icon};
use crate::ui::theme::{radius, size};
use crate::ui::widgets;

/// macOS draws its own window buttons; other systems use ours.
const NATIVE_BUTTONS: bool = cfg!(target_os = "macos");

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
            // Leave room for macOS's traffic lights.
            let inset = if NATIVE_BUTTONS { 76.0 } else { 0.0 };
            let logo = Rect::from_center_size(
                Pos2::new(bar.left() + inset + 12.0 + 9.0, bar.center().y),
                vec2(18.0, 18.0),
            );
            paint_logo(ui.painter(), logo);

            // Tabs, then Settings while it's open, then "+".
            let mut x = logo.right() + 14.0;
            let settings_open = app.view.settings_open;
            let count = app.tab_count();
            let room = bar.right()
                - 3.0 * size::WINDOW_BUTTON_WIDTH
                - 40.0
                - x
                - if settings_open { 160.0 } else { 0.0 };
            let max_width = (room / count as f32 - 2.0).clamp(80.0, 240.0);
            let mut action = None;
            for i in 0..count {
                let location = app.tab_location(i);
                let start = app.tab_is_start(i);
                let name = if start {
                    "New tab".to_owned()
                } else {
                    location
                        .file_name()
                        .map(|n| n.to_string_lossy().into_owned())
                        .unwrap_or_else(|| location.display().to_string())
                };
                let active = i == app.active_tab && !settings_open;
                let style = TabStyle {
                    active,
                    closable: count > 1 && (active || i == app.active_tab),
                    busy: app.tab_busy(i),
                    max_width,
                };
                let (rect, response) = tab(
                    ui,
                    &p,
                    x,
                    &name,
                    if start { Icon::Home } else { Icon::Folder },
                    style,
                    ("folder-tab", i),
                );
                x = rect.right() + 2.0;
                if response.closed {
                    action = Some((i, true));
                } else if response.clicked {
                    action = Some((i, false));
                }
            }
            if settings_open {
                let style = TabStyle {
                    active: true,
                    closable: true,
                    busy: false,
                    max_width: 160.0,
                };
                let (rect, settings) =
                    tab(ui, &p, x, "Settings", Icon::Settings, style, "settings-tab");
                x = rect.right() + 2.0;
                if settings.closed {
                    app.view.settings_open = false;
                }
            }
            let plus = Rect::from_center_size(
                Pos2::new(x + 4.0 + 14.0, bar.bottom() - 3.0 - 14.0),
                vec2(28.0, 28.0),
            );
            let plus_response = ui
                .interact(plus, ui.id().with("new-tab"), Sense::click())
                .on_hover_text("New tab (Ctrl T)");
            if plus_response.hovered() {
                ui.painter()
                    .rect_filled(plus, CornerRadius::same(radius::SM), p.hover);
            }
            icons::paint(
                ui.painter(),
                plus,
                Icon::Plus,
                14.0,
                if plus_response.hovered() {
                    p.text
                } else {
                    p.muted
                },
            );
            if plus_response.clicked() {
                app.new_start_tab();
            }
            match action {
                Some((i, true)) => app.close_tab(i),
                Some((i, false)) => {
                    app.view.settings_open = false;
                    app.switch_tab(i);
                }
                None => {}
            }

            // Window buttons (macOS has its own).
            if NATIVE_BUTTONS {
                return;
            }
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

/// How to draw a tab.
#[derive(Clone, Copy)]
pub struct TabStyle {
    pub active: bool,
    pub closable: bool,
    /// Still loading: shows an accent dot.
    pub busy: bool,
    pub max_width: f32,
}

/// Draws a 34 px tab at `left`. The active tab shares the toolbar color
/// and "opens" into it; no underline, no accent fill. Middle-click closes.
pub fn tab(
    ui: &mut egui::Ui,
    p: &crate::ui::theme::Palette,
    left: f32,
    label: &str,
    icon: Icon,
    style: TabStyle,
    id: impl std::hash::Hash + std::fmt::Debug,
) -> (Rect, TabResponse) {
    let TabStyle {
        active,
        closable,
        busy,
        max_width,
    } = style;
    let bar = ui.max_rect();
    let font = if active {
        crate::ui::theme::text::body_medium()
    } else {
        crate::ui::theme::text::body()
    };
    let color = if active { p.text } else { p.text_2 };
    let close_w = if closable { 22.0 + 4.0 } else { 0.0 };
    let dot_w = if busy { 12.0 } else { 0.0 };
    let galley = widgets::truncated(
        ui,
        label,
        font,
        color,
        max_width - 12.0 - 16.0 - 8.0 - 12.0 - close_w - dot_w,
    );
    let galley_width = galley.size().x;
    let rect = Rect::from_min_size(
        Pos2::new(left, bar.bottom() - 34.0),
        vec2(
            12.0 + 16.0 + 8.0 + galley.size().x + 12.0 + close_w + dot_w,
            34.0,
        ),
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
    if busy {
        let dot = Pos2::new(
            icon_rect.right() + 8.0 + galley_width + 9.0,
            rect.center().y,
        );
        ui.painter().circle_filled(dot, 3.0, p.accent);
    }
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

/// The app icon, small.
fn paint_logo(painter: &egui::Painter, rect: Rect) {
    static TEXTURE: std::sync::OnceLock<Option<egui::TextureHandle>> = std::sync::OnceLock::new();
    let texture = TEXTURE.get_or_init(|| {
        let png = include_bytes!("../../assets/icon/icon-256.png");
        let image = image::load_from_memory(png).ok()?.into_rgba8();
        let size = [image.width() as usize, image.height() as usize];
        Some(painter.ctx().load_texture(
            "app-icon",
            egui::ColorImage::from_rgba_unmultiplied(size, image.as_raw()),
            egui::TextureOptions::LINEAR,
        ))
    });
    if let Some(texture) = texture {
        painter.image(
            texture.id(),
            rect,
            Rect::from_min_max(Pos2::ZERO, Pos2::new(1.0, 1.0)),
            Color32::WHITE,
        );
    }
}
