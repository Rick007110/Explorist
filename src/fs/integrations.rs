//! Other apps in the right-click menu: archivers (WinRAR, 7-Zip) and
//! editors (Sublime Text, Notepad++, VS Code), found where they're usually
//! installed, plus any program the user adds.
//!
//! An action is a program and its arguments, with placeholders filled in
//! from the selection:
//!
//! - `{files}`: every selected path, each as its own argument
//! - `{file}`: the first selected path
//! - `{dir}`: the open folder
//! - `{name}`: the first item's name without its extension (for archive
//!   names: "photos" → photos.rar)
//! - `{stem}`: an archive's name without its archive extension, for
//!   "Extract to" folders

use std::ffi::OsString;
use std::io;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

/// What an action works on.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
pub enum Applies {
    Files,
    Folders,
    #[default]
    Any,
    /// Archive files only (zip, rar, 7z, tar...).
    Archives,
}

impl Applies {
    pub fn label(self) -> &'static str {
        match self {
            Self::Files => "Files",
            Self::Folders => "Folders",
            Self::Any => "Files and folders",
            Self::Archives => "Archives",
        }
    }

    /// Whether the selection (`(path, is_dir)` pairs) fits.
    pub fn fits(self, selection: &[(PathBuf, bool)]) -> bool {
        !selection.is_empty()
            && selection.iter().all(|(path, is_dir)| match self {
                Self::Files => !is_dir,
                Self::Folders => *is_dir,
                Self::Any => true,
                Self::Archives => !is_dir && is_archive(path),
            })
    }
}

/// Archive types the archivers open.
pub fn is_archive(path: &Path) -> bool {
    let name = path
        .file_name()
        .map(|n| n.to_string_lossy().to_lowercase())
        .unwrap_or_default();
    [
        ".zip", ".rar", ".7z", ".tar", ".gz", ".tgz", ".bz2", ".xz", ".iso", ".cab", ".zst",
    ]
    .iter()
    .any(|ext| name.ends_with(ext))
}

/// An archive's name without its archive ending ("photos.tar.gz" →
/// "photos", "backup.7z" → "backup"), for the folder it extracts into.
fn archive_stem(path: &Path) -> String {
    let name = path
        .file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_default();
    let lower = name.to_lowercase();
    // Two-part endings first, so "x.tar.gz" doesn't become "x.tar".
    const ENDINGS: [&str; 17] = [
        ".tar.gz", ".tar.bz2", ".tar.xz", ".tar.zst", ".tgz", ".tbz2", ".txz", ".zip", ".rar",
        ".7z", ".tar", ".gz", ".bz2", ".xz", ".zst", ".iso", ".cab",
    ];
    ENDINGS
        .iter()
        .find(|end| lower.ends_with(*end) && lower.len() > end.len())
        .map_or(name.clone(), |end| {
            name[..name.len() - end.len()].to_owned()
        })
}

/// One menu entry.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Action {
    pub label: String,
    /// The label when several items are selected, if it should differ.
    pub label_many: Option<String>,
    pub args: Vec<String>,
    pub applies: Applies,
}

/// An app in the menu, found on this computer or added by hand.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct App {
    /// Stable id for settings: "winrar", or "custom:<name>".
    pub id: String,
    pub name: String,
    pub program: PathBuf,
    pub actions: Vec<Action>,
}

/// A program the user added in Settings → Integrations.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(default)]
pub struct CustomApp {
    pub name: String,
    pub program: PathBuf,
    /// Arguments, split on spaces; "double quotes" keep spaces in.
    pub args: String,
    pub applies: Applies,
}

impl CustomApp {
    pub fn to_app(&self) -> App {
        let args = if self.args.trim().is_empty() {
            vec!["{files}".to_owned()]
        } else {
            split_args(&self.args)
        };
        App {
            id: format!("custom:{}", self.name),
            name: self.name.clone(),
            program: self.program.clone(),
            actions: vec![Action {
                label: format!("Open with {}", self.name),
                label_many: None,
                args,
                applies: self.applies,
            }],
        }
    }
}

/// An app Explorist knows about, and whether it's installed.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Known {
    pub id: &'static str,
    pub name: &'static str,
    /// Where it was found, if anywhere.
    pub found: Option<PathBuf>,
}

/// The built-in apps for this platform, found or not, and the ones found
/// as menu-ready [`App`]s.
pub fn detect() -> (Vec<Known>, Vec<App>) {
    let mut known = Vec::new();
    let mut apps = Vec::new();
    for spec in specs() {
        let found = spec.candidates.iter().find(|p| p.is_file()).cloned();
        if let Some(program) = &found {
            apps.push(App {
                id: spec.id.to_owned(),
                name: spec.name.to_owned(),
                program: program.clone(),
                actions: (spec.actions)(),
            });
        }
        known.push(Known {
            id: spec.id,
            name: spec.name,
            found,
        });
    }
    (known, apps)
}

struct Spec {
    id: &'static str,
    name: &'static str,
    candidates: Vec<PathBuf>,
    actions: fn() -> Vec<Action>,
}

fn action(label: &str, args: &[&str], applies: Applies) -> Action {
    Action {
        label: label.to_owned(),
        label_many: None,
        args: args.iter().map(|a| (*a).to_owned()).collect(),
        applies,
    }
}

/// `dir` + separator + `rest`, as a template.
fn in_dir(rest: &str) -> String {
    format!("{{dir}}{}{rest}", std::path::MAIN_SEPARATOR)
}

#[cfg_attr(not(windows), allow(dead_code))]
fn winrar_actions() -> Vec<Action> {
    vec![
        Action {
            label: "Add to “{name}.rar”".to_owned(),
            label_many: None,
            args: vec![
                "a".into(),
                "-ep1".into(),
                in_dir("{name}.rar"),
                "{files}".into(),
            ],
            applies: Applies::Any,
        },
        Action {
            label: "Extract here".to_owned(),
            label_many: None,
            args: vec!["x".into(), "-o-".into(), "{file}".into(), in_dir("")],
            applies: Applies::Archives,
        },
        Action {
            label: "Extract to “{stem}”".to_owned(),
            label_many: Some("Extract each to its own folder".to_owned()),
            args: vec![
                "x".into(),
                "{file}".into(),
                in_dir(&format!("{{stem}}{}", std::path::MAIN_SEPARATOR)),
            ],
            applies: Applies::Archives,
        },
    ]
}

fn seven_zip_actions() -> Vec<Action> {
    vec![
        Action {
            label: "Add to “{name}.7z”".to_owned(),
            label_many: None,
            args: vec!["a".into(), in_dir("{name}.7z"), "{files}".into()],
            applies: Applies::Any,
        },
        Action {
            label: "Extract here".to_owned(),
            label_many: None,
            args: vec!["x".into(), "{file}".into(), "-o{dir}".into(), "-aos".into()],
            applies: Applies::Archives,
        },
        Action {
            label: "Extract to “{stem}”".to_owned(),
            label_many: Some("Extract each to its own folder".to_owned()),
            args: vec![
                "x".into(),
                "{file}".into(),
                format!("-o{}", in_dir("{stem}")),
            ],
            applies: Applies::Archives,
        },
    ]
}

fn sublime_actions() -> Vec<Action> {
    vec![action("Open in Sublime Text", &["{files}"], Applies::Any)]
}

#[cfg_attr(not(windows), allow(dead_code))]
fn notepad_pp_actions() -> Vec<Action> {
    vec![action("Edit with Notepad++", &["{files}"], Applies::Files)]
}

fn vscode_actions() -> Vec<Action> {
    vec![action("Open in VS Code", &["{files}"], Applies::Any)]
}

/// Where each app usually lives on this platform.
fn specs() -> Vec<Spec> {
    #[cfg(windows)]
    {
        let env = |key: &str| std::env::var_os(key).map(PathBuf::from);
        let program_dirs: Vec<PathBuf> = ["ProgramFiles", "ProgramFiles(x86)", "ProgramW6432"]
            .iter()
            .filter_map(|k| env(k))
            .collect();
        let under =
            |rel: &str| -> Vec<PathBuf> { program_dirs.iter().map(|d| d.join(rel)).collect() };
        let mut vscode = under("Microsoft VS Code\\Code.exe");
        if let Some(local) = env("LOCALAPPDATA") {
            vscode.insert(0, local.join("Programs\\Microsoft VS Code\\Code.exe"));
        }
        vec![
            Spec {
                id: "winrar",
                name: "WinRAR",
                candidates: under("WinRAR\\WinRAR.exe"),
                actions: winrar_actions,
            },
            Spec {
                id: "7zip",
                name: "7-Zip",
                candidates: under("7-Zip\\7zG.exe"),
                actions: seven_zip_actions,
            },
            Spec {
                id: "sublime",
                name: "Sublime Text",
                candidates: [
                    under("Sublime Text\\sublime_text.exe"),
                    under("Sublime Text 3\\sublime_text.exe"),
                ]
                .concat(),
                actions: sublime_actions,
            },
            Spec {
                id: "notepad++",
                name: "Notepad++",
                candidates: under("Notepad++\\notepad++.exe"),
                actions: notepad_pp_actions,
            },
            Spec {
                id: "vscode",
                name: "VS Code",
                candidates: vscode,
                actions: vscode_actions,
            },
        ]
    }
    #[cfg(target_os = "macos")]
    {
        // Apps started from Finder get a short PATH, so look where
        // Homebrew puts them too (Apple silicon, then Intel).
        let mut seven = in_path(&["7zz", "7z"]);
        for dir in ["/opt/homebrew/bin", "/usr/local/bin"] {
            for name in ["7zz", "7z"] {
                seven.push(Path::new(dir).join(name));
            }
        }
        // Apps can be in the user's own Applications folder too.
        let apps = |rel: &str| -> Vec<PathBuf> {
            let mut found = vec![Path::new("/Applications").join(rel)];
            if let Some(home) = dirs::home_dir() {
                found.push(home.join("Applications").join(rel));
            }
            found
        };
        vec![
            Spec {
                id: "7zip",
                name: "7-Zip",
                candidates: seven,
                actions: seven_zip_actions,
            },
            Spec {
                id: "sublime",
                name: "Sublime Text",
                candidates: apps("Sublime Text.app/Contents/SharedSupport/bin/subl"),
                actions: sublime_actions,
            },
            Spec {
                id: "vscode",
                name: "VS Code",
                candidates: apps("Visual Studio Code.app/Contents/Resources/app/bin/code"),
                actions: vscode_actions,
            },
        ]
    }
    #[cfg(not(any(windows, target_os = "macos")))]
    {
        let mut sublime = in_path(&["subl"]);
        sublime.push(PathBuf::from("/opt/sublime_text/sublime_text"));
        vec![
            Spec {
                id: "7zip",
                name: "7-Zip",
                candidates: in_path(&["7zz", "7z"]),
                actions: seven_zip_actions,
            },
            Spec {
                id: "sublime",
                name: "Sublime Text",
                candidates: sublime,
                actions: sublime_actions,
            },
            Spec {
                id: "vscode",
                name: "VS Code",
                candidates: in_path(&["code"]),
                actions: vscode_actions,
            },
        ]
    }
}

/// Programs by name in the folders of `PATH`.
#[cfg_attr(windows, allow(dead_code))]
fn in_path(names: &[&str]) -> Vec<PathBuf> {
    let Some(path) = std::env::var_os("PATH") else {
        return Vec::new();
    };
    std::env::split_paths(&path)
        .flat_map(|dir| names.iter().map(move |n| dir.join(n)))
        .collect()
}

/// The values placeholders stand for.
pub struct Context<'a> {
    pub selection: &'a [PathBuf],
    pub dir: &'a Path,
}

impl Context<'_> {
    fn first(&self) -> Option<&Path> {
        self.selection.first().map(PathBuf::as_path)
    }

    fn name(&self) -> String {
        match self.first() {
            Some(p) if p.is_dir() => p.file_name(),
            Some(p) => p.file_stem(),
            None => None,
        }
        .map_or_else(
            || "Archive".to_owned(),
            |n| n.to_string_lossy().into_owned(),
        )
    }

    fn stem(&self) -> String {
        self.first().map(archive_stem).unwrap_or_default()
    }

    fn fill(&self, text: &str) -> String {
        text.replace("{dir}", &self.dir.to_string_lossy())
            .replace(
                "{file}",
                &self
                    .first()
                    .map(|p| p.to_string_lossy().into_owned())
                    .unwrap_or_default(),
            )
            .replace("{name}", &self.name())
            .replace("{stem}", &self.stem())
    }

    /// Whether `action` runs once per selected item: it names one item
    /// (`{file}`, `{stem}`) rather than taking them all (`{files}`).
    fn per_item(&self, action: &Action) -> bool {
        self.selection.len() > 1 && !action.args.iter().any(|a| a == "{files}")
    }

    /// The menu label, with names filled in.
    pub fn label(&self, action: &Action) -> String {
        match &action.label_many {
            Some(many) if self.selection.len() > 1 => many.clone(),
            _ => self.fill(&action.label),
        }
    }

    /// The argument lists to run `action` with: one, or one per selected
    /// item for actions that work on a single item (extracting several
    /// archives extracts each).
    pub fn runs(&self, action: &Action) -> Vec<Vec<OsString>> {
        if self.per_item(action) {
            self.selection
                .iter()
                .map(|item| {
                    Context {
                        selection: std::slice::from_ref(item),
                        dir: self.dir,
                    }
                    .args(action)
                })
                .collect()
        } else {
            vec![self.args(action)]
        }
    }

    fn args(&self, action: &Action) -> Vec<OsString> {
        let mut out = Vec::new();
        for arg in &action.args {
            if arg == "{files}" {
                out.extend(self.selection.iter().map(|p| p.as_os_str().to_owned()));
            } else {
                out.push(OsString::from(self.fill(arg)));
            }
        }
        out
    }
}

/// Starts `program` with `args`, in `dir`.
pub fn run(program: &Path, args: &[OsString], dir: &Path) -> io::Result<()> {
    #[cfg(target_os = "macos")]
    let (program, args) = &app_bundle_command(program, args);
    let mut command = std::process::Command::new(program);
    command.args(args).current_dir(dir);
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        // No console window for GUI apps started from here.
        const CREATE_NO_WINDOW: u32 = 0x0800_0000;
        command.creation_flags(CREATE_NO_WINDOW);
    }
    command.spawn().map(drop)
}

/// A macOS app (`Kate.app`) is a folder, so it's started with `open -a`:
/// paths go to it as documents to open, anything else after `--args`.
/// Other programs run as they are.
#[cfg_attr(not(target_os = "macos"), allow(dead_code))]
fn app_bundle_command(program: &Path, args: &[OsString]) -> (PathBuf, Vec<OsString>) {
    let bundle = program.is_dir()
        && program
            .extension()
            .is_some_and(|e| e.eq_ignore_ascii_case("app"));
    if !bundle {
        return (program.to_path_buf(), args.to_vec());
    }
    let mut out = vec![OsString::from("-a"), program.as_os_str().to_owned()];
    if args.iter().all(|a| Path::new(a).exists()) {
        out.extend(args.iter().cloned());
    } else {
        out.push("--args".into());
        out.extend(args.iter().cloned());
    }
    (PathBuf::from("/usr/bin/open"), out)
}

/// Splits on spaces, keeping "double quoted" parts together.
pub fn split_args(line: &str) -> Vec<String> {
    let mut words = Vec::new();
    let mut word = String::new();
    let mut quoted = false;
    let mut started = false;
    for c in line.chars() {
        match c {
            '"' => {
                quoted = !quoted;
                started = true;
            }
            ' ' | '\t' if !quoted => {
                if started {
                    words.push(std::mem::take(&mut word));
                    started = false;
                }
            }
            c => {
                word.push(c);
                started = true;
            }
        }
    }
    if started {
        words.push(word);
    }
    words
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn placeholders_fill_in() {
        let dir = tempfile::tempdir().unwrap();
        let file = dir.path().join("photos.tar.gz");
        std::fs::write(&file, b"").unwrap();
        let other = dir.path().join("notes.txt");
        let selection = [file.clone(), other.clone()];
        let ctx = Context {
            selection: &selection,
            dir: dir.path(),
        };
        let action = action(
            "Do {name} {stem}",
            &["x", "{files}", "-o{dir}", "{file}"],
            Applies::Any,
        );
        assert_eq!(ctx.label(&action), "Do photos.tar photos");
        let args = ctx.args(&action);
        assert_eq!(ctx.runs(&action), std::slice::from_ref(&args));
        assert_eq!(
            args,
            [
                OsString::from("x"),
                file.clone().into_os_string(),
                other.into_os_string(),
                OsString::from(format!("-o{}", dir.path().display())),
                file.into_os_string(),
            ]
        );
    }

    #[test]
    fn folders_name_archives_after_themselves() {
        let dir = tempfile::tempdir().unwrap();
        let folder = dir.path().join("Holiday 2026");
        std::fs::create_dir(&folder).unwrap();
        let selection = [folder];
        let ctx = Context {
            selection: &selection,
            dir: dir.path(),
        };
        assert_eq!(ctx.label(&winrar_actions()[0]), "Add to “Holiday 2026.rar”");
    }

    #[test]
    fn extracting_several_archives_extracts_each() {
        let dir = tempfile::tempdir().unwrap();
        let a = dir.path().join("a.zip");
        let b = dir.path().join("b.tar.gz");
        let selection = [a.clone(), b.clone()];
        let ctx = Context {
            selection: &selection,
            dir: dir.path(),
        };
        let [add, here, to] = &seven_zip_actions()[..] else {
            panic!("three actions")
        };
        // Adding takes them all at once.
        assert_eq!(ctx.runs(add).len(), 1);
        // Extracting runs once per archive, each into its own folder.
        let runs = ctx.runs(to);
        assert_eq!(runs.len(), 2);
        assert_eq!(runs[0][1], a.as_os_str());
        assert_eq!(runs[1][1], b.as_os_str());
        let folder = |run: &Vec<OsString>| run[2].to_string_lossy().into_owned();
        assert!(folder(&runs[0]).ends_with("a"));
        assert!(folder(&runs[1]).ends_with("b"));
        assert_eq!(ctx.runs(here).len(), 2);
        assert_eq!(ctx.label(to), "Extract each to its own folder");
        assert_eq!(ctx.label(here), "Extract here");
    }

    #[test]
    fn app_bundles_open_with_open() {
        let dir = tempfile::tempdir().unwrap();
        let bundle = dir.path().join("Kate.app");
        std::fs::create_dir(&bundle).unwrap();
        let file = dir.path().join("notes.txt");
        std::fs::write(&file, b"").unwrap();
        let (program, args) = app_bundle_command(&bundle, &[file.clone().into_os_string()]);
        assert_eq!(program, Path::new("/usr/bin/open"));
        assert_eq!(
            args,
            [
                OsString::from("-a"),
                bundle.clone().into_os_string(),
                file.clone().into_os_string()
            ]
        );
        let (_, args) = app_bundle_command(&bundle, &["--new".into(), file.clone().into()]);
        assert_eq!(args[2], "--args");
        // A plain program runs as it is.
        let plain = dir.path().join("tool");
        std::fs::write(&plain, b"").unwrap();
        let (program, args) = app_bundle_command(&plain, &[file.clone().into()]);
        assert_eq!((program, args), (plain, vec![file.into_os_string()]));
    }

    #[test]
    fn extract_folders_drop_the_archive_ending() {
        let stem = |name: &str| archive_stem(Path::new(name));
        assert_eq!(
            stem("Ars_Icon_Theme_for_Windows_10.7z"),
            "Ars_Icon_Theme_for_Windows_10"
        );
        assert_eq!(stem("backup.RAR"), "backup");
        assert_eq!(stem("photos.tar.gz"), "photos");
        assert_eq!(stem("logs.tar.zst"), "logs");
        assert_eq!(stem("disc.iso"), "disc");
        assert_eq!(stem("v1.2.zip"), "v1.2");
        assert_eq!(stem(".7z"), ".7z");
    }

    #[test]
    fn what_actions_apply_to() {
        let zip = (PathBuf::from("a.zip"), false);
        let txt = (PathBuf::from("a.txt"), false);
        let dir = (PathBuf::from("a"), true);
        assert!(Applies::Archives.fits(std::slice::from_ref(&zip)));
        assert!(!Applies::Archives.fits(&[zip.clone(), txt.clone()]));
        assert!(Applies::Files.fits(&[zip.clone(), txt.clone()]));
        assert!(!Applies::Files.fits(&[txt.clone(), dir.clone()]));
        assert!(Applies::Folders.fits(std::slice::from_ref(&dir)));
        assert!(Applies::Any.fits(&[txt, dir]));
        assert!(!Applies::Any.fits(&[]));
    }

    #[test]
    fn custom_arguments_split_with_quotes() {
        assert_eq!(
            split_args(r#"--new-window "{file}"  -x "a b""#),
            ["--new-window", "{file}", "-x", "a b"]
        );
        let app = CustomApp {
            name: "Kate".into(),
            program: PathBuf::from("/usr/bin/kate"),
            args: String::new(),
            applies: Applies::Files,
        }
        .to_app();
        assert_eq!(app.actions[0].args, ["{files}"]);
        assert_eq!(app.actions[0].label, "Open with Kate");
    }
}
