# Explorist

A fast, keyboard-friendly file manager for Windows, with Linux and macOS
builds too, written in Rust with [egui](https://github.com/emilk/egui).

## Installing

Download from the latest release:

- **Windows:** `explorist-<version>-windows-x86_64.zip`. Unzip and run
  `explorist.exe`. Windows SmartScreen may warn because the build isn't
  signed; choose *More info → Run anyway*.
- **Linux:** `explorist-<version>-linux-x86_64.tar.gz`. Unpack and run
  `explorist` (X11 or Wayland). To add it to your app menu, copy
  `explorist` to `~/.local/bin`, `explorist.png` to
  `~/.local/share/icons/` and `explorist.desktop` to
  `~/.local/share/applications/`.
- **macOS:** `explorist-<version>-macos-universal.zip` (Apple Silicon and
  Intel). Unzip and move `Explorist.app` to Applications. The app isn't
  notarized, so the first time right-click it and choose *Open*, or run
  `xattr -dr com.apple.quarantine /Applications/Explorist.app`.

## Status

Early development, Windows first. Working: browsing with tabs, sorting
(per-folder), filtering, inline and batch rename, cut/copy/paste through the
Windows clipboard, Recycle Bin and undo, background copy/move with progress,
auto-refresh, a grid view with thumbnails, a preview panel, a command palette, git status, a Settings page
with rebindable shortcuts, and update checks.

## Keyboard shortcuts

| Action | Shortcut |
|---|---|
| Command palette / recent places | Ctrl P / Ctrl E (`>` commands, `@` places, `/` files) |
| Open | Enter (or double-click) |
| Back / Forward | Alt ← / Alt → (Backspace also goes back) |
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
| Select all | Ctrl A |
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

Most shortcuts can be changed in Settings → Keyboard.

## Building

```
cargo run            # debug build
cargo run -- D:\some\folder   # start in a folder
cargo build --release
```

Settings are saved to `%APPDATA%\Explorist\settings.toml`.

## Releasing

1. Set the new `version` in `Cargo.toml` and commit.
2. Tag and push: `git tag v0.2.0 && git push origin v0.2.0`, or push the
   commit to the `release` branch (`git push origin main:release`) and the
   workflow creates the tag.
3. The Release workflow builds Windows, Linux and macOS packages and
   publishes them in one GitHub Release. Installed copies offer the update within a day (or on
   *Settings → General → Check now*).

The update check reads the repository's latest release, so it only finds
releases while the repository (or a release mirror) is public.

## Fonts

Explorist bundles [Geist and Geist Mono](https://vercel.com/font) under the SIL Open Font License; see `assets/fonts/OFL.txt`.
