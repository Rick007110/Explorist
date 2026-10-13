//! Tabs. The active tab's state lives in the app's usual fields (so the
//! rest of the code needn't know about tabs); the others wait here and are
//! swapped in when chosen.

use std::path::PathBuf;
use std::sync::Arc;

use crate::app::ExploristApp;
use crate::app::browser::Browser;

/// A tab that isn't showing.
pub struct TabSlot {
    pub browser: Browser,
    pub filter_open: bool,
    pub filter_text: String,
    pub list_offset: f32,
}

impl ExploristApp {
    /// Number of tabs, the active one included.
    pub(crate) fn tab_count(&self) -> usize {
        self.tabs.len()
    }

    /// The folder shown in tab `i`.
    pub(crate) fn tab_location(&self, i: usize) -> PathBuf {
        match self.tabs.get(i) {
            Some(Some(slot)) => slot.browser.location().to_path_buf(),
            _ => self.browser.location().to_path_buf(),
        }
    }

    /// Whether tab `i` is still reading its folder.
    pub(crate) fn tab_busy(&self, i: usize) -> bool {
        match self.tabs.get(i) {
            Some(Some(slot)) => slot.browser.is_loading(),
            _ => self.browser.is_loading(),
        }
    }

    /// Opens `path` in a new tab after the active one and switches to it.
    pub(crate) fn new_tab(&mut self, path: PathBuf) {
        let ctx = self.ctx.clone();
        let browser = Browser::new(
            path.clone(),
            self.settings.sort_for(&path),
            self.settings.show_hidden,
            Arc::new(move || ctx.request_repaint()),
        );
        let slot = TabSlot {
            browser,
            filter_open: false,
            filter_text: String::new(),
            list_offset: 0.0,
        };
        let at = self.active_tab + 1;
        self.tabs.insert(at, Some(slot));
        self.switch_tab(at);
    }

    /// Makes tab `i` the active one.
    pub(crate) fn switch_tab(&mut self, i: usize) {
        if i == self.active_tab || i >= self.tabs.len() {
            return;
        }
        let Some(incoming) = self.tabs[i].take() else {
            return;
        };
        let outgoing = TabSlot {
            browser: std::mem::replace(&mut self.browser, incoming.browser),
            filter_open: std::mem::replace(&mut self.view.filter_open, incoming.filter_open),
            filter_text: std::mem::replace(&mut self.view.filter_text, incoming.filter_text),
            list_offset: std::mem::replace(&mut self.view.list_offset, incoming.list_offset),
        };
        self.tabs[self.active_tab] = Some(outgoing);
        self.active_tab = i;
        self.view.rename = None;
        self.view.path_edit = None;
        self.view.settings_open = false;
        self.view.scroll_to_focus = true;
    }

    /// Closes tab `i`. Closing the last tab closes the window.
    pub(crate) fn close_tab(&mut self, i: usize) {
        if self.tabs.len() <= 1 {
            self.ctx.send_viewport_cmd(egui::ViewportCommand::Close);
            return;
        }
        if i == self.active_tab {
            // Show a neighbour first, then drop the old active state.
            let next = if i + 1 < self.tabs.len() {
                i + 1
            } else {
                i - 1
            };
            self.switch_tab(next);
        }
        self.tabs.remove(i);
        if i < self.active_tab {
            self.active_tab -= 1;
        }
    }

    pub(crate) fn cycle_tab(&mut self, forward: bool) {
        let n = self.tabs.len();
        if n > 1 {
            let next = if forward {
                (self.active_tab + 1) % n
            } else {
                (self.active_tab + n - 1) % n
            };
            self.switch_tab(next);
        }
    }

    /// Keeps background tabs up to date.
    pub(crate) fn poll_background_tabs(&mut self) {
        for slot in self.tabs.iter_mut().flatten() {
            slot.browser.poll();
        }
    }

    /// Every tab's folder, in order, for saving.
    pub(crate) fn tab_locations(&self) -> Vec<PathBuf> {
        (0..self.tabs.len()).map(|i| self.tab_location(i)).collect()
    }
}
