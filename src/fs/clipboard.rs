//! Cut, copy and paste of files.
//!
//! On Windows this uses the system file clipboard, the same one Explorer
//! uses, so files copied here paste in Explorer and the other way round.
//! Elsewhere it's kept inside the app.

use std::io;
use std::path::PathBuf;

/// Files on the clipboard.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ClipFiles {
    pub paths: Vec<PathBuf>,
    /// Cut (move on paste) rather than copy.
    pub cut: bool,
}

pub use imp::{clear, read};

#[cfg(windows)]
mod imp {
    use std::ffi::OsString;
    use std::os::windows::ffi::{OsStrExt, OsStringExt};
    use std::path::PathBuf;
    use std::{io, mem, ptr, thread, time::Duration};

    use windows_sys::Win32::Foundation::{GlobalFree, HANDLE};
    use windows_sys::Win32::System::DataExchange::{
        CloseClipboard, EmptyClipboard, GetClipboardData, IsClipboardFormatAvailable,
        OpenClipboard, RegisterClipboardFormatW, SetClipboardData,
    };
    use windows_sys::Win32::System::Memory::{
        GMEM_MOVEABLE, GlobalAlloc, GlobalLock, GlobalUnlock,
    };
    use windows_sys::Win32::UI::Shell::{DROPFILES, DragQueryFileW};

    use super::ClipFiles;

    const CF_HDROP: u32 = 15;
    const DROPEFFECT_COPY: u32 = 1;
    const DROPEFFECT_MOVE: u32 = 2;

    /// Holds the clipboard open; closes it when dropped.
    struct Open;

    impl Open {
        fn new() -> io::Result<Self> {
            // Another program may hold it for a moment.
            for _ in 0..10 {
                // SAFETY: no window handle is needed to open the clipboard.
                if unsafe { OpenClipboard(ptr::null_mut()) } != 0 {
                    return Ok(Self);
                }
                thread::sleep(Duration::from_millis(15));
            }
            Err(io::Error::last_os_error())
        }
    }

    impl Drop for Open {
        fn drop(&mut self) {
            // SAFETY: we opened it in `new`.
            unsafe { CloseClipboard() };
        }
    }

    fn drop_effect_format() -> u32 {
        let name: Vec<u16> = "Preferred DropEffect"
            .encode_utf16()
            .chain(Some(0))
            .collect();
        // SAFETY: `name` is NUL-terminated.
        unsafe { RegisterClipboardFormatW(name.as_ptr()) }
    }

    /// Copies `bytes` into a movable global block, as SetClipboardData needs.
    fn global_from(bytes: &[u8]) -> io::Result<HANDLE> {
        // SAFETY: the block is `bytes.len()` long, locked before writing and
        // unlocked after; it's freed on every error path.
        unsafe {
            let handle = GlobalAlloc(GMEM_MOVEABLE, bytes.len());
            if handle.is_null() {
                return Err(io::Error::last_os_error());
            }
            let dst = GlobalLock(handle);
            if dst.is_null() {
                GlobalFree(handle);
                return Err(io::Error::last_os_error());
            }
            ptr::copy_nonoverlapping(bytes.as_ptr(), dst.cast::<u8>(), bytes.len());
            GlobalUnlock(handle);
            Ok(handle)
        }
    }

    fn set(format: u32, bytes: &[u8]) -> io::Result<()> {
        let handle = global_from(bytes)?;
        // SAFETY: the clipboard is open; on success it owns `handle`.
        if unsafe { SetClipboardData(format, handle) }.is_null() {
            let err = io::Error::last_os_error();
            unsafe { GlobalFree(handle) };
            return Err(err);
        }
        Ok(())
    }

    pub fn write(paths: &[PathBuf], cut: bool) -> io::Result<()> {
        // DROPFILES header, then each path as wide chars with a NUL, then
        // one more NUL to end the list.
        let header = DROPFILES {
            pFiles: mem::size_of::<DROPFILES>() as u32,
            pt: windows_sys::Win32::Foundation::POINT { x: 0, y: 0 },
            fNC: 0,
            fWide: 1,
        };
        let mut bytes = Vec::new();
        // SAFETY: DROPFILES is plain old data.
        bytes.extend_from_slice(unsafe {
            std::slice::from_raw_parts(
                (&header as *const DROPFILES).cast::<u8>(),
                mem::size_of::<DROPFILES>(),
            )
        });
        for path in paths {
            for unit in path.as_os_str().encode_wide().chain(Some(0)) {
                bytes.extend_from_slice(&unit.to_le_bytes());
            }
        }
        bytes.extend_from_slice(&0u16.to_le_bytes());

        let effect = if cut {
            DROPEFFECT_MOVE
        } else {
            DROPEFFECT_COPY
        };
        let _open = Open::new()?;
        // SAFETY: the clipboard is open.
        if unsafe { EmptyClipboard() } == 0 {
            return Err(io::Error::last_os_error());
        }
        set(CF_HDROP, &bytes)?;
        set(drop_effect_format(), &effect.to_le_bytes())
    }

    pub fn read() -> Option<ClipFiles> {
        // SAFETY: checking for a format needs no open clipboard.
        if unsafe { IsClipboardFormatAvailable(CF_HDROP) } == 0 {
            return None;
        }
        let _open = Open::new().ok()?;
        // SAFETY: the clipboard is open and owns the returned handles; we
        // only read them while it stays open.
        unsafe {
            let hdrop = GetClipboardData(CF_HDROP);
            if hdrop.is_null() {
                return None;
            }
            let count = DragQueryFileW(hdrop, u32::MAX, ptr::null_mut(), 0);
            let mut paths = Vec::with_capacity(count as usize);
            for i in 0..count {
                let len = DragQueryFileW(hdrop, i, ptr::null_mut(), 0);
                let mut buf = vec![0u16; len as usize + 1];
                let got = DragQueryFileW(hdrop, i, buf.as_mut_ptr(), buf.len() as u32);
                buf.truncate(got as usize);
                paths.push(PathBuf::from(OsString::from_wide(&buf)));
            }

            let mut cut = false;
            let effect = GetClipboardData(drop_effect_format());
            if !effect.is_null() {
                let p = GlobalLock(effect).cast::<u32>();
                if !p.is_null() {
                    cut = p.read_unaligned() & DROPEFFECT_MOVE != 0;
                    GlobalUnlock(effect);
                }
            }
            Some(ClipFiles { paths, cut })
        }
    }

    pub fn clear() {
        if let Ok(_open) = Open::new() {
            // SAFETY: the clipboard is open.
            unsafe { EmptyClipboard() };
        }
    }
}

#[cfg(not(windows))]
mod imp {
    use std::io;
    use std::path::PathBuf;
    use std::sync::Mutex;

    use super::ClipFiles;

    static CLIP: Mutex<Option<ClipFiles>> = Mutex::new(None);

    pub fn write(paths: &[PathBuf], cut: bool) -> io::Result<()> {
        *CLIP
            .lock()
            .map_err(|_| io::Error::other("clipboard poisoned"))? = Some(ClipFiles {
            paths: paths.to_vec(),
            cut,
        });
        Ok(())
    }

    pub fn read() -> Option<ClipFiles> {
        CLIP.lock().ok()?.clone()
    }

    pub fn clear() {
        if let Ok(mut clip) = CLIP.lock() {
            *clip = None;
        }
    }
}

/// Puts files on the clipboard.
pub fn put(paths: &[PathBuf], cut: bool) -> io::Result<()> {
    imp::write(paths, cut)
}

#[cfg(test)]
mod tests {
    use super::*;

    // One test touches the shared clipboard, so it runs alone.
    #[test]
    fn round_trip() {
        let dir = tempfile::tempdir().unwrap();
        let a = dir.path().join("a.txt");
        let b = dir.path().join("été.txt");
        std::fs::write(&a, b"").unwrap();
        std::fs::write(&b, b"").unwrap();

        put(&[a.clone(), b.clone()], true).unwrap();
        let clip = read().expect("files on the clipboard");
        assert_eq!(clip.paths, [a.clone(), b]);
        assert!(clip.cut);

        put(std::slice::from_ref(&a), false).unwrap();
        let clip = read().unwrap();
        assert_eq!(clip.paths, [a]);
        assert!(!clip.cut);

        clear();
        assert_eq!(read(), None);
    }
}
