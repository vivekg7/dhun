#!/usr/bin/env bash
#
# Build Dhun.app (plan 024) and archive it into local/ as a zip, as
# archive-apk.sh does for the phone. Run at will, or `make mac`.
#
#   ./scripts/build-mac.sh            build, sign ad hoc, archive
#   ./scripts/build-mac.sh --force    allow replacing an existing archive
#   ./scripts/build-mac.sh --install  also copy the app to /Applications
#
# Signed ad hoc: there is no Developer ID. On another Mac the first launch
# is allowed once under System Settings → Privacy & Security.
#
# A clean checkout produces "dhun-mac-v0.1.0.zip"; a dirty working tree
# "dhun-mac-v0.1.0-dirty-g1a2b3c4.zip", so work in progress never takes a
# release's name. An existing archive is never overwritten without --force.

set -euo pipefail

usage() { awk 'NR>1 && /^#/ { sub(/^# ?/, ""); print; next } NR>1 { exit }' "$0"; }

FORCE=0
INSTALL=0
for arg in "$@"; do
    case "$arg" in
        --force) FORCE=1 ;;
        --install) INSTALL=1 ;;
        -h|--help) usage; exit 0 ;;
        *) echo "unknown option: $arg" >&2; exit 2 ;;
    esac
done

ROOT=$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)
cd "$ROOT"

(cd macos && swift build -c release --product Dhun)
BIN=$(cd macos && swift build -c release --product Dhun --show-bin-path)/Dhun

APP=macos/.build/Dhun.app
rm -rf "$APP"
mkdir -p "$APP/Contents/MacOS" "$APP/Contents/Resources"
cp "$BIN" "$APP/Contents/MacOS/Dhun"
cp macos/Info.plist "$APP/Contents/Info.plist"

ICONSET=macos/.build/Dhun.iconset
rm -rf "$ICONSET"
swift macos/make-icon.swift "$ICONSET"
iconutil -c icns "$ICONSET" -o "$APP/Contents/Resources/Dhun.icns"

codesign --force --sign - --options runtime "$APP"

VERSION=$(/usr/libexec/PlistBuddy -c 'Print :CFBundleShortVersionString' "$APP/Contents/Info.plist")
SUFFIX=""
if [ -n "$(git status --porcelain 2>/dev/null)" ]; then
    SUFFIX="-dirty-g$(git rev-parse --short HEAD 2>/dev/null || echo unknown)"
    echo "warning: working tree is dirty; archiving as a work-in-progress build"
fi
mkdir -p local
TARGET="local/dhun-mac-v${VERSION}${SUFFIX}.zip"
if [ -e "$TARGET" ] && [ "$FORCE" != 1 ]; then
    echo "error: $TARGET already exists. Raise the version in macos/Info.plist, or pass --force." >&2
    exit 1
fi
rm -f "$TARGET"
ditto -c -k --keepParent "$APP" "$TARGET"
shasum -a 256 "$TARGET" | awk '{print $1}' > "$TARGET.sha256"
echo "built:    $APP"
echo "archived: $TARGET  ($(wc -c < "$TARGET" | tr -d ' ') bytes)"

if [ "$INSTALL" = 1 ]; then
    rm -rf /Applications/Dhun.app
    cp -R "$APP" /Applications/
    echo "installed: /Applications/Dhun.app"
fi
