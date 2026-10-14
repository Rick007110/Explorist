//! Dragging files: onto folders in the list, the grid or the sidebar, and
//! out of the window into other apps.

use std::path::{Path, PathBuf};

use egui::{CornerRadius, Pos2, Rect, Stroke, StrokeKind, vec2};

use crate::app::ExploristApp;
use crate::fs::jobs::TransferKind;
use crate::ui::theme::{Palette, radius, text};

/// Files being dragged inside the window.
pub struct FileDrag {
    pub paths: Vec<PathBuf>,
    /// The folder under the pointer this frame, if it can take the files.
    target: Option<PathBuf>,
}

/// Whether files can be dragged out to other apps here. Linux would need
/// GTK for it.
pub const OUT_OF_WINDOW: bool = cfg!(any(windows, target_os = "macos"));

/// Ctrl (Option on macOS) copies instead of moving.
fn copying(ctx: &egui::Context) -> bool {
    ctx.input(|i| {
        if cfg!(target_os = "macos") {
            i.modifiers.alt
        } else {
            i.modifiers.ctrl
        }
    })
}

/// Whether `folder` can take `paths`: not one of them, and not where they
/// already are.
pub fn accepts(folder: &Path, paths: &[PathBuf]) -> bool {
    !paths.iter().any(|p| p == folder) && !paths.iter().all(|p| p.parent() == Some(folder))
}

impl ExploristApp {
    /// A row or tile started being dragged: drag the selection, or just
    /// this item when it isn't selected.
    pub(crate) fn begin_drag(&mut self, row: usize, selected: bool) {
        if !selected {
            self.browser.select(|sel, rows| sel.click(rows, row));
        }
        let paths = self.selected_paths();
        if !paths.is_empty() {
            self.view.drag = Some(FileDrag {
                paths,
                target: None,
            });
        }
    }

    /// Called for each folder drawn while dragging. Returns true when the
    /// pointer is over it and it would take the files, so it can be
    /// highlighted.
    pub(crate) fn drop_target(&mut self, ui: &egui::Ui, rect: Rect, folder: &Path) -> bool {
        let Some(drag) = &mut self.view.drag else {
            return false;
        };
        if !ui.rect_contains_pointer(rect) || !accepts(folder, &drag.paths) {
            return false;
        }
        drag.target = Some(folder.to_path_buf());
        true
    }

    /// After the UI is drawn: drop, hand the drag to the system once the
    /// pointer leaves the window, and show what will happen.
    pub(crate) fn finish_drag(&mut self, ctx: &egui::Context, frame: &eframe::Frame) {
        let Some(drag) = &mut self.view.drag else {
            return;
        };
        let target = drag.target.take();
        let (down, pos) = ctx.input(|i| (i.pointer.primary_down(), i.pointer.latest_pos()));
        let inside = pos.is_some_and(|p| ctx.content_rect().shrink(2.0).contains(p));

        if !down {
            let drag = self.view.drag.take().expect("checked above");
            if let Some(folder) = target {
                let kind = if copying(ctx) {
                    TransferKind::Copy
                } else {
                    TransferKind::Move
                };
                self.transfer(kind, &drag.paths, &folder, false);
            }
            return;
        }
        if !inside && OUT_OF_WINDOW {
            let drag = self.view.drag.take().expect("checked above");
            self.drag_out(frame, drag.paths);
            return;
        }

        ctx.set_cursor_icon(match (&target, copying(ctx)) {
            (None, _) => egui::CursorIcon::Grabbing,
            (Some(_), true) => egui::CursorIcon::Copy,
            (Some(_), false) => egui::CursorIcon::Move,
        });
        if let Some(pos) = pos {
            let label = drag_label(&drag.paths, target.as_deref(), copying(ctx));
            paint_badge(ctx, &self.palette, pos, &label);
        }
        ctx.request_repaint();
    }

    #[cfg(any(windows, target_os = "macos"))]
    fn drag_out(&mut self, frame: &eframe::Frame, paths: Vec<PathBuf>) {
        let image = drag::Image::Raw(include_bytes!("../../assets/drag.png").to_vec());
        let result = drag::start_drag(
            frame,
            drag::DragItem::Files(paths),
            image,
            |_, _| {},
            drag::Options::default(),
        );
        // The system kept the button release to itself.
        self.view.release_pointer = true;
        if let Err(err) = result {
            self.toast(format!("Couldn’t drag out: {err}"), true);
        }
    }

    #[cfg(not(any(windows, target_os = "macos")))]
    fn drag_out(&mut self, _frame: &eframe::Frame, _paths: Vec<PathBuf>) {}
}

/// "Move 3 items to “src”", or the item count when not over a folder.
fn drag_label(paths: &[PathBuf], target: Option<&Path>, copy: bool) -> String {
    let what = match paths {
        [one] => one
            .file_name()
            .map(|n| format!("“{}”", n.to_string_lossy()))
            .unwrap_or_else(|| "1 item".to_owned()),
        many => format!("{} items", many.len()),
    };
    match target {
        Some(folder) => {
            let name = folder
                .file_name()
                .map(|n| n.to_string_lossy().into_owned())
                .unwrap_or_else(|| folder.display().to_string());
            let verb = if copy { "Copy" } else { "Move" };
            format!("{verb} {what} to “{name}”")
        }
        None => what,
    }
}

/// The label that follows the pointer.
fn paint_badge(ctx: &egui::Context, p: &Palette, pos: Pos2, label: &str) {
    let painter = ctx.layer_painter(egui::LayerId::new(
        egui::Order::Tooltip,
        egui::Id::new("drag-badge"),
    ));
    let galley = painter.layout_no_wrap(label.to_owned(), text::label(), p.text);
    let rect = Rect::from_min_size(pos + vec2(16.0, 14.0), galley.size() + vec2(20.0, 12.0));
    painter.rect(
        rect,
        CornerRadius::same(radius::SM),
        p.raised,
        Stroke::new(1.0, p.border),
        StrokeKind::Inside,
    );
    painter.galley(rect.min + vec2(10.0, 6.0), galley, p.text);
}

/// Accent outline on the folder files would drop into.
pub fn paint_drop_highlight(painter: &egui::Painter, p: &Palette, rect: Rect) {
    painter.rect(
        rect,
        CornerRadius::same(radius::SM),
        p.accent_alpha(if p.dark { 0.14 } else { 0.10 }),
        Stroke::new(1.5, p.accent),
        StrokeKind::Inside,
    );
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn folders_refuse_themselves_and_their_own_children() {
        let src = PathBuf::from("/p/src");
        let a = PathBuf::from("/p/a.txt");
        assert!(accepts(&src, std::slice::from_ref(&a)));
        assert!(!accepts(&src, &[src.clone(), a.clone()]));
        // Already there.
        assert!(!accepts(Path::new("/p"), std::slice::from_ref(&a)));
        // Up to the parent from a subfolder is fine.
        assert!(accepts(Path::new("/"), &[a]));
    }

    #[test]
    fn labels_say_what_happens() {
        let one = [PathBuf::from("/p/a.txt")];
        let two = [PathBuf::from("/p/a"), PathBuf::from("/p/b")];
        assert_eq!(drag_label(&one, None, false), "“a.txt”");
        assert_eq!(
            drag_label(&two, Some(Path::new("/p/src")), false),
            "Move 2 items to “src”"
        );
        assert_eq!(
            drag_label(&one, Some(Path::new("/p/src")), true),
            "Copy “a.txt” to “src”"
        );
    }
}
