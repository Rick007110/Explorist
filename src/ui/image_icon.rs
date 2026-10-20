//! Folder icons made from the user's own image files.
//!
//! A chosen image is copied into `folder-icons` next to the settings file,
//! so it keeps working if the original moves, and the folder's icon is
//! stored as `file:<copy's name>`. Images are drawn as they are, not tinted.

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::{Mutex, OnceLock};

use egui::{Color32, Painter, Rect, TextureHandle, pos2, vec2};

use crate::config::Settings;

/// Prefix of icons that are image files.
pub const PREFIX: &str = "file:";

/// File types offered in the picker.
pub const EXTENSIONS: &[&str] = &["png", "jpg", "jpeg", "ico", "svg", "webp", "bmp", "gif"];

/// Bigger files are refused; an icon doesn't need them.
const MAX_BYTES: u64 = 20 * 1024 * 1024;

/// Pixels images are scaled down to.
const PIXELS: u32 = 128;

/// Where the copies live.
pub fn dir() -> Option<PathBuf> {
    Settings::default_path()
        .and_then(|p| p.parent().map(Path::to_path_buf))
        .map(|d| d.join("folder-icons"))
}

/// The image file behind an `icon` value, if it is one.
pub fn file_of(icon: &str) -> Option<&str> {
    icon.strip_prefix(PREFIX)
}

/// Copies `source` into the icon folder and returns the icon value for it.
pub fn import(source: &Path) -> Result<String, String> {
    let dir = dir().ok_or("There's no settings folder to keep the image in.")?;
    import_into(source, &dir)
}

fn import_into(source: &Path, dir: &Path) -> Result<String, String> {
    let size = std::fs::metadata(source).map_err(|e| e.to_string())?.len();
    if size > MAX_BYTES {
        return Err("That image is too big for an icon (over 20 MB).".to_owned());
    }
    if load(source).is_none() {
        return Err("Explorist can't read that image.".to_owned());
    }
    std::fs::create_dir_all(dir).map_err(|e| e.to_string())?;
    let original = source
        .file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_else(|| "icon".to_owned());
    let mut name = original.clone();
    let mut n = 1;
    while dir.join(&name).exists() {
        n += 1;
        let path = Path::new(&original);
        let stem = path.file_stem().unwrap_or_default().to_string_lossy();
        name = match path.extension() {
            Some(ext) => format!("{stem} ({n}).{}", ext.to_string_lossy()),
            None => format!("{stem} ({n})"),
        };
    }
    std::fs::copy(source, dir.join(&name)).map_err(|e| e.to_string())?;
    Ok(format!("{PREFIX}{name}"))
}

/// Deletes copies no folder uses any more.
pub fn prune(settings: &Settings) {
    let Some(dir) = dir() else { return };
    prune_in(settings, &dir);
}

fn prune_in(settings: &Settings, dir: &Path) {
    let Ok(read) = std::fs::read_dir(dir) else {
        return;
    };
    let used: Vec<&str> = settings
        .folder_icons
        .values()
        .filter_map(|i| file_of(&i.icon))
        .collect();
    for item in read.flatten() {
        let name = item.file_name().to_string_lossy().into_owned();
        if !used.contains(&name.as_str()) {
            let _ = std::fs::remove_file(item.path());
            cache().lock().unwrap().remove(&name);
        }
    }
}

/// Reads an image, scaled down to fit [`PIXELS`].
fn load(path: &Path) -> Option<egui::ColorImage> {
    let is_svg = path
        .extension()
        .is_some_and(|e| e.eq_ignore_ascii_case("svg"));
    if is_svg {
        let data = std::fs::read(path).ok()?;
        let tree = resvg::usvg::Tree::from_data(&data, &resvg::usvg::Options::default()).ok()?;
        let size = tree.size();
        let scale = PIXELS as f32 / size.width().max(size.height());
        let w = ((size.width() * scale).round() as u32).max(1);
        let h = ((size.height() * scale).round() as u32).max(1);
        let mut pixmap = resvg::tiny_skia::Pixmap::new(w, h)?;
        resvg::render(
            &tree,
            resvg::tiny_skia::Transform::from_scale(scale, scale),
            &mut pixmap.as_mut(),
        );
        return Some(egui::ColorImage::from_rgba_premultiplied(
            [w as usize, h as usize],
            pixmap.data(),
        ));
    }
    let image = image::ImageReader::open(path)
        .ok()?
        .with_guessed_format()
        .ok()?
        .decode()
        .ok()?;
    let image = if image.width() > PIXELS || image.height() > PIXELS {
        image.thumbnail(PIXELS, PIXELS)
    } else {
        image
    };
    let rgba = image.to_rgba8();
    Some(egui::ColorImage::from_rgba_unmultiplied(
        [rgba.width() as usize, rgba.height() as usize],
        rgba.as_raw(),
    ))
}

/// Loaded textures by file name; `None` when the file is gone or broken.
fn cache() -> &'static Mutex<HashMap<String, Option<TextureHandle>>> {
    static CACHE: OnceLock<Mutex<HashMap<String, Option<TextureHandle>>>> = OnceLock::new();
    CACHE.get_or_init(Default::default)
}

/// Draws the image `file` fitted into a `size` square in the middle of
/// `rect`. Returns false when it can't be read, so the caller can draw
/// something else.
pub fn paint(painter: &Painter, rect: Rect, file: &str, size: f32, dim: bool) -> bool {
    let mut cache = cache().lock().unwrap();
    let texture = cache.entry(file.to_owned()).or_insert_with(|| {
        let image = load(&dir()?.join(file))?;
        Some(
            painter
                .ctx()
                .load_texture(format!("folder-icon:{file}"), image, Default::default()),
        )
    });
    let Some(texture) = texture else {
        return false;
    };
    let [w, h] = texture.size().map(|v| v as f32);
    let scale = size / w.max(h);
    let target = Rect::from_center_size(rect.center(), vec2(w * scale, h * scale));
    let tint = if dim {
        Color32::WHITE.gamma_multiply(0.5)
    } else {
        Color32::WHITE
    };
    painter.image(
        texture.id(),
        target,
        Rect::from_min_max(pos2(0.0, 0.0), pos2(1.0, 1.0)),
        tint,
    );
    true
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::FolderIcon;

    fn png(path: &Path, w: u32, h: u32) {
        image::RgbaImage::from_pixel(w, h, image::Rgba([200, 30, 30, 255]))
            .save(path)
            .unwrap();
    }

    #[test]
    fn images_are_copied_under_free_names() {
        let src = tempfile::tempdir().unwrap();
        let icons = tempfile::tempdir().unwrap();
        let file = src.path().join("cat.png");
        png(&file, 300, 200);
        assert_eq!(import_into(&file, icons.path()).unwrap(), "file:cat.png");
        assert_eq!(
            import_into(&file, icons.path()).unwrap(),
            "file:cat (2).png"
        );
        let image = load(&icons.path().join("cat (2).png")).unwrap();
        assert_eq!(image.size, [128, 85]);
    }

    #[test]
    fn small_images_keep_their_size_and_svgs_render() {
        let dir = tempfile::tempdir().unwrap();
        let small = dir.path().join("s.png");
        png(&small, 16, 16);
        assert_eq!(load(&small).unwrap().size, [16, 16]);
        let svg = dir.path().join("i.svg");
        std::fs::write(
            &svg,
            r#"<svg xmlns="http://www.w3.org/2000/svg" width="24" height="12"><rect width="24" height="12" fill="red"/></svg>"#,
        )
        .unwrap();
        assert_eq!(load(&svg).unwrap().size, [128, 64]);
    }

    #[test]
    fn non_images_are_refused() {
        let dir = tempfile::tempdir().unwrap();
        let file = dir.path().join("notes.png");
        std::fs::write(&file, b"not an image").unwrap();
        assert!(import_into(&file, &dir.path().join("icons")).is_err());
        assert!(!dir.path().join("icons").exists());
    }

    #[test]
    fn unused_copies_are_removed() {
        let icons = tempfile::tempdir().unwrap();
        std::fs::write(icons.path().join("kept.png"), b"").unwrap();
        std::fs::write(icons.path().join("old.png"), b"").unwrap();
        let mut settings = Settings::default();
        settings.set_folder_icon(
            Path::new("/music"),
            Some(FolderIcon {
                icon: "file:kept.png".to_owned(),
                color: None,
            }),
        );
        prune_in(&settings, icons.path());
        assert!(icons.path().join("kept.png").exists());
        assert!(!icons.path().join("old.png").exists());
    }
}
