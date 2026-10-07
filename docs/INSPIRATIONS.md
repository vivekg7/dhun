# Inspirations

Projects this one learns from. Credit goes here **in the same commit** as the
idea or code it covers. Code copied from a GPL-3.0 project also keeps its
original copyright notice in the file it lands in.

This list doubles as the watch list for the `upstream-watch` skill
(`.agents/skills/upstream-watch/`). That skill reviews these projects
regularly for new features, issues and bugs worth acting on. `Last reviewed`
is the date of the latest review. The findings of each review go to
[`upstream/`](upstream/).

## Credited

What we have actually taken, and from where.

| Project                                                                                                                                       | What we took                                                                                                                                                     | Where in our code                                                         |
| --------------------------------------------------------------------------------------------------------------------------------------------- | ---------------------------------------------------------------------------------------------------------------------------------------------------------------- | ------------------------------------------------------------------------- |
| Musicolet (behaviour only, never code)                                                                                                        | Remembering the place in long files, with a minimum length and resume-or-ask ([plan 009](plans/009_resume_long_files.md)); Favorites as a built-in list          | `server/internal/api/sync.go` (`resume`, `mark`)                          |
| Musicolet (behaviour only, never code)                                                                                                        | The Android layout: icon-only tabs in its order, a search box per tab, the current song marked in the accent, theme modes ([plan 011](plans/011_android_app.md)) | `android/app/src/main/java/io/github/vivekg7/dhun/ui/`                    |
| [Material Symbols](https://github.com/google/material-design-icons) (Google; Apache 2.0)                                                      | The path data of the app's ~40 icons                                                                                                                             | `android/.../ui/Icons.kt`                                                 |
| Immich, Navidrome                                                                                                                             | Creating the first admin from environment variables, read only while there are no users ([plan 003](plans/003_deployment.md))                                    | `server/cmd/dhun/main.go`, `EnsureAdmin` in `server/internal/api/auth.go` |
| [go.senan.xyz/taglib](https://github.com/sentriz/go-taglib) (by Gonic's author; TagLib compiled to WebAssembly; LGPL-2.1, GPL-3.0-compatible) | Tag and audio-property reading without cgo, so the server is one static binary                                                                                   | `server/internal/library/tags.go`, `media.go`                             |

## Watched

| Project      | Why we watch it                                                                                                                                                                           | Licence      | Source                                                                              | Last reviewed           |
| ------------ | ----------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- | ------------ | ----------------------------------------------------------------------------------- | ----------------------- |
| Musicolet    | The feature benchmark: multiple queues, sleep timer, lyrics, everything in the [inventory](research/musicolet-feature-inventory.md). Closed source; review its changelog, never its code. | Proprietary  | [krosbits.in/musicolet](https://krosbits.in/musicolet/)                             | 2026-10-06 (APK 6.14.1) |
| Symfonium    | The only other player with multiple queues; well-polished offline and caching. Closed source; review its docs and forum.                                                                  | Proprietary  | [symfonium.app](https://symfonium.app/)                                             | 2026-10-06              |
| Navidrome    | Server: scanning, tag parsing, lyrics, Subsonic API.                                                                                                                                      | GPL-3.0      | [navidrome/navidrome](https://github.com/navidrome/navidrome)                       | 2026-10-06              |
| Gonic        | Server: real folder browsing, light Go scanner.                                                                                                                                           | GPL-3.0      | [sentriz/gonic](https://github.com/sentriz/gonic)                                   | 2026-10-06              |
| LMS          | Server: lyrics handling, low-footprint design.                                                                                                                                            | GPL-3.0      | [epoupon/lms](https://github.com/epoupon/lms)                                       | 2026-10-06              |
| OpenSubsonic | The API we add for compatibility after v1.                                                                                                                                                | —            | [opensubsonic/open-subsonic-api](https://github.com/opensubsonic/open-subsonic-api) | 2026-10-06              |
| Ultrasonic   | Android: cache-then-pin offline model (a cached song becomes a download by renaming the file).                                                                                            | GPL-3.0      | [gitlab.com/ultrasonic/ultrasonic](https://gitlab.com/ultrasonic/ultrasonic)        | 2026-10-06              |
| Tempus       | Android: separate download and stream caches, ReplayGain, Media3 usage.                                                                                                                   | GPL-3.0      | [eddyizm/tempus](https://github.com/eddyizm/tempus)                                 | 2026-10-06              |
| Navic        | Android: Compose + Media3, word-level lyrics, Android Auto.                                                                                                                               | GPL-3.0      | [paigely/Navic](https://github.com/paigely/Navic)                                   | 2026-10-06              |
| Feishin      | Web and desktop UI patterns.                                                                                                                                                              | GPL-3.0      | [jeffvli/feishin](https://github.com/jeffvli/feishin)                               | 2026-10-06              |
| Submariner   | Native macOS client (AppKit).                                                                                                                                                             | BSD-3-Clause | [SubmarinerApp/Submariner](https://github.com/SubmarinerApp/Submariner)             | 2026-10-06              |
| Amperfy      | Native Swift iOS and macOS client with offline support.                                                                                                                                   | GPL-3.0      | [BLeeEZ/amperfy](https://github.com/BLeeEZ/amperfy)                                 | 2026-10-06              |

The 2026-10-06 dates mark the initial survey in
[research/existing-options.md](research/existing-options.md), not a full
upstream review.
