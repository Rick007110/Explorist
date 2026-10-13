//! Deciding which entries the file list shows.

use super::entry::Entry;

/// The name filter (Ctrl F) plus the "show hidden files" toggle (Ctrl H).
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Filter {
    query: String,
    /// Lowercased copy of `query`, compared against lowercased names.
    needle: Vec<char>,
    wildcard: bool,
    pub show_hidden: bool,
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

    /// Whether the name filter narrows the list at all.
    pub fn is_active(&self) -> bool {
        !self.needle.is_empty()
    }

    pub fn matches(&self, entry: &Entry) -> bool {
        if !self.show_hidden && (entry.hidden || entry.system) {
            return false;
        }
        self.matches_name(&entry.name)
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
        let mut out = Filtered::default();
        for (i, entry) in entries.iter().enumerate() {
            if !self.show_hidden && (entry.hidden || entry.system) {
                out.hidden += 1;
            } else if self.matches_name(&entry.name) {
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
