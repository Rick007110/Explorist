# Explorist

A blazing-fast, keyboard-first file browser for **Windows, macOS and Linux**, with a
Spotlight/Raycast-style search bar for your apps and files. Written in Rust with
[egui](https://github.com/emilk/egui). Free and open source.

## Highlights

- **Quick search from anywhere** *(coming in the next release)*. Press **Alt Space**
  (Option Space on macOS) in any app and a search box pops up over everything,
  finding any app or file on your device as you type. Apps come first, with their
  icons, matched by name or initials ("vsc" finds Visual Studio Code). **Enter**
  starts the app or opens the file; **Ctrl Enter** (⌘ Enter) shows it in Explorist.
- **Fast.** Explorist keeps the list of every file name on your device between runs,
  so searching everywhere works as soon as it starts and takes a few milliseconds,
  even with millions of files *(coming in the next release)*.
- **Keyboard first.** Every action has a shortcut, most can be changed in Settings,
  and the command palette (**Ctrl P**) reaches everything else.
- **A complete file manager.** Tabs, list and grid views with thumbnails, Quick Look,
  a preview panel, inline and batch rename, undo, background copy and move with
  progress, drag and drop, git status, Properties and Open with.
- **Keeps itself up to date.** New versions install from inside the app.

Explorist is updated often and ideas are welcome:
[open an issue](https://github.com/Rick007110/Explorist/issues).

## Installing

Download from the [latest release](https://github.com/Rick007110/Explorist/releases/latest):

- **Windows:** `explorist-<version>-windows-x86_64-setup.exe`. It installs
  for your user only (no administrator prompt) and adds Explorist to the
  Start menu, with an uninstaller in Settings → Apps. Windows SmartScreen
  may warn because the installer isn't code-signed; choose *More info → Run
  anyway*. A portable `.zip` is there too.
- **Linux:** `explorist-<version>-linux-x86_64.tar.gz`. Unpack and run
  `explorist` (X11 or Wayland). To add it to your app menu, copy
  `explorist` to `~/.local/bin`, `explorist.png` to
  `~/.local/share/icons/` and `explorist.desktop` to
  `~/.local/share/applications/`.
- **macOS:** `explorist-<version>-macos-universal.dmg` (Apple Silicon and
  Intel). Open it and drag Explorist to Applications. The app isn't
  notarized, so the first time right-click it and choose *Open*, or run
  `xattr -dr com.apple.quarantine /Applications/Explorist.app`.

The `.zip` and `.tar.gz` files are also what the built-in updater
downloads.

## Status

Early development, updated often. Explorist runs on Windows, macOS and Linux;
Windows gets the most testing. A few things differ by system: on Windows, cut, copy
and paste share the system clipboard with Explorer, while on macOS and Linux they work
within Explorist; dragging files out into other apps works on Windows and macOS.

## Keyboard shortcuts

| Action | Shortcut |
|---|---|
| Command palette / recent places | Ctrl P / Ctrl E (`>` commands, `@` places, `/` files; in file search, Ctrl E switches between this folder and everywhere) |
| Open | Enter (or double-click) |
| Back / Forward | Alt ← / Alt → (Backspace and the mouse's side buttons too) |
| Up one level | Alt ↑ |
| Edit path | Ctrl L or Alt D (Tab completes folders; anything that isn't a path searches) |
| Filter by name | Ctrl F (`*` and `?` work as wildcards) |
| Rename / batch rename | F2 (Tab renames the next item) / Ctrl F2 |
| New file / New folder | Ctrl N / Ctrl Shift N |
| Cut / Copy / Paste | Ctrl X / Ctrl C / Ctrl V (works with Explorer too) |
| Move to Recycle Bin | Del |
| Delete permanently | Shift Del (asks first) |
| Undo | Ctrl Z |
| Copy path | Ctrl Shift C |
| Properties | Alt Enter |
| Select all / none / invert | Ctrl A / Ctrl Shift A / Ctrl I |
| Quick Look | Space (arrows move to the next item) |
| Duplicate | Ctrl D |
| Show hidden files | Ctrl H |
| Preview panel | Ctrl . |
| List / grid view | Ctrl G |
| New tab / close tab / next tab | Ctrl T / Ctrl W / Ctrl Tab |
| Open terminal here | Ctrl \` |
| Refresh | F5 or Ctrl R |
| Settings | Ctrl , |
| Jump to sidebar place | Alt 1–9 |
| Move / extend selection / move focus only | ↑ ↓ PgUp PgDn Home End, with Shift / Ctrl |
| Toggle the focused item | Ctrl Space |
| Jump to a name | Start typing |
| Select with a box | Drag across empty space (Ctrl adds to the selection) |
| Move / copy by dragging | Drag onto a folder or sidebar place (hold Ctrl, or Option on macOS, to copy) |

Most shortcuts can be changed in Settings → Keyboard. On macOS, use ⌘ where it
says Ctrl and Option where it says Alt.

## Building

```
cargo run            # debug build
cargo run -- D:\some\folder   # start in a folder
cargo build --release
```

Settings are saved to `settings.toml` in `%APPDATA%\Explorist` on Windows,
`~/Library/Application Support/Explorist` on macOS and `~/.config/Explorist` on
Linux.

## Releasing

1. Set the new `version` in `Cargo.toml`, add a `## <version>` section to
   `CHANGELOG.md` saying what changed for users, and commit. The section
   becomes the release notes (the release fails without one).
2. Tag and push: `git tag v0.2.0 && git push origin v0.2.0`, or push the
   commit to the `release` branch (`git push origin main:release`) and the
   workflow creates the tag.
3. The Release workflow builds the Windows installer and zip, the macOS
   disk image and zip, and the Linux tarball, and publishes them in one
   GitHub Release. To try packaging changes without publishing, push to
   `release-dry-run`. Installed copies offer the update the next time they
   start, or within 4 hours while running (or on *Settings → General →
   Check now*).

For a beta, use a version like `0.4.0-beta1`: it's published as a
pre-release, and only copies with *Settings → General → Get beta
versions* turned on offer it.

To fix a published release's notes, edit `CHANGELOG.md` and push to the
`release-notes` branch (`git push origin main:release-notes`).

The update check reads the repository's latest release (with betas turned
on, its newest release, pre-releases included), so it only finds releases
while the repository (or a release mirror) is public.

## Fonts and icons

Explorist bundles [Geist and Geist Mono](https://vercel.com/font) under the SIL Open Font License (see `assets/fonts/OFL.txt`) and icons from [Lucide](https://lucide.dev) under the ISC License (see `assets/icons/lucide/LICENSE`).

## License

Explorist is free software: you can use, change and share it under the GNU
General Public License, version 3 or (at your option) any later version. See
[LICENSE](LICENSE). The bundled fonts and icons keep their own licenses, listed
above.
