//! Drive actions from the sidebar: eject, rename, and what file system a
//! drive uses. Each platform does these its own way:
//!
//! - Windows: the shell's Eject verb, `SetVolumeLabelW`.
//! - macOS: `diskutil eject` and `diskutil rename`.
//! - Linux: `gio mount --eject`, else `udisksctl` or `umount`. Renaming
//!   depends on the file system's own tools, so it isn't offered.

use std::io;
use std::path::Path;
use std::process::Command;

use super::places::{Drive, DriveKind};

/// Whether the drive can be ejected: removable and optical drives, and on
/// macOS and Linux anything mounted besides the system disk.
pub fn can_eject(drive: &Drive) -> bool {
    if cfg!(windows) {
        matches!(drive.kind, DriveKind::Removable | DriveKind::Optical)
    } else {
        drive.root != Path::new("/")
    }
}

/// Whether the drive's name can be changed from here.
pub fn can_rename(drive: &Drive) -> bool {
    if cfg!(windows) {
        matches!(drive.kind, DriveKind::Fixed | DriveKind::Removable)
    } else {
        cfg!(target_os = "macos")
    }
}

/// Runs a command, turning a failure into an error with its message.
#[cfg_attr(windows, allow(dead_code))]
fn run(program: &str, args: &[&std::ffi::OsStr]) -> io::Result<()> {
    let output = Command::new(program).args(args).output()?;
    if output.status.success() {
        return Ok(());
    }
    let message = String::from_utf8_lossy(&output.stderr).trim().to_owned();
    Err(io::Error::other(if message.is_empty() {
        format!("{program} failed")
    } else {
        message
    }))
}

#[cfg(windows)]
pub fn eject(root: &Path) -> io::Result<()> {
    use std::os::windows::process::CommandExt;
    const CREATE_NO_WINDOW: u32 = 0x0800_0000;
    // The same "Eject" Explorer offers; it handles USB and optical drives.
    let letter = root
        .to_string_lossy()
        .trim_end_matches(['\\', '/'])
        .to_owned();
    if !(letter.len() == 2 && letter.ends_with(':')) {
        return Err(io::Error::other("not a drive letter"));
    }
    let script = format!(
        "(New-Object -ComObject Shell.Application).Namespace(17).ParseName('{letter}').InvokeVerb('Eject')"
    );
    let status = Command::new("powershell")
        .args(["-NoProfile", "-NonInteractive", "-Command", &script])
        .creation_flags(CREATE_NO_WINDOW)
        .status()?;
    if status.success() {
        Ok(())
    } else {
        Err(io::Error::other("Windows couldn’t eject the drive"))
    }
}

#[cfg(target_os = "macos")]
pub fn eject(root: &Path) -> io::Result<()> {
    run("diskutil", &["eject".as_ref(), root.as_os_str()])
}

#[cfg(not(any(windows, target_os = "macos")))]
pub fn eject(root: &Path) -> io::Result<()> {
    // gio unmounts and powers the device off, like the desktop's own
    // eject button; plain umount is the fallback without it.
    match run(
        "gio",
        &["mount".as_ref(), "--eject".as_ref(), root.as_os_str()],
    ) {
        Ok(()) => Ok(()),
        Err(gio) => match mount_device(root) {
            Some(device) => run(
                "udisksctl",
                &["unmount".as_ref(), "-b".as_ref(), device.as_ref()],
            )
            .or_else(|_| run("umount", &[root.as_os_str()]))
            .map_err(|_| gio),
            None => run("umount", &[root.as_os_str()]).map_err(|_| gio),
        },
    }
}

#[cfg(windows)]
pub fn rename(root: &Path, name: &str) -> io::Result<()> {
    use windows_sys::Win32::Storage::FileSystem::SetVolumeLabelW;
    let root: Vec<u16> = root
        .to_string_lossy()
        .encode_utf16()
        .chain(Some(0))
        .collect();
    let label: Vec<u16> = name.encode_utf16().chain(Some(0)).collect();
    // SAFETY: both buffers are NUL-terminated UTF-16 and outlive the call.
    if unsafe { SetVolumeLabelW(root.as_ptr(), label.as_ptr()) } == 0 {
        return Err(io::Error::last_os_error());
    }
    Ok(())
}

#[cfg(target_os = "macos")]
pub fn rename(root: &Path, name: &str) -> io::Result<()> {
    run(
        "diskutil",
        &["rename".as_ref(), root.as_os_str(), name.as_ref()],
    )
}

#[cfg(not(any(windows, target_os = "macos")))]
pub fn rename(_root: &Path, _name: &str) -> io::Result<()> {
    Err(io::Error::other("renaming drives isn’t supported here"))
}

/// The file system's name, like "NTFS", "apfs" or "ext4".
#[cfg(windows)]
pub fn file_system(root: &Path) -> Option<String> {
    use windows_sys::Win32::Storage::FileSystem::GetVolumeInformationW;
    let wide: Vec<u16> = root
        .to_string_lossy()
        .encode_utf16()
        .chain(Some(0))
        .collect();
    let mut name = [0u16; 64];
    // SAFETY: `wide` is NUL-terminated, `name` is a buffer of the size
    // passed, and the other out-pointers are null, which is allowed.
    let ok = unsafe {
        GetVolumeInformationW(
            wide.as_ptr(),
            std::ptr::null_mut(),
            0,
            std::ptr::null_mut(),
            std::ptr::null_mut(),
            std::ptr::null_mut(),
            name.as_mut_ptr(),
            name.len() as u32,
        )
    } != 0;
    let len = name.iter().position(|&c| c == 0).unwrap_or(name.len());
    (ok && len > 0).then(|| String::from_utf16_lossy(&name[..len]))
}

#[cfg(target_os = "macos")]
pub fn file_system(root: &Path) -> Option<String> {
    use std::os::unix::ffi::OsStrExt;
    let path = std::ffi::CString::new(root.as_os_str().as_bytes()).ok()?;
    let mut stat: libc::statfs = unsafe { std::mem::zeroed() };
    // SAFETY: `path` is NUL-terminated and `stat` is a valid out-pointer.
    if unsafe { libc::statfs(path.as_ptr(), &mut stat) } != 0 {
        return None;
    }
    // SAFETY: the kernel fills f_fstypename with a NUL-terminated name.
    let name = unsafe { std::ffi::CStr::from_ptr(stat.f_fstypename.as_ptr()) };
    Some(name.to_string_lossy().into_owned())
}

#[cfg(not(any(windows, target_os = "macos")))]
pub fn file_system(root: &Path) -> Option<String> {
    let mounts = std::fs::read_to_string("/proc/self/mounts").ok()?;
    mount_entry(&mounts, root).map(|(_, fs)| fs)
}

#[cfg(not(any(windows, target_os = "macos")))]
fn mount_device(root: &Path) -> Option<String> {
    let mounts = std::fs::read_to_string("/proc/self/mounts").ok()?;
    mount_entry(&mounts, root).map(|(device, _)| device)
}

/// The device and file system mounted at `root`, from a mounts table. The
/// last match wins, as later mounts hide earlier ones.
#[cfg_attr(any(windows, target_os = "macos"), allow(dead_code))]
fn mount_entry(mounts: &str, root: &Path) -> Option<(String, String)> {
    let root = root.to_string_lossy();
    mounts.lines().rev().find_map(|line| {
        let mut fields = line.split_whitespace();
        let device = fields.next()?;
        // Spaces in mount points are written as \040.
        let point = fields.next()?.replace("\\040", " ");
        let fs = fields.next()?;
        (point == root).then(|| (device.to_owned(), fs.to_owned()))
    })
}

#[cfg(test)]
mod tests {
    use std::path::PathBuf;

    use super::*;

    fn drive(root: &str, kind: DriveKind) -> Drive {
        Drive {
            root: PathBuf::from(root),
            volume_label: String::new(),
            kind,
            free_bytes: None,
            total_bytes: None,
        }
    }

    #[test]
    fn mounts_table_is_read() {
        let mounts = "/dev/sda1 / ext4 rw 0 0\n\
                      tmpfs /run tmpfs rw 0 0\n\
                      /dev/sdb1 /media/rick/MY\\040STICK vfat rw 0 0\n\
                      /dev/sdc1 /mnt ext4 rw 0 0\n\
                      /dev/sdd1 /mnt xfs rw 0 0\n";
        assert_eq!(
            mount_entry(mounts, Path::new("/")),
            Some(("/dev/sda1".into(), "ext4".into()))
        );
        assert_eq!(
            mount_entry(mounts, Path::new("/media/rick/MY STICK")),
            Some(("/dev/sdb1".into(), "vfat".into()))
        );
        assert_eq!(
            mount_entry(mounts, Path::new("/mnt")),
            Some(("/dev/sdd1".into(), "xfs".into()))
        );
        assert_eq!(mount_entry(mounts, Path::new("/nope")), None);
    }

    #[test]
    fn only_some_drives_eject() {
        if cfg!(windows) {
            assert!(can_eject(&drive("E:\\", DriveKind::Removable)));
            assert!(can_eject(&drive("D:\\", DriveKind::Optical)));
            assert!(!can_eject(&drive("C:\\", DriveKind::Fixed)));
        } else {
            assert!(!can_eject(&drive("/", DriveKind::Fixed)));
            assert!(can_eject(&drive("/media/rick/STICK", DriveKind::Removable)));
        }
    }

    #[test]
    fn the_system_disk_has_a_file_system() {
        let root = if cfg!(windows) { "C:\\" } else { "/" };
        assert!(file_system(Path::new(root)).is_some_and(|f| !f.is_empty()));
    }
}
