//! Noticing changes made to the open folder by other programs.

use std::collections::HashSet;
use std::path::{Path, PathBuf};
use std::sync::mpsc::{self, Receiver};

use notify::{EventKind, RecommendedWatcher, RecursiveMode, Watcher};

/// Above this many changed paths in one go, re-reading the whole folder is
/// cheaper than checking each path.
pub const RESCAN_THRESHOLD: usize = 500;

/// What changed since the last [`FolderWatcher::poll`].
#[derive(Debug, Default, PartialEq, Eq)]
pub struct Changes {
    /// Paths directly inside the folder that were created, modified,
    /// renamed or removed.
    pub paths: HashSet<PathBuf>,
    /// The watcher lost track (overflow) or too much changed: re-read all.
    pub rescan: bool,
}

impl Changes {
    pub fn is_empty(&self) -> bool {
        self.paths.is_empty() && !self.rescan
    }
}

enum Signal {
    Paths(Vec<PathBuf>),
    Rescan,
}

/// Watches one folder, not its subfolders. Dropping it stops watching.
pub struct FolderWatcher {
    folder: PathBuf,
    events: Receiver<Signal>,
    _watcher: RecommendedWatcher,
}

impl FolderWatcher {
    /// Starts watching `folder`. `wake` is called from the watcher's thread
    /// whenever something changes.
    pub fn start(folder: &Path, wake: impl Fn() + Send + 'static) -> notify::Result<Self> {
        let (tx, events) = mpsc::channel();
        let mut watcher =
            notify::recommended_watcher(move |result: notify::Result<notify::Event>| {
                let signal = match result {
                    Ok(event) if event.need_rescan() => Signal::Rescan,
                    Ok(event) => match event.kind {
                        EventKind::Access(_) => return,
                        _ => Signal::Paths(event.paths),
                    },
                    Err(_) => Signal::Rescan,
                };
                if tx.send(signal).is_ok() {
                    wake();
                }
            })?;
        watcher.watch(folder, RecursiveMode::NonRecursive)?;
        Ok(Self {
            folder: folder.to_path_buf(),
            events,
            _watcher: watcher,
        })
    }

    /// Takes the changes reported so far, without waiting.
    pub fn poll(&self) -> Changes {
        let mut changes = Changes::default();
        while let Ok(signal) = self.events.try_recv() {
            match signal {
                Signal::Rescan => changes.rescan = true,
                Signal::Paths(paths) => {
                    for path in paths {
                        if path == self.folder {
                            // The folder itself was renamed or removed.
                            changes.rescan = true;
                        } else if path.parent() == Some(self.folder.as_path()) {
                            changes.paths.insert(path);
                        }
                    }
                }
            }
        }
        if changes.paths.len() > RESCAN_THRESHOLD {
            changes.rescan = true;
        }
        changes
    }
}

#[cfg(test)]
mod tests {
    use std::fs;
    use std::time::{Duration, Instant};

    use super::*;

    /// Collects changes until `done` is satisfied or time runs out.
    fn wait_for(watcher: &FolderWatcher, done: impl Fn(&Changes) -> bool) -> Changes {
        let deadline = Instant::now() + Duration::from_secs(10);
        let mut all = Changes::default();
        while Instant::now() < deadline {
            let more = watcher.poll();
            all.paths.extend(more.paths);
            all.rescan |= more.rescan;
            if done(&all) {
                break;
            }
            std::thread::sleep(Duration::from_millis(20));
        }
        all
    }

    #[test]
    fn reports_created_and_removed_files() {
        let dir = tempfile::tempdir().unwrap();
        // Watch the canonical path: macOS and Windows temp dirs can be links.
        let root = fs::canonicalize(dir.path()).unwrap();
        let watcher = FolderWatcher::start(&root, || {}).unwrap();

        let file = root.join("new.txt");
        fs::write(&file, b"hi").unwrap();
        let changes = wait_for(&watcher, |c| c.paths.contains(&file) || c.rescan);
        assert!(
            changes.paths.contains(&file) || changes.rescan,
            "{changes:?}"
        );

        fs::remove_file(&file).unwrap();
        let changes = wait_for(&watcher, |c| c.paths.contains(&file) || c.rescan);
        assert!(changes.paths.contains(&file) || changes.rescan);
    }

    #[test]
    fn ignores_changes_inside_subfolders() {
        let dir = tempfile::tempdir().unwrap();
        let root = fs::canonicalize(dir.path()).unwrap();
        fs::create_dir(root.join("sub")).unwrap();
        let watcher = FolderWatcher::start(&root, || {}).unwrap();

        fs::write(root.join("sub").join("deep.txt"), b"").unwrap();
        std::thread::sleep(Duration::from_millis(300));
        let changes = watcher.poll();
        assert!(!changes.paths.contains(&root.join("sub").join("deep.txt")));
    }

    #[test]
    fn missing_folders_cannot_be_watched() {
        let dir = tempfile::tempdir().unwrap();
        assert!(FolderWatcher::start(&dir.path().join("nope"), || {}).is_err());
    }
}
