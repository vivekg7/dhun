#!/usr/bin/env bash
#
# Copy docs/GUIDE.md, the How it works page, into each app
# (docs/plans/034_settings_text_defaults_reset.md). Gradle's assets, SwiftPM and
# go:embed each read only their own folders, so every app carries a committed
# copy; the Mac's is a Swift string, because a SwiftPM resource would need a
# resource bundle copied into Dhun.app by hand.
#
#   scripts/guide.sh          write the copies (make fmt)
#   scripts/guide.sh --check  fail when a copy is out of date (make lint)
set -euo pipefail
cd "$(dirname "$0")/.."

src=docs/GUIDE.md
out=$(mktemp -d)
trap 'rm -rf "$out"' EXIT

cp "$src" "$out/android.md"
cp "$src" "$out/web.md"
{
	echo "// Written by scripts/guide.sh from docs/GUIDE.md: edit that, then run make fmt."
	echo 'let guideText = #"""'
	# Indented as swift-format would, so make fmt leaves it alone; Swift strips
	# the closing delimiter's indent from every line.
	sed 's/^\(.\)/    \1/' "$src"
	echo '    """#'
} >"$out/Guide.swift"

copies=(
	"android.md:android/app/src/main/assets/guide.md"
	"web.md:server/web/guide.md"
	"Guide.swift:macos/Sources/Dhun/Guide.swift"
)
stale=0
for c in "${copies[@]}"; do
	from=$out/${c%%:*} to=${c#*:}
	if [ "${1:-}" = --check ]; then
		cmp -s "$from" "$to" || { echo "$to is out of date with $src: run make fmt"; stale=1; }
	else
		cp "$from" "$to"
	fi
done
exit $stale
