# Working on Explorist

## Releases

- **Only release when the owner explicitly asks** for that release ("release
  0.3.2"). A request for a bug sweep, a fix or a check is not permission to
  release, and neither is earlier permission for another version.
- **Before every release, do a bug sweep** and report what it found, then
  wait for the go-ahead:
  1. Review everything changed since the last release (`git log` /
     `git diff v<last>..main`) for logic mistakes and edge cases: every
     entry point to a feature (toolbar buttons, menus, shortcuts and the
     palette can each have their own code path), empty input, things
     running at the same time, new tabs and the start page, and keys that
     mean something else while typing.
  2. Drive the app under Xvfb through the changed features and the main
     flows (tabs, search, rename/undo, copy/duplicate/undo, grid, Quick
     Look, Properties, settings), check screenshots, and look in the log
     for panics.
  3. Fix what turns up, add tests where they fit, and note each fix in
     `CHANGELOG.md`.
  4. `cargo fmt`, `cargo clippy --all-targets -- -D warnings` (also with
     `--target x86_64-pc-windows-msvc`), `cargo test`, and wait for CI to
     pass on Windows, macOS and Linux before pushing to `release`.
- Say plainly what couldn't be tested here (anything needing real
  Windows/macOS hardware: eject, drive rename, drag-out, the installer).

## Releasing, once asked

1. Add a `## <version>` section to `CHANGELOG.md` in plain words for users.
2. Set `version` in `Cargo.toml`, commit, push `main`, then
   `git push origin main:release`. The Release workflow builds and publishes
   everything; its notes come from the changelog section.

## Commits

- Author: `Rick007110 <71711131+Rick007110@users.noreply.github.com>`
  (set per command or in the repo's local config, never globally).
- Don't put model names in commits, code or docs.
