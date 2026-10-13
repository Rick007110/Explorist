//! Undo for file operations (Ctrl Z).
//!
//! Undoable actions never ask for confirmation; they show a toast with an
//! Undo button instead. Permanent delete can't be undone, so it asks first.

use std::fs;
use std::path::{Path, PathBuf};

use crate::fs::jobs::{TransferKind, Work};
use crate::fs::ops::{self, OpError};

/// How many actions are remembered.
const LIMIT: usize = 100;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Action {
    Rename {
        from: PathBuf,
        to: PathBuf,
    },
    Create {
        path: PathBuf,
        is_dir: bool,
    },
    Trash {
        paths: Vec<PathBuf>,
    },
    /// Copies made; undo sends them to the Recycle Bin.
    Copy {
        created: Vec<PathBuf>,
    },
    /// `(from, to)` moves; undo moves them back.
    Move {
        pairs: Vec<(PathBuf, PathBuf)>,
    },
}

/// What undoing takes.
#[derive(Debug)]
pub enum Undo {
    /// Done already, with the toast text or the error.
    Done(Result<String, OpError>),
    /// Needs a background job; the text is the toast for when it finishes.
    Job(Work, String),
}

impl Action {
    /// Toast text after doing the action: "Renamed to “b.txt”".
    pub fn done_message(&self) -> String {
        match self {
            Self::Rename { to, .. } => format!("Renamed to “{}”", file_name(to)),
            Self::Create { path, is_dir: true } => format!("Created folder “{}”", file_name(path)),
            Self::Create { path, .. } => format!("Created “{}”", file_name(path)),
            Self::Trash { paths } => match paths.as_slice() {
                [one] => format!("Moved “{}” to the Recycle Bin", file_name(one)),
                many => format!("Moved {} items to the Recycle Bin", many.len()),
            },
            Self::Copy { created } => format!(
                "Copied {}{}",
                count(created.len()),
                to_folder(created.first())
            ),
            Self::Move { pairs } => format!(
                "Moved {}{}",
                count(pairs.len()),
                to_folder(pairs.first().map(|(_, to)| to))
            ),
        }
    }

    /// Starts undoing: right away for quick actions, or as a job.
    pub fn begin_undo(&self) -> Undo {
        match self {
            Self::Copy { created } => Undo::Job(
                Work::Trash(created.clone()),
                format!("Removed the {}", plural(created.len(), "copy", "copies")),
            ),
            Self::Move { pairs } => Undo::Job(
                Work::Transfer {
                    kind: TransferKind::Move,
                    pairs: pairs
                        .iter()
                        .map(|(from, to)| (to.clone(), from.clone()))
                        .collect(),
                },
                format!("Moved {} back", count(pairs.len())),
            ),
            other => Undo::Done(other.undo()),
        }
    }

    /// Reverses the action. Returns the toast text.
    pub fn undo(&self) -> Result<String, OpError> {
        match self {
            Self::Rename { from, to } => {
                ops::rename(to, &file_name(from))?;
                Ok(format!("Name changed back to “{}”", file_name(from)))
            }
            Self::Create { path, is_dir } => {
                remove_if_untouched(path, *is_dir)?;
                Ok(format!("Removed “{}”", file_name(path)))
            }
            Self::Trash { paths } => {
                ops::restore_from_trash(paths)?;
                Ok(match paths.as_slice() {
                    [one] => format!("Restored “{}”", file_name(one)),
                    many => format!("Restored {} items", many.len()),
                })
            }
            Self::Copy { .. } | Self::Move { .. } => Err(OpError::Io(std::io::Error::other(
                "this undo runs in the background",
            ))),
        }
    }
}

/// Undoing "create" deletes the item, but only while it's still empty, so
/// undo never destroys work.
fn remove_if_untouched(path: &Path, is_dir: bool) -> Result<(), OpError> {
    let changed = || {
        OpError::Io(std::io::Error::other(format!(
            "“{}” isn’t empty any more, so it was kept",
            file_name(path)
        )))
    };
    if is_dir {
        if fs::read_dir(path)?.next().is_some() {
            return Err(changed());
        }
        fs::remove_dir(path)?;
    } else {
        if fs::metadata(path)?.len() > 0 {
            return Err(changed());
        }
        fs::remove_file(path)?;
    }
    Ok(())
}

fn count(n: usize) -> String {
    plural(n, "item", "items")
}

fn plural(n: usize, one: &str, many: &str) -> String {
    if n == 1 {
        format!("1 {one}")
    } else {
        format!("{n} {many}")
    }
}

/// " to “docs”", naming the folder something went into.
fn to_folder(path: Option<&PathBuf>) -> String {
    path.and_then(|p| p.parent())
        .map(|dir| format!(" to “{}”", file_name(dir)))
        .unwrap_or_default()
}

fn file_name(path: &Path) -> String {
    path.file_name()
        .unwrap_or(path.as_os_str())
        .to_string_lossy()
        .into_owned()
}

#[derive(Debug, Default)]
pub struct UndoStack {
    actions: Vec<Action>,
}

impl UndoStack {
    pub fn push(&mut self, action: Action) {
        if self.actions.len() == LIMIT {
            self.actions.remove(0);
        }
        self.actions.push(action);
    }

    pub fn can_undo(&self) -> bool {
        !self.actions.is_empty()
    }

    /// Takes the most recent action to undo. It's gone from the stack
    /// whether or not undoing works, as retrying would fail the same way.
    pub fn pop(&mut self) -> Option<Action> {
        self.actions.pop()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn undo_rename() {
        let dir = tempfile::tempdir().unwrap();
        let from = dir.path().join("a.txt");
        fs::write(&from, b"x").unwrap();
        let to = ops::rename(&from, "b.txt").unwrap();

        let mut stack = UndoStack::default();
        stack.push(Action::Rename {
            from: from.clone(),
            to: to.clone(),
        });
        let Undo::Done(result) = stack.pop().unwrap().begin_undo() else {
            panic!("rename undoes right away");
        };
        assert_eq!(result.unwrap(), "Name changed back to “a.txt”");
        assert!(from.exists());
        assert!(!to.exists());
        assert!(!stack.can_undo());
    }

    #[test]
    fn undo_create_removes_untouched_items() {
        let dir = tempfile::tempdir().unwrap();
        let folder = ops::create_folder(dir.path(), "New folder").unwrap();
        let file = ops::create_file(dir.path(), "New file.txt").unwrap();

        Action::Create {
            path: folder.clone(),
            is_dir: true,
        }
        .undo()
        .unwrap();
        Action::Create {
            path: file.clone(),
            is_dir: false,
        }
        .undo()
        .unwrap();
        assert!(!folder.exists());
        assert!(!file.exists());
    }

    #[test]
    fn undo_create_keeps_items_with_content() {
        let dir = tempfile::tempdir().unwrap();
        let folder = ops::create_folder(dir.path(), "New folder").unwrap();
        fs::write(folder.join("work.txt"), b"").unwrap();
        let file = ops::create_file(dir.path(), "notes.txt").unwrap();
        fs::write(&file, b"important").unwrap();

        let err = Action::Create {
            path: folder.clone(),
            is_dir: true,
        }
        .undo()
        .unwrap_err();
        assert!(err.to_string().contains("kept"));
        assert!(
            Action::Create {
                path: file.clone(),
                is_dir: false
            }
            .undo()
            .is_err()
        );
        assert!(folder.exists());
        assert_eq!(fs::read(&file).unwrap(), b"important");
    }

    #[test]
    fn undo_runs_newest_first_and_stack_is_bounded() {
        let mut stack = UndoStack::default();
        for i in 0..LIMIT + 5 {
            stack.push(Action::Create {
                path: PathBuf::from(format!("/nowhere/{i}")),
                is_dir: true,
            });
        }
        assert_eq!(stack.actions.len(), LIMIT);
        let action = stack.pop().unwrap();
        let result = action.undo();
        assert_eq!(
            action,
            Action::Create {
                path: PathBuf::from(format!("/nowhere/{}", LIMIT + 4)),
                is_dir: true
            }
        );
        assert!(result.is_err(), "path doesn't exist");
        assert_eq!(stack.actions.len(), LIMIT - 1);
    }

    #[test]
    fn copy_and_move_undo_in_the_background() {
        let copy = Action::Copy {
            created: vec![PathBuf::from("dest/a.txt")],
        };
        assert!(
            matches!(copy.begin_undo(), Undo::Job(Work::Trash(paths), _) if paths == [PathBuf::from("dest/a.txt")])
        );

        let moved = Action::Move {
            pairs: vec![(PathBuf::from("a/x"), PathBuf::from("b/x"))],
        };
        let Undo::Job(Work::Transfer { kind, pairs }, message) = moved.begin_undo() else {
            panic!("move undoes as a job");
        };
        assert_eq!(kind, TransferKind::Move);
        assert_eq!(pairs, [(PathBuf::from("b/x"), PathBuf::from("a/x"))]);
        assert_eq!(message, "Moved 1 item back");
        assert_eq!(moved.done_message(), "Moved 1 item to “b”");
    }

    #[test]
    fn messages() {
        let one = Action::Trash {
            paths: vec![PathBuf::from("x/a.txt")],
        };
        let many = Action::Trash {
            paths: vec![PathBuf::from("a"), PathBuf::from("b"), PathBuf::from("c")],
        };
        assert_eq!(one.done_message(), "Moved “a.txt” to the Recycle Bin");
        assert_eq!(many.done_message(), "Moved 3 items to the Recycle Bin");
        let rename = Action::Rename {
            from: PathBuf::from("a"),
            to: PathBuf::from("b.md"),
        };
        assert_eq!(rename.done_message(), "Renamed to “b.md”");
    }
}
