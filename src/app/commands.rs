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
        }
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
