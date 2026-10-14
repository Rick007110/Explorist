//! Icons: Lucide (https://lucide.dev, ISC license) for everything but the
//! window buttons, which are drawn to look like the system's own.
//!
//! The SVGs are rasterized at the size and stroke asked for, in white, and
//! tinted when painted, so one texture serves every color.

use std::collections::HashMap;
use std::sync::{Mutex, OnceLock};

use egui::{Color32, Painter, Pos2, Rect, Shape, Stroke, TextureHandle, vec2};

use crate::fs::kind::FileKind;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
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
    PreviewPanel,
    Branch,
    Grid,
    List,
    Settings,
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

    /// The Lucide SVG, or `None` for the window buttons.
    fn svg(self) -> Option<&'static str> {
        macro_rules! lucide {
            ($name:literal) => {
                include_str!(concat!("../../assets/icons/lucide/", $name, ".svg"))
            };
        }
        Some(match self {
            Self::Folder => lucide!("folder"),
            Self::Code => lucide!("file-code"),
            Self::Text => lucide!("file-text"),
            Self::Config => lucide!("file-cog"),
            Self::Lock => lucide!("lock"),
            Self::Pdf => lucide!("file-type"),
            Self::Image => lucide!("file-image"),
            Self::Vector => lucide!("pen-tool"),
            Self::Archive => lucide!("file-archive"),
            Self::Audio => lucide!("file-audio"),
            Self::Video => lucide!("file-video"),
            Self::Exe => lucide!("app-window"),
            Self::File => lucide!("file"),
            Self::Back => lucide!("arrow-left"),
            Self::Forward => lucide!("arrow-right"),
            Self::Up | Self::SortUp => lucide!("arrow-up"),
            Self::SortDown => lucide!("arrow-down"),
            Self::Refresh => lucide!("refresh-cw"),
            Self::Filter => lucide!("list-filter"),
            Self::Close => lucide!("x"),
            Self::Plus => lucide!("plus"),
            Self::Check => lucide!("check"),
            Self::Warning => lucide!("triangle-alert"),
            Self::PreviewPanel => lucide!("panel-right"),
            Self::Branch => lucide!("git-branch"),
            Self::Grid => lucide!("layout-grid"),
            Self::List => lucide!("list"),
            Self::Settings => lucide!("settings"),
            Self::Home => lucide!("house"),
            Self::Desktop => lucide!("monitor"),
            Self::Downloads => lucide!("folder-down"),
            Self::Documents => lucide!("files"),
            Self::Drive => lucide!("hard-drive"),
            Self::Minimize | Self::Maximize | Self::Restore | Self::CloseWindow => return None,
        })
    }
}

/// Draws `icon` centered in `rect` at `size` points, with Lucide's own
/// line weight (2 units of 24).
pub fn paint(painter: &Painter, rect: Rect, icon: Icon, size: f32, color: Color32) {
    paint_stroked(painter, rect, icon, size, 2.0 * size / 24.0, color);
}

/// Like [`paint`], with lines `width` points wide, for big icons that would
/// look heavy with lines scaled up.
pub fn paint_stroked(
    painter: &Painter,
    rect: Rect,
    icon: Icon,
    size: f32,
    width: f32,
    color: Color32,
) {
    let target = Rect::from_center_size(rect.center(), vec2(size, size));
    let Some(svg) = icon.svg() else {
        window_button(painter, target, icon, color);
        return;
    };
    let ppp = painter.ctx().pixels_per_point();
    let pixels = (size * ppp).round().max(1.0) as u32;
    // In twentieths of a viewBox unit, so near-equal sizes share a texture.
    let stroke = ((width * 24.0 / size) * 20.0).round() as u32;
    let key = (icon, pixels, stroke);

    type Cache = HashMap<(Icon, u32, u32), Option<TextureHandle>>;
    static CACHE: OnceLock<Mutex<Cache>> = OnceLock::new();
    let mut cache = CACHE
        .get_or_init(Default::default)
        .lock()
        .unwrap_or_else(|e| e.into_inner());
    let texture = cache.entry(key).or_insert_with(|| {
        let image = rasterize(svg, pixels, stroke as f32 / 20.0)?;
        Some(painter.ctx().load_texture(
            format!("icon:{icon:?}:{pixels}:{stroke}"),
            image,
            egui::TextureOptions::LINEAR,
        ))
    });
    if let Some(texture) = texture {
        painter.image(
            texture.id(),
            target,
            Rect::from_min_max(Pos2::ZERO, Pos2::new(1.0, 1.0)),
            color,
        );
    }
}

/// Renders a Lucide SVG in white at `pixels` square, with `stroke` (in
/// viewBox units) instead of its usual 2.
fn rasterize(svg: &str, pixels: u32, stroke: f32) -> Option<egui::ColorImage> {
    let svg = svg
        .replace("stroke=\"currentColor\"", "stroke=\"#ffffff\"")
        .replace("stroke-width=\"2\"", &format!("stroke-width=\"{stroke}\""));
    let tree = resvg::usvg::Tree::from_str(&svg, &resvg::usvg::Options::default()).ok()?;
    let mut pixmap = resvg::tiny_skia::Pixmap::new(pixels, pixels)?;
    let scale = pixels as f32 / tree.size().width();
    resvg::render(
        &tree,
        resvg::tiny_skia::Transform::from_scale(scale, scale),
        &mut pixmap.as_mut(),
    );
    Some(egui::ColorImage::from_rgba_premultiplied(
        [pixels as usize, pixels as usize],
        pixmap.data(),
    ))
}

/// Minimize, maximize, restore and close, drawn on a 10-unit grid like the
/// system's caption buttons.
fn window_button(painter: &Painter, rect: Rect, icon: Icon, color: Color32) {
    let scale = rect.width() / 10.0;
    let p = |x: f32, y: f32| rect.min + vec2(x, y) * scale;
    let stroke = Stroke::new(1.0, color);
    let shapes = match icon {
        Icon::Minimize => vec![Shape::line_segment([p(0.5, 5.0), p(9.5, 5.0)], stroke)],
        Icon::Maximize => vec![Shape::rect_stroke(
            Rect::from_min_max(p(0.6, 0.6), p(9.4, 9.4)),
            1.5 * scale,
            stroke,
            egui::StrokeKind::Middle,
        )],
        Icon::Restore => vec![
            Shape::rect_stroke(
                Rect::from_min_max(p(0.6, 2.6), p(7.4, 9.4)),
                1.2 * scale,
                stroke,
                egui::StrokeKind::Middle,
            ),
            Shape::line(
                vec![
                    p(2.6, 2.6),
                    p(2.6, 0.6),
                    p(9.4, 0.6),
                    p(9.4, 7.4),
                    p(7.4, 7.4),
                ],
                stroke,
            ),
        ],
        _ => vec![
            Shape::line_segment([p(0.5, 0.5), p(9.5, 9.5)], stroke),
            Shape::line_segment([p(9.5, 0.5), p(0.5, 9.5)], stroke),
        ],
    };
    painter.extend(shapes);
}

/// The filled triangle used for menu chevrons.
pub fn paint_chevron_down(painter: &Painter, center: Pos2, color: Color32) {
    painter.add(Shape::convex_polygon(
        vec![
            center + vec2(-2.0, -1.0),
            center + vec2(2.0, -1.0),
            center + vec2(0.0, 1.0),
        ],
        color,
        Stroke::NONE,
    ));
}

/// Size of the area an icon of `size` needs.
pub fn icon_rect(center: Pos2, size: f32) -> Rect {
    Rect::from_center_size(center, vec2(size, size))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_lucide_icon_renders() {
        let all = [
            Icon::Folder,
            Icon::Code,
            Icon::Text,
            Icon::Config,
            Icon::Lock,
            Icon::Pdf,
            Icon::Image,
            Icon::Vector,
            Icon::Archive,
            Icon::Audio,
            Icon::Video,
            Icon::Exe,
            Icon::File,
            Icon::Back,
            Icon::Forward,
            Icon::Up,
            Icon::Refresh,
            Icon::Filter,
            Icon::Close,
            Icon::Plus,
            Icon::SortUp,
            Icon::SortDown,
            Icon::Check,
            Icon::Warning,
            Icon::PreviewPanel,
            Icon::Branch,
            Icon::Grid,
            Icon::List,
            Icon::Settings,
            Icon::Home,
            Icon::Desktop,
            Icon::Downloads,
            Icon::Documents,
            Icon::Drive,
        ];
        for icon in all {
            let image = rasterize(icon.svg().unwrap(), 32, 2.0)
                .unwrap_or_else(|| panic!("{icon:?} didn't render"));
            assert_eq!(image.size, [32, 32]);
            let inked = image.pixels.iter().filter(|p| p.a() > 0).count();
            assert!(inked > 20, "{icon:?} came out empty");
        }
        assert_eq!(Icon::Minimize.svg(), None);
    }

    #[test]
    fn stroke_width_changes_the_ink() {
        let svg = Icon::Folder.svg().unwrap();
        let ink = |w| {
            rasterize(svg, 48, w)
                .unwrap()
                .pixels
                .iter()
                .map(|p| p.a() as u32)
                .sum::<u32>()
        };
        assert!(ink(1.0) < ink(2.0));
    }
}
