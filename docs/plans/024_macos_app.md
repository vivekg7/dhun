# 024 — The macOS app, at the Android app's level

**Status:** `RELEASED` — in the 0.13.0 GitHub release (2026-10-12); 0.1.0 installed on the owner's Mac (2026-10-08) and in daily use
**Started:** 2026-10-08

## Problem

The owner wants a Mac app first, before phone-local songs and the web
client, with **everything the Android app has** and as light (owner,
2026-10-08). That reverses two earlier decisions:

- REQUIREMENTS listed macOS as "minimal features, online-only".
- [007](007_client_architecture.md) built that in: a Mac app with no
  database and no outbox, which was why there is no core shared with
  Android, as the hard offline logic was needed only there.

The owner answered four questions about behaviour (2026-10-08):

| Question         | Answer                                                                                                                                         |
| ---------------- | ---------------------------------------------------------------------------------------------------------------------------------------------- |
| Offline          | **Full offline, as Android:** Downloads, the play cache, and an offline working copy whose edits are pushed on reconnect.                      |
| Layout           | **A native Mac window with a sidebar**, not the phone's tabs. It has the same features in Mac form.                                            |
| Mini player      | **Both a menu bar control and a floating mini window**, the Mac's counterparts of the phone's bar and pill ([022](022_mini_player_styles.md)). |
| Files on the Mac | **Open from Finder** only, as [021](021_open_from_other_apps.md) does on the phone. **No Mac-local library**: the Mac plays NAS songs.         |

## What carries over, and what does not

The Android app was about 9,300 lines of Kotlin, most of it logic that
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

The Black and by-time themes are left out as phone problems: Black saves
power on OLED screens, and macOS already has an Automatic appearance.

**Parity is a standing commitment.** From now on, a plan for an Android
feature also says what the Mac does, or why the Mac leaves it out.

## Options

### Shared code with Android

- **A. Kotlin Multiplatform core.** Still rejected, for the reasons in 007:
  the Mac app would carry a Kotlin/Native runtime and a second toolchain,
  and Swift could reach it only through a weak bridge with no `async`.
  Being lightweight is part of what the owner asked for.
- **B. A native Swift port, with the API as the only contract.** Chosen.
  The logic is written twice, as 007 accepted for the clients in general,
  and agents make the second copy cheap. The risk is drift, limited by
  three things: the server holds the truth, [006](006_api_and_sync.md)
  fixes the ops and their merge rules, and the Android unit tests are
  ported with the code they pin.

### Local storage

- **SQLite through the system's `libsqlite3`** (`import SQLite3`), with a
  wrapper of about 150 lines of our own. Chosen: no dependency, and the
  same tables as Room, so the Kotlin `Store` and `Sync` port line by line.
- **SwiftData or Core Data.** Rejected. Their models and migrations are
  framework magic, so mirroring Room's schema becomes a translation rather
  than a copy, and neither offers anything a family library needs.
- **GRDB.** Rejected: a well-made package, but a dependency for what the
  standard library already does.

### Playback

The player has to play **gapless**, change **speed and pitch
independently** (±6 semitones; [016](016_speed_and_pitch.md)), play a
song **while it is still arriving** in the cache (019), and play every
format the server indexes, including **Opus**.

**What the owner's `.opus` files really are** (checked 2026-10-08 on a
sample from the phone's SD card): of 30, 16 are Ogg Opus and 14 are **WebM
(Matroska) with Opus inside**, saved as `.opus` as YouTube downloaders do.
The card holds 1,267 `.opus` files, so several hundred songs are WebM. On
macOS 27, `AudioFileOpenURL` opens the Ogg ones (`Oggf`, Opus at 48 kHz)
and fails on the WebM ones. No Vorbis was found. FLAC (16-bit, 44.1 kHz)
opens natively.

- **A. `AVQueuePlayer`** (what 007 assumed). Rejected: it can keep the
  pitch while changing speed but cannot shift it on its own, plays queued
  items close together but not reliably gapless, and cannot open WebM.
- **B. `AVAudioEngine`: `AVAudioPlayerNode` → `AVAudioUnitTimePitch` →
  output, fed by our own decoder.** Chosen. The time-pitch unit gives speed
  and pitch independently, as Media3 does on the phone.
  - **Decoding.** Every file is opened through **our own read callbacks**
    (`AudioFileOpenWithCallbacks`, then `ExtAudioFile`), reading from a
    downloaded file, a cached file, or a cache file still arriving
    (`SparseFile`). A read past what has arrived waits, and tells the
    fetcher (`Fetch`) where. The fetcher fills the file from the stream
    endpoint with `Range` requests, and jumps when a reader waits behind it
    or more than 1 MB ahead, as the phone does (019). One path serves all
    four sources.
  - **Rejected during the spike: `AudioFileStream`**, the first plan. It
    parses bytes only in order, so an MP4 whose index (`moov`) follows the
    audio cannot play until the whole file has arrived, and many of the
    library's `.m4a` files are like that. With callbacks, Core Audio reads
    the index at the end through a `Range` jump, seeks by itself, and
    applies each format's encoder delay and padding.
  - **Gapless.** The next song's buffers are scheduled straight after the
    current song's on one player node. `ExtAudioFile` trims what MP3 (LAME
    header), AAC, FLAC and Ogg Opus declare.
  - **MP3 still arriving.** Core Audio walks every frame before a place
    to find it, so a seek far into a file still arriving decodes a slice
    from an estimated place instead, as Media3 does (`Mp3.swift`,
    [031](031_podcasts_and_audiobooks.md)).
  - **Ogg Opus** needs nothing of ours: Core Audio reads it.
  - **WebM Opus.** Nothing in macOS reads WebM, so a small reader of our
    own does (`WebM.swift`): the EBML header, the codec's private data
    (`OpusHead`), the one audio track's blocks, `DiscardPadding` and the
    `Cues`, feeding the Opus packets to Apple's Opus decoder. It decides by
    the file's first bytes, never its name. **Apple's Opus decoder trims
    part of the pre-skip itself** (120 samples on macOS 27, whatever
    `primeMethod` says), so the reader measures the first packet's
    shortfall against its header's length and drops only the rest.
  - **Ogg Vorbis.** Apple has no Vorbis decoder, and none was found. If
    some turn up, the owner decides.

**What the spike showed** (2026-10-08, macOS 27, `macos/` and its
`dhun-play` harness):

- **Exact length in every format.** MP3, AAC (index at either end), FLAC,
  Ogg Opus and WebM Opus decode to exactly the length the file declares.
- **Gapless, sample-exact.** A tone cut at a sample no codec frame lines up
  with, each half encoded on its own, joins without a jump in AAC, FLAC
  (unit tests), MP3, Ogg Opus and WebM Opus; cross-correlation with the
  source puts every format at offset 0.
- **Speed and pitch.** At 2×, two seconds of song pass per second, and
  pitch moves separately.
- **Starting and seeking.** Playback starts in about 30 ms from a file or
  a local server's stream, and a seek lands where asked.
- **Slow arrival.** A 30 MB WebM jukebox arriving at 64 KB/s seeks to
  15:00, and an M4A with its index at the end starts in 1.5 s at
  200 KB/s.

**What step 2 must respect.** Opening and seeking can wait on the
network, so `Engine.play` and `seek` are never called on the main thread;
`Playback` calls them from its own queue, as the phone's player runs off
the UI thread.

**What the spike found on the server.** It indexed WebM `.opus` files with
no format and a duration of 0, as its tag reader goes by the extension,
and every client needs the duration: for the seek bar, long-file resume
(009) and the half-heard rule for play counts (008). Fixed separately: the
scanner reads Matroska with an EBML parser of its own, deciding by the
first bytes, and re-reads the songs it indexed wrongly
([005](005_storage_and_library_model.md)). In `server-v0.1.5`
(2026-10-08), all 24 WebM songs on the NAS had a format and a duration.

- **C. Play only finished files with `AVAudioFile`, waiting for the cache
  to fill.** Rejected once B worked: a song arrives in about a second at
  home, but an audiobook would wait much longer.

### Project and build

- **A Swift package (`macos/Package.swift`).** Chosen. One executable
  target for the app and one library target for the logic, which
  `swift test` tests without a window. `make mac` builds it in release
  mode, assembles `Dhun.app` (binary, `Info.plist` with the Finder document
  types, an `.icns` made by `iconutil`), signs it ad hoc and archives a zip
  in `local/`, as `make apk` does. It is plain text agents and reviewers
  can read, built from the command line with no IDE.
- **An `.xcodeproj`**, which 007 had named. Rejected: its project file is
  opaque, conflicts in every branch, and is meant to be edited only by
  Xcode.
- **XcodeGen or Tuist.** Rejected: a dependency in order to generate the
  file we do not want.

## Decision

A native SwiftUI app in `macos/`, at the Android app's level and offline,
built as described above. Technical defaults, which the owner can override:

- **macOS 26 and newer** (owner, 2026-10-08: no older Macs to support).
  The first plan said 14, for older family Macs; on the current release the
  app uses SwiftUI as it is now (a floating window level, table columns
  shown only on some lists) with no workarounds.
- **The window.**
  - A `NavigationSplitView` sidebar with Queues; the library (Folders,
    Albums, Artists, Genres); Favorites, Listen Later and the automatic
    views (010); the user's playlists; and Downloads.
  - The search field finds songs, artists, albums, genres, folders,
    playlists and lyrics, as the phone's Search tab does, and the tabs'
    lists and every page past 12 rows have a filter that matches the same
    way ([029](029_search.md)). Lists support multiple selection and drag
    to reorder.
  - A Now playing bar along the bottom: cover, title, controls, seek,
    favourite, speed, sleep and the lyrics toggle, with the hand-off bar
    above it.
  - A panel on the right shows the playing queue or the lyrics; not
    SwiftUI's inspector, which crashed the app
    ([028](028_mac_crash_and_polish.md)).
  - Keyboard: Space plays or pauses, ⌘← and ⌘→ go to the previous and next
    song, and ⌘F searches. A **Controls** menu holds the same commands.
- **Closing the window does not stop the music.** The app keeps playing
  and syncing from the menu bar control, as Music does; quitting is ⌘Q.
- **The menu bar control** (`MenuBarExtra`, window style) shows the cover,
  title, controls, seek and the queue picker.
- **The floating mini window** stays on top of other windows, is shown
  from the Window menu, and remembers its position. It has no title bar,
  which took a strip as tall as the player: it is moved by dragging
  anywhere on it, and closed by a button over the cover while the pointer
  is on it ([028](028_mac_crash_and_polish.md)).
- **Open With from Finder.** The app declares the audio types as an
  alternate handler. The file plays in a small window of its own, through a
  second engine, making no queue and logging no listen; the playing queue
  pauses where it is ([021](021_open_from_other_apps.md)).
- **The sign-in token** is kept in a file only the user can read (mode 0600) in the app's data folder, as the phone keeps it in app-private
  storage. The Keychain, the first choice, was rejected when the app first
  ran (2026-10-08): its access is tied to the code signature, an ad-hoc
  signature changes with every build, so each update would ask for the
  login password. The device name sent at sign-in is the Mac's
  (`Host.current().localizedName`).
- **Storage.** Downloads, the cache and covers live under
  `~/Library/Application Support/Dhun/`, on one volume so that promoting a
  song is a rename, as on the phone. The defaults are the phone's: a 10 GB
  download limit, a 3 GB cache, and no downloads on an expensive network.
- **When sync runs.** As on the phone: after every edit (2 s debounce), on
  becoming active, when the network returns (`NWPathMonitor`), on waking
  from sleep, and from **Sync now**.
- **Versioning.** The version is in `macos/Info.plist`, and `make mac`
  builds it. The app started at 0.1.0, counting on its own; since 0.12.0 it
  shares one sequence with the server and the other apps
  ([README](../../README.md#versions)).
- **Signing is ad hoc only** (`codesign --sign -`): no Apple account, team
  or certificate. The Apple account in Xcode on the owner's Mac belongs to
  the owner's employer, and Dhun is a personal project, so it must never be
  signed with it (owner, 2026-10-08). With no Developer ID, a family Mac
  allows the app once under System Settings → Privacy & Security. A
  Developer ID and notarization could come later, only on an account of
  the owner's own, with no code change.
- **Tooling.** `swift format` from the Xcode toolchain, so no new tool.
  `make fmt`, `lint-macos` and `test-macos` join the `Makefile`, and a CI
  job runs on a macOS runner.

## Plan

Each step ends usable and is installed on the owner's Mac.

0. **Playback spike. Done (2026-10-08).** MP3, AAC/M4A, FLAC, Ogg Opus and
   WebM Opus, from a file, a still-arriving file and the stream: option B
   held (results above). Its engine is the app's,
   `macos/Sources/DhunKit/Play/`. Its `dhun-play` harness was the test
   bench for engine changes until plan 036 removed it, once the app had
   played music for months. The test music is in `~/Music` on the owner's
   Mac.
1. **Skeleton.** `make mac` and CI (`lint-macos` and `test-macos` came with
   step 0). Sign-in, the token file, the SQLite schema, the library pull,
   the catalogue (albums, artists, genres, the folder tree, search and
   natural sort), and browsing with covers. Ported tests: `CatalogTest`.
2. **Playing.** The engine from step 0 under `Playback`. The 20 queues,
   the queue inspector and picker, listens, resume points (009), hand-off
   (017), media keys and Control Center, and pause on an output change.
   **Release 0.1.0**, the first install.
3. **Offline.** The outbox and the push/pull loop with back-off; a revoked
   token keeps the unsent edits, as on Android. The play cache with
   prefetch, then Downloads and pins with promotion by rename. Ported
   tests: `QueueOrderTest`, `DownloadsTest`; `SongCacheTest`'s waiting
   reader is `SparseFileTests`.
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

## How the build went (2026-10-08)

Steps 1 to 5 were built in one go after the spike, rather than one
install at a time.

**Checked:**

- The Swift tests, including the phone's catalogue, lyrics, tempo,
  queue-order and downloads tests, ported.
- A throwaway end-to-end run of the real `AppModel` against the server on
  the owner's Mac: it signed in, synced 194 songs, played an album, skipped
  and seeked, marked a favourite, and the outbox emptied on the next sync.
- The window, signed in, showing the albums with their covers.

**Checked by hand afterwards** (2026-10-08), in the installed app signed in
to the owner's NAS: every sidebar section and all 22 playlists; play,
pause, skip, seek; speed (2× plays two seconds a second); the sleep timer
at the end of a song; search; synced lyrics; hand-off from the phone, in
the window and the menu bar control; an album downloaded and removed; the
menu bar control; the mini window, whose place survives a relaunch; Open
With from Finder; Settings.

That pass, and a read of the phone's code against the Mac's, found these,
all fixed:

- **Crashes.** Switching between lists, and opening Queues with the
  inspector open, aborted the app. The first was SwiftUI on macOS 26
  rebuilding a reused row before its environment is there, so rows take
  the model as a value. The second looked like an AppKit layout loop with
  two lists side by side, so Queues became a list of queues, each opening
  as a page as on the phone. The loop was SwiftUI's inspector and still
  struck on other pages, until 028 replaced it.
- **Album downloads never started.** An album's id holds a NUL; the SQLite
  layer bound text as a C string and cut it there. Text is now bound by
  length.
- **Paused showed 0:00**, and saved it as the queue's place: a paused
  player node has no time, so the engine keeps the last one.
- **Play did nothing** after a relaunch when the sleep timer had stopped
  the last song at its very end; such a song now starts over.
- **Keys.** Space did nothing while a list had the focus, and ⌘F did not
  reach the search field. Space and ⌘← ⌘→ are now caught before the
  focused view, except while typing.
- **Where the Mac differed from the phone:** shared playlists were
  editable by the admin (plan 013 keeps them read-only for everyone); a
  search queue was named with the bare query and could refill an album's
  queue of that name; several Finder files played at once; the speed
  slider wrote a synced op on every step of a drag.
- **What the phone has that the Mac lacked:** the downloaded mark on songs
  and on downloaded albums, folders, artists and genres; the Downloads
  page's songs; a download's progress ("3/5") and a question before one
  is removed; the note after adding to a playlist; reserved names; a
  question before a queue is removed; a filter in a queue and its place
  ("3 / 40 · 1:02 left of 2:30"); "Don't stop after this song"; Listen
  Later in Now playing; artists and genres in album order; a header for a
  folder holding only folders; the lyrics holding still after a scroll and
  keeping the display awake; the sleep timer in Control Center; a family
  member's password shown as typed, and when they were last seen.

**Changes from the plan above:**

- The token moved from the Keychain to a file (see the decision above).
- macOS 26 replaced 14, by the owner's choice.
- The song cache drops a part file left by an earlier run, as a part
  filled out of order (a seek, an index at the end) knew only in memory
  which bytes had arrived. Downloads are written in order, so they still
  resume.
- **The engine takes bytes, not decoders.** It opens each song itself, so
  it can interrupt a read waiting for the network, and a skip or seek never
  waits for bytes that may not come.
- **Two bugs the first runs found:**
  - The Now Playing artwork closure was inferred as main-actor and trapped
    when MediaPlayer called it on its own queue.
  - Sync's debounce cancelled a sync already under way, and with it its
    requests.

## Open questions

None for the owner. Ogg Vorbis comes back as a question only if Vorbis
files turn up.
