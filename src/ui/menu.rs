//! Context menu items. Menus list the shortcut for every item, so
//! right-click teaches the keyboard.

use egui::{Align2, CornerRadius, Pos2, Sense, Stroke, vec2};

use crate::app::commands::Command;
use crate::app::keymap;
use crate::ui::theme::{Palette, radius, text};

pub const MENU_WIDTH: f32 = 248.0;

/// One menu row. Returns true when chosen.
pub fn item(ui: &mut egui::Ui, p: &Palette, command: Command, enabled: bool) -> bool {
    item_labeled(ui, p, command.label(), command, enabled)
}

pub fn item_labeled(
    ui: &mut egui::Ui,
    p: &Palette,
    label: &str,
    command: Command,
    enabled: bool,
) -> bool {
    let danger = matches!(command, Command::Trash | Command::DeletePermanently);
    let sense = if enabled {
        Sense::click()
    } else {
        Sense::hover()
    };
    let (rect, response) = ui.allocate_exact_size(vec2(MENU_WIDTH - 8.0, 30.0), sense);
    if enabled && response.hovered() {
        ui.painter().rect_filled(
            rect,
            CornerRadius::same(radius::SM - 1),
            p.accent_alpha(if p.dark { 0.16 } else { 0.10 }),
        );
    }
    let color = if !enabled {
        p.disabled
    } else if danger {
        p.error
    } else {
        p.text
    };
    ui.painter().text(
        Pos2::new(rect.left() + 10.0, rect.center().y),
        Align2::LEFT_CENTER,
        label,
        text::body(),
        color,
    );
    if let Some(keys) = keymap::shortcut_label(command) {
        ui.painter().text(
            Pos2::new(rect.right() - 10.0, rect.center().y),
            Align2::RIGHT_CENTER,
            keys,
            text::kbd(),
            if enabled { p.text_2 } else { p.disabled },
        );
    }
    let chosen = enabled && response.clicked();
    if chosen {
        ui.close();
    }
    chosen
}

pub fn separator(ui: &mut egui::Ui, p: &Palette) {
    let (rect, _) = ui.allocate_exact_size(vec2(MENU_WIDTH - 8.0, 9.0), Sense::hover());
    ui.painter().hline(
        (rect.left() + 6.0)..=(rect.right() - 6.0),
        rect.center().y,
        Stroke::new(1.0, p.border_subtle),
    );
}

/// Applies the menu look to the popup's Ui.
pub fn style(ui: &mut egui::Ui) {
    ui.spacing_mut().item_spacing = vec2(0.0, 0.0);
    ui.set_min_width(MENU_WIDTH - 8.0);
}
