//! Loading what the preview panel shows, off the UI thread.

use std::fs;
use std::io::Read;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::mpsc::{self, Receiver};
use std::thread;

use super::entry::Entry;
use super::kind::FileKind;
use super::sort::natural_cmp;

/// Read at most this much of a text file.
const TEXT_BYTES: usize = 64 * 1024;
/// Show at most this many lines.
const TEXT_LINES: usize = 400;
/// Images are scaled down to fit this square.
const IMAGE_SIDE: u32 = 640;
/// Don't decode images larger than this.
const IMAGE_BYTES: u64 = 64 * 1024 * 1024;
/// List at most this many of a folder's items.
const FOLDER_ITEMS: usize = 500;
/// Read the details of at most this many items to pick them from.
const FOLDER_SCAN: usize = 20_000;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Content {
    Text {
        lines: Vec<String>,
        /// More lines or bytes were left out.
        truncated: bool,
    },
    Image {
        /// Original size.
        width: u32,
        height: u32,
        /// Scaled RGBA pixels and their size.
        pixels: Vec<u8>,
        scaled: [usize; 2],
    },
    Folder {
        /// How many items it holds.
        items: usize,
        /// Its first items, folders first and then by name. Shared, as
        /// the views copy the content every frame.
        children: Arc<Vec<Entry>>,
    },
    /// Nothing to show beyond the details.
    None,
    Error(String),
}

/// Works out what to show for `path`.
pub fn load(path: &Path, kind: FileKind, is_dir: bool) -> Content {
    load_unless(path, kind, is_dir, &|| false)
}

/// [`load`], giving up on a big folder once `stale` says nobody wants it
/// any more.
fn load_unless(path: &Path, kind: FileKind, is_dir: bool, stale: &dyn Fn() -> bool) -> Content {
    if is_dir {
        return match fs::read_dir(path) {
            Ok(dir) => load_folder(dir, stale),
            Err(err) => Content::Error(err.to_string()),
        };
    }
    match kind {
        FileKind::Image => load_image(path),
        FileKind::Code | FileKind::Text | FileKind::Config | FileKind::Lock | FileKind::Other => {
            load_text(path)
        }
        _ => Content::None,
    }
}

fn load_folder(dir: fs::ReadDir, stale: &dyn Fn() -> bool) -> Content {
    let mut items = 0;
    let mut children = Vec::new();
    for entry in dir.flatten() {
        items += 1;
        if items % 256 == 0 && stale() {
            return Content::None;
        }
        if children.len() < FOLDER_SCAN
            && let Ok(entry) = Entry::from_dir_entry(&entry)
        {
            children.push(entry);
        }
    }
    children.sort_by(|a, b| {
        b.is_dir
            .cmp(&a.is_dir)
            .then_with(|| natural_cmp(&a.name, &b.name))
    });
    children.truncate(FOLDER_ITEMS);
    Content::Folder {
        items,
        children: Arc::new(children),
    }
}

fn load_text(path: &Path) -> Content {
    let mut buf = Vec::with_capacity(TEXT_BYTES);
    let read =
        fs::File::open(path).and_then(|f| f.take(TEXT_BYTES as u64 + 1).read_to_end(&mut buf));
    if let Err(err) = read {
        return Content::Error(err.to_string());
    }
    let mut truncated = buf.len() > TEXT_BYTES;
    buf.truncate(TEXT_BYTES);
    // NUL bytes mean binary; don't show garbage.
    if buf.contains(&0) {
        return Content::None;
    }
    let text = match String::from_utf8(buf) {
        Ok(text) => text,
        Err(err) => {
            // A cut in the middle of a character is fine; real non-UTF-8 isn't.
            let valid = err.utf8_error().valid_up_to();
            if !truncated || valid + 4 < err.as_bytes().len() {
                return Content::None;
            }
            String::from_utf8_lossy(&err.as_bytes()[..valid]).into_owned()
        }
    };
    let mut lines: Vec<String> = text.lines().map(|l| l.replace('\t', "    ")).collect();
    if lines.len() > TEXT_LINES {
        lines.truncate(TEXT_LINES);
        truncated = true;
    }
    Content::Text { lines, truncated }
}

fn load_image(path: &Path) -> Content {
    match fs::metadata(path) {
        Ok(meta) if meta.len() > IMAGE_BYTES => return Content::None,
        Err(err) => return Content::Error(err.to_string()),
        _ => {}
    }
    let image = match image::open(path) {
        Ok(image) => image,
        // Formats we don't decode (HEIC, PSD...) just show details.
        Err(image::ImageError::Unsupported(_)) => return Content::None,
        Err(err) => return Content::Error(err.to_string()),
    };
    let (width, height) = (image.width(), image.height());
    let small = if width > IMAGE_SIDE || height > IMAGE_SIDE {
        image.thumbnail(IMAGE_SIDE, IMAGE_SIDE)
    } else {
        image
    };
    let rgba = small.to_rgba8();
    Content::Image {
        width,
        height,
        scaled: [rgba.width() as usize, rgba.height() as usize],
        pixels: rgba.into_raw(),
    }
}

/// Loads previews in the background. Only the latest request matters:
/// moving through the list quickly never queues up work.
#[derive(Default)]
pub struct Loader {
    /// Counts requests, so a load can tell it's been replaced.
    generation: Arc<AtomicU64>,
    current: Option<PathBuf>,
    result: Option<(PathBuf, Content)>,
    pending: Option<Receiver<(PathBuf, Content)>>,
}

impl Loader {
    /// Asks for `path`'s preview, unless it's already loaded or loading.
    pub fn request(
        &mut self,
        path: &Path,
        kind: FileKind,
        is_dir: bool,
        wake: impl Fn() + Send + 'static,
    ) {
        if self.current.as_deref() == Some(path) {
            return;
        }
        self.current = Some(path.to_path_buf());
        self.result = None;
        let (tx, rx) = mpsc::channel();
        let path = path.to_path_buf();
        let generation = Arc::clone(&self.generation);
        let mine = generation.fetch_add(1, Ordering::Relaxed) + 1;
        thread::Builder::new()
            .name("explorist-preview".into())
            .spawn(move || {
                let stale = || generation.load(Ordering::Relaxed) != mine;
                let content = load_unless(&path, kind, is_dir, &stale);
                if tx.send((path, content)).is_ok() {
                    wake();
                }
            })
            .ok();
        self.pending = Some(rx);
    }

    /// Forgets the current preview, e.g. when nothing is selected.
    pub fn clear(&mut self) {
        self.generation.fetch_add(1, Ordering::Relaxed);
        self.current = None;
        self.result = None;
        self.pending = None;
    }

    /// The loaded content for the current path, once ready.
    pub fn poll(&mut self) -> Option<&Content> {
        if let Some(rx) = &self.pending
            && let Ok((path, content)) = rx.try_recv()
        {
            self.pending = None;
            if self.current.as_deref() == Some(path.as_path()) {
                self.result = Some((path, content));
            }
        }
        self.result.as_ref().map(|(_, c)| c)
    }

    /// The path whose preview is shown or loading.
    pub fn current(&self) -> Option<&Path> {
        self.current.as_deref()
    }
}

#[cfg(test)]
mod tests {
    use std::time::{Duration, Instant};

    use super::*;

    #[test]
    fn text_files_show_lines() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("Cargo.toml");
        fs::write(&path, "[package]\n\tname = \"atlas\"\n").unwrap();
        assert_eq!(
            load(&path, FileKind::Config, false),
            Content::Text {
                lines: vec!["[package]".into(), "    name = \"atlas\"".into()],
                truncated: false
            }
        );
    }

    #[test]
    fn long_text_is_cut() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("big.log");
        let text: String = (0..1000).map(|i| format!("line {i}\n")).collect();
        fs::write(&path, text).unwrap();
        let Content::Text { lines, truncated } = load(&path, FileKind::Text, false) else {
            panic!("expected text");
        };
        assert_eq!(lines.len(), TEXT_LINES);
        assert!(truncated);
    }

    #[test]
    fn a_cut_multibyte_character_is_fine() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("été.txt");
        let mut text = "a".repeat(TEXT_BYTES - 1);
        text.push('é'); // two bytes, straddling the limit
        text.push_str("tail");
        fs::write(&path, text).unwrap();
        assert!(matches!(
            load(&path, FileKind::Text, false),
            Content::Text {
                truncated: true,
                ..
            }
        ));
    }

    #[test]
    fn binary_files_show_nothing() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("blob.bin");
        fs::write(&path, [0u8, 159, 146, 150]).unwrap();
        assert_eq!(load(&path, FileKind::Other, false), Content::None);
    }

    #[test]
    fn folders_list_their_items() {
        let dir = tempfile::tempdir().unwrap();
        for name in ["file10.txt", "a.txt", "file2.txt"] {
            fs::write(dir.path().join(name), b"").unwrap();
        }
        fs::create_dir(dir.path().join("zebra")).unwrap();
        let Content::Folder { items, children } = load(dir.path(), FileKind::Folder, true) else {
            panic!("not a folder preview");
        };
        assert_eq!(items, 4);
        let names: Vec<&str> = children.iter().map(|e| e.name.as_str()).collect();
        assert_eq!(names, ["zebra", "a.txt", "file2.txt", "file10.txt"]);
    }

    #[test]
    fn replaced_folder_loads_give_up() {
        let dir = tempfile::tempdir().unwrap();
        for i in 0..300 {
            fs::write(dir.path().join(format!("f{i}")), b"").unwrap();
        }
        assert_eq!(
            load_unless(dir.path(), FileKind::Folder, true, &|| true),
            Content::None
        );
        assert!(matches!(
            load_unless(dir.path(), FileKind::Folder, true, &|| false),
            Content::Folder { items: 300, .. }
        ));
    }

    #[test]
    fn images_are_scaled_down() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("wide.png");
        image::RgbaImage::from_pixel(1280, 320, image::Rgba([10, 20, 30, 255]))
            .save(&path)
            .unwrap();
        let Content::Image {
            width,
            height,
            scaled,
            pixels,
        } = load(&path, FileKind::Image, false)
        else {
            panic!("expected an image");
        };
        assert_eq!((width, height), (1280, 320));
        assert_eq!(scaled, [640, 160]);
        assert_eq!(pixels.len(), 640 * 160 * 4);
    }

    #[test]
    fn broken_images_report_an_error() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("broken.png");
        fs::write(&path, b"not a png").unwrap();
        assert!(matches!(
            load(&path, FileKind::Image, false),
            Content::Error(_) | Content::None
        ));
    }

    #[test]
    fn loader_keeps_only_the_latest_request() {
        let dir = tempfile::tempdir().unwrap();
        let a = dir.path().join("a.txt");
        let b = dir.path().join("b.txt");
        fs::write(&a, "A").unwrap();
        fs::write(&b, "B").unwrap();
        let mut loader = Loader::default();
        loader.request(&a, FileKind::Text, false, || {});
        loader.request(&b, FileKind::Text, false, || {});
        let deadline = Instant::now() + Duration::from_secs(5);
        loop {
            if let Some(Content::Text { lines, .. }) = loader.poll() {
                assert_eq!(lines, &["B"]);
                break;
            }
            assert!(Instant::now() < deadline);
            std::thread::sleep(Duration::from_millis(5));
        }
    }
}
