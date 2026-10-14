//! Deciding which entries the file list shows.

use std::time::{Duration, SystemTime};

use chrono::{DateTime, Datelike, Local, TimeZone};

use super::entry::Entry;

/// The filter bar: a name filter (Ctrl F), optional Type / Size / Modified
/// chips, and the "show hidden files" toggle (Ctrl H).
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Filter {
    query: String,
    /// Lowercased copy of `query`, compared against lowercased names.
    needle: Vec<char>,
    wildcard: bool,
    pub show_hidden: bool,
    /// Type column labels to keep ("TOML document"). Empty: any type.
    pub types: Vec<String>,
    pub size: Option<SizeRule>,
    pub modified: Option<Recency>,
}

/// "Size > 1 KB". Folders have no size, so a size rule hides them.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SizeRule {
    pub at_least: bool,
    pub bytes: u64,
}

/// "Modified: last 7 days".
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Recency {
    Today,
    LastWeek,
    LastMonth,
    ThisYear,
}

impl Recency {
    pub const ALL: [Recency; 4] = [Self::Today, Self::LastWeek, Self::LastMonth, Self::ThisYear];

    pub fn label(self) -> &'static str {
        match self {
            Self::Today => "Today",
            Self::LastWeek => "Last 7 days",
            Self::LastMonth => "Last 30 days",
            Self::ThisYear => "This year",
        }
    }

    /// The earliest time that still counts, as of `now`.
    fn since(self, now: SystemTime) -> SystemTime {
        let day = Duration::from_secs(24 * 60 * 60);
        let local = DateTime::<Local>::from(now);
        let start_of = |y, m, d| {
            Local
                .with_ymd_and_hms(y, m, d, 0, 0, 0)
                .earliest()
                .map_or(now, SystemTime::from)
        };
        match self {
            Self::Today => start_of(local.year(), local.month(), local.day()),
            Self::LastWeek => now - day * 7,
            Self::LastMonth => now - day * 30,
            Self::ThisYear => start_of(local.year(), 1, 1),
        }
    }
}

/// Result of filtering a listing.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Filtered {
    /// Indices into the listing, in listing order.
    pub visible: Vec<usize>,
    /// Hidden or system items left out because hidden files are off,
    /// for the "2 hidden items" hint.
    pub hidden: usize,
}

impl Filter {
    pub fn query(&self) -> &str {
        &self.query
    }

    /// Sets the text typed in the filter bar. A query containing `*` or `?`
    /// matches the whole name as a wildcard pattern; anything else matches
    /// anywhere in the name. Case never matters.
    pub fn set_query(&mut self, query: &str) {
        self.query = query.to_owned();
        let trimmed = query.trim();
        self.needle = trimmed.chars().flat_map(char::to_lowercase).collect();
        self.wildcard = trimmed.contains(['*', '?']);
    }

    /// Whether the filter narrows the list at all.
    pub fn is_active(&self) -> bool {
        !self.needle.is_empty() || self.has_chips()
    }

    /// Whether any Type / Size / Modified rule is set.
    pub fn has_chips(&self) -> bool {
        !self.types.is_empty() || self.size.is_some() || self.modified.is_some()
    }

    /// Clears the name and every chip; hidden files stay as they are.
    pub fn clear(&mut self) {
        self.set_query("");
        self.types.clear();
        self.size = None;
        self.modified = None;
    }

    /// Whether `entry` passes the Type / Size / Modified chips.
    fn matches_chips(&self, entry: &Entry, since: Option<SystemTime>) -> bool {
        if !self.types.is_empty() && !self.types.iter().any(|t| *t == entry.type_label) {
            return false;
        }
        if let Some(rule) = self.size {
            let Some(size) = entry.size else { return false };
            if rule.at_least != (size >= rule.bytes) {
                return false;
            }
        }
        if let Some(since) = since {
            return entry.modified.is_some_and(|m| m >= since);
        }
        true
    }

    pub fn matches(&self, entry: &Entry) -> bool {
        if !self.show_hidden && (entry.hidden || entry.system) {
            return false;
        }
        let since = self.modified.map(|r| r.since(SystemTime::now()));
        self.matches_name(&entry.name) && self.matches_chips(entry, since)
    }

    pub fn matches_name(&self, name: &str) -> bool {
        if self.needle.is_empty() {
            return true;
        }
        let name: Vec<char> = name.chars().flat_map(char::to_lowercase).collect();
        if self.wildcard {
            wildcard_match(&self.needle, &name)
        } else {
            name.windows(self.needle.len()).any(|w| w == self.needle)
        }
    }

    pub fn apply(&self, entries: &[Entry]) -> Filtered {
        self.apply_at(entries, SystemTime::now())
    }

    /// [`Self::apply`] as of `now`, for "Modified: today" and friends.
    pub fn apply_at(&self, entries: &[Entry], now: SystemTime) -> Filtered {
        let since = self.modified.map(|r| r.since(now));
        let mut out = Filtered::default();
        for (i, entry) in entries.iter().enumerate() {
            if !self.show_hidden && (entry.hidden || entry.system) {
                out.hidden += 1;
            } else if self.matches_name(&entry.name) && self.matches_chips(entry, since) {
                out.visible.push(i);
            }
        }
        out
    }
}

/// `*` matches any run of characters, `?` exactly one. Linear-time greedy
/// matching with backtracking to the last `*`.
fn wildcard_match(pattern: &[char], text: &[char]) -> bool {
    let (mut p, mut t) = (0, 0);
    let mut star: Option<(usize, usize)> = None;
    while t < text.len() {
        match pattern.get(p) {
            Some('*') => {
                star = Some((p, t));
                p += 1;
            }
            Some(&c) if c == '?' || c == text[t] => {
                p += 1;
                t += 1;
            }
            _ => match star {
                Some((sp, st)) => {
                    p = sp + 1;
                    t = st + 1;
                    star = Some((sp, st + 1));
                }
                None => return false,
            },
        }
    }
    pattern[p..].iter().all(|&c| c == '*')
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::fs::kind;
    use std::path::PathBuf;
    use std::time::Duration;

    fn entry(name: &str, hidden: bool) -> Entry {
        let (kind, type_label) = kind::classify(name, false);
        Entry {
            name: name.into(),
            path: PathBuf::from(name),
            is_dir: false,
            kind,
            type_label,
            size: Some(0),
            modified: None,
            created: None,
            hidden,
            system: false,
        }
    }

    fn filter(query: &str) -> Filter {
        let mut f = Filter::default();
        f.set_query(query);
        f
    }

    #[test]
    fn empty_query_matches_everything() {
        let f = filter("   ");
        assert!(!f.is_active());
        assert!(f.matches_name("anything"));
    }

    #[test]
    fn substring_match_ignores_case() {
        let f = filter("CARGO");
        assert!(f.matches_name("Cargo.toml"));
        assert!(f.matches_name("my-cargo-notes.md"));
        assert!(!f.matches_name("README.md"));
    }

    #[test]
    fn substring_longer_than_name_does_not_match() {
        assert!(!filter("toolong").matches_name("short"));
    }

    #[test]
    fn wildcards_match_the_whole_name() {
        let f = filter("*.toml");
        assert!(f.matches_name("Cargo.toml"));
        assert!(!f.matches_name("Cargo.toml.bak"));

        let f = filter("file?.txt");
        assert!(f.matches_name("file1.txt"));
        assert!(!f.matches_name("file10.txt"));

        assert!(filter("*a*b*").matches_name("xxaxxbxx"));
        assert!(!filter("*a*b").matches_name("xxbxxa"));
        assert!(filter("**").matches_name(""));
    }

    #[test]
    fn non_ascii_names_match() {
        assert!(filter("ÉTÉ").matches_name("photos-été-2026"));
    }

    #[test]
    fn hidden_items_are_counted_not_shown() {
        let entries = [
            entry("a.txt", false),
            entry(".env", true),
            entry("b.txt", false),
        ];
        let result = filter("").apply(&entries);
        assert_eq!(result.visible, [0, 2]);
        assert_eq!(result.hidden, 1);

        let mut show = filter("");
        show.show_hidden = true;
        assert_eq!(show.apply(&entries).visible, [0, 1, 2]);
        assert_eq!(show.apply(&entries).hidden, 0);
    }

    fn sized(name: &str, size: u64, days_old: u64, now: SystemTime) -> Entry {
        Entry {
            size: Some(size),
            modified: Some(now - Duration::from_secs(days_old * 24 * 60 * 60 + 60)),
            ..entry(name, false)
        }
    }

    #[test]
    fn type_size_and_date_chips() {
        let now = SystemTime::now();
        let entries = [
            sized("Cargo.toml", 1_200, 0, now),
            sized("Cargo.lock", 48_000, 2, now),
            sized("README.md", 6_800, 40, now),
            sized("hero.png", 800_000, 400, now),
        ];
        let visible = |f: &Filter| -> Vec<String> {
            f.apply_at(&entries, now)
                .visible
                .iter()
                .map(|&i| entries[i].name.clone())
                .collect()
        };

        let mut f = Filter {
            types: vec!["TOML document".into(), "Lockfile".into(), "Markdown".into()],
            ..Filter::default()
        };
        assert_eq!(visible(&f), ["Cargo.toml", "Cargo.lock", "README.md"]);
        assert!(f.is_active());

        f.size = Some(SizeRule {
            at_least: true,
            bytes: 2_000,
        });
        assert_eq!(visible(&f), ["Cargo.lock", "README.md"]);

        f.modified = Some(Recency::LastWeek);
        assert_eq!(visible(&f), ["Cargo.lock"]);

        f.clear();
        assert!(!f.is_active());
        f.size = Some(SizeRule {
            at_least: false,
            bytes: 2_000,
        });
        assert_eq!(visible(&f), ["Cargo.toml"]);
    }

    #[test]
    fn size_rules_leave_out_folders() {
        let mut folder = entry("src", false);
        folder.is_dir = true;
        folder.size = None;
        let f = Filter {
            size: Some(SizeRule {
                at_least: false,
                bytes: u64::MAX,
            }),
            ..Filter::default()
        };
        assert!(!f.matches(&folder));
    }

    #[test]
    fn recency_boundaries() {
        let now = SystemTime::now();
        let today = Recency::Today.since(now);
        assert!(
            today <= now && now.duration_since(today).unwrap() < Duration::from_secs(25 * 3600)
        );
        assert!(Recency::ThisYear.since(now) <= today);
        assert_eq!(
            now.duration_since(Recency::LastWeek.since(now)).unwrap(),
            Duration::from_secs(7 * 86400)
        );
    }

    #[test]
    fn query_and_hidden_combine() {
        let entries = [
            entry("Cargo.toml", false),
            entry("cargo-secret", true),
            entry("README.md", false),
        ];
        let result = filter("cargo").apply(&entries);
        assert_eq!(result.visible, [0]);
        assert_eq!(result.hidden, 1);
        assert!(!filter("cargo").matches(&entries[1]));
    }
}
