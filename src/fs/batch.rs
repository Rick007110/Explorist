//! Batch rename: turning a rule into new names, checking them, and
//! renaming safely.

use std::collections::{HashMap, HashSet};
use std::fs;
use std::io;
use std::path::{Path, PathBuf};
use std::time::SystemTime;

use chrono::{DateTime, Local};
use regex::Regex;

use super::entry::Entry;
use super::jobs::Outcome;
use super::ops;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum DateSource {
    #[default]
    Modified,
    Created,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum CaseStyle {
    #[default]
    Lower,
    Upper,
    /// Every Word Capitalized
    Title,
    /// First letter only
    Sentence,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Rule {
    /// `{name}`, `{ext}`, `{n}` or `{n:02}`, `{date}`, `{parent}`.
    Template {
        template: String,
        start: u64,
        date: DateSource,
    },
    Replace {
        find: String,
        replace: String,
        match_case: bool,
    },
    Case(CaseStyle),
    /// Applied to the name without its extension; `$1` refers to groups.
    Regex {
        pattern: String,
        replace: String,
    },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct Options {
    pub lowercase_ext: bool,
    pub include_folders: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Status {
    Ready,
    /// The new name is the same as the old one.
    Unchanged,
    /// Something else in the folder already has that name.
    Taken,
    /// Two items would get the same name; the later one is skipped.
    Duplicate,
    Invalid(String),
}

impl Status {
    pub fn label(&self) -> String {
        match self {
            Self::Ready => "Ready".to_owned(),
            Self::Unchanged => "No change".to_owned(),
            Self::Taken => "Name taken · skip".to_owned(),
            Self::Duplicate => "Duplicate · skip".to_owned(),
            Self::Invalid(why) => why.clone(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Planned {
    pub from: PathBuf,
    pub old_name: String,
    pub new_name: String,
    pub status: Status,
}

/// Works out new names for `items` (in display order). `folder_names`
/// holds every name in the folder, to spot clashes.
pub fn plan(
    items: &[Entry],
    rule: &Rule,
    options: Options,
    folder_names: &[String],
) -> Result<Vec<Planned>, String> {
    let regex = match rule {
        Rule::Regex { pattern, .. } if !pattern.is_empty() => Some(
            Regex::new(pattern).map_err(|e| format!("Pattern: {}", first_line(&e.to_string())))?,
        ),
        _ => None,
    };
    let key = |name: &str| name.to_lowercase();
    let moving: HashSet<String> = items
        .iter()
        .filter(|e| options.include_folders || !e.is_dir)
        .map(|e| key(&e.name))
        .collect();
    let others: HashSet<String> = folder_names
        .iter()
        .map(|n| key(n))
        .filter(|n| !moving.contains(n))
        .collect();

    let mut taken_by_batch: HashMap<String, usize> = HashMap::new();
    let mut out = Vec::new();
    let mut n = match rule {
        Rule::Template { start, .. } => *start,
        _ => 1,
    };
    for entry in items {
        if entry.is_dir && !options.include_folders {
            continue;
        }
        let (stem, ext) = split(&entry.name, entry.is_dir);
        let mut new_name = match rule {
            Rule::Template { template, date, .. } => {
                let when = match date {
                    DateSource::Modified => entry.modified,
                    DateSource::Created => entry.created.or(entry.modified),
                };
                let parent = entry
                    .path
                    .parent()
                    .and_then(Path::file_name)
                    .map(|p| p.to_string_lossy().into_owned())
                    .unwrap_or_default();
                let name = fill_template(template, stem, ext, n, when, &parent);
                n += 1;
                name
            }
            Rule::Replace {
                find,
                replace,
                match_case,
            } => {
                if find.is_empty() {
                    entry.name.clone()
                } else if *match_case {
                    format!("{}{ext}", stem.replace(find.as_str(), replace))
                } else {
                    format!("{}{ext}", replace_ignore_case(stem, find, replace))
                }
            }
            Rule::Case(style) => format!("{}{ext}", change_case(stem, *style)),
            Rule::Regex { replace, .. } => match &regex {
                Some(re) => format!("{}{ext}", re.replace_all(stem, replace.as_str())),
                None => entry.name.clone(),
            },
        };
        if options.lowercase_ext
            && !entry.is_dir
            && let Some(dot) = new_name.rfind('.').filter(|&d| d > 0)
        {
            let lower = new_name[dot..].to_lowercase();
            new_name.replace_range(dot.., &lower);
        }

        let status = if new_name == entry.name {
            Status::Unchanged
        } else if let Err(err) = ops::validate_name(&new_name) {
            Status::Invalid(err.to_string())
        } else if others.contains(&key(&new_name)) {
            Status::Taken
        } else if taken_by_batch.contains_key(&key(&new_name)) {
            Status::Duplicate
        } else {
            Status::Ready
        };
        if matches!(status, Status::Ready | Status::Unchanged) {
            taken_by_batch.insert(key(&new_name), out.len());
        }
        out.push(Planned {
            from: entry.path.clone(),
            old_name: entry.name.clone(),
            new_name,
            status,
        });
    }
    // An item keeping its name blocks others from taking it.
    for i in 0..out.len() {
        if out[i].status == Status::Ready {
            let k = key(&out[i].new_name);
            if out
                .iter()
                .any(|o| o.status == Status::Unchanged && key(&o.old_name) == k)
            {
                out[i].status = Status::Duplicate;
            }
        }
    }
    Ok(out)
}

fn first_line(s: &str) -> &str {
    s.lines().find(|l| !l.trim().is_empty()).unwrap_or(s).trim()
}

/// "photo.JPG" → ("photo", ".JPG"); folders and dotfiles have no extension.
fn split(name: &str, is_dir: bool) -> (&str, &str) {
    if is_dir {
        return (name, "");
    }
    match name.rfind('.') {
        Some(dot) if dot > 0 => (&name[..dot], &name[dot..]),
        _ => (name, ""),
    }
}

fn fill_template(
    template: &str,
    stem: &str,
    ext: &str,
    n: u64,
    when: Option<SystemTime>,
    parent: &str,
) -> String {
    let mut out = String::new();
    let mut rest = template;
    while let Some(open) = rest.find('{') {
        out.push_str(&rest[..open]);
        let after = &rest[open + 1..];
        let Some(close) = after.find('}') else {
            out.push_str(&rest[open..]);
            return out;
        };
        let token = &after[..close];
        let (name, arg) = token.split_once(':').unwrap_or((token, ""));
        match name {
            "name" => out.push_str(stem),
            "ext" => out.push_str(ext),
            "parent" => out.push_str(parent),
            "date" => {
                if let Some(t) = when {
                    out.push_str(&DateTime::<Local>::from(t).format("%Y-%m-%d").to_string());
                }
            }
            "n" => {
                let width = arg.parse::<usize>().unwrap_or(0);
                out.push_str(&format!("{n:0width$}"));
            }
            _ => {
                // Not a token: keep it as typed.
                out.push('{');
                out.push_str(token);
                out.push('}');
            }
        }
        rest = &after[close + 1..];
    }
    out.push_str(rest);
    out
}

fn replace_ignore_case(text: &str, find: &str, replace: &str) -> String {
    let lower_text = text.to_lowercase();
    let lower_find = find.to_lowercase();
    // Only safe when lowercasing keeps byte offsets, i.e. the common case.
    if lower_text.len() != text.len() || lower_find.len() != find.len() {
        return text.replace(find, replace);
    }
    let mut out = String::new();
    let mut last = 0;
    for (i, _) in lower_text.match_indices(&lower_find) {
        out.push_str(&text[last..i]);
        out.push_str(replace);
        last = i + find.len();
    }
    out.push_str(&text[last..]);
    out
}

fn change_case(text: &str, style: CaseStyle) -> String {
    match style {
        CaseStyle::Lower => text.to_lowercase(),
        CaseStyle::Upper => text.to_uppercase(),
        CaseStyle::Title => {
            let mut out = String::new();
            let mut start = true;
            for c in text.chars() {
                if start {
                    out.extend(c.to_uppercase());
                } else {
                    out.extend(c.to_lowercase());
                }
                start = matches!(c, ' ' | '_' | '-' | '.');
            }
            out
        }
        CaseStyle::Sentence => {
            let lower = text.to_lowercase();
            let mut chars = lower.chars();
            match chars.next() {
                Some(first) => first.to_uppercase().chain(chars).collect(),
                None => String::new(),
            }
        }
    }
}

/// Renames each `(from, new name)` pair in its folder. Goes through
/// temporary names first, so swaps and chains (a→b, b→a) work. Never
/// overwrites: if a final name turns out to be taken, that item keeps a
/// recoverable name and is reported. Returns the pairs done as
/// `(old path, new path)`.
pub fn rename_all(pairs: &[(PathBuf, String)]) -> Outcome {
    let mut staged = Vec::new();
    let mut failed = Vec::new();
    for (i, (from, new_name)) in pairs.iter().enumerate() {
        let dir = from.parent().unwrap_or(Path::new(""));
        let temp = dir.join(format!(".explorist-rename-{}-{i}", std::process::id()));
        match fs::rename(from, &temp) {
            Ok(()) => staged.push((from.clone(), temp, dir.join(new_name))),
            Err(err) => failed.push((from.clone(), err.to_string())),
        }
    }
    let mut done = Vec::new();
    for (from, temp, to) in staged {
        let result = if fs::symlink_metadata(&to).is_ok() {
            Err(io::Error::from(io::ErrorKind::AlreadyExists))
        } else {
            fs::rename(&temp, &to)
        };
        match result {
            Ok(()) => done.push((from, to)),
            Err(err) => {
                // Put it back under its old name if that's free.
                let back = if fs::symlink_metadata(&from).is_err() {
                    fs::rename(&temp, &from).is_ok()
                } else {
                    false
                };
                let note = if back {
                    String::new()
                } else {
                    format!(" (left as {})", temp.display())
                };
                failed.push((from, format!("{err}{note}")));
            }
        }
    }
    Outcome {
        done,
        failed,
        cancelled: false,
    }
}

#[cfg(test)]
mod tests {
    use std::time::Duration;

    use super::*;
    use crate::fs::kind;

    fn entry(name: &str, is_dir: bool) -> Entry {
        let (kind, type_label) = kind::classify(name, is_dir);
        Entry {
            name: name.into(),
            path: PathBuf::from("/photos").join(name),
            is_dir,
            kind,
            type_label,
            size: Some(1),
            modified: Some(SystemTime::UNIX_EPOCH + Duration::from_secs(1_791_000_000)),
            created: None,
            hidden: false,
            system: false,
        }
    }

    fn names(planned: &[Planned]) -> Vec<String> {
        planned.iter().map(|p| p.new_name.clone()).collect()
    }

    fn template(t: &str) -> Rule {
        Rule::Template {
            template: t.into(),
            start: 1,
            date: DateSource::Modified,
        }
    }

    #[test]
    fn templates_fill_tokens() {
        let items = [
            entry("Screenshot A.PNG", false),
            entry("Screenshot B.png", false),
        ];
        let date = DateTime::<Local>::from(items[0].modified.unwrap())
            .format("%Y-%m-%d")
            .to_string();
        let opts = Options {
            lowercase_ext: true,
            include_folders: false,
        };
        let planned = plan(&items, &template("{date}_{n:02}{ext}"), opts, &[]).unwrap();
        assert_eq!(
            names(&planned),
            [format!("{date}_01.png"), format!("{date}_02.png")]
        );
        let planned = plan(
            &items,
            &template("{parent} - {name} {x}"),
            Options::default(),
            &[],
        )
        .unwrap();
        assert_eq!(planned[0].new_name, "photos - Screenshot A {x}");
    }

    #[test]
    fn find_and_replace() {
        let items = [entry("IMG_001.jpg", false), entry("img_002.jpg", false)];
        let rule = Rule::Replace {
            find: "img_".into(),
            replace: "Holiday ".into(),
            match_case: false,
        };
        assert_eq!(
            names(&plan(&items, &rule, Options::default(), &[]).unwrap()),
            ["Holiday 001.jpg", "Holiday 002.jpg"]
        );
        let rule = Rule::Replace {
            find: "img_".into(),
            replace: "x".into(),
            match_case: true,
        };
        let planned = plan(&items, &rule, Options::default(), &[]).unwrap();
        assert_eq!(planned[0].status, Status::Unchanged);
        assert_eq!(planned[1].new_name, "x002.jpg");
    }

    #[test]
    fn change_case_keeps_the_extension() {
        let items = [entry("my holiday-pics.JPG", false)];
        let title = plan(
            &items,
            &Rule::Case(CaseStyle::Title),
            Options::default(),
            &[],
        )
        .unwrap();
        assert_eq!(title[0].new_name, "My Holiday-Pics.JPG");
        let upper = plan(
            &items,
            &Rule::Case(CaseStyle::Upper),
            Options::default(),
            &[],
        )
        .unwrap();
        assert_eq!(upper[0].new_name, "MY HOLIDAY-PICS.JPG");
        let sentence = plan(
            &items,
            &Rule::Case(CaseStyle::Sentence),
            Options::default(),
            &[],
        )
        .unwrap();
        assert_eq!(sentence[0].new_name, "My holiday-pics.JPG");
    }

    #[test]
    fn regex_with_groups() {
        let items = [entry("Screenshot 2026-10-09 114702.png", false)];
        let rule = Rule::Regex {
            pattern: r"Screenshot (\d+)-(\d+)-(\d+).*".into(),
            replace: "$3.$2.$1".into(),
        };
        assert_eq!(
            plan(&items, &rule, Options::default(), &[]).unwrap()[0].new_name,
            "09.10.2026.png"
        );
        let bad = Rule::Regex {
            pattern: "(".into(),
            replace: String::new(),
        };
        assert!(plan(&items, &bad, Options::default(), &[]).is_err());
    }

    #[test]
    fn clashes_are_skipped() {
        let items = [
            entry("a.txt", false),
            entry("b.txt", false),
            entry("c.txt", false),
        ];
        let folder = ["a.txt", "b.txt", "c.txt", "taken.txt"].map(String::from);
        // a → taken (exists, not moving), b and c → same name.
        let rule = Rule::Template {
            template: "{n}".into(),
            start: 1,
            date: DateSource::Modified,
        };
        let planned = plan(&items, &rule, Options::default(), &folder).unwrap();
        assert!(planned.iter().all(|p| p.status == Status::Ready));

        let rule = Rule::Replace {
            find: "a".into(),
            replace: "taken".into(),
            match_case: true,
        };
        assert_eq!(
            plan(&items, &rule, Options::default(), &folder).unwrap()[0].status,
            Status::Taken
        );

        let same = template("same");
        let planned = plan(&items, &same, Options::default(), &folder).unwrap();
        assert_eq!(
            planned.iter().map(|p| p.status.clone()).collect::<Vec<_>>(),
            [Status::Ready, Status::Duplicate, Status::Duplicate]
        );
    }

    #[test]
    fn names_freed_by_the_batch_can_be_reused() {
        // 1 → 2 is fine because 2 is renamed to 3 in the same batch.
        let items = [entry("1.txt", false), entry("2.txt", false)];
        let folder = ["1.txt", "2.txt"].map(String::from);
        let rule = Rule::Template {
            template: "{n}{ext}".into(),
            start: 2,
            date: DateSource::Modified,
        };
        let planned = plan(&items, &rule, Options::default(), &folder).unwrap();
        assert_eq!(names(&planned), ["2.txt", "3.txt"]);
        assert!(planned.iter().all(|p| p.status == Status::Ready));
    }

    #[test]
    fn a_name_kept_by_another_item_blocks_it() {
        let items = [entry("a.txt", false), entry("b.txt", false)];
        let folder = ["a.txt", "b.txt"].map(String::from);
        let rule = Rule::Replace {
            find: "a".into(),
            replace: "b".into(),
            match_case: true,
        };
        let planned = plan(&items, &rule, Options::default(), &folder).unwrap();
        assert_eq!(planned[0].status, Status::Duplicate);
        assert_eq!(planned[1].status, Status::Unchanged);
    }

    #[test]
    fn invalid_names_and_folders() {
        let items = [entry("x.txt", false), entry("folder", true)];
        let planned = plan(&items, &template("CON{ext}"), Options::default(), &[]).unwrap();
        assert_eq!(planned.len(), 1, "folders are left out by default");
        assert!(matches!(planned[0].status, Status::Invalid(_)));
        let with_folders = Options {
            include_folders: true,
            ..Options::default()
        };
        assert_eq!(
            plan(&items, &template("{name}-1"), with_folders, &[])
                .unwrap()
                .len(),
            2
        );
    }

    #[test]
    fn rename_all_handles_swaps() {
        let dir = tempfile::tempdir().unwrap();
        let a = dir.path().join("a.txt");
        let b = dir.path().join("b.txt");
        fs::write(&a, b"A").unwrap();
        fs::write(&b, b"B").unwrap();
        let Outcome { done, failed, .. } =
            rename_all(&[(a.clone(), "b.txt".into()), (b.clone(), "a.txt".into())]);
        assert!(failed.is_empty(), "{failed:?}");
        assert_eq!(done.len(), 2);
        assert_eq!(fs::read(&a).unwrap(), b"B");
        assert_eq!(fs::read(&b).unwrap(), b"A");
    }

    #[test]
    fn rename_all_never_overwrites() {
        let dir = tempfile::tempdir().unwrap();
        let a = dir.path().join("a.txt");
        let keep = dir.path().join("keep.txt");
        fs::write(&a, b"A").unwrap();
        fs::write(&keep, b"K").unwrap();
        let Outcome { done, failed, .. } = rename_all(&[(a.clone(), "keep.txt".into())]);
        assert!(done.is_empty());
        assert_eq!(failed.len(), 1);
        assert_eq!(fs::read(&keep).unwrap(), b"K");
        assert_eq!(fs::read(&a).unwrap(), b"A", "put back under its old name");
    }
}
