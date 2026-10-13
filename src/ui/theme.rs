//! The design's tokens: colors, sizes, radii, shadows and the type ramp.
//!
//! All sizes are logical pixels at 100% scale; egui multiplies them by
//! `pixels_per_point` on high-DPI displays.

use egui::epaint::Shadow;
use egui::{Color32, CornerRadius, FontId, Stroke, Visuals};

use crate::config::{Accent, Density};
use crate::ui::fonts;

/// Colors for one theme.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Palette {
    pub dark: bool,
    /// Tab bar, status bar, code wells.
    pub bg_0: Color32,
    /// Sidebar, preview panel.
    pub bg_1: Color32,
    /// File list and toolbar; the reference background.
    pub bg_2: Color32,
    /// Inputs, path bar, cards, dialogs.
    pub surface: Color32,
    /// Menus and toasts.
    pub raised: Color32,
    pub hover: Color32,
    pub pressed: Color32,
    pub border: Color32,
    pub border_subtle: Color32,
    pub text: Color32,
    pub text_2: Color32,
    pub muted: Color32,
    pub faint: Color32,
    pub disabled: Color32,
    pub accent: Color32,
    pub on_accent: Color32,
    pub success: Color32,
    pub warning: Color32,
    pub error: Color32,
    pub scroll_track: Color32,
    pub scroll_thumb: Color32,
    pub skeleton: Color32,
    pub kbd_border: Color32,
    /// Dims everything behind a dialog.
    pub scrim: Color32,
}

const fn hex(rgb: u32) -> Color32 {
    Color32::from_rgb((rgb >> 16) as u8, (rgb >> 8) as u8, rgb as u8)
}

impl Palette {
    pub fn dark(accent: Accent) -> Self {
        Self {
            dark: true,
            bg_0: hex(0x0E1013),
            bg_1: hex(0x13161A),
            bg_2: hex(0x181B20),
            surface: hex(0x1F2329),
            raised: hex(0x252A31),
            hover: hex(0x1E2228),
            pressed: hex(0x23282F),
            border: hex(0x2B3038),
            border_subtle: hex(0x22262D),
            text: hex(0xE7E9EC),
            text_2: hex(0xA9B0BA),
            muted: hex(0x868E99),
            faint: hex(0x4F5660),
            disabled: hex(0x4A505A),
            accent: accent_color(accent, true),
            on_accent: hex(0x0E1013),
            success: hex(0x8FCB7E),
            warning: hex(0xE2B35E),
            error: hex(0xE57A72),
            scroll_track: hex(0x262B32),
            scroll_thumb: hex(0x3A414B),
            skeleton: hex(0x22262D),
            kbd_border: hex(0x353B44),
            scrim: Color32::from_rgba_unmultiplied(6, 7, 9, 140),
        }
    }

    pub fn light(accent: Accent) -> Self {
        Self {
            dark: false,
            bg_0: hex(0xE9EBEE),
            bg_1: hex(0xF3F4F6),
            bg_2: hex(0xFBFBFC),
            surface: hex(0xFFFFFF),
            raised: hex(0xFFFFFF),
            hover: hex(0xEDEFF2),
            pressed: hex(0xE3E6EA),
            border: hex(0xD5D9DE),
            border_subtle: hex(0xE4E7EB),
            text: hex(0x15181C),
            text_2: hex(0x4A515B),
            muted: hex(0x5F6670),
            faint: hex(0xA0A6AE),
            disabled: hex(0xA9AFB7),
            accent: accent_color(accent, false),
            on_accent: hex(0xFFFFFF),
            success: hex(0x2A7434),
            warning: hex(0x8A5A00),
            error: hex(0xBC352B),
            scroll_track: hex(0xE1E4E8),
            scroll_thumb: hex(0xB9BFC6),
            skeleton: hex(0xECEEF1),
            kbd_border: hex(0xD0D5DB),
            scrim: Color32::from_rgba_unmultiplied(20, 24, 30, 71),
        }
    }

    /// Fill for selected rows: the accent at 16% (dark) or 11% (light).
    pub fn selection(&self) -> Color32 {
        self.accent_alpha(if self.dark { 0.16 } else { 0.11 })
    }

    /// The 1 px ring on the focused row: accent at 75% / 65%.
    pub fn focus_ring(&self) -> Color32 {
        self.accent_alpha(if self.dark { 0.75 } else { 0.65 })
    }

    /// The accent with the given opacity, for tints and focus glows.
    pub fn accent_alpha(&self, alpha: f32) -> Color32 {
        self.accent.gamma_multiply(alpha)
    }

    /// Menus, tooltips, toast (e1).
    pub fn shadow_float(&self) -> Shadow {
        Shadow {
            offset: [0, 16],
            blur: 40,
            spread: 0,
            color: Color32::from_black_alpha(if self.dark { 115 } else { 36 }),
        }
    }

    /// Command palette and dialogs, over a scrim (e2).
    pub fn shadow_modal(&self) -> Shadow {
        Shadow {
            offset: [0, 24],
            blur: 64,
            spread: 0,
            color: Color32::from_black_alpha(if self.dark { 140 } else { 46 }),
        }
    }
}

/// The accent's shade for each theme. Light shades are darker so text in
/// the accent color keeps 4.5:1 contrast.
pub fn accent_color(accent: Accent, dark: bool) -> Color32 {
    match (accent, dark) {
        (Accent::Teal, true) => hex(0x6CC5B0),
        (Accent::Teal, false) => hex(0x0D7366),
        (Accent::Blue, true) => hex(0x8EA7F0),
        (Accent::Blue, false) => hex(0x3552C2),
        (Accent::Orange, true) => hex(0xE2A66B),
        (Accent::Orange, false) => hex(0x9A520E),
        (Accent::Purple, true) => hex(0xC59AE6),
        (Accent::Purple, false) => hex(0x7442A3),
    }
}

/// Fixed sizes from the design, in logical pixels.
pub mod size {
    pub const TAB_BAR: f32 = 40.0;
    pub const TOOLBAR: f32 = 52.0;
    pub const PATH_BAR: f32 = 34.0;
    pub const COLUMN_HEADER: f32 = 34.0;
    pub const STATUS_BAR: f32 = 28.0;
    pub const SIDEBAR: f32 = 224.0;
    pub const PREVIEW: f32 = 320.0;
    pub const ICON: f32 = 16.0;
    pub const ICON_STROKE: f32 = 1.5;
    pub const BUTTON: f32 = 32.0;
    pub const BUTTON_SMALL: f32 = 28.0;
    pub const WINDOW_BUTTON_WIDTH: f32 = 46.0;
    /// File list columns after Name: Size, Modified, Type.
    pub const COL_SIZE: f32 = 96.0;
    pub const COL_MODIFIED: f32 = 148.0;
    pub const COL_TYPE: f32 = 112.0;
    pub const COL_GAP: f32 = 16.0;
}

pub mod radius {
    /// Keyboard hints.
    pub const XS: u8 = 4;
    /// Rows and buttons.
    pub const SM: u8 = 6;
    /// Menus and inputs.
    pub const MD: u8 = 8;
    /// Dialogs.
    pub const LG: u8 = 12;
}

/// Row height for the chosen density.
pub fn row_height(density: Density) -> f32 {
    match density {
        Density::Comfortable => 30.0,
        Density::Compact => 24.0,
    }
}

/// The type ramp. Line heights are handled by layout, so only size and
/// weight live here.
pub mod text {
    use super::*;

    /// 18 / 600: dialog titles.
    pub fn title() -> FontId {
        fonts::semibold(18.0)
    }
    /// 15 / 600: section headings.
    pub fn heading() -> FontId {
        fonts::semibold(15.0)
    }
    /// 13 / 400: file names and prose.
    pub fn body() -> FontId {
        fonts::regular(13.0)
    }
    /// 13 / 500: folder names, active items.
    pub fn body_medium() -> FontId {
        fonts::medium(13.0)
    }
    /// 12 / 500: column headers, labels, buttons.
    pub fn label() -> FontId {
        fonts::medium(12.0)
    }
    /// 11.5 / 400: section labels, hints, status text.
    pub fn caption() -> FontId {
        fonts::regular(11.5)
    }
    /// 12.5 mono: paths, sizes, dates.
    pub fn mono() -> FontId {
        fonts::mono(12.5)
    }
    /// 11 mono: keyboard shortcuts.
    pub fn kbd() -> FontId {
        fonts::mono(11.0)
    }
}

/// Applies the palette to egui's built-in widgets, so anything not drawn
/// by hand (text fields, scroll bars, menus) matches the design.
pub fn apply(ctx: &egui::Context, palette: &Palette) {
    let mut visuals = if palette.dark {
        Visuals::dark()
    } else {
        Visuals::light()
    };

    visuals.override_text_color = Some(palette.text);
    visuals.weak_text_color = Some(palette.muted);
    visuals.hyperlink_color = palette.accent;
    visuals.panel_fill = palette.bg_2;
    visuals.faint_bg_color = palette.bg_1;
    visuals.extreme_bg_color = palette.surface;
    visuals.text_edit_bg_color = Some(palette.surface);
    visuals.code_bg_color = palette.bg_0;
    visuals.warn_fg_color = palette.warning;
    visuals.error_fg_color = palette.error;

    visuals.window_fill = palette.raised;
    visuals.window_stroke = Stroke::new(1.0, palette.border);
    visuals.window_corner_radius = CornerRadius::same(radius::LG);
    visuals.window_shadow = palette.shadow_modal();
    visuals.popup_shadow = palette.shadow_float();
    visuals.menu_corner_radius = CornerRadius::same(radius::MD);

    visuals.selection.bg_fill = palette.accent_alpha(0.35);
    visuals.selection.stroke = Stroke::new(1.0, palette.accent);
    visuals.text_cursor.stroke = Stroke::new(1.5, palette.accent);

    let w = &mut visuals.widgets;
    w.noninteractive.bg_fill = palette.bg_2;
    w.noninteractive.weak_bg_fill = palette.bg_2;
    w.noninteractive.bg_stroke = Stroke::new(1.0, palette.border_subtle);
    w.noninteractive.fg_stroke = Stroke::new(1.0, palette.text_2);
    for (state, fill, fg) in [
        (&mut w.inactive, Color32::TRANSPARENT, palette.text_2),
        (&mut w.hovered, palette.hover, palette.text),
        (&mut w.active, palette.pressed, palette.text),
        (&mut w.open, palette.pressed, palette.text),
    ] {
        state.bg_fill = fill;
        state.weak_bg_fill = fill;
        state.bg_stroke = Stroke::NONE;
        state.fg_stroke = Stroke::new(1.0, fg);
        state.corner_radius = CornerRadius::same(radius::SM);
        state.expansion = 0.0;
    }
    // Text fields draw their border from the inactive/hovered strokes.
    w.inactive.bg_stroke = Stroke::new(1.0, palette.border);
    w.hovered.bg_stroke = Stroke::new(1.0, palette.border);

    ctx.set_visuals(visuals);
    ctx.global_style_mut(|style| {
        style.spacing.item_spacing = egui::vec2(8.0, 4.0);
        style.spacing.button_padding = egui::vec2(12.0, 6.0);
        style.spacing.interact_size.y = size::BUTTON;
        style
            .text_styles
            .insert(egui::TextStyle::Body, text::body());
        style
            .text_styles
            .insert(egui::TextStyle::Button, fonts::medium(12.5));
        style
            .text_styles
            .insert(egui::TextStyle::Small, text::caption());
        style
            .text_styles
            .insert(egui::TextStyle::Monospace, text::mono());
        style
            .text_styles
            .insert(egui::TextStyle::Heading, text::heading());
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    /// WCAG relative luminance.
    fn luminance(c: Color32) -> f64 {
        let lin = |v: u8| {
            let v = f64::from(v) / 255.0;
            if v <= 0.040_45 {
                v / 12.92
            } else {
                ((v + 0.055) / 1.055).powf(2.4)
            }
        };
        0.2126 * lin(c.r()) + 0.7152 * lin(c.g()) + 0.0722 * lin(c.b())
    }

    fn contrast(a: Color32, b: Color32) -> f64 {
        let (a, b) = (luminance(a), luminance(b));
        (a.max(b) + 0.05) / (a.min(b) + 0.05)
    }

    fn palettes() -> Vec<Palette> {
        [Accent::Teal, Accent::Blue, Accent::Orange, Accent::Purple]
            .into_iter()
            .flat_map(|a| [Palette::dark(a), Palette::light(a)])
            .collect()
    }

    #[test]
    fn text_passes_aa_on_every_background() {
        for p in palettes() {
            for bg in [p.bg_0, p.bg_1, p.bg_2, p.surface] {
                for (name, fg) in [
                    ("text", p.text),
                    ("text_2", p.text_2),
                    ("muted", p.muted),
                    ("accent", p.accent),
                ] {
                    let ratio = contrast(fg, bg);
                    assert!(
                        ratio >= 4.5,
                        "{name} {fg:?} on {bg:?}: {ratio:.2} (dark: {})",
                        p.dark
                    );
                }
            }
        }
    }

    #[test]
    fn primary_buttons_are_readable() {
        for p in palettes() {
            let ratio = contrast(p.on_accent, p.accent);
            assert!(ratio >= 4.5, "on_accent: {ratio:.2} (dark: {})", p.dark);
        }
    }

    #[test]
    fn hex_unpacks_channels() {
        assert_eq!(hex(0x6CC5B0), Color32::from_rgb(0x6C, 0xC5, 0xB0));
    }

    #[test]
    fn row_heights_match_the_design() {
        assert_eq!(row_height(Density::Comfortable), 30.0);
        assert_eq!(row_height(Density::Compact), 24.0);
    }
}
