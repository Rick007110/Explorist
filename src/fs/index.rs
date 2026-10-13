//! A quick list of everything under a folder, for finding files by name
//! in the command palette.

use std::collections::VecDeque;
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{self, Receiver};
use std::thread;

/// Stop after this many items, so huge trees stay quick.
pub const MAX_ITEMS: usize = 50_000;
const MAX_DEPTH: usize = 8;

/// Folders that are big, generated, or private to tools.
const SKIP: [&str; 5] = [
    "node_modules",
    "target",
    "$RECYCLE.BIN",
    "System Volume Information",
    "__pycache__",
];

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Indexed {
    /// Path relative to the indexed folder, with `/` separators and a
    /// trailing `/` for folders: "assets/brand/".
    pub relative: String,
    pub path: PathBuf,
    pub is_dir: bool,
}

/// Walks a folder tree in the background, breadth first so nearby items
/// come first. Dropping it stops the walk.
pub struct FileIndex {
    root: PathBuf,
    items: Vec<Indexed>,
    rx: Receiver<Vec<Indexed>>,
    stop: Arc<AtomicBool>,
    done: bool,
}

impl FileIndex {
    pub fn start(root: &Path, wake: impl Fn() + Send + 'static) -> Self {
        let (tx, rx) = mpsc::channel();
        let stop = Arc::new(AtomicBool::new(false));
        let walk_stop = Arc::clone(&stop);
        let walk_root = root.to_path_buf();
        thread::Builder::new()
            .name("explorist-index".into())
            .spawn(move || {
                let mut queue = VecDeque::from([(walk_root.clone(), 0usize)]);
                let mut count = 0;
                let mut batch = Vec::new();
                while let Some((dir, depth)) = queue.pop_front() {
                    if walk_stop.load(Ordering::Relaxed) {
                        return;
                    }
                    let Ok(entries) = fs::read_dir(&dir) else {
                        continue;
                    };
                    for entry in entries.flatten() {
                        let name = entry.file_name().to_string_lossy().into_owned();
                        if name.starts_with('.') {
                            continue;
                        }
                        let is_dir = entry.file_type().is_ok_and(|t| t.is_dir());
                        let path = entry.path();
                        let mut relative = path
                            .strip_prefix(&walk_root)
                            .unwrap_or(&path)
                            .to_string_lossy()
                            .replace('\\', "/");
                        if is_dir {
                            relative.push('/');
                            if depth + 1 < MAX_DEPTH && !SKIP.contains(&name.as_str()) {
                                queue.push_back((path.clone(), depth + 1));
                            }
                        }
                        batch.push(Indexed {
                            relative,
                            path,
                            is_dir,
                        });
                        count += 1;
                        if count >= MAX_ITEMS {
                            let _ = tx.send(batch);
                            wake();
                            return;
                        }
                    }
                    if batch.len() >= 1000 {
                        if tx.send(std::mem::take(&mut batch)).is_err() {
                            return;
                        }
                        wake();
                    }
                }
                let _ = tx.send(batch);
                wake();
            })
            .ok();
        Self {
            root: root.to_path_buf(),
            items: Vec::new(),
            rx,
            stop,
            done: false,
        }
    }

    pub fn root(&self) -> &Path {
        &self.root
    }

    /// Everything found so far.
    pub fn items(&mut self) -> &[Indexed] {
        loop {
            match self.rx.try_recv() {
                Ok(batch) => self.items.extend(batch),
                Err(mpsc::TryRecvError::Empty) => break,
                Err(mpsc::TryRecvError::Disconnected) => {
                    self.done = true;
                    break;
                }
            }
        }
        &self.items
    }

    pub fn is_done(&self) -> bool {
        self.done
    }
}

impl Drop for FileIndex {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::Relaxed);
    }
}

#[cfg(test)]
mod tests {
    use std::time::{Duration, Instant};

    use super::*;

    #[test]
    fn indexes_nested_items_and_skips_noise() {
        let dir = tempfile::tempdir().unwrap();
        fs::create_dir_all(dir.path().join("assets/brand")).unwrap();
        fs::write(dir.path().join("assets/brand/logo.svg"), b"").unwrap();
        fs::create_dir_all(dir.path().join("node_modules/pkg")).unwrap();
        fs::create_dir_all(dir.path().join(".git/objects")).unwrap();
        fs::write(dir.path().join("README.md"), b"").unwrap();

        let mut index = FileIndex::start(dir.path(), || {});
        let deadline = Instant::now() + Duration::from_secs(5);
        while !index.is_done() {
            index.items();
            assert!(Instant::now() < deadline);
            std::thread::sleep(Duration::from_millis(5));
        }
        let mut names: Vec<String> = index.items().iter().map(|i| i.relative.clone()).collect();
        names.sort();
        assert_eq!(
            names,
            [
                "README.md",
                "assets/",
                "assets/brand/",
                "assets/brand/logo.svg",
                "node_modules/"
            ]
        );
    }
}
