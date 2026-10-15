# Changelog

What's new in each version of Explorist. The release workflow publishes
each version's section as its release notes, and the in-app update prompt
shows the first points.

## 0.3.1

### Changed
- The refresh button sits next to back, forward and up.

### Fixed
- Backspace in a search or text field went back to the previous folder instead of deleting a character.

## 0.3.0

### New
- **Drive menu**: right-click a drive to open it, open it in a new tab or a terminal, copy its path, see its properties (capacity, free space, file system), eject it (USB sticks, SD cards, discs), or rename it (Windows and macOS).
- Drives you plug in or eject now appear and disappear on their own, and their free space stays up to date (right after copying or deleting, too).
- **Folder sizes** (Settings → Sorting & view): the Size column shows how big each folder is. Sizes are worked out in the background on all processor cores, so even big folders fill in fast, and sorting by size includes folders.
- **Quick Look**: press Space for a large preview of the selected file; the arrow keys move to the next one.
- **Compress to .zip** and **Extract here** (.zip, .tar, .tar.gz) from the right-click menu, with progress, cancel and undo.
- **Duplicate** (Ctrl D) makes a copy next to the original.
- **New folder with selection**: moves the selected items into a new folder and lets you name it.
- **Select none** (Ctrl Shift A) and **Invert selection** (Ctrl I).
- **Open in new window** from the right-click menu.
- The mouse's back and forward buttons go back and forward.

## 0.2.1

### New
- **Selection box**: drag across empty space to select several files at once, in the list or the grid. In the list, start on a row's blank part or its Size, Modified or Type columns (dragging the name still moves files). Hold Ctrl (⌘ on macOS) to add to the selection. The view scrolls when you drag near the top or bottom.

### Fixed
- Updating on macOS left an app that macOS refused to open. Updates now replace the whole app. Install this version by hand once; updates after it work by themselves.

## 0.2.0

### New
- **Grid view** (Ctrl G or the toolbar button): files as tiles, with thumbnails for images.
- **Drag and drop**: drag files onto a folder or a sidebar place to move them, or hold Ctrl (Option on macOS) to copy. On Windows and macOS you can also drag files out into other apps.
- **Properties** (Alt Enter): size, item counts, dates and image dimensions, with switches for Read-only (and Hidden on Windows).
- **Open with…**: pick which app opens a file. On Windows this is the usual system chooser.
- **Path bar completion**: press Tab to complete folder names while typing a path. Typing something that isn't a path searches for files instead.
- **Retry as administrator** on folders Windows won't let you read.
- **Windows installer** and **macOS disk image** downloads. The Windows installer needs no administrator rights and adds a Start menu entry and an uninstaller.

### Changed
- New app icon.
- New, cleaner icons throughout the app, from Lucide.
- The theme option "Match Windows" is now called "Match system".

### Fixed
- The refresh button's icon looked broken.

## 0.1.0

First release.

- Browse folders with tabs, back and forward, and an editable path bar.
- Sort by any column (remembered per folder) and filter by name, type, size or date.
- Rename in place (F2) or rename many files at once with a preview (Ctrl F2).
- Cut, copy and paste, including with Explorer and other apps, with progress and cancel for big copies.
- Move to the Recycle Bin and undo with Ctrl Z.
- Preview panel for images and text files.
- Command palette (Ctrl P) for commands, places and files.
- Git status badges and branch info in repositories.
- Light and dark themes, accent colors, compact rows, and rebindable shortcuts.
- Update checks with one-click install.
