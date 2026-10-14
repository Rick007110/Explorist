//! Folder sizes for the Size column, worked out in the background as fast
//! as the disk allows.
//!
//! Every folder in view is a "root". Worker threads (one per core, up to
//! eight) share the work of walking all of them: each subfolder found is a
//! task any idle worker can take, so one huge folder is spread over every
//! core too. Tasks are handed out round-robin across the roots, so small
//! folders finish quickly instead of waiting behind a big one.
//!
//! Links aren't followed, so nothing is counted twice through a link and
//! loops are impossible. Sizes are the files' lengths, like Explorer's
//! "Size" (not the space they take on disk).

use std::collections::{HashMap, VecDeque};
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, AtomicU64, AtomicUsize, Ordering};
use std::sync::{Arc, Condvar, Mutex};
use std::thread;
use std::time::{Duration, Instant};

const MAX_WORKERS: usize = 8;
/// How often running totals are pushed to the screen at most.
const WAKE_INTERVAL: Duration = Duration::from_millis(120);

/// The tasks waiting: per-root stacks of folders still to read, and the
/// roots that have any, in turn order.
struct Queue {
    stacks: Vec<Vec<PathBuf>>,
    turns: VecDeque<usize>,
    in_turns: Vec<bool>,
    /// Workers reading a folder right now.
    busy: usize,
    stop: bool,
}

impl Queue {
    fn push(&mut self, root: usize, dir: PathBuf) {
        self.stacks[root].push(dir);
        if !self.in_turns[root] {
            self.in_turns[root] = true;
            self.turns.push_back(root);
        }
    }

    /// The next folder to read, taking roots in turn.
    fn pop(&mut self) -> Option<(usize, PathBuf)> {
        let root = self.turns.pop_front()?;
        let dir = self.stacks[root].pop()?;
        if self.stacks[root].is_empty() {
            self.in_turns[root] = false;
        } else {
            self.turns.push_back(root);
        }
        Some((root, dir))
    }
}

struct Shared {
    queue: Mutex<Queue>,
    ready: Condvar,
    /// Bytes found so far, per root.
    bytes: Vec<AtomicU64>,
    /// Folders queued or being read, per root; zero means finished.
    pending: Vec<AtomicUsize>,
    /// Set when anything changed since the screen last looked.
    changed: AtomicBool,
    stopped: AtomicBool,
}

/// Sizes of a set of folders, being worked out.
pub struct FolderSizer {
    index: HashMap<PathBuf, usize>,
    shared: Arc<Shared>,
}

impl FolderSizer {
    /// Starts sizing `folders`. `wake` runs (at most every ~0.1 s) when
    /// there's progress to show.
    pub fn start(folders: Vec<PathBuf>, wake: impl Fn() + Send + Sync + 'static) -> Self {
        let n = folders.len();
        let mut queue = Queue {
            stacks: vec![Vec::new(); n],
            turns: VecDeque::with_capacity(n),
            in_turns: vec![false; n],
            busy: 0,
            stop: false,
        };
        let mut index = HashMap::with_capacity(n);
        for (i, folder) in folders.into_iter().enumerate() {
            index.insert(folder.clone(), i);
            queue.push(i, folder);
        }
        let shared = Arc::new(Shared {
            queue: Mutex::new(queue),
            ready: Condvar::new(),
            bytes: (0..n).map(|_| AtomicU64::new(0)).collect(),
            pending: (0..n).map(|_| AtomicUsize::new(1)).collect(),
            changed: AtomicBool::new(false),
            stopped: AtomicBool::new(false),
        });
        let workers = thread::available_parallelism()
            .map_or(4, |n| n.get())
            .clamp(1, MAX_WORKERS);
        let workers = if n == 0 { 0 } else { workers };
        let wake: Arc<dyn Fn() + Send + Sync> = Arc::new(wake);
        let last_wake = Arc::new(Mutex::new(Instant::now()));
        for i in 0..workers {
            let shared = Arc::clone(&shared);
            let wake = Arc::clone(&wake);
            let last_wake = Arc::clone(&last_wake);
            thread::Builder::new()
                .name(format!("explorist-sizes-{i}"))
                .spawn(move || work(&shared, &*wake, &last_wake))
                .ok();
        }
        Self { index, shared }
    }

    /// Bytes counted so far for `folder`, and whether it's finished.
    pub fn get(&self, folder: &Path) -> Option<(u64, bool)> {
        let &i = self.index.get(folder)?;
        Some((
            self.shared.bytes[i].load(Ordering::Relaxed),
            self.shared.pending[i].load(Ordering::Acquire) == 0,
        ))
    }

    /// Whether every folder is finished.
    pub fn done(&self) -> bool {
        self.shared
            .pending
            .iter()
            .all(|p| p.load(Ordering::Acquire) == 0)
    }

    /// True once after new totals arrive.
    pub fn take_changed(&self) -> bool {
        self.shared.changed.swap(false, Ordering::AcqRel)
    }

    /// Finished folders and their sizes.
    pub fn finished(&self) -> impl Iterator<Item = (&Path, u64)> {
        self.index
            .iter()
            .filter(|&(_, &i)| self.shared.pending[i].load(Ordering::Acquire) == 0)
            .map(|(path, &i)| (path.as_path(), self.shared.bytes[i].load(Ordering::Relaxed)))
    }
}

impl Drop for FolderSizer {
    fn drop(&mut self) {
        self.shared.stopped.store(true, Ordering::Relaxed);
        let mut queue = self.shared.queue.lock().unwrap_or_else(|e| e.into_inner());
        queue.stop = true;
        self.shared.ready.notify_all();
    }
}

fn work(shared: &Shared, wake: &(dyn Fn() + Send + Sync), last_wake: &Mutex<Instant>) {
    let mut found: Vec<PathBuf> = Vec::new();
    loop {
        // Take a folder, or finish when nothing is left and nobody can add more.
        let (root, dir) = {
            let mut queue = shared.queue.lock().unwrap_or_else(|e| e.into_inner());
            loop {
                if queue.stop {
                    return;
                }
                if let Some(task) = queue.pop() {
                    queue.busy += 1;
                    break task;
                }
                if queue.busy == 0 {
                    // All done: let the other idle workers go too.
                    queue.stop = true;
                    shared.ready.notify_all();
                    return;
                }
                queue = shared.ready.wait(queue).unwrap_or_else(|e| e.into_inner());
            }
        };

        // Read it without holding the lock.
        let mut bytes = 0u64;
        found.clear();
        if let Ok(read) = fs::read_dir(&dir) {
            for item in read.flatten() {
                if shared.stopped.load(Ordering::Relaxed) {
                    break;
                }
                let Ok(kind) = item.file_type() else { continue };
                if kind.is_dir() {
                    found.push(item.path());
                } else if kind.is_file() {
                    // On Windows this comes with the listing; elsewhere it's
                    // one stat per file.
                    bytes += item.metadata().map_or(0, |m| m.len());
                }
            }
        }
        shared.bytes[root].fetch_add(bytes, Ordering::Relaxed);

        let finished_root = {
            let mut queue = shared.queue.lock().unwrap_or_else(|e| e.into_inner());
            shared.pending[root].fetch_add(found.len(), Ordering::AcqRel);
            for sub in found.drain(..) {
                queue.push(root, sub);
            }
            queue.busy -= 1;
            if !queue.turns.is_empty() || queue.busy == 0 {
                shared.ready.notify_all();
            }
            shared.pending[root].fetch_sub(1, Ordering::AcqRel) == 1
        };

        shared.changed.store(true, Ordering::Release);
        let mut last = last_wake.lock().unwrap_or_else(|e| e.into_inner());
        if finished_root || last.elapsed() >= WAKE_INTERVAL {
            *last = Instant::now();
            drop(last);
            wake();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn wait(sizer: &FolderSizer) {
        let start = Instant::now();
        while !sizer.done() && start.elapsed() < Duration::from_secs(20) {
            thread::sleep(Duration::from_millis(5));
        }
        assert!(sizer.done(), "sizing didn't finish");
    }

    #[test]
    fn sizes_nested_folders() {
        let dir = tempfile::tempdir().unwrap();
        let a = dir.path().join("a");
        let b = dir.path().join("b");
        let empty = dir.path().join("empty");
        fs::create_dir_all(a.join("x/y/z")).unwrap();
        fs::create_dir_all(&b).unwrap();
        fs::create_dir_all(&empty).unwrap();
        fs::write(a.join("one"), vec![0u8; 100]).unwrap();
        fs::write(a.join("x/two"), vec![0u8; 20]).unwrap();
        fs::write(a.join("x/y/z/three"), vec![0u8; 3]).unwrap();
        fs::write(b.join("four"), vec![0u8; 4000]).unwrap();

        let sizer = FolderSizer::start(vec![a.clone(), b.clone(), empty.clone()], || {});
        wait(&sizer);
        assert_eq!(sizer.get(&a), Some((123, true)));
        assert_eq!(sizer.get(&b), Some((4000, true)));
        assert_eq!(sizer.get(&empty), Some((0, true)));
        assert_eq!(sizer.get(&dir.path().join("nope")), None);
        assert_eq!(sizer.finished().count(), 3);
    }

    #[test]
    fn a_wide_tree_adds_up() {
        // Many folders, to give every worker something to do at once.
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path().join("root");
        let mut expected = 0u64;
        for i in 0..40 {
            let sub = root.join(format!("d{i}")).join("deeper");
            fs::create_dir_all(&sub).unwrap();
            for j in 0..5 {
                let len = (i * 7 + j) as usize;
                fs::write(sub.join(format!("f{j}")), vec![1u8; len]).unwrap();
                expected += len as u64;
            }
        }
        let sizer = FolderSizer::start(vec![root.clone()], || {});
        wait(&sizer);
        assert_eq!(sizer.get(&root), Some((expected, true)));
    }

    #[cfg(unix)]
    #[test]
    fn links_are_not_followed() {
        let dir = tempfile::tempdir().unwrap();
        let a = dir.path().join("a");
        fs::create_dir_all(&a).unwrap();
        fs::write(a.join("file"), vec![0u8; 10]).unwrap();
        // A link back up would loop forever if followed.
        std::os::unix::fs::symlink(dir.path(), a.join("loop")).unwrap();
        let sizer = FolderSizer::start(vec![a.clone()], || {});
        wait(&sizer);
        assert_eq!(sizer.get(&a), Some((10, true)));
    }

    #[test]
    fn dropping_stops_the_workers() {
        let dir = tempfile::tempdir().unwrap();
        let sizer = FolderSizer::start(vec![dir.path().to_path_buf()], || {});
        drop(sizer);
    }

    /// Timing on a real tree: `EXPLORIST_SIZE_BENCH=/usr cargo test --release
    /// size_bench -- --ignored --nocapture`.
    #[test]
    #[ignore]
    fn size_bench() {
        let Ok(root) = std::env::var("EXPLORIST_SIZE_BENCH") else {
            return;
        };
        let folders: Vec<PathBuf> = fs::read_dir(&root)
            .unwrap()
            .flatten()
            .filter(|e| e.file_type().is_ok_and(|t| t.is_dir()))
            .map(|e| e.path())
            .collect();
        let start = Instant::now();
        let sizer = FolderSizer::start(folders.clone(), || {});
        while !sizer.done() {
            thread::sleep(Duration::from_millis(1));
        }
        let elapsed = start.elapsed();
        let mut total = 0;
        for f in &folders {
            let (bytes, _) = sizer.get(f).unwrap();
            total += bytes;
            println!("{bytes:>14}  {}", f.display());
        }
        println!("{} folders, {total} bytes in {elapsed:?}", folders.len());
    }
}
