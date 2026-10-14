//! Small image thumbnails for the grid, decoded off the UI thread.
//!
//! Requests go on a stack, so the tiles asked for last (the ones on screen
//! now) are decoded first when scrolling quickly.

use std::collections::HashSet;
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::mpsc::{self, Receiver, Sender};
use std::sync::{Arc, Condvar, Mutex};
use std::thread;

/// Thumbnails fit this square.
pub const SIDE: u32 = 128;
/// Don't decode images larger than this.
const MAX_BYTES: u64 = 64 * 1024 * 1024;
const WORKERS: usize = 2;
/// Older requests than this are dropped; they've scrolled away.
const MAX_QUEUED: usize = 256;

/// A decoded thumbnail: RGBA pixels and their size.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Thumb {
    pub size: [usize; 2],
    pub pixels: Vec<u8>,
}

/// Decodes `path` and scales it to fit [`SIDE`]. `None` for files that
/// aren't images we can read.
pub fn make(path: &Path) -> Option<Thumb> {
    if fs::metadata(path).ok()?.len() > MAX_BYTES {
        return None;
    }
    let mut image = image::open(path).ok()?;
    if image.width() > SIDE || image.height() > SIDE {
        image = image.thumbnail(SIDE, SIDE);
    }
    let image = image.to_rgba8();
    Some(Thumb {
        size: [image.width() as usize, image.height() as usize],
        pixels: image.into_raw(),
    })
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
    /// Starts the workers. `wake` runs whenever a thumbnail is ready.
    pub fn new(wake: impl Fn() + Send + Sync + 'static) -> Self {
        let queue: Shared = Arc::default();
        let (tx, results) = mpsc::channel();
        let wake = Arc::new(wake);
        for i in 0..WORKERS {
            let queue = Arc::clone(&queue);
            let tx: Sender<_> = tx.clone();
            let wake = Arc::clone(&wake);
            thread::Builder::new()
                .name(format!("explorist-thumbs-{i}"))
                .spawn(move || {
                    while let Some(path) = next(&queue) {
                        let thumb = make(&path);
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
        let mut thumbs = Thumbnailer::new(|| {});
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
}
