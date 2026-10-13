# Explorist

A fast, keyboard-friendly file manager for Windows, written in Rust with [egui](https://github.com/emilk/egui).

## Status

Early development. The main window works: browsing, sorting, filtering, rename, new file/folder,
Recycle Bin with undo, and folders with tens of thousands of items. Preview panel, command
palette, batch rename, settings screen, tabs and update checks are planned next.

## Keyboard shortcuts

| Action | Shortcut |
|---|---|
| Open | Enter (or double-click) |
| Back / Forward | Alt ← / Alt → (Backspace also goes back) |
| Up one level | Alt ↑ |
| Edit path | Ctrl L or Alt D |
| Filter by name | Ctrl F (`*` and `?` work as wildcards) |
| Rename | F2 (Tab renames the next item) |
| New file / New folder | Ctrl N / Ctrl Shift N |
| Move to Recycle Bin | Del |
| Delete permanently | Shift Del (asks first) |
| Undo | Ctrl Z |
| Copy path | Ctrl Shift C |
| Select all | Ctrl A |
| Show hidden files | Ctrl H |
| Open terminal here | Ctrl \` |
| Refresh | F5 or Ctrl R |
| Jump to sidebar place | Alt 1–9 |
| Move / extend selection / move focus only | ↑ ↓ PgUp PgDn Home End, with Shift / Ctrl |
| Toggle the focused item | Ctrl Space |
| Jump to a name | Start typing |

## Building

```
cargo run            # debug build
cargo run -- D:\some\folder   # start in a folder
cargo build --release
```

Settings are saved to `%APPDATA%\Explorist\settings.toml`.

## Fonts

Explorist bundles [Geist and Geist Mono](https://vercel.com/font) under the SIL Open Font License; see `assets/fonts/OFL.txt`.
