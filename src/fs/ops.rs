//! Changing things on disk: rename, create, delete.
//!
//! Names are checked against Windows' rules on every platform, since that's
//! where Explorist runs.

use std::fmt;
use std::fs;
use std::io;
use std::ops::Range;
use std::path::{Path, PathBuf};

/// Characters Windows never allows in a file name.
const FORBIDDEN: [char; 9] = ['<', '>', ':', '"', '/', '\\', '|', '?', '*'];

/// Device names Windows reserves, with or without an extension.
const RESERVED: [&str; 4] = ["CON", "PRN", "AUX", "NUL"];

/// Longest name NTFS allows, in UTF-16 code units.
const MAX_NAME_LEN: usize = 255;

/// Why a name was rejected. `Display` gives the message shown under the
/// rename field.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum NameError {
    Empty,
    DotName,
    ForbiddenChar(char),
    ControlChar,
    Reserved(String),
    TrailingSpaceOrPeriod,
    TooLong,
}

impl fmt::Display for NameError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Empty => f.write_str("Name can’t be empty"),
            Self::DotName => f.write_str("“.” and “..” aren’t allowed as names"),
            Self::ForbiddenChar(c) => write!(f, "Names can’t contain {c}"),
            Self::ControlChar => f.write_str("Names can’t contain control characters"),
            Self::Reserved(word) => write!(f, "“{word}” is reserved by Windows"),
            Self::TrailingSpaceOrPeriod => f.write_str("Names can’t end with a space or period"),
            Self::TooLong => write!(f, "Names can be at most {MAX_NAME_LEN} characters"),
        }
    }
}

/// Checks a single file or folder name against Windows' rules.
pub fn validate_name(name: &str) -> Result<(), NameError> {
    if name.trim().is_empty() {
        return Err(NameError::Empty);
    }
    if name == "." || name == ".." {
        return Err(NameError::DotName);
    }
    if let Some(c) = name.chars().find(|c| FORBIDDEN.contains(c)) {
        return Err(NameError::ForbiddenChar(c));
    }
    if name.chars().any(|c| (c as u32) < 32) {
        return Err(NameError::ControlChar);
    }
    if name.ends_with([' ', '.']) {
        return Err(NameError::TrailingSpaceOrPeriod);
    }
    if name.encode_utf16().count() > MAX_NAME_LEN {
        return Err(NameError::TooLong);
    }
    // "CON", "con.txt" and "COM1.tar.gz" are all reserved; Windows looks at
    // the part before the first dot, ignoring trailing spaces.
    let device = name.split('.').next().unwrap_or(name).trim_end();
    let upper = device.to_uppercase();
    let numbered = |prefix: &str| {
        upper.strip_prefix(prefix).is_some_and(|n| {
            matches!(
                n,
                "1" | "2" | "3" | "4" | "5" | "6" | "7" | "8" | "9" | "¹" | "²" | "³"
            )
        })
    };
    if RESERVED.contains(&upper.as_str()) || numbered("COM") || numbered("LPT") {
        return Err(NameError::Reserved(upper));
    }
    Ok(())
}

/// The part of a name that rename starts with selected: everything before
/// the last dot for files ("release-notes" of "release-notes.md"), the
/// whole name for folders and dotfiles. Returns a byte range.
pub fn stem_range(name: &str, is_dir: bool) -> Range<usize> {
    if is_dir {
        return 0..name.len();
    }
    match name.rfind('.') {
        Some(dot) if dot > 0 => 0..dot,
        _ => 0..name.len(),
    }
}

/// Why a file operation failed. `Display` gives the toast message.
#[derive(Debug)]
pub enum OpError {
    InvalidName(NameError),
    AlreadyExists(PathBuf),
    Io(io::Error),
    Trash(String),
}

impl fmt::Display for OpError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::InvalidName(err) => err.fmt(f),
            Self::AlreadyExists(path) => {
                let name = path.file_name().unwrap_or(path.as_os_str());
                write!(f, "“{}” already exists", name.to_string_lossy())
            }
            Self::Io(err) => err.fmt(f),
            Self::Trash(msg) => write!(f, "Recycle Bin: {msg}"),
        }
    }
}

impl std::error::Error for OpError {}

impl From<NameError> for OpError {
    fn from(err: NameError) -> Self {
        Self::InvalidName(err)
    }
}

impl From<io::Error> for OpError {
    fn from(err: io::Error) -> Self {
        Self::Io(err)
    }
}

/// Renames `from` to `new_name` in the same folder and returns the new path.
///
/// Refuses to replace an existing item (Windows' rename would silently
/// overwrite files), except for a change in case only, like `readme.md`
/// to `README.md`.
pub fn rename(from: &Path, new_name: &str) -> Result<PathBuf, OpError> {
    validate_name(new_name)?;
    let parent = from.parent().unwrap_or(Path::new(""));
    let to = parent.join(new_name);
    if to == from {
        return Ok(to);
    }
    let only_case_differs = from
        .file_name()
        .is_some_and(|old| old.to_string_lossy().to_lowercase() == new_name.to_lowercase());
    if !only_case_differs && fs::symlink_metadata(&to).is_ok() {
        return Err(OpError::AlreadyExists(to));
    }
    fs::rename(from, &to)?;
    Ok(to)
}

/// Creates an empty folder. Fails if anything with that name exists.
pub fn create_folder(parent: &Path, name: &str) -> Result<PathBuf, OpError> {
    validate_name(name)?;
    let path = parent.join(name);
    fs::create_dir(&path).map_err(|err| exists_or_io(err, &path))?;
    Ok(path)
}

/// Creates an empty file. Fails if anything with that name exists.
pub fn create_file(parent: &Path, name: &str) -> Result<PathBuf, OpError> {
    validate_name(name)?;
    let path = parent.join(name);
    fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(&path)
        .map_err(|err| exists_or_io(err, &path))?;
    Ok(path)
}

fn exists_or_io(err: io::Error, path: &Path) -> OpError {
    // Windows reports "access denied" when creating a file where a folder
    // is, so check what's actually there.
    if err.kind() == io::ErrorKind::AlreadyExists || fs::symlink_metadata(path).is_ok() {
        OpError::AlreadyExists(path.to_path_buf())
    } else {
        OpError::Io(err)
    }
}

/// A name in `parent` not taken yet: `base`, then `base (2)`, `base (3)`...
/// with `ext` (no dot, may be empty) kept at the end.
pub fn unique_name(parent: &Path, base: &str, ext: &str) -> String {
    let with_ext = |stem: String| {
        if ext.is_empty() {
            stem
        } else {
            format!("{stem}.{ext}")
        }
    };
    let first = with_ext(base.to_owned());
    if fs::symlink_metadata(parent.join(&first)).is_err() {
        return first;
    }
    (2..)
        .map(|n| with_ext(format!("{base} ({n})")))
        .find(|name| fs::symlink_metadata(parent.join(name)).is_err())
        .expect("ran out of numbers")
}

/// Moves items to the Recycle Bin.
pub fn move_to_trash(paths: &[PathBuf]) -> Result<(), OpError> {
    trash::delete_all(paths).map_err(|err| OpError::Trash(err.to_string()))
}

/// Puts items moved to the Recycle Bin back where they were. When the bin
/// holds several items from the same path, the most recently deleted wins.
pub fn restore_from_trash(paths: &[PathBuf]) -> Result<(), OpError> {
    let trashed = trash::os_limited::list().map_err(|err| OpError::Trash(err.to_string()))?;
    let mut found = Vec::with_capacity(paths.len());
    for path in paths {
        let wanted = comparable(path);
        let item = trashed
            .iter()
            .filter(|item| comparable(&item.original_path()) == wanted)
            .max_by_key(|item| item.time_deleted)
            .ok_or_else(|| {
                OpError::Trash(format!(
                    "“{}” is no longer in the Recycle Bin",
                    path.file_name()
                        .unwrap_or(path.as_os_str())
                        .to_string_lossy()
                ))
            })?;
        found.push(item.clone());
    }
    trash::os_limited::restore_all(found).map_err(|err| OpError::Trash(err.to_string()))
}

/// A form of `path` that compares equal however it was spelled: the same
/// folder can appear as `C:\Users\RUNNER~1` or `C:\Users\runneradmin`, in
/// any case. The item itself is gone, so only its parent is resolved.
fn comparable(path: &Path) -> PathBuf {
    let resolved = match (path.parent(), path.file_name()) {
        (Some(parent), Some(name)) => fs::canonicalize(parent)
            .map(|p| p.join(name))
            .unwrap_or_else(|_| path.to_path_buf()),
        _ => path.to_path_buf(),
    };
    if cfg!(windows) {
        PathBuf::from(resolved.to_string_lossy().to_lowercase())
    } else {
        resolved
    }
}

/// Deletes items for good (Shift Del). Folders go with everything inside.
pub fn delete_permanently(paths: &[PathBuf]) -> Result<(), OpError> {
    for path in paths {
        let metadata = fs::symlink_metadata(path)?;
        if metadata.is_dir() {
            fs::remove_dir_all(path)?;
        } else {
            fs::remove_file(path)?;
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ordinary_names_are_fine() {
        for name in [
            "Cargo.toml",
            ".gitignore",
            "a b c",
            "été 2026",
            "x".repeat(255).as_str(),
            "CONSOLE.txt",
            "COM10",
        ] {
            assert_eq!(validate_name(name), Ok(()), "{name}");
        }
    }

    #[test]
    fn reserved_device_names_are_rejected() {
        assert_eq!(validate_name("CON"), Err(NameError::Reserved("CON".into())));
        assert_eq!(
            validate_name("con.txt"),
            Err(NameError::Reserved("CON".into()))
        );
        assert_eq!(
            validate_name("Lpt9.tar.gz"),
            Err(NameError::Reserved("LPT9".into()))
        );
        assert!(validate_name("com1").is_err());
        assert!(validate_name("COM²").is_err());
        assert!(validate_name("NUL .txt").is_err());
        assert_eq!(
            NameError::Reserved("CON".into()).to_string(),
            "“CON” is reserved by Windows"
        );
    }

    #[test]
    fn bad_characters_are_rejected() {
        assert_eq!(validate_name("a:b"), Err(NameError::ForbiddenChar(':')));
        assert_eq!(validate_name("what?"), Err(NameError::ForbiddenChar('?')));
        assert_eq!(
            validate_name("dir/file"),
            Err(NameError::ForbiddenChar('/'))
        );
        assert_eq!(validate_name("tab\there"), Err(NameError::ControlChar));
    }

    #[test]
    fn bad_shapes_are_rejected() {
        assert_eq!(validate_name(""), Err(NameError::Empty));
        assert_eq!(validate_name("   "), Err(NameError::Empty));
        assert_eq!(validate_name(".."), Err(NameError::DotName));
        assert_eq!(
            validate_name("name."),
            Err(NameError::TrailingSpaceOrPeriod)
        );
        assert_eq!(
            validate_name("name "),
            Err(NameError::TrailingSpaceOrPeriod)
        );
        assert_eq!(validate_name(&"x".repeat(256)), Err(NameError::TooLong));
    }

    #[test]
    fn stem_selection() {
        let stem = |name: &str, dir| name[stem_range(name, dir)].to_owned();
        assert_eq!(stem("release-notes.md", false), "release-notes");
        assert_eq!(stem("archive.tar.gz", false), "archive.tar");
        assert_eq!(stem(".gitignore", false), ".gitignore");
        assert_eq!(stem("LICENSE", false), "LICENSE");
        assert_eq!(stem("v1.2", true), "v1.2");
        assert_eq!(stem("été.txt", false), "été");
    }

    #[test]
    fn rename_moves_the_file() {
        let dir = tempfile::tempdir().unwrap();
        let from = dir.path().join("old.txt");
        fs::write(&from, b"hi").unwrap();

        let to = rename(&from, "new.txt").unwrap();
        assert_eq!(to, dir.path().join("new.txt"));
        assert!(!from.exists());
        assert_eq!(fs::read(&to).unwrap(), b"hi");
    }

    #[test]
    fn rename_never_overwrites() {
        let dir = tempfile::tempdir().unwrap();
        let a = dir.path().join("a.txt");
        let b = dir.path().join("b.txt");
        fs::write(&a, b"a").unwrap();
        fs::write(&b, b"b").unwrap();

        assert!(matches!(
            rename(&a, "b.txt"),
            Err(OpError::AlreadyExists(_))
        ));
        assert_eq!(fs::read(&b).unwrap(), b"b");
        assert!(a.exists());
    }

    #[test]
    fn rename_allows_a_case_change() {
        let dir = tempfile::tempdir().unwrap();
        let from = dir.path().join("readme.md");
        fs::write(&from, b"").unwrap();
        let to = rename(&from, "README.md").unwrap();
        assert!(
            fs::read_dir(dir.path())
                .unwrap()
                .any(|e| e.unwrap().file_name() == "README.md")
        );
        assert_eq!(to.file_name().unwrap(), "README.md");
    }

    #[test]
    fn rename_to_same_name_is_a_no_op() {
        let dir = tempfile::tempdir().unwrap();
        let from = dir.path().join("same.txt");
        fs::write(&from, b"").unwrap();
        assert_eq!(rename(&from, "same.txt").unwrap(), from);
    }

    #[test]
    fn rename_rejects_invalid_names_before_touching_disk() {
        let dir = tempfile::tempdir().unwrap();
        let from = dir.path().join("a.txt");
        fs::write(&from, b"").unwrap();
        assert!(matches!(rename(&from, "CON"), Err(OpError::InvalidName(_))));
        assert!(from.exists());
    }

    #[test]
    fn create_folder_and_file() {
        let dir = tempfile::tempdir().unwrap();
        assert!(create_folder(dir.path(), "New folder").unwrap().is_dir());
        assert!(create_file(dir.path(), "notes.txt").unwrap().is_file());
        assert!(matches!(
            create_folder(dir.path(), "notes.txt"),
            Err(OpError::AlreadyExists(_))
        ));
        assert!(matches!(
            create_file(dir.path(), "New folder"),
            Err(OpError::AlreadyExists(_))
        ));
    }

    #[test]
    fn unique_names_count_up() {
        let dir = tempfile::tempdir().unwrap();
        assert_eq!(unique_name(dir.path(), "New folder", ""), "New folder");
        fs::create_dir(dir.path().join("New folder")).unwrap();
        fs::create_dir(dir.path().join("New folder (2)")).unwrap();
        assert_eq!(unique_name(dir.path(), "New folder", ""), "New folder (3)");

        fs::write(dir.path().join("New file.txt"), b"").unwrap();
        assert_eq!(
            unique_name(dir.path(), "New file", "txt"),
            "New file (2).txt"
        );
    }

    #[test]
    fn delete_permanently_removes_files_and_folders() {
        let dir = tempfile::tempdir().unwrap();
        let file = dir.path().join("f.txt");
        let folder = dir.path().join("sub");
        fs::write(&file, b"").unwrap();
        fs::create_dir(&folder).unwrap();
        fs::write(folder.join("inner.txt"), b"").unwrap();

        delete_permanently(&[file.clone(), folder.clone()]).unwrap();
        assert!(!file.exists());
        assert!(!folder.exists());
    }

    #[test]
    fn delete_permanently_reports_missing_items() {
        let dir = tempfile::tempdir().unwrap();
        assert!(matches!(
            delete_permanently(&[dir.path().join("gone")]),
            Err(OpError::Io(_))
        ));
    }

    #[cfg(windows)]
    #[test]
    fn trash_and_restore() {
        let dir = tempfile::tempdir().unwrap();
        let file = dir.path().join("explorist-trash-test.txt");
        fs::write(&file, b"").unwrap();
        move_to_trash(std::slice::from_ref(&file)).unwrap();
        assert!(!file.exists());
        restore_from_trash(std::slice::from_ref(&file)).unwrap();
        assert!(file.exists());
    }
}
