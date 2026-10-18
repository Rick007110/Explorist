//! A list of every file and folder name under some roots, for searching by
//! name: one folder (with everything inside it) or the whole device.
//!
//! Built in the background by one worker per core (up to eight), sharing a
//! queue of folders to read. Names are kept compactly (one shared string
//! and a small record per item, with a link to the parent folder), so even
//! millions of items take little memory, and searching them is a quick
//! scan spread over every core.
//!
//! Matching: every word of the query must appear in the name, ignoring
//! case; no scattered letters. Exact names rank first, then names that
//! start with the query, then ones where it's a separate word, then the
//! rest; nearer and shorter names break ties.

use std::collections::{BinaryHeap, HashSet, VecDeque};
use std::fs;
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Arc, Condvar, Mutex, RwLock};
use std::thread;

const MAX_WORKERS: usize = 8;
/// Stop listing past this many items.
const MAX_ITEMS: usize = 8_000_000;
const NO_PARENT: u32 = u32::MAX;

/// Folders that are big, generated, or private to tools, and never worth
/// searching.
const SKIP_NAMES: [&str; 7] = [
    "node_modules",
    "$RECYCLE.BIN",
    "$Recycle.Bin",
    "System Volume Information",
    "__pycache__",
    "target",
    "Windows.old",
];

#[derive(Debug, Clone, Copy)]
struct Node {
    parent: u32,
    name_start: u32,
    name_len: u16,
    depth: u16,
    is_dir: bool,
}

#[derive(Default)]
struct Store {
    nodes: Vec<Node>,
    names: String,
}

impl Store {
    fn name(&self, node: &Node) -> &str {
        let start = node.name_start as usize;
        &self.names[start..start + node.name_len as usize]
    }

    fn push(&mut self, parent: u32, depth: u16, name: &str, is_dir: bool) -> Option<u32> {
        if self.nodes.len() >= MAX_ITEMS
            || name.len() > u16::MAX as usize
            || self.names.len() + name.len() > u32::MAX as usize
        {
            return None;
        }
        let id = self.nodes.len() as u32;
        self.nodes.push(Node {
            parent,
            name_start: self.names.len() as u32,
            name_len: name.len() as u16,
            depth,
            is_dir,
        });
        self.names.push_str(name);
        Some(id)
    }

    /// Rebuilds a full path by following parents up to the root.
    fn path(&self, id: u32) -> PathBuf {
        let mut parts = Vec::new();
        let mut current = id;
        while current != NO_PARENT {
            let node = &self.nodes[current as usize];
            parts.push(self.name(node));
            current = node.parent;
        }
        let mut path = PathBuf::new();
        for part in parts.into_iter().rev() {
            path.push(part);
        }
        path
    }
}

struct Walk {
    queue: VecDeque<(u32, PathBuf, u16)>,
    busy: usize,
    stop: bool,
}

struct Inner {
    store: RwLock<Store>,
    walk: Mutex<Walk>,
    ready: Condvar,
    stopped: AtomicBool,
    done: AtomicBool,
    /// Bumped as items arrive, so searches know when to run again.
    generation: AtomicU64,
    /// Folders not to go into: other roots, and system areas.
    skip_paths: HashSet<PathBuf>,
}

/// One search result.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Hit {
    pub path: PathBuf,
    /// Path below the root it was found under, with `/` separators and a
    /// trailing `/` for folders: "photos/2024/".
    pub relative: String,
    pub is_dir: bool,
    pub score: i32,
    /// Matched parts of the name, as char ranges into the name.
    pub matches: Vec<(usize, usize)>,
}

/// Names under some roots, listed in the background. Dropping it stops
/// the walk.
pub struct NameIndex {
    roots: Vec<PathBuf>,
    inner: Arc<Inner>,
}

impl NameIndex {
    /// Starts listing everything under `roots`. Anything under
    /// `skip_paths` is left out.
    pub fn start(
        roots: Vec<PathBuf>,
        skip_paths: Vec<PathBuf>,
        wake: impl Fn() + Send + Sync + 'static,
    ) -> Self {
        let mut store = Store::default();
        let mut queue = VecDeque::new();
        for root in &roots {
            let name = root.to_string_lossy();
            if let Some(id) = store.push(NO_PARENT, 0, &name, true) {
                queue.push_back((id, root.clone(), 0));
            }
        }
        let skip: HashSet<PathBuf> = skip_paths
            .into_iter()
            .chain(roots.iter().cloned())
            .collect();
        let inner = Arc::new(Inner {
            store: RwLock::new(store),
            walk: Mutex::new(Walk {
                queue,
                busy: 0,
                stop: false,
            }),
            ready: Condvar::new(),
            stopped: AtomicBool::new(false),
            done: AtomicBool::new(false),
            generation: AtomicU64::new(1),
            skip_paths: skip,
        });
        let workers = thread::available_parallelism()
            .map_or(4, |n| n.get())
            .clamp(1, MAX_WORKERS);
        let wake: Arc<dyn Fn() + Send + Sync> = Arc::new(wake);
        for i in 0..workers {
            let inner = Arc::clone(&inner);
            let wake = Arc::clone(&wake);
            thread::Builder::new()
                .name(format!("explorist-index-{i}"))
                .spawn(move || work(&inner, &*wake))
                .ok();
        }
        Self { roots, inner }
    }

    pub fn roots(&self) -> &[PathBuf] {
        &self.roots
    }

    pub fn is_done(&self) -> bool {
        self.inner.done.load(Ordering::Acquire)
    }

    /// Items listed so far.
    pub fn len(&self) -> usize {
        self.store().nodes.len().saturating_sub(self.roots.len())
    }

    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }

    /// Changes whenever more items have been listed.
    pub fn generation(&self) -> u64 {
        self.inner.generation.load(Ordering::Acquire)
    }

    fn store(&self) -> std::sync::RwLockReadGuard<'_, Store> {
        self.inner.store.read().unwrap_or_else(|e| e.into_inner())
    }

    /// The best `limit` matches for `query`, best first.
    pub fn search(&self, query: &str, limit: usize) -> Vec<Hit> {
        let query = Query::new(query);
        if query.terms.is_empty() || limit == 0 {
            return Vec::new();
        }
        let store = self.store();
        let nodes = &store.nodes;
        let threads = thread::available_parallelism()
            .map_or(4, |n| n.get())
            .clamp(1, MAX_WORKERS);
        let chunk = nodes.len().div_ceil(threads).max(1);
        let mut best: Vec<(i32, u32)> = thread::scope(|scope| {
            let handles: Vec<_> = nodes
                .chunks(chunk)
                .enumerate()
                .map(|(c, part)| {
                    let store = &*store;
                    let query = &query;
                    scope.spawn(move || {
                        // Keeps the top `limit`: a min-heap of the best so far.
                        let mut heap = BinaryHeap::with_capacity(limit + 1);
                        for (i, node) in part.iter().enumerate() {
                            if node.depth == 0 {
                                continue;
                            }
                            if let Some(score) = query.score(store.name(node), node.depth) {
                                heap.push(std::cmp::Reverse((score, (c * chunk + i) as u32)));
                                if heap.len() > limit {
                                    heap.pop();
                                }
                            }
                        }
                        heap.into_iter().map(|r| r.0).collect::<Vec<_>>()
                    })
                })
                .collect();
            handles
                .into_iter()
                .flat_map(|h| h.join().unwrap_or_default())
                .collect()
        });
        best.sort_by(|a, b| b.0.cmp(&a.0).then(a.1.cmp(&b.1)));
        best.truncate(limit);
        best.into_iter()
            .map(|(score, id)| {
                let node = &nodes[id as usize];
                let path = store.path(id);
                let root = self
                    .roots
                    .iter()
                    .find(|r| path.starts_with(r))
                    .cloned()
                    .unwrap_or_default();
                let mut relative = path
                    .strip_prefix(&root)
                    .unwrap_or(&path)
                    .to_string_lossy()
                    .replace('\\', "/");
                if node.is_dir {
                    relative.push('/');
                }
                Hit {
                    matches: query.ranges(store.name(node)),
                    path,
                    relative,
                    is_dir: node.is_dir,
                    score,
                }
            })
            .collect()
    }
}

impl Drop for NameIndex {
    fn drop(&mut self) {
        self.inner.stopped.store(true, Ordering::Relaxed);
        let mut walk = self.inner.walk.lock().unwrap_or_else(|e| e.into_inner());
        walk.stop = true;
        self.inner.ready.notify_all();
    }
}

/// Whether to leave a folder out of the index.
fn skip_dir(name: &str, hidden: bool) -> bool {
    hidden || name.starts_with('.') || SKIP_NAMES.contains(&name)
}

#[cfg(windows)]
fn is_hidden(meta: &fs::Metadata) -> bool {
    use std::os::windows::fs::MetadataExt;
    const HIDDEN: u32 = 0x2;
    const SYSTEM: u32 = 0x4;
    meta.file_attributes() & (HIDDEN | SYSTEM) != 0
}

#[cfg(not(windows))]
fn is_hidden(_meta: &fs::Metadata) -> bool {
    false
}

fn work(inner: &Inner, wake: &(dyn Fn() + Send + Sync)) {
    let mut found: Vec<(String, bool, PathBuf)> = Vec::new();
    let mut last_wake = std::time::Instant::now();
    loop {
        let (parent, dir, depth) = {
            let mut walk = inner.walk.lock().unwrap_or_else(|e| e.into_inner());
            loop {
                if walk.stop {
                    return;
                }
                if let Some(task) = walk.queue.pop_front() {
                    walk.busy += 1;
                    break task;
                }
                if walk.busy == 0 {
                    walk.stop = true;
                    inner.done.store(true, Ordering::Release);
                    inner.generation.fetch_add(1, Ordering::AcqRel);
                    inner.ready.notify_all();
                    drop(walk);
                    wake();
                    return;
                }
                walk = inner.ready.wait(walk).unwrap_or_else(|e| e.into_inner());
            }
        };

        found.clear();
        if let Ok(read) = fs::read_dir(&dir) {
            for item in read.flatten() {
                if inner.stopped.load(Ordering::Relaxed) {
                    break;
                }
                let Ok(kind) = item.file_type() else { continue };
                let name = item.file_name().to_string_lossy().into_owned();
                if name.starts_with('.') {
                    continue;
                }
                let is_dir = kind.is_dir();
                if is_dir {
                    let hidden = cfg!(windows) && item.metadata().is_ok_and(|m| is_hidden(&m));
                    let path = item.path();
                    if skip_dir(&name, hidden) || inner.skip_paths.contains(&path) {
                        continue;
                    }
                    found.push((name, true, path));
                } else {
                    found.push((name, false, PathBuf::new()));
                }
            }
        }

        let mut subdirs = Vec::new();
        {
            let mut store = inner.store.write().unwrap_or_else(|e| e.into_inner());
            for (name, is_dir, path) in found.drain(..) {
                let Some(id) = store.push(parent, depth + 1, &name, is_dir) else {
                    break;
                };
                if is_dir {
                    subdirs.push((id, path, depth + 1));
                }
            }
        }
        inner.generation.fetch_add(1, Ordering::AcqRel);
        {
            let mut walk = inner.walk.lock().unwrap_or_else(|e| e.into_inner());
            walk.queue.extend(subdirs);
            walk.busy -= 1;
            inner.ready.notify_all();
        }
        if last_wake.elapsed() >= std::time::Duration::from_millis(150) {
            last_wake = std::time::Instant::now();
            wake();
        }
    }
}

/// A parsed query: lowercase words, all of which must appear.
struct Query {
    terms: Vec<String>,
    whole: String,
}

impl Query {
    fn new(text: &str) -> Self {
        let terms: Vec<String> = text.split_whitespace().map(str::to_lowercase).collect();
        Self {
            whole: terms.join(" "),
            terms,
        }
    }

    /// The score for `name`, or `None` if a word is missing.
    fn score(&self, name: &str, depth: u16) -> Option<i32> {
        let lower_owned;
        let lower: &str = if name.is_ascii() {
            // Fast path: compare ASCII bytes without allocating.
            for term in &self.terms {
                find_ascii_ci(name.as_bytes(), term.as_bytes())?;
            }
            lower_owned = name.to_ascii_lowercase();
            &lower_owned
        } else {
            lower_owned = name.to_lowercase();
            for term in &self.terms {
                lower_owned.find(term.as_str())?;
            }
            &lower_owned
        };
        let first = &self.terms[0];
        let stem = lower.rsplit_once('.').map_or(lower, |(s, _)| s);
        let at = lower.find(first.as_str()).unwrap_or(0);
        let mut score = if lower == self.whole {
            1000
        } else if stem == self.whole {
            900
        } else if lower.starts_with(self.whole.as_str()) {
            700
        } else if at == 0 {
            600
        } else if !lower[..at]
            .chars()
            .next_back()
            .is_some_and(char::is_alphanumeric)
        {
            400
        } else {
            200
        };
        score -= i32::from(depth) * 3;
        score -= (name.len() as i32 - self.whole.len() as i32).max(0) / 2;
        Some(score)
    }

    /// Char ranges of the first place each word appears in `name`.
    fn ranges(&self, name: &str) -> Vec<(usize, usize)> {
        let lower = name.to_lowercase();
        let mut out: Vec<(usize, usize)> = self
            .terms
            .iter()
            .filter_map(|term| {
                let byte = lower.find(term.as_str())?;
                let start = lower[..byte].chars().count();
                Some((start, start + term.chars().count()))
            })
            .collect();
        out.sort();
        out
    }
}

/// Finds lowercase ASCII `needle` in `hay`, ignoring case.
fn find_ascii_ci(hay: &[u8], needle: &[u8]) -> Option<usize> {
    if needle.is_empty() {
        return Some(0);
    }
    if needle.len() > hay.len() {
        return None;
    }
    let first = needle[0];
    (0..=hay.len() - needle.len()).find(|&i| {
        hay[i].to_ascii_lowercase() == first
            && hay[i + 1..i + needle.len()]
                .iter()
                .zip(&needle[1..])
                .all(|(a, b)| a.to_ascii_lowercase() == *b)
    })
}

#[cfg(test)]
mod tests {
    use std::time::{Duration, Instant};

    use super::*;

    fn wait(index: &NameIndex) {
        let deadline = Instant::now() + Duration::from_secs(10);
        while !index.is_done() {
            assert!(Instant::now() < deadline, "indexing didn't finish");
            thread::sleep(Duration::from_millis(2));
        }
    }

    fn tree() -> tempfile::TempDir {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();
        fs::create_dir_all(root.join("photos/2024")).unwrap();
        fs::create_dir_all(root.join("node_modules/pkg")).unwrap();
        fs::create_dir_all(root.join(".git/objects")).unwrap();
        for name in [
            "park.jpg",
            "photos/2024/Parking lot.png",
            "photos/2024/spark plug.txt",
            "photos/papers.doc",
            "photos/prank.txt",
            "photos/pear.txt",
            "node_modules/pkg/park.js",
        ] {
            fs::write(root.join(name), b"").unwrap();
        }
        dir
    }

    fn names(hits: &[Hit]) -> Vec<&str> {
        hits.iter().map(|h| h.relative.as_str()).collect()
    }

    #[test]
    fn only_names_containing_the_query_match() {
        let dir = tree();
        let index = NameIndex::start(vec![dir.path().to_path_buf()], Vec::new(), || {});
        wait(&index);
        let hits = index.search("park", 50);
        // No "papers" or "prank": the letters must be together.
        assert_eq!(
            names(&hits),
            [
                "park.jpg",
                "photos/2024/Parking lot.png",
                "photos/2024/spark plug.txt"
            ]
        );
        assert_eq!(hits[0].matches, [(0, 4)]);
        assert_eq!(hits[2].matches, [(1, 5)]);
    }

    #[test]
    fn every_word_must_appear() {
        let dir = tree();
        let index = NameIndex::start(vec![dir.path().to_path_buf()], Vec::new(), || {});
        wait(&index);
        assert_eq!(
            names(&index.search("lot park", 10)),
            ["photos/2024/Parking lot.png"]
        );
        assert!(index.search("park zebra", 10).is_empty());
        assert_eq!(names(&index.search("PHOTOS", 10)), ["photos/"]);
    }

    #[test]
    fn noise_is_skipped_and_counted() {
        let dir = tree();
        let index = NameIndex::start(vec![dir.path().to_path_buf()], Vec::new(), || {});
        wait(&index);
        // node_modules and .git aren't listed.
        assert!(index.search("pkg", 10).is_empty());
        assert!(index.search("objects", 10).is_empty());
        // 2 folders + 6 files.
        assert_eq!(index.len(), 8);
    }

    #[test]
    fn several_roots_and_skipped_paths() {
        let a = tree();
        let b = tempfile::tempdir().unwrap();
        fs::write(b.path().join("park bench.txt"), b"").unwrap();
        let index = NameIndex::start(
            vec![a.path().to_path_buf(), b.path().to_path_buf()],
            vec![a.path().join("photos")],
            || {},
        );
        wait(&index);
        let hits = index.search("park", 10);
        assert_eq!(names(&hits), ["park.jpg", "park bench.txt"]);
        assert_eq!(hits[1].path, b.path().join("park bench.txt"));
    }

    #[test]
    fn ascii_search_ignores_case() {
        assert_eq!(find_ascii_ci(b"Parking", b"park"), Some(0));
        assert_eq!(find_ascii_ci(b"sPARK", b"park"), Some(1));
        assert_eq!(find_ascii_ci(b"pa rk", b"park"), None);
        assert_eq!(find_ascii_ci(b"pa", b"park"), None);
    }

    /// `EXPLORIST_INDEX_BENCH=/usr cargo test --release index_bench --
    /// --ignored --nocapture`
    #[test]
    #[ignore]
    fn index_bench() {
        let Ok(root) = std::env::var("EXPLORIST_INDEX_BENCH") else {
            return;
        };
        let start = Instant::now();
        let index = NameIndex::start(vec![PathBuf::from(root)], Vec::new(), || {});
        wait(&index);
        println!("indexed {} items in {:?}", index.len(), start.elapsed());
        for q in ["lib", "python", "zz", "config json"] {
            let start = Instant::now();
            let hits = index.search(q, 50);
            println!(
                "{q:?}: {} hits in {:?}, best {:?}",
                hits.len(),
                start.elapsed(),
                hits.first().map(|h| &h.relative)
            );
        }
    }
}
