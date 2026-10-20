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
    paint_svg(
        painter,
        target,
        IconKey::Fixed(icon),
        svg,
        size,
        width,
        color,
    );
}

/// Icons people can give a folder, by name, in the order the picker shows
/// them. The first is the usual folder.
pub const FOLDER_ICONS: &[(&str, &str)] = &[
    (
        "folder",
        include_str!("../../assets/icons/lucide/folder.svg"),
    ),
    (
        "folder-open",
        include_str!("../../assets/icons/lucide/folder-open.svg"),
    ),
    (
        "folder-heart",
        include_str!("../../assets/icons/lucide/folder-heart.svg"),
    ),
    (
        "folder-code",
        include_str!("../../assets/icons/lucide/folder-code.svg"),
    ),
    (
        "folder-git-2",
        include_str!("../../assets/icons/lucide/folder-git-2.svg"),
    ),
    (
        "folder-kanban",
        include_str!("../../assets/icons/lucide/folder-kanban.svg"),
    ),
    (
        "folder-lock",
        include_str!("../../assets/icons/lucide/folder-lock.svg"),
    ),
    (
        "folder-archive",
        include_str!("../../assets/icons/lucide/folder-archive.svg"),
    ),
    (
        "folder-cog",
        include_str!("../../assets/icons/lucide/folder-cog.svg"),
    ),
    (
        "folder-key",
        include_str!("../../assets/icons/lucide/folder-key.svg"),
    ),
    (
        "folder-dot",
        include_str!("../../assets/icons/lucide/folder-dot.svg"),
    ),
    ("star", include_str!("../../assets/icons/lucide/star.svg")),
    ("heart", include_str!("../../assets/icons/lucide/heart.svg")),
    (
        "bookmark",
        include_str!("../../assets/icons/lucide/bookmark.svg"),
    ),
    ("flag", include_str!("../../assets/icons/lucide/flag.svg")),
    (
        "trophy",
        include_str!("../../assets/icons/lucide/trophy.svg"),
    ),
    (
        "sparkles",
        include_str!("../../assets/icons/lucide/sparkles.svg"),
    ),
    (
        "rocket",
        include_str!("../../assets/icons/lucide/rocket.svg"),
    ),
    ("zap", include_str!("../../assets/icons/lucide/zap.svg")),
    (
        "briefcase",
        include_str!("../../assets/icons/lucide/briefcase.svg"),
    ),
    (
        "wallet",
        include_str!("../../assets/icons/lucide/wallet.svg"),
    ),
    (
        "receipt",
        include_str!("../../assets/icons/lucide/receipt.svg"),
    ),
    (
        "shopping-cart",
        include_str!("../../assets/icons/lucide/shopping-cart.svg"),
    ),
    (
        "calendar",
        include_str!("../../assets/icons/lucide/calendar.svg"),
    ),
    ("mail", include_str!("../../assets/icons/lucide/mail.svg")),
    ("inbox", include_str!("../../assets/icons/lucide/inbox.svg")),
    (
        "book-open",
        include_str!("../../assets/icons/lucide/book-open.svg"),
    ),
    (
        "graduation-cap",
        include_str!("../../assets/icons/lucide/graduation-cap.svg"),
    ),
    (
        "file-text",
        include_str!("../../assets/icons/lucide/file-text.svg"),
    ),
    ("code", include_str!("../../assets/icons/lucide/code.svg")),
    (
        "terminal",
        include_str!("../../assets/icons/lucide/terminal.svg"),
    ),
    (
        "database",
        include_str!("../../assets/icons/lucide/database.svg"),
    ),
    (
        "server",
        include_str!("../../assets/icons/lucide/server.svg"),
    ),
    ("globe", include_str!("../../assets/icons/lucide/globe.svg")),
    ("cloud", include_str!("../../assets/icons/lucide/cloud.svg")),
    (
        "package",
        include_str!("../../assets/icons/lucide/package.svg"),
    ),
    ("box", include_str!("../../assets/icons/lucide/box.svg")),
    (
        "layers",
        include_str!("../../assets/icons/lucide/layers.svg"),
    ),
    (
        "archive",
        include_str!("../../assets/icons/lucide/archive.svg"),
    ),
    (
        "download",
        include_str!("../../assets/icons/lucide/download.svg"),
    ),
    ("music", include_str!("../../assets/icons/lucide/music.svg")),
    ("image", include_str!("../../assets/icons/lucide/image.svg")),
    (
        "camera",
        include_str!("../../assets/icons/lucide/camera.svg"),
    ),
    ("film", include_str!("../../assets/icons/lucide/film.svg")),
    ("video", include_str!("../../assets/icons/lucide/video.svg")),
    (
        "gamepad-2",
        include_str!("../../assets/icons/lucide/gamepad-2.svg"),
    ),
    (
        "palette",
        include_str!("../../assets/icons/lucide/palette.svg"),
    ),
    (
        "pen-tool",
        include_str!("../../assets/icons/lucide/pen-tool.svg"),
    ),
    (
        "lightbulb",
        include_str!("../../assets/icons/lucide/lightbulb.svg"),
    ),
    (
        "wrench",
        include_str!("../../assets/icons/lucide/wrench.svg"),
    ),
    (
        "shield",
        include_str!("../../assets/icons/lucide/shield.svg"),
    ),
    ("lock", include_str!("../../assets/icons/lucide/lock.svg")),
    ("key", include_str!("../../assets/icons/lucide/key.svg")),
    ("house", include_str!("../../assets/icons/lucide/house.svg")),
    ("users", include_str!("../../assets/icons/lucide/users.svg")),
    ("user", include_str!("../../assets/icons/lucide/user.svg")),
    ("leaf", include_str!("../../assets/icons/lucide/leaf.svg")),
    (
        "coffee",
        include_str!("../../assets/icons/lucide/coffee.svg"),
    ),
    ("plane", include_str!("../../assets/icons/lucide/plane.svg")),
    ("car", include_str!("../../assets/icons/lucide/car.svg")),
    (
        "dumbbell",
        include_str!("../../assets/icons/lucide/dumbbell.svg"),
    ),
    (
        "trash-2",
        include_str!("../../assets/icons/lucide/trash-2.svg"),
    ),
];

/// Draws one of [`FOLDER_ICONS`] by name; unknown names draw the plain
/// folder.
pub fn paint_named(painter: &Painter, rect: Rect, name: &str, size: f32, color: Color32) {
    paint_named_stroked(painter, rect, name, size, 2.0 * size / 24.0, color);
}

/// Like [`paint_named`], with lines `width` points wide.
pub fn paint_named_stroked(
    painter: &Painter,
    rect: Rect,
    name: &str,
    size: f32,
    width: f32,
    color: Color32,
) {
    let (name, svg) = FOLDER_ICONS
        .iter()
        .find(|(n, _)| *n == name)
        .copied()
        .unwrap_or(FOLDER_ICONS[0]);
    let target = Rect::from_center_size(rect.center(), vec2(size, size));
    paint_svg(
        painter,
        target,
        IconKey::Named(name),
        svg,
        size,
        width,
        color,
    );
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
enum IconKey {
    Fixed(Icon),
    Named(&'static str),
}

/// Paints `svg` into `target`, from a texture made once per size and
/// stroke.
fn paint_svg(
    painter: &Painter,
    target: Rect,
    key: IconKey,
    svg: &str,
    size: f32,
    width: f32,
    color: Color32,
) {
    let ppp = painter.ctx().pixels_per_point();
    let pixels = (size * ppp).round().max(1.0) as u32;
    // In twentieths of a viewBox unit, so near-equal sizes share a texture.
    let stroke = ((width * 24.0 / size) * 20.0).round() as u32;

    type Cache = HashMap<(IconKey, u32, u32), Option<TextureHandle>>;
    static CACHE: OnceLock<Mutex<Cache>> = OnceLock::new();
    let mut cache = CACHE
        .get_or_init(Default::default)
        .lock()
        .unwrap_or_else(|e| e.into_inner());
    let texture = cache.entry((key, pixels, stroke)).or_insert_with(|| {
        let image = rasterize(svg, pixels, stroke as f32 / 20.0)?;
        Some(painter.ctx().load_texture(
            format!("icon:{key:?}:{pixels}:{stroke}"),
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

    #[test]
    fn every_folder_icon_renders() {
        for (name, svg) in FOLDER_ICONS {
            let image = rasterize(svg, 24, 2.0).unwrap_or_else(|| panic!("{name} didn't render"));
            assert!(
                image.pixels.iter().any(|p| p.a() > 0),
                "{name} came out empty"
            );
        }
        assert_eq!(FOLDER_ICONS[0].0, "folder");
    }
}
