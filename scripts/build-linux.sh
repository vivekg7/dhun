#!/usr/bin/env bash
#
# Build the Linux app's Flatpak (plan 039) and archive it into local/, as
# build-mac.sh does for the Mac. Run at will, or `make linux`.
#
#   ./scripts/build-linux.sh            build, archive
#   ./scripts/build-linux.sh --force    allow replacing an existing archive
#   ./scripts/build-linux.sh --install  also install it for this user
#
# Needs flatpak-builder once: flatpak install --user flathub org.flatpak.Builder
#
# A clean checkout produces "dhun-linux-v0.16.3.flatpak"; a dirty working
# tree "dhun-linux-v0.16.3-dirty-g1a2b3c4.flatpak", so work in progress never
# takes a release's name. An existing archive is never overwritten without --force.

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
cd "$ROOT/linux"

ID=io.github.vivekg7.dhun
REPO=target/flatpak-repo
flatpak run --user org.flatpak.Builder --user --install-deps-from=flathub --force-clean \
    --repo="$REPO" build-dir "$ID.yml"

VERSION=$(sed -n 's/^version = "\(.*\)"/\1/p' app/Cargo.toml | head -1)
SUFFIX=""
if [ -n "$(git status --porcelain 2>/dev/null)" ]; then
    SUFFIX="-dirty-g$(git rev-parse --short HEAD 2>/dev/null || echo unknown)"
    echo "warning: working tree is dirty; archiving as a work-in-progress build"
fi
mkdir -p "$ROOT/local"
TARGET="$ROOT/local/dhun-linux-v${VERSION}${SUFFIX}.flatpak"
if [ -e "$TARGET" ] && [ "$FORCE" != 1 ]; then
    echo "error: $TARGET already exists. Raise the version in linux/app/Cargo.toml, or pass --force." >&2
    exit 1
fi
rm -f "$TARGET"
flatpak build-bundle --runtime-repo=https://flathub.org/repo/flathub.flatpakrepo "$REPO" "$TARGET" "$ID"
sha256sum "$TARGET" | awk '{print $1}' > "$TARGET.sha256"
echo "archived: $TARGET  ($(wc -c < "$TARGET" | tr -d ' ') bytes)"

if [ "$INSTALL" = 1 ]; then
    flatpak install --user -y --reinstall "$TARGET"
    echo "installed: flatpak run $ID"
fi
