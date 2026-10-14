//! Open with: the system's app chooser on Windows; elsewhere, a list of
//! installed apps for Explorist's own picker.

use std::io;
use std::path::{Path, PathBuf};

/// An app that can open files.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct App {
    pub name: String,
    /// The .desktop file on Linux, the .app bundle on macOS.
    pub id: PathBuf,
    /// Registered for this kind of file.
    pub recommended: bool,
    /// Linux: the Exec line from the .desktop file.
    pub exec: String,
}

/// Windows shows its own chooser ("How do you want to open this file?").
pub const SYSTEM_CHOOSER: bool = cfg!(windows);

/// Shows the Windows chooser for `path`, on its own thread because the
/// dialog blocks until closed.
#[cfg(windows)]
pub fn choose(path: &Path) {
    use std::os::windows::ffi::OsStrExt;
    use windows_sys::Win32::UI::Shell::{
        OAIF_ALLOW_REGISTRATION, OAIF_EXEC, OAIF_REGISTER_EXT, OPENASINFO, SHOpenWithDialog,
    };
    let wide: Vec<u16> = path.as_os_str().encode_wide().chain(Some(0)).collect();
    std::thread::spawn(move || {
        let info = OPENASINFO {
            pcszFile: wide.as_ptr(),
            pcszClass: std::ptr::null(),
            oaifInFlags: OAIF_EXEC | OAIF_ALLOW_REGISTRATION | OAIF_REGISTER_EXT,
        };
        // SAFETY: `info` and the path it points to live until the call returns.
        unsafe {
            SHOpenWithDialog(std::ptr::null_mut(), &info);
        }
    });
}

/// Installed apps, the ones registered for `path`'s type first, each
/// group by name.
pub fn apps_for(path: &Path) -> Vec<App> {
    let mut apps = installed(path);
    apps.sort_by(|a, b| {
        b.recommended
            .cmp(&a.recommended)
            .then_with(|| super::sort::natural_cmp(&a.name, &b.name))
    });
    apps.dedup_by(|a, b| a.name == b.name && a.recommended == b.recommended);
    apps
}

/// Opens `files` in `app`.
pub fn launch(app: &App, files: &[PathBuf]) -> io::Result<()> {
    launch_impl(app, files)
}

#[cfg(target_os = "macos")]
fn installed(_path: &Path) -> Vec<App> {
    let mut roots = vec![
        PathBuf::from("/Applications"),
        PathBuf::from("/System/Applications"),
        PathBuf::from("/Applications/Utilities"),
        PathBuf::from("/System/Applications/Utilities"),
    ];
    if let Some(home) = dirs::home_dir() {
        roots.push(home.join("Applications"));
    }
    let mut apps = Vec::new();
    for root in roots {
        let Ok(read) = std::fs::read_dir(&root) else {
            continue;
        };
        for item in read.flatten() {
            let path = item.path();
            if path.extension().is_some_and(|e| e == "app") {
                let name = path
                    .file_stem()
                    .map(|s| s.to_string_lossy().into_owned())
                    .unwrap_or_default();
                apps.push(App {
                    name,
                    id: path,
                    recommended: false,
                    exec: String::new(),
                });
            }
        }
    }
    apps
}

#[cfg(target_os = "macos")]
fn launch_impl(app: &App, files: &[PathBuf]) -> io::Result<()> {
    std::process::Command::new("open")
        .arg("-a")
        .arg(&app.id)
        .args(files)
        .spawn()
        .map(drop)
}

#[cfg(windows)]
fn installed(_path: &Path) -> Vec<App> {
    Vec::new()
}

#[cfg(windows)]
fn launch_impl(_app: &App, _files: &[PathBuf]) -> io::Result<()> {
    Err(io::Error::other("use the system chooser"))
}

#[cfg(not(any(windows, target_os = "macos")))]
fn installed(path: &Path) -> Vec<App> {
    let mime = linux::mime_type(path);
    linux::desktop_dirs()
        .iter()
        .flat_map(|dir| std::fs::read_dir(dir).into_iter().flatten().flatten())
        .filter(|item| item.path().extension().is_some_and(|e| e == "desktop"))
        .filter_map(|item| {
            let text = std::fs::read_to_string(item.path()).ok()?;
            linux::parse_desktop(&text, item.path(), mime.as_deref())
        })
        .collect()
}

#[cfg(not(any(windows, target_os = "macos")))]
fn launch_impl(app: &App, files: &[PathBuf]) -> io::Result<()> {
    let argv = linux::command_line(&app.exec, files);
    let (program, args) = argv
        .split_first()
        .ok_or_else(|| io::Error::other("the app has no command"))?;
    std::process::Command::new(program)
        .args(args)
        .spawn()
        .map(drop)
}

/// Desktop entries and MIME types. Plain parsing, so it's tested on every
/// platform.
#[cfg_attr(any(windows, target_os = "macos"), allow(dead_code))]
mod linux {
    use std::path::{Path, PathBuf};

    use super::App;

    pub fn desktop_dirs() -> Vec<PathBuf> {
        let mut dirs = Vec::new();
        if let Some(data) = dirs::data_dir() {
            dirs.push(data.join("applications"));
        }
        let system = std::env::var("XDG_DATA_DIRS")
            .unwrap_or_else(|_| "/usr/local/share:/usr/share".to_owned());
        for dir in system.split(':').filter(|d| !d.is_empty()) {
            dirs.push(Path::new(dir).join("applications"));
        }
        dirs.push(PathBuf::from("/var/lib/flatpak/exports/share/applications"));
        dirs
    }

    /// The MIME type for `path`'s name, from shared-mime-info's glob list.
    pub fn mime_type(path: &Path) -> Option<String> {
        let name = path.file_name()?.to_string_lossy().to_lowercase();
        let globs = ["/usr/share/mime/globs2", "/usr/local/share/mime/globs2"]
            .iter()
            .find_map(|p| std::fs::read_to_string(p).ok())?;
        match_glob(&globs, &name)
    }

    /// Best match from a globs2 file ("weight:type:glob" lines): highest
    /// weight, then longest pattern.
    pub fn match_glob(globs: &str, name: &str) -> Option<String> {
        globs
            .lines()
            .filter(|l| !l.starts_with('#'))
            .filter_map(|l| {
                let mut parts = l.splitn(4, ':');
                let weight: u32 = parts.next()?.parse().ok()?;
                let mime = parts.next()?;
                let glob = parts.next()?.to_lowercase();
                let hit = match glob.strip_prefix('*') {
                    Some(suffix) if !suffix.contains(['*', '?', '[']) => name.ends_with(suffix),
                    _ => glob == name,
                };
                hit.then(|| (weight, glob.len(), mime.to_owned()))
            })
            .max_by_key(|(weight, len, _)| (*weight, *len))
            .map(|(_, _, mime)| mime)
    }

    /// An app from a .desktop file, unless it's hidden or can't take files.
    pub fn parse_desktop(text: &str, id: PathBuf, mime: Option<&str>) -> Option<App> {
        let mut in_entry = false;
        let (mut name, mut exec, mut types) = (None, None, String::new());
        let mut hidden = false;
        let mut application = false;
        for line in text.lines() {
            let line = line.trim();
            if line.starts_with('[') {
                in_entry = line == "[Desktop Entry]";
                continue;
            }
            if !in_entry {
                continue;
            }
            let Some((key, value)) = line.split_once('=') else {
                continue;
            };
            match key.trim() {
                "Name" => name = Some(value.trim().to_owned()),
                "Exec" => exec = Some(value.trim().to_owned()),
                "MimeType" => types = value.to_owned(),
                "Type" => application = value.trim() == "Application",
                "NoDisplay" | "Hidden" if value.trim() == "true" => hidden = true,
                _ => {}
            }
        }
        let exec = exec?;
        let takes_files = ["%f", "%F", "%u", "%U"].iter().any(|c| exec.contains(c));
        if hidden || !application || !takes_files {
            return None;
        }
        let recommended = mime.is_some_and(|m| types.split(';').any(|t| t == m));
        Some(App {
            name: name?,
            id,
            recommended,
            exec,
        })
    }

    /// The Exec line split into arguments, with the field codes replaced.
    pub fn command_line(exec: &str, files: &[PathBuf]) -> Vec<String> {
        let mut argv = Vec::new();
        let mut used_files = false;
        for word in split_words(exec) {
            match word.as_str() {
                "%f" | "%u" => {
                    // Apps that take one file at a time get the first.
                    if let Some(f) = files.first() {
                        argv.push(f.display().to_string());
                    }
                    used_files = true;
                }
                "%F" | "%U" => {
                    argv.extend(files.iter().map(|f| f.display().to_string()));
                    used_files = true;
                }
                // Icon, name and location codes aren't needed.
                "%i" | "%c" | "%k" => {}
                _ => argv.push(word.replace("%%", "%")),
            }
        }
        if !used_files {
            argv.extend(files.iter().map(|f| f.display().to_string()));
        }
        argv
    }

    /// Splits on spaces, keeping "double quoted" words together.
    fn split_words(line: &str) -> Vec<String> {
        let mut words = Vec::new();
        let mut word = String::new();
        let mut quoted = false;
        let mut chars = line.chars();
        while let Some(c) = chars.next() {
            match c {
                '"' => quoted = !quoted,
                '\\' if quoted => {
                    if let Some(next) = chars.next() {
                        word.push(next);
                    }
                }
                ' ' | '\t' if !quoted => {
                    if !word.is_empty() {
                        words.push(std::mem::take(&mut word));
                    }
                }
                c => word.push(c),
            }
        }
        if !word.is_empty() {
            words.push(word);
        }
        words
    }

    #[cfg(test)]
    mod tests {
        use super::*;

        const GLOBS: &str = "# comment\n50:text/plain:*.txt\n50:text/markdown:*.md\n\
                             55:application/x-trash:*~\n50:image/png:*.png\n\
                             50:text/x-makefile:makefile\n60:text/x-cmake:*.cmake.txt\n";

        #[test]
        fn globs_pick_the_best_match() {
            assert_eq!(
                match_glob(GLOBS, "notes.txt").as_deref(),
                Some("text/plain")
            );
            assert_eq!(
                match_glob(GLOBS, "readme.md").as_deref(),
                Some("text/markdown")
            );
            assert_eq!(
                match_glob(GLOBS, "makefile").as_deref(),
                Some("text/x-makefile")
            );
            assert_eq!(
                match_glob(GLOBS, "lists.cmake.txt").as_deref(),
                Some("text/x-cmake")
            );
            assert_eq!(match_glob(GLOBS, "archive.zip"), None);
        }

        #[test]
        fn desktop_files_parse() {
            let text = "[Desktop Entry]\nType=Application\nName=Text Editor\n\
                        Name[nl]=Teksteditor\nExec=gedit %U\nMimeType=text/plain;text/markdown;\n\
                        [Desktop Action new-window]\nName=New Window\nExec=gedit --new-window\n";
            let app =
                parse_desktop(text, PathBuf::from("gedit.desktop"), Some("text/plain")).unwrap();
            assert_eq!(app.name, "Text Editor");
            assert_eq!(app.exec, "gedit %U");
            assert!(app.recommended);
            let other =
                parse_desktop(text, PathBuf::from("gedit.desktop"), Some("image/png")).unwrap();
            assert!(!other.recommended);
        }

        #[test]
        fn hidden_or_fileless_entries_are_skipped() {
            let hidden = "[Desktop Entry]\nType=Application\nName=X\nExec=x %f\nNoDisplay=true\n";
            assert_eq!(parse_desktop(hidden, PathBuf::new(), None), None);
            let no_files = "[Desktop Entry]\nType=Application\nName=Settings\nExec=settings\n";
            assert_eq!(parse_desktop(no_files, PathBuf::new(), None), None);
            let link = "[Desktop Entry]\nType=Link\nName=Site\nExec=x %u\n";
            assert_eq!(parse_desktop(link, PathBuf::new(), None), None);
        }

        #[test]
        fn exec_lines_expand() {
            let files = [PathBuf::from("/a b/one.txt"), PathBuf::from("/two.txt")];
            assert_eq!(
                command_line("gedit %U", &files),
                ["gedit", "/a b/one.txt", "/two.txt"]
            );
            assert_eq!(
                command_line("\"/opt/My App/app\" --open %f %i", &files),
                ["/opt/My App/app", "--open", "/a b/one.txt"]
            );
            assert_eq!(
                command_line("viewer --pct=100%%", &files[..1]),
                ["viewer", "--pct=100%", "/a b/one.txt"]
            );
        }
    }
}
