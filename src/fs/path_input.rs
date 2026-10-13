//! Making sense of a path typed or pasted into the path bar.

use std::fs;
use std::path::{Component, Path, PathBuf};

/// Where a typed path leads.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Target {
    Folder(PathBuf),
    /// A file: open its folder with the file selected.
    File(PathBuf),
}

/// Resolves path-bar input against the current folder.
///
/// Accepts absolute paths, paths relative to `current`, `~` for the home
/// folder, `%VARIABLES%`, surrounding quotes (as "Copy as path" adds),
/// forward slashes and a bare drive like `D:`.
pub fn resolve(input: &str, current: &Path) -> Result<Target, String> {
    let trimmed = input.trim();
    let unquoted = trimmed
        .strip_prefix('"')
        .and_then(|s| s.strip_suffix('"'))
        .unwrap_or(trimmed);
    if unquoted.is_empty() {
        return Err("Type a path".to_owned());
    }

    let expanded = expand_home(&expand_env(unquoted));
    let mut path = PathBuf::from(&expanded);
    if is_bare_drive(&expanded) {
        path.push(std::path::MAIN_SEPARATOR_STR);
    }
    if path.is_relative() {
        path = current.join(path);
    }
    let path = normalize(&path);

    match fs::metadata(&path) {
        Ok(meta) if meta.is_dir() => Ok(Target::Folder(path)),
        Ok(_) => Ok(Target::File(path)),
        Err(_) => Err(format!("Can’t find “{unquoted}”")),
    }
}

/// `D:` means the root of D:, not "the current folder on D:".
fn is_bare_drive(s: &str) -> bool {
    let b = s.as_bytes();
    b.len() == 2 && b[0].is_ascii_alphabetic() && b[1] == b':'
}

fn expand_home(s: &str) -> String {
    let rest = match s.strip_prefix('~') {
        Some(rest) if rest.is_empty() || rest.starts_with(['/', '\\']) => rest,
        _ => return s.to_owned(),
    };
    match dirs::home_dir() {
        Some(home) => format!("{}{rest}", home.display()),
        None => s.to_owned(),
    }
}

/// Replaces `%NAME%` with the environment variable's value. Unknown names
/// are left as written, like the Windows shell does.
fn expand_env(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    let mut rest = s;
    while let Some(start) = rest.find('%') {
        out.push_str(&rest[..start]);
        let after = &rest[start + 1..];
        match after.find('%') {
            Some(end) if end > 0 => {
                let name = &after[..end];
                match std::env::var(name) {
                    Ok(value) => out.push_str(&value),
                    Err(_) => {
                        out.push('%');
                        out.push_str(name);
                        out.push('%');
                    }
                }
                rest = &after[end + 1..];
            }
            _ => {
                out.push('%');
                rest = after;
            }
        }
    }
    out.push_str(rest);
    out
}

/// Removes `.` and `..` without touching the disk, so the result never
/// gets Windows' `\\?\` prefix the way `canonicalize` does.
fn normalize(path: &Path) -> PathBuf {
    let mut out = PathBuf::new();
    for component in path.components() {
        match component {
            Component::CurDir => {}
            Component::ParentDir => {
                // Never climb above the root.
                if out.file_name().is_some() {
                    out.pop();
                }
            }
            other => out.push(other.as_os_str()),
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tree() -> tempfile::TempDir {
        let dir = tempfile::tempdir().unwrap();
        fs::create_dir_all(dir.path().join("code").join("atlas")).unwrap();
        fs::write(dir.path().join("code").join("notes.md"), b"").unwrap();
        dir
    }

    #[test]
    fn absolute_and_relative_folders() {
        let dir = tree();
        let atlas = dir.path().join("code").join("atlas");
        assert_eq!(
            resolve(atlas.to_str().unwrap(), Path::new("/")),
            Ok(Target::Folder(atlas.clone()))
        );
        assert_eq!(
            resolve("code/atlas", dir.path()),
            Ok(Target::Folder(atlas.clone()))
        );
        assert_eq!(
            resolve("  ./code/../code/atlas/  ", dir.path()),
            Ok(Target::Folder(atlas))
        );
    }

    #[test]
    fn files_resolve_to_file_targets() {
        let dir = tree();
        let notes = dir.path().join("code").join("notes.md");
        let quoted = format!("\"{}\"", notes.display());
        assert_eq!(resolve(&quoted, Path::new("/")), Ok(Target::File(notes)));
    }

    #[test]
    fn missing_paths_explain_themselves() {
        let dir = tree();
        assert_eq!(
            resolve("nope", dir.path()),
            Err("Can’t find “nope”".to_owned())
        );
        assert_eq!(resolve("   ", dir.path()), Err("Type a path".to_owned()));
    }

    #[test]
    fn parent_never_climbs_past_the_root() {
        let root = if cfg!(windows) {
            PathBuf::from("C:\\")
        } else {
            PathBuf::from("/")
        };
        assert_eq!(normalize(&root.join("..").join("..")), root);
    }

    #[test]
    fn env_vars_expand() {
        // SAFETY: tests in this module don't read this variable concurrently.
        unsafe { std::env::set_var("EXPLORIST_TEST_DIR", "somewhere") };
        assert_eq!(expand_env("%EXPLORIST_TEST_DIR%\\x"), "somewhere\\x");
        assert_eq!(expand_env("%NOT_SET_EXPLORIST%"), "%NOT_SET_EXPLORIST%");
        assert_eq!(expand_env("100% done"), "100% done");
        assert_eq!(expand_env("%%"), "%%");
    }

    #[test]
    fn tilde_is_home() {
        if let Some(home) = dirs::home_dir() {
            assert_eq!(
                resolve("~", Path::new("/")),
                Ok(Target::Folder(normalize(&home)))
            );
        }
        assert_eq!(expand_home("~user"), "~user");
    }

    #[test]
    fn bare_drives() {
        assert!(is_bare_drive("D:"));
        assert!(!is_bare_drive("D:\\"));
        assert!(!is_bare_drive("DD"));
    }
}
