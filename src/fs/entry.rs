//! One item in a folder listing.

use std::borrow::Cow;
use std::fs::{self, DirEntry, Metadata};
use std::io;
use std::path::{Path, PathBuf};
use std::time::SystemTime;

use super::kind::{self, FileKind};

/// A file or folder as the file list shows it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Entry {
    /// File name. Names that aren't valid Unicode are shown lossily.
    pub name: String,
    pub path: PathBuf,
    pub is_dir: bool,
    pub kind: FileKind,
    /// Text for the Type column.
    pub type_label: Cow<'static, str>,
    /// Size in bytes. `None` for folders.
    pub size: Option<u64>,
    pub modified: Option<SystemTime>,
    pub created: Option<SystemTime>,
    pub hidden: bool,
    /// Windows "system" attribute, e.g. `pagefile.sys`.
    pub system: bool,
}

impl Entry {
    /// Builds an entry from a directory listing. On Windows the metadata
    /// comes from the listing itself, so this costs no extra system call
    /// except for symbolic links, which are followed to see what they point at.
    pub fn from_dir_entry(entry: &DirEntry) -> io::Result<Self> {
        let metadata = entry.metadata()?;
        Ok(Self::build(
            entry.file_name().to_string_lossy().into_owned(),
            entry.path(),
            &metadata,
        ))
    }

    /// Builds an entry for a single path, e.g. to refresh one row after a
    /// rename.
    pub fn from_path(path: &Path) -> io::Result<Self> {
        let metadata = fs::symlink_metadata(path)?;
        let name = path
            .file_name()
            .map(|n| n.to_string_lossy().into_owned())
            .unwrap_or_else(|| path.display().to_string());
        Ok(Self::build(name, path.to_path_buf(), &metadata))
    }

    fn build(name: String, path: PathBuf, metadata: &Metadata) -> Self {
        // A link counts as whatever it points at; a broken link as a file.
        let is_dir = if metadata.file_type().is_symlink() {
            fs::metadata(&path).is_ok_and(|target| target.is_dir())
        } else {
            metadata.is_dir()
        };
        let (kind, type_label) = kind::classify(&name, is_dir);
        let (hidden, system) = attributes(&name, metadata);

        Self {
            size: (!is_dir).then_some(metadata.len()),
            modified: metadata.modified().ok(),
            created: metadata.created().ok(),
            name,
            path,
            is_dir,
            kind,
            type_label,
            hidden,
            system,
        }
    }
}

/// Whether `path` counts as hidden on this platform.
pub fn is_hidden(path: &Path, metadata: &Metadata) -> bool {
    let name = path
        .file_name()
        .map(|n| n.to_string_lossy())
        .unwrap_or_default();
    attributes(&name, metadata).0
}

#[cfg(windows)]
fn attributes(_name: &str, metadata: &Metadata) -> (bool, bool) {
    use std::os::windows::fs::MetadataExt;
    const FILE_ATTRIBUTE_HIDDEN: u32 = 0x2;
    const FILE_ATTRIBUTE_SYSTEM: u32 = 0x4;
    let attrs = metadata.file_attributes();
    (
        attrs & FILE_ATTRIBUTE_HIDDEN != 0,
        attrs & FILE_ATTRIBUTE_SYSTEM != 0,
    )
}

/// Outside Windows, dotfiles are hidden and nothing is a system file.
#[cfg(not(windows))]
fn attributes(name: &str, _metadata: &Metadata) -> (bool, bool) {
    (name.starts_with('.'), false)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn only_entry(dir: &Path) -> Entry {
        let entry = fs::read_dir(dir).unwrap().next().unwrap().unwrap();
        Entry::from_dir_entry(&entry).unwrap()
    }

    #[test]
    fn files_have_a_size_and_kind() {
        let dir = tempfile::tempdir().unwrap();
        fs::write(dir.path().join("Cargo.toml"), b"[package]\n").unwrap();

        let entry = only_entry(dir.path());
        assert_eq!(entry.name, "Cargo.toml");
        assert!(!entry.is_dir);
        assert_eq!(entry.size, Some(10));
        assert_eq!(entry.kind, FileKind::Config);
        assert_eq!(entry.type_label, "TOML document");
        assert!(entry.modified.is_some());
        assert!(!entry.hidden);
    }

    #[test]
    fn folders_have_no_size() {
        let dir = tempfile::tempdir().unwrap();
        fs::create_dir(dir.path().join("src")).unwrap();

        let entry = only_entry(dir.path());
        assert!(entry.is_dir);
        assert_eq!(entry.size, None);
        assert_eq!(entry.kind, FileKind::Folder);
    }

    #[test]
    fn from_path_matches_from_dir_entry() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("notes.md");
        fs::write(&path, b"hi").unwrap();

        assert_eq!(Entry::from_path(&path).unwrap(), only_entry(dir.path()));
    }

    #[test]
    fn from_path_reports_missing_files() {
        let dir = tempfile::tempdir().unwrap();
        assert!(Entry::from_path(&dir.path().join("gone.txt")).is_err());
    }

    #[cfg(unix)]
    #[test]
    fn dotfiles_are_hidden_outside_windows() {
        let dir = tempfile::tempdir().unwrap();
        fs::write(dir.path().join(".env"), b"").unwrap();
        assert!(only_entry(dir.path()).hidden);
    }

    #[cfg(unix)]
    #[test]
    fn links_count_as_what_they_point_at() {
        let dir = tempfile::tempdir().unwrap();
        fs::create_dir(dir.path().join("target")).unwrap();
        std::os::unix::fs::symlink(dir.path().join("target"), dir.path().join("link")).unwrap();
        std::os::unix::fs::symlink(dir.path().join("missing"), dir.path().join("broken")).unwrap();

        assert!(Entry::from_path(&dir.path().join("link")).unwrap().is_dir);
        assert!(!Entry::from_path(&dir.path().join("broken")).unwrap().is_dir);
    }
}
