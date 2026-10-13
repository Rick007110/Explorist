//! Sorting the file list.

use std::cmp::Ordering;
use std::iter::Peekable;
use std::str::Chars;

use serde::{Deserialize, Serialize};

use super::entry::Entry;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SortColumn {
    #[default]
    Name,
    Size,
    Modified,
    Type,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SortDirection {
    #[default]
    Ascending,
    Descending,
}

impl SortDirection {
    pub fn flipped(self) -> Self {
        match self {
            Self::Ascending => Self::Descending,
            Self::Descending => Self::Ascending,
        }
    }
}

/// How the file list is ordered.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct SortOrder {
    pub column: SortColumn,
    pub direction: SortDirection,
    /// Keep folders above files whatever the column and direction.
    pub folders_first: bool,
    /// Compare runs of digits by value, so `file2` sorts before `file10`.
    pub natural: bool,
}

impl Default for SortOrder {
    fn default() -> Self {
        Self {
            column: SortColumn::Name,
            direction: SortDirection::Ascending,
            folders_first: true,
            natural: true,
        }
    }
}

impl SortOrder {
    /// What a click on a column header does: the same column flips the
    /// direction, another column starts ascending.
    pub fn clicked(self, column: SortColumn) -> Self {
        let direction = if column == self.column {
            self.direction.flipped()
        } else {
            SortDirection::Ascending
        };
        Self {
            column,
            direction,
            ..self
        }
    }

    pub fn sort(&self, entries: &mut [Entry]) {
        entries.sort_by(|a, b| self.compare(a, b));
    }

    pub fn compare(&self, a: &Entry, b: &Entry) -> Ordering {
        if self.folders_first && a.is_dir != b.is_dir {
            return if a.is_dir {
                Ordering::Less
            } else {
                Ordering::Greater
            };
        }

        let by_column = match self.column {
            SortColumn::Name => Ordering::Equal,
            SortColumn::Size => a.size.cmp(&b.size),
            SortColumn::Modified => a.modified.cmp(&b.modified),
            SortColumn::Type => cmp_ignore_case(&a.type_label, &b.type_label),
        };
        // Ties fall back to the name. For the Name column the name is the
        // key itself, so it follows the direction too.
        let by_name = || self.compare_names(&a.name, &b.name);
        match (self.column, self.direction) {
            (SortColumn::Name, SortDirection::Ascending) => by_name(),
            (SortColumn::Name, SortDirection::Descending) => by_name().reverse(),
            (_, SortDirection::Ascending) => by_column.then_with(by_name),
            (_, SortDirection::Descending) => by_column.reverse().then_with(by_name),
        }
    }

    fn compare_names(&self, a: &str, b: &str) -> Ordering {
        let ordering = if self.natural {
            natural_cmp(a, b)
        } else {
            cmp_ignore_case(a, b)
        };
        // Names differing only in case still get a fixed order.
        ordering.then_with(|| a.cmp(b))
    }
}

fn cmp_ignore_case(a: &str, b: &str) -> Ordering {
    a.chars()
        .flat_map(char::to_lowercase)
        .cmp(b.chars().flat_map(char::to_lowercase))
}

/// Case-insensitive comparison that treats runs of ASCII digits as numbers,
/// like Windows Explorer: `file2` < `file10`, `v1.9` < `v1.10`.
///
/// Numbers equal in value but written differently (`07` vs `7`) compare as
/// equal here; the caller breaks the tie.
pub fn natural_cmp(a: &str, b: &str) -> Ordering {
    let mut a = a.chars().peekable();
    let mut b = b.chars().peekable();
    loop {
        match (a.peek().copied(), b.peek().copied()) {
            (None, None) => return Ordering::Equal,
            (None, Some(_)) => return Ordering::Less,
            (Some(_), None) => return Ordering::Greater,
            (Some(x), Some(y)) if x.is_ascii_digit() && y.is_ascii_digit() => {
                let ordering = cmp_numbers(&take_digits(&mut a), &take_digits(&mut b));
                if ordering != Ordering::Equal {
                    return ordering;
                }
            }
            (Some(x), Some(y)) => {
                let ordering = x.to_lowercase().cmp(y.to_lowercase());
                if ordering != Ordering::Equal {
                    return ordering;
                }
                a.next();
                b.next();
            }
        }
    }
}

fn take_digits(chars: &mut Peekable<Chars<'_>>) -> String {
    let mut digits = String::new();
    while let Some(c) = chars.next_if(char::is_ascii_digit) {
        digits.push(c);
    }
    digits
}

/// Compares two digit strings by value without parsing, so any length works.
fn cmp_numbers(a: &str, b: &str) -> Ordering {
    let a = a.trim_start_matches('0');
    let b = b.trim_start_matches('0');
    a.len().cmp(&b.len()).then_with(|| a.cmp(b))
}

#[cfg(test)]
mod tests {
    use std::borrow::Cow;
    use std::path::PathBuf;
    use std::time::{Duration, SystemTime};

    use super::*;
    use crate::fs::kind;

    fn file(name: &str, size: u64, age_secs: u64) -> Entry {
        let (kind, type_label) = kind::classify(name, false);
        Entry {
            name: name.into(),
            path: PathBuf::from(name),
            is_dir: false,
            kind,
            type_label,
            size: Some(size),
            modified: Some(SystemTime::UNIX_EPOCH + Duration::from_secs(1_000_000 - age_secs)),
            created: None,
            hidden: false,
            system: false,
        }
    }

    fn folder(name: &str) -> Entry {
        Entry {
            is_dir: true,
            size: None,
            kind: kind::FileKind::Folder,
            type_label: Cow::Borrowed("Folder"),
            ..file(name, 0, 0)
        }
    }

    fn names(order: SortOrder, mut entries: Vec<Entry>) -> Vec<String> {
        order.sort(&mut entries);
        entries.into_iter().map(|e| e.name).collect()
    }

    #[test]
    fn natural_order_compares_numbers_by_value() {
        let mut list = vec!["file10", "file2", "file1", "File3", "file02"];
        list.sort_by(|a, b| natural_cmp(a, b).then_with(|| a.cmp(b)));
        assert_eq!(list, ["file1", "file02", "file2", "File3", "file10"]);
    }

    #[test]
    fn natural_order_handles_versions_and_huge_numbers() {
        assert_eq!(natural_cmp("v1.9", "v1.10"), Ordering::Less);
        assert_eq!(
            natural_cmp("x99999999999999999999999", "x100000000000000000000000"),
            Ordering::Less
        );
        assert_eq!(natural_cmp("a", "a1"), Ordering::Less);
        assert_eq!(natural_cmp("1a", "a"), Ordering::Less);
    }

    #[test]
    fn natural_order_ignores_case() {
        assert_eq!(natural_cmp("README.md", "readme.md"), Ordering::Equal);
        assert_eq!(natural_cmp("apple", "Banana"), Ordering::Less);
    }

    #[test]
    fn plain_order_when_natural_is_off() {
        let order = SortOrder {
            natural: false,
            ..SortOrder::default()
        };
        let got = names(order, vec![file("file10", 0, 0), file("file2", 0, 0)]);
        assert_eq!(got, ["file10", "file2"]);
    }

    #[test]
    fn folders_stay_first_in_both_directions() {
        let entries = || {
            vec![
                file("a.txt", 1, 0),
                folder("src"),
                folder("docs"),
                file("b.txt", 1, 0),
            ]
        };
        let asc = SortOrder::default();
        assert_eq!(names(asc, entries()), ["docs", "src", "a.txt", "b.txt"]);

        let desc = SortOrder {
            direction: SortDirection::Descending,
            ..asc
        };
        assert_eq!(names(desc, entries()), ["src", "docs", "b.txt", "a.txt"]);
    }

    #[test]
    fn folders_mix_in_when_grouping_is_off() {
        let order = SortOrder {
            folders_first: false,
            ..SortOrder::default()
        };
        let got = names(
            order,
            vec![file("b.txt", 1, 0), folder("c"), file("a.txt", 1, 0)],
        );
        assert_eq!(got, ["a.txt", "b.txt", "c"]);
    }

    #[test]
    fn size_sort_breaks_ties_by_name() {
        let order = SortOrder {
            column: SortColumn::Size,
            ..SortOrder::default()
        };
        let entries = vec![
            file("big", 900, 0),
            file("b-small", 10, 0),
            file("a-small", 10, 0),
        ];
        assert_eq!(names(order, entries.clone()), ["a-small", "b-small", "big"]);

        let desc = SortOrder {
            direction: SortDirection::Descending,
            ..order
        };
        // Descending flips the size, but ties still read A to Z.
        assert_eq!(names(desc, entries), ["big", "a-small", "b-small"]);
    }

    #[test]
    fn modified_sort_puts_newest_last_when_ascending() {
        let order = SortOrder {
            column: SortColumn::Modified,
            ..SortOrder::default()
        };
        let got = names(order, vec![file("new", 0, 10), file("old", 0, 500)]);
        assert_eq!(got, ["old", "new"]);
    }

    #[test]
    fn type_sort_groups_by_label() {
        let order = SortOrder {
            column: SortColumn::Type,
            ..SortOrder::default()
        };
        let got = names(
            order,
            vec![file("z.md", 0, 0), file("a.toml", 0, 0), file("b.md", 0, 0)],
        );
        assert_eq!(got, ["b.md", "z.md", "a.toml"]);
    }

    #[test]
    fn clicking_headers() {
        let start = SortOrder::default();
        let flipped = start.clicked(SortColumn::Name);
        assert_eq!(flipped.direction, SortDirection::Descending);

        let size = flipped.clicked(SortColumn::Size);
        assert_eq!(
            (size.column, size.direction),
            (SortColumn::Size, SortDirection::Ascending)
        );
        assert!(size.folders_first);
    }
}
