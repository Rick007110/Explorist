//! Reading a folder on a background thread.
//!
//! Large folders (10,000+ items) arrive in batches so the list fills in
//! while it loads, and the read can be cancelled with Esc or by navigating
//! away.

use std::fmt;
use std::fs;
use std::io;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{self, Receiver, Sender, TryRecvError};
use std::thread;
use std::time::{Duration, Instant};

use super::entry::Entry;

/// Send a batch once it has this many entries...
const BATCH_SIZE: usize = 500;
/// ...or once this much time has passed since the last one.
const BATCH_INTERVAL: Duration = Duration::from_millis(50);

/// Why a folder couldn't be read.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ListError {
    pub kind: ListErrorKind,
    /// The operating system's error code, e.g. 5 for ERROR_ACCESS_DENIED.
    pub os_code: Option<i32>,
    pub message: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ListErrorKind {
    NotFound,
    AccessDenied,
    NotAFolder,
    Other,
}

impl ListError {
    fn from_io(err: &io::Error) -> Self {
        let kind = match err.kind() {
            io::ErrorKind::NotFound => ListErrorKind::NotFound,
            io::ErrorKind::PermissionDenied => ListErrorKind::AccessDenied,
            io::ErrorKind::NotADirectory => ListErrorKind::NotAFolder,
            _ => ListErrorKind::Other,
        };
        Self {
            kind,
            os_code: err.raw_os_error(),
            message: err.to_string(),
        }
    }
}

impl fmt::Display for ListError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.message)
    }
}

/// Something the reader thread reports.
#[derive(Debug)]
pub enum ListingEvent {
    /// More entries, unsorted.
    Batch(Vec<Entry>),
    /// The folder couldn't be opened. Nothing else follows.
    Failed(ListError),
    /// Reading finished. `skipped` counts items that vanished or couldn't be
    /// read while listing.
    Done { skipped: usize },
}

/// A folder being read in the background. Dropping it cancels the read.
pub struct Listing {
    path: PathBuf,
    events: Receiver<ListingEvent>,
    cancelled: Arc<AtomicBool>,
    finished: bool,
}

impl Listing {
    /// Starts reading `path`. `wake` is called from the reader thread after
    /// each event, so the UI can repaint.
    pub fn start(path: PathBuf, wake: impl Fn() + Send + 'static) -> Self {
        let (tx, events) = mpsc::channel();
        let cancelled = Arc::new(AtomicBool::new(false));
        let reader = Reader {
            path: path.clone(),
            tx,
            cancelled: Arc::clone(&cancelled),
            wake: Box::new(wake),
        };
        thread::Builder::new()
            .name("explorist-listing".into())
            .spawn(move || reader.run())
            .expect("failed to spawn the folder reader thread");
        Self {
            path,
            events,
            cancelled,
            finished: false,
        }
    }

    pub fn path(&self) -> &Path {
        &self.path
    }

    /// Stops the read as soon as the thread notices. Entries already sent
    /// stay available from [`Self::poll`].
    pub fn cancel(&self) {
        self.cancelled.store(true, Ordering::Relaxed);
    }

    pub fn is_cancelled(&self) -> bool {
        self.cancelled.load(Ordering::Relaxed)
    }

    /// Whether `Done` or `Failed` has been received, or the thread has gone.
    pub fn is_finished(&self) -> bool {
        self.finished
    }

    /// Takes every event that has arrived so far, without waiting.
    pub fn poll(&mut self) -> Vec<ListingEvent> {
        let mut out = Vec::new();
        loop {
            match self.events.try_recv() {
                Ok(event) => {
                    if matches!(event, ListingEvent::Done { .. } | ListingEvent::Failed(_)) {
                        self.finished = true;
                    }
                    out.push(event);
                }
                Err(TryRecvError::Empty) => break,
                Err(TryRecvError::Disconnected) => {
                    self.finished = true;
                    break;
                }
            }
        }
        out
    }

    /// Waits up to `timeout` for the next event. For tests and tools; the UI
    /// uses [`Self::poll`].
    pub fn recv_timeout(&mut self, timeout: Duration) -> Option<ListingEvent> {
        let event = self.events.recv_timeout(timeout).ok()?;
        if matches!(event, ListingEvent::Done { .. } | ListingEvent::Failed(_)) {
            self.finished = true;
        }
        Some(event)
    }
}

impl Drop for Listing {
    fn drop(&mut self) {
        self.cancel();
    }
}

struct Reader {
    path: PathBuf,
    tx: Sender<ListingEvent>,
    cancelled: Arc<AtomicBool>,
    wake: Box<dyn Fn() + Send>,
}

impl Reader {
    fn run(self) {
        let dir = match fs::read_dir(&self.path) {
            Ok(dir) => dir,
            Err(err) => {
                self.send(ListingEvent::Failed(ListError::from_io(&err)));
                return;
            }
        };

        let mut batch = Vec::with_capacity(BATCH_SIZE);
        let mut last_send = Instant::now();
        let mut skipped = 0;
        for item in dir {
            if self.cancelled.load(Ordering::Relaxed) {
                return;
            }
            match item.and_then(|item| Entry::from_dir_entry(&item)) {
                Ok(entry) => batch.push(entry),
                Err(_) => skipped += 1,
            }
            if batch.len() >= BATCH_SIZE || last_send.elapsed() >= BATCH_INTERVAL {
                if !batch.is_empty() && !self.send(ListingEvent::Batch(std::mem::take(&mut batch)))
                {
                    return;
                }
                last_send = Instant::now();
            }
        }
        if !batch.is_empty() && !self.send(ListingEvent::Batch(batch)) {
            return;
        }
        self.send(ListingEvent::Done { skipped });
    }

    /// Returns false once nobody is listening any more.
    fn send(&self, event: ListingEvent) -> bool {
        let delivered = self.tx.send(event).is_ok();
        (self.wake)();
        delivered
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::AtomicUsize;

    const WAIT: Duration = Duration::from_secs(10);

    /// Reads to the end and returns the entry names plus the final event.
    fn read_all(path: &Path) -> (Vec<String>, ListingEvent) {
        let mut listing = Listing::start(path.to_path_buf(), || {});
        let mut names = Vec::new();
        loop {
            match listing.recv_timeout(WAIT).expect("listing stalled") {
                ListingEvent::Batch(batch) => names.extend(batch.into_iter().map(|e| e.name)),
                last => {
                    assert!(listing.is_finished());
                    names.sort();
                    return (names, last);
                }
            }
        }
    }

    #[test]
    fn reads_every_entry() {
        let dir = tempfile::tempdir().unwrap();
        fs::write(dir.path().join("a.txt"), b"a").unwrap();
        fs::create_dir(dir.path().join("sub")).unwrap();

        let (names, last) = read_all(dir.path());
        assert_eq!(names, ["a.txt", "sub"]);
        assert!(matches!(last, ListingEvent::Done { skipped: 0 }));
    }

    #[test]
    fn empty_folder_finishes_with_no_batches() {
        let dir = tempfile::tempdir().unwrap();
        let (names, last) = read_all(dir.path());
        assert!(names.is_empty());
        assert!(matches!(last, ListingEvent::Done { skipped: 0 }));
    }

    #[test]
    fn large_folders_arrive_in_several_batches() {
        let dir = tempfile::tempdir().unwrap();
        let count = BATCH_SIZE * 3 + 7;
        for i in 0..count {
            fs::write(dir.path().join(format!("file{i}")), b"").unwrap();
        }

        let mut listing = Listing::start(dir.path().to_path_buf(), || {});
        let (mut batches, mut total) = (0, 0);
        while let Some(ListingEvent::Batch(batch)) = listing.recv_timeout(WAIT) {
            assert!(batch.len() <= BATCH_SIZE);
            batches += 1;
            total += batch.len();
        }
        assert_eq!(total, count);
        assert!(batches >= 4);
    }

    #[test]
    fn missing_folder_fails() {
        let dir = tempfile::tempdir().unwrap();
        let (_, last) = read_all(&dir.path().join("nope"));
        let ListingEvent::Failed(err) = last else {
            panic!("expected Failed, got {last:?}");
        };
        assert_eq!(err.kind, ListErrorKind::NotFound);
        assert!(err.os_code.is_some());
    }

    #[test]
    fn a_file_is_not_a_folder() {
        let dir = tempfile::tempdir().unwrap();
        let file = dir.path().join("file.txt");
        fs::write(&file, b"").unwrap();
        let (_, last) = read_all(&file);
        assert!(matches!(
            last,
            ListingEvent::Failed(ListError {
                kind: ListErrorKind::NotAFolder,
                ..
            })
        ));
    }

    #[test]
    fn wake_is_called_per_event() {
        let dir = tempfile::tempdir().unwrap();
        fs::write(dir.path().join("a"), b"").unwrap();
        let wakes = Arc::new(AtomicUsize::new(0));
        let counter = Arc::clone(&wakes);
        let mut listing = Listing::start(dir.path().to_path_buf(), move || {
            counter.fetch_add(1, Ordering::SeqCst);
        });
        while !matches!(
            listing.recv_timeout(WAIT),
            Some(ListingEvent::Done { .. }) | None
        ) {}
        assert_eq!(wakes.load(Ordering::SeqCst), 2);
    }

    #[test]
    fn cancelling_stops_before_done() {
        let dir = tempfile::tempdir().unwrap();
        for i in 0..BATCH_SIZE * 4 {
            fs::write(dir.path().join(format!("f{i}")), b"").unwrap();
        }
        let mut listing = Listing::start(dir.path().to_path_buf(), || {});
        listing.cancel();
        assert!(listing.is_cancelled());
        // The thread may already have sent a batch or two, but once it sees
        // the flag it stops without sending Done, and the channel closes.
        while let Some(event) = listing.recv_timeout(WAIT) {
            if let ListingEvent::Done { .. } = event {
                // Possible only if the whole read beat the cancel; allowed.
                return;
            }
        }
        assert!(listing.poll().is_empty());
        assert!(listing.is_finished());
    }

    #[cfg(unix)]
    #[test]
    fn unreadable_folder_is_access_denied() {
        use std::os::unix::fs::PermissionsExt;
        let dir = tempfile::tempdir().unwrap();
        let locked = dir.path().join("locked");
        fs::create_dir(&locked).unwrap();
        fs::set_permissions(&locked, fs::Permissions::from_mode(0o000)).unwrap();
        // Root ignores permission bits; there is nothing to test then.
        let enforced = fs::read_dir(&locked).is_err();
        let (_, last) = read_all(&locked);
        fs::set_permissions(&locked, fs::Permissions::from_mode(0o755)).unwrap();
        if enforced {
            assert!(matches!(
                last,
                ListingEvent::Failed(ListError {
                    kind: ListErrorKind::AccessDenied,
                    ..
                })
            ));
        }
    }
}
