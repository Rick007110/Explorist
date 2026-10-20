//! One Explorist in the tray: opening Explorist again while it's there
//! shows that window instead of starting a second copy.
//!
//! The running copy listens on a local port and writes it to a file next
//! to the settings. A new copy started without a folder to open asks it to
//! show itself, and quits if it agrees. Copies started with a folder (Open
//! in new window) always get their own window.

use std::io::{BufRead, BufReader, Write};
use std::net::{Ipv4Addr, SocketAddr, TcpListener, TcpStream};
use std::path::PathBuf;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Duration;

use crate::config::Settings;
use crate::update::CURRENT;

fn port_file() -> Option<PathBuf> {
    Some(Settings::default_path()?.parent()?.join("instance"))
}

fn connect() -> Option<TcpStream> {
    let port: u16 = std::fs::read_to_string(port_file()?)
        .ok()?
        .trim()
        .parse()
        .ok()?;
    let addr = SocketAddr::from((Ipv4Addr::LOCALHOST, port));
    let stream = TcpStream::connect_timeout(&addr, Duration::from_millis(300)).ok()?;
    stream.set_read_timeout(Some(Duration::from_secs(2))).ok()?;
    Some(stream)
}

/// The line asking for the window. It names the version, so a copy that
/// was just updated doesn't hand over to the old one.
fn request() -> String {
    format!("show {CURRENT}")
}

/// Asks a running Explorist to show its window. True when it did, so
/// this one can quit.
pub fn hand_off() -> bool {
    let Some(mut stream) = connect() else {
        return false;
    };
    if writeln!(stream, "{}", request()).is_err() {
        return false;
    }
    let mut reply = String::new();
    BufReader::new(stream).read_line(&mut reply).is_ok() && reply.trim() == "ok"
}

/// Listens for later copies asking for the window.
pub struct Listener {
    /// Agree to show the window (Close to tray is on).
    accept: Arc<AtomicBool>,
    show: Arc<AtomicBool>,
    port: u16,
}

impl Listener {
    /// Starts listening, unless another Explorist already does.
    pub fn start(ctx: egui::Context) -> Option<Self> {
        if connect().is_some() {
            return None;
        }
        let file = port_file()?;
        let listener = TcpListener::bind((Ipv4Addr::LOCALHOST, 0)).ok()?;
        let port = listener.local_addr().ok()?.port();
        std::fs::create_dir_all(file.parent()?).ok()?;
        std::fs::write(&file, port.to_string()).ok()?;

        let accept = Arc::new(AtomicBool::new(false));
        let show = Arc::new(AtomicBool::new(false));
        let (accepting, showing) = (Arc::clone(&accept), Arc::clone(&show));
        std::thread::Builder::new()
            .name("explorist-instance".into())
            .spawn(move || {
                for stream in listener.incoming().flatten() {
                    let _ = stream.set_read_timeout(Some(Duration::from_secs(2)));
                    let mut line = String::new();
                    if BufReader::new(&stream).read_line(&mut line).is_err() {
                        continue;
                    }
                    let yes = line.trim() == request() && accepting.load(Ordering::Relaxed);
                    if yes {
                        showing.store(true, Ordering::Relaxed);
                        ctx.request_repaint();
                    }
                    let _ = writeln!(&stream, "{}", if yes { "ok" } else { "no" });
                }
            })
            .ok()?;
        Some(Self { accept, show, port })
    }

    pub fn set_accepting(&self, on: bool) {
        self.accept.store(on, Ordering::Relaxed);
    }

    /// Whether another copy asked for the window since the last call.
    pub fn take_show(&self) -> bool {
        self.show.swap(false, Ordering::Relaxed)
    }
}

impl Drop for Listener {
    fn drop(&mut self) {
        // Leave the file alone if another copy has taken over.
        if let Some(file) = port_file()
            && std::fs::read_to_string(&file).is_ok_and(|t| t.trim() == self.port.to_string())
        {
            let _ = std::fs::remove_file(file);
        }
    }
}
