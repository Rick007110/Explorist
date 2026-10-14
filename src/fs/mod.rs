//! File-system logic: reading folders, sorting, filtering and file operations.
//!
//! Nothing in here depends on the UI, so all of it is unit-tested.

pub mod batch;
pub mod clipboard;
pub mod entry;
pub mod filter;
pub mod format;
pub mod git;
pub mod index;
pub mod jobs;
pub mod kind;
pub mod listing;
pub mod ops;
pub mod path_input;
pub mod places;
pub mod preview;
pub mod props;
pub mod shell;
pub mod sort;
pub mod thumbs;
pub mod watch;

/// Helpers shared by tests across modules.
#[cfg(test)]
pub(crate) mod test_support {
    use std::path::Path;

    /// Creates an empty file that counts as hidden on this platform: a
    /// dotfile elsewhere, the hidden attribute on Windows.
    pub fn hidden_file(dir: &Path, name: &str) -> std::path::PathBuf {
        let path = dir.join(name);
        std::fs::write(&path, b"").unwrap();
        #[cfg(windows)]
        {
            use windows_sys::Win32::Storage::FileSystem::{
                FILE_ATTRIBUTE_HIDDEN, SetFileAttributesW,
            };
            let wide: Vec<u16> = path
                .as_os_str()
                .to_string_lossy()
                .encode_utf16()
                .chain(Some(0))
                .collect();
            // SAFETY: `wide` is a NUL-terminated UTF-16 path that outlives the call.
            let ok = unsafe { SetFileAttributesW(wide.as_ptr(), FILE_ATTRIBUTE_HIDDEN) };
            assert_ne!(ok, 0, "couldn't hide {}", path.display());
        }
        path
    }
}
