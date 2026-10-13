//! Small building blocks shared by the panels, styled after the design's
//! component sheet.

use egui::{
    Align2, Color32, CornerRadius, FontId, Galley, Painter, Pos2, Rect, Response, Sense, Stroke,
    StrokeKind, Ui, Vec2, vec2,
};
use std::sync::Arc;

use crate::ui::icons::{self, Icon};
use crate::ui::theme::{Palette, radius, size, text};

/// A 32 px square button with an icon and a tooltip.
pub fn icon_button(
    ui: &mut Ui,
    palette: &Palette,
    icon: Icon,
    tooltip: &str,
    enabled: bool,
    pressed: bool,
) -> Response {
    icon_button_sized(
        ui,
        palette,
        icon,
        tooltip,
        enabled,
        pressed,
        size::BUTTON,
        16.0,
    )
}

/// Same as [`icon_button`] at any size, e.g. 24 px for "close" buttons.
#[allow(clippy::too_many_arguments)]
pub fn icon_button_sized(
    ui: &mut Ui,
    palette: &Palette,
    icon: Icon,
    tooltip: &str,
    enabled: bool,
    pressed: bool,
    side: f32,
    icon_size: f32,
) -> Response {
    let sense = if enabled {
        Sense::click()
    } else {
        Sense::hover()
    };
    let (rect, response) = ui.allocate_exact_size(Vec2::splat(side), sense);
    if ui.is_rect_visible(rect) {
        let fill = if pressed {
            palette.pressed
        } else if enabled && response.hovered() {
            palette.hover
        } else {
            Color32::TRANSPARENT
        };
        ui.painter()
            .rect_filled(rect, CornerRadius::same(radius::SM), fill);
        let color = if !enabled {
            palette.disabled
        } else if pressed || response.hovered() {
            palette.text
        } else {
            palette.text_2
        };
        icons::paint(ui.painter(), rect, icon, icon_size, color);
        focus_ring(ui, &response, rect, palette);
    }
    response.on_hover_text(tooltip)
}

/// The 2 px accent ring drawn outside a control with keyboard focus.
pub fn focus_ring(ui: &Ui, response: &Response, rect: Rect, palette: &Palette) {
    if response.has_focus() && ui.memory(|m| m.focused() == Some(response.id)) {
        ui.painter().rect_stroke(
            rect.expand(1.0),
            CornerRadius::same(radius::SM + 1),
            Stroke::new(2.0, palette.accent),
            StrokeKind::Outside,
        );
    }
}

/// Lays out a keyboard hint like "Ctrl L".
pub fn kbd_galley(ui: &Ui, palette: &Palette, label: &str) -> Arc<Galley> {
    ui.painter()
        .layout_no_wrap(label.to_owned(), text::kbd(), palette.text_2)
}

/// Width a keyboard hint takes, padding and border included.
pub fn kbd_width(galley: &Galley) -> f32 {
    galley.size().x + 10.0
}

/// Paints a keyboard hint with its left edge at `left`, vertically centered
/// on `center_y`. Returns the rect it used.
pub fn paint_kbd(
    painter: &Painter,
    palette: &Palette,
    galley: Arc<Galley>,
    left: f32,
    center_y: f32,
    on_accent: bool,
) -> Rect {
    let rect = Rect::from_min_size(
        Pos2::new(left, center_y - 8.0),
        vec2(kbd_width(&galley), 16.0),
    );
    if on_accent {
        painter.rect_filled(
            rect,
            CornerRadius::same(radius::XS),
            palette.on_accent.gamma_multiply(0.16),
        );
    } else {
        painter.rect(
            rect,
            CornerRadius::same(radius::XS),
            palette.bg_2,
            Stroke::new(1.0, palette.kbd_border),
            StrokeKind::Inside,
        );
    }
    let color = if on_accent {
        palette.on_accent
    } else {
        palette.text_2
    };
    painter.galley_with_override_text_color(rect.center() - galley.size() / 2.0, galley, color);
    rect
}

/// A keyboard hint as its own widget.
pub fn kbd(ui: &mut Ui, palette: &Palette, label: &str) -> Response {
    let galley = kbd_galley(ui, palette, label);
    let (rect, response) = ui.allocate_exact_size(vec2(kbd_width(&galley), 16.0), Sense::hover());
    paint_kbd(
        ui.painter(),
        palette,
        galley,
        rect.left(),
        rect.center().y,
        false,
    );
    response
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ButtonStyle {
    Primary,
    Secondary,
    Ghost,
    Destructive,
}

/// How wide [`button`] will be, for centering a row of buttons.
pub fn button_width(
    ui: &Ui,
    palette: &Palette,
    label: &str,
    style: ButtonStyle,
    shortcut: Option<&str>,
) -> f32 {
    let font = match style {
        ButtonStyle::Primary => crate::ui::fonts::semibold(12.5),
        _ => crate::ui::fonts::medium(12.5),
    };
    let label_w = ui
        .painter()
        .layout_no_wrap(label.to_owned(), font, palette.text)
        .size()
        .x;
    let kbd_w = shortcut.map_or(0.0, |s| 8.0 + kbd_width(&kbd_galley(ui, palette, s)) - 2.0);
    24.0 + label_w + kbd_w
}

/// A 32 px text button, optionally with a shortcut hint after the label.
pub fn button(
    ui: &mut Ui,
    palette: &Palette,
    label: &str,
    style: ButtonStyle,
    shortcut: Option<&str>,
) -> Response {
    let font: FontId = match style {
        ButtonStyle::Primary => crate::ui::fonts::semibold(12.5),
        _ => crate::ui::fonts::medium(12.5),
    };
    let (fill, fg, border) = match style {
        ButtonStyle::Primary => (palette.accent, palette.on_accent, None),
        ButtonStyle::Secondary => (palette.surface, palette.text, Some(palette.border)),
        ButtonStyle::Ghost => (Color32::TRANSPARENT, palette.text_2, None),
        ButtonStyle::Destructive => (palette.error, palette.on_accent, None),
    };
    let label_galley = ui.painter().layout_no_wrap(label.to_owned(), font, fg);
    let kbd = shortcut.map(|s| kbd_galley(ui, palette, s));
    let mut width = 24.0 + label_galley.size().x;
    if let Some(k) = &kbd {
        width += 8.0 + kbd_width(k) - 2.0;
    }
    let (rect, response) = ui.allocate_exact_size(vec2(width, size::BUTTON), Sense::click());
    if ui.is_rect_visible(rect) {
        let hovered = response.hovered();
        let fill = match (style, hovered) {
            (ButtonStyle::Ghost, true) => palette.hover,
            (ButtonStyle::Secondary, true) => palette.hover,
            (ButtonStyle::Primary | ButtonStyle::Destructive, true) => fill.gamma_multiply(0.9),
            _ => fill,
        };
        let painter = ui.painter();
        painter.rect_filled(rect, CornerRadius::same(radius::SM), fill);
        if let Some(border) = border {
            painter.rect_stroke(
                rect,
                CornerRadius::same(radius::SM),
                Stroke::new(1.0, border),
                StrokeKind::Inside,
            );
        }
        let text_pos = Pos2::new(
            rect.left() + 12.0,
            rect.center().y - label_galley.size().y / 2.0,
        );
        let label_right = text_pos.x + label_galley.size().x;
        painter.galley(text_pos, label_galley, fg);
        if let Some(k) = kbd {
            let on_accent = matches!(style, ButtonStyle::Primary | ButtonStyle::Destructive);
            paint_kbd(
                painter,
                palette,
                k,
                label_right + 8.0,
                rect.center().y,
                on_accent,
            );
        }
        focus_ring(ui, &response, rect, palette);
    }
    response
}

/// Section label in the sidebar: "Favorites", "Drives".
pub fn section_label(ui: &mut Ui, palette: &Palette, label: &str) {
    let galley = ui.painter().layout_no_wrap(
        label.to_owned(),
        crate::ui::fonts::medium(11.5),
        palette.muted,
    );
    let (rect, _) = ui.allocate_exact_size(vec2(ui.available_width(), 22.0), Sense::hover());
    ui.painter().galley(
        Pos2::new(rect.left() + 10.0, rect.top()),
        galley,
        palette.muted,
    );
}

/// Text truncated with an ellipsis to fit `max_width`.
pub fn truncated(
    ui: &Ui,
    label: &str,
    font: FontId,
    color: Color32,
    max_width: f32,
) -> Arc<Galley> {
    let mut job = egui::text::LayoutJob::simple_singleline(label.to_owned(), font, color);
    job.wrap = egui::text::TextWrapping::truncate_at_width(max_width.max(0.0));
    ui.painter().layout_job(job)
}

/// Paints a galley aligned within `rect`.
pub fn paint_aligned(
    painter: &Painter,
    rect: Rect,
    galley: Arc<Galley>,
    align: Align2,
    color: Color32,
) {
    let pos = align.align_size_within_rect(galley.size(), rect).min;
    painter.galley(pos, galley, color);
}

/// A 36×20 on/off switch. Returns true when flipped.
pub fn switch(ui: &mut Ui, palette: &Palette, on: &mut bool) -> bool {
    let (rect, response) = ui.allocate_exact_size(vec2(36.0, 20.0), Sense::click());
    if response.clicked() {
        *on = !*on;
    }
    let track = if *on {
        palette.accent
    } else {
        palette.kbd_border
    };
    let knob = if *on { palette.on_accent } else { palette.text };
    ui.painter()
        .rect_filled(rect, CornerRadius::same(10), track);
    let x = if *on {
        rect.right() - 10.0
    } else {
        rect.left() + 10.0
    };
    ui.painter()
        .circle_filled(Pos2::new(x, rect.center().y), 8.0, knob);
    focus_ring(ui, &response, rect, palette);
    response.clicked()
}

/// A row of mutually exclusive options in a sunken track, as the design's
/// "Comfortable · 30 px / Compact · 24 px". Returns the index clicked.
pub fn segmented(
    ui: &mut Ui,
    palette: &Palette,
    options: &[&str],
    selected: usize,
) -> Option<usize> {
    let font = crate::ui::fonts::regular(12.5);
    let galleys: Vec<_> = options
        .iter()
        .map(|o| {
            ui.painter()
                .layout_no_wrap((*o).to_owned(), font.clone(), palette.text)
        })
        .collect();
    let widths: Vec<f32> = galleys.iter().map(|g| g.size().x + 24.0).collect();
    let total = widths.iter().sum::<f32>() + 2.0 * (options.len() as f32 - 1.0) + 8.0;
    let (rect, _) = ui.allocate_exact_size(vec2(total, 36.0), Sense::hover());
    ui.painter().rect(
        rect,
        CornerRadius::same(radius::MD),
        palette.bg_1,
        Stroke::new(1.0, palette.border),
        StrokeKind::Inside,
    );
    let mut clicked = None;
    let mut x = rect.left() + 4.0;
    for (i, (galley, w)) in galleys.into_iter().zip(widths).enumerate() {
        let item = Rect::from_min_size(Pos2::new(x, rect.top() + 4.0), vec2(w, 28.0));
        let response = ui.interact(item, ui.id().with(("seg", options[i], i)), Sense::click());
        let on = i == selected;
        let fill = if on {
            palette.border
        } else if response.hovered() {
            palette.hover
        } else {
            Color32::TRANSPARENT
        };
        ui.painter()
            .rect_filled(item, CornerRadius::same(radius::SM), fill);
        let color = if on { palette.text } else { palette.text_2 };
        paint_aligned(ui.painter(), item, galley, Align2::CENTER_CENTER, color);
        if response.clicked() && !on {
            clicked = Some(i);
        }
        x += w + 2.0;
    }
    clicked
}

/// A round 28 px color swatch with a ring when chosen.
pub fn swatch(ui: &mut Ui, palette: &Palette, color: Color32, chosen: bool, label: &str) -> bool {
    let (rect, response) = ui.allocate_exact_size(vec2(28.0, 28.0), Sense::click());
    let c = rect.center();
    ui.painter().circle_filled(c, 14.0, color);
    if chosen {
        ui.painter()
            .circle_stroke(c, 13.0, Stroke::new(2.0, palette.text));
        ui.painter()
            .circle_stroke(c, 10.5, Stroke::new(2.0, palette.bg_2));
    }
    response.on_hover_text(label).clicked()
}

/// A framed one-line text field `width` wide and 32 tall, with the
/// design's accent border and glow while focused.
pub fn text_field(
    ui: &mut Ui,
    palette: &Palette,
    text: &mut String,
    width: f32,
    mono: bool,
    hint: &str,
) -> Response {
    let (rect, _) = ui.allocate_exact_size(vec2(width, 32.0), Sense::hover());
    // Reserve the background's place so it's drawn under the text, then
    // fill it in once we know whether the field has focus.
    let background = ui.painter().add(egui::Shape::Noop);
    let inner = Rect::from_center_size(rect.center(), vec2(rect.width() - 20.0, 18.0));
    let mut child = ui.new_child(egui::UiBuilder::new().max_rect(inner));
    let font = if mono {
        crate::ui::fonts::mono(13.0)
    } else {
        text::body()
    };
    let response = child.add(
        egui::TextEdit::singleline(text)
            .hint_text(hint)
            .font(font)
            .text_color(palette.text)
            .frame(egui::Frame::NONE)
            .margin(egui::Margin::ZERO)
            .desired_width(inner.width()),
    );
    let focused = response.has_focus();
    ui.painter().set(
        background,
        egui::Shape::Rect(egui::epaint::RectShape::new(
            rect,
            CornerRadius::same(radius::SM),
            palette.bg_2,
            Stroke::new(
                1.0,
                if focused {
                    palette.accent
                } else {
                    palette.border
                },
            ),
            StrokeKind::Inside,
        )),
    );
    if focused {
        ui.painter().rect_stroke(
            rect,
            CornerRadius::same(radius::SM),
            Stroke::new(
                3.0,
                palette.accent_alpha(if palette.dark { 0.22 } else { 0.18 }),
            ),
            StrokeKind::Outside,
        );
    }
    response
}

/// A small label above a field.
pub fn field_label(ui: &mut Ui, palette: &Palette, label: &str) {
    ui.label(
        egui::RichText::new(label)
            .font(crate::ui::fonts::medium(12.0))
            .color(palette.text_2),
    );
}
