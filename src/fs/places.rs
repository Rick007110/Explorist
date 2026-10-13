//! The sidebar's places: standard folders and drives.

use std::path::{Path, PathBuf};

/// A standard folder shown under Favorites.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Place {
    pub label: String,
    pub path: PathBuf,
    pub icon: PlaceIcon,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PlaceIcon {
    Home,
    Desktop,
    Downloads,
    Documents,
    Folder,
}

/// Home, Desktop, Downloads and Documents, skipping any this system lacks.
pub fn standard_places() -> Vec<Place> {
    [
        ("Home", dirs::home_dir(), PlaceIcon::Home),
        ("Desktop", dirs::desktop_dir(), PlaceIcon::Desktop),
        ("Downloads", dirs::download_dir(), PlaceIcon::Downloads),
        ("Documents", dirs::document_dir(), PlaceIcon::Documents),
    ]
    .into_iter()
    .filter_map(|(label, path, icon)| {
        Some(Place {
            label: label.to_owned(),
            path: path?,
            icon,
        })
    })
    .collect()
}

/// A place pinned by the user, labelled with its folder name.
pub fn pinned_place(path: &Path) -> Place {
    let label = path
        .file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_else(|| path.display().to_string());
    Place {
        label,
        path: path.to_path_buf(),
        icon: PlaceIcon::Folder,
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DriveKind {
    Fixed,
    Removable,
    Network,
    Optical,
    Other,
}

/// A drive under Drives in the sidebar.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Drive {
    /// Root path, e.g. `C:\`.
    pub root: PathBuf,
    /// Volume label as set by the user, e.g. "Windows". Empty if none.
    pub volume_label: String,
    pub kind: DriveKind,
    pub free_bytes: Option<u64>,
    pub total_bytes: Option<u64>,
}

impl Drive {
    /// "Windows (C:)", or a generic name like "Local Disk (C:)" when the
    /// volume has no label.
    pub fn display_name(&self) -> String {
        let root = self.root.to_string_lossy();
        let letter = root.trim_end_matches(['\\', '/']);
        let name = if self.volume_label.is_empty() {
            match self.kind {
                DriveKind::Fixed => "Local Disk",
                DriveKind::Removable => "USB Drive",
                DriveKind::Network => "Network Drive",
                DriveKind::Optical => "CD Drive",
                DriveKind::Other => "Drive",
            }
        } else {
            &self.volume_label
        };
        if letter.is_empty() {
            name.to_owned()
        } else {
            format!("{name} ({letter})")
        }
    }

    /// Fraction of the drive in use, for the bar under its name.
    pub fn used_fraction(&self) -> Option<f32> {
        let (free, total) = (self.free_bytes?, self.total_bytes?);
        (total > 0).then(|| 1.0 - free as f32 / total as f32)
    }
}

/// Lists the drives. Querying a disconnected network drive can block for
/// seconds, so call this off the UI thread.
#[cfg(windows)]
pub fn drives() -> Vec<Drive> {
    use windows_sys::Win32::Storage::FileSystem::GetLogicalDrives;

    // SAFETY: takes no arguments and only returns a bit mask.
    let mask = unsafe { GetLogicalDrives() };
    (0..26u8)
        .filter(|bit| mask & (1 << bit) != 0)
        .map(|bit| windows::drive(char::from(b'A' + bit)))
        .collect()
}

/// Outside Windows there's a single root.
#[cfg(not(windows))]
pub fn drives() -> Vec<Drive> {
    vec![Drive {
        root: PathBuf::from("/"),
        volume_label: "File system".to_owned(),
        kind: DriveKind::Fixed,
        free_bytes: None,
        total_bytes: None,
    }]
}

#[cfg(windows)]
mod windows {
    use std::path::PathBuf;
    use std::ptr;

    use windows_sys::Win32::Storage::FileSystem::{
        GetDiskFreeSpaceExW, GetDriveTypeW, GetVolumeInformationW,
    };

    use super::{Drive, DriveKind};

    // From winbase.h; windows-sys keeps these behind an unrelated feature.
    const DRIVE_REMOVABLE: u32 = 2;
    const DRIVE_FIXED: u32 = 3;
    const DRIVE_REMOTE: u32 = 4;
    const DRIVE_CDROM: u32 = 5;

    pub(super) fn drive(letter: char) -> Drive {
        let root = format!("{letter}:\\");
        let wide: Vec<u16> = root.encode_utf16().chain(Some(0)).collect();

        // SAFETY: `wide` is a NUL-terminated UTF-16 string that outlives
        // each call, and every out-pointer is either null (allowed by the
        // API) or points at a local of the documented type and size.
        let kind = match unsafe { GetDriveTypeW(wide.as_ptr()) } {
            DRIVE_FIXED => DriveKind::Fixed,
            DRIVE_REMOVABLE => DriveKind::Removable,
            DRIVE_REMOTE => DriveKind::Network,
            DRIVE_CDROM => DriveKind::Optical,
            _ => DriveKind::Other,
        };

        let mut label = [0u16; 261];
        let has_label = unsafe {
            GetVolumeInformationW(
                wide.as_ptr(),
                label.as_mut_ptr(),
                label.len() as u32,
                ptr::null_mut(),
                ptr::null_mut(),
                ptr::null_mut(),
                ptr::null_mut(),
                0,
            )
        } != 0;
        let volume_label = if has_label {
            let len = label.iter().position(|&c| c == 0).unwrap_or(label.len());
            String::from_utf16_lossy(&label[..len])
        } else {
            String::new()
        };

        let (mut free, mut total) = (0u64, 0u64);
        let has_space =
            unsafe { GetDiskFreeSpaceExW(wide.as_ptr(), &mut free, &mut total, ptr::null_mut()) }
                != 0;

        Drive {
            root: PathBuf::from(root),
            volume_label,
            kind,
            free_bytes: has_space.then_some(free),
            total_bytes: has_space.then_some(total),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn drive(root: &str, label: &str, kind: DriveKind) -> Drive {
        Drive {
            root: PathBuf::from(root),
            volume_label: label.to_owned(),
            kind,
            free_bytes: None,
            total_bytes: None,
        }
    }

    #[test]
    fn drive_names() {
        assert_eq!(
            drive("C:\\", "Windows", DriveKind::Fixed).display_name(),
            "Windows (C:)"
        );
        assert_eq!(
            drive("D:\\", "", DriveKind::Fixed).display_name(),
            "Local Disk (D:)"
        );
        assert_eq!(
            drive("E:\\", "", DriveKind::Removable).display_name(),
            "USB Drive (E:)"
        );
        assert_eq!(
            drive("Z:\\", "", DriveKind::Network).display_name(),
            "Network Drive (Z:)"
        );
        assert_eq!(
            drive("/", "File system", DriveKind::Fixed).display_name(),
            "File system"
        );
    }

    #[test]
    fn used_fraction() {
        let mut d = drive("C:\\", "", DriveKind::Fixed);
        assert_eq!(d.used_fraction(), None);
        d.free_bytes = Some(25);
        d.total_bytes = Some(100);
        assert_eq!(d.used_fraction(), Some(0.75));
        d.total_bytes = Some(0);
        assert_eq!(d.used_fraction(), None);
    }

    #[test]
    fn there_is_always_a_drive() {
        let drives = drives();
        assert!(!drives.is_empty());
        // Empty card readers and CD drives are listed but not readable.
        assert!(drives.iter().any(|d| d.root.is_dir()));
    }

    #[test]
    fn standard_places_exist() {
        for place in standard_places() {
            assert!(place.path.is_absolute(), "{place:?}");
        }
    }

    #[test]
    fn pinned_places_use_the_folder_name() {
        let place = pinned_place(Path::new("/home/someone/code/atlas"));
        assert_eq!(place.label, "atlas");
        assert_eq!(place.icon, PlaceIcon::Folder);
    }
}
