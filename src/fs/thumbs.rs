//! Small image thumbnails for the grid, decoded off the UI thread.
//!
//! Requests go on a stack, so the tiles asked for last (the ones on screen
//! now) are decoded first when scrolling quickly.
//!
//! Finished thumbnails are also saved to disk (see [`DiskCache`]), so a
//! folder seen before shows its thumbnails at once, even after a restart.

use std::collections::HashSet;
use std::fs;
use std::io;
use std::path::{Path, PathBuf};
use std::sync::mpsc::{self, Receiver, Sender};
use std::sync::{Arc, Condvar, Mutex};
use std::thread;
use std::time::{SystemTime, UNIX_EPOCH};

use crate::fs::kind::FileKind;

/// Thumbnails fit this square.
pub const SIDE: u32 = 128;
/// Don't decode images larger than this.
const MAX_BYTES: u64 = 64 * 1024 * 1024;
/// Workers making thumbnails; saved ones are read in a millisecond, so a
/// few slow decodes shouldn't hold them up.
fn workers() -> usize {
    thread::available_parallelism().map_or(2, |n| (n.get() / 2).clamp(2, 4))
}
/// Older requests than this are dropped; they've scrolled away.
const MAX_QUEUED: usize = 256;

/// A decoded thumbnail: RGBA pixels and their size.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Thumb {
    pub size: [usize; 2],
    pub pixels: Vec<u8>,
}

/// Whether files of this kind get thumbnails.
pub fn shows_thumbnail(kind: FileKind) -> bool {
    matches!(kind, FileKind::Image | FileKind::Video)
}

fn kind_of(path: &Path) -> FileKind {
    let name = path
        .file_name()
        .map(|n| n.to_string_lossy())
        .unwrap_or_default();
    crate::fs::kind::classify(&name, false).0
}

/// Makes `path`'s thumbnail, scaled to fit [`SIDE`]: images decoded here,
/// videos (and images we can't read, like HEIC) from the system. `None`
/// for files with no picture to show.
pub fn make(path: &Path) -> Option<Thumb> {
    let image = if kind_of(path) == FileKind::Video {
        super::system_thumbs::thumbnail(path, SIDE)?
    } else {
        let ours = (fs::metadata(path).ok()?.len() <= MAX_BYTES)
            .then(|| image::open(path).ok())
            .flatten()
            .map(|i| i.to_rgba8());
        match ours {
            Some(image) => image,
            None => super::system_thumbs::thumbnail(path, SIDE)?,
        }
    };
    let image = if image.width() > SIDE || image.height() > SIDE {
        image::DynamicImage::ImageRgba8(image)
            .thumbnail(SIDE, SIDE)
            .to_rgba8()
    } else {
        image
    };
    Some(Thumb {
        size: [image.width() as usize, image.height() as usize],
        pixels: image.into_raw(),
    })
}

/// Whether a missing thumbnail is worth remembering: not for a video when
/// there's nothing here that reads videos yet (installing ffmpeg on Linux
/// should bring them).
fn remember_missing(path: &Path) -> bool {
    kind_of(path) != FileKind::Video || super::system_thumbs::available()
}

/// Thumbnails saved on disk, one small PNG each, named after a hash of the
/// file's path, size and modified time: an edited image gets a new name,
/// and its old thumbnail is cleared out with the oldest ones once the
/// cache is over [`DiskCache::MAX_BYTES`]. Files that have no thumbnail
/// are remembered as empty files, so they aren't tried again.
#[derive(Debug, Clone)]
pub struct DiskCache {
    dir: PathBuf,
}

impl DiskCache {
    /// Bigger than this, the least recently used thumbnails go.
    pub const MAX_BYTES: u64 = 500 * 1024 * 1024;
    /// Cleaning stops at this size, so it doesn't run every time.
    const TRIM_TO: u64 = 400 * 1024 * 1024;

    pub fn new(dir: PathBuf) -> Self {
        Self { dir }
    }

    /// `Explorist/thumbnails` in the user's cache folder.
    pub fn default_dir() -> Option<PathBuf> {
        dirs::cache_dir().map(|d| d.join("Explorist").join("thumbnails"))
    }

    fn file_for(&self, path: &Path, meta: &fs::Metadata) -> PathBuf {
        let modified = meta
            .modified()
            .ok()
            .and_then(|t| t.duration_since(UNIX_EPOCH).ok())
            .map_or(0, |d| d.as_nanos());
        let key = fnv1a(
            format!(
                "{}|{}|{modified}|{SIDE}",
                path.to_string_lossy(),
                meta.len()
            )
            .as_bytes(),
        );
        let name = format!("{key:016x}");
        self.dir.join(&name[..2]).join(format!("{name}.png"))
    }

    /// The saved thumbnail: `Some(None)` when the file is known to have
    /// none, `None` when it isn't saved.
    pub fn get(&self, path: &Path, meta: &fs::Metadata) -> Option<Option<Thumb>> {
        let file = self.file_for(path, meta);
        let data = fs::read(&file).ok()?;
        // Mark it used, for trimming. Only once a day, to save writes.
        let stale = fs::metadata(&file)
            .and_then(|m| m.modified())
            .ok()
            .and_then(|t| t.elapsed().ok())
            .is_some_and(|age| age.as_secs() > 24 * 60 * 60);
        if stale && let Ok(f) = fs::File::options().write(true).open(&file) {
            let _ = f.set_modified(SystemTime::now());
        }
        if data.is_empty() {
            return Some(None);
        }
        let image = image::load_from_memory_with_format(&data, image::ImageFormat::Png)
            .ok()?
            .to_rgba8();
        Some(Some(Thumb {
            size: [image.width() as usize, image.height() as usize],
            pixels: image.into_raw(),
        }))
    }

    /// Saves `thumb` (or that there is none) for `path`.
    pub fn put(&self, path: &Path, meta: &fs::Metadata, thumb: Option<&Thumb>) -> io::Result<()> {
        let file = self.file_for(path, meta);
        if let Some(dir) = file.parent() {
            fs::create_dir_all(dir)?;
        }
        let mut data = Vec::new();
        if let Some(thumb) = thumb {
            use image::ImageEncoder;
            image::codecs::png::PngEncoder::new_with_quality(
                &mut data,
                image::codecs::png::CompressionType::Fast,
                image::codecs::png::FilterType::Adaptive,
            )
            .write_image(
                &thumb.pixels,
                thumb.size[0] as u32,
                thumb.size[1] as u32,
                image::ExtendedColorType::Rgba8,
            )
            .map_err(io::Error::other)?;
        }
        // Write next to it and rename, so a reader never sees half a file.
        let tmp = file.with_extension(format!("tmp{}", std::process::id()));
        fs::write(&tmp, data)?;
        fs::rename(&tmp, &file)
    }

    /// Every saved thumbnail, with its size and last use.
    fn files(&self) -> Vec<(PathBuf, u64, SystemTime)> {
        let Ok(dirs) = fs::read_dir(&self.dir) else {
            return Vec::new();
        };
        dirs.flatten()
            .filter_map(|d| fs::read_dir(d.path()).ok())
            .flat_map(|files| files.flatten())
            .filter_map(|f| {
                let meta = f.metadata().ok()?;
                Some((f.path(), meta.len(), meta.modified().ok()?))
            })
            .collect()
    }

    /// How much space the saved thumbnails take.
    pub fn size(&self) -> u64 {
        self.files().iter().map(|(_, len, _)| len).sum()
    }

    /// Deletes the least recently used thumbnails while over `max` bytes.
    pub fn trim(&self, max: u64, to: u64) {
        let mut files = self.files();
        let mut total: u64 = files.iter().map(|(_, len, _)| len).sum();
        if total <= max {
            return;
        }
        files.sort_by_key(|(_, _, used)| *used);
        for (path, len, _) in files {
            if total <= to {
                break;
            }
            if fs::remove_file(&path).is_ok() {
                total -= len;
            }
        }
    }

    /// Deletes every saved thumbnail.
    pub fn clear(&self) -> io::Result<()> {
        match fs::remove_dir_all(&self.dir) {
            Err(err) if err.kind() != io::ErrorKind::NotFound => Err(err),
            _ => Ok(()),
        }
    }
}

/// FNV-1a: a small hash that stays the same between versions, unlike
/// std's, so saved names keep matching after an update.
fn fnv1a(bytes: &[u8]) -> u64 {
    bytes.iter().fold(0xcbf2_9ce4_8422_2325, |hash, b| {
        (hash ^ u64::from(*b)).wrapping_mul(0x0000_0100_0000_01b3)
    })
}

/// A thumbnail from the disk cache, or made and then saved there.
fn cached_or_make(cache: Option<&DiskCache>, path: &Path) -> Option<Thumb> {
    let Some(cache) = cache else {
        return make(path);
    };
    let Ok(meta) = fs::metadata(path) else {
        return None;
    };
    if let Some(saved) = cache.get(path, &meta) {
        return saved;
    }
    let thumb = make(path);
    if thumb.is_some() || remember_missing(path) {
        let _ = cache.put(path, &meta, thumb.as_ref());
    }
    thumb
}

#[derive(Default)]
struct Queue {
    stack: Vec<PathBuf>,
    closed: bool,
}

type Shared = Arc<(Mutex<Queue>, Condvar)>;

/// A pool of workers making thumbnails.
pub struct Thumbnailer {
    queue: Shared,
    results: Receiver<(PathBuf, Option<Thumb>)>,
    /// Asked for and not yet received.
    waiting: HashSet<PathBuf>,
}

impl Thumbnailer {
    /// Starts the workers, saving thumbnails in `cache` if given. `wake`
    /// runs whenever a thumbnail is ready.
    pub fn new(cache: Option<DiskCache>, wake: impl Fn() + Send + Sync + 'static) -> Self {
        let queue: Shared = Arc::default();
        let (tx, results) = mpsc::channel();
        let wake = Arc::new(wake);
        if let Some(cache) = cache.clone() {
            // Keep the cache in bounds, out of the way.
            thread::Builder::new()
                .name("explorist-thumbs-trim".into())
                .spawn(move || cache.trim(DiskCache::MAX_BYTES, DiskCache::TRIM_TO))
                .ok();
        }
        for i in 0..workers() {
            let queue = Arc::clone(&queue);
            let tx: Sender<_> = tx.clone();
            let wake = Arc::clone(&wake);
            let cache = cache.clone();
            thread::Builder::new()
                .name(format!("explorist-thumbs-{i}"))
                .spawn(move || {
                    while let Some(path) = next(&queue) {
                        let thumb = cached_or_make(cache.as_ref(), &path);
                        if tx.send((path, thumb)).is_err() {
                            return;
                        }
                        wake();
                    }
                })
                .ok();
        }
        Self {
            queue,
            results,
            waiting: HashSet::new(),
        }
    }

    /// Asks for `path`'s thumbnail, unless it's already on its way.
    pub fn request(&mut self, path: &Path) {
        if !self.waiting.insert(path.to_path_buf()) {
            return;
        }
        let (lock, ready) = &*self.queue;
        let mut queue = lock.lock().unwrap_or_else(|e| e.into_inner());
        queue.stack.push(path.to_path_buf());
        if queue.stack.len() > MAX_QUEUED {
            let dropped: Vec<PathBuf> = queue.stack.drain(..MAX_QUEUED / 2).collect();
            for path in dropped {
                self.waiting.remove(&path);
            }
        }
        ready.notify_one();
    }

    /// Drops everything not started yet, e.g. after leaving the folder.
    pub fn clear_queue(&mut self) {
        let (lock, _) = &*self.queue;
        let mut queue = lock.lock().unwrap_or_else(|e| e.into_inner());
        for path in queue.stack.drain(..) {
            self.waiting.remove(&path);
        }
    }

    /// Thumbnails finished since the last call.
    pub fn poll(&mut self) -> Vec<(PathBuf, Option<Thumb>)> {
        let done: Vec<_> = self.results.try_iter().collect();
        for (path, _) in &done {
            self.waiting.remove(path);
        }
        done
    }
}

impl Drop for Thumbnailer {
    fn drop(&mut self) {
        let (lock, ready) = &*self.queue;
        lock.lock().unwrap_or_else(|e| e.into_inner()).closed = true;
        ready.notify_all();
    }
}

/// Blocks until there's work; `None` once the pool shuts down.
fn next(queue: &Shared) -> Option<PathBuf> {
    let (lock, ready) = &**queue;
    let mut queue = lock.lock().unwrap_or_else(|e| e.into_inner());
    loop {
        if queue.closed {
            return None;
        }
        if let Some(path) = queue.stack.pop() {
            return Some(path);
        }
        queue = ready.wait(queue).unwrap_or_else(|e| e.into_inner());
    }
}

#[cfg(test)]
mod tests {
    use std::time::{Duration, Instant};

    use super::*;

    fn png(path: &Path, w: u32, h: u32) {
        image::RgbaImage::from_pixel(w, h, image::Rgba([200, 40, 40, 255]))
            .save(path)
            .unwrap();
    }

    #[test]
    fn scales_to_fit_keeping_the_shape() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("wide.png");
        png(&path, 512, 256);
        let thumb = make(&path).unwrap();
        assert_eq!(thumb.size, [128, 64]);
        assert_eq!(thumb.pixels.len(), 128 * 64 * 4);
    }

    #[test]
    fn not_an_image_gives_none() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("fake.png");
        fs::write(&path, b"not a png").unwrap();
        assert_eq!(make(&path), None);
        assert_eq!(make(&dir.path().join("missing.png")), None);
    }

    #[test]
    fn workers_deliver_each_request_once() {
        let dir = tempfile::tempdir().unwrap();
        let a = dir.path().join("a.png");
        let b = dir.path().join("b.png");
        png(&a, 16, 16);
        png(&b, 300, 30);
        let mut thumbs = Thumbnailer::new(None, || {});
        thumbs.request(&a);
        thumbs.request(&a);
        thumbs.request(&b);
        let mut got = Vec::new();
        let start = Instant::now();
        while got.len() < 2 && start.elapsed() < Duration::from_secs(10) {
            got.extend(thumbs.poll());
            thread::sleep(Duration::from_millis(10));
        }
        thread::sleep(Duration::from_millis(50));
        got.extend(thumbs.poll());
        got.sort_by(|x, y| x.0.cmp(&y.0));
        assert_eq!(got.len(), 2);
        assert_eq!(got[0].1.as_ref().unwrap().size, [16, 16]);
        assert_eq!(got[1].1.as_ref().unwrap().size, [128, 13]);
    }

    #[test]
    fn saved_thumbnails_come_back_and_follow_edits() {
        let dir = tempfile::tempdir().unwrap();
        let cache = DiskCache::new(dir.path().join("cache"));
        let image = dir.path().join("photo.png");
        png(&image, 400, 200);
        let made = cached_or_make(Some(&cache), &image).unwrap();
        assert_eq!(made.size, [128, 64]);
        let meta = fs::metadata(&image).unwrap();
        assert_eq!(cache.get(&image, &meta), Some(Some(made)));
        assert!(cache.size() > 0);

        // An edited file is a different entry.
        thread::sleep(Duration::from_millis(20));
        png(&image, 100, 300);
        let meta = fs::metadata(&image).unwrap();
        assert_eq!(cache.get(&image, &meta), None);
        assert_eq!(
            cached_or_make(Some(&cache), &image).unwrap().size,
            [43, 128]
        );

        // Files that aren't images are remembered as having none.
        let fake = dir.path().join("fake.png");
        fs::write(&fake, b"nope").unwrap();
        assert_eq!(cached_or_make(Some(&cache), &fake), None);
        assert_eq!(cache.get(&fake, &fs::metadata(&fake).unwrap()), Some(None));

        cache.clear().unwrap();
        assert_eq!(cache.size(), 0);
        cache.clear().unwrap();
    }

    /// Needs ffmpeg to make the test video and read it back, so it's
    /// skipped where there's none.
    #[test]
    fn videos_get_a_frame() {
        if !crate::fs::system_thumbs::available() {
            return;
        }
        let dir = tempfile::tempdir().unwrap();
        let video = dir.path().join("clip.mp4");
        let made = std::process::Command::new("ffmpeg")
            .args([
                "-v",
                "error",
                "-f",
                "lavfi",
                "-i",
                "testsrc=size=640x360:rate=10",
            ])
            .args(["-t", "1", "-pix_fmt", "yuv420p"])
            .arg(&video)
            .status();
        if !made.is_ok_and(|s| s.success()) {
            return;
        }
        let thumb = make(&video).unwrap();
        assert_eq!(thumb.size, [128, 72]);
        assert!(shows_thumbnail(kind_of(&video)));
    }

    #[test]
    fn trimming_drops_the_least_recently_used() {
        let dir = tempfile::tempdir().unwrap();
        let cache = DiskCache::new(dir.path().to_path_buf());
        fs::create_dir_all(dir.path().join("ab")).unwrap();
        let old = dir.path().join("ab").join("old.png");
        let new = dir.path().join("ab").join("new.png");
        fs::write(&old, vec![0; 1000]).unwrap();
        fs::write(&new, vec![0; 1000]).unwrap();
        let long_ago = SystemTime::now() - Duration::from_secs(3600);
        fs::File::options()
            .write(true)
            .open(&old)
            .unwrap()
            .set_modified(long_ago)
            .unwrap();
        cache.trim(3000, 1500);
        assert!(old.exists() && new.exists());
        cache.trim(1500, 1000);
        assert!(!old.exists() && new.exists());
    }
}
