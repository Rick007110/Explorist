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

/// One menu entry.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Action {
    pub label: String,
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
            args: vec!["x".into(), "-o-".into(), "{file}".into(), in_dir("")],
            applies: Applies::Archives,
        },
        Action {
            label: "Extract to “{stem}”".to_owned(),
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
            args: vec!["a".into(), in_dir("{name}.7z"), "{files}".into()],
            applies: Applies::Any,
        },
        Action {
            label: "Extract here".to_owned(),
            args: vec!["x".into(), "{file}".into(), "-o{dir}".into(), "-aos".into()],
            applies: Applies::Archives,
        },
        Action {
            label: "Extract to “{stem}”".to_owned(),
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
        let mut seven = in_path(&["7zz", "7z"]);
        seven.push(PathBuf::from("/opt/homebrew/bin/7zz"));
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
                candidates: vec![PathBuf::from(
                    "/Applications/Sublime Text.app/Contents/SharedSupport/bin/subl",
                )],
                actions: sublime_actions,
            },
            Spec {
                id: "vscode",
                name: "VS Code",
                candidates: vec![PathBuf::from(
                    "/Applications/Visual Studio Code.app/Contents/Resources/app/bin/code",
                )],
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
        self.first().map(super::archive::stem).unwrap_or_default()
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

    /// The menu label, with names filled in.
    pub fn label(&self, action: &Action) -> String {
        self.fill(&action.label)
    }

    /// The arguments to run `action` with.
    pub fn args(&self, action: &Action) -> Vec<OsString> {
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
