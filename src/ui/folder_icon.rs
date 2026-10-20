//! "Change icon…" for folders: pick an icon and a color, or go back to the
//! plain folder. Applies to every selected folder.

use std::path::PathBuf;

use egui::{Color32, CornerRadius, Frame, Id, Margin, Modal, Pos2, Rect, Sense, Stroke, vec2};

use crate::app::ExploristApp;
use crate::config::FolderIcon;
use crate::fs::entry::Entry;
use crate::ui::icons::{self, FOLDER_ICONS};
use crate::ui::image_icon;
use crate::ui::theme::{Palette, radius, text};
use crate::ui::widgets::{self, ButtonStyle};

/// Colors to choose from, besides the accent color.
pub const COLORS: [(&str, [u8; 3]); 10] = [
    ("Red", [0xEF, 0x44, 0x44]),
    ("Orange", [0xF9, 0x73, 0x16]),
    ("Yellow", [0xEA, 0xB3, 0x08]),
    ("Green", [0x22, 0xC5, 0x5E]),
    ("Teal", [0x14, 0xB8, 0xA6]),
    ("Blue", [0x3B, 0x82, 0xF6]),
    ("Indigo", [0x63, 0x66, 0xF1]),
    ("Purple", [0xA8, 0x55, 0xF7]),
    ("Pink", [0xEC, 0x48, 0x99]),
    ("Gray", [0x9C, 0xA3, 0xAF]),
];

const COLUMNS: usize = 10;
const CELL: f32 = 40.0;

pub struct FolderIconState {
    folders: Vec<PathBuf>,
    icon: String,
    color: Option<[u8; 3]>,
    /// The Enter that opened it (from the palette) mustn't save.
    fresh: bool,
}

impl FolderIconState {
    /// For the selected folders, starting from the first one's icon.
    pub fn new(app: &ExploristApp, folders: Vec<PathBuf>) -> Self {
        let current = folders.first().and_then(|f| app.settings.folder_icon(f));
        Self {
            icon: current.map_or_else(|| "folder".to_owned(), |c| c.icon.clone()),
            color: current.and_then(|c| c.color),
            folders,
            fresh: true,
        }
    }
}

/// The color to draw a folder's icon in.
pub fn color_of(p: &Palette, color: Option<[u8; 3]>) -> Color32 {
    color.map_or(p.accent, |[r, g, b]| Color32::from_rgb(r, g, b))
}

/// Draws a chosen icon: an image file as it is, or a named icon in its
/// color. `width` is the line width, or `None` for the normal weight.
pub fn paint_chosen(
    painter: &egui::Painter,
    p: &Palette,
    rect: Rect,
    chosen: &FolderIcon,
    size: f32,
    width: Option<f32>,
    dim: bool,
) {
    let mut color = color_of(p, chosen.color);
    if dim {
        color = color.gamma_multiply(0.5);
    }
    let name = match image_icon::file_of(&chosen.icon) {
        Some(file) if image_icon::paint(painter, rect, file, size, dim) => return,
        // The image is gone: a plain folder instead.
        Some(_) => "folder",
        None => &chosen.icon,
    };
    icons::paint_named_stroked(
        painter,
        rect,
        name,
        size,
        width.unwrap_or(2.0 * size / 24.0),
        color,
    );
}

/// Draws `entry`'s icon: the chosen one for folders that have one, else
/// the usual icon for its kind in `default_color`. `width` is the line
/// width for big icons, or `None` for the normal weight.
#[allow(clippy::too_many_arguments)]
pub fn paint_entry_icon(
    painter: &egui::Painter,
    app: &ExploristApp,
    rect: Rect,
    entry: &Entry,
    size: f32,
    width: Option<f32>,
    default_color: Color32,
    dim: bool,
) {
    let chosen = if entry.is_dir {
        app.settings.folder_icon(&entry.path)
    } else {
        None
    };
    match chosen {
        Some(chosen) => paint_chosen(painter, &app.palette, rect, chosen, size, width, dim),
        None => {
            let icon = icons::Icon::for_kind(entry.kind);
            match width {
                Some(w) => icons::paint_stroked(painter, rect, icon, size, w, default_color),
                None => icons::paint(painter, rect, icon, size, default_color),
            }
        }
    }
}

pub fn show(ctx: &egui::Context, app: &mut ExploristApp) {
    let Some(state) = &mut app.view.folder_icon else {
        return;
    };
    let p = app.palette;
    let mut close = false;
    let mut save: Option<Option<FolderIcon>> = None;
    let mut error = None;
    let fresh = std::mem::take(&mut state.fresh);
    let title = match state.folders.as_slice() {
        [one] => format!(
            "Icon for “{}”",
            one.file_name()
                .map(|n| n.to_string_lossy().into_owned())
                .unwrap_or_default()
        ),
        many => format!("Icon for {} folders", many.len()),
    };
    let width = COLUMNS as f32 * (CELL + 4.0) - 4.0;

    let modal = Modal::new(Id::new("folder-icon"))
        .backdrop_color(p.scrim)
        .frame(
            Frame::NONE
                .fill(p.surface)
                .stroke(Stroke::new(1.0, p.border))
                .corner_radius(CornerRadius::same(radius::LG))
                .shadow(p.shadow_modal())
                .inner_margin(Margin::same(22)),
        )
        .show(ctx, |ui| {
            ui.set_width(width);
            ui.spacing_mut().item_spacing = vec2(4.0, 4.0);

            // Title with a preview of the choice.
            let (head, _) = ui.allocate_exact_size(vec2(width, 44.0), Sense::hover());
            let tile = Rect::from_min_size(head.min, vec2(44.0, 44.0));
            ui.painter()
                .rect_filled(tile, CornerRadius::same(radius::MD), p.bg_2);
            let preview = FolderIcon {
                icon: state.icon.clone(),
                color: state.color,
            };
            paint_chosen(ui.painter(), &p, tile, &preview, 28.0, None, false);
            let name = widgets::truncated(ui, &title, text::heading(), p.text, width - 60.0);
            ui.painter().galley(
                Pos2::new(tile.right() + 14.0, head.center().y - name.size().y / 2.0),
                name,
                p.text,
            );
            ui.add_space(10.0);

            // Icons.
            for row in FOLDER_ICONS.chunks(COLUMNS) {
                ui.horizontal(|ui| {
                    for (name, _) in row {
                        let (rect, response) =
                            ui.allocate_exact_size(vec2(CELL, CELL), Sense::click());
                        let chosen = state.icon == *name;
                        let fill = if chosen {
                            p.accent_alpha(0.16)
                        } else if response.hovered() {
                            p.hover
                        } else {
                            Color32::TRANSPARENT
                        };
                        ui.painter().rect(
                            rect,
                            CornerRadius::same(radius::SM),
                            fill,
                            if chosen {
                                Stroke::new(1.0, p.accent)
                            } else {
                                Stroke::NONE
                            },
                            egui::StrokeKind::Inside,
                        );
                        let color = if chosen {
                            color_of(&p, state.color)
                        } else {
                            p.text_2
                        };
                        icons::paint_named(ui.painter(), rect, name, 20.0, color);
                        if response.on_hover_text(name.replace('-', " ")).clicked() {
                            state.icon = (*name).to_owned();
                        }
                    }
                });
            }

            // An image of the user's own.
            ui.add_space(6.0);
            ui.horizontal(|ui| {
                ui.spacing_mut().item_spacing.x = 8.0;
                if widgets::button(ui, &p, "Use an image…", ButtonStyle::Secondary, None).clicked()
                    && let Some(file) = rfd::FileDialog::new()
                        .set_title("Choose an image for the icon")
                        .add_filter("Images", image_icon::EXTENSIONS)
                        .pick_file()
                {
                    match image_icon::import(&file) {
                        Ok(icon) => state.icon = icon,
                        Err(err) => error = Some(err),
                    }
                }
                let note = if image_icon::file_of(&state.icon).is_some() {
                    "Images show as they are, in their own colors."
                } else {
                    "PNG, JPG, ICO, SVG, WebP, BMP or GIF."
                };
                ui.label(egui::RichText::new(note).font(text::label()).color(p.muted));
            });

            // Colors: the accent, then the fixed ones. Images keep their own.
            ui.add_space(10.0);
            ui.scope(|ui| {
                if image_icon::file_of(&state.icon).is_some() {
                    ui.disable();
                    ui.multiply_opacity(0.35);
                }
                ui.label(
                    egui::RichText::new("Color")
                        .font(text::label())
                        .color(p.text_2),
                );
                ui.horizontal(|ui| {
                    ui.spacing_mut().item_spacing.x = 8.0;
                    let options = std::iter::once(("Accent", None))
                        .chain(COLORS.iter().map(|(n, c)| (*n, Some(*c))));
                    for (label, color) in options {
                        let (rect, response) =
                            ui.allocate_exact_size(vec2(26.0, 26.0), Sense::click());
                        let chosen = state.color == color;
                        if chosen {
                            ui.painter().circle_stroke(
                                rect.center(),
                                12.5,
                                Stroke::new(2.0, p.text),
                            );
                        }
                        ui.painter()
                            .circle_filled(rect.center(), 9.0, color_of(&p, color));
                        if response.on_hover_text(label).clicked() {
                            state.color = color;
                        }
                    }
                });
            });

            ui.add_space(14.0);
            ui.horizontal(|ui| {
                ui.spacing_mut().item_spacing.x = 8.0;
                if widgets::button(ui, &p, "Reset to plain folder", ButtonStyle::Ghost, None)
                    .clicked()
                {
                    save = Some(None);
                }
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    let ok = widgets::button(ui, &p, "Save", ButtonStyle::Primary, Some("Enter"));
                    let cancel = widgets::button(ui, &p, "Cancel", ButtonStyle::Ghost, Some("Esc"));
                    if ok.clicked() || (!fresh && ui.input(|i| i.key_pressed(egui::Key::Enter))) {
                        // The plain folder in the accent color is no icon at all.
                        let image = image_icon::file_of(&state.icon).is_some();
                        save = Some((state.icon != "folder" || state.color.is_some()).then(|| {
                            FolderIcon {
                                icon: state.icon.clone(),
                                // Images keep their own colors.
                                color: state.color.filter(|_| !image),
                            }
                        }));
                    }
                    if cancel.clicked() {
                        close = true;
                    }
                });
            });
        });
    if modal.should_close() {
        close = true;
    }
    if let Some(choice) = save {
        let folders = std::mem::take(&mut state.folders);
        for folder in &folders {
            app.settings.set_folder_icon(folder, choice.clone());
        }
        app.save_settings();
        close = true;
    }
    if let Some(err) = error {
        app.toast_error(err);
    }
    if close {
        app.view.folder_icon = None;
        // Drop images picked but not kept, and ones no folder uses now.
        image_icon::prune(&app.settings);
    }
}
