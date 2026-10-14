//! The selection box: drag across empty space to select everything the
//! box touches, in the list or the grid. Ctrl (⌘ on macOS) adds to the
//! selection instead of replacing it. Dragging near the top or bottom edge
//! scrolls.
//!
//! The box is kept in content coordinates (scroll offset included), so it
//! stays anchored while the view scrolls under it.

use std::collections::HashSet;
use std::ops::Range;
use std::path::PathBuf;

use egui::{CornerRadius, Pos2, Rect, Stroke, StrokeKind, Vec2, vec2};

use crate::app::ExploristApp;

/// A selection box being dragged.
pub struct Marquee {
    /// Where the drag began, in content coordinates.
    start: Pos2,
    /// What stays selected whatever the box covers.
    base: HashSet<PathBuf>,
}

/// How the list or grid is laid out, in content coordinates.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Layout {
    /// Full-width rows, `step` apart.
    List { row_height: f32, step: f32 },
    /// Tiles in `columns` columns.
    Grid {
        columns: usize,
        tile: Vec2,
        gap: f32,
    },
}

/// Where the scroll area was last frame: the viewport on screen and how
/// far it's scrolled.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Viewport {
    pub rect: Rect,
    pub offset: f32,
}

impl Default for Viewport {
    fn default() -> Self {
        Self {
            rect: Rect::NOTHING,
            offset: 0.0,
        }
    }
}

impl Viewport {
    fn to_content(self, screen: Pos2) -> Pos2 {
        screen - self.rect.min.to_vec2() + vec2(0.0, self.offset)
    }

    fn to_screen(self, content: Pos2) -> Pos2 {
        content + self.rect.min.to_vec2() - vec2(0.0, self.offset)
    }
}

/// The items a box (content coordinates) touches.
pub fn hits(layout: Layout, band: Rect, count: usize) -> Vec<usize> {
    match layout {
        Layout::List { row_height, step } => lines(
            band.y_range().min,
            band.y_range().max,
            row_height,
            step,
            count,
        )
        .collect(),
        Layout::Grid { columns, tile, gap } => {
            let lines_total = count.div_ceil(columns.max(1));
            let cols = lines(
                band.x_range().min,
                band.x_range().max,
                tile.x,
                tile.x + gap,
                columns,
            );
            let mut out = Vec::new();
            for line in lines(
                band.y_range().min,
                band.y_range().max,
                tile.y,
                tile.y + gap,
                lines_total,
            ) {
                for col in cols.clone() {
                    let i = line * columns + col;
                    if i < count {
                        out.push(i);
                    }
                }
            }
            out
        }
    }
}

/// The indices of `size`-long cells spaced `step` apart that overlap
/// `from..to`.
fn lines(from: f32, to: f32, size: f32, step: f32, count: usize) -> Range<usize> {
    if to <= 0.0 || count == 0 {
        return 0..0;
    }
    // Cell i spans i*step .. i*step+size.
    let first = (((from - size) / step).floor() + 1.0).max(0.0) as usize;
    let last = ((to / step).ceil().max(0.0) as usize).min(count);
    first.min(last)..last
}

/// Scroll speed when the pointer is near or past an edge: faster the
/// further out, in points per frame.
pub fn edge_scroll(pointer_y: f32, viewport: Rect) -> f32 {
    const EDGE: f32 = 28.0;
    let top = viewport.top() + EDGE;
    let bottom = viewport.bottom() - EDGE;
    if pointer_y < top {
        -((top - pointer_y) * 0.5).min(40.0)
    } else if pointer_y > bottom {
        ((pointer_y - bottom) * 0.5).min(40.0)
    } else {
        0.0
    }
}

impl ExploristApp {
    /// A drag began on empty space (or a row's blank part).
    pub(crate) fn begin_marquee(&mut self, ctx: &egui::Context) {
        let (origin, additive) = ctx.input(|i| {
            (
                i.pointer.press_origin(),
                i.modifiers.ctrl || i.modifiers.command,
            )
        });
        let Some(origin) = origin else { return };
        if self.view.rename.is_some() && !self.commit_rename() {
            return;
        }
        let base = if additive {
            self.browser.selection.snapshot()
        } else {
            HashSet::new()
        };
        self.view.marquee = Some(Marquee {
            start: self.view.viewport.to_content(origin),
            base,
        });
    }

    /// After the list or grid is drawn: select what the box covers, and
    /// draw it. Returns how far to scroll next frame.
    pub(crate) fn update_marquee(&mut self, ui: &egui::Ui, layout: Layout) -> f32 {
        let Some(marquee) = &self.view.marquee else {
            return 0.0;
        };
        let (down, pos) = ui.input(|i| (i.pointer.primary_down(), i.pointer.latest_pos()));
        let (true, Some(pos)) = (down, pos) else {
            self.view.marquee = None;
            return 0.0;
        };
        let viewport = self.view.viewport;
        let end = viewport.to_content(pos);
        let band = Rect::from_two_pos(marquee.start, end);
        let count = self.browser.rows().len();
        let hit = hits(layout, band, count);
        // Focus the item the box ends at, else the nearest one it covers.
        let focus = if end.y >= marquee.start.y {
            hit.last()
        } else {
            hit.first()
        }
        .copied();
        let base = marquee.base.clone();
        self.browser
            .select(|sel, rows| sel.band(rows, &base, hit, focus));

        let p = self.palette;
        let screen = Rect::from_two_pos(viewport.to_screen(band.min), viewport.to_screen(band.max));
        let painter = ui.painter().with_clip_rect(viewport.rect);
        painter.rect(
            screen,
            CornerRadius::same(2),
            p.accent_alpha(if p.dark { 0.12 } else { 0.10 }),
            Stroke::new(1.0, p.accent),
            StrokeKind::Inside,
        );
        ui.ctx().request_repaint();
        edge_scroll(pos.y, viewport.rect)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const LIST: Layout = Layout::List {
        row_height: 30.0,
        step: 31.0,
    };

    fn band(x0: f32, y0: f32, x1: f32, y1: f32) -> Rect {
        Rect::from_two_pos(Pos2::new(x0, y0), Pos2::new(x1, y1))
    }

    #[test]
    fn list_rows_touched_by_the_box() {
        // Row 0: 0..30, row 1: 31..61, row 2: 62..92.
        assert_eq!(hits(LIST, band(0.0, 5.0, 10.0, 40.0), 10), [0, 1]);
        assert_eq!(
            hits(LIST, band(0.0, 30.2, 10.0, 30.8), 10),
            Vec::<usize>::new()
        );
        assert_eq!(hits(LIST, band(0.0, 40.0, 10.0, 70.0), 10), [1, 2]);
        // Past the end, or above the start.
        assert_eq!(hits(LIST, band(0.0, 250.0, 10.0, 900.0), 10), [8, 9]);
        assert_eq!(
            hits(LIST, band(0.0, -50.0, 10.0, -1.0), 10),
            Vec::<usize>::new()
        );
        assert_eq!(
            hits(LIST, band(0.0, 0.0, 10.0, 100.0), 0),
            Vec::<usize>::new()
        );
    }

    #[test]
    fn grid_tiles_touched_by_the_box() {
        let grid = Layout::Grid {
            columns: 4,
            tile: vec2(100.0, 100.0),
            gap: 4.0,
        };
        // Tiles 1, 2 of line 0 and 5, 6 of line 1.
        assert_eq!(
            hits(grid, band(150.0, 50.0, 250.0, 150.0), 10),
            [1, 2, 5, 6]
        );
        // Line 2 has only tiles 8 and 9.
        assert_eq!(hits(grid, band(0.0, 220.0, 500.0, 260.0), 10), [8, 9]);
        // In the gap between columns.
        assert_eq!(
            hits(grid, band(101.0, 10.0, 103.0, 20.0), 10),
            Vec::<usize>::new()
        );
    }

    #[test]
    fn scrolls_faster_further_past_the_edge() {
        let view = band(0.0, 100.0, 500.0, 600.0);
        assert_eq!(edge_scroll(300.0, view), 0.0);
        assert!(edge_scroll(110.0, view) < 0.0);
        assert!(edge_scroll(590.0, view) > 0.0);
        assert!(edge_scroll(700.0, view) > edge_scroll(590.0, view));
        assert_eq!(edge_scroll(5000.0, view), 40.0);
    }
}
