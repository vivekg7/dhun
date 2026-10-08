# 007 — Client architecture and repo layout

**Status:** `IN PROGRESS` — the Android app is being built ([011](011_android_app.md)); macOS and web not started
**Started:** 2026-10-06

## Problem

There are three clients with very different scopes. The Android app is
complete and offline-capable. The macOS and web clients are minimal
([REQUIREMENTS](../REQUIREMENTS.md#platforms)). We must decide what, if
anything, they share, and how the Android app is built so that multiple
queues, Downloads and offline sync fit on top of Media3.

## Shared code or not

**A. Kotlin Multiplatform core** (API client, models, sync engine, database)
used by Android and, through a Swift framework, by macOS. One sync engine,
written once. Costs: a Kotlin/Native toolchain inside the macOS build, rough
Swift interop (no `async`/`await`, generic types are lost), and the
lightweight macOS app would carry a Kotlin runtime.

**B. Separate native clients, with the API as the only shared contract.**
Chosen. The hard client logic — the offline outbox, Downloads, the 20-queue
player — is needed **only on Android**. The minimal clients are
**online-only**: they send each operation straight to the server and render
its reply, so they have no outbox and nothing to merge. A shared core would be
shared with nobody. Revisit if an iOS app or a full offline macOS app is ever
wanted.

**Online-only macOS and web** was confirmed by the owner (2026-10-06). Both
are used at home or the office over Tailscale, where the server is reachable.

## Android

| Concern      | Choice                                                                                                                                                                                                                                                        |
| ------------ | ------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| Language, UI | Kotlin, Jetpack Compose, Material 3.                                                                                                                                                                                                                          |
| Playback     | Media3 ExoPlayer in a `MediaLibraryService`. The same service gives the notification, lock screen, Bluetooth and headset controls now, and **Android Auto later with no rework**. Gapless playback, and speed and pitch (`PlaybackParameters`), are built in. |
| Local data   | Room. It holds the catalogue copy, the user-data working copy, the outbox, and the download index ([006](006_api_and_sync.md)).                                                                                                                               |
| Network      | OkHttp plus kotlinx.serialization. Media3's OkHttp data source shares the same client, so auth is one interceptor.                                                                                                                                            |
| Background   | Sync runs in-process, with retries and on reconnect: WorkManager would bring a second copy of Room ([011](011_android_app.md)). Downloads run in a `dataSync` foreground service ([012](012_downloads.md)).                                                   |
| DI           | Manual: one `AppGraph` object. Hilt is not worth it for a single-module app.                                                                                                                                                                                  |
| Modules      | **One `app` module.** Split only when a second consumer appears (for example, an Android Auto or Wear module).                                                                                                                                                |
| `minSdk`     | 31 (Android 12), the owner's choice (2026-10-07; first 29), and `targetSdk` the latest.                                                                                                                                                                       |

**Multiple queues on one player.** Media3's player holds one playlist. The
app's `QueueManager` owns all 20 queues (in Room, mirrored from the server),
and loads **only the active queue** into the player. Switching queues saves
the current song and position into the outgoing queue, then loads the
incoming one at its saved song and position. Gapless playback works within a
queue, which is where it matters. "End of queue → jump to the next queue"
(Musicolet, after v1) is the same switch, triggered on completion.

**Where audio comes from.** Each song resolves, in order, to a **Download**,
then the **Cache** (built in [019](019_networking_and_caching.md), read
while it is still arriving), then the **stream**. This happens in a data
source of our own (`SongSource`), so the player only ever sees a song ID.

**Downloads and Cache are plain files, not Media3's `SimpleCache`.**
`SimpleCache` stores byte ranges in opaque span files, and two caches cannot
share files. That makes "a cached song becomes a download without fetching it
again" impossible. Instead, both are folders of original files under the
app's external storage. The owner's phone has an SD card, and the app's own
folder on it needs no permission. Room tracks their sizes and quotas.
Promoting a cached song to Downloads is a **file rename**, as in Ultrasonic
([INSPIRATIONS](../INSPIRATIONS.md)). Downloads are fetched in full by a
foreground service, never by the player ([012](012_downloads.md)).

**Phone-local songs** come from MediaStore. They are a separate source with
their own IDs, which are never sent to the server ([REQUIREMENTS](../REQUIREMENTS.md#phone-local-songs)).

**Lyrics:** `.lrc` and embedded lyrics are parsed on the device by a small
parser of our own. Synced lines are highlighted from the player's position.

**What the server already expects from the app** (plans 008–010):

- **One `play` per listen**, skips included, built from the player's events.
  The open listen is saved in Room as it plays, so a killed app closes it as
  `interrupted` on the next start ([008](008_listening_history.md)).
- **Resume points** for files of at least `longFiles.minMinutes`: saved on
  pause, on switching away and every 30 s, with one `resume.set` per song
  kept in the outbox, and cleared at the end ([009](009_resume_long_files.md)).
- **Favorites and Listen Later** pinned above the playlists. **Continue
  listening, Recently added, Recently played, Most played and Not played
  lately** are computed from Room ([010](010_special_playlists.md)).
- **Synced settings**, read from every pull and written with `setting.set`.

## macOS

SwiftUI, with AVFoundation (`AVQueuePlayer`) for playback and `URLSession` for
the API. A media-key and Now Playing integration through
`MPRemoteCommandCenter`. Online-only, no database: it fetches the catalogue
into memory at launch (about 2 MB). No third-party dependencies.

## Web

Plain HTML, CSS and JavaScript ES modules: no framework and no build step.
The files are **embedded in the Go binary** (`embed`) and served by the
backend from the same origin, so the cookie auth works and there is nothing
extra to deploy. The admin panel lives here, where a large screen suits
reviewing an inbox; its curation screens ([004](004_curation_workflow.md))
follow after v1. Its first screen was to be **Users**, in v1. That moved to
the Android app (2026-10-08, [023](023_users_on_android.md)): a short list
and a form need no large screen, and it should not wait on this client.

## Repo layout

```
server/          Go module: the backend
server/web/      the web client (inside the Go module, so `embed` can reach it)
android/         Gradle project
macos/           Xcode project
api/openapi.yaml the contract
deploy/          docker-compose.yml, Dockerfile
docs/
```

Each toolchain gets `fmt`, `lint` and `test` targets in the root `Makefile`
as its code lands. The pre-commit hook runs the formatter only for staged
files of that language.

## Rejected

- **KMP or Compose Multiplatform** (option A above). Compose Desktop would
  also bundle a JVM, about 100 MB, against the lightweight goal.
- **A React, Svelte or Vue web client:** a build toolchain for a minimal
  page.
- **Media3 `SimpleCache` for offline storage:** cannot promote a cached song
  to a download without fetching it again.
- **Hilt and multi-module Android from day one:** structure with no second
  consumer yet.

## Open questions

None.
