# Changelog

What's new in each version of Explorist. The release workflow publishes
each version's section as its release notes, and the in-app update prompt
shows the first points.

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
