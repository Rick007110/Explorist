# Explorist

A fast, keyboard-friendly file manager for Windows, written in Rust with [egui](https://github.com/emilk/egui).

## Status

Early development, Windows first. Working: browsing with tabs, sorting
(per-folder), filtering, inline and batch rename, cut/copy/paste through the
Windows clipboard, Recycle Bin and undo, background copy/move with progress,
auto-refresh, a preview panel, a command palette, git status, a Settings page
with rebindable shortcuts, and update checks.

## Keyboard shortcuts

| Action | Shortcut |
|---|---|
| Command palette / recent places | Ctrl P / Ctrl E (`>` commands, `@` places, `/` files) |
| Open | Enter (or double-click) |
| Back / Forward | Alt ← / Alt → (Backspace also goes back) |
| Up one level | Alt ↑ |
| Edit path | Ctrl L or Alt D |
| Filter by name | Ctrl F (`*` and `?` work as wildcards) |
| Rename / batch rename | F2 (Tab renames the next item) / Ctrl F2 |
| New file / New folder | Ctrl N / Ctrl Shift N |
| Cut / Copy / Paste | Ctrl X / Ctrl C / Ctrl V (works with Explorer too) |
| Move to Recycle Bin | Del |
| Delete permanently | Shift Del (asks first) |
| Undo | Ctrl Z |
| Copy path | Ctrl Shift C |
| Select all | Ctrl A |
| Show hidden files | Ctrl H |
| Preview panel | Ctrl . |
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
2. Tag and push: `git tag v0.2.0 && git push origin v0.2.0`.
3. The Release workflow builds `explorist.exe`, zips it and publishes a
   GitHub Release. Installed copies offer the update within a day (or on
   *Settings → General → Check now*).

The update check reads the repository's latest release, so it only finds
releases while the repository (or a release mirror) is public.

## Fonts

Explorist bundles [Geist and Geist Mono](https://vercel.com/font) under the SIL Open Font License; see `assets/fonts/OFL.txt`.
