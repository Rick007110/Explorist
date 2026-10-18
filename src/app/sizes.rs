//! Folder sizes in the Size column (Settings → Sorting & view).
//!
//! Sizing starts once a folder has loaded. Finished sizes go into the
//! entries, so sorting by size and the selection total include folders.
//! Until a folder is done, the column shows the last size seen for it (from
//! an earlier visit) or the running count, dimmed.

use std::collections::HashMap;
use std::path::{Path, PathBuf};

use crate::app::ExploristApp;
use crate::app::browser::LoadState;
use crate::fs::dirsize::FolderSizer;

/// Forget remembered sizes past this many, rather than track their age.
const CACHE_LIMIT: usize = 100_000;

#[derive(Default)]
pub struct FolderSizes {
    sizer: Option<FolderSizer>,
    /// The folder `sizer` is working on, and how many folders it had.
    folder: PathBuf,
    count: usize,
    /// Sizes from earlier, shown until fresh ones are in.
    cache: HashMap<PathBuf, u64>,
    seen_generation: u64,
    /// Start over on the next frame (Refresh).
    pub restart: bool,
    /// Sizes were on last frame, so turning them off clears them.
    active: bool,
}

impl FolderSizes {
    /// What to show for a folder not finished yet: an earlier size, or
    /// the count so far.
    pub fn provisional(&self, folder: &Path) -> Option<u64> {
        if let Some(&bytes) = self.cache.get(folder) {
            return Some(bytes);
        }
        let (bytes, done) = self.sizer.as_ref()?.get(folder)?;
        (done || bytes > 0).then_some(bytes)
    }
}

impl ExploristApp {
    /// Runs each frame: starts, follows and applies folder sizing.
    pub(crate) fn update_folder_sizes(&mut self) {
        let sizes = &mut self.folder_sizes;
        if !self.settings.folder_sizes {
            if std::mem::take(&mut sizes.active) {
                sizes.sizer = None;
                // Drop the sizes from the list.
                self.browser.refresh();
            }
            return;
        }
        sizes.active = true;
        if self.browser.start_page || !matches!(self.browser.load_state(), LoadState::Ready { .. })
        {
            return;
        }

        let location = self.browser.location();
        let dirs: Vec<&Path> = self
            .browser
            .entries()
            .iter()
            .filter(|e| e.is_dir)
            .map(|e| e.path.as_path())
            .collect();
        let stale = sizes.sizer.is_none()
            || sizes.folder != location
            || sizes.count != dirs.len()
            || std::mem::take(&mut sizes.restart);
        if stale {
            let ctx = self.ctx.clone();
            sizes.folder = location.to_path_buf();
            sizes.count = dirs.len();
            sizes.sizer = Some(FolderSizer::start(
                dirs.into_iter().map(Path::to_path_buf).collect(),
                move || ctx.request_repaint(),
            ));
            sizes.seen_generation = u64::MAX;
        }

        let Some(sizer) = &sizes.sizer else { return };
        let generation = self.browser.generation();
        if sizer.take_changed() || generation != sizes.seen_generation {
            let finished: Vec<(PathBuf, u64)> = sizer
                .finished()
                .map(|(p, bytes)| (p.to_path_buf(), bytes))
                .collect();
            if sizes.cache.len() + finished.len() > CACHE_LIMIT {
                sizes.cache.clear();
            }
            sizes.cache.extend(finished.iter().cloned());
            self.browser.set_folder_sizes(&finished);
            sizes.seen_generation = self.browser.generation();
        }
    }
}
