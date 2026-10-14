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
