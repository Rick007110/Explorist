use std::collections::BTreeMap;
use std::fs;
use std::io;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use crate::fs::format::DateFormat;
use crate::fs::sort::{SortColumn, SortDirection, SortOrder};

/// Folders that remember their own sort, at most.
const MAX_FOLDER_SORTS: usize = 500;

const APP_DIR: &str = "Explorist";
const FILE_NAME: &str = "settings.toml";

/// Everything the Settings screen can change. Missing keys take their
/// default, so older settings files keep working as fields are added.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Settings {
    pub theme: Theme,
    pub accent: Accent,
    pub density: Density,
    /// `None` follows the Windows "show animations" setting.
    pub reduce_motion: Option<bool>,
    pub sort: SortOrder,
    pub show_hidden: bool,
    /// Folders the user pinned under Favorites, in order.
    pub pinned: Vec<PathBuf>,
    pub updates: Updates,
    pub start_in: StartIn,
    /// The folder open when Explorist last closed.
    pub last_folder: Option<PathBuf>,
    pub date_format: DateFormat,
    /// Sorts chosen by hand, per folder (path → column and direction).
    pub folder_sorts: BTreeMap<String, FolderSort>,
    /// Custom shortcuts: command id → "Ctrl Shift N".
    pub shortcuts: BTreeMap<String, String>,
    /// The preview panel is open (Ctrl .).
    pub show_preview: bool,
    /// Show the folder as a grid of tiles instead of a list (Ctrl G).
    pub grid_view: bool,
    /// Folders open in tabs when Explorist closed, and which was active.
    pub open_tabs: Vec<PathBuf>,
    pub active_tab: usize,
    /// Show git badges and the branch (needs git installed).
    pub git_status: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum StartIn {
    #[default]
    Home,
    LastFolder,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct FolderSort {
    pub column: SortColumn,
    pub direction: SortDirection,
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            theme: Theme::System,
            accent: Accent::Teal,
            density: Density::Comfortable,
            reduce_motion: None,
            sort: SortOrder::default(),
            show_hidden: false,
            pinned: Vec::new(),
            updates: Updates::default(),
            start_in: StartIn::Home,
            last_folder: None,
            date_format: DateFormat::Iso,
            folder_sorts: BTreeMap::new(),
            shortcuts: BTreeMap::new(),
            show_preview: true,
            grid_view: false,
            open_tabs: Vec::new(),
            active_tab: 0,
            git_status: true,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Theme {
    Dark,
    Light,
    /// Match Windows' light/dark setting.
    #[default]
    System,
}

/// The four accents the design offers. Each has a dark and a light shade,
/// defined in the UI theme.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Accent {
    #[default]
    Teal,
    Blue,
    Orange,
    Purple,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Density {
    /// 30 px rows.
    #[default]
    Comfortable,
    /// 24 px rows.
    Compact,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct Updates {
    /// Look for a new version at startup, at most once a day.
    pub check_automatically: bool,
    /// Unix time of the last check, successful or not.
    pub last_checked: Option<i64>,
    /// A version the user chose to skip; no prompt until a newer one.
    pub skipped_version: Option<String>,
}

impl Default for Updates {
    fn default() -> Self {
        Self {
            check_automatically: true,
            last_checked: None,
            skipped_version: None,
        }
    }
}

/// What happened when loading settings.
#[derive(Debug)]
pub enum LoadOutcome {
    Loaded,
    /// No file yet; defaults are in use.
    Missing,
    /// The file couldn't be parsed. It was moved aside to `backup` so the
    /// user's edits aren't lost, and defaults are in use.
    Invalid {
        error: String,
        backup: PathBuf,
    },
}

impl Settings {
    /// The sort for `folder`: its own if the user sorted it by hand, else
    /// the default. Folders-first and natural order are always global.
    pub fn sort_for(&self, folder: &Path) -> SortOrder {
        let mut sort = self.sort;
        if let Some(own) = self.folder_sorts.get(&folder_key(folder)) {
            sort.column = own.column;
            sort.direction = own.direction;
        }
        sort
    }

    /// Remembers a sort chosen by hand for `folder`.
    pub fn remember_sort(&mut self, folder: &Path, sort: SortOrder) {
        let key = folder_key(folder);
        let own = FolderSort {
            column: sort.column,
            direction: sort.direction,
        };
        if own.column == self.sort.column && own.direction == self.sort.direction {
            self.folder_sorts.remove(&key);
            return;
        }
        if self.folder_sorts.len() >= MAX_FOLDER_SORTS && !self.folder_sorts.contains_key(&key) {
            let oldest = self.folder_sorts.keys().next().cloned();
            if let Some(oldest) = oldest {
                self.folder_sorts.remove(&oldest);
            }
        }
        self.folder_sorts.insert(key, own);
    }

    /// `%APPDATA%\Explorist\settings.toml`, or the platform equivalent.
    pub fn default_path() -> Option<PathBuf> {
        dirs::config_dir().map(|dir| dir.join(APP_DIR).join(FILE_NAME))
    }

    /// Loads settings, falling back to defaults when the file is missing or
    /// broken. Only an I/O failure while reading is an error.
    pub fn load(path: &Path) -> io::Result<(Self, LoadOutcome)> {
        let text = match fs::read_to_string(path) {
            Ok(text) => text,
            Err(err) if err.kind() == io::ErrorKind::NotFound => {
                return Ok((Self::default(), LoadOutcome::Missing));
            }
            Err(err) => return Err(err),
        };
        match toml::from_str(&text) {
            Ok(settings) => Ok((settings, LoadOutcome::Loaded)),
            Err(err) => {
                let backup = path.with_extension("toml.bad");
                fs::rename(path, &backup)?;
                Ok((
                    Self::default(),
                    LoadOutcome::Invalid {
                        error: err.message().to_owned(),
                        backup,
                    },
                ))
            }
        }
    }

    /// Saves settings. Writes a temporary file first and renames it over the
    /// old one, so a crash mid-write can't leave a half-written file.
    pub fn save(&self, path: &Path) -> io::Result<()> {
        if let Some(dir) = path.parent() {
            fs::create_dir_all(dir)?;
        }
        let text = toml::to_string_pretty(self).map_err(io::Error::other)?;
        let tmp = path.with_extension("toml.tmp");
        fs::write(&tmp, text)?;
        fs::rename(&tmp, path)
    }
}

/// Folder paths compare case-insensitively on Windows.
fn folder_key(folder: &Path) -> String {
    let text = folder.to_string_lossy();
    if cfg!(windows) {
        text.to_lowercase()
    } else {
        text.into_owned()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::fs::sort::{SortColumn, SortDirection};

    #[test]
    fn missing_file_gives_defaults() {
        let dir = tempfile::tempdir().unwrap();
        let (settings, outcome) = Settings::load(&dir.path().join("settings.toml")).unwrap();
        assert_eq!(settings, Settings::default());
        assert!(matches!(outcome, LoadOutcome::Missing));
    }

    #[test]
    fn save_then_load_round_trips() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("nested").join("settings.toml");
        let settings = Settings {
            theme: Theme::Light,
            accent: Accent::Purple,
            density: Density::Compact,
            reduce_motion: Some(true),
            sort: SortOrder::default().clicked(SortColumn::Modified),
            show_hidden: true,
            pinned: vec![PathBuf::from("C:\\Users\\someone\\code")],
            updates: Updates {
                check_automatically: false,
                last_checked: Some(1_791_900_000),
                skipped_version: Some("0.2.0".into()),
            },
            start_in: StartIn::LastFolder,
            last_folder: Some(PathBuf::from("D:\\photos")),
            date_format: DateFormat::DayFirst,
            folder_sorts: BTreeMap::from([(
                "d:\\photos".to_owned(),
                FolderSort {
                    column: SortColumn::Modified,
                    direction: SortDirection::Descending,
                },
            )]),
            shortcuts: BTreeMap::from([("rename".to_owned(), "Ctrl R".to_owned())]),
            show_preview: false,
            grid_view: true,
            open_tabs: vec![PathBuf::from("C:\\Users"), PathBuf::from("D:\\photos")],
            active_tab: 1,
            git_status: false,
        };
        settings.save(&path).unwrap();
        assert!(!path.with_extension("toml.tmp").exists());

        let (loaded, outcome) = Settings::load(&path).unwrap();
        assert!(matches!(outcome, LoadOutcome::Loaded));
        assert_eq!(loaded, settings);
    }

    #[test]
    fn partial_files_fill_in_defaults() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("settings.toml");
        fs::write(
            &path,
            "theme = \"dark\"\n\n[sort]\ndirection = \"descending\"\n",
        )
        .unwrap();

        let (settings, _) = Settings::load(&path).unwrap();
        assert_eq!(settings.theme, Theme::Dark);
        assert_eq!(settings.sort.direction, SortDirection::Descending);
        assert!(settings.sort.folders_first);
        assert!(settings.updates.check_automatically);
    }

    #[test]
    fn broken_files_are_moved_aside() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("settings.toml");
        fs::write(&path, "theme = \"neon\"").unwrap();

        let (settings, outcome) = Settings::load(&path).unwrap();
        assert_eq!(settings, Settings::default());
        let LoadOutcome::Invalid { backup, .. } = outcome else {
            panic!("expected Invalid, got {outcome:?}");
        };
        assert!(!path.exists());
        assert_eq!(fs::read_to_string(backup).unwrap(), "theme = \"neon\"");
    }

    #[test]
    fn folders_remember_their_own_sort() {
        let mut settings = Settings::default();
        let photos = Path::new("/photos");
        let by_date = SortOrder::default().clicked(SortColumn::Modified);
        settings.remember_sort(photos, by_date);
        assert_eq!(settings.sort_for(photos).column, SortColumn::Modified);
        assert_eq!(
            settings.sort_for(Path::new("/other")).column,
            SortColumn::Name
        );

        // Going back to the default forgets the entry.
        settings.remember_sort(photos, SortOrder::default());
        assert!(settings.folder_sorts.is_empty());

        // Global options still apply to folders with their own sort.
        settings.remember_sort(photos, by_date);
        settings.sort.folders_first = false;
        assert!(!settings.sort_for(photos).folders_first);
    }

    #[test]
    fn folder_sorts_are_capped() {
        let mut settings = Settings::default();
        let by_size = SortOrder::default().clicked(SortColumn::Size);
        for i in 0..MAX_FOLDER_SORTS + 10 {
            settings.remember_sort(Path::new(&format!("/f{i}")), by_size);
        }
        assert_eq!(settings.folder_sorts.len(), MAX_FOLDER_SORTS);
    }

    #[test]
    fn defaults_check_for_updates() {
        assert!(Settings::default().updates.check_automatically);
    }
}
