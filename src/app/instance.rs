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

/// Greeting each side sends first, so a port left in the file from an
/// earlier run, now used by some other program, isn't mistaken for us.
const HELLO: &str = "explorist-instance 1";

/// Connects to the running Explorist, if there is one.
fn connect() -> Option<BufReader<TcpStream>> {
    let port: u16 = std::fs::read_to_string(port_file()?)
        .ok()?
        .trim()
        .parse()
        .ok()?;
    let addr = SocketAddr::from((Ipv4Addr::LOCALHOST, port));
    let stream = TcpStream::connect_timeout(&addr, Duration::from_millis(300)).ok()?;
    stream
        .set_read_timeout(Some(Duration::from_millis(500)))
        .ok()?;
    let mut stream = BufReader::new(stream);
    let mut hello = String::new();
    stream.read_line(&mut hello).ok()?;
    (hello.trim() == HELLO).then_some(stream)
}

/// What a later copy asks for.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Ask {
    /// Show the window (Explorist opened again).
    Show,
    /// Open the quick search box (`explorist --search`).
    Search,
}

impl Ask {
    /// The line asking for it. It names the version, so a copy that was
    /// just updated doesn't hand over to the old one.
    fn line(self) -> String {
        let word = match self {
            Self::Show => "show",
            Self::Search => "search",
        };
        format!("{word} {CURRENT}")
    }
}

/// Asks a running Explorist for `ask`. True when it agreed, so this one
/// can quit.
pub fn hand_off(ask: Ask) -> bool {
    let Some(mut stream) = connect() else {
        return false;
    };
    if writeln!(stream.get_mut(), "{}", ask.line()).is_err() {
        return false;
    }
    let mut reply = String::new();
    stream.read_line(&mut reply).is_ok() && reply.trim() == "ok"
}

#[derive(Default)]
struct Flags {
    /// Agree to show the window (Close to tray is on).
    accept_show: AtomicBool,
    /// Agree to open the search box (quick search is on).
    accept_search: AtomicBool,
    show: AtomicBool,
    search: AtomicBool,
}

/// Listens for later copies asking for the window or the search box.
pub struct Listener {
    flags: Arc<Flags>,
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

        let flags = Arc::new(Flags::default());
        let shared = Arc::clone(&flags);
        std::thread::Builder::new()
            .name("explorist-instance".into())
            .spawn(move || {
                for stream in listener.incoming().flatten() {
                    let _ = stream.set_read_timeout(Some(Duration::from_secs(2)));
                    if writeln!(&stream, "{HELLO}").is_err() {
                        continue;
                    }
                    let mut line = String::new();
                    if BufReader::new(&stream).read_line(&mut line).is_err() {
                        continue;
                    }
                    let line = line.trim();
                    let asked = if line == Ask::Show.line() {
                        Some((&shared.accept_show, &shared.show))
                    } else if line == Ask::Search.line() {
                        Some((&shared.accept_search, &shared.search))
                    } else {
                        None
                    };
                    let yes = asked.is_some_and(|(accept, flag)| {
                        let yes = accept.load(Ordering::Relaxed);
                        if yes {
                            flag.store(true, Ordering::Relaxed);
                            ctx.request_repaint();
                        }
                        yes
                    });
                    let _ = writeln!(&stream, "{}", if yes { "ok" } else { "no" });
                }
            })
            .ok()?;
        Some(Self { flags, port })
    }

    /// What this copy agrees to do for later ones.
    pub fn set_accepting(&self, show: bool, search: bool) {
        self.flags.accept_show.store(show, Ordering::Relaxed);
        self.flags.accept_search.store(search, Ordering::Relaxed);
    }

    /// Whether another copy asked for the window since the last call.
    pub fn take_show(&self) -> bool {
        self.flags.show.swap(false, Ordering::Relaxed)
    }

    /// Whether another copy asked for the search box since the last call.
    pub fn take_search(&self) -> bool {
        self.flags.search.swap(false, Ordering::Relaxed)
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
