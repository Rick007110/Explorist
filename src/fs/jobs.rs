//! Long file operations on a background thread: copy, move, Recycle Bin
//! and permanent delete, with progress and cancel.

use std::fs;
use std::io;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{self, Receiver, Sender};
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

/// What a job should do.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Work {
    /// Copy or move each `(from, to)` pair. Destinations must not exist.
    Transfer {
        kind: TransferKind,
        pairs: Vec<(PathBuf, PathBuf)>,
    },
    Trash(Vec<PathBuf>),
    Delete(Vec<PathBuf>),
}

impl Work {
    /// Plans copying or moving `sources` into `dest_dir`. Moves within the
    /// same folder are dropped; copies there get a "- Copy" name, as in
    /// Explorer. Fails for a folder moved or copied into itself.
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
            let dst = free_name(dest_dir, &name, same_folder, &taken);
            taken.push(dst.clone());
            pairs.push((src.clone(), dst));
        }
        Ok(Self::Transfer { kind, pairs })
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
        }
    }

    pub fn item_count(&self) -> usize {
        match self {
            Self::Transfer { pairs, .. } => pairs.len(),
            Self::Trash(paths) | Self::Delete(paths) => paths.len(),
        }
    }
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
    /// Pairs fully transferred, or paths trashed/deleted.
    pub done: Vec<(PathBuf, PathBuf)>,
    pub failed: Vec<(PathBuf, String)>,
    pub cancelled: bool,
}

#[derive(Debug)]
pub enum JobEvent {
    Progress(Progress),
    Finished(Outcome),
}

/// A running job. Dropping it doesn't stop it; call [`Job::cancel`].
pub struct Job {
    pub work: Work,
    events: Receiver<JobEvent>,
    cancelled: Arc<AtomicBool>,
    progress: Progress,
}

impl Job {
    pub fn start(work: Work, wake: impl Fn() + Send + 'static) -> Self {
        let (tx, events) = mpsc::channel();
        let cancelled = Arc::new(AtomicBool::new(false));
        let mut runner = Runner {
            tx,
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
            cancelled,
        }
    }

    pub fn cancel(&self) {
        self.cancelled.store(true, Ordering::Relaxed);
    }

    pub fn progress(&self) -> &Progress {
        &self.progress
    }

    /// Takes the latest progress; returns the outcome once finished.
    pub fn poll(&mut self) -> Option<Outcome> {
        while let Ok(event) = self.events.try_recv() {
            match event {
                JobEvent::Progress(p) => self.progress = p,
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

    fn run(&mut self, work: Work) -> Outcome {
        let mut outcome = Outcome::default();
        match work {
            Work::Transfer { kind, pairs } => {
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
                    let result = match kind {
                        TransferKind::Copy => self.copy_tree(&src, &dst),
                        TransferKind::Move => self.move_item(&src, &dst),
                    };
                    match result {
                        Ok(()) => outcome.done.push((src, dst)),
                        Err(err) if err.kind() == io::ErrorKind::Interrupted => {
                            // Cancelled midway: drop the partial copy.
                            let _ = ops::delete_permanently(std::slice::from_ref(&dst));
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
        }
        self.report(true);
        outcome
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
        run(work);
        assert_eq!(
            fs::read(dir.path().join("dest/notes.md")).unwrap(),
            b"keep me"
        );
        assert!(dir.path().join("dest/notes - Copy.md").exists());
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
}
