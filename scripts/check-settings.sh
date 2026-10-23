#!/usr/bin/env bash
#
# Fail when the apps disagree on a synced setting's default
# (docs/plans/034_settings_text_defaults_reset.md). The same user's phone, Mac
# and browser read one stored value, and a default that differs between them
# makes the same unset setting behave differently on each. The Linux app's
# table (plan 039) joins them.
#
# Each app keeps its synced settings in one table, a key and its default
# together ("longFiles.seekBack", 10); the server's readSetting calls have the
# same shape.
set -euo pipefail
cd "$(dirname "$0")/.."

pairs() { # file [line filter]: "name default" per line, sorted
	grep -E "${2:-.}" "$1" | grep -oE '"[A-Za-z]+\.[A-Za-z]+", *[^,)]+' |
		sed -E 's/^"([^"]+)", *"?([^"]*)"?$/\1 \2/; s/\.0$//' | sort -u
}

android=$(pairs android/app/src/main/java/io/github/vivekg7/dhun/data/Synced.kt)
mac=$(pairs macos/Sources/DhunKit/Data/Synced.swift)
web=$(pairs server/web/js/synced.js)
linux=$(pairs linux/core/src/synced.rs)
server=$(pairs server/internal/api/sync.go readSetting)

bad=0
[ "$android" = "$mac" ] || { echo "Android and macOS differ:"; diff <(echo "$android") <(echo "$mac") || true; bad=1; }
[ "$android" = "$web" ] || { echo "Android and the web differ:"; diff <(echo "$android") <(echo "$web") || true; bad=1; }
[ "$android" = "$linux" ] || { echo "Android and Linux differ:"; diff <(echo "$android") <(echo "$linux") || true; bad=1; }
missing=$(comm -13 <(echo "$android") <(echo "$server"))
[ -z "$missing" ] || { echo "The server's defaults differ from the apps':"; echo "$missing"; bad=1; }
exit $bad
