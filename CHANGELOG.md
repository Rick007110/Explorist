# Changelog

What's new in each version of Explorist. The release workflow publishes
each version's section as its release notes, and the in-app update prompt
shows the first points.

## 0.4.0

### New
- **Quick search**: press **Alt Space** (Option Space on macOS) in any app to pop up a search box over everything and find any app or file on this device as you type, like Spotlight or Raycast. Apps come first, with their icons: on Windows every app in the Start menu (Store apps like Calculator and Settings included), on macOS the Applications folders, on Linux your desktop's app menu; type part of the name or its initials ("vsc" for Visual Studio Code). **Enter** starts the app or opens the file (a folder opens in Explorist), **Ctrl Enter** (⌘ Enter) shows it in Explorist, **Ctrl Shift C** copies its path (so does **Ctrl C**, unless you've selected text in the box), **Esc** closes. With nothing typed, it lists your recent folders. It's also in the tray icon's menu. Choose other keys, leave apps out, or turn it off in Settings → General. It works while Explorist is running, which with Close to tray means always. On Linux with Wayland, where apps can't set up such shortcuts, add one in your desktop's keyboard settings that runs `explorist --search`.
- **Beta versions**: turn on **Get beta versions** in Settings → General → Updates to be offered test versions of what's coming, so you can try new features before everyone else. Betas may have more bugs. It's off unless you turn it on, and turned off again, you keep your version until the next full release.

### Changed
- **Quick Look on a folder shows what's in it**: its folders and files with their sizes and dates, instead of only a large folder icon.
- **Searching everywhere is faster and ready at once**: the list of every file name on the device is saved between runs, so searches work right after Explorist starts, and it's refreshed in the background without the results going away meanwhile. Searching it is several times faster: a few milliseconds even with millions of files.
- **Explorist is now open source** under the GNU General Public License v3 (or later): you're free to use, study, change and share it.

### Fixed
- On Windows PCs whose graphics only offer the old OpenGL 1.1 (no graphics driver installed, and some virtual machines and Remote Desktop sessions), Explorist closed straight away without a word. Now it draws with Direct3D 12 there, which Windows can run without a graphics card, and remembers that for next time. If that doesn't work either, a message explains why it can't open.
- Clicking the tray icon, or opening Explorist again, might not bring back the window hidden in the tray on Windows.
- File names showed only as "…" when the window was narrow, as on smaller screens or with display scaling, with the preview panel open. Now the Type, Modified and Size columns make way for the name, in that order.
- After clicking **Edit** for a keyboard shortcut (Settings → Keyboard), leaving Settings without pressing keys made the next shortcut you pressed anywhere in Explorist become that shortcut.

## 0.4.0-beta3

A third test version of 0.4.0, with two fixes from testing 0.4.0-beta2. If you turned on Get beta versions, 0.4.0-beta2 offers it as an update.

### Changed
- **Quick Look on a folder shows what's in it**: its folders and files with their sizes and dates, instead of only a large folder icon.

### Fixed
- In quick search, **Ctrl C** copied the search text instead of the result's path when the box reopened with your last search in it. Now it copies the path unless you've selected some of the text yourself.

## 0.4.0-beta2

A second test version of 0.4.0. It adds a way to get betas through Explorist's own updates; everything else new in 0.4.0 is in the 0.4.0-beta1 notes. On 0.4.0-beta1, download this one by hand: that version can't find betas by itself.

### New
- **Beta versions**: turn on **Get beta versions** in Settings → General → Updates to be offered test versions of what's coming, so you can try new features before everyone else. Betas may have more bugs. It's off unless you turn it on, and turned off again, you keep your version until the next full release.

## 0.4.0-beta1

A test version of 0.4.0, to try before it's released for everyone. Explorist's own update check doesn't offer it: download it below.

### New
- **Quick search**: press **Alt Space** (Option Space on macOS) in any app to pop up a search box over everything and find any app or file on this device as you type, like Spotlight or Raycast. Apps come first, with their icons: on Windows every app in the Start menu (Store apps like Calculator and Settings included), on macOS the Applications folders, on Linux your desktop's app menu; type part of the name or its initials ("vsc" for Visual Studio Code). **Enter** starts the app or opens the file (a folder opens in Explorist), **Ctrl Enter** (⌘ Enter) shows it in Explorist, **Ctrl Shift C** copies its path (so does **Ctrl C** when no text is selected in the box), **Esc** closes. With nothing typed, it lists your recent folders. It's also in the tray icon's menu. Choose other keys, leave apps out, or turn it off in Settings → General. It works while Explorist is running, which with Close to tray means always. On Linux with Wayland, where apps can't set up such shortcuts, add one in your desktop's keyboard settings that runs `explorist --search`.

### Changed
- **Searching everywhere is faster and ready at once**: the list of every file name on the device is saved between runs, so searches work right after Explorist starts, and it's refreshed in the background without the results going away meanwhile. Searching it is several times faster: a few milliseconds even with millions of files.
- **Explorist is now open source** under the GNU General Public License v3 (or later): you're free to use, study, change and share it.

### Fixed
- On Windows PCs whose graphics only offer the old OpenGL 1.1 (no graphics driver installed, and some virtual machines and Remote Desktop sessions), Explorist closed straight away without a word. Now it draws with Direct3D 12 there, which Windows can run without a graphics card, and remembers that for next time. If that doesn't work either, a message explains why it can't open.
- Clicking the tray icon, or opening Explorist again, might not bring back the window hidden in the tray on Windows.
- File names showed only as "…" when the window was narrow, as on smaller screens or with display scaling, with the preview panel open. Now the Type, Modified and Size columns make way for the name, in that order.
- After clicking **Edit** for a keyboard shortcut (Settings → Keyboard), leaving Settings without pressing keys made the next shortcut you pressed anywhere in Explorist become that shortcut.

## 0.3.4

### New
- **Video thumbnails**: videos show a frame from the video, with a small play button in the grid. On Windows these are the same thumbnails Explorer shows, so any video your installed codecs can play works; macOS uses Quick Look; Linux uses ffmpeg (or ffmpegthumbnailer) if it's installed. On Windows and macOS, image types Explorist can't read itself, like HEIC, get thumbnails the same way. More video types are recognized too (M4V, WMV, FLV, MPEG, 3GP, M2TS, OGV).
- **Thumbnails in the list view**: images and videos show a small thumbnail in place of their icon, not only in the grid.

### Changed
- **Update checks happen more often**: Explorist looks for a new version every time it starts, when you bring it back from the tray, and every 4 hours while it keeps running, instead of once a day.

### Fixed
- Long file names in the right-click menu ran past its edge. Menus with archive names now widen to fit, and anything still too long is shortened with "…" and shown in full when you point at it.
- "Extract to" for .7z, .rar and other archive types (WinRAR, 7-Zip) named the folder with the archive's ending still on, like "photos.7z".

## 0.3.3

### New
- **Folder icons**: right-click a folder and choose **Change icon…** to give it one of 60 icons (music, photos, code, games, work, star and more) in your accent color or one of ten others, or **Use an image…** to pick any picture of your own (PNG, JPG, ICO, SVG, WebP, BMP or GIF). Icon files (.ico) work too, including ones that hold several sizes; the biggest is used. Explorist keeps its own copy of the picture, so the icon stays if the original is moved or deleted. Icons show in the list, the grid, pinned folders in the sidebar and the start page. Works on several folders at once; **Reset to plain folder** undoes it. Icons stay with folders you rename or move in Explorist.
- **Other apps in the right-click menu**: WinRAR (add to archive, extract here, extract to a folder), 7-Zip (the same), Sublime Text, Notepad++ and VS Code show up in the menu when they're installed. Turn each one on or off in Settings → Integrations, and add any other program there yourself (on macOS you can pick the app itself), with its own arguments and whether it's for files, folders or archives. With several archives selected, extracting extracts each one.
- **Close to tray** (Settings → General, on by default): closing the window keeps Explorist running in the notification area (the menu bar on macOS), so it opens again instantly. Click the icon to bring the window back, or right-click it and choose **Quit Explorist**. Opening Explorist again from the Start menu, Dock or launcher also brings back the hidden window instead of starting a second copy. Turn it off to have closing quit as before. On Linux this needs a desktop with a tray area; where there's none (stock GNOME), closing quits and Settings says so.

### Changed
- **Thumbnails load instantly**: image thumbnails in the grid are now saved on disk, so a folder you've opened before shows them at once, even after restarting Explorist. Changed images get a new thumbnail by themselves. The saved thumbnails take little space (about 30 KB each), the oldest are cleared past 500 MB, and Settings → Advanced shows the space used with a **Clear** button. Thumbnails also stay in memory when you move between folders, so going back is instant.

## 0.3.2

### Changed
- **New tabs open a start page** with your favorite places, drives (with free space) and recent folders, instead of a copy of a folder that's already open. Pick one to open it in that tab.

### Fixed
- The + button in the tab bar still opened a copy of the current folder.
- Back or Forward on a fresh tab hid the start page even though there was nowhere to go.
- Pressing Enter in an empty path bar opened an empty search; it now just closes the field.
- Files dropped onto a new tab's start page were copied into your Home folder.
- "New folder with selection" while a copy was running left an empty folder behind.
- File search said "No matches" while searching everywhere was still listing files.

## 0.3.1

### New
- **Search this folder or everywhere**: file search (Ctrl P, then `/`, or typing a name in the path bar) can look in the open folder and everything inside it, or on every drive of the device. Switch with the buttons in the search box or Ctrl E; the choice is remembered. Searching everywhere lists the device's file names in the background once, using all processor cores, so later searches are near-instant.

### Changed
- **Better file search**: only names that contain what you typed are found ("park" finds "park.jpg" and "Parking lot", not any name with those letters somewhere). Several words must all appear. Exact names come first, then names that start with the text.
- Results show the name first and its folder beside it, and the list scrolls.
- A new tab opens at your Home folder instead of copying the current tab.
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
