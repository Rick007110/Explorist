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
    TogglePreview,
    /// Size, dates and attributes of the selection or the folder.
    Properties,
    /// Switch between the list and the grid of tiles.
    ToggleGrid,
    CommandPalette,
    BatchRename,
    NewTab,
    CloseTab,
    NextTab,
    PreviousTab,
    /// Open the selected folders in new tabs.
    OpenInNewTab,
    /// The palette, listing recent and favorite places.
    RecentPlaces,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MoveTarget {
    Up,
    Down,
    PageUp,
    PageDown,
    First,
    Last,
    /// Sideways, in the grid.
    Left,
    Right,
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
            Self::TogglePreview => "Toggle preview panel",
            Self::ToggleGrid => "Toggle grid view",
            Self::Properties => "Properties",
            Self::CommandPalette => "Command palette",
            Self::BatchRename => "Batch rename…",
            Self::NewTab => "New tab",
            Self::CloseTab => "Close tab",
            Self::NextTab => "Next tab",
            Self::PreviousTab => "Previous tab",
            Self::OpenInNewTab => "Open in new tab",
            Self::RecentPlaces => "Browse recent locations",
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
            Self::TogglePreview => "toggle_preview",
            Self::ToggleGrid => "toggle_grid",
            Self::Properties => "properties",
            Self::CommandPalette => "command_palette",
            Self::BatchRename => "batch_rename",
            Self::NewTab => "new_tab",
            Self::CloseTab => "close_tab",
            Self::NextTab => "next_tab",
            Self::PreviousTab => "previous_tab",
            Self::RecentPlaces => "recent_places",
            // Clipboard shortcuts are handled by the system layer, and
            // navigation keys are fixed.
            _ => return None,
        })
    }

    /// Commands listed on the Keyboard settings page, in order.
    pub const CUSTOMIZABLE: [Command; 26] = [
        Command::CommandPalette,
        Command::RecentPlaces,
        Command::EditPath,
        Command::Filter,
        Command::Rename,
        Command::BatchRename,
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
        Command::Properties,
        Command::TogglePreview,
        Command::ToggleGrid,
        Command::NewTab,
        Command::CloseTab,
        Command::NextTab,
        Command::PreviousTab,
        Command::OpenSettings,
    ];

    /// Commands offered in the palette, in order of usefulness.
    pub const PALETTE: [Command; 30] = [
        Command::Open,
        Command::Rename,
        Command::BatchRename,
        Command::NewFile,
        Command::NewFolder,
        Command::Copy,
        Command::Cut,
        Command::Paste,
        Command::CopyPath,
        Command::Properties,
        Command::Trash,
        Command::DeletePermanently,
        Command::Undo,
        Command::SelectAll,
        Command::Filter,
        Command::EditPath,
        Command::ToggleHidden,
        Command::TogglePreview,
        Command::ToggleGrid,
        Command::Refresh,
        Command::GoBack,
        Command::GoForward,
        Command::GoUp,
        Command::OpenTerminal,
        Command::PinFolder,
        Command::RecentPlaces,
        Command::NewTab,
        Command::CloseTab,
        Command::OpenInNewTab,
        Command::OpenSettings,
    ];

    /// Whether the command works on the selection, so the palette hides
    /// it when nothing is selected.
    pub fn needs_selection(self) -> bool {
        matches!(
            self,
            Self::Open
                | Self::Rename
                | Self::Copy
                | Self::Cut
                | Self::Trash
                | Self::DeletePermanently
                | Self::PinFolder
                | Self::OpenInNewTab
        )
    }

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
                | Self::TogglePreview
                | Self::ToggleGrid
                | Self::CommandPalette
                | Self::RecentPlaces
                | Self::NewTab
                | Self::CloseTab
                | Self::NextTab
                | Self::PreviousTab
                | Self::JumpToPlace(_)
        )
    }
}
