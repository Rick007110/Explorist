//! Properties (Alt Enter): size, dates and attributes of the selection, or
//! of the folder when nothing is selected.

use std::path::Path;

use egui::{Align2, CornerRadius, Frame, Id, Margin, Modal, Pos2, Rect, Sense, Stroke, vec2};

use crate::app::ExploristApp;
use crate::fs::entry::Entry;
use crate::fs::props::{self, Counter, Details};
use crate::fs::{format, kind::FileKind};
use crate::ui::icons::{self, Icon};
use crate::ui::theme::{radius, text};
use crate::ui::widgets::{self, ButtonStyle};

/// Width of the label column.
const LABEL_WIDTH: f32 = 92.0;

pub struct PropertiesState {
    entries: Vec<Entry>,
    /// Dates and attributes, when there's a single item.
    details: Option<Details>,
    /// Width and height, for one image.
    dimensions: Option<(u32, u32)>,
    counter: Counter,
    error: Option<String>,
    /// Shown for the first time this frame: the Enter that opened it
    /// mustn't close it.
    fresh: bool,
    /// A new id each time, so the dialog sizes itself afresh.
    id: u64,
}

impl PropertiesState {
    pub fn new(ctx: &egui::Context, entries: Vec<Entry>) -> Self {
        let details = match entries.as_slice() {
            [one] => props::details(&one.path).ok(),
            _ => None,
        };
        let dimensions = match entries.as_slice() {
            [one] if one.kind == FileKind::Image => image::image_dimensions(&one.path).ok(),
            _ => None,
        };
        let ctx = ctx.clone();
        let counter = Counter::start(
            entries.iter().map(|e| e.path.clone()).collect(),
            move || ctx.request_repaint(),
        );
        Self {
            entries,
            details,
            dimensions,
            counter,
            error: None,
            fresh: true,
            id: {
                static NEXT: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
                NEXT.fetch_add(1, std::sync::atomic::Ordering::Relaxed)
            },
        }
    }
}

pub fn show(ctx: &egui::Context, app: &mut ExploristApp) {
    let Some(state) = &mut app.view.properties else {
        return;
    };
    let p = app.palette;
    let date_format = app.settings.date_format;
    let totals = state.counter.totals();
    let fresh = std::mem::take(&mut state.fresh);
    let mut close = false;
    let mut copy_path = None;

    let modal = Modal::new(Id::new(("properties", state.id)))
        .backdrop_color(p.scrim)
        .frame(
            Frame::NONE
                .fill(p.surface)
                .stroke(Stroke::new(1.0, p.border))
                .corner_radius(CornerRadius::same(radius::LG))
                .shadow(p.shadow_modal())
                .inner_margin(Margin::same(24)),
        )
        .show(ctx, |ui| {
            ui.set_width(440.0);
            ui.spacing_mut().item_spacing = vec2(0.0, 0.0);
            let single = state.entries.len() == 1;
            let first = &state.entries[0];
            let any_dir = state.entries.iter().any(|e| e.is_dir);

            // Icon, name and what it is.
            let (title, subtitle, icon, is_dir) = if single {
                (
                    first.name.clone(),
                    first.type_label.to_string(),
                    Icon::for_kind(first.kind),
                    first.is_dir,
                )
            } else {
                let folders = state.entries.iter().filter(|e| e.is_dir).count();
                let files = state.entries.len() - folders;
                (
                    format!("{} items", format::count(state.entries.len() as u64)),
                    kinds(files, folders),
                    if files == 0 { Icon::Folder } else { Icon::File },
                    files == 0,
                )
            };
            let (head, _) =
                ui.allocate_exact_size(vec2(ui.available_width(), 44.0), Sense::hover());
            let tile = Rect::from_min_size(head.min, vec2(44.0, 44.0));
            ui.painter().rect(
                tile,
                CornerRadius::same(radius::MD),
                p.bg_2,
                Stroke::new(1.0, p.border_subtle),
                egui::StrokeKind::Inside,
            );
            icons::paint(
                ui.painter(),
                tile,
                icon,
                22.0,
                if is_dir { p.accent } else { p.text_2 },
            );
            let text_left = tile.right() + 14.0;
            let name = widgets::truncated(
                ui,
                &title,
                text::heading(),
                p.text,
                head.right() - text_left,
            );
            ui.painter()
                .galley(Pos2::new(text_left, head.top() + 3.0), name, p.text);
            ui.painter().text(
                Pos2::new(text_left, head.bottom() - 3.0),
                Align2::LEFT_BOTTOM,
                subtitle,
                text::body(),
                p.muted,
            );
            ui.add_space(18.0);
            divider(ui, &p);

            // Where and how big.
            let location = first
                .path
                .parent()
                .map(|l| l.display().to_string())
                .unwrap_or_else(|| first.path.display().to_string());
            row(ui, &p, "Location", &location, true);
            if let Some(target) = state.details.as_ref().and_then(|d| d.link_target.as_ref()) {
                row(ui, &p, "Links to", &target.display().to_string(), true);
            }
            let counting = if totals.done { "" } else { " …" };
            let size = format!(
                "{} ({}){counting}",
                format::size(totals.bytes),
                format::exact_size(totals.bytes)
            );
            row(ui, &p, "Size", &size, true);
            if let Some((w, h)) = state.dimensions {
                row(ui, &p, "Dimensions", &format!("{w} × {h} pixels"), true);
            }
            if any_dir {
                let mut contains = kinds(totals.files as usize, totals.folders as usize);
                if totals.unreadable > 0 {
                    contains.push_str(&format!(
                        " · {} couldn’t be read",
                        format::count(totals.unreadable)
                    ));
                }
                contains.push_str(counting);
                row(ui, &p, "Contains", &contains, false);
            }

            // Dates, for one item.
            if let Some(d) = &state.details {
                divider(ui, &p);
                for (label, time) in [
                    ("Created", d.created),
                    ("Modified", d.modified),
                    ("Accessed", d.accessed),
                ] {
                    if let Some(time) = time {
                        row(ui, &p, label, &format::date(time, date_format), true);
                    }
                }
            }

            // Attributes, for one item.
            if single && let Some(d) = &mut state.details {
                divider(ui, &p);
                let path = first.path.clone();
                if attribute(ui, &p, "Read-only", &mut d.read_only) {
                    state.error = props::set_read_only(&path, d.read_only)
                        .err()
                        .map(|e| format!("Couldn’t change read-only: {e}"));
                }
                #[cfg(windows)]
                if attribute(ui, &p, "Hidden", &mut d.hidden) {
                    state.error = props::set_hidden(&path, d.hidden)
                        .err()
                        .map(|e| format!("Couldn’t change hidden: {e}"));
                }
                #[cfg(not(windows))]
                if d.hidden {
                    row(ui, &p, "Hidden", "Yes (the name starts with a dot)", false);
                }
            }
            if let Some(err) = &state.error {
                ui.add_space(8.0);
                ui.label(
                    egui::RichText::new(err)
                        .font(text::caption())
                        .color(p.error),
                );
            }

            ui.add_space(20.0);
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                ui.spacing_mut().item_spacing.x = 8.0;
                let done = widgets::button(ui, &p, "Close", ButtonStyle::Primary, Some("Esc"));
                let copy = widgets::button(ui, &p, "Copy path", ButtonStyle::Ghost, None);
                if done.clicked() || (!fresh && ui.input(|i| i.key_pressed(egui::Key::Enter))) {
                    close = true;
                }
                if copy.clicked() {
                    copy_path = Some(paths_text(&state.entries));
                }
            });
        });
    if modal.should_close() {
        close = true;
    }
    if let Some(text) = copy_path {
        ctx.copy_text(text);
        app.toast("Path copied".to_owned(), false);
    }
    if close {
        app.view.properties = None;
    }
}

fn kinds(files: usize, folders: usize) -> String {
    let plural = |n: usize, one: &str, many: &str| {
        format!(
            "{} {}",
            format::count(n as u64),
            if n == 1 { one } else { many }
        )
    };
    match (files, folders) {
        (f, 0) => plural(f, "file", "files"),
        (0, d) => plural(d, "folder", "folders"),
        (f, d) => format!(
            "{}, {}",
            plural(f, "file", "files"),
            plural(d, "folder", "folders")
        ),
    }
}

fn paths_text(entries: &[Entry]) -> String {
    entries
        .iter()
        .map(|e| e.path.display().to_string())
        .collect::<Vec<_>>()
        .join("\n")
}

fn divider(ui: &mut egui::Ui, p: &crate::ui::theme::Palette) {
    ui.add_space(8.0);
    let (r, _) = ui.allocate_exact_size(vec2(ui.available_width(), 1.0), Sense::hover());
    ui.painter()
        .hline(r.x_range(), r.center().y, Stroke::new(1.0, p.border_subtle));
    ui.add_space(8.0);
}

/// A label and its value, which may wrap (paths) onto more lines.
fn row(ui: &mut egui::Ui, p: &crate::ui::theme::Palette, label: &str, value: &str, mono: bool) {
    let width = ui.available_width();
    let font = if mono {
        crate::ui::fonts::mono(12.0)
    } else {
        text::body()
    };
    let mut job = egui::text::LayoutJob::single_section(
        value.to_owned(),
        egui::TextFormat::simple(font, p.text_2),
    );
    job.wrap = egui::text::TextWrapping {
        max_width: width - LABEL_WIDTH,
        max_rows: 3,
        break_anywhere: true,
        overflow_character: Some('…'),
    };
    let galley = ui.painter().layout_job(job);
    let height = galley.size().y.max(18.0) + 10.0;
    let (r, response) = ui.allocate_exact_size(vec2(width, height), Sense::hover());
    ui.painter().text(
        Pos2::new(r.left(), r.top() + 5.0),
        Align2::LEFT_TOP,
        label,
        text::body(),
        p.muted,
    );
    ui.painter().galley(
        Pos2::new(r.left() + LABEL_WIDTH, r.top() + 6.0),
        galley.clone(),
        p.text_2,
    );
    if galley.elided {
        response.on_hover_text(value);
    }
}

/// A label with a switch. Returns true when flipped.
fn attribute(ui: &mut egui::Ui, p: &crate::ui::theme::Palette, label: &str, on: &mut bool) -> bool {
    let mut changed = false;
    ui.horizontal(|ui| {
        ui.set_height(30.0);
        let (r, _) = ui.allocate_exact_size(vec2(LABEL_WIDTH, 30.0), Sense::hover());
        ui.painter().text(
            Pos2::new(r.left(), r.center().y),
            Align2::LEFT_CENTER,
            label,
            text::body(),
            p.muted,
        );
        changed = widgets::switch(ui, p, on);
    });
    changed
}

/// What Properties should describe: the selection, or else the folder.
pub fn targets(app: &ExploristApp) -> Vec<Entry> {
    let selected: Vec<Entry> = app
        .browser
        .selected_entries()
        .into_iter()
        .cloned()
        .collect();
    if !selected.is_empty() {
        return selected;
    }
    folder_entry(app.browser.location()).into_iter().collect()
}

fn folder_entry(path: &Path) -> Option<Entry> {
    let mut entry = Entry::from_path(path).ok()?;
    if entry.name.is_empty() {
        // A drive root has no file name.
        entry.name = path.display().to_string();
    }
    entry.kind = FileKind::Folder;
    Some(entry)
}
