//! What the Properties dialog shows: dates and attributes, and folder
//! sizes counted in the background.

use std::fs;
use std::io;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::SystemTime;

/// One item's dates and attributes.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Details {
    pub created: Option<SystemTime>,
    pub modified: Option<SystemTime>,
    pub accessed: Option<SystemTime>,
    pub read_only: bool,
    pub hidden: bool,
    /// Where a link points, for symlinks and junctions.
    pub link_target: Option<PathBuf>,
}

pub fn details(path: &Path) -> io::Result<Details> {
    let link = fs::symlink_metadata(path)?;
    let link_target = if link.file_type().is_symlink() {
        fs::read_link(path).ok()
    } else {
        None
    };
    let meta = fs::metadata(path).unwrap_or(link);
    Ok(Details {
        created: meta.created().ok(),
        modified: meta.modified().ok(),
        accessed: meta.accessed().ok(),
        read_only: meta.permissions().readonly(),
        hidden: super::entry::is_hidden(path, &meta),
        link_target,
    })
}

/// Makes `path` read-only or writable.
pub fn set_read_only(path: &Path, read_only: bool) -> io::Result<()> {
    let mut perms = fs::metadata(path)?.permissions();
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        // Only the owner's write bit, so turning it back on doesn't make
        // the file writable by everyone.
        let mode = perms.mode();
        perms.set_mode(if read_only {
            mode & !0o222
        } else {
            mode | 0o200
        });
    }
    #[cfg(not(unix))]
    perms.set_readonly(read_only);
    fs::set_permissions(path, perms)
}

/// Sets or clears the hidden attribute. Windows only: elsewhere hidden
/// means the name starts with a dot.
#[cfg(windows)]
pub fn set_hidden(path: &Path, hidden: bool) -> io::Result<()> {
    use std::os::windows::ffi::OsStrExt;
    use windows_sys::Win32::Storage::FileSystem::{
        FILE_ATTRIBUTE_HIDDEN, GetFileAttributesW, INVALID_FILE_ATTRIBUTES, SetFileAttributesW,
    };
    let wide: Vec<u16> = path.as_os_str().encode_wide().chain(Some(0)).collect();
    // SAFETY: `wide` is a NUL-terminated UTF-16 path that outlives both calls.
    unsafe {
        let attrs = GetFileAttributesW(wide.as_ptr());
        if attrs == INVALID_FILE_ATTRIBUTES {
            return Err(io::Error::last_os_error());
        }
        let attrs = if hidden {
            attrs | FILE_ATTRIBUTE_HIDDEN
        } else {
            attrs & !FILE_ATTRIBUTE_HIDDEN
        };
        if SetFileAttributesW(wide.as_ptr(), attrs) == 0 {
            return Err(io::Error::last_os_error());
        }
    }
    Ok(())
}

/// Running totals for a set of items and everything inside them.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct Totals {
    pub files: u64,
    pub folders: u64,
    pub bytes: u64,
    /// Folders that couldn't be read.
    pub unreadable: u64,
    pub done: bool,
}

/// Counts in the background; stops when dropped.
pub struct Counter {
    totals: Arc<Mutex<Totals>>,
    stop: Arc<AtomicBool>,
}

impl Counter {
    pub fn start(paths: Vec<PathBuf>, wake: impl Fn() + Send + 'static) -> Self {
        let totals = Arc::new(Mutex::new(Totals::default()));
        let stop = Arc::new(AtomicBool::new(false));
        let (t, s) = (Arc::clone(&totals), Arc::clone(&stop));
        thread::Builder::new()
            .name("explorist-sizes".into())
            .spawn(move || {
                count(&paths, &t, &s, &wake);
                t.lock().unwrap_or_else(|e| e.into_inner()).done = true;
                wake();
            })
            .ok();
        Self { totals, stop }
    }

    pub fn totals(&self) -> Totals {
        *self.totals.lock().unwrap_or_else(|e| e.into_inner())
    }
}

impl Drop for Counter {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::Relaxed);
    }
}

/// Walks without following links, so a link to a parent folder can't loop.
fn count(paths: &[PathBuf], totals: &Mutex<Totals>, stop: &AtomicBool, wake: &dyn Fn()) {
    let mut stack: Vec<PathBuf> = Vec::new();
    let add = |meta: &fs::Metadata, path: &Path, stack: &mut Vec<PathBuf>| {
        let mut t = totals.lock().unwrap_or_else(|e| e.into_inner());
        if meta.is_dir() {
            t.folders += 1;
            stack.push(path.to_path_buf());
        } else {
            t.files += 1;
            t.bytes += meta.len();
        }
    };
    for path in paths {
        if let Ok(meta) = fs::symlink_metadata(path) {
            add(&meta, path, &mut stack);
        }
    }
    // The selected folders themselves aren't "contained", so don't count them.
    {
        let mut t = totals.lock().unwrap_or_else(|e| e.into_inner());
        t.folders -= stack.len() as u64;
    }
    let mut since_wake = 0;
    while let Some(dir) = stack.pop() {
        if stop.load(Ordering::Relaxed) {
            return;
        }
        let Ok(read) = fs::read_dir(&dir) else {
            totals.lock().unwrap_or_else(|e| e.into_inner()).unreadable += 1;
            continue;
        };
        for item in read.flatten() {
            if let Ok(meta) = fs::symlink_metadata(item.path()) {
                add(&meta, &item.path(), &mut stack);
            }
        }
        since_wake += 1;
        if since_wake >= 64 {
            since_wake = 0;
            wake();
        }
    }
}

#[cfg(test)]
mod tests {
    use std::time::{Duration, Instant};

    use super::*;

    fn wait(counter: &Counter) -> Totals {
        let start = Instant::now();
        while !counter.totals().done && start.elapsed() < Duration::from_secs(10) {
            thread::sleep(Duration::from_millis(5));
        }
        counter.totals()
    }

    #[test]
    fn counts_everything_inside() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path().join("project");
        fs::create_dir_all(root.join("src/ui")).unwrap();
        fs::write(root.join("a.txt"), b"12345").unwrap();
        fs::write(root.join("src/b.rs"), b"123").unwrap();
        fs::write(root.join("src/ui/c.rs"), b"1").unwrap();
        let totals = wait(&Counter::start(vec![root], || {}));
        assert_eq!(
            (totals.files, totals.folders, totals.bytes),
            (3, 2, 9),
            "{totals:?}"
        );
    }

    #[test]
    fn mixed_selection_adds_up() {
        let dir = tempfile::tempdir().unwrap();
        let file = dir.path().join("one.bin");
        fs::write(&file, vec![0u8; 100]).unwrap();
        let sub = dir.path().join("sub");
        fs::create_dir(&sub).unwrap();
        fs::write(sub.join("two.bin"), vec![0u8; 50]).unwrap();
        let totals = wait(&Counter::start(vec![file, sub], || {}));
        assert_eq!((totals.files, totals.folders, totals.bytes), (2, 0, 150));
    }

    #[test]
    fn read_only_round_trips() {
        let dir = tempfile::tempdir().unwrap();
        let file = dir.path().join("notes.txt");
        fs::write(&file, b"x").unwrap();
        set_read_only(&file, true).unwrap();
        assert!(details(&file).unwrap().read_only);
        set_read_only(&file, false).unwrap();
        let d = details(&file).unwrap();
        assert!(!d.read_only);
        assert!(d.modified.is_some());
        assert_eq!(d.link_target, None);
    }

    #[cfg(windows)]
    #[test]
    fn hidden_round_trips() {
        let dir = tempfile::tempdir().unwrap();
        let file = dir.path().join("notes.txt");
        fs::write(&file, b"x").unwrap();
        set_hidden(&file, true).unwrap();
        assert!(details(&file).unwrap().hidden);
        set_hidden(&file, false).unwrap();
        assert!(!details(&file).unwrap().hidden);
    }
}
