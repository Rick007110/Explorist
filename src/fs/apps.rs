//! The apps installed on this computer, for finding and starting them from
//! quick search.
//!
//! - Windows: everything in the Start menu's "All apps", desktop and Store
//!   apps alike, read from the shell's Apps folder. Started through the
//!   shell, the way the Start menu does it.
//! - macOS: the `.app` bundles in the Applications folders.
//! - Linux: the desktop entries (`.desktop` files) that menus show.

use std::io;
use std::path::PathBuf;

/// One app.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct App {
    pub name: String,
    /// What starts it: an app id (Windows), a bundle (macOS) or a desktop
    /// entry (Linux).
    pub target: String,
    /// The file behind it, to show in Explorist, if there is one.
    pub file: Option<PathBuf>,
    pub icon: Icon,
}

/// Where an app's icon comes from.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Icon {
    /// The Windows shell knows it by this name.
    Shell(String),
    /// Quick Look draws it from the bundle (macOS).
    Bundle(PathBuf),
    /// An image file (Linux icon themes).
    File(PathBuf),
    None,
}

/// Every app, sorted by name.
pub fn list() -> Vec<App> {
    let mut apps = platform::list();
    apps.retain(|a| !a.name.trim().is_empty() && !a.name.to_lowercase().contains("uninstall"));
    apps.sort_by_key(|a| a.name.to_lowercase());
    apps.dedup_by(|a, b| a.name.eq_ignore_ascii_case(&b.name));
    apps
}

/// Starts `app`.
pub fn launch(app: &App) -> io::Result<()> {
    platform::launch(app)
}

/// Matched parts of a name, as char ranges.
pub type Ranges = Vec<(usize, usize)>;

/// The best `limit` apps for `query`, best first, with the matched parts
/// of their names.
pub fn find(apps: &[App], query: &str, limit: usize) -> Vec<(usize, Ranges)> {
    let terms: Vec<String> = query.split_whitespace().map(str::to_lowercase).collect();
    if terms.is_empty() {
        return Vec::new();
    }
    let whole = terms.join(" ");
    let mut found: Vec<(i32, usize, Ranges)> = apps
        .iter()
        .enumerate()
        .filter_map(|(i, app)| {
            let (score, ranges) = score(&app.name, &terms, &whole)?;
            Some((score, i, ranges))
        })
        .collect();
    found.sort_by(|a, b| b.0.cmp(&a.0).then(a.1.cmp(&b.1)));
    found.truncate(limit);
    found.into_iter().map(|(_, i, r)| (i, r)).collect()
}

/// How well `name` fits: the whole name, its start, the start of a word,
/// anywhere, or its initials ("vsc" for Visual Studio Code).
fn score(name: &str, terms: &[String], whole: &str) -> Option<(i32, Ranges)> {
    let lower = name.to_lowercase();
    let chars: Vec<char> = lower.chars().collect();
    let word_start = |at: usize| at == 0 || !chars[at - 1].is_alphanumeric();
    let found: Option<Ranges> = terms
        .iter()
        .map(|term| {
            // Prefer a match at the start of a word.
            let starts: Vec<usize> = lower.match_indices(term.as_str()).map(|(b, _)| b).collect();
            let byte = starts
                .iter()
                .copied()
                .find(|&b| word_start(lower[..b].chars().count()))
                .or_else(|| starts.first().copied())?;
            let start = lower[..byte].chars().count();
            Some((start, start + term.chars().count()))
        })
        .collect();
    let penalty = (chars.len() as i32 - whole.chars().count() as i32).max(0);
    if let Some(mut ranges) = found {
        ranges.sort();
        let tier = if lower == whole {
            1000
        } else if lower.starts_with(whole) {
            800
        } else if ranges.iter().all(|&(a, _)| word_start(a)) {
            600
        } else {
            300
        };
        return Some((tier - penalty, ranges));
    }
    // Initials: one query word, each letter starting a word in order.
    if terms.len() != 1 {
        return None;
    }
    let starts: Vec<usize> = (0..chars.len())
        .filter(|&i| word_start(i) && chars[i].is_alphanumeric())
        .collect();
    let wanted: Vec<char> = whole.chars().collect();
    if wanted.len() < 2 || wanted.len() > starts.len() {
        return None;
    }
    let mut ranges = Vec::new();
    let mut next = 0;
    for &at in &starts {
        if next < wanted.len() && chars[at] == wanted[next] {
            ranges.push((at, at + 1));
            next += 1;
        }
    }
    (next == wanted.len()).then_some((500 - penalty, ranges))
}

/// A desktop entry's name, command and icon, if it's an app menus show.
#[cfg_attr(any(windows, target_os = "macos"), allow(dead_code))]
fn desktop_entry(text: &str) -> Option<(String, String, Option<String>)> {
    let mut in_entry = false;
    let (mut name, mut exec, mut icon) = (None, None, None);
    let mut app = false;
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
            "Icon" => icon = Some(value.trim().to_owned()),
            "Type" => app = value.trim() == "Application",
            "NoDisplay" | "Hidden" if value.trim() == "true" => return None,
            _ => {}
        }
    }
    app.then_some(())?;
    Some((name?, exec?, icon.filter(|i| !i.is_empty())))
}

/// A desktop entry's `Exec` line as a program and its arguments, without
/// the `%f`-style placeholders for files (none are passed).
#[cfg_attr(any(windows, target_os = "macos"), allow(dead_code))]
fn exec_args(exec: &str) -> Vec<String> {
    let mut args = Vec::new();
    let mut word = String::new();
    let (mut quoted, mut started, mut escaped) = (false, false, false);
    for c in exec.chars() {
        if escaped {
            word.push(c);
            escaped = false;
            continue;
        }
        match c {
            '\\' if quoted => escaped = true,
            '"' => {
                quoted = !quoted;
                started = true;
            }
            ' ' | '\t' if !quoted => {
                if started {
                    args.push(std::mem::take(&mut word));
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
        args.push(word);
    }
    args.into_iter()
        .filter(|a| !(a.len() == 2 && a.starts_with('%')))
        .map(|a| a.replace("%%", "%"))
        .collect()
}

#[cfg(windows)]
mod platform {
    use std::ffi::c_void;
    use std::io;

    use windows_sys::Win32::System::Com::CoTaskMemFree;
    use windows_sys::Win32::UI::Shell::SHGetKnownFolderItem;
    use windows_sys::core::{GUID, HRESULT};

    use super::{App, Icon};

    const FOLDERID_APPS_FOLDER: GUID = GUID::from_u128(0x1e87508d_89c2_42f0_8a7e_645a0f50ca58);
    const BHID_ENUM_ITEMS: GUID = GUID::from_u128(0x94f60519_2850_4924_aa5a_d15e84868039);
    const IID_ISHELLITEM: GUID = GUID::from_u128(0x43826d1e_e718_42ee_bc55_a1e261c37bfe);
    const IID_IENUMSHELLITEMS: GUID = GUID::from_u128(0x70629033_e363_4a28_a567_0db78006e6d7);
    const SIGDN_NORMALDISPLAY: i32 = 0;
    const SIGDN_PARENTRELATIVEPARSING: i32 = 0x8001_8001_u32 as i32;

    /// `IShellItem`'s methods, in order.
    #[repr(C)]
    struct ShellItemVtbl {
        _query_interface: usize,
        _add_ref: usize,
        release: unsafe extern "system" fn(*mut c_void) -> u32,
        bind_to_handler: unsafe extern "system" fn(
            *mut c_void,
            *mut c_void,
            *const GUID,
            *const GUID,
            *mut *mut c_void,
        ) -> HRESULT,
        _get_parent: usize,
        get_display_name: unsafe extern "system" fn(*mut c_void, i32, *mut *mut u16) -> HRESULT,
    }

    /// `IEnumShellItems`'s methods, in order.
    #[repr(C)]
    struct EnumVtbl {
        _query_interface: usize,
        _add_ref: usize,
        release: unsafe extern "system" fn(*mut c_void) -> u32,
        next: unsafe extern "system" fn(*mut c_void, u32, *mut *mut c_void, *mut u32) -> HRESULT,
    }

    /// The Start menu's "All apps": the shell's Apps folder.
    pub fn list() -> Vec<App> {
        crate::fs::system_thumbs::com();
        let mut apps = Vec::new();
        // SAFETY: COM calls through the documented vtable layouts above;
        // every interface we get is released, every string freed.
        unsafe {
            let mut folder: *mut c_void = std::ptr::null_mut();
            let hr = SHGetKnownFolderItem(
                &FOLDERID_APPS_FOLDER,
                0,
                std::ptr::null_mut(),
                &IID_ISHELLITEM,
                &mut folder,
            );
            if hr < 0 || folder.is_null() {
                return apps;
            }
            let folder_vtbl = *(folder as *const *const ShellItemVtbl);
            let mut items: *mut c_void = std::ptr::null_mut();
            let hr = ((*folder_vtbl).bind_to_handler)(
                folder,
                std::ptr::null_mut(),
                &BHID_ENUM_ITEMS,
                &IID_IENUMSHELLITEMS,
                &mut items,
            );
            ((*folder_vtbl).release)(folder);
            if hr < 0 || items.is_null() {
                return apps;
            }
            let enum_vtbl = *(items as *const *const EnumVtbl);
            loop {
                let mut item: *mut c_void = std::ptr::null_mut();
                let mut fetched = 0u32;
                let hr = ((*enum_vtbl).next)(items, 1, &mut item, &mut fetched);
                if hr != 0 || fetched != 1 || item.is_null() {
                    break;
                }
                let vtbl = *(item as *const *const ShellItemVtbl);
                let text = |kind: i32| {
                    let mut ptr: *mut u16 = std::ptr::null_mut();
                    if ((*vtbl).get_display_name)(item, kind, &mut ptr) < 0 || ptr.is_null() {
                        return None;
                    }
                    let len = (0..).take_while(|&i| *ptr.add(i) != 0).count();
                    let s = String::from_utf16_lossy(std::slice::from_raw_parts(ptr, len));
                    CoTaskMemFree(ptr.cast());
                    Some(s)
                };
                if let (Some(name), Some(id)) =
                    (text(SIGDN_NORMALDISPLAY), text(SIGDN_PARENTRELATIVEPARSING))
                {
                    // Desktop apps are often their own path; show those.
                    let file = std::path::PathBuf::from(&id);
                    let file = (file.is_absolute() && file.exists()).then_some(file);
                    apps.push(App {
                        icon: Icon::Shell(format!("shell:AppsFolder\\{id}")),
                        name,
                        target: id,
                        file,
                    });
                }
                ((*vtbl).release)(item);
            }
            ((*enum_vtbl).release)(items);
        }
        apps
    }

    /// Through the shell, as the Start menu does: works for Store apps too.
    pub fn launch(app: &App) -> io::Result<()> {
        use std::os::windows::process::CommandExt;
        const CREATE_NO_WINDOW: u32 = 0x0800_0000;
        std::process::Command::new("explorer.exe")
            .arg(format!("shell:AppsFolder\\{}", app.target))
            .creation_flags(CREATE_NO_WINDOW)
            .spawn()
            .map(drop)
    }
}

#[cfg(target_os = "macos")]
mod platform {
    use std::io;
    use std::path::{Path, PathBuf};

    use super::{App, Icon};

    /// `.app` bundles in the Applications folders, one level of subfolders
    /// deep (Utilities and the like).
    pub fn list() -> Vec<App> {
        let mut dirs: Vec<PathBuf> = [
            "/Applications",
            "/System/Applications",
            "/System/Applications/Utilities",
            "/Applications/Utilities",
            "/System/Library/CoreServices/Applications",
        ]
        .iter()
        .map(PathBuf::from)
        .collect();
        if let Some(home) = dirs::home_dir() {
            dirs.push(home.join("Applications"));
        }
        let mut apps = Vec::new();
        for dir in dirs {
            collect(&dir, 0, &mut apps);
        }
        apps
    }

    fn collect(dir: &Path, depth: usize, apps: &mut Vec<App>) {
        let Ok(read) = std::fs::read_dir(dir) else {
            return;
        };
        for item in read.flatten() {
            let path = item.path();
            let is_app = path
                .extension()
                .is_some_and(|e| e.eq_ignore_ascii_case("app"));
            if is_app {
                let name = path
                    .file_stem()
                    .map(|n| n.to_string_lossy().into_owned())
                    .unwrap_or_default();
                apps.push(App {
                    name,
                    target: path.to_string_lossy().into_owned(),
                    file: Some(path.clone()),
                    icon: Icon::Bundle(path),
                });
            } else if depth == 0 && item.file_type().is_ok_and(|t| t.is_dir()) {
                collect(&path, depth + 1, apps);
            }
        }
    }

    pub fn launch(app: &App) -> io::Result<()> {
        std::process::Command::new("/usr/bin/open")
            .arg(&app.target)
            .spawn()
            .map(drop)
    }
}

#[cfg(not(any(windows, target_os = "macos")))]
mod platform {
    use std::io;
    use std::path::{Path, PathBuf};

    use super::{App, Icon, desktop_entry, exec_args};

    /// Where desktop entries live: the user's, the system's, Flatpak's and
    /// Snap's.
    fn entry_dirs() -> Vec<PathBuf> {
        let mut dirs = Vec::new();
        if let Some(data) = dirs::data_dir() {
            dirs.push(data.join("applications"));
            dirs.push(data.join("flatpak/exports/share/applications"));
        }
        let system = std::env::var("XDG_DATA_DIRS")
            .unwrap_or_else(|_| "/usr/local/share:/usr/share".to_owned());
        dirs.extend(system.split(':').map(|d| Path::new(d).join("applications")));
        dirs.push(PathBuf::from("/var/lib/flatpak/exports/share/applications"));
        dirs.push(PathBuf::from("/var/lib/snapd/desktop/applications"));
        dirs
    }

    pub fn list() -> Vec<App> {
        let mut apps = Vec::new();
        let mut seen = std::collections::HashSet::new();
        for dir in entry_dirs() {
            let Ok(read) = std::fs::read_dir(&dir) else {
                continue;
            };
            for item in read.flatten() {
                let path = item.path();
                if path.extension().is_none_or(|e| e != "desktop") {
                    continue;
                }
                // The first one found wins (the user's own over the system's).
                if !seen.insert(item.file_name()) {
                    continue;
                }
                let Ok(text) = std::fs::read_to_string(&path) else {
                    continue;
                };
                let Some((name, exec, icon)) = desktop_entry(&text) else {
                    continue;
                };
                apps.push(App {
                    name,
                    target: exec,
                    icon: icon
                        .and_then(|i| find_icon(&i))
                        .map_or(Icon::None, Icon::File),
                    file: Some(path),
                });
            }
        }
        apps
    }

    /// An icon by theme name ("firefox") or path.
    fn find_icon(icon: &str) -> Option<PathBuf> {
        let path = Path::new(icon);
        if path.is_absolute() {
            return path.is_file().then(|| path.to_path_buf());
        }
        let mut bases = Vec::new();
        if let Some(data) = dirs::data_dir() {
            bases.push(data.join("icons"));
            bases.push(data.join("flatpak/exports/share/icons"));
        }
        let system = std::env::var("XDG_DATA_DIRS")
            .unwrap_or_else(|_| "/usr/local/share:/usr/share".to_owned());
        bases.extend(system.split(':').map(|d| Path::new(d).join("icons")));
        bases.push(PathBuf::from("/var/lib/flatpak/exports/share/icons"));
        for base in &bases {
            for size in ["scalable", "256x256", "128x128", "96x96", "64x64", "48x48"] {
                for ext in ["svg", "png"] {
                    let file = base
                        .join("hicolor")
                        .join(size)
                        .join("apps")
                        .join(format!("{icon}.{ext}"));
                    if file.is_file() {
                        return Some(file);
                    }
                }
            }
        }
        ["png", "svg"]
            .iter()
            .map(|ext| Path::new("/usr/share/pixmaps").join(format!("{icon}.{ext}")))
            .find(|f| f.is_file())
    }

    pub fn launch(app: &App) -> io::Result<()> {
        let args = exec_args(&app.target);
        let (program, rest) = args
            .split_first()
            .ok_or_else(|| io::Error::other("the app has no command"))?;
        std::process::Command::new(program)
            .args(rest)
            .stdin(std::process::Stdio::null())
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::null())
            .spawn()
            .map(drop)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn app(name: &str) -> App {
        App {
            name: name.to_owned(),
            target: String::new(),
            file: None,
            icon: Icon::None,
        }
    }

    fn names<'a>(apps: &'a [App], query: &str) -> Vec<&'a str> {
        find(apps, query, 10)
            .into_iter()
            .map(|(i, _)| apps[i].name.as_str())
            .collect()
    }

    #[test]
    fn apps_rank_by_how_well_the_name_fits() {
        let apps = [
            app("Visual Studio Code"),
            app("Calculator"),
            app("Code Writer"),
            app("Notepad"),
            app("Notepad++"),
            app("Microsoft Teams"),
        ];
        assert_eq!(names(&apps, "notepad"), ["Notepad", "Notepad++"]);
        assert_eq!(names(&apps, "code"), ["Code Writer", "Visual Studio Code"]);
        assert_eq!(names(&apps, "calc"), ["Calculator"]);
        // Initials.
        assert_eq!(names(&apps, "vsc"), ["Visual Studio Code"]);
        assert_eq!(names(&apps, "teams micro"), ["Microsoft Teams"]);
        assert!(names(&apps, "zebra").is_empty());
        let (_, ranges) = &find(&apps, "studio", 1)[0];
        assert_eq!(ranges, &[(7, 13)]);
    }

    #[test]
    fn desktop_entries_are_read() {
        let entry = "[Desktop Entry]\nType=Application\nName=Text Editor\nName[de]=Texteditor\nExec=gnome-text-editor %U\nIcon=org.gnome.TextEditor\n\n[Desktop Action new]\nName=New Window\nExec=gnome-text-editor --new-window\n";
        assert_eq!(
            desktop_entry(entry),
            Some((
                "Text Editor".to_owned(),
                "gnome-text-editor %U".to_owned(),
                Some("org.gnome.TextEditor".to_owned())
            ))
        );
        assert_eq!(
            desktop_entry("[Desktop Entry]\nType=Application\nName=X\nExec=x\nNoDisplay=true\n"),
            None
        );
        assert_eq!(
            desktop_entry("[Desktop Entry]\nType=Link\nName=X\nExec=x\n"),
            None
        );
        assert_eq!(
            exec_args(r#""/opt/My App/app" --flag %F 100%%"#),
            ["/opt/My App/app", "--flag", "100%"]
        );
    }
}
