# 039 — The Linux app, at the Android app's level

**Status:** `RELEASED` — 0.17.0, the first Linux release, in the GitHub release with every platform (2026-10-24); 0.17.1, a Linux-only patch (2026-10-25); 0.18.0 in the GitHub release with every platform (2026-10-25)
**Started:** 2026-10-23

## Problem

REQUIREMENTS listed Linux as "optional", and nothing was built for it. On
the owner's Linux desktop only the web client worked, and it cannot do what
matters most there:

- **No offline mode.** The desktop reaches the NAS over slow Wi-Fi, and a
  browser keeps no downloads and no song cache: service workers need HTTPS
  and the NAS is plain HTTP ([030](030_web_client.md)). Every song streams
  again, every time.
- **No pitch**, no Open With from the file manager, and no place in the
  desktop's media controls once the tab is closed.

The owner wants a lightweight Linux app, installed natively or as a
standalone package, and answered three questions (owner, 2026-10-23):

| Question           | Answer                                                                                                                                                                            |
| ------------------ | --------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| Scope              | **Everything.** The Android app is the gold standard: a feature the Mac lacks but Android has is built on Linux too, wherever Linux allows it. Offline as on Android and the Mac. |
| Files on the PC    | **Open With only**, as [021](021_open_from_other_apps.md) on the phone and the Mac's Finder handler. **No local library**: the Linux app plays NAS songs.                         |
| Closing the window | **Keeps playing**, as on the Mac. Playback is controlled from the desktop's media controls and a tray icon; the launcher brings the window back.                                  |

## What carries over, and what does not

The Android app (about 14,500 lines of Kotlin) is the specification. The
Mac app (about 12,000 lines of Swift, [024](024_macos_app.md)) is the
nearest port: it solved the same desktop problems first (a sidebar window,
a player fed by our own byte source, media keys, Open With). Where this
plan is silent, Linux does what the phone does, for the reason the phone's
plan gives.

| Android                                                                               | Linux                                                                                                                                  |
| ------------------------------------------------------------------------------------- | -------------------------------------------------------------------------------------------------------------------------------------- |
| Room tables, `Store` (local write + outbox op), `Sync`                                | The same tables in SQLite, the same behaviour                                                                                          |
| `Playback`: 20 queues, listens, resume points, hand-off                               | Ported, on GStreamer (below)                                                                                                           |
| `SongSource`: download → cache → growing `.part` → stream                             | The same order                                                                                                                         |
| Downloads, the song cache with prefetch, covers                                       | The same, under the app's data folder                                                                                                  |
| Sleep timer, speed and pitch, lyrics, bookmarks (033)                                 | The same                                                                                                                               |
| Podcasts and audiobooks ([031](031_podcasts_and_audiobooks.md))                       | The same                                                                                                                               |
| Tabs, mini bar or floating pill                                                       | A sidebar window with a Now playing bar, as the Mac; a narrow window shows one pane at a time over the phone's mini bar; a mini window |
| Media session, notification, headset controls                                         | **MPRIS** on D-Bus: media keys, the panel's media controls (COSMIC, GNOME, KDE), Bluetooth headset buttons                             |
| Pause when headphones are unplugged                                                   | Pause when the output device goes away                                                                                                 |
| `PlayFileActivity` (open from other apps)                                             | **Open With** from the file manager, in a small window of its own                                                                      |
| Family members (admin only, [023](023_users_on_android.md))                           | The same, under Settings                                                                                                               |
| Light, dark, **black** and **by-time** themes; 8 palettes                             | All four, and the eight palettes as the accent. The Mac left out Black and by-time; on Linux both are cheap                            |
| Wi-Fi only for downloads; prefetch 2 on mobile, 10 on Wi-Fi                           | "Metered network" (GIO's `network-metered`) in place of mobile data, as the Mac's "expensive network"                                  |
| App shortcuts: Search, Resume, Shuffle all ([038](038_voice_search_and_assistant.md)) | The same three, as `Actions` in the `.desktop` file (right-click on the launcher icon)                                                 |
| Voice search, "play it on Dhun" (038)                                                 | **Left out.** Linux has no system speech recogniser and no assistant to hand a request to                                              |
| The headset button with no player running (038)                                       | **Left out.** MPRIS exists only while the app runs; nothing on Linux remembers a player to start                                       |
| Phone-local songs ([025](025_phone_local_songs.md))                                   | **None** (owner, 2026-10-23)                                                                                                           |
| The fast scroller ([035](035_fast_scroller.md))                                       | **Left out**, as on the Mac and the web: a desktop scrollbar can already be dragged                                                    |

**Parity is a standing commitment**, as 024 made it for the Mac: a plan for
an Android feature also says what Linux does, or why Linux leaves it out.

## Options

### Toolkit

- **A. GTK4 with libadwaita.** Chosen. It is the platform toolkit of most
  Linux desktops, already on the owner's machine, and inside a Flatpak it
  comes with the shared GNOME runtime rather than our package.
  `GtkListView` and `GtkColumnView` recycle their rows, so 100,000 songs
  cost only the rows on screen. libadwaita gives the split view, the
  breakpoints and the preference pages. COSMIC, the owner's desktop, runs
  GTK4 apps as they are.
- **B. Qt 6 (QML), through cxx-qt.** Rejected. Native on KDE but not on
  COSMIC or GNOME; the Rust bindings need a C++ bridge and build; a Flatpak
  needs the KDE runtime as well.
- **C. iced, or libcosmic** (which COSMIC is built on). Rejected. Both are
  young and change their API between releases, libcosmic looks native only
  on COSMIC, and neither has a mature virtualised list, text input or
  accessibility layer.
- **D. A WebKitGTK window around the web client.** Rejected. A browser
  engine, which the "native and lightweight" rule excludes, and it would
  still need native code for offline mode, the cache and MPRIS: the reasons
  for the app.

### Language

- **Rust, with gtk4-rs.** Chosen (owner, 2026-10-23). The best-maintained
  GTK bindings, and compiled code meets the speed targets at 100,000 songs
  without special effort.
- **Python with PyGObject.** Rejected, although it is the owner's first
  language. Sorting the catalogue and building the search index for
  100,000 songs would miss the 1 s and 50 ms targets (the Mac's compiled
  Swift already takes 0.8 s at 29,000).
- **Go with gotk4.** Rejected. It would share a language with the server,
  but the bindings use cgo, build slowly and are less actively maintained.
- **Vala or C.** Rejected. Vala has few users outside GNOME; C gives up
  memory safety for nothing we need.

### Playback

The player's duties are the Mac's: **gapless**, **speed and pitch
independently** (±6 semitones, [016](016_speed_and_pitch.md)), playing a
song **while it is still arriving** in the cache
([019](019_networking_and_caching.md)), and every format the server
indexes, including the library's **WebM Opus** files saved as `.opus` (024).

- **A. GStreamer**, `playbin3` fed by a source element of our own.
  Chosen. It is in the GNOME runtime and on every desktop, and plays MP3,
  AAC/M4A, FLAC, Ogg Opus and WebM Opus (`matroskademux`) as they are. Our
  source reads a download, a cache file or a cache file still arriving; its
  `fill(offset, length)` is the Mac's read callback: a read past what has
  arrived waits and tells the fetcher where. So one path serves all four
  sources. `about-to-finish` queues the next song gaplessly, and
  `scaletempo` changes speed while keeping pitch.
- **B. libmpv.** Rejected. Gapless with a good time-stretcher, but it
  brings FFmpeg, which the GNOME runtime lacks: tens of megabytes of codecs
  GStreamer already provides.
- **C. Symphonia (pure-Rust decoders) with PipeWire or cpal.** Rejected.
  No Opus decoder, and we would own the resampler, the time-stretcher and
  the gapless joins: thousands of lines GStreamer has already debugged.

### What the spike showed (2026-10-23)

A throwaway player in the GNOME 51 SDK (GStreamer 1.28.6, Rust 1.98), on
the Mac spike's test files and real NAS songs through the test account.
Each test file is a gliding tone cut at a sample no codec frame lines up
with, each half encoded on its own, played back to back and
cross-correlated with the uncut tone.

| Test                                                          | Result                                                                                    |
| ------------------------------------------------------------- | ----------------------------------------------------------------------------------------- |
| Gapless: FLAC, Ogg Opus, WebM Opus                            | **Exact**: no slip at the join, length exact                                              |
| Gapless: AAC in M4A, index at either end                      | **One sample** off at the join (23 µs), three extra in all. Inaudible; the Mac is exact   |
| Gapless: MP3 written by FFmpeg (`Lavc` in the tag)            | **Not trimmed**: 25 ms of delay play at the start and 28 ms of padding at the join        |
| Gapless: the same MP3 with the tag saying `LAME`              | **Exact**                                                                                 |
| Speed, `scaletempo`, 0.5× to 2×                               | Exact length, pitch kept (440 Hz stays 440 Hz)                                            |
| Pitch and speed, `pitch` (built, below)                       | Exact: +6 semitones gives 622.4 Hz for 622.3, 1.5× at −2 gives 6.667 s at 392.1 Hz        |
| Gapless with `pitch` in the chain, even at no shift           | **Broken**: 45 ms lost between FLAC halves, 3 s between AAC halves                        |
| Gapless with `scaletempo` in the chain                        | Exact, as without it                                                                      |
| Speed after a gapless switch                                  | **Resets to 1×** for the next song, unless set again (below)                              |
| M4A, index at the end, arriving at 200 KB/s                   | First sound in 1.3 s (the Mac: 1.5 s); a seek to 2:30 or 8:00 lands exactly in 0.6 s      |
| 30 MB WebM arriving at 64 KB/s                                | First sound in 3.3 s; a seek to 5:00, 15:00 or 25:00 lands exactly in 2–3 s               |
| Nine real songs from the NAS, all five containers, over Wi-Fi | First sound in 0.12–0.31 s; each decodes to within 25 ms of the length the server reports |

The 64 KB/s figures are mostly the test's own 64 KB fetch unit, a second
each at that rate; the real fetcher streams.

**The library behind those numbers** (test account, 2026-10-23): 32,434
songs, of which 30,704 AAC in MP4, 860 Ogg Opus, 813 MP3, 49 WebM Opus and
8 FLAC. Of eight AAC songs sampled, six had FFmpeg's edit list and two no
gapless information at all, which no player can trim. Of twelve MP3s, one
had a `LAME` tag, four an FFmpeg (`Lavc`) tag with real delay and padding,
two an FFmpeg tag of zeros, and five no tag.

**What the app must do because of it:**

1. **Its own source element, not `appsrc`.** With `appsrc` in
   random-access mode, fed from a thread, a buffer from before a seek could
   land after it, and qtdemux read an atom from the wrong offset ("atom has
   bogus size"). A `BaseSrc` subclass registered for a `dhun://` URI is
   asked for an exact offset and length, and its `unlock` wakes a read
   waiting for the network when a seek or stop flushes: the Mac engine's
   interruptible read, in about 120 lines.
2. **Tell GStreamer the container.** Type detection probes the middle of
   the file for MP3, which for a file still arriving costs a jump there and
   back: 2.6 s instead of 1.3 s to first sound for the M4A above. The
   server reports each song's container, so the app sets `force-caps` on
   `typefind` (`element-setup`). Open With, with no server row, leaves
   detection on.
3. **Rename FFmpeg's MP3 tag for the parser.** `mpegaudioparse` reads the
   encoder delay and padding only from a tag that starts `LAME`
   (`gstmpegaudioparse.c`); FFmpeg writes the same layout under `Lavc` or
   `Lavf`. Our source rewrites those four bytes in the first frame as it
   serves it, and GStreamer then trims exactly. A one-line fix upstream
   would make this unnecessary; it is worth proposing.
4. **Speed on every song's segment, and `pitch` only while pitch is
   shifted.** A pad probe sets the rate on each song's segment event, so
   speed holds across a gapless switch without a seek (2× came out exactly
   half the length). `pitch` breaks gapless joins, so the filter is
   `scaletempo` alone, with `pitch` put in only while a song's pitch is off
   zero; such a song may lose a few milliseconds at its join, which this
   plan accepts.

**Pitch in the Flatpak.** The GNOME runtime has no `pitch` element
(Ubuntu's GStreamer has it). The Flatpak manifest builds libSoundTouch
2.3.3 (LGPL, 143 KB) and only the soundtouch plugin from gst-plugins-bad
1.28.6 (196 KB), in under a minute. libSoundTouch needs
`-DCMAKE_POLICY_VERSION_MINIMUM=3.5` with the SDK's CMake, and
`BUILD_SHARED_LIBS`: its default static library cannot be linked into the
plugin.

**Not measured by the spike:** Ubuntu 24.04's GStreamer 1.24, whose
parsers may differ (CI builds and tests against it); playback through a
real audio device (the spike decoded into memory).

### Packaging

- **A Flatpak**, on the GNOME runtime. Chosen as the standalone package
  (owner, 2026-10-23). GTK4, libadwaita and GStreamer come with the shared
  runtime, so the app is a few megabytes. It works on any distro, COSMIC's
  store and `flatpak` install it, and the sandbox lets the app see only the
  network, audio, the display and the files it is handed.
- **A native install**, `make linux-install`, for a machine with the
  development packages: built against the system's GTK, it installs the
  binary, `.desktop` file, icon and AppStream metadata into `~/.local`.
  Chosen alongside the Flatpak, as the owner asked for both.
- **An AppImage.** Rejected. It must bundle GTK4, libadwaita and GStreamer
  with its plugins: 60–100 MB, against "lightweight". It is tied to the
  oldest glibc it was built on, and bundled GStreamer plugins are a
  well-known cause of AppImages with no sound. Nothing here stops one being
  made later from the same build.
- **A `.deb`.** Not now. It suits only Debian and Ubuntu, where the Flatpak
  and the native install already work.

## Decision

A native Rust app in `linux/`, GTK4 and libadwaita for the window and
GStreamer for playback, at the Android app's level and offline, shipped as
a Flatpak and a native install. Technical defaults, which the owner can
override:

- **The oldest system it builds against is Ubuntu 24.04** (GTK 4.14,
  libadwaita 1.5), the owner's Pop!_OS, so the native install works there
  and one source serves both builds with no feature gates. The Flatpak uses
  the current, newer GNOME runtime.
- **The app ID is `io.github.vivekg7.dhun`**, the Mac's log subsystem.
- **Layout.** As the Mac: `AdwNavigationSplitView` with a sidebar of
  Queues; the library (Folders, Albums, Artists, Genres); podcasts and
  audiobooks; Favorites, Listen Later and the automatic views (010); the
  user's playlists; and Downloads. A Now playing bar along the bottom, the
  hand-off bar above it, and a panel on the right for the playing queue or
  the lyrics. Below 960sp the bar drops its times, or below 1080sp for a
  long file, whose three more buttons would otherwise widen the window
  past its frame; below 800sp the window
  shows one pane at a time and the bar becomes the phone's mini bar.
  Keyboard: Space plays or pauses, Ctrl+← and Ctrl+→ skip, Ctrl+F
  searches, Ctrl+Q quits.
- **Closing the window does not stop the music** (owner, 2026-10-23). The
  window is hidden, not closed, so the app keeps running; a Flatpak asks
  the Background portal the first time, so it is not stopped as an app left
  behind. GNOME's and KDE's portals have that interface; COSMIC's does not,
  and the request fails quietly there, which loses nothing since COSMIC
  does not stop a Flatpak with no window. The launcher, the tray or the
  media controls bring back the same window. Ctrl+Q, or Quit in the ☰ menu
  or the tray, ends it. Signed out, closing quits: there is no music to
  keep.
- **The tray icon** is a StatusNotifierItem with a `com.canonical.dbusmenu`
  menu, which COSMIC and KDE show (GNOME needs its AppIndicator
  extension). Menu: play or pause, previous, next, the current song, the
  mini window, Show Dhun, Quit. A click shows the window, a middle click
  plays or pauses. Written over GIO's D-Bus in about 300 lines, with no
  crate. It registers by the connection's own name, as Chromium's does, so
  the Flatpak only needs to talk to the watcher, not own a name.
- **The mini window**: the cover, title and controls, opened from the ☰
  menu or the tray. A Wayland app cannot keep itself above other windows,
  so the owner pins it with the desktop's own "Always on top". With no
  title bar, as the Mac's, any part of it moves it, and a right-click opens
  that window menu. A double click on the song shows the main window. It
  replaces the Mac's floating mini window and the phone's pill.
- **MPRIS** (`org.mpris.MediaPlayer2.dhun`) over GIO's D-Bus: play, pause,
  next, previous, seek, position, cover and title, and `Raise`, which shows
  the main window (never the mini window or a file's).
- **Pause when the output goes away.** When Pulse moves the stream to
  another device (`pulsesink`'s `current-device` changes: Bluetooth gone,
  the output moved), playback pauses, as on the phone. A jack unplugged
  is not noticed: PipeWire changes the port, not the device, and GStreamer
  says nothing.
- **Open With.** The `.desktop` file's `MimeType` lists the formats the
  server indexes. A file opened from the file manager plays in a small
  window of its own, through a second, plain `playbin3`. It makes no queue
  and logs no listen; the playing queue pauses where it is (021). With Dhun
  not running, the file opens without the library: no sync starts and the
  app ends when the file's window closes.
- **The launcher's shortcuts** (038) are `--search`, `--resume` and
  `--shuffle`, the `.desktop` file's `Actions`. Each is handed to the copy
  already running, or done once the library has loaded on a cold start.
- **Storage.** Everything in `$XDG_DATA_HOME/dhun/`
  (`~/.var/app/io.github.vivekg7.dhun/data/dhun/` in the Flatpak): the
  SQLite database, downloads, the cache and covers, on one volume so that
  promoting a song is a rename. Defaults are the phone's: a 10 GB download
  limit, a 3 GB cache, no downloads on a metered network. This machine's
  settings are `prefs.json` there, not GSettings, whose schema would have
  to be compiled and installed beside the app for a handful of values only
  it reads.
- **The sign-in token** is a file only the user can read (mode 0600) in
  that folder, as on the Mac and the phone. The Secret Service (libsecret)
  would add a keyring dependency and a portal for no gain over the app's
  private folder. The device name is the host name.
- **When sync runs.** As on the phone: after every edit (2 s debounce),
  when the window becomes active, when the network returns (GIO's
  `NetworkMonitor`), on waking from sleep (logind's `PrepareForSleep`), and
  from **Sync now**.
- **Dependencies** (crates): `gtk4`, `libadwaita`, `gstreamer` and
  `gstreamer-base` (for the source element) from the gtk-rs project;
  `rusqlite` on the system's SQLite, not its bundled copy; `ureq` for HTTP,
  with gzip for the first sync and TLS off (the NAS is plain HTTP), on
  worker threads so no async runtime joins; `serde` and `serde_json`;
  `unicode-normalization` for search's NFKD folding, which the standard
  library lacks; `libc` to punch holes behind a long file's window (step 3).
  D-Bus, the network monitor and file paths come from GIO, so `zbus`,
  `dirs` and the like stay out. Base64 for thumbnails is our own few lines.
  `Cargo.lock` is committed.
- **No HTTPS yet.** An `https://` server (the NAS behind `tailscale serve`,
  [003](003_deployment.md)) is refused with a message. It needs a TLS
  crate, which waits until someone uses that setup.
- **Two crates in one workspace.** `linux/core` holds what needs no
  display: the API client, the database, `Store` and `Sync`, the catalogue
  and search, the cache and fetcher, lyrics. `linux/app` holds the window,
  the player, MPRIS and the tray. The split lets `cargo test` run on `core`
  without GTK, GStreamer or a display, as `DhunKit` is tested apart from
  the Mac's window.
- **The Flatpak's build fetches the crates** `Cargo.lock` pins, with
  network access for that one step. Flathub would want each crate listed as
  a source (`cargo-sources.json`); this Flatpak is built on the owner's
  machine and handed out in GitHub releases, so a generator and a second
  file to keep in step with `Cargo.lock` buy nothing. Sandbox permissions:
  network, IPC, Wayland with X11 as a fallback, the GPU, PulseAudio
  (PipeWire answers it), the MPRIS name, the StatusNotifierItem watcher
  and logind (`login1`) for the sleep signal. No file system access: Open
  With hands files in through the document portal.
- **Versioning.** Linux joins the shared sequence (README, Versions); its
  version is in `linux/app/Cargo.toml`. The builds installed while it was
  first built kept 0.16.3 (owner, 2026-10-23): the others reached 0.16 by
  being stable, and a bump per iteration of a first build would say
  nothing. Its first release, 0.17.0, was a minor release for every
  platform together rather than a Linux-only patch (owner, 2026-10-24).
  Later Linux fixes go out as Linux-only patches, from 0.17.1.
- **Releases.** `make linux` builds the Flatpak and archives it in `local/`
  as `dhun-linux-v<version>.flatpak` with its `.sha256`, under the naming
  rules of `make apk` and `make mac`. It is copied to the Mac, which
  publishes the one GitHub release with every platform's build;
  `flatpak install --user <file>` installs it.
- **Tooling.** `cargo fmt` joins `make fmt`; `lint-linux` (`cargo fmt
--check`, `cargo clippy -D warnings`) and `test-linux` (`cargo test`)
  join the `Makefile`, and a CI job runs them on `ubuntu-24.04` with the
  GTK and GStreamer development packages. Without `cargo` on the PATH they
  run inside the GNOME SDK through `scripts/sdk.sh`, so the owner's desktop
  needs no Rust or `-dev` packages; a machine with neither (the Mac) leaves
  them to CI, since its pre-push hook runs `make check`.
  `scripts/check-settings.sh` compares the Linux app's synced defaults with
  the other apps', and `scripts/guide.sh` copies the guide (034) into
  `linux/app/src/guide.md`.
- **Checking the window with no one at the screen**, as an agent does:
  `DHUN_SCREENSHOT=<png>` draws the main window, or the one
  `DHUN_SCREENSHOT_OF` names (`mini`, `file`), dialogs and open popovers
  included, to a PNG a few seconds after start, and quits. `DHUN_OPEN`
  runs steps once the library has loaded: pages (`artists`,
  `artist:<name>`, `album:<name>`, `search:<words>`, …), clicks as a mouse
  does (`nav:N`, `activate:N`), `play:<song id>`, menus and dialogs
  (`select:`, `do:`, `type:`, `popup:`), `tempo:`, `sleep:`, `settings:`,
  `app:<action>` and `close`. `DHUN_SIZE` sets the window's size, and
  `DHUN_AUDIO_SINK` the output (`fakesink sync=true` keeps time,
  silently). On GTK's headless Broadway display (`gtk4-broadwayd`, in the
  SDK) nothing appears on the desktop. `DHUN_DATA=<folder>` gives a build
  under test its own library and sign-in, as on the Mac, and runs it
  beside the installed app instead of handing itself over to it.
- **The speed targets apply.** The app logs `catalogue:` and `search`
  timings under the GLib log domain `Dhun`, as the other apps do, and
  `performance-baseline.md` has a Linux section at 30,000 and 100,000
  songs, with the Flatpak's size and the app's memory, so "lightweight" has
  a number on Linux too.
- **Development is against the production NAS**, signed in with the test
  account in `local/test_account`, never the owner's (owner, 2026-10-23).
  The desktop needs no Docker and no Go for this.

## Plan

Each step ended usable and installed on the owner's desktop.

0. **Toolchain and playback spike.** The GNOME SDK with the Rust extension
   and flatpak-builder as user Flatpaks, so the desktop needs no system
   development packages; then a throwaway player measuring gapless joins,
   speed and pitch, seeks into a still-arriving M4A and WebM, and start
   time. Had a format not trimmed exactly, it would get a trimming step of
   our own, as the Mac's WebM reader trims Opus pre-skip; MP3 needed one,
   the tag renamed. **Done (2026-10-23)**, see
   [What the spike showed](#what-the-spike-showed-2026-10-23). The spike
   stayed out of the repository.
1. **Skeleton.** The workspace, `make linux`, `make linux-install`, the
   Makefile targets and CI; sign-in, the schema, the library pull, the
   catalogue and search, browsing with covers. **Done (2026-10-23)**, see
   [How step 1 went](#how-step-1-went-2026-10-23).
2. **Playing.** The 20 queues, the queue panel and picker, listens, resume
   points (009), hand-off (017), MPRIS, pause on an output change, and the
   outbox and push/pull loop, moved here from step 3 because a queue made
   here and a listen are ops. The first install. **Done (2026-10-23)**, see
   [How step 2 went](#how-step-2-went-2026-10-23).
3. **Offline.** The song cache with prefetch, then Downloads and pins with
   promotion by rename. **Done (2026-10-23)**, see
   [How step 3 went](#how-step-3-went-2026-10-23).
4. **The rest of the phone.** Playlist editing (013), Favorites, Listen
   Later and the automatic views (010), podcasts and audiobooks (031),
   bookmarks (033), lyrics (015), speed and pitch (016), the sleep timer
   (014), settings with all themes and palettes (018, 034), family members
   (023). **Done (2026-10-24)**, see
   [How step 4 went](#how-step-4-went-2026-10-24).
5. **Linux-only pieces.** The tray icon, the mini window, Open With, the
   launcher's shortcuts, the Background portal. **Done (2026-10-24)**, see
   [How step 5 went](#how-step-5-went-2026-10-24).
6. **Measure and release.** The Linux section of `performance-baseline.md`,
   then the docs brought up to date. **Done (2026-10-24)**, see
   [How step 6 went](#how-step-6-went-2026-10-24).

What is still open is under [Still open](#still-open).

## How step 1 went (2026-10-23)

**Built:** `linux/core` (the API client, the database with the Mac's
schema and migrations, the library pull with plan 013's playlist rules,
covers, the catalogue and search) and `linux/app` (sign-in, the sidebar,
albums as a grid of covers, artists, genres, the folder tree, audiobooks
and podcasts, a filter on each page past 12 rows, search over all of
them). The bundle was 793 KB, 2.5 MB installed.

**Checked:** 15 tests, among them the phone's and the Mac's catalogue and
search cases and `api/search-fold.tsv`, which the Rust fold passes as the
other four copies do. The first pull took 32,434 songs in 1.5 s and the
6,975 thumbnails in 104 s over the desktop's Wi-Fi. Every page was checked
in Broadway screenshots. The timings, 31,212 songs on the desktop, are in
[performance-baseline](../research/performance-baseline.md#2026-10-23-the-linux-app-plan-039-step-1):
library on screen about 1.1 s after start (target 1 s), a keystroke
6–20 ms (target 50 ms), the catalogue built in 145–160 ms where the Mac
takes 723–809 ms.

**Changed while building:**

- **The search index keeps its words, sound keys and titles packed** in a
  few large arrays rather than an allocation each. The first build took
  41 MB for the song index, half of it freed pages stranded among the
  index's small allocations; packed, 13 MB, with keystrokes as fast. The
  Mac's and the phone's indexes hold the same data the old way, which is
  worth a look there.
- **The catalogue shows before the search indexes are built**, in a second
  pass; together they kept the library off screen a third of a second
  longer.
- **Songs are read straight into shared pointers**, so the catalogue no
  longer copies each (13 MB freed and stranded).
- **Decoded covers are kept up to 32 MB**, not up to 200 covers, which at
  256 px came to 50 MB.

## How step 2 went (2026-10-23)

**Built:** in `linux/core`, the queue, mark, resume, bookmark, setting and
play-count tables; the outbox and its ops (the Mac's `Store`); the whole
sync round with play counts; the synced settings' defaults; queue order,
play order, rewinding and chapters as plain functions; and `Part`, a
song's bytes arriving in a file. In `linux/app`, the engine (`playbin3`,
our source element, the spike's four findings), `Playback` (the Mac's,
less what step 4 added), MPRIS, the Now playing bar, the queue panel and
picker, the Queues pages, the hand-off and resume offers, and Play Next
and Add to Queue on every song list.

**Checked**, on Broadway with `fakesink sync=true`, against the NAS:

- An album played, and a seek near the end of its first song went on to
  the second gaplessly.
- A song heard from 1:40 to its end (3:34) was counted by the server once,
  at the time it was heard.
- The test account's 18 queues arrived, and one made here reached the
  server.
- MPRIS on the session bus gave title, artist, album, length and cover;
  PlayPause from the bus played.
- An audiobook sought an hour in, then the app quit: the queue's place and
  a resume point were kept at 1:00:03.5, sent at the next start, and the
  app came back paused there.
- Another device's playback was offered ("Continue from Phone test: “Mast
  Magan” at 1:01"), and Continue took its queue and song and played on.
- 21 unit tests: the outbox's replacing ops, queue order, what plays next,
  rewinding, chapters and the waiting reader.

The owner first heard it through the speakers on 2026-10-23.

**Changed while building:**

- **A change announced while another is being told reaches everyone.**
  The first version took the listeners out while calling them, so a page
  opened, or a queue restored, from the catalogue's listener told no one.
- **No borrow is held across a call that can announce a change.** 0.16.3
  as first installed aborted on any sidebar click: the click held the
  sidebar's routes while opening the page, whose `Section` change
  refilled the sidebar, borrowing them again, and a panic in a GTK signal
  cannot unwind. The test hook opened pages directly and never saw it; its
  `nav:N` and `activate:N` steps now click as a mouse does.
- **Each song is fetched as it plays**, into a file that goes when nothing
  reads it; step 3's cache keeps them.

## How step 3 went (2026-10-23)

**Built:** in `linux/core`, the song cache, which keeps every song played
and fetches the next ten of the queue while playing (two on a metered
network) within 3 GB; `Part` grown to keep a whole song, open a downloaded
or cached file, notice a song gone from the server, and be a long file's
window (037); Downloads' pins turned into songs, and a download resumed
where a broken try stopped. In `linux/app`, Downloads one song at a time,
a cached song moving there by rename; Download on the pages of an album,
folder, artist, genre, playlist, book and show, and in the song menu; a
Downloads page; a mark on a downloaded song; and, with the server
unreachable, songs not on this computer dimmed.

**Checked** against the NAS, silent:

- An album played: the song playing and the three after it were kept whole
  in the cache, byte for byte the server's file.
- That album's folder and one other song downloaded: the four cached songs
  moved into Downloads without a fetch; the other was fetched.
- Server unreachable: a downloaded and a cached song played; others were
  dimmed, and one played waits for the network.
- A 60-minute episode (44 MB) was kept as a window: 24 MB, its first 30
  minutes. Sought to 45 minutes, the window moved and what was behind went
  back to the disk: 15 MB in all.
- 27 unit tests, with the window following its reader and the pins (three
  ported from the Mac).

**Changed while building:**

- **The cache is shared, not the main thread's.** GStreamer opens the next
  song on its own thread for a gapless start, so the cache answers under a
  lock. A finished fetch sets a flag that `Playback`'s tick reads, where
  the Mac's fetch calls back into its main actor.
- **Behind a window, a hole is punched, not the file rolled** (037 rolled
  it, since Android cannot punch). Linux file systems give the blocks back
  in place, so nothing is copied and no reader reopens. `libc` joined the
  dependencies for that one call; it was already built, through GLib.
- **A cell follows what it shows.** Rows on screen are not bound again when
  only the songs kept here change, so each song-list cell listens for that
  itself, and the selection is kept.
- **A build under test (`DHUN_DATA`) runs beside the installed app.** As a
  second launch of the same app it had handed itself to the running copy
  and quit.

## How step 4 went (2026-10-24)

**Built:** in `linux/core`, the remaining `Store` edits (marks, bookmarks,
every playlist edit, a move naming entries rather than songs, since a song
can be in a playlist twice), the sort orders shared by queues and
playlists, `Tempo`, lyrics (the LRC parser, kept copies, offline search),
`Heard` for books and shows, the synced settings' choices, and the
family-members calls. In `linux/app`: the Lists section (Favorites,
Listen Later, the five automatic views); the phone's song menu; playlists
made, renamed, sorted, deleted, and their songs removed or dragged; a
queue's songs dragged, sorted and saved as a playlist, queues dragged into
order and "Remove all other queues"; Play and Shuffle on every list's
header; on the Now playing bar Favorite and Listen Later, and for a long
file seek back and forward, a bookmark, and chapters, notes and bookmarks
in Shuffle's place; speed and pitch; the sleep timer, with "Stop after
this song" in the playing queue's menu; lyrics beside the page, kept with
downloads and searched; a book's and a show's page as the Mac's; and
Settings: five themes (system, light, dark, black, day and night by the
clock), eight accents, the Playback settings that follow the user, this
computer's limits and space used, the account, family members for the
admin, and How it works.

**Checked** against the NAS, silent:

- A favourite from an album's song menu showed in Favorites; a playlist
  made from the menu was created on the server
  (`Playlists/test/Linux test.m3u8`), then added to, sorted, a song
  removed, and renamed, the server's file following each.
- A queue reversed from its Sort menu, the server's copy too.
- 1.5× moved a song 7.5 s in 5 s, and 1× back to 5 s; −2 semitones at
  1.25× reopened the song in place with `pitch` in the chain and went on
  at 1.25×. The Flatpak has `pitch` (`gst-inspect-1.0`).
- "At the end of this song" paused at the next song's start.
- A chaptered episode showed "Chapter 5 of 24 · Perceptronium" and its
  chapters in the panel, the one playing marked.
- Synced lyrics followed the song; "baarish ka bosa" found it by its lyrics
  on the server.
- The Settings pages, the black theme with the Teal accent, and the
  Downloads and Account figures.
- 33 unit tests, among them the playlist move, the tempo setting read
  defensively, three lyrics tests and a book carrying on from its first
  file not played.

**Changed while building:**

- **A seek waiting for its song to open is dropped when another opens
  first.** Started at once after a queue was restored paused at 44 s, the
  new song began at 44 s. A bug since step 2, found by the speed check.
- **Pages keep their place when built again.** The playing queue's page
  was rebuilt at every song; now only an edit to its name or songs
  rebuilds it, and a rebuilt page scrolls back to where it was. Space no
  longer plays or pauses while a dialog is open.
- **Return in a dialog's text field answers it** through the field's own
  `activate`: a field inside an `AdwAlertDialog` does not reach the
  default button.

## How step 5 went (2026-10-24)

**Built:** the tray icon (`tray.rs`), the mini window (`mini.rs`), Open
With (`openfile.rs`), the launcher's three shortcuts, and closing the
window hiding it. The ☰ menu gained Mini Player and Quit.

**Checked**, silent:

- The tray through the Flatpak's D-Bus filter with only
  `--talk-name=org.kde.StatusNotifierWatcher`: COSMIC's watcher listed it,
  its tooltip and menu answered, Pause from the menu paused (MPRIS agreed)
  and the item became Play; Quit ended the app and the panel dropped the
  icon.
- The window closed while a song played: it played on (MPRIS's position
  moved 3 s in 3 s), and `Raise` showed the same window.
- With a running copy, on a private session bus: `dhun --shuffle` started
  "All songs" shuffled, `dhun <file>` paused the queue and opened the
  file's window, `dhun --search` showed the window; each second launch
  exited at once.
- A file opened with Dhun not running: a tagged M4A showed its title,
  artists and embedded cover and played; an untagged FLAC showed its name
  and played to the end.
- The mini window after Shuffle all: cover, song and controls.

## How step 6 went (2026-10-24)

Measured through the installed Flatpak at 31,239 songs and at 100,000 made
from them; the numbers are in
[performance-baseline](../research/performance-baseline.md#2026-10-24-the-linux-app-with-every-feature-plan-039-step-6).
Keystrokes are within 50 ms at both sizes. The library is drawn 1.0–1.2 s
after launch, just over the 1 s target, and 1.9 s at 100,000; with the
catalogue built off the main thread, what is left is the views made again
on it and GTK's first layout of them. Two fixes came out of it: the queue
panel no longer remakes all its rows on every play, pause and song change
(75 ms with "All songs" as the queue, 250 ms at 100,000), and a list drawn
before the thumbnails arrive now shows them, since a placeholder cover
waits for them itself (GTK does not bind a row again for the same item).

A later session measured CPU, memory and wakeups (0.17.0, in the
[baseline](../research/performance-baseline.md#2026-10-24-the-linux-apps-resources-over-a-session-0170))
and led to 0.17.1's fixes: a leak on navigation, Go to Album and Artist
building pages nobody saw, the albums grid decoding covers at every
scroll frame, an artist's album strip built all at once, Next and
Previous holding the window, and small rows keeping full-size
thumbnails on the GPU.

## A pass over every feature (2026-10-24)

Every page, list, popover and Settings page was drawn as the test account;
play, pause, next, previous, seek, the position and the launcher's
shortcuts were driven over MPRIS; themes were checked on the owner's
desktop, where COSMIC sets a system accent; and the Android app was read
alongside for what Linux lacked.

**Fixed:**

- **The accent and the Black theme did nothing.** Since libadwaita 1.6 the
  stylesheet paints with CSS variables (`--accent-bg-color`), so the
  `@define-color` overrides changed nothing and the system's accent
  showed. They now set the variables.
- **The Now playing bar did not fit.** Below about 1,050 px the seek bar
  shrank to a sliver, below 920 px the window was cut off. Long titles ask
  for less room, and the two breakpoints (Decision, Layout) keep
  everything inside.
- **With no sound output the queue ran away.** With no output (no
  PulseAudio, a device that refuses) every song failed in turn and was
  passed over, about 30 a second, the queue's place synced as it went; a
  queue restored paused did it at launch. An output's failure now stops
  where it is, with a note; a song that fails while paused stays put; one
  failure told several times counts once. Only a song's own failure while
  playing moves on, as on the phone.
- **Sync on waking from sleep**, which the Decision promised, was missing.
- **Sign Out in the ☰ menu** signed out at once; it now asks, as Settings
  does.
- **Play and Shuffle** were missing on a folder's page (every song under
  it) and on the user's own playlists.
- **Search results had no song menu**; songs, episodes and lyrics hits
  now have the phone's.
- A smaller song-cache limit now trims the cache at once.

**Missing against the phone, then built:**

- **Recent searches and the empty field's help** (029). A click into the
  empty search field, or Ctrl+F, opens the search page on the last ten
  searches, each with ✕, and Clear; with none, what the field searches and
  its `artist:` and other words. A search is kept when something it found
  is opened or played; they belong to this computer and survive a
  sign-out, as on the phone. A page rather than the Mac's suggestions
  under the field, since a popover there would take the field's focus.
- **Signing out asks whether to keep the downloads** (026), from Settings
  and the ☰ menu: Keep Downloads or Delete Downloads, with how many songs
  and how much room; with none, just Sign Out. Kept, the files, their pins
  and lyrics stay, and the song cache goes. Unlike the phone they do not
  play while signed out: Linux has no songs of its own, so there is
  nothing to use without an account. Whoever signs in next takes them
  over: someone else gets each file as a download of its own song, the
  pins of the leaver's playlists and lists go, and the library's pins
  (album, folder, artist, genre, book or show) stay. Until that sign-in's
  first sync, no file is deleted for want of a pin. Signing in as someone
  else keeps the downloads too, as on the phone (it used to delete them),
  whatever the server's address, since one server is reached at two. A
  file whose song the server lacks, or holds at another size, goes or is
  fetched again, as any download.
- **The "downloaded" mark** (012) after a downloaded album (in the grid),
  folder, artist, genre, book, show or playlist (in the sidebar), and such
  a search result. It shows the download asked for, whether or not every
  file is here; the page's Download button says how far along it is.
- **The thin resume bar** (009) under a long file's title in every song
  list, as far as its resume point; none under 1%, which every episode
  played a moment has. Resume points are kept in memory with the marks, so
  a row reads them without the database, and a pause moves the bar at
  once.
- **A queue's search finds rather than filters** (032), as Musicolet's
  and the Mac's: "Find in queue" lists the matches with their number in
  the queue; ↑ and ↓ move through them, Return or a click picks one, and
  the queue scrolls to it, centred and selected. The queue stays whole, so
  a song can be dragged with its neighbours in view. GTK's `scroll_to`
  cannot centre a row, so the list is scrolled by hand first: its rows are
  of one height.
- **The playing-queue panel has the song menu and dragging** (020), as a
  queue's page: a right-click opens the queue's song menu (Stop After This
  Song and Remove from Queue among it), and a song dropped on another
  takes its place.
- **The queue picker renames and removes** (020), as the phone's: each
  queue has ✎ and ✕, each asking first, and Remove All Other Queues… keeps
  only the one playing. The dialogs open from the picker's button, the
  picker closing first.
- **The line under the title on the bar is links**, as on the phone: each
  artist opens their page, the album its own, and an episode's show or a
  book's title the show's or book's, in the line's colour and underlined
  under the pointer. The line is set again only when its text changes: set
  four times a second with the position, a link could lose a click between
  press and release.
- **Downloads has Play and Shuffle** for its songs, music only (031), and
  lists downloaded episodes and books' files apart, under "Podcasts and
  audiobooks", by book and show in their order; one plays its own show or
  book from there, as on their pages.

## Still open

None of these needs the owner's answer.

- **Speed.** The library is drawn just over the 1 s target today, 1.9 s at
  100,000 songs. About 1.4 s after launch, online, the main thread stops
  for 0.2–0.3 s during the first sync, not yet traced. The albums grid
  still takes 42–55 % of a core to scroll, which is GTK's
  ([baseline](../research/performance-baseline.md#2026-10-24-the-linux-apps-resources-over-a-session-0170)).
  Memory rose 70 MB from step 1 to step 6, mostly decoded thumbnails, now
  kept to 1,000. The native build is not measured.
- **Behaviour.** A jack unplugged does not pause (Decision). The server
  counts as unreachable only once a sync has failed; a song that cannot be
  fetched does not say so sooner, as the Mac's fetcher does. Playlist
  entries that match no song are not shown, where the Mac shows them as
  "Unavailable"; they are kept, and sorting keeps them at the end. A
  book's or a show's filter says "Filter songs".
- **Untried.** Family members (the test account is not an admin);
  dragging a song or a queue by hand (the playlist move is tested, the
  queue's is not); and, seen only through D-Bus and screenshots, the tray
  icon on the panel, the mini window pinned "Always on top", Open With
  from COSMIC Files, and whether COSMIC's dock shows the `.desktop`
  `Actions`.
- **Upstream.** Propose the `Lavc`/`Lavf` tag fix to `mpegaudioparse`, and
  look at packing the Mac's and the phone's search indexes as here.
