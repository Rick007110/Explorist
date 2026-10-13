use std::fs;
use std::io;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use crate::fs::sort::SortOrder;

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
    fn defaults_check_for_updates() {
        assert!(Settings::default().updates.check_automatically);
    }
}
