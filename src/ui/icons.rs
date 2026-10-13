//! The design's outline icons, drawn with egui's painter.
//!
//! Each icon is drawn on a 16-unit grid (the design's SVG viewBox) and
//! scaled to the target rect, with a 1.5-unit round-capped stroke. Icons
//! are told apart by silhouette, not color.

use std::f32::consts::{FRAC_PI_2, PI};

use egui::{Color32, Painter, Pos2, Rect, Shape, Stroke, vec2};

use crate::fs::kind::FileKind;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Icon {
    // Files
    Folder,
    Code,
    Text,
    Config,
    Lock,
    Pdf,
    Image,
    Vector,
    Archive,
    Audio,
    Video,
    Exe,
    File,
    // Navigation and tools
    Back,
    Forward,
    Up,
    Refresh,
    Filter,
    Close,
    Plus,
    SortUp,
    SortDown,
    Check,
    Warning,
    // Places
    Home,
    Desktop,
    Downloads,
    Documents,
    Drive,
    // Window buttons, on a 10-unit grid
    Minimize,
    Maximize,
    Restore,
    CloseWindow,
}

impl Icon {
    pub fn for_kind(kind: FileKind) -> Self {
        match kind {
            FileKind::Folder => Self::Folder,
            FileKind::Code => Self::Code,
            FileKind::Text => Self::Text,
            FileKind::Config => Self::Config,
            FileKind::Lock => Self::Lock,
            FileKind::Pdf => Self::Pdf,
            FileKind::Image => Self::Image,
            FileKind::Vector => Self::Vector,
            FileKind::Archive => Self::Archive,
            FileKind::Audio => Self::Audio,
            FileKind::Video => Self::Video,
            FileKind::Executable => Self::Exe,
            FileKind::Other => Self::File,
        }
    }
}

/// Draws `icon` centered in `rect` at `size` points.
pub fn paint(painter: &Painter, rect: Rect, icon: Icon, size: f32, color: Color32) {
    let grid = match icon {
        Icon::Minimize | Icon::Maximize | Icon::Restore | Icon::CloseWindow => 10.0,
        _ => 16.0,
    };
    let origin = rect.center() - vec2(size, size) / 2.0;
    let pen = Pen {
        painter,
        origin,
        scale: size / grid,
        stroke: Stroke::new(1.5 * size / 16.0, color),
        shapes: Vec::new(),
    };
    draw(pen, icon);
}

/// Collects shapes in grid units and paints them in one go.
struct Pen<'a> {
    painter: &'a Painter,
    origin: Pos2,
    scale: f32,
    stroke: Stroke,
    shapes: Vec<Shape>,
}

impl Pen<'_> {
    fn p(&self, x: f32, y: f32) -> Pos2 {
        self.origin + vec2(x, y) * self.scale
    }

    /// An open polyline with round joins and caps.
    fn line(&mut self, points: &[(f32, f32)]) {
        let pts: Vec<Pos2> = points.iter().map(|&(x, y)| self.p(x, y)).collect();
        self.caps(&pts);
        self.shapes.push(Shape::line(pts, self.stroke));
    }

    /// A closed polygon outline.
    fn closed(&mut self, points: &[(f32, f32)]) {
        let pts: Vec<Pos2> = points.iter().map(|&(x, y)| self.p(x, y)).collect();
        self.caps(&pts);
        self.shapes.push(Shape::closed_line(pts, self.stroke));
    }

    /// egui strokes have square ends; dots at each vertex give the design's
    /// round caps and joins.
    fn caps(&mut self, pts: &[Pos2]) {
        let r = self.stroke.width / 2.0;
        for &pt in pts {
            self.shapes
                .push(Shape::circle_filled(pt, r, self.stroke.color));
        }
    }

    /// A dot: the design's zero-length round-capped segments.
    fn dot(&mut self, x: f32, y: f32) {
        let r = self.stroke.width / 2.0;
        self.shapes
            .push(Shape::circle_filled(self.p(x, y), r, self.stroke.color));
    }

    fn rect(&mut self, x: f32, y: f32, w: f32, h: f32, radius: f32) {
        let rect = Rect::from_min_size(self.p(x, y), vec2(w, h) * self.scale);
        self.shapes.push(Shape::rect_stroke(
            rect,
            radius * self.scale,
            self.stroke,
            egui::StrokeKind::Middle,
        ));
    }

    fn circle(&mut self, cx: f32, cy: f32, r: f32) {
        self.shapes.push(Shape::circle_stroke(
            self.p(cx, cy),
            r * self.scale,
            self.stroke,
        ));
    }

    fn fill_triangle(&mut self, points: [(f32, f32); 3]) {
        let pts = points.iter().map(|&(x, y)| self.p(x, y)).collect();
        self.shapes
            .push(Shape::convex_polygon(pts, self.stroke.color, Stroke::NONE));
    }

    /// Points along an arc, angles in radians (0 = right, clockwise in
    /// screen space), for building paths.
    fn arc(cx: f32, cy: f32, r: f32, from: f32, to: f32) -> Vec<(f32, f32)> {
        const STEPS: usize = 8;
        (0..=STEPS)
            .map(|i| {
                let a = from + (to - from) * i as f32 / STEPS as f32;
                (cx + r * a.cos(), cy + r * a.sin())
            })
            .collect()
    }

    fn finish(self) {
        self.painter.extend(self.shapes);
    }
}

/// The page outline shared by document icons, with its folded corner.
fn page(pen: &mut Pen<'_>) {
    pen.closed(&[
        (4.0, 1.75),
        (9.0, 1.75),
        (12.25, 5.0),
        (12.25, 14.25),
        (4.0, 14.25),
    ]);
    pen.line(&[(9.0, 1.75), (9.0, 5.0), (12.25, 5.0)]);
}

fn draw(mut pen: Pen<'_>, icon: Icon) {
    match icon {
        Icon::Folder => {
            // M2 4.5 A1.5 1.5 0 0 1 3.5 3 h3 l1.5 1.5 h4.5 A1.5 1.5 0 0 1 14 6
            // v5.5 a1.5 1.5 0 0 1 -1.5 1.5 h-9 A1.5 1.5 0 0 1 2 11.5 z
            let mut pts = Pen::arc(3.5, 4.5, 1.5, PI, 1.5 * PI);
            pts.extend([(6.5, 3.0), (8.0, 4.5)]);
            pts.extend(Pen::arc(12.5, 6.0, 1.5, 1.5 * PI, 2.0 * PI));
            pts.extend(Pen::arc(12.5, 11.5, 1.5, 0.0, FRAC_PI_2));
            pts.extend(Pen::arc(3.5, 11.5, 1.5, FRAC_PI_2, PI));
            pen.closed(&pts);
        }
        Icon::Code => {
            page(&mut pen);
            pen.line(&[(6.75, 8.25), (5.5, 9.75), (6.75, 11.25)]);
            pen.line(&[(9.5, 8.25), (10.75, 9.75), (9.5, 11.25)]);
        }
        Icon::Text => {
            page(&mut pen);
            pen.line(&[(6.0, 8.0), (10.0, 8.0)]);
            pen.line(&[(6.0, 10.0), (10.0, 10.0)]);
            pen.line(&[(6.0, 12.0), (8.5, 12.0)]);
        }
        Icon::Documents => {
            page(&mut pen);
            pen.line(&[(6.0, 8.0), (10.0, 8.0)]);
            pen.line(&[(6.0, 10.5), (10.0, 10.5)]);
        }
        Icon::File => page(&mut pen),
        Icon::Pdf => {
            page(&mut pen);
            pen.rect(5.75, 8.25, 4.5, 3.5, 0.75);
        }
        Icon::Config => {
            pen.line(&[(2.5, 4.5), (6.5, 4.5)]);
            pen.line(&[(9.5, 4.5), (13.5, 4.5)]);
            pen.line(&[(2.5, 11.5), (9.5, 11.5)]);
            pen.line(&[(12.5, 11.5), (13.5, 11.5)]);
            pen.circle(8.0, 4.5, 1.5);
            pen.circle(11.0, 11.5, 1.5);
        }
        Icon::Lock => {
            pen.rect(3.5, 7.0, 9.0, 6.75, 1.5);
            let mut shackle = vec![(5.5, 7.0)];
            shackle.extend(Pen::arc(8.0, 5.0, 2.5, PI, 2.0 * PI));
            shackle.push((10.5, 7.0));
            pen.line(&shackle);
        }
        Icon::Image => {
            pen.rect(2.0, 2.5, 12.0, 11.0, 1.5);
            pen.circle(6.0, 6.25, 1.25);
            pen.line(&[
                (2.5, 12.0),
                (6.0, 8.5),
                (8.5, 11.0),
                (10.5, 9.0),
                (13.5, 12.0),
            ]);
        }
        Icon::Vector => {
            pen.line(&[(3.0, 13.0), (8.0, 3.0), (13.0, 13.0)]);
            pen.rect(1.75, 11.75, 2.5, 2.5, 0.5);
            pen.rect(11.75, 11.75, 2.5, 2.5, 0.5);
            pen.rect(6.75, 1.75, 2.5, 2.5, 0.5);
        }
        Icon::Archive => {
            pen.rect(2.5, 2.5, 11.0, 11.0, 1.5);
            pen.line(&[(8.0, 2.5), (8.0, 4.0)]);
            pen.line(&[(8.0, 5.0), (8.0, 6.5)]);
            pen.line(&[(8.0, 7.5), (8.0, 9.0)]);
            pen.rect(6.75, 9.25, 2.5, 2.5, 0.5);
        }
        Icon::Audio => {
            pen.line(&[(6.0, 12.0), (6.0, 4.0), (13.0, 2.5), (13.0, 10.5)]);
            pen.circle(4.5, 12.0, 1.5);
            pen.circle(11.5, 10.5, 1.5);
        }
        Icon::Video => {
            pen.rect(2.0, 3.0, 12.0, 10.0, 1.5);
            pen.closed(&[(6.75, 6.0), (6.75, 10.0), (10.25, 8.0)]);
        }
        Icon::Exe => {
            pen.rect(2.0, 2.5, 12.0, 11.0, 1.5);
            pen.line(&[(2.0, 5.5), (14.0, 5.5)]);
            pen.line(&[(5.0, 8.5), (7.0, 10.0), (5.0, 11.5)]);
            pen.line(&[(8.5, 11.5), (11.0, 11.5)]);
        }
        Icon::Back => pen.line(&[(10.0, 3.5), (5.5, 8.0), (10.0, 12.5)]),
        Icon::Forward => pen.line(&[(6.0, 3.5), (10.5, 8.0), (6.0, 12.5)]),
        Icon::Up => {
            pen.line(&[(8.0, 13.0), (8.0, 3.5)]);
            pen.line(&[(3.5, 8.0), (8.0, 3.5), (12.5, 8.0)]);
        }
        Icon::Refresh => {
            // Not in the design; built from its arrow style.
            let pts = Pen::arc(8.0, 8.0, 5.0, -0.25 * PI, 1.5 * PI);
            pen.line(&pts);
            pen.line(&[(11.5, 1.75), (11.55, 4.45), (8.85, 4.5)]);
        }
        Icon::Filter => {
            pen.line(&[(2.5, 4.0), (13.5, 4.0)]);
            pen.line(&[(4.5, 8.0), (11.5, 8.0)]);
            pen.line(&[(6.5, 12.0), (9.5, 12.0)]);
        }
        Icon::Close => {
            pen.line(&[(4.0, 4.0), (12.0, 12.0)]);
            pen.line(&[(12.0, 4.0), (4.0, 12.0)]);
        }
        Icon::Plus => {
            pen.line(&[(8.0, 2.75), (8.0, 13.25)]);
            pen.line(&[(2.75, 8.0), (13.25, 8.0)]);
        }
        Icon::SortUp => {
            pen.line(&[(8.0, 12.7), (8.0, 3.3)]);
            pen.line(&[(4.0, 7.3), (8.0, 3.3), (12.0, 7.3)]);
        }
        Icon::SortDown => {
            pen.line(&[(8.0, 3.3), (8.0, 12.7)]);
            pen.line(&[(4.0, 8.7), (8.0, 12.7), (12.0, 8.7)]);
        }
        Icon::Check => pen.line(&[(3.5, 8.5), (6.5, 11.5), (12.5, 4.5)]),
        Icon::Warning => {
            pen.closed(&[(8.0, 2.5), (14.0, 13.0), (2.0, 13.0)]);
            pen.line(&[(8.0, 6.5), (8.0, 9.5)]);
            pen.dot(8.0, 11.5);
        }
        Icon::Home => {
            pen.closed(&[
                (2.5, 7.0),
                (8.0, 2.5),
                (13.5, 7.0),
                (13.5, 13.5),
                (2.5, 13.5),
            ]);
            pen.line(&[(6.5, 13.5), (6.5, 10.0), (9.5, 10.0), (9.5, 13.5)]);
        }
        Icon::Desktop => {
            pen.rect(2.0, 2.5, 12.0, 8.5, 1.5);
            pen.line(&[(5.5, 13.5), (10.5, 13.5)]);
            pen.line(&[(8.0, 11.0), (8.0, 13.5)]);
        }
        Icon::Downloads => {
            pen.line(&[(8.0, 2.5), (8.0, 10.5)]);
            pen.line(&[(4.5, 7.5), (8.0, 11.0), (11.5, 7.5)]);
            pen.line(&[(2.5, 13.5), (13.5, 13.5)]);
        }
        Icon::Drive => {
            pen.rect(2.0, 4.0, 12.0, 8.0, 1.5);
            pen.dot(11.0, 8.0);
        }
        Icon::Minimize => pen.line(&[(0.5, 5.0), (9.5, 5.0)]),
        Icon::Maximize => pen.rect(0.6, 0.6, 8.8, 8.8, 1.5),
        Icon::Restore => {
            pen.rect(0.6, 2.6, 6.8, 6.8, 1.2);
            pen.line(&[(2.6, 2.6), (2.6, 0.6), (9.4, 0.6), (9.4, 7.4), (7.4, 7.4)]);
        }
        Icon::CloseWindow => {
            pen.line(&[(0.5, 0.5), (9.5, 9.5)]);
            pen.line(&[(9.5, 0.5), (0.5, 9.5)]);
        }
    }
    pen.finish();
}

/// The filled triangle used for menu chevrons.
pub fn paint_chevron_down(painter: &Painter, center: Pos2, color: Color32) {
    let pen = Pen {
        painter,
        origin: center - vec2(4.0, 4.0),
        scale: 0.5,
        stroke: Stroke::new(1.0, color),
        shapes: Vec::new(),
    };
    let mut pen = pen;
    pen.fill_triangle([(4.0, 6.0), (12.0, 6.0), (8.0, 10.0)]);
    pen.finish();
}

/// Size of the area an icon of `size` needs.
pub fn icon_rect(center: Pos2, size: f32) -> Rect {
    Rect::from_center_size(center, vec2(size, size))
}
