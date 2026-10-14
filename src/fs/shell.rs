//! Handing things to the rest of the system: default apps and terminals.

use std::io;
use std::path::Path;
use std::process::Command;

/// Opens a file with its default app, without waiting for it.
pub fn open_default(path: &Path) -> io::Result<()> {
    open::that_detached(path)
}

/// Opens a terminal in `dir`: Windows Terminal if installed, else
/// PowerShell in a new console window.
#[cfg(windows)]
pub fn open_terminal(dir: &Path) -> io::Result<()> {
    use std::os::windows::process::CommandExt;
    const CREATE_NEW_CONSOLE: u32 = 0x0000_0010;

    if Command::new("wt.exe").arg("-d").arg(dir).spawn().is_ok() {
        return Ok(());
    }
    Command::new("powershell.exe")
        .arg("-NoExit")
        .current_dir(dir)
        .creation_flags(CREATE_NEW_CONSOLE)
        .spawn()
        .map(drop)
}

/// Opens Terminal.app in `dir`.
#[cfg(target_os = "macos")]
pub fn open_terminal(dir: &Path) -> io::Result<()> {
    Command::new("open")
        .args(["-a", "Terminal"])
        .arg(dir)
        .spawn()
        .map(drop)
}

/// Opens the desktop's terminal in `dir`, trying the common ones in turn.
#[cfg(all(unix, not(target_os = "macos")))]
pub fn open_terminal(dir: &Path) -> io::Result<()> {
    let candidates: [(&str, &[&str]); 5] = [
        ("x-terminal-emulator", &[]),
        ("gnome-terminal", &[]),
        ("konsole", &[]),
        ("xfce4-terminal", &[]),
        ("xterm", &[]),
    ];
    let mut last = io::Error::from(io::ErrorKind::NotFound);
    for (program, args) in candidates {
        match Command::new(program).args(args).current_dir(dir).spawn() {
            Ok(_) => return Ok(()),
            Err(err) => last = err,
        }
    }
    Err(last)
}

/// Whether the system wants animations: Windows' "Animation effects"
/// setting. Always true elsewhere.
/// Whether this platform can reopen a folder with administrator rights.
pub const CAN_ELEVATE: bool = cfg!(windows);

/// Starts another Explorist with administrator rights (after the UAC
/// prompt), showing `dir`. Fails if the prompt is declined.
#[cfg(windows)]
pub fn open_as_admin(dir: &Path) -> io::Result<()> {
    use windows_sys::Win32::UI::Shell::ShellExecuteW;
    use windows_sys::Win32::UI::WindowsAndMessaging::SW_SHOWNORMAL;
    let wide = |s: &std::ffi::OsStr| -> Vec<u16> {
        use std::os::windows::ffi::OsStrExt;
        s.encode_wide().chain(Some(0)).collect()
    };
    let exe = std::env::current_exe()?;
    let args = quote_arg(&dir.display().to_string());
    let (verb, exe, args) = (
        wide("runas".as_ref()),
        wide(exe.as_os_str()),
        wide(args.as_ref()),
    );
    // SAFETY: every pointer is to a NUL-terminated UTF-16 buffer that
    // outlives the call.
    let result = unsafe {
        ShellExecuteW(
            std::ptr::null_mut(),
            verb.as_ptr(),
            exe.as_ptr(),
            args.as_ptr(),
            std::ptr::null(),
            SW_SHOWNORMAL,
        )
    };
    // Values above 32 mean it started.
    if result as isize > 32 {
        Ok(())
    } else {
        Err(io::Error::last_os_error())
    }
}

/// Quotes one argument for a Windows command line. Backslashes before the
/// closing quote are doubled so `C:\` doesn't escape it.
#[cfg_attr(not(windows), allow(dead_code))]
fn quote_arg(arg: &str) -> String {
    let trailing = arg.len() - arg.trim_end_matches('\\').len();
    format!("\"{}{}\"", arg, "\\".repeat(trailing))
}

#[cfg(windows)]
pub fn animations_enabled() -> bool {
    use windows_sys::Win32::UI::WindowsAndMessaging::{
        SPI_GETCLIENTAREAANIMATION, SystemParametersInfoW,
    };
    let mut enabled: i32 = 1;
    // SAFETY: SPI_GETCLIENTAREAANIMATION writes one BOOL to the pointer.
    let ok = unsafe {
        SystemParametersInfoW(
            SPI_GETCLIENTAREAANIMATION,
            0,
            (&mut enabled as *mut i32).cast(),
            0,
        )
    };
    ok == 0 || enabled != 0
}

#[cfg(not(windows))]
pub fn animations_enabled() -> bool {
    true
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn arguments_are_quoted_for_windows() {
        assert_eq!(quote_arg(r"D:\Program Files"), r#""D:\Program Files""#);
        assert_eq!(quote_arg(r"C:\"), r#""C:\\""#);
        assert_eq!(quote_arg(r"\\server\share\"), r#""\\server\share\\""#);
    }
}
