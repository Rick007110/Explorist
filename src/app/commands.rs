//! Everything the user can do, whichever way they ask: keyboard shortcut,
//! context menu or button.

use crate::app::selection::MoveMode;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Command {
    /// Open the focused folder, or the selected files in their default app.
    Open,
    GoBack,
    GoForward,
    GoUp,
    Refresh,
    /// Swap the breadcrumbs for an editable path field.
    EditPath,
    /// Show and focus the filter bar.
    Filter,
    Rename,
    NewFolder,
    NewFile,
    /// Move the selection to the Recycle Bin.
    Trash,
    /// Delete the selection for good, after confirmation.
    DeletePermanently,
    CopyPath,
    Cut,
    Copy,
    Paste,
    SelectAll,
    ToggleHidden,
    Undo,
    OpenTerminal,
    /// Alt 1–9: the nth place in the sidebar.
    JumpToPlace(u8),
    /// Close whatever is open: menu, filter, rename, or stop loading.
    Escape,
    Move(MoveTarget, MoveMode),
    /// Ctrl Space: toggle the focused row's selection.
    ToggleFocused,
    OpenSettings,
    /// Add the selected folder to Favorites.
    PinFolder,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MoveTarget {
    Up,
    Down,
    PageUp,
    PageDown,
    First,
    Last,
}

impl Command {
    /// Name shown in menus and tooltips.
    pub fn label(self) -> &'static str {
        match self {
            Self::Open => "Open",
            Self::GoBack => "Back",
            Self::GoForward => "Forward",
            Self::GoUp => "Up one level",
            Self::Refresh => "Refresh",
            Self::EditPath => "Edit or search",
            Self::Filter => "Filter by name",
            Self::Rename => "Rename",
            Self::NewFolder => "New folder",
            Self::NewFile => "New file",
            Self::Trash => "Move to Recycle Bin",
            Self::DeletePermanently => "Delete permanently",
            Self::CopyPath => "Copy path",
            Self::Cut => "Cut",
            Self::Copy => "Copy",
            Self::Paste => "Paste",
            Self::SelectAll => "Select all",
            Self::ToggleHidden => "Show hidden files",
            Self::Undo => "Undo",
            Self::OpenTerminal => "Open terminal here",
            Self::JumpToPlace(_) => "Jump to place",
            Self::Escape => "Cancel",
            Self::Move(..) => "Move",
            Self::ToggleFocused => "Toggle selection",
            Self::OpenSettings => "Settings",
            Self::PinFolder => "Pin to Favorites",
        }
    }

    /// Stable name used in the settings file for custom shortcuts. `None`
    /// for commands that can't be rebound.
    pub fn id(self) -> Option<&'static str> {
        Some(match self {
            Self::Open => "open",
            Self::GoBack => "go_back",
            Self::GoForward => "go_forward",
            Self::GoUp => "go_up",
            Self::Refresh => "refresh",
            Self::EditPath => "edit_path",
            Self::Filter => "filter",
            Self::Rename => "rename",
            Self::NewFolder => "new_folder",
            Self::NewFile => "new_file",
            Self::Trash => "trash",
            Self::SelectAll => "select_all",
            Self::ToggleHidden => "toggle_hidden",
            Self::Undo => "undo",
            Self::OpenTerminal => "open_terminal",
            Self::OpenSettings => "open_settings",
            // Clipboard shortcuts are handled by the system layer, and
            // navigation keys are fixed.
            _ => return None,
        })
    }

    /// Commands listed on the Keyboard settings page, in order.
    pub const CUSTOMIZABLE: [Command; 16] = [
        Command::EditPath,
        Command::Filter,
        Command::Rename,
        Command::NewFile,
        Command::NewFolder,
        Command::Trash,
        Command::Undo,
        Command::SelectAll,
        Command::ToggleHidden,
        Command::GoBack,
        Command::GoForward,
        Command::GoUp,
        Command::Refresh,
        Command::Open,
        Command::OpenTerminal,
        Command::OpenSettings,
    ];

    pub fn from_id(id: &str) -> Option<Command> {
        Self::CUSTOMIZABLE.into_iter().find(|c| c.id() == Some(id))
    }

    /// Whether the shortcut still works while a text field has focus.
    /// Everything else would clash with typing (Backspace, Delete, letters).
    pub fn works_while_typing(self) -> bool {
        matches!(
            self,
            Self::GoBack
                | Self::GoForward
                | Self::GoUp
                | Self::Refresh
                | Self::EditPath
                | Self::Filter
                | Self::ToggleHidden
                | Self::JumpToPlace(_)
        )
    }
}
