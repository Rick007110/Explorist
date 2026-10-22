//! Long file operations on a background thread: copy, move, Recycle Bin
//! and permanent delete, with progress and cancel.

use std::fs;
use std::io;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{self, Receiver, RecvTimeoutError, Sender};
use std::thread;
use std::time::{Duration, Instant};

use super::ops;

/// How often progress is reported at most.
const PROGRESS_INTERVAL: Duration = Duration::from_millis(50);

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TransferKind {
    Copy,
    Move,
}

/// What to do about files that are already where others are going.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Existing {
    /// Send the ones there to the Recycle Bin and put the new ones in place.
    Replace,
    /// Leave the ones there, and the new ones where they were.
    Skip,
    /// Put the new ones beside them under a "- Copy" name.
    KeepBoth,
}

/// What a job should do.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Work {
    /// Copy or move each `(from, to)` pair. A destination that exists is
    /// a folder of the same name to put the contents into, or a file of
    /// the same name; the job asks what to do about those.
    Transfer {
        kind: TransferKind,
        pairs: Vec<(PathBuf, PathBuf)>,
    },
    Trash(Vec<PathBuf>),
    Delete(Vec<PathBuf>),
    /// Zip `sources` into `archive`, which mustn't exist.
    Compress {
        sources: Vec<PathBuf>,
        archive: PathBuf,
    },
    /// Unpack `archive` into the new folder `dest`.
    Extract {
        archive: PathBuf,
        dest: PathBuf,
    },
}

impl Work {
    /// Plans copying or moving `sources` into `dest_dir`. Moves within the
    /// same folder are dropped; copies there get a "- Copy" name, as in
    /// Explorer. A folder whose name is taken by a folder goes into it, and
    /// a file whose name is taken by a file waits for the person's choice;
    /// a folder and a file of the same name are both kept. Fails for a
    /// folder moved or copied into itself.
    pub fn transfer(
        kind: TransferKind,
        sources: &[PathBuf],
        dest_dir: &Path,
    ) -> Result<Self, String> {
        let mut pairs = Vec::new();
        let mut taken = Vec::new();
        for src in sources {
            let Some(name) = src.file_name() else {
                continue;
            };
            if dest_dir.starts_with(src) {
                return Err(format!(
                    "“{}” can’t go inside itself",
                    name.to_string_lossy()
                ));
            }
            let same_folder = src.parent() == Some(dest_dir);
            if same_folder && kind == TransferKind::Move {
                continue;
            }
            let name = name.to_string_lossy();
            let plain = dest_dir.join(&*name);
            let dst = if !same_folder && !taken.contains(&plain) && same_kind_or_free(src, &plain) {
                plain
            } else {
                free_name(dest_dir, &name, same_folder, &taken)
            };
            taken.push(dst.clone());
            pairs.push((src.clone(), dst));
        }
        Ok(Self::Transfer { kind, pairs })
    }

    /// Zips `sources` next to the first one, under a name that's free.
    pub fn compress(sources: &[PathBuf]) -> Option<Self> {
        let dir = sources.first()?.parent()?;
        let archive = free_name(dir, &super::archive::zip_name(sources), false, &[]);
        Some(Self::Compress {
            sources: sources.to_vec(),
            archive,
        })
    }

    /// Unpacks `archive` into a new folder beside it, named after it.
    pub fn extract(archive: &Path) -> Option<Self> {
        let dir = archive.parent()?;
        let dest = free_name(dir, &super::archive::stem(archive), false, &[]);
        Some(Self::Extract {
            archive: archive.to_path_buf(),
            dest,
        })
    }

    /// Present-tense label for the status bar: "Copying", "Moving"...
    pub fn verb(&self) -> &'static str {
        match self {
            Self::Transfer {
                kind: TransferKind::Copy,
                ..
            } => "Copying",
            Self::Transfer {
                kind: TransferKind::Move,
                ..
            } => "Moving",
            Self::Trash(_) => "Moving to the Recycle Bin",
            Self::Delete(_) => "Deleting",
            Self::Compress { .. } => "Compressing",
            Self::Extract { .. } => "Extracting",
        }
    }

    pub fn item_count(&self) -> usize {
        match self {
            Self::Transfer { pairs, .. } => pairs.len(),
            Self::Trash(paths) | Self::Delete(paths) => paths.len(),
            Self::Compress { sources, .. } => sources.len(),
            Self::Extract { .. } => 1,
        }
    }
}

/// Whether `dst` is free, or the same kind of thing as `src`: both folders
/// or both files.
fn same_kind_or_free(src: &Path, dst: &Path) -> bool {
    match fs::symlink_metadata(dst) {
        Err(_) => true,
        Ok(there) => fs::symlink_metadata(src).is_ok_and(|from| from.is_dir() == there.is_dir()),
    }
}

/// The files already where `pairs` would put files, looking inside folders
/// of the same name too, as those get merged.
fn clashes(pairs: &[(PathBuf, PathBuf)]) -> Vec<PathBuf> {
    fn walk(src: &Path, dst: &Path, found: &mut Vec<PathBuf>) {
        let (Ok(from), Ok(there)) = (fs::symlink_metadata(src), fs::symlink_metadata(dst)) else {
            return;
        };
        match (from.is_dir(), there.is_dir()) {
            (true, true) => {
                for child in fs::read_dir(src).into_iter().flatten().flatten() {
                    walk(&child.path(), &dst.join(child.file_name()), found);
                }
            }
            (false, false) => found.push(dst.to_path_buf()),
            // A folder and a file: both are kept.
            _ => {}
        }
    }
    let mut found = Vec::new();
    for (src, dst) in pairs {
        walk(src, dst, &mut found);
    }
    found
}

/// A destination path in `dir` that isn't taken: `name`, or for copies
/// into the same folder or onto an existing name, "stem - Copy.ext",
/// "stem - Copy (2).ext"...
fn free_name(dir: &Path, name: &str, force_copy_name: bool, taken: &[PathBuf]) -> PathBuf {
    let free = |p: &Path| fs::symlink_metadata(p).is_err() && !taken.iter().any(|t| t == p);
    let plain = dir.join(name);
    if !force_copy_name && free(&plain) {
        return plain;
    }
    let (stem, ext) = match name.rfind('.') {
        Some(dot) if dot > 0 => (&name[..dot], &name[dot..]),
        _ => (name, ""),
    };
    (1..)
        .map(|n| {
            if n == 1 {
                dir.join(format!("{stem} - Copy{ext}"))
            } else {
                dir.join(format!("{stem} - Copy ({n}){ext}"))
            }
        })
        .find(|p| free(p))
        .expect("ran out of numbers")
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Progress {
    pub items_done: usize,
    pub items_total: usize,
    pub bytes_done: u64,
    /// Zero when sizes weren't measured (moves within a drive, deletes).
    pub bytes_total: u64,
    /// Name of the item being worked on.
    pub current: String,
}

impl Progress {
    /// 0.0–1.0, by bytes when known, else by items.
    pub fn fraction(&self) -> f32 {
        if self.bytes_total > 0 {
            (self.bytes_done as f64 / self.bytes_total as f64) as f32
        } else if self.items_total > 0 {
            self.items_done as f32 / self.items_total as f32
        } else {
            0.0
        }
    }
}

/// How a job ended.
#[derive(Debug, Default, PartialEq, Eq)]
pub struct Outcome {
    /// Pairs fully transferred, or paths trashed/deleted. Inside merged
    /// folders, each item that went in.
    pub done: Vec<(PathBuf, PathBuf)>,
    pub failed: Vec<(PathBuf, String)>,
    pub cancelled: bool,
    /// Folders put into a folder of the same name: `(from, into)`.
    pub merged: Vec<(PathBuf, PathBuf)>,
    /// Files that were replaced, now in the Recycle Bin.
    pub replaced: Vec<PathBuf>,
    /// The Recycle Bin items they became, to put exactly those back.
    pub replaced_in_bin: Vec<trash::TrashItem>,
    /// Files left out because one was already there.
    pub skipped: usize,
}

#[derive(Debug)]
pub enum JobEvent {
    Progress(Progress),
    /// Files already where others are going: the job waits for
    /// [`Job::answer`].
    Existing(Vec<PathBuf>),
    Finished(Outcome),
}

/// A running job. Dropping it doesn't stop it; call [`Job::cancel`].
pub struct Job {
    pub work: Work,
    events: Receiver<JobEvent>,
    answers: Sender<Existing>,
    /// Files already there, while the job waits to hear what to do.
    asking: Option<Vec<PathBuf>>,
    cancelled: Arc<AtomicBool>,
    progress: Progress,
}

impl Job {
    pub fn start(work: Work, wake: impl Fn() + Send + 'static) -> Self {
        let (tx, events) = mpsc::channel();
        let (answers, answer_rx) = mpsc::channel();
        let cancelled = Arc::new(AtomicBool::new(false));
        let mut runner = Runner {
            tx,
            answers: answer_rx,
            cancelled: Arc::clone(&cancelled),
            wake: Box::new(wake),
            progress: Progress {
                items_total: work.item_count(),
                ..Progress::default()
            },
            last_report: Instant::now(),
        };
        let job_work = work.clone();
        thread::Builder::new()
            .name("explorist-job".into())
            .spawn(move || {
                let outcome = runner.run(job_work);
                runner.send(JobEvent::Finished(outcome));
            })
            .expect("failed to spawn the file operation thread");
        Self {
            progress: Progress {
                items_total: work.item_count(),
                ..Progress::default()
            },
            work,
            events,
            answers,
            asking: None,
            cancelled,
        }
    }

    pub fn cancel(&self) {
        self.cancelled.store(true, Ordering::Relaxed);
    }

    /// The files already there, while the job waits for [`Job::answer`].
    pub fn asking(&self) -> Option<&[PathBuf]> {
        self.asking.as_deref()
    }

    /// What to do about the files already there. Can be given before the
    /// job asks.
    pub fn answer(&mut self, choice: Existing) {
        self.asking = None;
        let _ = self.answers.send(choice);
    }

    pub fn progress(&self) -> &Progress {
        &self.progress
    }

    /// Takes the latest progress; returns the outcome once finished.
    pub fn poll(&mut self) -> Option<Outcome> {
        while let Ok(event) = self.events.try_recv() {
            match event {
                JobEvent::Progress(p) => self.progress = p,
                JobEvent::Existing(paths) => self.asking = Some(paths),
                JobEvent::Finished(outcome) => return Some(outcome),
            }
        }
        None
    }

    /// Blocks until the job ends. For tests and tools.
    pub fn wait(mut self) -> Outcome {
        loop {
            match self.events.recv() {
                Ok(JobEvent::Progress(p)) => self.progress = p,
                // Waits for an answer given with `answer` beforehand.
                Ok(JobEvent::Existing(_)) => {}
                Ok(JobEvent::Finished(outcome)) => return outcome,
                Err(_) => {
                    return Outcome {
                        cancelled: true,
                        ..Outcome::default()
                    };
                }
            }
        }
    }
}

struct Runner {
    tx: Sender<JobEvent>,
    answers: Receiver<Existing>,
    cancelled: Arc<AtomicBool>,
    wake: Box<dyn Fn() + Send>,
    progress: Progress,
    last_report: Instant,
}

impl Runner {
    fn send(&self, event: JobEvent) {
        if self.tx.send(event).is_ok() {
            (self.wake)();
        }
    }

    fn report(&mut self, force: bool) {
        if force || self.last_report.elapsed() >= PROGRESS_INTERVAL {
            self.last_report = Instant::now();
            self.send(JobEvent::Progress(self.progress.clone()));
        }
    }

    fn is_cancelled(&self) -> bool {
        self.cancelled.load(Ordering::Relaxed)
    }

    /// Asks what to do about files already there; `None` if cancelled.
    fn ask(&self, existing: Vec<PathBuf>) -> Option<Existing> {
        self.send(JobEvent::Existing(existing));
        loop {
            if self.is_cancelled() {
                return None;
            }
            match self.answers.recv_timeout(Duration::from_millis(100)) {
                Ok(choice) => return Some(choice),
                Err(RecvTimeoutError::Timeout) => {}
                Err(RecvTimeoutError::Disconnected) => return None,
            }
        }
    }

    fn run(&mut self, work: Work) -> Outcome {
        let mut outcome = Outcome::default();
        match work {
            Work::Transfer { kind, pairs } => {
                let existing = clashes(&pairs);
                let mut choice = Existing::KeepBoth;
                if !existing.is_empty() {
                    match self.ask(existing.clone()) {
                        Some(answer) => choice = answer,
                        None => {
                            outcome.cancelled = true;
                            return outcome;
                        }
                    }
                }
                if choice == Existing::Replace {
                    match ops::trash_remembering(&existing) {
                        Ok(items) => {
                            outcome.replaced = existing;
                            outcome.replaced_in_bin = items;
                        }
                        Err(err) => {
                            outcome.failed = existing
                                .into_iter()
                                .map(|path| (path, format!("couldn’t replace it: {err}")))
                                .collect();
                            return outcome;
                        }
                    }
                }
                if kind == TransferKind::Copy {
                    self.progress.bytes_total = pairs.iter().map(|(src, _)| tree_size(src)).sum();
                    self.report(true);
                }
                for (src, dst) in pairs {
                    if self.is_cancelled() {
                        outcome.cancelled = true;
                        break;
                    }
                    self.progress.current = display_name(&src);
                    match self.transfer_item(kind, &src, &dst, choice, &mut outcome) {
                        Ok(()) => {}
                        Err(err) if err.kind() == io::ErrorKind::Interrupted => {
                            outcome.cancelled = true;
                            break;
                        }
                        Err(err) => outcome.failed.push((src, err.to_string())),
                    }
                    self.progress.items_done += 1;
                    self.report(false);
                }
            }
            Work::Trash(paths) => {
                // The Recycle Bin works best in one batch.
                self.progress.current = paths.first().map(|p| display_name(p)).unwrap_or_default();
                self.report(true);
                match ops::move_to_trash(&paths) {
                    Ok(()) => outcome.done = paths.into_iter().map(|p| (p.clone(), p)).collect(),
                    Err(err) => {
                        // Some may have gone before the failure.
                        for path in paths {
                            if path.exists() {
                                outcome.failed.push((path, err.to_string()));
                            } else {
                                outcome.done.push((path.clone(), path));
                            }
                        }
                    }
                }
            }
            Work::Delete(paths) => {
                for path in paths {
                    if self.is_cancelled() {
                        outcome.cancelled = true;
                        break;
                    }
                    self.progress.current = display_name(&path);
                    match ops::delete_permanently(std::slice::from_ref(&path)) {
                        Ok(()) => outcome.done.push((path.clone(), path)),
                        Err(err) => outcome.failed.push((path, err.to_string())),
                    }
                    self.progress.items_done += 1;
                    self.report(false);
                }
            }
            Work::Compress { sources, archive } => {
                self.progress.bytes_total = super::archive::size_of(&sources);
                self.report(true);
                let result = super::archive::compress(&sources, &archive, &mut |done, name| {
                    self.progress.bytes_done = done;
                    self.progress.current = name.to_owned();
                    self.report(false);
                    !self.is_cancelled()
                });
                self.finish_single(&mut outcome, result, archive.clone(), archive);
            }
            Work::Extract { archive, dest } => {
                self.progress.bytes_total = super::archive::unpacked_size(&archive);
                self.report(true);
                let result = super::archive::extract(&archive, &dest, &mut |done, name| {
                    self.progress.bytes_done = done;
                    self.progress.current = name.to_owned();
                    self.report(false);
                    !self.is_cancelled()
                });
                self.finish_single(&mut outcome, result, archive, dest);
            }
        }
        self.report(true);
        outcome
    }

    /// Records how a one-result job (compress, extract) went.
    fn finish_single(
        &mut self,
        outcome: &mut Outcome,
        result: io::Result<()>,
        from: PathBuf,
        to: PathBuf,
    ) {
        match result {
            Ok(()) => outcome.done.push((from, to)),
            Err(err) if err.kind() == io::ErrorKind::Interrupted => outcome.cancelled = true,
            Err(err) => outcome.failed.push((from, err.to_string())),
        }
        self.progress.items_done = self.progress.items_total;
    }

    /// Copies or moves `src` to `dst`. Into a new place it goes whole; into
    /// a folder of the same name, item by item; past a file of the same
    /// name as `choice` says. Records each item that went in `outcome`, so
    /// undo and cancel only ever touch those, never what was there.
    /// Returns `Interrupted` if cancelled.
    fn transfer_item(
        &mut self,
        kind: TransferKind,
        src: &Path,
        dst: &Path,
        choice: Existing,
        outcome: &mut Outcome,
    ) -> io::Result<()> {
        if self.is_cancelled() {
            return Err(io::ErrorKind::Interrupted.into());
        }
        let Ok(there) = fs::symlink_metadata(dst) else {
            let result = match kind {
                TransferKind::Copy => self.copy_tree(src, dst),
                TransferKind::Move => self.move_item(src, dst),
            };
            return match result {
                Ok(()) => {
                    outcome.done.push((src.to_path_buf(), dst.to_path_buf()));
                    Ok(())
                }
                Err(err) if err.kind() == io::ErrorKind::Interrupted => {
                    // Cancelled midway: drop the partial copy, which is new.
                    let _ = ops::delete_permanently(&[dst.to_path_buf()]);
                    Err(err)
                }
                Err(err) => {
                    outcome.failed.push((src.to_path_buf(), err.to_string()));
                    Ok(())
                }
            };
        };
        let from_dir = fs::symlink_metadata(src)?.is_dir();
        if from_dir && there.is_dir() {
            // Only the outermost folder counts as merged.
            if outcome
                .merged
                .iter()
                .all(|(_, into)| !dst.starts_with(into))
            {
                outcome.merged.push((src.to_path_buf(), dst.to_path_buf()));
            }
            for child in fs::read_dir(src)? {
                let child = child?;
                let target = dst.join(child.file_name());
                self.transfer_item(kind, &child.path(), &target, choice, outcome)?;
            }
            if kind == TransferKind::Move {
                // Empty now, unless something was skipped or failed.
                let _ = fs::remove_dir(src);
            }
            return Ok(());
        }
        if from_dir == there.is_dir() && choice == Existing::Skip {
            outcome.skipped += 1;
            return Ok(());
        }
        // Keep both, also for a folder and a file of the same name.
        let dir = dst.parent().unwrap_or(dst);
        let name = display_name(dst);
        let beside = free_name(dir, &name, false, &[]);
        self.transfer_item(kind, src, &beside, choice, outcome)
    }

    /// Copies a file or a whole folder. Returns `Interrupted` if cancelled.
    fn copy_tree(&mut self, src: &Path, dst: &Path) -> io::Result<()> {
        if self.is_cancelled() {
            return Err(io::ErrorKind::Interrupted.into());
        }
        let meta = fs::symlink_metadata(src)?;
        if meta.is_dir() {
            fs::create_dir(dst)?;
            for child in fs::read_dir(src)? {
                let child = child?;
                self.copy_tree(&child.path(), &dst.join(child.file_name()))?;
            }
        } else {
            if fs::symlink_metadata(dst).is_ok() {
                return Err(io::ErrorKind::AlreadyExists.into());
            }
            self.progress.current = display_name(src);
            fs::copy(src, dst)?;
            self.progress.bytes_done += meta.len();
            self.report(false);
        }
        Ok(())
    }

    /// Renames when possible; across drives, copies then removes the source.
    fn move_item(&mut self, src: &Path, dst: &Path) -> io::Result<()> {
        if fs::symlink_metadata(dst).is_ok() {
            return Err(io::ErrorKind::AlreadyExists.into());
        }
        // Moving back out of a merged folder: its old folder may be gone.
        if let Some(parent) = dst.parent() {
            fs::create_dir_all(parent)?;
        }
        match fs::rename(src, dst) {
            Ok(()) => Ok(()),
            Err(err) if err.kind() == io::ErrorKind::CrossesDevices => {
                self.copy_tree(src, dst)?;
                ops::delete_permanently(&[src.to_path_buf()])
                    .map_err(|e| io::Error::other(e.to_string()))
            }
            Err(err) => Err(err),
        }
    }
}

fn tree_size(path: &Path) -> u64 {
    match fs::symlink_metadata(path) {
        Ok(meta) if meta.is_dir() => fs::read_dir(path)
            .map(|dir| dir.flatten().map(|e| tree_size(&e.path())).sum())
            .unwrap_or(0),
        Ok(meta) => meta.len(),
        Err(_) => 0,
    }
}

fn display_name(path: &Path) -> String {
    path.file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_else(|| path.display().to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn run(work: Work) -> Outcome {
        Job::start(work, || {}).wait()
    }

    /// Runs a job that will ask about files already there.
    fn run_answering(work: Work, choice: Existing) -> Outcome {
        let mut job = Job::start(work, || {});
        job.answer(choice);
        job.wait()
    }

    /// `dest/src` already there, holding `a.txt` (a clash) and `keep.txt`.
    fn tree_with_dest_src() -> tempfile::TempDir {
        let dir = tree();
        fs::create_dir(dir.path().join("dest/src")).unwrap();
        fs::write(dir.path().join("dest/src/a.txt"), b"old").unwrap();
        fs::write(dir.path().join("dest/src/keep.txt"), b"k").unwrap();
        dir
    }

    fn tree() -> tempfile::TempDir {
        let dir = tempfile::tempdir().unwrap();
        fs::create_dir_all(dir.path().join("src").join("nested")).unwrap();
        fs::write(dir.path().join("src").join("a.txt"), b"aaaa").unwrap();
        fs::write(dir.path().join("src").join("nested").join("b.txt"), b"bb").unwrap();
        fs::create_dir(dir.path().join("dest")).unwrap();
        fs::write(dir.path().join("notes.md"), b"n").unwrap();
        dir
    }

    #[test]
    fn copies_folders_recursively() {
        let dir = tree();
        let work = Work::transfer(
            TransferKind::Copy,
            &[dir.path().join("src")],
            &dir.path().join("dest"),
        )
        .unwrap();
        let outcome = run(work);
        assert!(outcome.failed.is_empty(), "{outcome:?}");
        assert_eq!(
            fs::read(dir.path().join("dest/src/nested/b.txt")).unwrap(),
            b"bb"
        );
        assert!(
            dir.path().join("src/a.txt").exists(),
            "copy keeps the source"
        );
    }

    #[test]
    fn moves_files_and_folders() {
        let dir = tree();
        let sources = [dir.path().join("src"), dir.path().join("notes.md")];
        let outcome =
            run(Work::transfer(TransferKind::Move, &sources, &dir.path().join("dest")).unwrap());
        assert_eq!(outcome.done.len(), 2);
        assert!(!dir.path().join("src").exists());
        assert!(dir.path().join("dest/src/a.txt").exists());
        assert!(dir.path().join("dest/notes.md").exists());
    }

    #[test]
    fn copies_into_the_same_folder_get_copy_names() {
        let dir = tree();
        let src = dir.path().join("notes.md");
        let work =
            Work::transfer(TransferKind::Copy, &[src.clone(), src.clone()], dir.path()).unwrap();
        let Work::Transfer { pairs, .. } = &work else {
            unreachable!()
        };
        let names: Vec<_> = pairs
            .iter()
            .map(|(_, d)| d.file_name().unwrap().to_string_lossy().into_owned())
            .collect();
        assert_eq!(names, ["notes - Copy.md", "notes - Copy (2).md"]);
        run(work);
        assert!(dir.path().join("notes - Copy (2).md").exists());
    }

    #[test]
    fn existing_names_are_kept_not_overwritten() {
        let dir = tree();
        fs::write(dir.path().join("dest/notes.md"), b"keep me").unwrap();
        let work = Work::transfer(
            TransferKind::Copy,
            &[dir.path().join("notes.md")],
            &dir.path().join("dest"),
        )
        .unwrap();
        run_answering(work, Existing::KeepBoth);
        assert_eq!(
            fs::read(dir.path().join("dest/notes.md")).unwrap(),
            b"keep me"
        );
        assert!(dir.path().join("dest/notes - Copy.md").exists());
    }

    #[test]
    fn folders_go_into_folders_of_the_same_name() {
        let dir = tree_with_dest_src();
        let work = Work::transfer(
            TransferKind::Copy,
            &[dir.path().join("src")],
            &dir.path().join("dest"),
        )
        .unwrap();
        let Work::Transfer { pairs, .. } = &work else {
            unreachable!()
        };
        assert_eq!(pairs[0].1, dir.path().join("dest/src"), "not a copy name");

        let mut job = Job::start(work, || {});
        let asked = loop {
            job.poll();
            if let Some(paths) = job.asking() {
                break paths.to_vec();
            }
            std::thread::sleep(Duration::from_millis(5));
        };
        assert_eq!(asked, [dir.path().join("dest/src/a.txt")]);
        job.answer(Existing::Skip);
        let outcome = job.wait();

        let dest = dir.path().join("dest/src");
        assert_eq!(fs::read(dest.join("a.txt")).unwrap(), b"old", "skipped");
        assert_eq!(fs::read(dest.join("keep.txt")).unwrap(), b"k", "untouched");
        assert_eq!(fs::read(dest.join("nested/b.txt")).unwrap(), b"bb");
        assert!(!dir.path().join("dest/src - Copy").exists());
        assert_eq!(outcome.skipped, 1);
        assert_eq!(outcome.merged, [(dir.path().join("src"), dest.clone())]);
        // Only what went in counts as done, so undo never takes the
        // folder that was there.
        assert_eq!(
            outcome.done,
            [(dir.path().join("src/nested"), dest.join("nested"))]
        );
    }

    #[test]
    fn keep_both_inside_a_merged_folder() {
        let dir = tree_with_dest_src();
        let work = Work::transfer(
            TransferKind::Copy,
            &[dir.path().join("src")],
            &dir.path().join("dest"),
        )
        .unwrap();
        run_answering(work, Existing::KeepBoth);
        let dest = dir.path().join("dest/src");
        assert_eq!(fs::read(dest.join("a.txt")).unwrap(), b"old");
        assert_eq!(fs::read(dest.join("a - Copy.txt")).unwrap(), b"aaaa");
    }

    #[test]
    fn moving_into_a_folder_of_the_same_name_and_back() {
        let dir = tree_with_dest_src();
        let src = dir.path().join("src");
        let work = Work::transfer(
            TransferKind::Move,
            std::slice::from_ref(&src),
            &dir.path().join("dest"),
        )
        .unwrap();
        let outcome = run_answering(work, Existing::KeepBoth);
        assert!(outcome.failed.is_empty(), "{outcome:?}");
        assert!(!src.exists(), "the emptied folder is gone");
        let dest = dir.path().join("dest/src");
        assert!(dest.join("nested/b.txt").exists());
        assert!(dest.join("a - Copy.txt").exists());

        // Undo moves each item back, making the folder again.
        let back: Vec<_> = outcome
            .done
            .iter()
            .map(|(a, b)| (b.clone(), a.clone()))
            .collect();
        let undone = run(Work::Transfer {
            kind: TransferKind::Move,
            pairs: back,
        });
        assert!(undone.failed.is_empty(), "{undone:?}");
        assert_eq!(fs::read(src.join("a.txt")).unwrap(), b"aaaa");
        assert!(src.join("nested/b.txt").exists());
        assert_eq!(fs::read(dest.join("a.txt")).unwrap(), b"old");
        assert!(dest.join("keep.txt").exists());
    }

    #[test]
    fn cancelling_the_question_changes_nothing() {
        let dir = tree_with_dest_src();
        let job = Job::start(
            Work::transfer(
                TransferKind::Move,
                &[dir.path().join("src")],
                &dir.path().join("dest"),
            )
            .unwrap(),
            || {},
        );
        job.cancel();
        let outcome = job.wait();
        assert!(outcome.cancelled);
        assert!(outcome.done.is_empty());
        assert!(dir.path().join("src/a.txt").exists());
        assert_eq!(fs::read(dir.path().join("dest/src/a.txt")).unwrap(), b"old");
    }

    #[test]
    fn a_folder_and_a_file_of_the_same_name_are_both_kept() {
        let dir = tree();
        fs::write(dir.path().join("dest/src"), b"a file").unwrap();
        let work = Work::transfer(
            TransferKind::Copy,
            &[dir.path().join("src")],
            &dir.path().join("dest"),
        )
        .unwrap();
        let outcome = run(work);
        assert!(outcome.failed.is_empty(), "{outcome:?}");
        assert_eq!(fs::read(dir.path().join("dest/src")).unwrap(), b"a file");
        assert!(dir.path().join("dest/src - Copy/a.txt").exists());
    }

    #[test]
    fn moving_within_a_folder_does_nothing() {
        let dir = tree();
        let work = Work::transfer(
            TransferKind::Move,
            &[dir.path().join("notes.md")],
            dir.path(),
        )
        .unwrap();
        assert_eq!(work.item_count(), 0);
    }

    #[test]
    fn a_folder_cannot_go_inside_itself() {
        let dir = tree();
        let src = dir.path().join("src");
        assert!(
            Work::transfer(
                TransferKind::Move,
                std::slice::from_ref(&src),
                &src.join("nested")
            )
            .is_err()
        );
        assert!(Work::transfer(TransferKind::Copy, std::slice::from_ref(&src), &src).is_err());
    }

    #[test]
    fn progress_counts_bytes_for_copies() {
        let dir = tree();
        let work = Work::transfer(
            TransferKind::Copy,
            &[dir.path().join("src")],
            &dir.path().join("dest"),
        )
        .unwrap();
        let mut job = Job::start(work, || {});
        let outcome = loop {
            if let Some(outcome) = job.poll() {
                break outcome;
            }
            std::thread::sleep(Duration::from_millis(5));
        };
        assert!(outcome.failed.is_empty());
        assert_eq!(job.progress().bytes_total, 6);
        assert_eq!(job.progress().bytes_done, 6);
        assert_eq!(job.progress().fraction(), 1.0);
    }

    #[test]
    fn cancel_stops_and_cleans_up() {
        let dir = tempfile::tempdir().unwrap();
        let sources: Vec<PathBuf> = (0..200)
            .map(|i| {
                let p = dir.path().join(format!("f{i}.bin"));
                fs::write(&p, vec![0u8; 1024]).unwrap();
                p
            })
            .collect();
        fs::create_dir(dir.path().join("dest")).unwrap();
        let job = Job::start(
            Work::transfer(TransferKind::Copy, &sources, &dir.path().join("dest")).unwrap(),
            || {},
        );
        job.cancel();
        let outcome = job.wait();
        let copied = fs::read_dir(dir.path().join("dest")).unwrap().count();
        assert_eq!(copied, outcome.done.len(), "no partial leftovers");
        assert!(outcome.cancelled || outcome.done.len() == 200);
    }

    #[test]
    fn delete_reports_each_item() {
        let dir = tree();
        let paths = vec![dir.path().join("src"), dir.path().join("missing")];
        let outcome = run(Work::Delete(paths));
        assert_eq!(outcome.done.len(), 1);
        assert_eq!(outcome.failed.len(), 1);
        assert!(!dir.path().join("src").exists());
    }

    #[test]
    fn compress_and_extract_as_jobs() {
        let dir = tree();
        let src = dir.path().join("src");
        let work = Work::compress(std::slice::from_ref(&src)).unwrap();
        let Work::Compress { archive, .. } = &work else {
            panic!("not a compress job")
        };
        assert_eq!(archive, &dir.path().join("src.zip"));
        let outcome = run(work.clone());
        assert_eq!(outcome.done.len(), 1, "{outcome:?}");

        // A second one doesn't overwrite the first.
        let Work::Compress {
            archive: second, ..
        } = Work::compress(&[src]).unwrap()
        else {
            panic!("not a compress job")
        };
        assert_eq!(second, dir.path().join("src - Copy.zip"));

        let extract = Work::extract(&dir.path().join("src.zip")).unwrap();
        let Work::Extract { dest, .. } = &extract else {
            panic!("not an extract job")
        };
        // "src" exists already, so the folder gets another name.
        assert_eq!(dest, &dir.path().join("src - Copy"));
        let dest = dest.clone();
        let outcome = run(extract);
        assert_eq!(outcome.done.len(), 1, "{outcome:?}");
        assert!(dest.join("src").is_dir());
    }
}
