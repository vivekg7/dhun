# 024 — The macOS app, at the Android app's level

**Status:** `IN PROGRESS` — every step built and installed on the owner's Mac (2026-10-08); in daily use, not yet released
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
  - **Decoding.** Every file is opened through **our own read
    callbacks** (`AudioFileOpenWithCallbacks`, then `ExtAudioFile`). The
    callbacks read from a downloaded file, a cached file, or a cache file
    still arriving (`SparseFile`). A read past what has arrived waits for
    it, and tells the fetcher where it waits. The fetcher (`Fetch`) fills
    the file from the stream endpoint with `Range` requests. It jumps when
    a reader waits behind it or more than 1 MB ahead, as the phone does
    (019). So one path serves all four sources.
  - **Rejected during the spike: `AudioFileStream`.** It was planned
    here. It parses bytes only in order, so an MP4 whose index (`moov`)
    comes after the audio cannot play until the whole file has arrived.
    Many of the library's `.m4a` files are like that, including a whole
    album. With callbacks, Core Audio reads the index at the end through a
    `Range` jump. It also seeks by itself and applies each format's encoder
    delay and padding.
  - **Gapless.** The next song's buffers are scheduled straight after the
    current song's on one player node. `ExtAudioFile` trims what MP3 (LAME
    header), AAC, FLAC and Ogg Opus declare.
  - **Ogg Opus** needs nothing of ours: Core Audio reads it.
  - **WebM Opus.** Nothing in macOS reads WebM, so a small WebM reader of
    our own does (`WebM.swift`, about 300 lines). It reads the EBML header,
    the codec's private data (`OpusHead`), the blocks of the one audio
    track, `DiscardPadding` and the `Cues`. It feeds the Opus packets to
    Apple's Opus decoder. It decides by the file's first bytes, never by
    its name. **Apple's Opus decoder trims part of the pre-skip itself**:
    120 samples on macOS 27, whatever `primeMethod` says. The reader
    therefore measures the first packet's shortfall against the length its
    header gives, and drops only the rest.
  - **Ogg Vorbis.** Apple has no Vorbis decoder. None was found. If some
    turn up, the owner decides what happens to them.

**What the spike showed** (2026-10-08, macOS 27, `macos/` and its
`dhun-play` harness):

- **Exact length in every format.** MP3, AAC (with the index at either
  end), FLAC, Ogg Opus and WebM Opus decode to exactly the length the
  file declares.
- **Gapless, sample-exact.** One tone was cut at a sample no codec frame
  lines up with, and each half was encoded on its own. Decoded back to
  back, the halves join without a jump in AAC, FLAC (unit tests), MP3,
  Ogg Opus and WebM Opus. Cross-correlation with the source puts every
  format at offset 0.
- **Speed and pitch.** At 2×, two seconds of song pass per second, and
  pitch moves separately.
- **Starting and seeking.** Playback starts in about 30 ms from a file or
  a local server's stream. A seek lands where asked.
- **Slow arrival.** A 30 MB WebM jukebox arriving at 64 KB/s seeks to
  15:00, and an M4A with its index at the end starts in 1.5 s at
  200 KB/s.

**What step 2 must respect.** Opening and seeking can wait on the
network, so `Engine.play` and `seek` must never be called on the main
thread. `Playback` calls them from its own queue, as the phone's player
runs off the UI thread.

**What the spike found on the server.** It indexes WebM `.opus` files
with no format and a duration of 0, because its tag reader goes by the
extension. Every client needs the duration: for the seek bar, long-file
resume (009) and the half-heard rule for play counts (008). Fixed on the
server, separately from this plan: the scanner reads Matroska with an EBML
parser of its own, deciding by the first bytes, and re-reads the songs it
indexed wrongly ([005](005_storage_and_library_model.md)).

- **C. Play only finished files with `AVAudioFile`, and wait for the cache
  to fill.** Rejected once B worked. A normal song arrives in about a second
  at home, but an audiobook would wait much longer.

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

- **macOS 26 and newer** (owner, 2026-10-08: no older Macs to support).
  The first plan said 14, for older family Macs; the owner chose the
  current release instead, so the app uses SwiftUI as it is now (a
  floating window level, table columns shown only on some lists) with no
  workarounds for older systems.
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
- **The sign-in token** is kept in a file that only the user can read
  (mode 0600), in the app's data folder, as the phone keeps it in its
  app-private storage. The Keychain was the first choice, and it was
  rejected when the app first ran (2026-10-08). Keychain access is tied to
  the code signature, and an ad-hoc signature changes with every build, so
  each update would ask for the login password. The device name sent at
  sign-in is the Mac's name (`Host.current().localizedName`).
- **Storage.** Downloads, the cache and covers live under
  `~/Library/Application Support/Dhun/`. All three are on one volume so
  that promoting a song is a rename, as on the phone. The defaults are the
  phone's: a 10 GB download limit, a 3 GB cache, and downloads not on an
  expensive network.
- **When sync runs.** As on the phone: after every edit (2 s debounce), on
  becoming active, when the network returns (`NWPathMonitor`), on waking
  from sleep, and from **Sync now**.
- **Versioning.** The Mac app is versioned on its own, starting at 0.1.0
  (`macos/Info.plist`) and tagged `macos-vX.Y.Z`. `make mac` builds it.
- **Signing is ad hoc only** (`codesign --sign -`): no Apple account, team
  or certificate is involved. The Apple account set up in Xcode on the
  owner's Mac belongs to the owner's employer, and Dhun is a personal project, so
  it must never be signed with it (owner, 2026-10-08). With no Developer
  ID, a family Mac allows the app once under System Settings → Privacy &
  Security. A Developer ID and notarization could come later, only on an
  account of the owner's own, with no code change.
- **Tooling.** `swift format` from the Xcode toolchain, so no new tool
  joins. `make fmt`, `lint-macos` and `test-macos` join the `Makefile`. A
  CI job runs on a macOS runner.

## Plan

Each step ends usable and is installed on the owner's Mac. The status line
moves with it.

0. **Playback spike. Done (2026-10-08).** It tested MP3, AAC/M4A, FLAC,
   Ogg Opus and WebM Opus, from a file, a still-arriving file and the
   stream, and option B held (results above). The engine it built is the
   app's: `macos/Sources/DhunKit/Play/`. The `dhun-play` harness stays as
   the test bench for engine changes, and goes once the app plays music.
   The test music is in `~/Music` on the owner's Mac.

1. **Skeleton.** `make mac` and CI (the Makefile's `lint-macos` and
   `test-macos` came with step 0).
   Sign-in, the token file, the SQLite schema, the library pull, the catalogue
   (albums, artists, genres, the folder tree, search and natural sort), and
   browsing with covers. Ported tests: `CatalogTest`.
2. **Playing.** The engine from step 0 under `Playback`. The 20 queues,
   the queue inspector and picker, listens, resume points (009), hand-off
   (017), media keys and Control Center, and pause on an output change.
   **Release 0.1.0**, the first install.
3. **Offline.** The outbox and the push/pull loop with back-off. A revoked
   token keeps the unsent edits, as on Android. The play cache with prefetch, then Downloads
   and pins with promotion by rename. Ported tests: `QueueOrderTest`,
   `DownloadsTest`; `SongCacheTest`'s waiting reader is `SparseFileTests`.
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

Steps 1 to 5 were built in one go, after the spike, rather than one
install at a time.

**Checked:**

- The Swift tests, which include the phone's catalogue, lyrics, tempo,
  queue-order and downloads tests, ported.
- A throwaway end-to-end run of the real `AppModel` against the server on
  the owner's Mac. It signed in, synced 194 songs, played an album, skipped
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
  inspector open, aborted the app. The first is SwiftUI on macOS 26
  rebuilding a reused row before its environment is there, so rows take
  the model as a value. The second is a layout loop in AppKit with two
  lists side by side in the window's split view, so Queues is now a list
  of queues and each queue opens as a page, as on the phone.
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
- The song cache drops a part file left by an earlier run. A part filled
  out of order (a seek, an index at the end) only knew in memory which of
  its bytes had arrived. Downloads are written in order, so they still
  resume.
- **The engine takes bytes, not decoders.** It opens each song itself, so
  it can interrupt a read waiting for the network. A skip or seek never
  waits for bytes that may not come.
- **Two bugs the first runs found:**
  - The Now Playing artwork closure was inferred as main-actor and trapped
    when MediaPlayer called it on its own queue.
  - Sync's debounce cancelled a sync already under way, and with it its
    requests.

## Open questions

None for the owner. Ogg Vorbis comes back as a question only if Vorbis
files turn up.
