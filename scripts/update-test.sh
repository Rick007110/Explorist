#!/usr/bin/env bash
# End-to-end test of the built-in updater: installs the current build,
# serves a fake "9.9.9" release packaged like the real ones from a local
# web server, runs the installed copy's updater against it, and checks the
# installed copy is now 9.9.9. On macOS the app is a signed Explorist.app,
# and its signature must still verify after the update.
#
# Run from the repository root: scripts/update-test.sh
# Leaves Cargo.toml and Cargo.lock as they were.
set -euo pipefail

work=$(mktemp -d)
port=8765
cp Cargo.toml "$work/Cargo.toml.orig"
cp Cargo.lock "$work/Cargo.lock.orig"
server=""
cleanup() {
  cp "$work/Cargo.toml.orig" Cargo.toml
  cp "$work/Cargo.lock.orig" Cargo.lock
  [ -n "$server" ] && kill "$server" 2>/dev/null || true
}
trap cleanup EXIT

case "$(uname -s)" in
  Linux*) os=linux ;;
  Darwin*) os=macos ;;
  MINGW* | MSYS* | CYGWIN*) os=windows ;;
  *) echo "unknown OS $(uname -s)"; exit 1 ;;
esac
exe=explorist
[ "$os" = windows ] && exe=explorist.exe
python=$(command -v python3 || command -v python)
# Talk to the local server directly, not through any proxy.
export NO_PROXY="127.0.0.1,localhost" no_proxy="127.0.0.1,localhost"

# A macOS app around a program, signed the way the release signs it.
make_app() { # make_app <program> <version> <dir>
  local app="$3/Explorist.app"
  mkdir -p "$app/Contents/MacOS" "$app/Contents/Resources"
  cp "$1" "$app/Contents/MacOS/explorist"
  cat > "$app/Contents/Info.plist" <<PLIST
<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0">
<dict>
  <key>CFBundleIdentifier</key><string>io.github.rick007110.explorist</string>
  <key>CFBundleExecutable</key><string>explorist</string>
  <key>CFBundlePackageType</key><string>APPL</string>
  <key>CFBundleShortVersionString</key><string>$2</string>
  <key>CFBundleVersion</key><string>$2</string>
</dict>
</plist>
PLIST
  codesign --force --deep --sign - "$app"
}

old_version=$(sed -n 's/^version = "\(.*\)"/\1/p' Cargo.toml | head -1)
echo "== Building $old_version (installed copy)"
cargo build --quiet
mkdir -p "$work/install"
if [ "$os" = macos ]; then
  make_app "target/debug/$exe" "$old_version" "$work/install"
  installed="$work/install/Explorist.app/Contents/MacOS/explorist"
else
  cp "target/debug/$exe" "$work/install/$exe"
  installed="$work/install/$exe"
fi

echo "== Building 9.9.9 (the fake release)"
"$python" - <<'PY'
import re
text = open("Cargo.toml").read()
open("Cargo.toml", "w").write(re.sub(r'(?m)^version = ".*"', 'version = "9.9.9"', text, count=1))
PY
cargo build --quiet
mkdir -p "$work/serve" "$work/pkg/explorist-9.9.9"
case "$os" in
  linux)
    asset=explorist-9.9.9-linux-x86_64.tar.gz
    cp "target/debug/$exe" "$work/pkg/explorist-9.9.9/"
    tar -czf "$work/serve/$asset" -C "$work/pkg" explorist-9.9.9
    ;;
  windows)
    asset=explorist-9.9.9-windows-x86_64.zip
    cp "target/debug/$exe" "$work/pkg/explorist-9.9.9/"
    (cd "$work/pkg" && "$python" -c "
import zipfile
with zipfile.ZipFile('../serve/$asset', 'w', zipfile.ZIP_DEFLATED) as z:
    z.write('explorist-9.9.9/explorist.exe')
")
    ;;
  macos)
    asset=explorist-9.9.9-macos-universal.zip
    make_app "target/debug/$exe" 9.9.9 "$work/pkg"
    ditto -c -k --keepParent "$work/pkg/Explorist.app" "$work/serve/$asset"
    ;;
esac
cat > "$work/serve/latest.json" <<JSON
{"tag_name": "v9.9.9", "body": "- Test", "html_url": "http://127.0.0.1:$port/",
 "assets": [{"name": "$asset", "browser_download_url": "http://127.0.0.1:$port/$asset"}]}
JSON

(cd "$work/serve" && exec "$python" -m http.server "$port" --bind 127.0.0.1) > /dev/null 2>&1 &
server=$!
for _ in $(seq 1 50); do
  curl -sf "http://127.0.0.1:$port/latest.json" > /dev/null && break
  sleep 0.2
done

echo "== Before: $("$installed" --version)"
export EXPLORIST_UPDATE_API="http://127.0.0.1:$port/latest.json"
"$installed" --update-now
after=$("$installed" --version | tr -d '\r')
echo "== After: $after"
[ "$after" = 9.9.9 ] || { echo "FAIL: expected 9.9.9"; exit 1; }

if [ "$os" = macos ]; then
  codesign --verify --deep --strict --verbose=2 "$work/install/Explorist.app"
  plist=$(defaults read "$work/install/Explorist.app/Contents/Info" CFBundleShortVersionString)
  [ "$plist" = 9.9.9 ] || { echo "FAIL: Info.plist says $plist"; exit 1; }
  leftovers=$(ls -A "$work/install" | grep -v '^Explorist.app$' || true)
  [ -z "$leftovers" ] || { echo "FAIL: left behind: $leftovers"; exit 1; }
fi

again=$("$installed" --update-now | tr -d '\r')
[ "$again" = "up to date" ] || { echo "FAIL: second check said: $again"; exit 1; }
echo "PASS: updated $old_version -> 9.9.9 on $os"
