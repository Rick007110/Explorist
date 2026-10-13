//! The 320 px preview panel on the right (Ctrl .).

use egui::text::LayoutJob;
use egui::{
    Align2, Color32, CornerRadius, Frame, Margin, Pos2, Rect, ScrollArea, Sense, Stroke,
    StrokeKind, TextFormat, TextureOptions, vec2,
};

use crate::app::ExploristApp;
use crate::app::commands::Command;
use crate::fs::entry::Entry;
use crate::fs::format;
use crate::fs::preview::Content;
use crate::ui::icons::{self, Icon};
use crate::ui::theme::{Palette, radius, size, text};
use crate::ui::widgets::{self, ButtonStyle};

/// The texture for the image being previewed, kept between frames.
pub struct PreviewImage {
    pub path: std::path::PathBuf,
    pub texture: egui::TextureHandle,
}

pub fn show(ui: &mut egui::Ui, app: &mut ExploristApp) {
    let p = app.palette;
    let mut commands = Vec::new();
    egui::Panel::right("preview")
        .exact_size(size::PREVIEW)
        .resizable(false)
        .show_separator_line(false)
        .frame(Frame::NONE.fill(p.bg_1))
        .show(ui, |ui| {
            let full = ui.max_rect();
            ui.painter().vline(
                full.left() + 0.5,
                full.y_range(),
                Stroke::new(1.0, p.border_subtle),
            );
            header(ui, app, &p);

            ScrollArea::vertical()
                .id_salt("preview-scroll")
                .auto_shrink([false, false])
                .show(ui, |ui| {
                    Frame::NONE.inner_margin(Margin::same(16)).show(ui, |ui| {
                        ui.set_width(size::PREVIEW - 32.0);
                        ui.spacing_mut().item_spacing = vec2(0.0, 16.0);
                        body(ui, app, &p, &mut commands);
                    });
                });
        });
    let ctx = ui.ctx().clone();
    for command in commands {
        app.run(&ctx, command);
    }
}

fn header(ui: &mut egui::Ui, app: &mut ExploristApp, p: &Palette) {
    let (rect, _) = ui.allocate_exact_size(
        vec2(ui.available_width(), size::PREVIEW_HEADER),
        Sense::hover(),
    );
    ui.painter().hline(
        rect.x_range(),
        rect.bottom() - 0.5,
        Stroke::new(1.0, p.border_subtle),
    );
    ui.painter().text(
        Pos2::new(rect.left() + 16.0, rect.center().y),
        Align2::LEFT_CENTER,
        "Preview",
        text::label(),
        p.muted,
    );
    let close = Rect::from_center_size(
        Pos2::new(rect.right() - 8.0 - 12.0, rect.center().y),
        vec2(24.0, 24.0),
    );
    let response = ui
        .interact(close, ui.id().with("collapse-preview"), Sense::click())
        .on_hover_text("Hide preview (Ctrl .)");
    if response.hovered() {
        ui.painter()
            .rect_filled(close, CornerRadius::same(5), p.hover);
    }
    icons::paint(ui.painter(), close, Icon::Close, 12.0, p.muted);
    if response.clicked() {
        app.settings.show_preview = false;
        app.save_settings();
    }
}

fn body(ui: &mut egui::Ui, app: &mut ExploristApp, p: &Palette, commands: &mut Vec<Command>) {
    let selected: Vec<Entry> = app
        .browser
        .selected_entries()
        .into_iter()
        .cloned()
        .collect();
    match selected.as_slice() {
        [] => {
            app.preview.clear();
            let name = app
                .browser
                .location()
                .file_name()
                .map(|n| n.to_string_lossy().into_owned())
                .unwrap_or_else(|| app.browser.location().display().to_string());
            let count = match app.browser.rows().len() {
                1 => "1 item".to_owned(),
                n => format!("{} items", format::count(n as u64)),
            };
            title_block(ui, p, Icon::Folder, p.accent, &name, &count);
            ui.label(
                egui::RichText::new("Select an item to preview it.")
                    .font(text::body())
                    .color(p.text_2),
            );
        }
        [entry] => single(ui, app, p, entry, commands),
        many => {
            app.preview.clear();
            let bytes: u64 = many.iter().filter_map(|e| e.size).sum();
            let folders = many.iter().filter(|e| e.is_dir).count();
            let mut detail = format::size(bytes);
            if folders > 0 {
                detail.push_str(&format!(
                    " in files · {folders} folder{}",
                    if folders == 1 { "" } else { "s" }
                ));
            }
            title_block(
                ui,
                p,
                Icon::File,
                p.text_2,
                &format!("{} items selected", format::count(many.len() as u64)),
                &detail,
            );
            for entry in many.iter().take(12) {
                ui.label(
                    egui::RichText::new(&entry.name)
                        .font(text::mono())
                        .color(p.text_2),
                );
            }
            if many.len() > 12 {
                ui.label(
                    egui::RichText::new(format!("and {} more", many.len() - 12))
                        .font(text::caption())
                        .color(p.muted),
                );
            }
        }
    }
}

fn title_block(
    ui: &mut egui::Ui,
    p: &Palette,
    icon: Icon,
    icon_color: Color32,
    title: &str,
    subtitle: &str,
) {
    ui.horizontal_top(|ui| {
        ui.spacing_mut().item_spacing.x = 12.0;
        let (tile, _) = ui.allocate_exact_size(vec2(36.0, 36.0), Sense::hover());
        ui.painter().rect_stroke(
            tile,
            CornerRadius::same(radius::MD),
            Stroke::new(1.0, p.border),
            StrokeKind::Inside,
        );
        icons::paint(ui.painter(), tile, icon, 16.0, icon_color);
        ui.vertical(|ui| {
            ui.spacing_mut().item_spacing.y = 0.0;
            ui.add(
                egui::Label::new(
                    egui::RichText::new(title)
                        .font(text::heading())
                        .color(p.text),
                )
                .wrap(),
            );
            ui.label(
                egui::RichText::new(subtitle)
                    .font(crate::ui::fonts::regular(12.0))
                    .color(p.muted),
            );
        });
    });
}

fn single(
    ui: &mut egui::Ui,
    app: &mut ExploristApp,
    p: &Palette,
    entry: &Entry,
    commands: &mut Vec<Command>,
) {
    let ctx = ui.ctx().clone();
    app.preview
        .request(&entry.path, entry.kind, entry.is_dir, move || {
            ctx.request_repaint()
        });

    let subtitle = match entry.size {
        Some(bytes) => format!("{} · {}", entry.type_label, format::size(bytes)),
        None => entry.type_label.to_string(),
    };
    let icon_color = if entry.is_dir { p.accent } else { p.text_2 };
    title_block(
        ui,
        p,
        Icon::for_kind(entry.kind),
        icon_color,
        &entry.name,
        &subtitle,
    );

    let content = app.preview.poll().cloned();
    let folder_items = match &content {
        None if entry.is_dir || entry.kind != crate::fs::kind::FileKind::Image => {
            ui.label(
                egui::RichText::new("Loading…")
                    .font(text::caption())
                    .color(p.muted),
            );
            None
        }
        None => None,
        Some(Content::Text { lines, truncated }) => {
            code_block(ui, p, lines, *truncated);
            None
        }
        Some(Content::Image {
            width,
            height,
            pixels,
            scaled,
        }) => {
            image_block(ui, app, p, &entry.path, *width, *height, pixels, *scaled);
            None
        }
        Some(Content::Folder { items }) => Some(*items),
        Some(Content::None) => None,
        Some(Content::Error(err)) => {
            ui.label(
                egui::RichText::new(format!("Couldn’t preview: {err}"))
                    .font(text::caption())
                    .color(p.error),
            );
            None
        }
    };

    // Details.
    let location = entry
        .path
        .parent()
        .map(|d| d.display().to_string())
        .unwrap_or_default();
    let date_format = app.settings.date_format;
    let mut details = vec![("Location", location, true)];
    match entry.size {
        Some(bytes) => details.push(("Size", format::exact_size(bytes), true)),
        None => {
            if let Some(items) = folder_items {
                let label = if items == 1 {
                    "1 item".to_owned()
                } else {
                    format!("{} items", format::count(items as u64))
                };
                details.push(("Contains", label, false));
            }
        }
    }
    if let Some(t) = entry.modified {
        details.push(("Modified", format::date(t, date_format), true));
    }
    if let Some(t) = entry.created {
        details.push(("Created", format::date(t, date_format), true));
    }
    if entry.hidden || entry.system {
        let what = if entry.system {
            "System file"
        } else {
            "Hidden"
        };
        details.push(("Attributes", what.to_owned(), false));
    }
    ui.vertical(|ui| {
        ui.spacing_mut().item_spacing.y = 6.0;
        for (label, value, mono) in details {
            ui.horizontal_top(|ui| {
                ui.spacing_mut().item_spacing.x = 8.0;
                let (r, _) = ui.allocate_exact_size(vec2(80.0, 18.0), Sense::hover());
                ui.painter().text(
                    r.left_center(),
                    Align2::LEFT_CENTER,
                    label,
                    crate::ui::fonts::regular(12.0),
                    p.muted,
                );
                let font = if mono {
                    crate::ui::fonts::mono(12.0)
                } else {
                    crate::ui::fonts::regular(12.0)
                };
                ui.add(
                    egui::Label::new(egui::RichText::new(value).font(font).color(p.text_2)).wrap(),
                );
            });
        }
    });

    ui.horizontal(|ui| {
        ui.spacing_mut().item_spacing.x = 8.0;
        if widgets::button(ui, p, "Open", ButtonStyle::Primary, None).clicked() {
            commands.push(Command::Open);
        }
        if widgets::button(
            ui,
            p,
            "Copy path",
            ButtonStyle::Secondary,
            Some("Ctrl Shift C"),
        )
        .clicked()
        {
            commands.push(Command::CopyPath);
        }
    });
}

fn code_block(ui: &mut egui::Ui, p: &Palette, lines: &[String], truncated: bool) {
    let font = crate::ui::fonts::mono(12.0);
    let number_width = format!("{}", lines.len()).len() as f32 * 7.3 + 8.0;
    Frame::NONE
        .fill(p.bg_0)
        .corner_radius(CornerRadius::same(radius::MD))
        .inner_margin(Margin::symmetric(0, 12))
        .show(ui, |ui| {
            ui.set_width(ui.available_width());
            ui.spacing_mut().item_spacing.y = 0.0;
            let wrap_width = ui.available_width() - number_width - 24.0;
            for (i, line) in lines.iter().enumerate().take(200) {
                let galley = ui.painter().layout_job({
                    let mut job = highlight(line, &font, p);
                    job.wrap.max_width = wrap_width;
                    job.wrap.max_rows = 1;
                    job
                });
                let (rect, _) =
                    ui.allocate_exact_size(vec2(ui.available_width(), 18.0), Sense::hover());
                ui.painter().text(
                    Pos2::new(rect.left() + 12.0 + number_width - 8.0, rect.center().y),
                    Align2::RIGHT_CENTER,
                    (i + 1).to_string(),
                    font.clone(),
                    p.faint,
                );
                ui.painter().galley(
                    Pos2::new(
                        rect.left() + 12.0 + number_width,
                        rect.center().y - galley.size().y / 2.0,
                    ),
                    galley,
                    p.code_text,
                );
            }
            if truncated || lines.len() > 200 {
                let (rect, _) =
                    ui.allocate_exact_size(vec2(ui.available_width(), 20.0), Sense::hover());
                ui.painter().text(
                    Pos2::new(rect.left() + 12.0 + number_width, rect.center().y),
                    Align2::LEFT_CENTER,
                    "… more not shown",
                    font.clone(),
                    p.muted,
                );
            }
        });
}

/// Minimal highlighting that works for most config and code files:
/// quoted strings and line comments.
fn highlight(line: &str, font: &egui::FontId, p: &Palette) -> LayoutJob {
    let mut job = LayoutJob::default();
    let format = |color| TextFormat {
        font_id: font.clone(),
        color,
        ..Default::default()
    };
    let mut rest = line;
    while !rest.is_empty() {
        let comment = [rest.find("//"), rest.find('#')]
            .into_iter()
            .flatten()
            .min();
        let quote = rest.find(['"', '\'']);
        match (comment, quote) {
            (Some(c), q) if q.is_none_or(|q| c < q) => {
                job.append(&rest[..c], 0.0, format(p.code_text));
                job.append(&rest[c..], 0.0, format(p.muted));
                break;
            }
            (_, Some(q)) => {
                let mark = rest[q..].chars().next().unwrap_or('"');
                let end = rest[q + 1..]
                    .find(mark)
                    .map_or(rest.len(), |e| q + 1 + e + 1);
                job.append(&rest[..q], 0.0, format(p.code_text));
                job.append(&rest[q..end], 0.0, format(p.code_string));
                rest = &rest[end..];
            }
            _ => {
                job.append(rest, 0.0, format(p.code_text));
                break;
            }
        }
    }
    job
}

#[allow(clippy::too_many_arguments)]
fn image_block(
    ui: &mut egui::Ui,
    app: &mut ExploristApp,
    p: &Palette,
    path: &std::path::Path,
    width: u32,
    height: u32,
    pixels: &[u8],
    scaled: [usize; 2],
) {
    let stale = app
        .preview_image
        .as_ref()
        .is_none_or(|img| img.path != path);
    if stale {
        let image = egui::ColorImage::from_rgba_unmultiplied(scaled, pixels);
        let texture = ui
            .ctx()
            .load_texture("preview-image", image, TextureOptions::LINEAR);
        app.preview_image = Some(PreviewImage {
            path: path.to_path_buf(),
            texture,
        });
    }
    let Some(img) = &app.preview_image else {
        return;
    };
    let max_w = ui.available_width();
    let aspect = scaled[1] as f32 / scaled[0].max(1) as f32;
    let w = max_w.min(scaled[0] as f32);
    let h = (w * aspect).min(400.0);
    let w = h / aspect;
    let (rect, _) = ui.allocate_exact_size(vec2(max_w, h), Sense::hover());
    let image_rect = Rect::from_center_size(rect.center(), vec2(w, h));
    ui.painter()
        .rect_filled(rect, CornerRadius::same(radius::MD), p.bg_0);
    ui.painter().image(
        img.texture.id(),
        image_rect,
        Rect::from_min_max(Pos2::ZERO, Pos2::new(1.0, 1.0)),
        Color32::WHITE,
    );
    ui.label(
        egui::RichText::new(format!("{width} × {height} px"))
            .font(crate::ui::fonts::mono(11.5))
            .color(p.muted),
    );
}
