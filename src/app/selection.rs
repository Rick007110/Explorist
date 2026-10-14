//! Which rows are selected and which one has keyboard focus.
//!
//! Selection and focus are kept by path, so they survive re-sorting,
//! filtering and reloads. Selection and focus are separate, as in the
//! design: focus stays visible inside a multi-selection.

use std::collections::HashSet;
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

use crate::fs::entry::Entry;
use crate::fs::sort::natural_cmp;

/// How long typed letters keep adding to a type-to-jump search.
const TYPE_AHEAD_TIMEOUT: Duration = Duration::from_millis(1000);

/// The rows as currently shown: entries in display order.
#[derive(Clone, Copy)]
pub struct Rows<'a> {
    pub entries: &'a [Entry],
    /// Indices into `entries`, in display order.
    pub visible: &'a [usize],
}

impl<'a> Rows<'a> {
    pub fn len(&self) -> usize {
        self.visible.len()
    }

    pub fn is_empty(&self) -> bool {
        self.visible.is_empty()
    }

    pub fn get(&self, row: usize) -> Option<&'a Entry> {
        self.visible.get(row).map(|&i| &self.entries[i])
    }

    pub fn position(&self, path: &Path) -> Option<usize> {
        self.visible
            .iter()
            .position(|&i| self.entries[i].path == path)
    }
}

/// How a keyboard move affects the selection.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MoveMode {
    /// Plain arrows: select only the new row.
    Replace,
    /// Shift+arrows: select from the anchor to the new row.
    Extend,
    /// Ctrl+arrows: move focus without changing the selection.
    FocusOnly,
}

#[derive(Debug, Default)]
pub struct Selection {
    selected: HashSet<PathBuf>,
    focus: Option<PathBuf>,
    /// Where Shift ranges start from.
    anchor: Option<PathBuf>,
    type_ahead: String,
    last_typed: Option<Instant>,
}

impl Selection {
    pub fn is_selected(&self, path: &Path) -> bool {
        self.selected.contains(path)
    }

    pub fn is_focused(&self, path: &Path) -> bool {
        self.focus.as_deref() == Some(path)
    }

    pub fn focus(&self) -> Option<&Path> {
        self.focus.as_deref()
    }

    pub fn count(&self) -> usize {
        self.selected.len()
    }

    pub fn is_empty(&self) -> bool {
        self.selected.is_empty()
    }

    /// Selected entries in display order.
    pub fn selected_entries<'a>(&self, rows: Rows<'a>) -> Vec<&'a Entry> {
        rows.visible
            .iter()
            .map(|&i| &rows.entries[i])
            .filter(|e| self.selected.contains(&e.path))
            .collect()
    }

    pub fn clear(&mut self) {
        self.selected.clear();
        self.focus = None;
        self.anchor = None;
    }

    /// Selects one path and nothing else, e.g. a renamed or new item.
    pub fn select_only(&mut self, path: &Path) {
        self.selected.clear();
        self.selected.insert(path.to_path_buf());
        self.focus = Some(path.to_path_buf());
        self.anchor = Some(path.to_path_buf());
    }

    /// Selects what wasn't selected, and the other way round.
    pub fn invert(&mut self, rows: Rows<'_>) {
        self.selected = rows
            .visible
            .iter()
            .map(|&i| &rows.entries[i].path)
            .filter(|p| !self.selected.contains(*p))
            .cloned()
            .collect();
    }

    /// The selected paths, to restore or add to.
    pub fn snapshot(&self) -> HashSet<PathBuf> {
        self.selected.clone()
    }

    /// Selection box: exactly `base` plus the rows in `hits`, with focus on
    /// `focus` (the row the box ends nearest).
    pub fn band(
        &mut self,
        rows: Rows<'_>,
        base: &HashSet<PathBuf>,
        hits: impl IntoIterator<Item = usize>,
        focus: Option<usize>,
    ) {
        self.selected = base.clone();
        self.selected.extend(
            hits.into_iter()
                .filter_map(|r| rows.get(r))
                .map(|e| e.path.clone()),
        );
        if let Some(entry) = focus.and_then(|r| rows.get(r)) {
            self.focus = Some(entry.path.clone());
            self.anchor = Some(entry.path.clone());
        }
    }

    /// Plain click.
    pub fn click(&mut self, rows: Rows<'_>, row: usize) {
        if let Some(entry) = rows.get(row) {
            self.select_only(&entry.path);
        }
    }

    /// Ctrl+click: toggles one row and moves the anchor there.
    pub fn toggle(&mut self, rows: Rows<'_>, row: usize) {
        let Some(entry) = rows.get(row) else { return };
        if !self.selected.remove(&entry.path) {
            self.selected.insert(entry.path.clone());
        }
        self.focus = Some(entry.path.clone());
        self.anchor = Some(entry.path.clone());
    }

    /// Shift+click: selects from the anchor to `row`. With `add` (Ctrl+Shift)
    /// the range is added to the selection instead of replacing it.
    pub fn extend_to(&mut self, rows: Rows<'_>, row: usize, add: bool) {
        let Some(target) = rows.get(row) else { return };
        let anchor = self
            .anchor
            .as_deref()
            .and_then(|a| rows.position(a))
            .unwrap_or(row);
        if !add {
            self.selected.clear();
        }
        let (lo, hi) = (anchor.min(row), anchor.max(row));
        for r in lo..=hi {
            if let Some(e) = rows.get(r) {
                self.selected.insert(e.path.clone());
            }
        }
        self.focus = Some(target.path.clone());
        if self.anchor.is_none() {
            self.anchor = Some(target.path.clone());
        }
    }

    /// Moves focus to `row` (clamped to the list).
    pub fn move_to(&mut self, rows: Rows<'_>, row: usize, mode: MoveMode) {
        if rows.is_empty() {
            return;
        }
        let row = row.min(rows.len() - 1);
        match mode {
            MoveMode::Replace => self.click(rows, row),
            MoveMode::Extend => self.extend_to(rows, row, false),
            MoveMode::FocusOnly => self.focus = rows.get(row).map(|e| e.path.clone()),
        }
    }

    /// Moves focus up or down by `delta` rows. With nothing focused, Down
    /// starts at the top and Up at the bottom.
    pub fn move_by(&mut self, rows: Rows<'_>, delta: isize, mode: MoveMode) {
        if rows.is_empty() {
            return;
        }
        let target = match self.focused_row(rows) {
            Some(current) => current.saturating_add_signed(delta),
            None if delta >= 0 => 0,
            None => rows.len() - 1,
        };
        self.move_to(rows, target, mode);
    }

    /// Space with Ctrl: toggles the focused row.
    pub fn toggle_focused(&mut self, rows: Rows<'_>) {
        if let Some(row) = self.focused_row(rows) {
            self.toggle(rows, row);
        }
    }

    pub fn select_all(&mut self, rows: Rows<'_>) {
        self.selected = rows
            .visible
            .iter()
            .map(|&i| rows.entries[i].path.clone())
            .collect();
        if self.focus.is_none() {
            self.focus = rows.get(0).map(|e| e.path.clone());
        }
    }

    pub fn focused_row(&self, rows: Rows<'_>) -> Option<usize> {
        rows.position(self.focus.as_deref()?)
    }

    /// Forgets paths no longer shown, e.g. after a reload or a filter change.
    pub fn retain_visible(&mut self, rows: Rows<'_>) {
        let visible: HashSet<&Path> = rows
            .visible
            .iter()
            .map(|&i| rows.entries[i].path.as_path())
            .collect();
        self.selected.retain(|p| visible.contains(p.as_path()));
        if self.focus.as_deref().is_some_and(|f| !visible.contains(f)) {
            self.focus = None;
        }
        if self.anchor.as_deref().is_some_and(|a| !visible.contains(a)) {
            self.anchor = None;
        }
    }

    /// Type-to-jump: letters typed in quick succession build a prefix, and
    /// focus jumps to the first name starting with it. Typing the same
    /// letter again cycles through names starting with that letter.
    pub fn type_ahead(&mut self, rows: Rows<'_>, ch: char, now: Instant) {
        let fresh = self
            .last_typed
            .is_none_or(|t| now.duration_since(t) > TYPE_AHEAD_TIMEOUT);
        if fresh {
            self.type_ahead.clear();
        }
        self.last_typed = Some(now);
        self.type_ahead.extend(ch.to_lowercase());

        let repeated_letter = {
            let mut chars = self.type_ahead.chars();
            let first = chars.next();
            self.type_ahead.chars().count() > 1 && chars.all(|c| Some(c) == first)
        };
        let (prefix, start) = if repeated_letter || self.type_ahead.chars().count() == 1 {
            // Cycle: search from the row after the focused one.
            let prefix: String = self.type_ahead.chars().take(1).collect();
            let start = self.focused_row(rows).map_or(0, |r| r + 1);
            (prefix, start)
        } else {
            (self.type_ahead.clone(), self.focused_row(rows).unwrap_or(0))
        };

        let len = rows.len();
        let hit = (0..len)
            .map(|offset| (start + offset) % len.max(1))
            .find(|&row| {
                rows.get(row)
                    .is_some_and(|e| e.name.to_lowercase().starts_with(&prefix))
            });
        if let Some(row) = hit {
            self.click(rows, row);
        }
    }
}

/// Finds the row for `name` among `rows`, e.g. to select the folder you
/// just came up out of. Case-insensitive.
pub fn find_by_name(rows: Rows<'_>, name: &str) -> Option<usize> {
    (0..rows.len()).find(|&r| {
        rows.get(r)
            .is_some_and(|e| natural_cmp(&e.name, name).is_eq())
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::fs::kind;

    fn entries(names: &[&str]) -> Vec<Entry> {
        names
            .iter()
            .map(|name| {
                let (kind, type_label) = kind::classify(name, false);
                Entry {
                    name: (*name).into(),
                    path: PathBuf::from(name),
                    is_dir: false,
                    kind,
                    type_label,
                    size: Some(0),
                    modified: None,
                    created: None,
                    hidden: false,
                    system: false,
                }
            })
            .collect()
    }

    fn selected(sel: &Selection, rows: Rows<'_>) -> Vec<String> {
        sel.selected_entries(rows)
            .into_iter()
            .map(|e| e.name.clone())
            .collect()
    }

    fn focused(sel: &Selection) -> Option<String> {
        sel.focus().map(|p| p.display().to_string())
    }

    const NAMES: [&str; 5] = ["a", "b", "c", "d", "e"];
    const ALL: [usize; 5] = [0, 1, 2, 3, 4];

    #[test]
    fn click_selects_one() {
        let e = entries(&NAMES);
        let rows = Rows {
            entries: &e,
            visible: &ALL,
        };
        let mut sel = Selection::default();
        sel.click(rows, 1);
        sel.click(rows, 3);
        assert_eq!(selected(&sel, rows), ["d"]);
        assert_eq!(focused(&sel).as_deref(), Some("d"));
    }

    #[test]
    fn ctrl_click_toggles() {
        let e = entries(&NAMES);
        let rows = Rows {
            entries: &e,
            visible: &ALL,
        };
        let mut sel = Selection::default();
        sel.click(rows, 0);
        sel.toggle(rows, 2);
        assert_eq!(selected(&sel, rows), ["a", "c"]);
        sel.toggle(rows, 0);
        assert_eq!(selected(&sel, rows), ["c"]);
    }

    #[test]
    fn shift_click_selects_a_range_from_the_anchor() {
        let e = entries(&NAMES);
        let rows = Rows {
            entries: &e,
            visible: &ALL,
        };
        let mut sel = Selection::default();
        sel.click(rows, 3);
        sel.extend_to(rows, 1, false);
        assert_eq!(selected(&sel, rows), ["b", "c", "d"]);
        // The anchor stays put, so a second Shift+click re-ranges from it.
        sel.extend_to(rows, 4, false);
        assert_eq!(selected(&sel, rows), ["d", "e"]);
    }

    #[test]
    fn ctrl_shift_click_adds_a_range() {
        let e = entries(&NAMES);
        let rows = Rows {
            entries: &e,
            visible: &ALL,
        };
        let mut sel = Selection::default();
        sel.click(rows, 0);
        sel.toggle(rows, 3);
        sel.extend_to(rows, 4, true);
        assert_eq!(selected(&sel, rows), ["a", "d", "e"]);
    }

    #[test]
    fn arrows_move_and_clamp() {
        let e = entries(&NAMES);
        let rows = Rows {
            entries: &e,
            visible: &ALL,
        };
        let mut sel = Selection::default();
        sel.move_by(rows, 1, MoveMode::Replace);
        assert_eq!(focused(&sel).as_deref(), Some("a"));
        sel.move_by(rows, -1, MoveMode::Replace);
        assert_eq!(focused(&sel).as_deref(), Some("a"));
        sel.move_by(rows, 10, MoveMode::Replace);
        assert_eq!(selected(&sel, rows), ["e"]);
    }

    #[test]
    fn up_with_nothing_focused_starts_at_the_bottom() {
        let e = entries(&NAMES);
        let rows = Rows {
            entries: &e,
            visible: &ALL,
        };
        let mut sel = Selection::default();
        sel.move_by(rows, -1, MoveMode::Replace);
        assert_eq!(focused(&sel).as_deref(), Some("e"));
    }

    #[test]
    fn shift_arrows_extend_and_ctrl_arrows_only_move_focus() {
        let e = entries(&NAMES);
        let rows = Rows {
            entries: &e,
            visible: &ALL,
        };
        let mut sel = Selection::default();
        sel.click(rows, 1);
        sel.move_by(rows, 2, MoveMode::Extend);
        assert_eq!(selected(&sel, rows), ["b", "c", "d"]);

        sel.move_by(rows, 1, MoveMode::FocusOnly);
        assert_eq!(selected(&sel, rows), ["b", "c", "d"]);
        assert_eq!(focused(&sel).as_deref(), Some("e"));
        sel.toggle_focused(rows);
        assert_eq!(selected(&sel, rows), ["b", "c", "d", "e"]);
    }

    #[test]
    fn select_all_and_retain_visible() {
        let e = entries(&NAMES);
        let rows = Rows {
            entries: &e,
            visible: &ALL,
        };
        let mut sel = Selection::default();
        sel.select_all(rows);
        assert_eq!(sel.count(), 5);

        let filtered = Rows {
            entries: &e,
            visible: &[1, 3],
        };
        sel.retain_visible(filtered);
        assert_eq!(selected(&sel, filtered), ["b", "d"]);
        assert_eq!(sel.focus(), None, "focus was on a, which is filtered out");
    }

    #[test]
    fn selection_follows_paths_not_positions() {
        let e = entries(&NAMES);
        let mut sel = Selection::default();
        sel.click(
            Rows {
                entries: &e,
                visible: &ALL,
            },
            1,
        );
        // Reverse the order: b is now row 3.
        let reversed = Rows {
            entries: &e,
            visible: &[4, 3, 2, 1, 0],
        };
        assert_eq!(sel.focused_row(reversed), Some(3));
    }

    #[test]
    fn type_ahead_builds_a_prefix() {
        let e = entries(&["apple", "banana", "blueberry", "cherry"]);
        let rows = Rows {
            entries: &e,
            visible: &[0, 1, 2, 3],
        };
        let mut sel = Selection::default();
        let t = Instant::now();
        sel.type_ahead(rows, 'B', t);
        assert_eq!(focused(&sel).as_deref(), Some("banana"));
        sel.type_ahead(rows, 'l', t + Duration::from_millis(200));
        assert_eq!(focused(&sel).as_deref(), Some("blueberry"));
    }

    #[test]
    fn type_ahead_cycles_on_repeated_letters_and_resets_after_a_pause() {
        let e = entries(&["apple", "banana", "blueberry", "cherry"]);
        let rows = Rows {
            entries: &e,
            visible: &[0, 1, 2, 3],
        };
        let mut sel = Selection::default();
        let t = Instant::now();
        sel.type_ahead(rows, 'b', t);
        sel.type_ahead(rows, 'b', t + Duration::from_millis(100));
        assert_eq!(focused(&sel).as_deref(), Some("blueberry"));
        sel.type_ahead(rows, 'b', t + Duration::from_millis(200));
        assert_eq!(focused(&sel).as_deref(), Some("banana"), "wraps around");

        sel.type_ahead(rows, 'c', t + Duration::from_secs(5));
        assert_eq!(focused(&sel).as_deref(), Some("cherry"));
    }

    #[test]
    fn empty_lists_are_safe() {
        let e = entries(&[]);
        let rows = Rows {
            entries: &e,
            visible: &[],
        };
        let mut sel = Selection::default();
        sel.move_by(rows, 1, MoveMode::Replace);
        sel.move_to(rows, 3, MoveMode::Extend);
        sel.type_ahead(rows, 'x', Instant::now());
        sel.select_all(rows);
        assert!(sel.is_empty());
    }

    #[test]
    fn find_by_name_ignores_case() {
        let e = entries(&["Docs", "src"]);
        let rows = Rows {
            entries: &e,
            visible: &[0, 1],
        };
        assert_eq!(find_by_name(rows, "docs"), Some(0));
        assert_eq!(find_by_name(rows, "missing"), None);
    }

    #[test]
    fn a_band_adds_its_rows_to_the_base() {
        let entries = entries(&NAMES);
        let rows = Rows {
            entries: &entries,
            visible: &ALL,
        };
        let mut sel = Selection::default();
        sel.click(rows, 0);
        let base = sel.snapshot();
        sel.band(rows, &base, [2, 3], Some(3));
        assert_eq!(sel.count(), 3);
        assert!(sel.is_focused(&entries[3].path));
        sel.band(rows, &HashSet::new(), [1], Some(1));
        assert_eq!(sel.count(), 1);
        assert!(sel.is_selected(&entries[1].path));
    }

    #[test]
    fn invert_flips_every_visible_row() {
        let entries = entries(&NAMES);
        let rows = Rows {
            entries: &entries,
            visible: &ALL,
        };
        let mut sel = Selection::default();
        sel.click(rows, 1);
        sel.invert(rows);
        assert_eq!(selected(&sel, rows), ["a", "c", "d", "e"]);
        sel.invert(rows);
        assert_eq!(selected(&sel, rows), ["b"]);
    }
}
