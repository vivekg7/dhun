# 024 — The macOS app, at the Android app's level

**Status:** `ACCEPTED` — no code yet; the playback spike comes first
**Started:** 2026-10-08

## Problem

The owner wants a Mac app first, before phone-local songs and the web
client. It should have **everything the Android app has** while staying as
light as the Android app (owner, 2026-10-08). That reverses two earlier
decisions:

- REQUIREMENTS listed macOS as "minimal features, online-only".
- [007](007_client_architecture.md) built that into the design: a Mac app
  with no database and no outbox. That was the reason for having no shared
  core with Android, because the hard offline logic was needed only on
  Android.

The owner answered four questions about behaviour (2026-10-08):

| Question         | Answer                                                                                                                                         |
| ---------------- | ---------------------------------------------------------------------------------------------------------------------------------------------- |
| Offline          | **Full offline, as Android:** Downloads, the play cache, and an offline working copy whose edits are pushed on reconnect.                      |
| Layout           | **A native Mac window with a sidebar**, not the phone's tabs. It has the same features in Mac form.                                            |
| Mini player      | **Both a menu bar control and a floating mini window**, the Mac's counterparts of the phone's bar and pill ([022](022_mini_player_styles.md)). |
| Files on the Mac | **Open from Finder** only, as [021](021_open_from_other_apps.md) does on the phone. **No Mac-local library**: the Mac plays NAS songs.         |

## What carries over, and what does not

The Android app is about 9,300 lines of Kotlin. Most of it is logic that
carries over to Swift as it stands. **Its behaviour is the specification**:
where this plan is silent, the Mac does what the phone does, for the reason
the phone's plan gives.

| Android                                                                                                      | Mac                                                                                       |
| ------------------------------------------------------------------------------------------------------------ | ----------------------------------------------------------------------------------------- |
| Room: songs, playlists, queues, marks, resume, settings, outbox, play stats, pins, downloads, lyrics, thumbs | The same tables in SQLite                                                                 |
| `Store` (local write + outbox op), `Sync` (pull library, push/pull loop, back-off)                           | Ported unchanged in behaviour                                                             |
| `Playback`: 20 queues, the active one in the player, listens, resume points, hand-off                        | Ported; the player underneath is different (below)                                        |
| `SongSource`: download → cache → growing `.part` → stream                                                    | The same order                                                                            |
| Downloads (pins), song cache with prefetch, covers                                                           | The same, in `~/Library/Application Support/Dhun/`                                        |
| Sleep timer, speed and pitch, lyrics and the LRC parser                                                      | The same                                                                                  |
| Tabs, mini bar or floating pill, the ⋮ menu                                                                  | A sidebar window, a menu bar control and a floating mini window; the app's menus          |
| Media session, notification, Bluetooth and headset controls                                                  | `MPRemoteCommandCenter` and `MPNowPlayingInfoCenter`: media keys, Control Center, AirPods |
| Pause when headphones are unplugged                                                                          | Pause when the output device changes                                                      |
| `PlayFileActivity` (open from other apps)                                                                    | A Finder **Open With** window                                                             |
| Family members (admin only)                                                                                  | The same, under Settings → Account                                                        |
| Light, dark, black and by-time themes; eight palettes                                                        | Follows the system's appearance, with the eight palettes as the accent colour             |
| Wi-Fi only for downloads; prefetch 2 on mobile data, 10 on Wi-Fi                                             | "Expensive network" (a phone hotspot) in place of mobile data                             |
| Phone-local songs                                                                                            | None (owner, 2026-10-08)                                                                  |

The Black and by-time themes are left out because they solve phone
problems. Black saves power on OLED screens, and macOS already has an
Automatic appearance.

**Parity is a standing commitment.** From now on, a plan for an Android
feature also says what the Mac does, or why the Mac leaves it out.

## Options

### Shared code with Android

- **A. Kotlin Multiplatform core.** Still rejected, for the reasons in 007.
  The Mac app would carry a Kotlin/Native runtime and a second toolchain,
  and Swift could reach it only through a weak bridge with no `async`.
  Being lightweight is part of what the owner asked for.
- **B. A native Swift port, with the API as the only contract.** Chosen.
  The logic is written twice. That is what 007 accepted for the clients in
  general, and agents make the second copy cheap. The risk is the two
  drifting apart. Three things limit it: the server holds the truth, the
  ops and their merge rules are fixed by [006](006_api_and_sync.md), and
  the Android unit tests are ported along with the code they pin.

### Local storage

- **SQLite through the system's `libsqlite3`** (`import SQLite3`), with a
  wrapper of about 150 lines of our own. Chosen. It needs no dependency
  and has the same tables as Room, so the Kotlin `Store` and `Sync` port
  line by line.
- **SwiftData or Core Data.** Rejected. Their models and migrations are
  framework magic, so a schema that mirrors Room's becomes a translation
  rather than a copy. Neither has anything that 7,000 rows need.
- **GRDB.** Rejected: a well-made package, but a dependency for what the
  standard library already does.

### Playback

The player has to play **gapless**, change **speed and pitch
independently** (±6 semitones; [016](016_speed_and_pitch.md)), play a
song **while it is still arriving** in the cache (019), and play every
format the server indexes, including **Opus**.

**What the owner's `.opus` files really are** (checked 2026-10-08 on a
sample copied from the phone's SD card). Of 30 `.opus` files, 16 are Ogg
Opus and 14 are **WebM (Matroska) with Opus inside**, saved with an
`.opus` name, as YouTube downloaders do. The card holds 1,267 `.opus`
files, so several hundred songs are WebM. On macOS 27,
`AudioFileOpenURL` opens the Ogg ones (`Oggf`, Opus at 48 kHz). It fails
on the WebM ones. No Vorbis was found. FLAC (16-bit, 44.1 kHz, from the
card's `Sound/` folder) opens natively.

- **A. `AVQueuePlayer`** (what 007 assumed). Rejected. It can keep the
  pitch while changing speed, but it cannot shift the pitch on its own. It
  plays queued items close together but not reliably gapless. It cannot
  open WebM.
- **B. `AVAudioEngine`: `AVAudioPlayerNode` → `AVAudioUnitTimePitch` →
  output, fed by our own decoder.** Chosen. The time-pitch unit gives speed
  and pitch independently, which is what Media3 gives the phone.
  - **Decoding.** `AudioFileStream` parses the bytes as they arrive, from
    a downloaded file, a cached or still-arriving `.part` file, or the HTTP
    stream, and `AVAudioConverter` decodes them to PCM. The same path
    serves all four sources.
  - **Gapless.** The next song's buffers are scheduled straight after the
    current song's. The encoder delay and padding that MP3 (LAME header)
    and AAC report are trimmed.
  - **Seeking.** `AudioFileStreamSeek` maps a time to a byte offset, and
    reading restarts there: from the file, or with a new `Range` request.
  - **Ogg Opus.** macOS reads it. The spike checks whether
    `AudioFileStream` parses it too, as `AudioFile` does. If it does not, a
    small Ogg page reader of our own feeds the packets to the decoder.
  - **WebM Opus.** Nothing in macOS reads WebM, so a small WebM reader of
    our own does. It reads the EBML header, the codec's private data
    (`OpusHead`) and the `SimpleBlock`s of the one audio track, and feeds
    the Opus packets to Apple's Opus decoder. It decides by the file's
    first bytes, never by its name.
  - **Ogg Vorbis.** Apple has no Vorbis decoder. None was found. If some
    turn up, the owner decides what happens to them.
- **C. Play only finished files with `AVAudioFile`, and wait for the cache
  to fill.** Kept as a fallback, if B's streaming decoder proves to be more
  code than it is worth. A normal song arrives in about a second at home.
  An audiobook would wait much longer, which is why it is not the choice.

### Project and build

- **A Swift package (`macos/Package.swift`).** Chosen. One executable
  target for the app and one library target for the logic, which
  `swift test` can test without a window. `make mac` builds it in release
  mode, assembles `Dhun.app` (binary, `Info.plist` with the Finder document
  types, and an `.icns` made by `iconutil`), signs it ad hoc and archives a
  zip in `local/`, as `make apk` does. It is plain text that agents and
  reviewers can read, and it builds from the command line with no IDE.
- **An `.xcodeproj`.** Rejected. Its project file is opaque, conflicts in
  every branch, and is meant to be edited only by Xcode. 007 had named an
  Xcode project; this replaces it.
- **XcodeGen or Tuist.** Rejected: a dependency in order to generate the
  file we do not want.

## Decision

A native SwiftUI app in `macos/`, at the Android app's level and offline,
built as described above. Technical defaults, which the owner can override:

- **macOS 14 (Sonoma) and newer.** It is the first release with the
  Observation framework (`@Observable`). The inspector and `MenuBarExtra`
  windows are there too. The owner runs macOS 27; 14 leaves room for older
  family Macs.
- **The window.**
  - A `NavigationSplitView` sidebar with Queues; the library (Folders,
    Albums, Artists, Genres); Favorites, Listen Later and the automatic
    views (010); the user's playlists; and Downloads.
  - The list in the middle has a search field. Lists support multiple
    selection and drag to reorder.
  - A Now playing bar runs along the bottom: cover, title, controls, seek,
    favourite, speed, sleep and the lyrics toggle. The hand-off bar sits
    above it.
  - An inspector on the right shows the playing queue or the lyrics.
  - Keyboard: Space plays or pauses, ⌘← and ⌘→ go to the previous and next
    song, and ⌘F searches. A **Controls** menu holds the same commands.
- **Closing the window does not stop the music.** The app keeps playing,
  and keeps syncing, from the menu bar control, as Music does. Quitting is
  ⌘Q.
- **The menu bar control** (`MenuBarExtra`, window style) shows the cover,
  title, controls, seek and the queue picker.
- **The floating mini window** is a small window that stays on top of
  other windows, shown from the Window menu. Its position is remembered.
- **Open With from Finder.** The app declares the audio types as an
  alternate handler. The file plays in a small window of its own, through a
  second engine. It makes no queue and logs no listen; the playing queue
  pauses where it is ([021](021_open_from_other_apps.md)).
- **The sign-in token** goes in the Keychain, not `UserDefaults`, where any
  process of the user could read it. The device name sent at sign-in is the
  Mac's name (`Host.current().localizedName`).
- **Storage.** Downloads, the cache and covers live under
  `~/Library/Application Support/Dhun/`. All three are on one volume so
  that promoting a song is a rename, as on the phone. The defaults are the
  phone's: a 10 GB download limit, a 3 GB cache, and downloads not on an
  expensive network.
- **When sync runs.** As on the phone: after every edit (2 s debounce), on
  becoming active, when the network returns (`NWPathMonitor`), on waking
  from sleep, and from **Sync now**.
- **Versioning.** The Mac app is versioned on its own, starting at 0.1.0
  and tagged `macos-vX.Y.Z`. It is signed ad hoc. With no Developer ID, a
  family Mac allows it once under System Settings → Privacy & Security. A
  paid Developer ID and notarization can come later, with no code change.
- **Tooling.** `swift format` from the Xcode toolchain, so no new tool
  joins. `make fmt`, `lint-macos` and `test-macos` join the `Makefile`. A
  CI job runs on a macOS runner.

## Plan

Each step ends usable and is installed on the owner's Mac. The status line
moves with it.

0. **Playback spike.** This is the part that can fail, so it goes first.
   It plays MP3, AAC/M4A, FLAC, Ogg Opus and WebM Opus, from a file and from
   the stream, and must show four things:
   - gapless playback between consecutive tracks of an album, in MP3 and
     in AAC;
   - speed and pitch changed independently while playing;
   - seeking into a range that has not arrived yet;
   - a still-arriving `.part` file playing as it grows.

   The test files are in `~/Music` on the owner's Mac: a film-score album
   in MP3 for gapless playback, long files, FLAC, and Opus in both
   containers.
   No AAC album is among them yet. If B fails here, the plan takes C and
   says so.

1. **Skeleton.** The package, `make mac`, the Makefile targets and CI.
   Sign-in, the Keychain, the SQLite schema, the library pull, the catalogue
   (albums, artists, genres, the folder tree, search and natural sort), and
   browsing with covers. Ported tests: `CatalogTest`.
2. **Playing.** The engine from step 0 under `Playback`. The 20 queues,
   the queue inspector and picker, listens, resume points (009), hand-off
   (017), media keys and Control Center, and pause on an output change.
   **Release 0.1.0**, the first install.
3. **Offline.** The outbox and the push/pull loop with back-off. A revoked
   token keeps the unsent edits, as on Android. The play cache with prefetch, then Downloads
   and pins with promotion by rename. Ported tests: `QueueOrderTest`,
   `DownloadsTest`, `SongCacheTest`.
4. **The rest of the phone.**
   - Playlist editing (013), Favorites, Listen Later and the automatic
     views (010).
   - Lyrics (015; ported `LyricsTest`).
   - Speed and pitch (016; ported `TempoTest`).
   - The sleep timer (014).
   - Settings and the eight palettes.
   - Family members (023).
5. **Mac-only pieces.** The menu bar control, the floating mini window and
   Open With from Finder.

## Open questions

None for the owner. Ogg Vorbis comes back as a question only if Vorbis
files turn up.
