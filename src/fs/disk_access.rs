//! Full Disk Access on macOS. Without it, macOS asks separately the first
//! time anything reads Desktop, Documents, Downloads or a drive, so the
//! search index and folder sizes, running in the background, would bring
//! up one question after another. Explorist asks once for Full Disk Access
//! instead, and keeps its background work out of those places until it has
//! it. Elsewhere there's nothing to ask for.

use std::path::{Path, PathBuf};

/// Whether Explorist may read everything. Always true outside macOS.
pub fn full() -> bool {
    imp::full()
}

/// Where background work stays out for now: the places macOS asks about
/// one by one, while Explorist doesn't have Full Disk Access.
pub fn kept_out() -> Vec<PathBuf> {
    if full() { Vec::new() } else { guarded() }
}

/// Whether background work should leave `path` alone for now.
pub fn keep_out(path: &Path) -> bool {
    !full() && guarded().iter().any(|place| path.starts_with(place))
}

/// Whether reading `path` would bring up a question, for work done on
/// the folders shown (like their sizes): Desktop, Documents and Downloads
/// themselves, and drives. Folders inside them are fine once the person
/// went in, as macOS asked then.
pub fn would_ask(path: &Path) -> bool {
    !full()
        && guarded().iter().any(|place| {
            path == place || (place == Path::new("/Volumes") && path.parent() == Some(place))
        })
}

/// Opens System Settings at Full Disk Access.
pub fn open_settings() -> std::io::Result<()> {
    imp::open_settings()
}

/// The places macOS asks about one by one.
fn guarded() -> Vec<PathBuf> {
    imp::guarded(dirs::home_dir().as_deref())
}

#[cfg(target_os = "macos")]
mod imp {
    use std::fs::File;
    use std::io;
    use std::path::{Path, PathBuf};
    use std::sync::Mutex;
    use std::time::{Duration, Instant};

    /// Checking is one file open, but callers ask often: remember the
    /// answer for a moment.
    static CHECKED: Mutex<Option<(Instant, bool)>> = Mutex::new(None);

    pub fn full() -> bool {
        let mut checked = CHECKED.lock().unwrap_or_else(|e| e.into_inner());
        if let Some((at, answer)) = *checked
            && at.elapsed() < Duration::from_secs(2)
        {
            return answer;
        }
        let answer = probe();
        *checked = Some((Instant::now(), answer));
        answer
    }

    /// Reads the privacy database, which only Full Disk Access allows.
    /// Being refused here never brings up a question.
    fn probe() -> bool {
        let home = dirs::home_dir().unwrap_or_default();
        let candidates = [
            home.join("Library/Application Support/com.apple.TCC/TCC.db"),
            PathBuf::from("/Library/Application Support/com.apple.TCC/TCC.db"),
        ];
        for path in candidates {
            match File::open(&path) {
                Ok(_) => return true,
                Err(err) if err.kind() == io::ErrorKind::NotFound => continue,
                Err(_) => return false,
            }
        }
        false
    }

    pub fn guarded(home: Option<&Path>) -> Vec<PathBuf> {
        let mut places: Vec<PathBuf> = home
            .map(|home| {
                ["Desktop", "Documents", "Downloads"]
                    .iter()
                    .map(|name| home.join(name))
                    .collect()
            })
            .unwrap_or_default();
        // Other drives, removable or on the network.
        places.push(PathBuf::from("/Volumes"));
        places
    }

    pub fn open_settings() -> io::Result<()> {
        open::that_detached(
            "x-apple.systempreferences:com.apple.preference.security?Privacy_AllFiles",
        )
    }
}

#[cfg(not(target_os = "macos"))]
mod imp {
    use std::path::{Path, PathBuf};

    pub fn full() -> bool {
        true
    }

    pub fn guarded(_home: Option<&Path>) -> Vec<PathBuf> {
        Vec::new()
    }

    pub fn open_settings() -> std::io::Result<()> {
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    #[cfg(not(target_os = "macos"))]
    fn nothing_is_kept_out_elsewhere() {
        assert!(full());
        assert!(kept_out().is_empty());
        assert!(!keep_out(Path::new("/home/someone/Desktop")));
    }

    #[test]
    #[cfg(target_os = "macos")]
    fn desktop_documents_downloads_and_drives_are_guarded() {
        let places = imp::guarded(Some(Path::new("/Users/someone")));
        for place in [
            "/Users/someone/Desktop",
            "/Users/someone/Documents",
            "/Users/someone/Downloads",
            "/Volumes",
        ] {
            assert!(places.contains(&PathBuf::from(place)), "{place}");
        }
        assert!(!places.contains(&PathBuf::from("/Users/someone/Pictures")));
    }
}
