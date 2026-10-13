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

#[cfg(not(windows))]
pub fn open_terminal(dir: &Path) -> io::Result<()> {
    Command::new("x-terminal-emulator")
        .current_dir(dir)
        .spawn()
        .map(drop)
}
