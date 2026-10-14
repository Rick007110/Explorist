//! Git status for the open folder, from the `git` command line tool.
//! Without git installed (or outside a repository) there is simply no
//! status.

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::mpsc::{self, Receiver};
use std::thread;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum FileState {
    Modified,
    Added,
    Deleted,
    Renamed,
    Untracked,
    Ignored,
    Conflicted,
}

impl FileState {
    /// The one-letter badge shown next to the name.
    pub fn letter(self) -> &'static str {
        match self {
            Self::Modified => "M",
            Self::Added => "A",
            Self::Deleted => "D",
            Self::Renamed => "R",
            Self::Untracked => "?",
            Self::Ignored => "!",
            Self::Conflicted => "U",
        }
    }

    pub fn describe(self) -> &'static str {
        match self {
            Self::Modified => "Modified, uncommitted",
            Self::Added => "Added",
            Self::Deleted => "Deleted",
            Self::Renamed => "Renamed",
            Self::Untracked => "Untracked",
            Self::Ignored => "Ignored",
            Self::Conflicted => "Conflict",
        }
    }

    /// Which state wins when a folder holds several.
    fn weight(self) -> u8 {
        match self {
            Self::Conflicted => 6,
            Self::Modified | Self::Deleted | Self::Renamed => 5,
            Self::Added => 4,
            Self::Untracked => 3,
            Self::Ignored => 1,
        }
    }
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct GitStatus {
    pub root: PathBuf,
    pub branch: Option<String>,
    pub ahead: u32,
    pub behind: u32,
    /// Paths relative to the root, with `/` separators, as git reports
    /// them. Ignored folders end in `/`.
    files: HashMap<String, FileState>,
}

impl GitStatus {
    /// The state to show for `path` (a file, or a folder summarizing what's
    /// inside it). `None` means tracked and unchanged, or outside the repo.
    pub fn state_of(&self, path: &Path, is_dir: bool) -> Option<FileState> {
        let rel = path.strip_prefix(&self.root).ok()?;
        let mut rel = rel.to_string_lossy().replace('\\', "/");
        if rel.is_empty() {
            return None;
        }
        if let Some(state) = self.files.get(&rel) {
            return Some(*state);
        }
        // Inside an ignored folder: ignored too.
        let ignored_parent = self.files.iter().any(|(p, s)| {
            *s == FileState::Ignored && p.ends_with('/') && rel.starts_with(p.as_str())
        });
        if ignored_parent {
            return Some(FileState::Ignored);
        }
        if !is_dir {
            return None;
        }
        rel.push('/');
        if let Some(state) = self.files.get(&rel) {
            return Some(*state);
        }
        self.files
            .iter()
            .filter(|(p, s)| p.starts_with(rel.as_str()) && **s != FileState::Ignored)
            .map(|(_, s)| *s)
            .max_by_key(|s| s.weight())
    }

    /// Changed files (not counting untracked or ignored).
    pub fn changed_count(&self) -> usize {
        self.files
            .values()
            .filter(|s| !matches!(s, FileState::Untracked | FileState::Ignored))
            .count()
    }
}

/// Parses `git status --porcelain=v2 -z --branch --ignored` output.
pub fn parse(root: &Path, output: &[u8]) -> GitStatus {
    let mut status = GitStatus {
        root: root.to_path_buf(),
        ..GitStatus::default()
    };
    let text = String::from_utf8_lossy(output);
    let mut records = text.split('\0').peekable();
    while let Some(record) = records.next() {
        if let Some(head) = record.strip_prefix("# branch.head ") {
            if head != "(detached)" {
                status.branch = Some(head.to_owned());
            }
        } else if let Some(ab) = record.strip_prefix("# branch.ab ") {
            for part in ab.split_whitespace() {
                if let Some(n) = part.strip_prefix('+') {
                    status.ahead = n.parse().unwrap_or(0);
                } else if let Some(n) = part.strip_prefix('-') {
                    status.behind = n.parse().unwrap_or(0);
                }
            }
        } else if let Some(rest) = record.strip_prefix("1 ") {
            // 1 XY sub mH mI mW hH hI path
            let fields: Vec<&str> = rest.splitn(8, ' ').collect();
            if let (Some(xy), Some(path)) = (fields.first(), fields.get(7)) {
                status.files.insert((*path).to_owned(), from_xy(xy));
            }
        } else if let Some(rest) = record.strip_prefix("2 ") {
            // 2 XY sub mH mI mW hH hI Xscore path, then the old path.
            let fields: Vec<&str> = rest.splitn(9, ' ').collect();
            if let Some(path) = fields.get(8) {
                status.files.insert((*path).to_owned(), FileState::Renamed);
            }
            records.next();
        } else if let Some(rest) = record.strip_prefix("u ") {
            let fields: Vec<&str> = rest.splitn(10, ' ').collect();
            if let Some(path) = fields.get(9) {
                status
                    .files
                    .insert((*path).to_owned(), FileState::Conflicted);
            }
        } else if let Some(path) = record.strip_prefix("? ") {
            status.files.insert(path.to_owned(), FileState::Untracked);
        } else if let Some(path) = record.strip_prefix("! ") {
            status.files.insert(path.to_owned(), FileState::Ignored);
        }
    }
    status
}

/// "XY": staged and unstaged state.
fn from_xy(xy: &str) -> FileState {
    let mut chars = xy.chars();
    let (x, y) = (chars.next().unwrap_or('.'), chars.next().unwrap_or('.'));
    match (x, y) {
        ('D', _) | (_, 'D') => FileState::Deleted,
        ('A', '.') => FileState::Added,
        ('R', _) => FileState::Renamed,
        _ => FileState::Modified,
    }
}

/// The repository containing `folder`: the nearest parent with `.git`.
pub fn find_root(folder: &Path) -> Option<PathBuf> {
    folder
        .ancestors()
        .find(|dir| dir.join(".git").exists())
        .map(Path::to_path_buf)
}

/// Runs `git status` for the repository around `folder`, limited to that
/// folder. `None` outside a repository or without git.
pub fn status(folder: &Path) -> Option<GitStatus> {
    let root = find_root(folder)?;
    let mut command = Command::new("git");
    command
        .arg("-C")
        .arg(&root)
        .args([
            "--no-optional-locks",
            "status",
            "--porcelain=v2",
            "-z",
            "--branch",
            "--ignored=matching",
            "--untracked-files=normal",
            "--",
        ])
        .arg(folder);
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        // Don't flash a console window.
        const CREATE_NO_WINDOW: u32 = 0x0800_0000;
        command.creation_flags(CREATE_NO_WINDOW);
    }
    let output = command.output().ok()?;
    if !output.status.success() {
        return None;
    }
    Some(parse(&root, &output.stdout))
}

/// Checks status in the background; one request at a time, latest wins.
#[derive(Default)]
pub struct GitWatcher {
    folder: Option<PathBuf>,
    pending: Option<Receiver<Option<GitStatus>>>,
    current: Option<GitStatus>,
}

impl GitWatcher {
    /// Starts a check for `folder`.
    pub fn refresh(&mut self, folder: &Path, wake: impl Fn() + Send + 'static) {
        if self.folder.as_deref() != Some(folder) {
            self.current = None;
        }
        self.folder = Some(folder.to_path_buf());
        let (tx, rx) = mpsc::channel();
        let folder = folder.to_path_buf();
        thread::Builder::new()
            .name("explorist-git".into())
            .spawn(move || {
                if tx.send(status(&folder)).is_ok() {
                    wake();
                }
            })
            .ok();
        self.pending = Some(rx);
    }

    pub fn clear(&mut self) {
        *self = Self::default();
    }

    /// The latest status for the folder last refreshed.
    pub fn poll(&mut self) -> Option<&GitStatus> {
        if let Some(rx) = &self.pending
            && let Ok(result) = rx.try_recv()
        {
            self.pending = None;
            self.current = result;
        }
        self.current.as_ref()
    }

    pub fn is_checking(&self) -> bool {
        self.pending.is_some()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const SAMPLE: &str = "# branch.oid abc\0# branch.head main\0# branch.upstream origin/main\0# branch.ab +2 -1\0\
1 .M N... 100644 100644 100644 aaa bbb README.md\0\
1 A. N... 000000 100644 100644 000 ccc src/new file.rs\0\
2 R. N... 100644 100644 100644 aaa bbb R100 docs/renamed.md\0docs/old.md\0\
? notes.txt\0\
! target/\0\
u UU N... 100644 100644 100644 100644 a b c conflict.txt\0";

    fn sample() -> GitStatus {
        parse(Path::new("/repo"), SAMPLE.as_bytes())
    }

    #[test]
    fn parses_branch_and_counts() {
        let s = sample();
        assert_eq!(s.branch.as_deref(), Some("main"));
        assert_eq!((s.ahead, s.behind), (2, 1));
        assert_eq!(s.changed_count(), 4);
    }

    #[test]
    fn file_states() {
        let s = sample();
        let at = |p: &str, dir| s.state_of(&Path::new("/repo").join(p), dir);
        assert_eq!(at("README.md", false), Some(FileState::Modified));
        assert_eq!(at("src/new file.rs", false), Some(FileState::Added));
        assert_eq!(at("docs/renamed.md", false), Some(FileState::Renamed));
        assert_eq!(at("notes.txt", false), Some(FileState::Untracked));
        assert_eq!(at("conflict.txt", false), Some(FileState::Conflicted));
        assert_eq!(at("Cargo.toml", false), None, "tracked and clean");
    }

    #[test]
    fn folders_summarize_and_ignored_folders_cover_their_contents() {
        let s = sample();
        let at = |p: &str, dir| s.state_of(&Path::new("/repo").join(p), dir);
        assert_eq!(at("src", true), Some(FileState::Added));
        assert_eq!(at("docs", true), Some(FileState::Renamed));
        assert_eq!(at("target", true), Some(FileState::Ignored));
        assert_eq!(at("target/debug/app.exe", false), Some(FileState::Ignored));
        assert_eq!(at("assets", true), None);
    }

    #[test]
    fn paths_outside_the_repo_have_no_state() {
        assert_eq!(sample().state_of(Path::new("/elsewhere/x"), false), None);
    }

    #[test]
    fn real_repository() {
        let has_git = Command::new("git")
            .arg("--version")
            .output()
            .is_ok_and(|o| o.status.success());
        if !has_git {
            return;
        }
        let dir = tempfile::tempdir().unwrap();
        let root = std::fs::canonicalize(dir.path()).unwrap();
        let git = |args: &[&str]| {
            Command::new("git")
                .arg("-C")
                .arg(&root)
                .args(args)
                .output()
                .unwrap()
        };
        git(&["init", "-q", "-b", "main"]);
        git(&["config", "user.email", "test@example.com"]);
        git(&["config", "user.name", "Test"]);
        std::fs::write(root.join("a.txt"), "1").unwrap();
        std::fs::write(root.join(".gitignore"), "build/\n").unwrap();
        git(&["add", "."]);
        git(&["commit", "-q", "-m", "init"]);
        std::fs::write(root.join("a.txt"), "2").unwrap();
        std::fs::create_dir(root.join("build")).unwrap();
        std::fs::write(root.join("build/out.bin"), "x").unwrap();
        std::fs::write(root.join("new.txt"), "n").unwrap();

        let s = status(&root).expect("status");
        assert_eq!(s.branch.as_deref(), Some("main"));
        assert_eq!(
            s.state_of(&root.join("a.txt"), false),
            Some(FileState::Modified)
        );
        assert_eq!(
            s.state_of(&root.join("new.txt"), false),
            Some(FileState::Untracked)
        );
        assert_eq!(
            s.state_of(&root.join("build"), true),
            Some(FileState::Ignored)
        );
        assert_eq!(s.state_of(&root.join(".gitignore"), false), None);
    }
}
