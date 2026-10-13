//! Keyboard shortcuts.
//!
//! Modifiers must match exactly, so Shift Del never triggers Del. Users can
//! rebind commands in Settings; the active keymap is installed globally so
//! menus and tooltips show the current keys.

use std::collections::BTreeMap;
use std::sync::RwLock;

use egui::Key;

use crate::app::commands::{Command, MoveTarget};
use crate::app::selection::MoveMode;

/// The modifier keys we care about.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct Mods {
    pub ctrl: bool,
    pub shift: bool,
    pub alt: bool,
}

impl Mods {
    pub const NONE: Self = Self {
        ctrl: false,
        shift: false,
        alt: false,
    };
    pub const CTRL: Self = Self {
        ctrl: true,
        shift: false,
        alt: false,
    };
    pub const SHIFT: Self = Self {
        ctrl: false,
        shift: true,
        alt: false,
    };
    pub const ALT: Self = Self {
        ctrl: false,
        shift: false,
        alt: true,
    };
    pub const CTRL_SHIFT: Self = Self {
        ctrl: true,
        shift: true,
        alt: false,
    };

    pub fn from_egui(m: egui::Modifiers) -> Self {
        Self {
            // `command` is Ctrl on Windows and Linux.
            ctrl: m.ctrl || m.command,
            shift: m.shift,
            alt: m.alt,
        }
    }
}

const FAVORITE_KEYS: [Key; 9] = [
    Key::Num1,
    Key::Num2,
    Key::Num3,
    Key::Num4,
    Key::Num5,
    Key::Num6,
    Key::Num7,
    Key::Num8,
    Key::Num9,
];

const TABLE: &[(Mods, Key, Command)] = &[
    (Mods::NONE, Key::Enter, Command::Open),
    (Mods::ALT, Key::ArrowLeft, Command::GoBack),
    (Mods::NONE, Key::Backspace, Command::GoBack),
    (Mods::ALT, Key::ArrowRight, Command::GoForward),
    (Mods::ALT, Key::ArrowUp, Command::GoUp),
    (Mods::NONE, Key::F5, Command::Refresh),
    (Mods::CTRL, Key::R, Command::Refresh),
    (Mods::CTRL, Key::L, Command::EditPath),
    (Mods::ALT, Key::D, Command::EditPath),
    (Mods::CTRL, Key::F, Command::Filter),
    (Mods::NONE, Key::F2, Command::Rename),
    (Mods::CTRL_SHIFT, Key::N, Command::NewFolder),
    (Mods::CTRL, Key::N, Command::NewFile),
    (Mods::NONE, Key::Delete, Command::Trash),
    (Mods::SHIFT, Key::Delete, Command::DeletePermanently),
    (Mods::CTRL_SHIFT, Key::C, Command::CopyPath),
    (Mods::CTRL, Key::X, Command::Cut),
    (Mods::CTRL, Key::C, Command::Copy),
    (Mods::CTRL, Key::V, Command::Paste),
    (Mods::CTRL, Key::A, Command::SelectAll),
    (Mods::CTRL, Key::H, Command::ToggleHidden),
    (Mods::CTRL, Key::Z, Command::Undo),
    (Mods::CTRL, Key::Backtick, Command::OpenTerminal),
    (Mods::NONE, Key::Escape, Command::Escape),
    (Mods::CTRL, Key::Space, Command::ToggleFocused),
    (Mods::CTRL, Key::Comma, Command::OpenSettings),
    (Mods::CTRL, Key::Period, Command::TogglePreview),
    (Mods::CTRL, Key::P, Command::CommandPalette),
    (Mods::CTRL, Key::E, Command::RecentPlaces),
];

/// Arrow-style keys, each in three flavors: plain moves the selection,
/// Shift extends it, Ctrl moves focus only.
const MOVES: [(Key, MoveTarget); 6] = [
    (Key::ArrowUp, MoveTarget::Up),
    (Key::ArrowDown, MoveTarget::Down),
    (Key::PageUp, MoveTarget::PageUp),
    (Key::PageDown, MoveTarget::PageDown),
    (Key::Home, MoveTarget::First),
    (Key::End, MoveTarget::Last),
];

/// Bindings in effect: the defaults with the user's overrides applied.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Keymap {
    bindings: Vec<(Mods, Key, Command)>,
}

impl Default for Keymap {
    fn default() -> Self {
        Self {
            bindings: TABLE.to_vec(),
        }
    }
}

impl Keymap {
    /// The defaults with `overrides` (command id → "Ctrl Shift N") applied.
    /// An overridden command loses its default keys; a key taken by an
    /// override is removed from whatever else used it. Unreadable entries
    /// are ignored.
    pub fn new(overrides: &BTreeMap<String, String>) -> Self {
        let mut map = Self::default();
        for (id, keys) in overrides {
            if let (Some(command), Some((mods, key))) = (Command::from_id(id), parse_shortcut(keys))
            {
                map.bind(command, mods, key);
            }
        }
        map
    }

    /// Makes `mods`+`key` run `command`, replacing its old keys and taking
    /// the keys away from anything else. Returns the command that lost them.
    pub fn bind(&mut self, command: Command, mods: Mods, key: Key) -> Option<Command> {
        let previous = self.conflict(mods, key).filter(|c| *c != command);
        self.bindings
            .retain(|(m, k, c)| *c != command && !(*m == mods && *k == key));
        self.bindings.push((mods, key, command));
        previous
    }

    /// Which command `mods`+`key` currently runs.
    pub fn conflict(&self, mods: Mods, key: Key) -> Option<Command> {
        self.bindings
            .iter()
            .find(|(m, k, _)| *m == mods && *k == key)
            .map(|(_, _, c)| *c)
    }

    /// The command for a key press, if any.
    pub fn lookup(&self, mods: Mods, key: Key) -> Option<Command> {
        if let Some(command) = self.conflict(mods, key) {
            return Some(command);
        }
        if mods == Mods::ALT
            && let Some(n) = FAVORITE_KEYS.iter().position(|k| *k == key)
        {
            return Some(Command::JumpToPlace(n as u8));
        }
        let (_, target) = MOVES.iter().find(|(k, _)| *k == key)?;
        let mode = match mods {
            Mods::NONE => MoveMode::Replace,
            Mods::SHIFT => MoveMode::Extend,
            Mods::CTRL => MoveMode::FocusOnly,
            _ => return None,
        };
        Some(Command::Move(*target, mode))
    }

    /// The first binding for `command`.
    pub fn binding(&self, command: Command) -> Option<(Mods, Key)> {
        self.bindings
            .iter()
            .find(|(_, _, c)| *c == command)
            .map(|(m, k, _)| (*m, *k))
    }

    /// The shortcut shown next to a command, as the design writes it:
    /// "Ctrl Shift C".
    pub fn label(&self, command: Command) -> Option<String> {
        if let Command::JumpToPlace(n) = command {
            return Some(format!("Alt {}", n + 1));
        }
        let (mods, key) = self.binding(command)?;
        Some(format_shortcut(mods, key))
    }

    /// The bindings that differ from the defaults, for the settings file.
    pub fn overrides(&self) -> BTreeMap<String, String> {
        let defaults = Self::default();
        Command::CUSTOMIZABLE
            .into_iter()
            .filter(|c| self.binding(*c) != defaults.binding(*c))
            .filter_map(|c| Some((c.id()?.to_owned(), self.label(c)?)))
            .collect()
    }
}

static ACTIVE: RwLock<Option<Keymap>> = RwLock::new(None);

/// Makes `keymap` the one used for lookups and labels.
pub fn install(keymap: Keymap) {
    if let Ok(mut active) = ACTIVE.write() {
        *active = Some(keymap);
    }
}

fn with_active<R>(f: impl FnOnce(&Keymap) -> R) -> R {
    match ACTIVE.read() {
        Ok(guard) => match guard.as_ref() {
            Some(map) => f(map),
            None => f(&Keymap::default()),
        },
        Err(_) => f(&Keymap::default()),
    }
}

/// The command for a key press under the installed keymap.
pub fn lookup(mods: Mods, key: Key) -> Option<Command> {
    with_active(|map| map.lookup(mods, key))
}

/// The installed shortcut for `command`, e.g. "Ctrl Shift N".
pub fn shortcut_label(command: Command) -> Option<String> {
    with_active(|map| map.label(command))
}

/// Short display names for keys whose egui name is long or unclear.
const KEY_NAMES: [(Key, &str); 9] = [
    (Key::Comma, ","),
    (Key::Period, "."),
    (Key::Delete, "Del"),
    (Key::Escape, "Esc"),
    (Key::Backtick, "`"),
    (Key::ArrowLeft, "←"),
    (Key::ArrowRight, "→"),
    (Key::ArrowUp, "↑"),
    (Key::ArrowDown, "↓"),
];

/// Shift, Ctrl, Alt and the Windows key on their own: never a shortcut.
pub fn is_modifier(key: Key) -> bool {
    matches!(
        key,
        Key::ShiftLeft
            | Key::ShiftRight
            | Key::ControlLeft
            | Key::ControlRight
            | Key::AltLeft
            | Key::AltRight
            | Key::SuperLeft
            | Key::SuperRight
    )
}

/// "Ctrl Shift N".
pub fn format_shortcut(mods: Mods, key: Key) -> String {
    let mut parts = Vec::new();
    if mods.ctrl {
        parts.push("Ctrl");
    }
    if mods.shift {
        parts.push("Shift");
    }
    if mods.alt {
        parts.push("Alt");
    }
    let name = KEY_NAMES
        .iter()
        .find(|(k, _)| *k == key)
        .map_or_else(|| key.name(), |(_, n)| n);
    parts.push(name);
    parts.join(" ")
}

/// Reads "Ctrl Shift N" back. Case doesn't matter for modifiers.
pub fn parse_shortcut(text: &str) -> Option<(Mods, Key)> {
    let mut mods = Mods::NONE;
    let mut key = None;
    for part in text.split_whitespace() {
        match part.to_ascii_lowercase().as_str() {
            "ctrl" | "control" => mods.ctrl = true,
            "shift" => mods.shift = true,
            "alt" => mods.alt = true,
            _ => {
                if key.is_some() {
                    return None;
                }
                key = KEY_NAMES
                    .iter()
                    .find(|(_, n)| *n == part)
                    .map(|(k, _)| *k)
                    .or_else(|| Key::from_name(part));
            }
        }
    }
    Some((mods, key?))
}

/// egui-winit turns some shortcuts into clipboard events before the app
/// sees them: Ctrl C/X/V (with or without Shift) and, on Windows, Shift Del
/// and Ctrl/Shift Insert. This maps them back. `mods` is the modifier state
/// at the time.
///
/// Ctrl V with only files on the clipboard (no text) produces no event at
/// all; see [`PasteDetector`].
pub fn clipboard_event(event: &egui::Event, mods: Mods) -> Option<Command> {
    match event {
        egui::Event::Copy if mods.shift => Some(Command::CopyPath),
        egui::Event::Copy => Some(Command::Copy),
        egui::Event::Cut if mods.shift && !mods.ctrl => Some(Command::DeletePermanently),
        egui::Event::Cut => Some(Command::Cut),
        egui::Event::Paste(_) => Some(Command::Paste),
        _ => None,
    }
}

/// Spots Ctrl V when egui-winit swallowed it. The press of a paste
/// shortcut never reaches the app (it becomes a Paste event, or nothing if
/// the clipboard holds no text), but the release of V does. A V release
/// without a V press before it therefore means "paste". Modifiers can't be
/// trusted at that point: Ctrl is often let go first.
#[derive(Debug, Default)]
pub struct PasteDetector {
    v_down: bool,
}

impl PasteDetector {
    /// Returns true when `event` completes a swallowed paste shortcut.
    pub fn feed(&mut self, event: &egui::Event) -> bool {
        match event {
            egui::Event::Key {
                key: Key::V,
                pressed: true,
                ..
            } => {
                self.v_down = true;
                false
            }
            egui::Event::Key {
                key: Key::V,
                pressed: false,
                ..
            } => !std::mem::take(&mut self.v_down),
            _ => false,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn overrides_rebind_and_steal_keys() {
        let mut overrides = BTreeMap::new();
        overrides.insert("rename".to_owned(), "Ctrl R".to_owned());
        overrides.insert("bogus".to_owned(), "Ctrl Q".to_owned());
        overrides.insert("filter".to_owned(), "not a key".to_owned());
        let map = Keymap::new(&overrides);
        assert_eq!(map.lookup(Mods::CTRL, Key::R), Some(Command::Rename));
        assert_eq!(map.lookup(Mods::NONE, Key::F2), None, "old key is gone");
        // Refresh still has F5, just not Ctrl R.
        assert_eq!(map.lookup(Mods::NONE, Key::F5), Some(Command::Refresh));
        assert_eq!(map.lookup(Mods::CTRL, Key::F), Some(Command::Filter));
        assert_eq!(
            map.overrides().get("rename").map(String::as_str),
            Some("Ctrl R")
        );
        // Refresh keeps its first key (F5), so only rename needs saving;
        // reloading reproduces the same bindings.
        assert_eq!(map.overrides().len(), 1);
        let reloaded = Keymap::new(&map.overrides());
        for command in Command::CUSTOMIZABLE {
            assert_eq!(reloaded.binding(command), map.binding(command));
        }
    }

    #[test]
    fn bind_reports_what_lost_its_keys() {
        let mut map = Keymap::default();
        assert_eq!(
            map.bind(Command::Undo, Mods::CTRL, Key::A),
            Some(Command::SelectAll)
        );
        assert_eq!(map.binding(Command::SelectAll), None);
        assert_eq!(
            map.bind(Command::Undo, Mods::CTRL, Key::A),
            None,
            "already its own"
        );
    }

    #[test]
    fn shortcuts_format_and_parse() {
        for (mods, key) in [
            (Mods::CTRL_SHIFT, Key::N),
            (Mods::SHIFT, Key::Delete),
            (Mods::ALT, Key::ArrowLeft),
            (Mods::CTRL, Key::Backtick),
            (Mods::NONE, Key::F2),
            (Mods::CTRL, Key::Comma),
        ] {
            let text = format_shortcut(mods, key);
            assert_eq!(parse_shortcut(&text), Some((mods, key)), "{text}");
        }
        assert_eq!(
            parse_shortcut("ctrl shift n"),
            Some((Mods::CTRL_SHIFT, Key::N))
        );
        assert_eq!(parse_shortcut("Ctrl"), None);
        assert_eq!(parse_shortcut("Ctrl A B"), None);
    }

    #[test]
    fn every_customizable_command_has_an_id_and_default() {
        let map = Keymap::default();
        for command in Command::CUSTOMIZABLE {
            let id = command.id().expect("id");
            assert_eq!(Command::from_id(id), Some(command));
            assert!(map.binding(command).is_some(), "{id} has no default key");
        }
    }

    #[test]
    fn modifiers_must_match_exactly() {
        let lookup = |m, k| Keymap::default().lookup(m, k);
        assert_eq!(lookup(Mods::NONE, Key::Delete), Some(Command::Trash));
        assert_eq!(
            lookup(Mods::SHIFT, Key::Delete),
            Some(Command::DeletePermanently)
        );
        assert_eq!(lookup(Mods::CTRL, Key::N), Some(Command::NewFile));
        assert_eq!(lookup(Mods::CTRL_SHIFT, Key::N), Some(Command::NewFolder));
        assert_eq!(lookup(Mods::CTRL, Key::Delete), None);
    }

    #[test]
    fn arrows_come_in_three_flavors() {
        let lookup = |m, k| Keymap::default().lookup(m, k);
        assert_eq!(
            lookup(Mods::NONE, Key::ArrowDown),
            Some(Command::Move(MoveTarget::Down, MoveMode::Replace))
        );
        assert_eq!(
            lookup(Mods::SHIFT, Key::End),
            Some(Command::Move(MoveTarget::Last, MoveMode::Extend))
        );
        assert_eq!(
            lookup(Mods::CTRL, Key::ArrowUp),
            Some(Command::Move(MoveTarget::Up, MoveMode::FocusOnly))
        );
        assert_eq!(lookup(Mods::ALT, Key::ArrowUp), Some(Command::GoUp));
        assert_eq!(lookup(Mods::CTRL_SHIFT, Key::ArrowUp), None);
    }

    #[test]
    fn alt_digits_jump_to_places() {
        let lookup = |m, k| Keymap::default().lookup(m, k);
        assert_eq!(lookup(Mods::ALT, Key::Num1), Some(Command::JumpToPlace(0)));
        assert_eq!(lookup(Mods::ALT, Key::Num9), Some(Command::JumpToPlace(8)));
        assert_eq!(lookup(Mods::NONE, Key::Num1), None);
    }

    #[test]
    fn every_binding_is_unique() {
        for (i, (m, k, _)) in TABLE.iter().enumerate() {
            assert!(
                !TABLE[i + 1..].iter().any(|(m2, k2, _)| m2 == m && k2 == k),
                "{m:?} {k:?} is bound twice"
            );
        }
    }

    #[test]
    fn clipboard_events_map_back_to_commands() {
        use egui::Event;
        assert_eq!(
            clipboard_event(&Event::Copy, Mods::CTRL),
            Some(Command::Copy)
        );
        assert_eq!(
            clipboard_event(&Event::Copy, Mods::CTRL_SHIFT),
            Some(Command::CopyPath)
        );
        assert_eq!(clipboard_event(&Event::Cut, Mods::CTRL), Some(Command::Cut));
        assert_eq!(
            clipboard_event(&Event::Cut, Mods::SHIFT),
            Some(Command::DeletePermanently)
        );
        assert_eq!(
            clipboard_event(&Event::Paste("x".into()), Mods::CTRL),
            Some(Command::Paste)
        );
    }

    #[test]
    fn swallowed_ctrl_v_is_spotted_on_release() {
        let v = |pressed| egui::Event::Key {
            key: Key::V,
            physical_key: None,
            pressed,
            repeat: false,
            modifiers: egui::Modifiers::NONE,
        };
        let mut detector = PasteDetector::default();
        // Typing a plain "v": press and release both arrive.
        assert!(!detector.feed(&v(true)));
        assert!(!detector.feed(&v(false)));
        // Ctrl V: the press was swallowed, only the release arrives.
        assert!(detector.feed(&v(false)));
    }

    #[test]
    fn labels_match_the_design() {
        let map = Keymap::default();
        let shortcut_label = |c| map.label(c);
        assert_eq!(
            shortcut_label(Command::CopyPath).as_deref(),
            Some("Ctrl Shift C")
        );
        assert_eq!(shortcut_label(Command::Trash).as_deref(), Some("Del"));
        assert_eq!(
            shortcut_label(Command::DeletePermanently).as_deref(),
            Some("Shift Del")
        );
        assert_eq!(shortcut_label(Command::Rename).as_deref(), Some("F2"));
        assert_eq!(
            shortcut_label(Command::OpenTerminal).as_deref(),
            Some("Ctrl `")
        );
        assert_eq!(shortcut_label(Command::GoBack).as_deref(), Some("Alt ←"));
        assert_eq!(shortcut_label(Command::Refresh).as_deref(), Some("F5"));
        assert_eq!(
            shortcut_label(Command::JumpToPlace(2)).as_deref(),
            Some("Alt 3")
        );
    }
}
