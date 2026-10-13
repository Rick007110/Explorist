//! Keyboard shortcuts.
//!
//! Modifiers must match exactly, so Shift Del never triggers Del.

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

/// The command for a key press, if any.
pub fn lookup(mods: Mods, key: Key) -> Option<Command> {
    if let Some(&(_, _, command)) = TABLE.iter().find(|(m, k, _)| *m == mods && *k == key) {
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

/// The shortcut shown next to a command in menus, as the design writes it:
/// "Ctrl Shift C". The first listed binding wins.
pub fn shortcut_label(command: Command) -> Option<String> {
    if let Command::JumpToPlace(n) = command {
        return Some(format!("Alt {}", n + 1));
    }
    let (mods, key, _) = TABLE.iter().find(|(_, _, c)| *c == command)?;
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
    let key_name = match key {
        Key::Delete => "Del",
        Key::Escape => "Esc",
        Key::Backtick => "`",
        Key::ArrowLeft => "←",
        Key::ArrowRight => "→",
        Key::ArrowUp => "↑",
        Key::ArrowDown => "↓",
        other => other.name(),
    };
    parts.push(key_name);
    Some(parts.join(" "))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn modifiers_must_match_exactly() {
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
