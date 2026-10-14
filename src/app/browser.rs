//! The open folder: where we are, how we got here, and what's in it.

use std::collections::HashSet;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::{Duration, Instant};

use crate::app::selection::{self, Rows, Selection};
use crate::fs::entry::Entry;
use crate::fs::filter::{Filter, Filtered};
use crate::fs::listing::{ListError, Listing, ListingEvent};
use crate::fs::sort::{SortColumn, SortOrder};
use crate::fs::watch::{Changes, FolderWatcher};

/// Outside changes are applied once things have been quiet this long, so a
/// burst (an unzip, a build) is handled in one go.
const SETTLE: Duration = Duration::from_millis(200);

/// Called from the reader thread when there's something new to show.
pub type Wake = Arc<dyn Fn() + Send + Sync>;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum LoadState {
    /// Reading; `read` items so far.
    Loading {
        read: usize,
    },
    /// Done. `cancelled` if the user pressed Esc, so the list is partial.
    Ready {
        skipped: usize,
        cancelled: bool,
    },
    Failed(ListError),
}

pub struct Browser {
    location: PathBuf,
    back: Vec<PathBuf>,
    forward: Vec<PathBuf>,
    listing: Option<Listing>,
    load: LoadState,
    /// Everything in the folder, sorted.
    entries: Vec<Entry>,
    sort: SortOrder,
    filter: Filter,
    view: Filtered,
    pub selection: Selection,
    /// Selected once it shows up, e.g. the folder we just went up out of.
    select_when_loaded: Option<PathBuf>,
    wake: Wake,
    /// Reports changes made by other programs. `None` if the folder can't
    /// be watched (some network drives).
    watcher: Option<FolderWatcher>,
    pending: Changes,
    last_change: Option<Instant>,
    /// Bumped whenever the entries change, so others can tell.
    generation: u64,
}

impl Browser {
    pub fn new(location: PathBuf, sort: SortOrder, show_hidden: bool, wake: Wake) -> Self {
        let mut filter = Filter::default();
        filter.show_hidden = show_hidden;
        let mut browser = Self {
            location: PathBuf::new(),
            back: Vec::new(),
            forward: Vec::new(),
            listing: None,
            load: LoadState::Loading { read: 0 },
            entries: Vec::new(),
            sort,
            filter,
            view: Filtered::default(),
            selection: Selection::default(),
            select_when_loaded: None,
            wake,
            watcher: None,
            pending: Changes::default(),
            last_change: None,
            generation: 0,
        };
        browser.load(location);
        browser
    }

    pub fn location(&self) -> &Path {
        &self.location
    }

    pub fn load_state(&self) -> &LoadState {
        &self.load
    }

    pub fn is_loading(&self) -> bool {
        matches!(self.load, LoadState::Loading { .. })
    }

    pub fn sort(&self) -> SortOrder {
        self.sort
    }

    pub fn filter(&self) -> &Filter {
        &self.filter
    }

    /// Items in the folder, hidden ones included.
    pub fn total(&self) -> usize {
        self.entries.len()
    }

    /// Items the name filter chooses from: everything but hidden items
    /// while those are off. "3 of 12" counts against this.
    pub fn shown_total(&self) -> usize {
        self.entries.len() - self.view.hidden
    }

    /// Hidden items left out of the view.
    pub fn hidden_count(&self) -> usize {
        self.view.hidden
    }

    pub fn rows(&self) -> Rows<'_> {
        Rows {
            entries: &self.entries,
            visible: &self.view.visible,
        }
    }

    /// Changes the selection against the rows as shown.
    pub fn select<R>(&mut self, change: impl FnOnce(&mut Selection, Rows<'_>) -> R) -> R {
        let rows = Rows {
            entries: &self.entries,
            visible: &self.view.visible,
        };
        change(&mut self.selection, rows)
    }

    pub fn selected_entries(&self) -> Vec<&Entry> {
        self.selection.selected_entries(self.rows())
    }

    pub fn can_go_back(&self) -> bool {
        !self.back.is_empty()
    }

    pub fn can_go_forward(&self) -> bool {
        !self.forward.is_empty()
    }

    pub fn can_go_up(&self) -> bool {
        self.location.parent().is_some()
    }

    /// Opens a folder, adding the current one to Back history.
    pub fn navigate(&mut self, path: PathBuf) {
        if path == self.location {
            return self.refresh();
        }
        self.back.push(std::mem::take(&mut self.location));
        self.forward.clear();
        self.load(path);
    }

    pub fn go_back(&mut self) {
        if let Some(path) = self.back.pop() {
            self.forward.push(std::mem::take(&mut self.location));
            self.load(path);
        }
    }

    pub fn go_forward(&mut self) {
        if let Some(path) = self.forward.pop() {
            self.back.push(std::mem::take(&mut self.location));
            self.load(path);
        }
    }

    /// Opens the parent folder with the current one selected.
    pub fn go_up(&mut self) {
        if let Some(parent) = self.location.parent().map(Path::to_path_buf) {
            let came_from = self.location.clone();
            self.navigate(parent);
            self.select_when_loaded = Some(came_from);
        }
    }

    /// Reads the folder again, keeping the selection.
    pub fn refresh(&mut self) {
        let focus = self.selection.focus().map(Path::to_path_buf);
        let location = std::mem::take(&mut self.location);
        self.start_listing(location);
        self.select_when_loaded = focus;
    }

    /// Esc while loading: stop and keep what's been read.
    pub fn cancel_loading(&mut self) {
        if let (Some(listing), LoadState::Loading { .. }) = (&self.listing, &self.load) {
            listing.cancel();
            self.listing = None;
            self.load = LoadState::Ready {
                skipped: 0,
                cancelled: true,
            };
        }
    }

    fn load(&mut self, path: PathBuf) {
        self.filter.clear();
        self.selection.clear();
        self.select_when_loaded = None;
        self.start_listing(path);
    }

    fn start_listing(&mut self, path: PathBuf) {
        // Watch before reading so nothing slips through in between.
        let wake = Arc::clone(&self.wake);
        self.watcher = FolderWatcher::start(&path, move || wake()).ok();
        self.pending = Changes::default();
        self.last_change = None;
        let wake = Arc::clone(&self.wake);
        self.listing = Some(Listing::start(path.clone(), move || wake()));
        self.location = path;
        self.entries.clear();
        self.view = Filtered::default();
        self.load = LoadState::Loading { read: 0 };
    }

    /// Takes in whatever the reader thread and the watcher have sent. Call
    /// once per frame.
    pub fn poll(&mut self) {
        self.poll_watcher();
        self.poll_listing();
    }

    /// When the UI should look again for settled outside changes.
    pub fn repaint_in(&self) -> Option<Duration> {
        let last = self.last_change?;
        Some(SETTLE.saturating_sub(last.elapsed()))
    }

    fn poll_watcher(&mut self) {
        let Some(watcher) = &self.watcher else { return };
        let fresh = watcher.poll();
        if !fresh.is_empty() {
            self.pending.paths.extend(fresh.paths);
            self.pending.rescan |= fresh.rescan;
            self.last_change = Some(Instant::now());
        }
        let settled = self.last_change.is_some_and(|t| t.elapsed() >= SETTLE);
        if !settled || self.is_loading() {
            return;
        }
        let changes = std::mem::take(&mut self.pending);
        self.last_change = None;
        if changes.rescan {
            self.refresh();
            return;
        }
        let mut removed = Vec::new();
        let mut added = Vec::new();
        for path in changes.paths {
            match Entry::from_path(&path) {
                Ok(entry) => added.push(entry),
                Err(_) => removed.push(path),
            }
        }
        self.apply_change(&removed, added);
    }

    fn poll_listing(&mut self) {
        let Some(listing) = &mut self.listing else {
            return;
        };
        let mut changed = false;
        for event in listing.poll() {
            match event {
                ListingEvent::Batch(batch) => {
                    self.entries.extend(batch);
                    self.load = LoadState::Loading {
                        read: self.entries.len(),
                    };
                    changed = true;
                }
                ListingEvent::Done { skipped } => {
                    self.load = LoadState::Ready {
                        skipped,
                        cancelled: false,
                    };
                }
                ListingEvent::Failed(err) => self.load = LoadState::Failed(err),
            }
        }
        if listing.is_finished() {
            self.listing = None;
        }
        if changed {
            self.resort();
        }
        if !self.is_loading() {
            self.apply_pending_selection();
        }
    }

    /// Selects `path` once the current read finishes, e.g. a file whose
    /// folder was opened from the path bar.
    pub fn select_when_loaded(&mut self, path: PathBuf) {
        self.select_when_loaded = Some(path);
        if !self.is_loading() {
            self.apply_pending_selection();
        }
    }

    fn apply_pending_selection(&mut self) {
        let Some(path) = self.select_when_loaded.take() else {
            return;
        };
        if self.rows().position(&path).is_some() {
            self.selection.select_only(&path);
        }
    }

    /// A click on a column header.
    pub fn sort_by(&mut self, column: SortColumn) {
        self.set_sort(self.sort.clicked(column));
    }

    pub fn set_sort(&mut self, sort: SortOrder) {
        self.sort = sort;
        self.resort();
    }

    pub fn set_filter_query(&mut self, query: &str) {
        self.filter.set_query(query);
        self.refilter();
    }

    /// Changes the Type / Size / Modified chips.
    pub fn edit_filter(&mut self, change: impl FnOnce(&mut Filter)) {
        change(&mut self.filter);
        self.refilter();
    }

    /// Clears the name filter and every chip.
    pub fn clear_filter(&mut self) {
        self.filter.clear();
        self.refilter();
    }

    /// Each Type label in the folder (hidden items aside) with how many
    /// items have it, most common first.
    pub fn type_counts(&self) -> Vec<(String, usize)> {
        let mut counts: std::collections::HashMap<&str, usize> = std::collections::HashMap::new();
        for entry in &self.entries {
            if self.filter.show_hidden || !(entry.hidden || entry.system) {
                *counts.entry(&entry.type_label).or_default() += 1;
            }
        }
        let mut list: Vec<(String, usize)> =
            counts.into_iter().map(|(t, n)| (t.to_owned(), n)).collect();
        list.sort_by(|a, b| b.1.cmp(&a.1).then_with(|| a.0.cmp(&b.0)));
        list
    }

    pub fn set_show_hidden(&mut self, show: bool) {
        self.filter.show_hidden = show;
        self.refilter();
    }

    /// Changes whenever the folder's contents change.
    pub fn generation(&self) -> u64 {
        self.generation
    }

    fn resort(&mut self) {
        self.generation += 1;
        self.sort.sort(&mut self.entries);
        self.refilter();
    }

    fn refilter(&mut self) {
        self.view = self.filter.apply(&self.entries);
        self.select(|sel, rows| sel.retain_visible(rows));
    }

    /// Updates the list after a change made by Explorist itself, without
    /// re-reading the folder: `removed` paths go, `added` entries come in
    /// (replacing any with the same path).
    pub fn apply_change(&mut self, removed: &[PathBuf], added: Vec<Entry>) {
        if removed.is_empty() && added.is_empty() {
            return;
        }
        let replaced: HashSet<&Path> = removed
            .iter()
            .map(PathBuf::as_path)
            .chain(added.iter().map(|e| e.path.as_path()))
            .collect();
        self.entries
            .retain(|e| !replaced.contains(e.path.as_path()));
        drop(replaced);
        self.entries.extend(added);
        self.resort();
    }

    /// Row of the entry named `name`, case-insensitively.
    pub fn find(&self, name: &str) -> Option<usize> {
        selection::find_by_name(self.rows(), name)
    }
}

#[cfg(test)]
mod tests {
    use std::fs;
    use std::time::{Duration, Instant};

    use super::*;
    use crate::fs::sort::SortDirection;

    fn browser(path: &Path) -> Browser {
        let mut b = Browser::new(
            path.to_path_buf(),
            SortOrder::default(),
            false,
            Arc::new(|| {}),
        );
        wait(&mut b);
        b
    }

    fn wait(b: &mut Browser) {
        let deadline = Instant::now() + Duration::from_secs(10);
        while b.is_loading() || b.listing.is_some() {
            assert!(Instant::now() < deadline, "listing stalled");
            b.poll();
            std::thread::sleep(Duration::from_millis(2));
        }
    }

    fn names(b: &Browser) -> Vec<String> {
        let rows = b.rows();
        (0..rows.len())
            .map(|r| rows.get(r).unwrap().name.clone())
            .collect()
    }

    fn tree() -> tempfile::TempDir {
        let dir = tempfile::tempdir().unwrap();
        fs::create_dir(dir.path().join("src")).unwrap();
        fs::create_dir(dir.path().join("docs")).unwrap();
        fs::write(dir.path().join("file10.txt"), b"1234567890").unwrap();
        fs::write(dir.path().join("file2.txt"), b"12").unwrap();
        crate::fs::test_support::hidden_file(dir.path(), ".env");
        fs::write(dir.path().join("src").join("main.rs"), b"").unwrap();
        dir
    }

    #[test]
    fn loads_sorted_with_hidden_items_left_out() {
        let dir = tree();
        let b = browser(dir.path());
        assert_eq!(names(&b), ["docs", "src", "file2.txt", "file10.txt"]);
        assert_eq!(b.total(), 5);
        assert_eq!(b.shown_total(), 4);
        assert_eq!(b.hidden_count(), 1);
        assert_eq!(
            b.load_state(),
            &LoadState::Ready {
                skipped: 0,
                cancelled: false
            }
        );
    }

    #[test]
    fn history_back_forward_and_up() {
        let dir = tree();
        let mut b = browser(dir.path());
        assert!(!b.can_go_back());

        b.navigate(dir.path().join("src"));
        wait(&mut b);
        assert_eq!(names(&b), ["main.rs"]);
        assert!(b.can_go_back());

        b.go_back();
        wait(&mut b);
        assert_eq!(b.location(), dir.path());
        assert!(b.can_go_forward());

        b.go_forward();
        wait(&mut b);
        assert_eq!(b.location(), dir.path().join("src"));

        b.go_up();
        wait(&mut b);
        assert_eq!(b.location(), dir.path());
        assert_eq!(
            b.selection.focus(),
            Some(dir.path().join("src").as_path()),
            "came-from folder is selected"
        );
        assert!(!b.can_go_forward(), "going up is a new navigation");
    }

    #[test]
    fn missing_folders_fail_but_history_still_works() {
        let dir = tree();
        let mut b = browser(dir.path());
        b.navigate(dir.path().join("nope"));
        wait(&mut b);
        assert!(matches!(b.load_state(), LoadState::Failed(_)));
        b.go_back();
        wait(&mut b);
        assert_eq!(names(&b).len(), 4);
    }

    #[test]
    fn sorting_and_filtering() {
        let dir = tree();
        let mut b = browser(dir.path());
        b.sort_by(SortColumn::Size);
        assert_eq!(names(&b), ["docs", "src", "file2.txt", "file10.txt"]);
        b.sort_by(SortColumn::Size);
        assert_eq!(b.sort().direction, SortDirection::Descending);
        assert_eq!(names(&b), ["docs", "src", "file10.txt", "file2.txt"]);

        b.set_filter_query("FILE");
        assert_eq!(names(&b), ["file10.txt", "file2.txt"]);
        b.set_show_hidden(true);
        b.set_filter_query("");
        assert_eq!(names(&b).len(), 5);
    }

    #[test]
    fn filtering_drops_hidden_selection() {
        let dir = tree();
        let mut b = browser(dir.path());
        b.select(|sel, rows| sel.select_all(rows));
        b.set_filter_query("src");
        assert_eq!(b.selected_entries().len(), 1);
    }

    #[test]
    fn refresh_keeps_focus_and_picks_up_changes() {
        let dir = tree();
        let mut b = browser(dir.path());
        let row = b.find("file2.txt").unwrap();
        b.select(|sel, rows| sel.click(rows, row));
        fs::write(dir.path().join("new.txt"), b"").unwrap();

        b.refresh();
        wait(&mut b);
        assert!(names(&b).contains(&"new.txt".to_owned()));
        assert_eq!(
            b.selection.focus(),
            Some(dir.path().join("file2.txt").as_path())
        );
    }

    #[test]
    fn apply_change_updates_without_reloading() {
        let dir = tree();
        let mut b = browser(dir.path());
        let old = dir.path().join("file2.txt");
        let new = crate::fs::ops::rename(&old, "zzz.txt").unwrap();
        b.apply_change(&[old], vec![Entry::from_path(&new).unwrap()]);
        assert_eq!(names(&b), ["docs", "src", "file10.txt", "zzz.txt"]);
    }

    #[test]
    fn outside_changes_show_up_without_refresh() {
        let dir = tree();
        let root = fs::canonicalize(dir.path()).unwrap();
        let mut b = browser(&root);
        fs::write(root.join("from-elsewhere.txt"), b"").unwrap();
        fs::remove_file(root.join("file2.txt")).unwrap();

        let deadline = Instant::now() + Duration::from_secs(10);
        loop {
            b.poll();
            let n = names(&b);
            if n.contains(&"from-elsewhere.txt".to_owned()) && !n.contains(&"file2.txt".to_owned())
            {
                break;
            }
            assert!(Instant::now() < deadline, "changes never arrived: {n:?}");
            std::thread::sleep(Duration::from_millis(20));
        }
    }

    #[test]
    fn navigating_clears_the_filter_and_selection() {
        let dir = tree();
        let mut b = browser(dir.path());
        b.set_filter_query("file");
        b.select(|sel, rows| sel.select_all(rows));
        b.navigate(dir.path().join("src"));
        wait(&mut b);
        assert_eq!(b.filter().query(), "");
        assert!(b.selection.is_empty());
    }

    #[test]
    fn cancel_keeps_partial_results() {
        let dir = tempfile::tempdir().unwrap();
        for i in 0..3000 {
            fs::write(dir.path().join(format!("f{i}")), b"").unwrap();
        }
        let mut b = Browser::new(
            dir.path().to_path_buf(),
            SortOrder::default(),
            false,
            Arc::new(|| {}),
        );
        b.cancel_loading();
        b.poll();
        assert!(!b.is_loading());
        // Whether anything arrived first depends on timing; either way the
        // browser is usable and reports the cancel (unless it already finished).
        assert!(matches!(b.load_state(), LoadState::Ready { .. }));
    }
}
