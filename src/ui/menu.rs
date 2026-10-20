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

/// A menu row with no command or shortcut behind it.
pub fn plain_item(ui: &mut egui::Ui, p: &Palette, label: &str) -> bool {
    plain_item_sized(ui, p, label, MENU_WIDTH - 8.0)
}

/// Widest a submenu grows to fit long names, before they're shortened.
pub const MAX_SUBMENU_WIDTH: f32 = 420.0;

/// The row width that fits all of `labels`, within the menu's limits.
pub fn width_for(ui: &egui::Ui, labels: &[&str]) -> f32 {
    let widest = labels
        .iter()
        .map(|l| {
            ui.painter()
                .layout_no_wrap((*l).to_owned(), text::body(), egui::Color32::WHITE)
                .size()
                .x
        })
        .fold(0.0, f32::max);
    (widest + 24.0).clamp(MENU_WIDTH - 8.0, MAX_SUBMENU_WIDTH)
}

/// A [`plain_item`] `width` wide. A label too long for it is cut short
/// with "…", and shown whole on hover.
pub fn plain_item_sized(ui: &mut egui::Ui, p: &Palette, label: &str, width: f32) -> bool {
    let (rect, response) = ui.allocate_exact_size(vec2(width, 30.0), Sense::click());
    if response.hovered() {
        ui.painter().rect_filled(
            rect,
            CornerRadius::same(radius::SM - 1),
            p.accent_alpha(if p.dark { 0.16 } else { 0.10 }),
        );
    }
    let galley =
        crate::ui::widgets::truncated(ui, label, text::body(), p.text, rect.width() - 20.0);
    let cut = galley.elided;
    ui.painter().galley(
        Pos2::new(rect.left() + 10.0, rect.center().y - galley.size().y / 2.0),
        galley,
        p.text,
    );
    let response = if cut {
        response.on_hover_text(label)
    } else {
        response
    };
    if response.clicked() {
        ui.close();
    }
    response.clicked()
}

/// A menu row that opens a submenu (shown with
/// [`egui::containers::menu::SubMenu`] on the returned response).
pub fn submenu_item(ui: &mut egui::Ui, p: &Palette, label: &str) -> egui::Response {
    let (rect, response) = ui.allocate_exact_size(vec2(MENU_WIDTH - 8.0, 30.0), Sense::click());
    let open = egui::containers::menu::SubMenu::id_from_widget_id(response.id);
    let is_open = egui::Popup::is_id_open(ui.ctx(), open);
    if response.hovered() || is_open {
        ui.painter().rect_filled(
            rect,
            CornerRadius::same(radius::SM - 1),
            p.accent_alpha(if p.dark { 0.16 } else { 0.10 }),
        );
    }
    let galley =
        crate::ui::widgets::truncated(ui, label, text::body(), p.text, rect.width() - 36.0);
    ui.painter().galley(
        Pos2::new(rect.left() + 10.0, rect.center().y - galley.size().y / 2.0),
        galley,
        p.text,
    );
    ui.painter().text(
        Pos2::new(rect.right() - 10.0, rect.center().y),
        Align2::RIGHT_CENTER,
        "›",
        text::body(),
        p.text_2,
    );
    response
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
