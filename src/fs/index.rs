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
use std::io::{self, BufWriter, Write};
use std::path::{Path, PathBuf};
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
    /// `names` with ASCII letters lowercased: same length and offsets, so
    /// a query can be found in all names at once with a fast byte search.
    lower: String,
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
        self.lower
            .extend(name.chars().map(|c| c.to_ascii_lowercase()));
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
    /// Unique per list, so a new list never looks like an old one.
    serial: u64,
}

fn next_serial() -> u64 {
    static NEXT: AtomicU64 = AtomicU64::new(1);
    NEXT.fetch_add(1, Ordering::Relaxed)
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
        Self {
            roots,
            inner,
            serial: next_serial(),
        }
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

    /// Changes whenever more items have been listed, and differs between
    /// lists.
    pub fn generation(&self) -> u64 {
        (self.serial << 40) ^ self.inner.generation.load(Ordering::Acquire)
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
        let best = if query.terms.iter().all(|t| t.is_ascii()) {
            find_ascii(&store, &query, limit)
        } else {
            scan(&store, &query, limit)
        };
        let nodes = &store.nodes;
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

fn threads() -> usize {
    thread::available_parallelism()
        .map_or(4, |n| n.get())
        .clamp(1, MAX_WORKERS)
}

/// The best results so far in one part of the search.
struct Top {
    /// A min-heap, so the worst kept result is on top.
    heap: BinaryHeap<std::cmp::Reverse<(i32, u32)>>,
    limit: usize,
}

impl Top {
    fn keep(&mut self, score: i32, id: u32) {
        self.heap.push(std::cmp::Reverse((score, id)));
        if self.heap.len() > self.limit {
            self.heap.pop();
        }
    }

    /// The score a result must beat, once there are enough.
    fn floor(&self) -> Option<i32> {
        (self.heap.len() >= self.limit).then(|| self.heap.peek().map_or(i32::MIN, |r| r.0.0))
    }
}

/// Keeps the best `limit` of what `each` finds in every part of the
/// nodes, run on all cores: (score, node id), best first.
fn best_in_parts(
    nodes: &[Node],
    limit: usize,
    each: impl Fn(std::ops::Range<usize>, &mut Top) + Sync,
) -> Vec<(i32, u32)> {
    let chunk = nodes.len().div_ceil(threads()).max(1);
    let mut best: Vec<(i32, u32)> = thread::scope(|scope| {
        let handles: Vec<_> = (0..nodes.len())
            .step_by(chunk)
            .map(|start| {
                let each = &each;
                let range = start..(start + chunk).min(nodes.len());
                scope.spawn(move || {
                    let mut top = Top {
                        heap: BinaryHeap::with_capacity(limit + 1),
                        limit,
                    };
                    each(range, &mut top);
                    top.heap.into_iter().map(|r| r.0).collect::<Vec<_>>()
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
    best
}

/// ASCII queries: finds the longest word in all the lowercased names with
/// one fast byte search per part, and only looks closer at names it's in.
fn find_ascii(store: &Store, query: &Query, limit: usize) -> Vec<(i32, u32)> {
    let nodes = &store.nodes;
    let lower = store.lower.as_bytes();
    let lead = query
        .terms
        .iter()
        .max_by_key(|t| t.len())
        .expect("terms aren't empty");
    let finder = memchr::memmem::Finder::new(lead.as_bytes());
    best_in_parts(nodes, limit, |range, top| {
        let Some(last) = range.clone().last() else {
            return;
        };
        let start = nodes[range.start].name_start as usize;
        let hay = &lower[start..end_of_node(&nodes[last])];
        let mut pos = 0;
        let mut id = range.start;
        while let Some(found) = finder.find(&hay[pos..]) {
            let at = start + pos + found;
            // The name it's in: names are stored back to back, in order.
            while end_of_node(&nodes[id]) <= at {
                id += 1;
            }
            let node = &nodes[id];
            let end = end_of_node(node);
            if at + lead.len() <= end && node.depth != 0 {
                let name = &store.lower[node.name_start as usize..end];
                let worth_it = top
                    .floor()
                    .is_none_or(|floor| query.best_rank(name, node.depth) >= floor);
                if worth_it
                    && query.terms.iter().all(|t| {
                        t == lead || memchr::memmem::find(name.as_bytes(), t.as_bytes()).is_some()
                    })
                {
                    top.keep(query.rank(name, node.depth), id as u32);
                }
            }
            // On to the next name.
            pos = end - start;
            if pos >= hay.len() {
                break;
            }
        }
    })
}

fn end_of_node(node: &Node) -> usize {
    node.name_start as usize + node.name_len as usize
}

/// Any query: looks at every name in turn.
fn scan(store: &Store, query: &Query, limit: usize) -> Vec<(i32, u32)> {
    best_in_parts(&store.nodes, limit, |range, top| {
        for id in range {
            let node = &store.nodes[id];
            if node.depth == 0 {
                continue;
            }
            if let Some(score) = query.score(store.name(node), node.depth) {
                top.keep(score, id as u32);
            }
        }
    })
}

/// Start of a saved list, with its format version.
const SAVED_MAGIC: &[u8; 8] = b"EXPIDX01";

impl NameIndex {
    /// Saves the finished list, so the next start can search at once
    /// (see [`Self::load`]).
    pub fn save(&self, path: &Path) -> io::Result<()> {
        if !self.is_done() {
            return Err(io::Error::other("the list isn't finished"));
        }
        if let Some(dir) = path.parent() {
            fs::create_dir_all(dir)?;
        }
        let tmp = path.with_extension(format!("tmp{}", std::process::id()));
        {
            let store = self.store();
            let mut out = BufWriter::new(fs::File::create(&tmp)?);
            out.write_all(SAVED_MAGIC)?;
            out.write_all(&(self.roots.len() as u32).to_le_bytes())?;
            for root in &self.roots {
                let text = root.to_string_lossy();
                out.write_all(&(text.len() as u32).to_le_bytes())?;
                out.write_all(text.as_bytes())?;
            }
            out.write_all(&(store.nodes.len() as u64).to_le_bytes())?;
            for node in &store.nodes {
                out.write_all(&node.parent.to_le_bytes())?;
                out.write_all(&node.name_start.to_le_bytes())?;
                out.write_all(&node.name_len.to_le_bytes())?;
                out.write_all(&node.depth.to_le_bytes())?;
                out.write_all(&[u8::from(node.is_dir)])?;
            }
            out.write_all(&(store.names.len() as u64).to_le_bytes())?;
            out.write_all(store.names.as_bytes())?;
            out.flush()?;
        }
        fs::rename(&tmp, path)
    }

    /// A list saved by [`Self::save`], finished and ready to search. `None`
    /// if it's missing or damaged.
    pub fn load(path: &Path) -> Option<Self> {
        let data = fs::read(path).ok()?;
        let mut at = 0usize;
        let mut take = |n: usize| -> Option<&[u8]> {
            let part = data.get(at..at.checked_add(n)?)?;
            at += n;
            Some(part)
        };
        if take(8)? != SAVED_MAGIC {
            return None;
        }
        let u32_of = |b: &[u8]| u32::from_le_bytes(b.try_into().unwrap_or_default());
        let roots_len = u32_of(take(4)?) as usize;
        let mut roots = Vec::with_capacity(roots_len.min(64));
        for _ in 0..roots_len {
            let len = u32_of(take(4)?) as usize;
            roots.push(PathBuf::from(std::str::from_utf8(take(len)?).ok()?));
        }
        let count = u64::from_le_bytes(take(8)?.try_into().ok()?) as usize;
        if count > MAX_ITEMS {
            return None;
        }
        let mut nodes = Vec::with_capacity(count);
        for _ in 0..count {
            let b = take(13)?;
            nodes.push(Node {
                parent: u32_of(&b[0..4]),
                name_start: u32_of(&b[4..8]),
                name_len: u16::from_le_bytes([b[8], b[9]]),
                depth: u16::from_le_bytes([b[10], b[11]]),
                is_dir: b[12] != 0,
            });
        }
        let names_len = u64::from_le_bytes(take(8)?.try_into().ok()?) as usize;
        let names = std::str::from_utf8(take(names_len)?).ok()?.to_owned();
        // Check every record, so a damaged file can't point outside the
        // names or make a loop of parents.
        for (id, node) in nodes.iter().enumerate() {
            let start = node.name_start as usize;
            let end = start + node.name_len as usize;
            let parent_ok = if node.parent == NO_PARENT {
                node.depth == 0
            } else {
                (node.parent as usize) < id
            };
            if end > names.len()
                || !names.is_char_boundary(start)
                || !names.is_char_boundary(end)
                || !parent_ok
            {
                return None;
            }
        }
        let inner = Arc::new(Inner {
            store: RwLock::new(Store {
                nodes,
                lower: names.to_ascii_lowercase(),
                names,
            }),
            walk: Mutex::new(Walk {
                queue: VecDeque::new(),
                busy: 0,
                stop: true,
            }),
            ready: Condvar::new(),
            stopped: AtomicBool::new(true),
            done: AtomicBool::new(true),
            generation: AtomicU64::new(1),
            skip_paths: HashSet::new(),
        });
        Some(Self {
            roots,
            inner,
            serial: next_serial(),
        })
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
        let lower = if name.is_ascii() {
            // Fast path: compare ASCII bytes without allocating.
            for term in &self.terms {
                find_ascii_ci(name.as_bytes(), term.as_bytes())?;
            }
            name.to_ascii_lowercase()
        } else {
            let lower = name.to_lowercase();
            for term in &self.terms {
                lower.find(term.as_str())?;
            }
            lower
        };
        Some(self.rank(&lower, depth))
    }

    /// The most [`Self::rank`] could give `lower`, worked out cheaply, to
    /// pass over names that can't make the results.
    fn best_rank(&self, lower: &str, depth: u16) -> i32 {
        let tier = if lower.starts_with(self.whole.as_str()) {
            1000
        } else if lower.starts_with(self.terms[0].as_str()) {
            600
        } else {
            400
        };
        tier - i32::from(depth) * 3 - (lower.len() as i32 - self.whole.len() as i32).max(0) / 2
    }

    /// How good a match `lower` (a lowercased name with every word in it)
    /// is: exact names first, then ones starting with the query, then ones
    /// where it starts a word; shallower and shorter names break ties.
    fn rank(&self, lower: &str, depth: u16) -> i32 {
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
        score -= (lower.len() as i32 - self.whole.len() as i32).max(0) / 2;
        score
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

    /// Search, save and load times for a made-up list the size of a busy
    /// computer's. Run with `cargo test --release scale_bench --
    /// --ignored --nocapture`.
    #[test]
    #[ignore]
    fn scale_bench() {
        const ITEMS: usize = 2_000_000;
        let words = [
            "report", "photo", "invoice", "holiday", "project", "backup", "notes", "music",
            "video", "draft", "final", "budget", "scan", "letter", "game", "config", "build",
            "index", "data", "song",
        ];
        let exts = [
            "txt", "jpg", "pdf", "mp4", "docx", "png", "rs", "json", "mp3", "zip",
        ];
        let mut store = Store::default();
        store.push(NO_PARENT, 0, "/", true);
        let mut folders = vec![0u32];
        let mut seed = 7u64;
        let mut next = || {
            seed ^= seed << 13;
            seed ^= seed >> 7;
            seed ^= seed << 17;
            seed
        };
        for i in 0..ITEMS {
            let r = next();
            let parent = folders[(r as usize >> 8) % folders.len()];
            let depth = store.nodes[parent as usize].depth + 1;
            let w1 = words[(r % 20) as usize];
            let w2 = words[((r >> 5) % 20) as usize];
            let is_dir = i % 12 == 0;
            let name = if is_dir {
                format!("{w1} {w2} {}", r % 1000)
            } else {
                format!(
                    "{w1}_{w2}_{}.{}",
                    r % 100_000,
                    exts[((r >> 11) % 10) as usize]
                )
            };
            if let Some(id) = store.push(parent, depth.min(30), &name, is_dir)
                && is_dir
            {
                folders.push(id);
            }
        }
        let inner = Arc::new(Inner {
            store: RwLock::new(store),
            walk: Mutex::new(Walk {
                queue: VecDeque::new(),
                busy: 0,
                stop: true,
            }),
            ready: Condvar::new(),
            stopped: AtomicBool::new(true),
            done: AtomicBool::new(true),
            generation: AtomicU64::new(1),
            skip_paths: HashSet::new(),
        });
        let index = NameIndex {
            roots: vec![PathBuf::from("/")],
            inner,
            serial: next_serial(),
        };
        println!("{} items", index.len());
        for q in ["holiday", "invoice 4521", "zz", "final_budget", "p"] {
            let start = Instant::now();
            let hits = index.search(q, 60);
            println!("{q:?}: {} hits in {:?}", hits.len(), start.elapsed());
        }
        let dir = tempfile::tempdir().unwrap();
        let file = dir.path().join("index.bin");
        let start = Instant::now();
        index.save(&file).unwrap();
        println!(
            "saved {} MB in {:?}",
            fs::metadata(&file).unwrap().len() / 1_000_000,
            start.elapsed()
        );
        let start = Instant::now();
        let loaded = NameIndex::load(&file).unwrap();
        println!("loaded {} items in {:?}", loaded.len(), start.elapsed());
    }

    /// The fast byte search finds what looking at every name finds.
    #[test]
    fn fast_search_matches_the_full_scan() {
        let dir = tree();
        for name in ["Über Park.txt", "PARK-ing.MD", "aparkb", "pa rk", "x.park"] {
            fs::write(dir.path().join(name), b"").unwrap();
        }
        let index = NameIndex::start(vec![dir.path().to_path_buf()], Vec::new(), || {});
        wait(&index);
        let store = index.store();
        for q in ["park", "PARK", "pa", "rk ing", "a", "x.", "über", "zzz"] {
            let query = Query::new(q);
            if query.terms.iter().all(|t| t.is_ascii()) {
                assert_eq!(
                    find_ascii(&store, &query, 50),
                    scan(&store, &query, 50),
                    "query {q:?}"
                );
            }
        }
        drop(store);
        assert_eq!(names(&index.search("über", 5)), ["Über Park.txt"]);
    }

    #[test]
    fn saved_lists_load_and_search_the_same() {
        let dir = tree();
        let index = NameIndex::start(vec![dir.path().to_path_buf()], Vec::new(), || {});
        wait(&index);
        let file = tempfile::tempdir().unwrap();
        let saved = file.path().join("index.bin");
        index.save(&saved).unwrap();
        let loaded = NameIndex::load(&saved).unwrap();
        assert!(loaded.is_done());
        assert_eq!(loaded.len(), index.len());
        assert_eq!(loaded.roots(), index.roots());
        assert_eq!(loaded.search("park", 10), index.search("park", 10));

        // Damaged files are refused rather than trusted.
        let mut data = fs::read(&saved).unwrap();
        let cut = data.len() - 3;
        data.truncate(cut);
        fs::write(&saved, &data).unwrap();
        assert!(NameIndex::load(&saved).is_none());
        fs::write(&saved, b"EXPIDX01junk").unwrap();
        assert!(NameIndex::load(&saved).is_none());
        assert!(NameIndex::load(&file.path().join("missing")).is_none());
    }
}
