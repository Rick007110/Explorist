//! File operations started from the UI: clipboard, Recycle Bin, delete,
//! drops, undo, and the background job that carries them out.

use std::path::{Path, PathBuf};

use crate::app::ExploristApp;
use crate::app::undo::{Action, Replaced, Undo};
use crate::fs::clipboard;
use crate::fs::entry::Entry;
use crate::fs::jobs::{Job, Outcome, TransferKind, Work};
use crate::fs::ops;

/// A job plus what to do when it ends.
pub struct RunningJob {
    pub job: Job,
    /// For undo jobs, the toast to show on success; their result isn't
    /// itself undoable.
    undo_message: Option<String>,
    /// Cut items: clear the clipboard after a successful paste.
    from_cut: bool,
    /// Recycle Bin items to put back once this (undo) job is done.
    restore: Vec<trash::TrashItem>,
}

impl ExploristApp {
    pub(crate) fn selected_paths(&self) -> Vec<PathBuf> {
        self.browser
            .selected_entries()
            .iter()
            .map(|e| e.path.clone())
            .collect()
    }

    pub(crate) fn clip_selection(&mut self, cut: bool) {
        let paths = self.selected_paths();
        if paths.is_empty() {
            return;
        }
        match clipboard::put(&paths, cut) {
            Ok(()) => {
                self.view.cut = if cut {
                    paths.iter().cloned().collect()
                } else {
                    Default::default()
                };
                let what = count(paths.len());
                self.toast(
                    if cut {
                        format!("Cut {what}")
                    } else {
                        format!("Copied {what} to the clipboard")
                    },
                    false,
                );
            }
            Err(err) => self.toast_error(format!("Couldn’t use the clipboard: {err}")),
        }
    }

    pub(crate) fn paste(&mut self) {
        let Some(clip) = clipboard::read() else {
            self.toast("The clipboard has no files".to_owned(), false);
            return;
        };
        let kind = if clip.cut {
            TransferKind::Move
        } else {
            TransferKind::Copy
        };
        let dest = self.browser.location().to_path_buf();
        self.transfer(kind, &clip.paths, &dest, clip.cut);
    }

    /// Copies or moves `sources` into `dest` in the background.
    pub(crate) fn transfer(
        &mut self,
        kind: TransferKind,
        sources: &[PathBuf],
        dest: &Path,
        from_cut: bool,
    ) {
        match Work::transfer(kind, sources, dest) {
            Ok(work) if work.item_count() == 0 => {}
            Ok(work) => self.start_job(work, None, from_cut),
            Err(message) => self.toast_error(message),
        }
    }

    /// Keeps chosen folder icons with folders that were renamed or moved:
    /// `(old path, new path)` pairs.
    pub(crate) fn carry_folder_icons(&mut self, pairs: &[(PathBuf, PathBuf)]) {
        if self.settings.folder_icons.is_empty() || pairs.is_empty() {
            return;
        }
        for (from, to) in pairs {
            self.settings.move_folder_icons(from, to);
        }
        self.save_settings();
    }

    /// Ctrl D: copies of the selection, beside it.
    pub(crate) fn duplicate_selection(&mut self) {
        let paths = self.selected_paths();
        let here = self.browser.location().to_path_buf();
        self.transfer(TransferKind::Copy, &paths, &here, false);
    }

    pub(crate) fn compress_selection(&mut self) {
        let paths = self.selected_paths();
        if let Some(work) = Work::compress(&paths) {
            self.start_job(work, None, false);
        }
    }

    /// Unpacks the first selected archive.
    pub(crate) fn extract_selection(&mut self) {
        let archive = self
            .selected_paths()
            .into_iter()
            .find(|p| crate::fs::archive::is_archive(p));
        match archive.and_then(|a| Work::extract(&a)) {
            Some(work) => self.start_job(work, None, false),
            None => self.toast("Select a .zip, .tar or .tar.gz file".to_owned(), false),
        }
    }

    /// Finder's "New Folder with Selection": a new folder holding the
    /// selection, named straight away.
    pub(crate) fn new_folder_with_selection(&mut self) {
        let paths = self.selected_paths();
        if paths.is_empty() {
            return;
        }
        // The move runs as a job; with one already running it would be
        // refused, leaving an empty folder behind.
        if self.job.is_some() {
            self.toast("Wait for the current operation to finish".to_owned(), false);
            return;
        }
        let here = self.browser.location().to_path_buf();
        let name = crate::fs::ops::unique_name(&here, "New folder with items", "");
        match crate::fs::ops::create_folder(&here, &name) {
            Ok(folder) => {
                self.push_undo(Action::Create {
                    path: folder.clone(),
                    is_dir: true,
                });
                self.view.rename_after_job = Some(folder.clone());
                self.transfer(TransferKind::Move, &paths, &folder, false);
            }
            Err(err) => self.toast_error(format!("Couldn’t create the folder: {err}")),
        }
    }

    pub(crate) fn trash_selection(&mut self) {
        let paths = self.selected_paths();
        if !paths.is_empty() {
            self.start_job(Work::Trash(paths), None, false);
        }
    }

    pub(crate) fn delete_permanently(&mut self, paths: Vec<PathBuf>) {
        self.start_job(Work::Delete(paths), None, false);
    }

    pub(crate) fn undo(&mut self) {
        if self.job.is_some() {
            self.toast("Wait for the current operation to finish".to_owned(), false);
            return;
        }
        let Some(action) = self.undo.pop() else {
            self.toast("Nothing to undo".to_owned(), false);
            return;
        };
        match action.begin_undo() {
            Undo::Job(work, message, restore) => {
                self.start_job(work, Some(message), false);
                if let Some(running) = &mut self.job {
                    running.restore = restore;
                }
            }
            Undo::Done(Ok(message)) => {
                self.toast(message, false);
                // Folder icons go back with the names.
                match &action {
                    Action::Rename { from, to } => {
                        self.carry_folder_icons(&[(to.clone(), from.clone())]);
                    }
                    Action::BatchRename { pairs } => {
                        let back: Vec<_> =
                            pairs.iter().map(|(a, b)| (b.clone(), a.clone())).collect();
                        self.carry_folder_icons(&back);
                    }
                    _ => {}
                }
                // Re-read first: refresh replaces any pending selection.
                self.browser.refresh();
                let reselect = match &action {
                    Action::Rename { from, .. } => Some(from.clone()),
                    Action::Trash { paths } => paths.first().cloned(),
                    _ => None,
                };
                if let Some(path) = reselect {
                    self.browser.select_when_loaded(path);
                }
            }
            Undo::Done(Err(err)) => self.toast_error(format!("Couldn’t undo: {err}")),
        }
    }

    fn start_job(&mut self, work: Work, undo_message: Option<String>, from_cut: bool) {
        if self.job.is_some() {
            self.toast("Wait for the current operation to finish".to_owned(), false);
            return;
        }
        let ctx = self.ctx.clone();
        self.job = Some(RunningJob {
            job: Job::start(work, move || ctx.request_repaint()),
            undo_message,
            from_cut,
            restore: Vec::new(),
        });
    }

    /// Checks on the running job and wraps it up when it ends.
    pub(crate) fn poll_job(&mut self) {
        let Some(running) = &mut self.job else { return };
        let Some(outcome) = running.job.poll() else {
            return;
        };
        let running = self.job.take().expect("checked above");
        self.finish_job(running, outcome);
    }

    fn finish_job(&mut self, running: RunningJob, outcome: Outcome) {
        // Free space changed.
        self.refresh_drives_soon();
        // Moved folders keep their icons.
        if matches!(
            running.job.work,
            Work::Transfer {
                kind: TransferKind::Move,
                ..
            }
        ) {
            self.carry_folder_icons(&outcome.done);
        }
        // Deleted for good: a new folder with the same name starts plain.
        // (Trashed folders keep theirs, for when they're put back.)
        if matches!(running.job.work, Work::Delete(_)) && !self.settings.folder_icons.is_empty() {
            let mut forgot = false;
            for (path, _) in &outcome.done {
                forgot |= self.settings.forget_folder_icons(path);
            }
            if forgot {
                self.save_settings();
                crate::ui::image_icon::prune(&self.settings);
            }
        }
        let here = self.browser.location().to_path_buf();
        let inside = |p: &Path| p.parent() == Some(here.as_path());

        // Show the result right away rather than waiting for the watcher.
        let (removed, added): (Vec<PathBuf>, Vec<PathBuf>) = match &running.job.work {
            Work::Transfer {
                kind: TransferKind::Copy,
                ..
            } => (
                Vec::new(),
                outcome.done.iter().map(|(_, to)| to.clone()).collect(),
            ),
            Work::Transfer {
                kind: TransferKind::Move,
                ..
            } => (
                outcome
                    .done
                    .iter()
                    .map(|(from, _)| from.clone())
                    // Folders emptied into one of the same name are gone.
                    .chain(
                        outcome
                            .merged
                            .iter()
                            .filter(|(from, _)| !from.exists())
                            .map(|(from, _)| from.clone()),
                    )
                    .collect(),
                outcome.done.iter().map(|(_, to)| to.clone()).collect(),
            ),
            Work::Trash(_) | Work::Delete(_) => (
                outcome.done.iter().map(|(p, _)| p.clone()).collect(),
                Vec::new(),
            ),
            Work::Compress { .. } | Work::Extract { .. } => (
                Vec::new(),
                outcome.done.iter().map(|(_, to)| to.clone()).collect(),
            ),
        };
        let next_focus = self.row_after_removal(&removed);
        let removed_here: Vec<PathBuf> = removed.into_iter().filter(|p| inside(p)).collect();
        let added_here: Vec<Entry> = added
            .iter()
            .filter(|p| inside(p))
            .filter_map(|p| Entry::from_path(p).ok())
            .collect();
        let new_paths: Vec<PathBuf> = added_here.iter().map(|e| e.path.clone()).collect();
        self.browser.apply_change(&removed_here, added_here);
        if let Some((first, rest)) = new_paths.split_first() {
            self.browser.selection.select_only(first);
            for path in rest {
                let row = self.browser.rows().position(path);
                if let Some(row) = row {
                    self.browser.select(|sel, rows| sel.toggle(rows, row));
                }
            }
            self.view.scroll_to_focus = true;
        } else if let Some((_, into)) = outcome.merged.iter().find(|(_, into)| inside(into)) {
            // Everything went into a folder that was here already.
            self.browser.selection.select_only(into);
            self.view.scroll_to_focus = true;
        } else if let Some(next) = next_focus {
            self.browser.selection.select_only(&next);
        }

        if running.from_cut && !outcome.done.is_empty() {
            clipboard::clear();
            self.view.cut.clear();
        }

        // Record undo and say what happened.
        let replaced = Replaced {
            paths: outcome.replaced.clone(),
            in_bin: outcome.replaced_in_bin.clone(),
        };
        let action = match running.job.work {
            _ if running.undo_message.is_some()
                || (outcome.done.is_empty() && outcome.replaced.is_empty()) =>
            {
                None
            }
            Work::Transfer {
                kind: TransferKind::Copy,
                ..
            } => Some(Action::Copy {
                created: outcome.done.iter().map(|(_, to)| to.clone()).collect(),
                replaced,
            }),
            Work::Transfer {
                kind: TransferKind::Move,
                ..
            } => Some(Action::Move {
                pairs: outcome.done.clone(),
                replaced,
            }),
            Work::Trash(_) => Some(Action::Trash {
                paths: outcome.done.iter().map(|(p, _)| p.clone()).collect(),
            }),
            Work::Delete(_) => None,
            Work::Compress { .. } | Work::Extract { .. } => {
                outcome.done.first().map(|(_, to)| Action::Archive {
                    created: to.clone(),
                    extracted: matches!(running.job.work, Work::Extract { .. }),
                })
            }
        };
        let transfer = matches!(running.job.work, Work::Transfer { .. });
        let skipped = match outcome.skipped {
            0 => String::new(),
            n => format!(", skipped {} already there", count(n)),
        };
        if let Some((path, err)) = outcome.failed.first() {
            let name = path
                .file_name()
                .map(|n| n.to_string_lossy().into_owned())
                .unwrap_or_default();
            let message = match outcome.failed.len() {
                1 => format!("“{name}” failed: {err}"),
                n => format!("{n} items failed. First, “{name}”: {err}"),
            };
            self.toast_error(message);
        } else if outcome.cancelled && outcome.done.is_empty() && action.is_none() {
            self.toast("Cancelled".to_owned(), false);
        } else if outcome.cancelled {
            self.toast(
                format!("Stopped after {}", count(outcome.done.len())),
                action.is_some(),
            );
        } else if let Some(message) = running.undo_message {
            // Then what the undone action replaced comes back.
            match running.restore.as_slice() {
                [] => self.toast(message, false),
                items => match ops::restore_trashed(items) {
                    Ok(()) => {
                        self.browser.refresh();
                        self.toast(message, false);
                    }
                    Err(err) => self.toast_error(format!(
                        "Couldn’t put back the replaced files: {err}. They’re in the Recycle Bin."
                    )),
                },
            }
        } else if let Some(action) = &action {
            self.toast(format!("{}{skipped}", action.done_message()), true);
        } else if transfer && outcome.skipped > 0 {
            self.toast(
                format!(
                    "Nothing new: skipped {} already there",
                    count(outcome.skipped)
                ),
                false,
            );
        } else if transfer {
            self.toast("Nothing to copy".to_owned(), false);
        } else {
            self.toast(format!("Deleted {}", count(outcome.done.len())), false);
        }
        if let Some(action) = action {
            self.undo.push(action);
        }
        if let Some(path) = self.view.rename_after_job.take()
            && let Ok(entry) = Entry::from_path(&path)
        {
            self.browser.apply_change(&[], vec![entry.clone()]);
            self.browser.selection.select_only(&entry.path);
            self.begin_rename(&entry);
        }
    }

    /// The row to focus after `paths` disappear: the first one below them.
    pub(crate) fn row_after_removal(&self, paths: &[PathBuf]) -> Option<PathBuf> {
        let rows = self.browser.rows();
        let last = (0..rows.len())
            .rev()
            .find(|&r| rows.get(r).is_some_and(|e| paths.contains(&e.path)))?;
        (last + 1..rows.len())
            .chain((0..last).rev())
            .filter_map(|r| rows.get(r))
            .find(|e| !paths.contains(&e.path))
            .map(|e| e.path.clone())
    }

    /// Files dragged in from outside are copied into the open folder.
    pub(crate) fn handle_dropped_files(&mut self, ctx: &egui::Context) {
        let (hovering, dropped) = ctx.input(|i| {
            (
                !i.raw.hovered_files.is_empty(),
                i.raw
                    .dropped_files
                    .iter()
                    .map(|f| f.path().to_path_buf())
                    .filter(|p| p.is_absolute())
                    .collect::<Vec<_>>(),
            )
        });
        self.view.drop_hover = hovering;
        // A new tab's start page isn't a folder to drop into.
        if self.browser.start_page {
            if !dropped.is_empty() {
                self.toast(
                    "Open a folder first, then drop files into it".to_owned(),
                    false,
                );
            }
            return;
        }
        let dest = self.browser.location().to_path_buf();
        // Dropping files where they already are (say, dragged out and
        // back in) does nothing, as in Explorer.
        let dropped: Vec<PathBuf> = dropped
            .into_iter()
            .filter(|p| p.parent() != Some(dest.as_path()))
            .collect();
        if !dropped.is_empty() {
            self.transfer(TransferKind::Copy, &dropped, &dest, false);
        }
    }
}

fn count(n: usize) -> String {
    if n == 1 {
        "1 item".to_owned()
    } else {
        format!("{n} items")
    }
}
