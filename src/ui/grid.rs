//! The grid view: the folder as tiles, with thumbnails for images.
//!
//! Like the list, only the lines of tiles on screen are laid out.

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::time::SystemTime;

use egui::{
    Align2, Color32, CornerRadius, Frame, Margin, Pos2, Rect, ScrollArea, Sense, Stroke,
    StrokeKind, TextureHandle, TextureOptions, vec2,
};

use crate::app::ExploristApp;
use crate::app::commands::Command;
use crate::fs::entry::Entry;
use crate::fs::git::FileState as GitState;
use crate::fs::kind::FileKind;
use crate::fs::thumbs::Thumbnailer;
use crate::ui::theme::{radius, text};

/// Narrowest a tile gets before a column is dropped.
const TILE_MIN_WIDTH: f32 = 112.0;
const TILE_HEIGHT: f32 = 116.0;
/// Room for the icon or thumbnail at the top of a tile.
const ART_HEIGHT: f32 = 64.0;
const GAP: f32 = 4.0;
const SIDE: f32 = 8.0;
/// Forget all thumbnails past this many, rather than track their age.
const CACHE_LIMIT: usize = 800;

/// Thumbnails made so far, as textures.
#[derive(Default)]
pub struct ThumbCache {
    worker: Option<Thumbnailer>,
    folder: PathBuf,
    /// `None` once we know the file has no thumbnail.
    textures: HashMap<PathBuf, (Option<SystemTime>, Option<TextureHandle>)>,
    /// Asked for, with the modified time at the time.
    requested: HashMap<PathBuf, Option<SystemTime>>,
}

impl ThumbCache {
    /// Takes in finished thumbnails, and starts over in a new folder.
    fn update(&mut self, ctx: &egui::Context, folder: &Path) {
        // Thumbnails stay in memory across folders, so going back is
        // instant; the queue is only for the folder on screen.
        if self.folder != folder {
            self.folder = folder.to_path_buf();
            self.requested.clear();
            if let Some(worker) = &mut self.worker {
                worker.clear_queue();
            }
        }
        if self.textures.len() > CACHE_LIMIT {
            self.textures.clear();
        }
        let Some(worker) = &mut self.worker else {
            return;
        };
        for (path, thumb) in worker.poll() {
            let Some(modified) = self.requested.remove(&path) else {
                continue;
            };
            let texture = thumb.map(|t| {
                let image = egui::ColorImage::from_rgba_unmultiplied(t.size, &t.pixels);
                ctx.load_texture(
                    format!("thumb:{}", path.display()),
                    image,
                    TextureOptions::LINEAR,
                )
            });
            self.textures.insert(path, (modified, texture));
        }
    }

    /// The thumbnail for `entry`, asking for it when it's missing or stale.
    fn get(&mut self, ctx: &egui::Context, entry: &Entry) -> Option<&TextureHandle> {
        let fresh = self
            .textures
            .get(&entry.path)
            .is_some_and(|(modified, _)| *modified == entry.modified);
        if !fresh && self.requested.get(&entry.path) != Some(&entry.modified) {
            let worker = self.worker.get_or_insert_with(|| {
                let ctx = ctx.clone();
                let cache = crate::fs::thumbs::DiskCache::default_dir()
                    .map(crate::fs::thumbs::DiskCache::new);
                Thumbnailer::new(cache, move || ctx.request_repaint())
            });
            worker.request(&entry.path);
            self.requested.insert(entry.path.clone(), entry.modified);
        }
        self.textures.get(&entry.path)?.1.as_ref()
    }
}

pub fn tiles(ui: &mut egui::Ui, app: &mut ExploristApp, commands: &mut Vec<Command>) {
    let count = app.browser.rows().len();
    let area = ui.available_rect_before_wrap();
    let background = ui.interact(area, ui.id().with("grid-bg"), Sense::click_and_drag());
    if background.drag_started() {
        app.begin_marquee(ui.ctx());
    }
    if background.clicked() {
        app.browser.select(|sel, _| sel.clear());
        app.view.rename = None;
    }
    background.context_menu(|ui| super::file_list::background_menu(ui, app, commands));

    let ctx = ui.ctx().clone();
    let location = app.browser.location().to_path_buf();
    app.thumbs.update(&ctx, &location);

    // Scrollbar room on the right, so tiles don't reflow when it shows.
    let inner_width = (area.width() - 2.0 * SIDE - 10.0).max(TILE_MIN_WIDTH);
    let columns = ((inner_width + GAP) / (TILE_MIN_WIDTH + GAP))
        .floor()
        .max(1.0) as usize;
    let tile_width = (inner_width + GAP) / columns as f32 - GAP;
    let lines = count.div_ceil(columns);
    let step = TILE_HEIGHT + GAP;
    app.view.columns = columns;
    app.view.page_rows = (((area.height() - 12.0) / step).floor() as usize).max(1);

    let mut scroll = ScrollArea::vertical()
        .id_salt("grid")
        .auto_shrink([false, false]);
    let marquee_scroll = std::mem::take(&mut app.view.marquee_scroll);
    if marquee_scroll != 0.0 {
        scroll = scroll.vertical_scroll_offset((app.view.list_offset + marquee_scroll).max(0.0));
    }
    if std::mem::take(&mut app.view.scroll_to_focus)
        && let Some(row) = app.browser.selection.focused_row(app.browser.rows())
    {
        let top = (row / columns) as f32 * step;
        let offset = app.view.list_offset;
        let height = app.view.list_height.max(TILE_HEIGHT);
        if top < offset {
            scroll = scroll.vertical_scroll_offset(top);
        } else if top + TILE_HEIGHT > offset + height {
            scroll = scroll.vertical_scroll_offset(top + TILE_HEIGHT - height);
        }
    }

    let output = Frame::NONE
        .inner_margin(Margin {
            left: SIDE as i8,
            right: SIDE as i8,
            top: 8,
            bottom: 8,
        })
        .show(ui, |ui| {
            ui.spacing_mut().item_spacing = vec2(GAP, GAP);
            scroll.show_rows(ui, TILE_HEIGHT, lines, |ui, range| {
                for line in range {
                    ui.horizontal(|ui| {
                        let first = line * columns;
                        for row in first..(first + columns).min(count) {
                            tile(ui, app, row, tile_width, commands);
                        }
                    });
                }
            })
        })
        .inner;
    app.view.list_offset = output.state.offset.y;
    app.view.list_height = output.inner_rect.height();
    app.view.viewport = super::marquee::Viewport {
        rect: output.inner_rect,
        offset: output.state.offset.y,
    };
    app.view.marquee_scroll = app.update_marquee(
        ui,
        super::marquee::Layout::Grid {
            columns,
            tile: vec2(tile_width, TILE_HEIGHT),
            gap: GAP,
        },
    );
}

fn tile(
    ui: &mut egui::Ui,
    app: &mut ExploristApp,
    row: usize,
    width: f32,
    commands: &mut Vec<Command>,
) {
    let p = app.palette;
    let Some(entry) = app.browser.rows().get(row).cloned() else {
        return;
    };
    let (rect, response) =
        ui.allocate_exact_size(vec2(width, TILE_HEIGHT), Sense::click_and_drag());
    if !ui.is_rect_visible(rect) {
        return;
    }
    let selected = app.browser.selection.is_selected(&entry.path);
    let focused = app.browser.selection.is_focused(&entry.path);
    let renaming = app
        .view
        .rename
        .as_ref()
        .is_some_and(|r| r.path == entry.path);
    let fill = if selected || renaming {
        p.selection()
    } else if response.hovered() {
        p.hover
    } else {
        Color32::TRANSPARENT
    };
    ui.painter()
        .rect_filled(rect, CornerRadius::same(radius::MD), fill);
    if entry.is_dir && app.drop_target(ui, rect, &entry.path) {
        crate::app::drag::paint_drop_highlight(ui.painter(), &p, rect);
    }
    if focused {
        ui.painter().rect_stroke(
            rect,
            CornerRadius::same(radius::MD),
            Stroke::new(1.0, p.focus_ring()),
            StrokeKind::Inside,
        );
    }

    let git = app
        .git
        .poll()
        .and_then(|g| g.state_of(&entry.path, entry.is_dir));
    let ignored = git == Some(GitState::Ignored);
    let dim = entry.hidden || ignored || app.view.cut.contains(&entry.path);

    // The picture: a thumbnail for images, or a big icon.
    let art = Rect::from_min_size(
        rect.min + vec2(10.0, 10.0),
        vec2(rect.width() - 20.0, ART_HEIGHT),
    );
    let thumb = if entry.kind == FileKind::Image {
        app.thumbs.get(ui.ctx(), &entry).cloned()
    } else {
        None
    };
    if let Some(texture) = thumb {
        let size = texture.size_vec2();
        let scale = (art.width() / size.x).min(art.height() / size.y).min(1.0);
        let shown = Rect::from_center_size(art.center(), size * scale);
        let tint = if dim {
            Color32::from_white_alpha(140)
        } else {
            Color32::WHITE
        };
        egui::Image::new(&texture)
            .corner_radius(CornerRadius::same(radius::XS))
            .tint(tint)
            .paint_at(ui, shown);
        ui.painter().rect_stroke(
            shown,
            CornerRadius::same(radius::XS),
            Stroke::new(1.0, p.border_subtle),
            StrokeKind::Outside,
        );
    } else {
        let color = if entry.is_dir {
            if dim { p.faint } else { p.accent }
        } else if dim {
            p.faint
        } else {
            p.text_2
        };
        let side = if entry.is_dir { 44.0 } else { 38.0 };
        crate::ui::folder_icon::paint_entry_icon(
            ui.painter(),
            app,
            Rect::from_center_size(art.center(), vec2(side, side)),
            &entry,
            side,
            Some(2.0),
            color,
            dim,
        );
    }
    if let Some(state) = git.filter(|g| *g != GitState::Ignored) {
        let tint = match state {
            GitState::Added | GitState::Untracked => p.success,
            GitState::Conflicted | GitState::Deleted => p.error,
            _ => p.warning,
        };
        let badge = Rect::from_min_size(rect.min + vec2(6.0, 6.0), vec2(18.0, 16.0));
        ui.painter()
            .rect_filled(badge, CornerRadius::same(4), p.bg_2);
        ui.painter().text(
            badge.center(),
            Align2::CENTER_CENTER,
            state.letter(),
            crate::ui::fonts::mono(11.0),
            tint,
        );
        ui.interact(badge, ui.id().with(("git", row)), Sense::hover())
            .on_hover_text(state.describe());
    }

    // The name, on up to two lines.
    let name_rect = Rect::from_min_max(
        Pos2::new(rect.left() + 6.0, art.bottom() + 6.0),
        Pos2::new(rect.right() - 6.0, rect.bottom() - 4.0),
    );
    if renaming {
        let field = Rect::from_min_size(name_rect.min, vec2(name_rect.width(), 26.0));
        let error_area = Rect::from_min_size(
            Pos2::new(name_rect.left(), field.bottom()),
            vec2(name_rect.width(), 16.0),
        );
        super::file_list::rename_field(
            ui,
            app,
            field.shrink2(vec2(4.0, 0.0)),
            error_area,
            commands,
        );
    }
    let mut elided = false;
    if !renaming {
        let font = if entry.is_dir && !dim {
            text::body_medium()
        } else {
            text::body()
        };
        let color = if dim { p.muted } else { p.text };
        let mut job = egui::text::LayoutJob::single_section(
            entry.name.clone(),
            egui::TextFormat::simple(font, color),
        );
        job.wrap = egui::text::TextWrapping {
            max_width: name_rect.width(),
            max_rows: 2,
            break_anywhere: false,
            overflow_character: Some('…'),
        };
        job.halign = egui::Align::Center;
        let galley = ui.painter().layout_job(job);
        elided = galley.elided;
        ui.painter().galley(
            Pos2::new(name_rect.center().x, name_rect.top()),
            galley,
            color,
        );
    }

    // The full name, when it didn't fit.
    let response = if elided {
        response.on_hover_text(&entry.name)
    } else {
        response
    };
    super::file_list::handle_clicks(
        ui, app, &response, &entry, row, selected, renaming, true, commands,
    );
}
