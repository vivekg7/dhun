# 039 — The Linux app, at the Android app's level

**Status:** `MERGED` — every step done and measured (2026-10-24): the phone's features, the tray, the mini window and Open With; 0.16.3 installed on the owner's desktop, not yet in a GitHub release
**Started:** 2026-10-23

## Problem

REQUIREMENTS listed Linux as "optional", and nothing was built for it. The
owner's Linux desktop could only use the web client, and the web client
cannot do what matters most on that machine:

- **It has no offline mode.** The desktop reaches the NAS over slow Wi-Fi,
  and a browser keeps no downloads and no song cache, because service
  workers need HTTPS and the NAS is plain HTTP
  ([030](030_web_client.md)). Every song streams again, every time.
- **It has no pitch**, no Open With for files from the file manager, and no
  place in the desktop's own media controls once the tab is closed.

The owner wants a lightweight Linux app, installed either natively or as a
standalone package (owner, 2026-10-23). They answered three questions about
behaviour (2026-10-23):

| Question           | Answer                                                                                                                                                                            |
| ------------------ | --------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| Scope              | **Everything.** The Android app is the gold standard: a feature the Mac lacks but Android has is built on Linux too, wherever Linux allows it. Offline as on Android and the Mac. |
| Files on the PC    | **Open With only**, as [021](021_open_from_other_apps.md) on the phone and the Mac's Finder handler. **No local library**: the Linux app plays NAS songs.                         |
| Closing the window | **Keeps playing**, as on the Mac. Playback is controlled from the desktop's media controls and a tray icon; the launcher brings the window back.                                  |

## What carries over, and what does not

The Android app (about 14,500 lines of Kotlin) is the specification. The
Mac app (about 12,000 lines of Swift, [024](024_macos_app.md)) is the
nearest port, because it solved the same desktop problems first: a sidebar
window, a player fed by our own byte source, media keys, Open With. Where
this plan is silent, Linux does what the phone does, for the reason the
phone's plan gives.

| Android                                                                               | Linux                                                                                                                                               |
| ------------------------------------------------------------------------------------- | --------------------------------------------------------------------------------------------------------------------------------------------------- |
| Room tables, `Store` (local write + outbox op), `Sync`                                | The same tables in SQLite, ported unchanged in behaviour                                                                                            |
| `Playback`: 20 queues, listens, resume points, hand-off                               | Ported; GStreamer underneath (below)                                                                                                                |
| `SongSource`: download → cache → growing `.part` → stream                             | The same order                                                                                                                                      |
| Downloads, the song cache with prefetch, covers                                       | The same, under the app's data folder                                                                                                               |
| Sleep timer, speed and pitch, lyrics, bookmarks (033)                                 | The same                                                                                                                                            |
| Podcasts and audiobooks ([031](031_podcasts_and_audiobooks.md))                       | The same                                                                                                                                            |
| Tabs, mini bar or floating pill                                                       | A sidebar window with a Now playing bar, as the Mac; a small window shows one pane at a time over the phone's mini bar. A small mini window (below) |
| Media session, notification, headset controls                                         | **MPRIS** on D-Bus: media keys, the panel's media controls (COSMIC, GNOME, KDE), Bluetooth headset buttons                                          |
| Pause when headphones are unplugged                                                   | Pause when the output device goes away                                                                                                              |
| `PlayFileActivity` (open from other apps)                                             | **Open With** from the file manager, in a small window of its own                                                                                   |
| Family members (admin only, [023](023_users_on_android.md))                           | The same, under Settings                                                                                                                            |
| Light, dark, **black** and **by-time** themes; 8 palettes                             | All four, and the eight palettes as the accent. The Mac left out Black and by-time; Linux can do both cheaply, so it has them                       |
| Wi-Fi only for downloads; prefetch 2 on mobile, 10 on Wi-Fi                           | "Metered network" (GIO's `network-metered`) in place of mobile data, as the Mac's "expensive network"                                               |
| App shortcuts: Search, Resume, Shuffle all ([038](038_voice_search_and_assistant.md)) | The same three, as `Actions` in the `.desktop` file (right-click on the launcher icon)                                                              |
| Voice search, "play it on Dhun" (038)                                                 | **Left out.** Linux has no system speech recogniser and no assistant to hand a request to                                                           |
| The headset button with no player running (038)                                       | **Left out.** MPRIS exists only while the app runs; nothing on Linux remembers a player to start                                                    |
| Phone-local songs ([025](025_phone_local_songs.md))                                   | **None** (owner, 2026-10-23)                                                                                                                        |
| The fast scroller ([035](035_fast_scroller.md))                                       | **Left out**, as on the Mac and the web: a desktop scrollbar can already be dragged                                                                 |

**Parity is a standing commitment**, as 024 made it for the Mac: from now on
a plan for an Android feature also says what Linux does, or why Linux leaves
it out.

## Options

### Toolkit

- **A. GTK4 with libadwaita.** Chosen. It is the platform toolkit of most
  Linux desktops, it is already on the owner's machine, and inside a
  Flatpak it comes with the shared GNOME runtime rather than inside our
  package. `GtkListView` and `GtkColumnView` recycle their rows, so a list
  of 100,000 songs costs only the rows on screen. libadwaita gives the
  sidebar split view, the narrow-window breakpoints and the preference
  pages. The owner's desktop is COSMIC, which runs GTK4 apps as they are.
- **B. Qt 6 (QML), through cxx-qt.** Rejected. Qt looks native on KDE but
  not on COSMIC or GNOME, the Rust bindings go through a C++ bridge and a
  C++ build, and a Flatpak needs the KDE runtime as well.
- **C. iced, or libcosmic** (which COSMIC itself is built on). Rejected.
  Both are young and change their API between releases, libcosmic looks
  native only on COSMIC, and neither has a mature virtualised list, text
  input or accessibility layer.
- **D. A WebKitGTK window around the web client.** Rejected. It is a
  browser engine, which the "native and lightweight" rule excludes. And it
  would still need native code for offline mode, the cache and MPRIS, which
  are the reasons for the app.

### Language

- **Rust, with gtk4-rs.** Chosen (owner, 2026-10-23). These are the
  best-maintained GTK bindings, and compiled code meets the speed targets
  at 100,000 songs without special effort.
- **Python with PyGObject.** Rejected, although it is the owner's first
  language. Sorting the catalogue and building the search index for
  100,000 songs in Python would miss the 1 s and 50 ms targets (the Mac's
  compiled Swift already takes 0.8 s at 29,000).
- **Go with gotk4.** Rejected. It would share a language with the server,
  but the bindings use cgo, build slowly and are maintained less actively.
- **Vala or C.** Rejected. Vala is GNOME's own language but has few users
  outside it; C gives up memory safety for nothing we need.

### Playback

The player has the same duties as the Mac's: **gapless**, **speed and pitch
independently** (±6 semitones, [016](016_speed_and_pitch.md)), playing a
song **while it is still arriving** in the cache
([019](019_networking_and_caching.md)), and every format the server
indexes, including the library's **WebM Opus** files saved as `.opus` (024).

- **A. GStreamer**, `playbin3` fed by a source element of our own.
  Chosen. GStreamer is in the GNOME runtime and on every desktop, and it
  plays MP3, AAC/M4A, FLAC, Ogg Opus and WebM Opus (`matroskademux`) as
  they are. Our source reads from a download, a cache file or a cache file
  still arriving, and its `fill(offset, length)` is the Mac's read
  callback: a read past what has arrived waits for it and tells the fetcher
  where it waits. So, as on the Mac, one path serves all four sources.
  `about-to-finish` queues the next song for gapless playback, and
  `scaletempo` changes the speed while keeping the pitch. What the spike
  showed, and the four things the app must do because of it, are below.
- **B. libmpv.** Rejected. mpv plays gapless and has a good time-stretcher,
  but it brings FFmpeg, which the GNOME runtime does not ship. The Flatpak
  would carry tens of megabytes of codecs that GStreamer already provides.
- **C. Symphonia (pure-Rust decoders) with PipeWire or cpal for output.**
  Rejected. Symphonia has no Opus decoder, and we would own the resampler,
  the time-stretcher and the gapless joins: thousands of lines that
  GStreamer has already debugged.

### What the spike showed (2026-10-23)

A throwaway player in the GNOME 51 SDK (GStreamer 1.28.6, Rust 1.98),
reading the Mac spike's kind of test files and real songs from the NAS
through the test account. Each test file is a tone whose pitch glides,
cut at a sample no codec frame lines up with, each half encoded on its
own; the two halves are played back to back and cross-correlated with
the uncut tone.

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

**The library behind those numbers** (the test account, 2026-10-23): 32,434
songs, of which 30,704 are AAC in MP4, 860 Ogg Opus, 813 MP3, 49 WebM Opus
and 8 FLAC. Of eight AAC songs sampled, six had FFmpeg's edit list (as the
test files) and two had no gapless information at all, which no player can
trim. Of twelve MP3s, one had a `LAME` tag, four an FFmpeg (`Lavc`) tag
with real delay and padding, two an FFmpeg tag of zeros, and five no tag.

**What the app must do because of it:**

1. **Its own source element, not `appsrc`.** The spike began with an
   `appsrc` in random-access mode, fed from a thread. A buffer from before
   a seek could land after it, and qtdemux then read an atom from the
   wrong offset ("atom has bogus size"). A `BaseSrc` subclass, registered
   for a `dhun://` URI, is asked for an exact offset and length, and its
   `unlock` wakes a read waiting for the network when a seek or stop
   flushes: the Mac engine's interruptible read, in about 120 lines.
2. **Tell GStreamer the container.** Type detection probes the middle of
   the file for MP3, which for a file still arriving costs a jump there and
   back: 2.6 s instead of 1.3 s for the first sound of the M4A above. The
   server already reports each song's container, so the app sets
   `force-caps` on the `typefind` element (`element-setup`). Open With,
   which has no server row, leaves detection on.
3. **Rename FFmpeg's MP3 tag for the parser.** GStreamer's `mpegaudioparse`
   reads the encoder delay and padding only from a tag that starts `LAME`
   (`gstmpegaudioparse.c`); FFmpeg writes the same layout under `Lavc` or
   `Lavf`. Our source rewrites those four bytes in the first frame as it
   serves it, which GStreamer then trims exactly. A one-line fix upstream
   would make this unnecessary; it is worth proposing.
4. **Speed on every song's segment, and `pitch` only while pitch is
   shifted.** A pad probe on the filter sets the rate on each song's
   segment event, so speed holds across a gapless switch without a seek
   (2× came out exactly half the length). The `pitch` element breaks
   gapless joins, so the filter is `scaletempo` alone, and `pitch` is put
   in only while a song's pitch is off zero; such a song may lose a few
   milliseconds at its join, which this plan accepts.

**Pitch in the Flatpak.** The GNOME runtime has no `pitch` element; Ubuntu's
GStreamer has it. The Flatpak manifest builds libSoundTouch 2.3.3 (LGPL,
143 KB) and only the soundtouch plugin from gst-plugins-bad 1.28.6
(196 KB), both inside the SDK in under a minute (spike, 2026-10-23).
libSoundTouch 2.3.3 needs `-DCMAKE_POLICY_VERSION_MINIMUM=3.5` with the
SDK's CMake.

**Not yet measured:** the native build against Ubuntu 24.04's GStreamer
1.24, whose parsers may differ; it is checked in step 1. Playback through
a real audio device: the spike decoded into memory, as fast as it could.

### Packaging

- **A Flatpak**, on the GNOME runtime. Chosen as the standalone package
  (owner, 2026-10-23). GTK4, libadwaita and GStreamer come with the shared
  runtime, so the app itself is a few megabytes. It works on any distro, and
  COSMIC's store and `flatpak` install it. It also sandboxes the app, which
  then sees only the network, audio, the display and the files it is
  handed.
- **A native install**, `make linux-install`, for a machine with the
  development packages. It builds against the system's GTK and installs the
  binary, the `.desktop` file, the icon and the AppStream metadata into
  `~/.local`. Chosen alongside the Flatpak, as the owner asked for both.
- **An AppImage.** Rejected. It has to bundle GTK4, libadwaita and
  GStreamer with its plugins: 60–100 MB, against "lightweight". It is tied
  to the oldest glibc it was built on, and bundled GStreamer plugins are a
  well-known source of AppImages with no sound. Nothing in this plan stops
  one being made later from the same build.
- **A `.deb`.** Not now. It suits only Debian and Ubuntu, and on those the
  Flatpak and the native install already work.

## Decision

A native Rust app in `linux/`, GTK4 and libadwaita for the window and
GStreamer for playback, at the Android app's level and offline, shipped as
a Flatpak and as a native install. Technical defaults, which the owner can
override:

- **The oldest system it builds against is Ubuntu 24.04** (GTK 4.14,
  libadwaita 1.5), the owner's Pop!_OS. The native install then works on
  the owner's desktop, and one source serves both builds with no feature
  gates. The Flatpak uses the current GNOME runtime, which is newer.
- **The app ID is `io.github.vivekg7.dhun`**, the Mac's log subsystem.
- **Layout.** As the Mac: `AdwNavigationSplitView` with a sidebar of
  Queues; the library (Folders, Albums, Artists, Genres); podcasts and
  audiobooks; Favorites, Listen Later and the automatic views (010); the
  user's playlists; and Downloads. A Now playing bar along the bottom, the
  hand-off bar above it, and a panel on the right for the playing queue or
  the lyrics. Below a width breakpoint the window shows one pane at a time,
  over the phone's mini bar, so a narrow window stays usable. Keyboard: Space plays
  or pauses, Ctrl+← and Ctrl+→ skip, Ctrl+F searches, Ctrl+Q quits.
- **Closing the window does not stop the music** (owner, 2026-10-23). The
  window is hidden rather than closed, which keeps the app running, and a
  Flatpak asks the Background portal for leave to do so, so it is not
  stopped as an app left behind. Opening the app again from the launcher,
  the tray or the desktop's media controls brings back the same window.
  Ctrl+Q, Quit in the ☰ menu or the tray's ends it. Signed out, closing
  quits: there is no music to keep.
- **The tray icon** is a StatusNotifierItem, which COSMIC and KDE show
  (GNOME needs its AppIndicator extension). Its menu: play or pause,
  previous, next, the current song, the mini window, Show Dhun, Quit. A
  click shows the window, a middle click plays or pauses. Written over
  GIO's D-Bus, about 250 lines, with no crate. It registers by the
  connection's own name, as Chromium's does, so the Flatpak needs only to
  talk to the watcher, not to own a name of its own.
- **The mini window** is a small window with the cover, title and controls,
  opened from the ☰ menu or the tray. A Wayland app cannot keep itself
  above other windows, so the owner pins it with the desktop's own "Always
  on top" window menu item. With no title bar, as the Mac's, it is moved
  by any part of it, and a right-click opens that window menu. A double
  click on the song shows the main window. This replaces the Mac's
  floating mini window and the phone's pill.
- **MPRIS** (`org.mpris.MediaPlayer2.dhun`) over GIO's D-Bus: play, pause,
  next, previous, seek, position, cover and title, and `Raise` to show the
  window.
- **Pause when the output goes away**: a `GstDeviceMonitor` on audio sinks;
  when the sink Dhun plays to disappears (headphones out, Bluetooth gone),
  playback pauses, as on the phone.
- **Open With.** The `.desktop` file declares the audio types the server
  indexes. A file opened from the file manager plays in a small window of
  its own, through a second player (a plain `playbin3` on the file). It
  makes no queue and logs no listen; the playing queue pauses where it is
  (021). With Dhun not running, the file opens without the library: no
  sync starts and the app ends when the file's window closes.
- **The launcher's shortcuts** (038) are `--search`, `--resume` and
  `--shuffle`, the `.desktop` file's `Actions`. Each is handed to the copy
  already running, or done once the library has loaded on a cold start.
- **Storage.** Everything in one folder, `$XDG_DATA_HOME/dhun/`
  (`~/.var/app/io.github.vivekg7.dhun/data/dhun/` inside the Flatpak): the
  SQLite database, downloads, the cache and covers, on one volume so that
  promoting a song is a rename. `DHUN_DATA=<folder>` points a build under
  test at a folder of its own, as on the Mac. The defaults are the
  phone's: a 10 GB download limit, a 3 GB cache, no downloads on a metered
  network.
- **The sign-in token** is kept in a file only the user can read (mode 0600) in that folder, as on the Mac and the phone. The Secret Service
  (libsecret) would add a keyring dependency and a portal for no gain over
  the app's own private folder. The device name sent at sign-in is the
  host name.
- **When sync runs.** As on the phone: after every edit (2 s debounce), when
  the window becomes active, when the network returns (GIO's
  `NetworkMonitor`), on waking from sleep (logind's `PrepareForSleep`), and
  from **Sync now**.
- **Dependencies** (crates): `gtk4`, `libadwaita`, `gstreamer`,
  `gstreamer-app` and `glib`/`gio` from the gtk-rs project; `rusqlite` on
  the system's SQLite (not its bundled copy); `ureq` for HTTP, with gzip
  for the first sync and its TLS off because the NAS is reached over plain
  HTTP, on worker threads so no async runtime joins; `serde` and
  `serde_json`; `unicode-normalization` for search's NFKD folding, which
  the standard library lacks; `libc` for punching holes behind a long
  file's window (step 3). D-Bus, the network monitor and file paths
  come from GIO, so `zbus`, `dirs` and the like stay out. Thumbnails'
  base64 is twenty lines of our own. `Cargo.lock` is committed.
- **No HTTPS yet.** An `https://` server (the NAS behind `tailscale serve`,
  [003](003_deployment.md)) is refused with a message. Supporting it means
  a TLS crate, which waits until someone uses that setup.
- **Two crates in one workspace.** `linux/core` holds what needs no display:
  the API client, the database, `Store` and `Sync`, the catalogue and
  search, the cache and the fetcher, lyrics. `linux/app` holds the window,
  the player, MPRIS and the tray. The split exists for the tests, which
  run `cargo test` on `core` without GTK, GStreamer or a display, as
  `DhunKit` is tested apart from the Mac's window.
- **The Flatpak's build fetches the crates** that `Cargo.lock` pins, with
  network access for that one build step. Flathub would want each crate
  listed as a source (`cargo-sources.json`, from flatpak-builder-tools'
  generator); this Flatpak is built on the owner's machine and handed out
  in GitHub releases, so a generator and a second file to keep in step with
  `Cargo.lock` buy nothing (changed in step 1). Sandbox permissions grow
  with the steps: network, IPC, Wayland with X11 as a fallback, the GPU,
  PulseAudio (PipeWire answers it) and the MPRIS name now; the
  StatusNotifierItem watcher and logind's sleep signal with what uses them.
  No file system access: Open With hands files in through the document
  portal.
- **Versioning.** Linux joins the shared sequence (README, Versions). Its
  version lives in `linux/app/Cargo.toml`. Its first release takes the next
  free patch number, for Linux alone. Until then the builds installed to
  try it keep 0.16.3, fixes and all (owner, 2026-10-23): the others reached
  0.16 by being stable, and a version bumped per iteration of a first build
  would say nothing.
- **Releases.** `make linux` builds the Flatpak and archives it in `local/`
  as `dhun-linux-v<version>.flatpak` with its `.sha256`, under the same
  naming rules as `make apk` and `make mac`. A GitHub release carries it;
  `flatpak install --user dhun-linux-v<version>.flatpak` installs it.
- **Tooling.** `cargo fmt` joins `make fmt`; `lint-linux` (`cargo fmt
--check`, `cargo clippy -D warnings`) and `test-linux` (`cargo test`) join
  the `Makefile`, and a CI job runs them on `ubuntu-24.04` with the GTK and
  GStreamer development packages. Where `cargo` is not on the PATH they run
  inside the GNOME SDK through `scripts/sdk.sh`, so the owner's desktop
  needs no Rust or `-dev` packages; a machine with neither (the Mac) leaves
  them out of `make check` and to CI, since its pre-push hook runs `make
check`. `scripts/check-settings.sh` learns the Linux app's copy of the
  synced defaults and the guide (034).
- **Checking the window with no one at the screen.** `DHUN_SCREENSHOT=<png>`
  draws the window to a PNG a few seconds after start and quits;
  `DHUN_OPEN` opens pages once the library has loaded (`artists`,
  `artist:<name>`, `album:<name>`, `search:<words>`, …) and `DHUN_SIZE`
  sets the window's size. Run on GTK's headless Broadway display
  (`gtk4-broadwayd`, in the SDK), nothing appears on the desktop: how an
  agent checks a change to the window. `DHUN_DATA` keeps a build under
  test apart from the installed app's library and sign-in, as on the Mac,
  and runs it beside the installed app rather than handing it over.
- **The speed targets apply.** The app logs `catalogue:` and `search`
  timings under the GLib log domain `Dhun`, as the other apps do, and
  `performance-baseline.md` gets a Linux section at 30,000 and 100,000
  songs. Besides the targets, the Flatpak's own size and the app's memory
  are recorded, so "lightweight" has a number on Linux too.
- **Development is against the production NAS**, signed in with the test
  account in `local/test_account`, never the owner's (owner, 2026-10-23).
  The desktop needs no Docker and no Go for this.

## Plan

Each step ends usable and installed on the owner's desktop. The status line
moves with it.

0. **Toolchain and playback spike.** Install the GNOME SDK with the Rust
   extension and flatpak-builder as user Flatpaks, so the desktop needs no
   system development packages. Then a throwaway player that plays each
   format from a file, from a still-arriving file fed slowly, and from the
   stream. It measures:
   - gapless joins with the Mac spike's split-tone test, for MP3, AAC (index
     at either end), FLAC, Ogg Opus and WebM Opus;
   - speed with `scaletempo`, and pitch alone with `pitch` (and whether the
     runtime has it);
   - a seek into an M4A whose index is at the end, and into a 30 MB WebM
     arriving at 64 KB/s;
   - starting time.

   **Can fail:** a format GStreamer does not trim exactly. Then that format
   gets a trimming step of our own, as the Mac's WebM reader trims Opus
   pre-skip. **Done (2026-10-23)**; the results are under
   [What the spike showed](#what-the-spike-showed-2026-10-23). MP3 needed
   such a step, the tag renamed. The spike stays out of the repository:
   the source element and the fetcher are written again in step 2.

1. **Skeleton.** The workspace, `make linux`, `make linux-install`, the
   Makefile targets and CI. Sign-in, the token file, the schema, the library
   pull, the catalogue (natural sort, groupings, the folder tree, search),
   browsing with covers. Ported tests: the catalogue's. Measured against
   the speed targets before going further. **Done (2026-10-23)**; see
   [How step 1 went](#how-step-1-went-2026-10-23).
2. **Playing.** The player under `Playback`: the 20 queues, the queue panel
   and picker, listens, resume points (009), hand-off (017), MPRIS, pause
   on an output change. **First release**, the first install. **Done
   (2026-10-23)**; see [How step 2 went](#how-step-2-went-2026-10-23). The
   outbox and the push/pull loop came here from step 3: a queue made here
   and a listen are ops, and without them they would never reach the
   server.
3. **Offline.** The song cache with prefetch, then Downloads and pins with
   promotion by rename. Ported tests: downloads. **Done
   (2026-10-23)**; see [How step 3 went](#how-step-3-went-2026-10-23).
4. **The rest of the phone.** Playlist editing (013), Favorites, Listen
   Later and the automatic views (010), podcasts and audiobooks (031),
   bookmarks (033), lyrics (015), speed and pitch (016), the sleep timer
   (014), settings with all four themes and the eight palettes (018, 034),
   family members (023). Ported tests: lyrics, tempo. **Done
   (2026-10-24)**; see [How step 4 went](#how-step-4-went-2026-10-24).
5. **Linux-only pieces.** The tray icon, the mini window, Open With, the
   launcher's three shortcuts, the Background portal. **Done
   (2026-10-24)**; see [How step 5 went](#how-step-5-went-2026-10-24).
6. **Measure and release.** The Linux section of `performance-baseline.md`;
   REQUIREMENTS, README and this plan's status brought up to date. **Measured
   (2026-10-24)**; see [How step 6 went](#how-step-6-went-2026-10-24).

## How step 1 went (2026-10-23)

**Built:** `linux/core` (the API client, the database with the Mac's
schema and migrations, the library pull with the playlist rules of plan
013, covers, the catalogue and search) and `linux/app` (sign-in, the
sidebar with the library's sections and the playlists, albums as a grid of
covers, artists, genres, the folder tree, audiobooks and podcasts, each
page's filter past 12 rows, and search over all of them). The window
collapses to one pane below 640 px. The Flatpak builds; the bundle is
793 KB, 2.5 MB installed, since GTK, libadwaita and SQLite come with the
runtime.

**Checked:** the 15 tests, among them the phone's and the Mac's catalogue
and search cases and `api/search-fold.tsv`, which the Rust fold passes as
the other four copies do. Signed in as the test account against the NAS,
the first pull took 32,434 songs in 1.5 s and the 6,975 thumbnails in
104 s over the desktop's Wi-Fi, once. Every page was looked at in
screenshots taken on Broadway.

**Measured** on the desktop, release build, 31,212 songs:

| What                                                      | Measured                                   | Target      |
| --------------------------------------------------------- | ------------------------------------------ | ----------- |
| Process start to the window                               | 0.8 s                                      | —           |
| Read the songs, build the catalogue (off the main thread) | 120–135 ms, 145–160 ms                     | —           |
| Process start to the library on screen                    | about 1.1 s                                | under 1 s   |
| Song and album search indexes (after, in the background)  | 325–370 ms                                 | —           |
| One keystroke, songs index, "t" to "tum hi ho"            | 6–15 ms; "arjit sing" (misspelt) 15–20 ms  | under 50 ms |
| Memory, nothing signed in, on Wayland                     | 126 MB, all of it GTK and its GPU renderer | —           |
| Memory, the library and the album grid, on Wayland        | about 280 MB                               | —           |
| The library's own data, measured without GTK              | 71 MB (songs 39, catalogue 13, index 13)   | —           |

The Mac takes 723–809 ms to build the same catalogue
([performance-baseline](../research/performance-baseline.md)).

**Changed while building:**

- **The search index keeps its words, sound keys and titles packed** in a
  few large arrays rather than an allocation each. The first build took
  41 MB for the song index, half of it pages freed by the build that could
  not go back to the system because the index's own small allocations sat
  among them; packed, it takes 13 MB, and keystrokes are as fast. The Mac's
  and the phone's indexes hold the same data the old way, which is worth a
  look there.
- **The catalogue shows before the search indexes are built**, in a second
  pass; together they had kept the library off the screen for a third of a
  second more.
- **Songs are read straight into shared pointers**, so building the
  catalogue no longer copies each into an allocation of its own (13 MB
  freed and stranded).
- **Decoded covers are kept up to 32 MB** rather than up to 200 covers,
  which at 256 px came to 50 MB.
- **This machine's settings are a JSON file**, `prefs.json` in the app's
  folder, not GSettings, whose schema would have to be compiled and
  installed beside the app for a handful of values only it reads.

**Not done:** the native install has not been built here, as the desktop
has no GTK `-dev` packages; CI builds and tests against Ubuntu 24.04's.
The sidebar has no Queues, lists or Downloads yet: they come with what
fills them, in steps 2 to 4.

## How step 2 went (2026-10-23)

**Built:** in `linux/core`, the queue, mark, resume, bookmark, setting and
play-count tables; the outbox and the ops each change records (the Mac's
`Store`); the whole sync round, push, pull and apply, with the play
counts; the synced settings' defaults, which `scripts/check-settings.sh`
now compares with the other apps'; the queues' order and play order,
rewinding and chapters as plain functions; and `Part`, a song's bytes
arriving in a file. In `linux/app`, the engine (`playbin3`, our source
element, the four findings of the spike), `Playback` (the Mac's, less the
sleep timer, speed and pitch, and chapters, which come with step 4),
MPRIS, the Now playing bar, the queue panel and picker, the Queues pages,
the hand-off and resume offers, and Play Next and Add to Queue on every
song list. Space plays and pauses, Ctrl+← and Ctrl+→ skip.

**Checked**, on Broadway with `fakesink sync=true` (in time, silent),
signed in as the test account against the NAS:

- An album played, and a seek near the end of its first song went on to
  the second by itself, gaplessly.
- A song heard from 1:40 to its end (3:34) was counted by the server once,
  at the time it was heard.
- The test account's 18 queues arrived, and the one made here reached the
  server.
- MPRIS, asked from the desktop's session bus: the title, artist, album,
  length and cover file; PlayPause from the bus played.
- An audiobook sought to an hour in and the app quit: the queue's place
  and a resume point were kept at 1:00:03.5, sent with the next start, and
  the app came back paused there.
- Another device's playback, reported through the sync API, was offered
  ("Continue from Phone test: “Mast Magan” at 1:01"), and Continue took
  its queue and song and played from there.
- The 21 unit tests, with the outbox's replacing ops, the queues' order,
  what plays next, rewinding, chapters and the waiting reader.

**Changed while building:**

- **A change announced while another is being told reaches everyone.**
  The first version took the listeners out while it called them, so a page
  opened from the catalogue's listener, or a queue restored there, told no
  one; the sidebar showed the wrong section.
- **No borrow is held across a call that can announce a change.** 0.16.3
  as first installed aborted on any click in the sidebar: the click held
  the sidebar's routes while opening the page, and the page's `Section`
  change refilled the sidebar, which borrowed them again. A panic in a GTK
  signal cannot unwind. The test hook opened pages directly, so it never
  saw it; its `nav:N` and `activate:N` steps now click a sidebar row and a
  song row as a mouse does.
- **Each song is fetched as it plays**, into a file that goes when nothing
  reads it any more. The cache that keeps them, and prefetches the next,
  is step 3's.

**Not done:**

- Unplugging headphones from a jack is not noticed: PipeWire changes the
  port, not the device, and GStreamer says nothing. A Bluetooth device
  leaving, or the output moving to another device, pauses.
- Closing the window quits, saving the place, until step 5 keeps the
  music playing.
- A queue's page is built again when the queues change, once a song as it
  plays, and loses its scroll; and Space plays or pauses even with a
  dialog's button focused. Both wait for step 4's queue editing.
- Playback through the speakers was first heard by the owner on
  2026-10-23, after the sidebar fix; every check here was silent.

## How step 3 went (2026-10-23)

**Built:** in `linux/core`, the song cache (`cache`), which keeps every
song played and fetches the next ten of the queue while playing, two on a
metered network, within 3 GB; `Part` grown to keep a whole song under its
own name, to open a downloaded or cached file, to notice a song gone from
the server, and to be a long file's window (037); and Downloads' pins
turned into songs, and one song fetched into Downloads, carried on from
where a broken try stopped. In `linux/app`, Downloads run one song at a
time, a cached song moving there by rename; a Download button on the
pages of an album, folder, artist, genre, playlist, book and show; Download
and Remove Download in the song menu; a Downloads page with each download,
its progress and the songs kept; a mark on a downloaded song; and, when
the server cannot be reached, songs that are not on this computer dimmed.

**Checked**, signed in as the test account against the NAS, silent:

- An album played: the song playing and the three after it were kept
  whole in the cache, byte for byte the server's file.
- That album's folder and one other song were downloaded: the four cached
  songs moved from the cache into Downloads and were not fetched again;
  the other was fetched. The Downloads page showed both and the songs.
- With the server unreachable, a downloaded song and a cached song
  played; songs on neither were dimmed, and one played waits for the
  network as before.
- A 60-minute episode (44 MB) was kept as a window: 24 MB, its first 30
  minutes, and no more. Sought to 45 minutes, the window moved there and
  what it had fetched from the start went back to the disk: 15 MB in all.
- The 27 unit tests, with the window following its reader and forgetting
  what is far behind, and the pins (three ported from the Mac).

**Changed while building:**

- **The cache is shared, not the main thread's.** GStreamer opens the next
  song on its own thread for a gapless start, so the cache answers it
  under a lock. A fetch that ends sets a flag that `Playback`'s tick reads,
  where the Mac's fetch calls back into its main actor.
- **Behind a window, a hole is punched, not the file rolled** (037 rolled
  it, since Android cannot punch). Linux file systems give the blocks back
  in place, so nothing is copied and no reader has to reopen. `libc`
  joins the dependencies for that one call; it was already built, through
  GLib.
- **A cell follows what it shows.** Rows on screen are not bound again
  when only the songs kept here change, so each cell of a song list
  listens for that itself, and the user's selection is kept.
- **A build under test (`DHUN_DATA`) runs beside the installed app.** As
  a second launch of the same app it handed itself to the owner's running
  copy and quit. The test hook gained `play:<song id>`.

**Not done:**

- The limits (3 GB of cache, 10 GB of downloads, none on a metered
  network) are fixed until step 4's Settings, which also show the space
  used.
- The server counts as unreachable only once a sync has failed; a song
  that cannot be fetched does not say so sooner, as the Mac's fetcher
  does.
- Lyrics are not kept with downloads; they come with step 4.

## How step 4 went (2026-10-24)

**Built:** in `linux/core`, the edits `Store` lacked (marks, bookmarks, and
every playlist edit, a move naming its entries rather than its songs, since
a song can be in a playlist twice), the sort orders shared by queues and
playlists, `Tempo`, lyrics (the LRC parser, kept copies, the offline search
of them), `Heard` for books and shows, the synced settings' choices, and
the family-members calls. In `linux/app`: the Lists section (Favorites,
Listen Later and the five automatic views); the phone's song menu (Play
Next, Add to Playing Queue, Add to Queue ▸, Add to Playlist ▸, the two
lists, Mark as Played, Download, Go to Album or Artist, and removing from
the list shown); playlists made, renamed, sorted, deleted, and their songs
removed or dragged into order; a queue's songs dragged, sorted, saved as a
playlist, the queues themselves dragged into order and "Remove all other
queues"; Play and Shuffle on every list's header; Favorite and Listen Later
on the Now playing bar, and for a long file seek back and forward, a
bookmark, and in Shuffle's place for a book or show its chapters, notes and
bookmarks; speed and pitch; the sleep timer, with "Stop after this song" in
the playing queue's menu; lyrics beside the page, following the song, kept
with downloads, and searched; a book's and a show's page as the Mac's
(Continue, Heard, parts or dates, played episodes hidden); and Settings:
the five themes (system, light, dark, black, day and night by the clock)
and the eight accents, the Playback settings that follow the user, this
computer's limits, the account, family members for the admin, and How it
works, which `scripts/guide.sh` now copies into `linux/app/src/guide.md`.

**Checked**, signed in as the test account against the NAS, silent:

- A favourite made from an album's song menu showed in Favorites; a
  playlist made from the menu was created on the server
  (`Playlists/test/Linux test.m3u8`), then added to, sorted, a song
  removed, and renamed, the server's file following each.
- A queue reversed from its Sort menu, the server's copy too.
- 1.5× moved a song 7.5 s in 5 s, and 1× back to 5 s; −2 semitones at
  1.25× opened the song again where it was with `pitch` in the chain and
  went on at 1.25×. The Flatpak's build now has `pitch`
  (`gst-inspect-1.0` in its build directory).
- "At the end of this song" paused at the next song's start, the timer
  done.
- A chaptered episode showed "Chapter 5 of 24 · Perceptronium" under its
  title, and its chapters in the panel, the one playing marked.
- A song's synced lyrics followed it; "baarish ka bosa" found it by its
  lyrics on the server.
- The Settings pages, the black theme with the Teal accent, and the
  Downloads and Account pages' figures.
- The 33 unit tests, among them the playlist move, the tempo setting read
  defensively, the three lyrics tests and a book carrying on from its
  first file not played.

**Changed while building:**

- **A seek waiting for its song to open is dropped when another opens
  first.** Started at once after the app restored a queue paused at 44 s,
  the new song began at 44 s: the restored song's seek landed on it. A
  bug since step 2, found by the speed check.
- **Pages keep their place when built again.** The playing queue's page was
  built again at every song; now only an edit to its name or songs builds
  it again, and a page built again is scrolled back to where it was. Space
  no longer plays or pauses while a dialog is open. (Both were left from
  step 2.)
- **The Flatpak builds libSoundTouch shared** (`BUILD_SHARED_LIBS`): its
  default static library cannot be linked into the plugin.
- **Return in a dialog's text field answers it** by the field's own
  `activate`: a field inside an `AdwAlertDialog` does not reach the
  default button.
- **The screenshot hook draws dialogs and open popovers**, and the test
  hook can drive a song list's menu, type into a dialog and open a menu
  button (`select:`, `do:`, `type:`, `popup:`, `tempo:`, `sleep:`,
  `settings:`).

**Not done:**

- Family members were not tried: the test account is not an admin.
- Dragging a song or a queue was not tried by hand; the playlist move is
  tested, the queue's is not.
- Entries of a playlist that match no song are not shown, as the Mac shows
  them ("Unavailable"); they are kept, and sorting keeps them at the end.
- A book's or a show's filter still says "Filter songs".

## How step 5 went (2026-10-24)

**Built:** the tray icon (`tray.rs`, a StatusNotifierItem and its
`com.canonical.dbusmenu` menu); the mini window (`mini.rs`); Open With
(`openfile.rs`, and the `.desktop` file's `MimeType`, the server's
formats); the launcher's Search, Resume and Shuffle all; and closing the
window hiding it, with the Background portal asked the first time. The ☰
menu gained Mini Player and Quit. MPRIS's `Raise` now shows the main
window, never the mini window or a file's.

**Checked**, signed in as the test account, silent:

- The tray, through the Flatpak's D-Bus filter with only
  `--talk-name=org.kde.StatusNotifierWatcher`: COSMIC's watcher listed it;
  its tooltip and menu answered from the host; Pause from the menu paused
  (MPRIS said so) and the item became Play; Quit ended the app and the
  panel dropped the icon.
- The window closed while a song played: it played on (MPRIS's position
  moved 3 s in 3 s), and `Raise` showed the same window again.
- On a private session bus with a running copy: `dhun --shuffle` started
  "All songs" shuffled, `dhun <file>` paused the queue and opened the
  file's window, `dhun --search` showed the window; each second launch
  exited at once.
- A file opened with Dhun not running: a tagged M4A showed its title,
  artists and embedded cover, and played; an untagged FLAC showed its
  name and played to the end.
- The mini window after Shuffle all: the cover, the song and the controls.

**Changed while building:**

- **COSMIC's portal has no Background interface**, so the request fails
  there, quietly. Nothing is lost: COSMIC does not stop a Flatpak with no
  window. GNOME's and KDE's portals have it.
- **The screenshot hook draws any window**, named by `DHUN_SCREENSHOT_OF`
  (`mini`, `file`), and starts with the app, so a file opened without the
  library can be drawn. The test hook gained `app:<action>` and `close`.

**Not done:**

- Seen only through D-Bus and screenshots: the tray icon's look on the
  panel, the mini window pinned with "Always on top", and Open With from
  COSMIC Files are for the owner to try.
- The shortcuts' names in the launcher's right-click menu are the
  `.desktop` file's; whether COSMIC's dock shows `Actions` is untried.

## How step 6 went (2026-10-24)

**Measured** through the installed Flatpak, at today's 31,239 songs and
at 100,000 made from them; the numbers are in
[performance-baseline.md](../research/performance-baseline.md#2026-10-24-the-linux-app-with-every-feature-plan-039-step-6).
Keystrokes are within 50 ms at both sizes. The library is drawn 1.0–1.2 s
after launch today, just over the 1 s target, and 1.9 s at 100,000 songs;
the catalogue is built off the main thread, and what is left is the views
made again on it and GTK's first layout of them.

**Fixed while measuring:**

- The queue panel made all its rows again on every play, pause and song
  change; with "All songs" as the queue that was 75 ms each, 250 ms at
  100,000. Now only another queue or other songs remake them.
- A list drawn before the thumbnails arrived kept its placeholders: GTK
  does not bind a row again for the same item object. A placeholder cover
  now waits for the thumbnails itself, and `rebind_on_thumbs` is gone.

**Left for later**, in the baseline: Next and Previous hold the main
thread 80–185 ms while playbin3 goes to READY; an untraced 0.2–0.3 s
pause during the first sync; 70 MB more memory than step 1, mostly
decoded thumbnails; the native build unmeasured.

## A pass over every feature (2026-10-24)

Every page, list, popover and Settings page was drawn signed in as the test
account. Play, pause, next, previous, seek, the position and the launcher's
shortcuts were driven over MPRIS. Themes were checked on the owner's
desktop, where COSMIC sets a system accent. The Android app was read
alongside, for what Linux lacks.

**Fixed:**

- **The accent and the Black theme did nothing.** Since libadwaita 1.6 the
  stylesheet paints with CSS variables (`--accent-bg-color`), so the
  `@define-color` overrides changed nothing on screen and the system's accent
  showed instead. They now set the variables.
- **The Now playing bar did not fit.** Below about 1,050 px the seek bar
  shrank to a sliver, and below 920 px the window was cut off. Long titles
  now ask for less room, and two breakpoints keep everything inside: below
  960sp the times go; below 800sp the window shows one pane and the bar
  becomes the phone's mini bar (the song, Previous, Play, Next, the queue).
- **With no sound output the queue ran away.** With no output (no
  PulseAudio, a device that refuses), every song failed in turn and was
  passed over, about 30 a second, the queue's place moving and synced as it
  went; a queue restored paused did it at launch. An output's failure now
  stops where it is, with a note. A song that fails while paused stays put,
  and one failure told several times counts once. Only a song's own
  failure, while playing, moves on, as on the phone.
- **Sync on waking from sleep** (logind's `PrepareForSleep`), which the
  Decision promised, was missing; the Flatpak now talks to `login1`.
- **Sign Out in the ☰ menu** signed out at once; it now asks, as Settings does.
- **Play and Shuffle** were missing on a folder's page (every song under it)
  and on the user's own playlists.
- **Search results had no song menu**; songs, episodes and lyrics hits now
  have the phone's.
- A smaller song-cache limit now trims the cache at once.

**Missing against the phone, then built:**

- **Recent searches and the empty field's help** (029). A click into the
  empty search field, or Ctrl+F, opens the search page on the last ten
  searches, each with ✕, and Clear; with none yet, what the field
  searches and its `artist:` and other words. A search is kept when
  something it found is opened or played. They are this computer's, kept
  through a sign-out, as on the phone. They are a page rather than the
  Mac's suggestions under the field: a popover under the field would take
  the focus from it.
- **Signing out asks whether to keep the downloads** (026), from Settings
  and from the ☰ menu: Keep Downloads or Delete Downloads, with how many
  songs and how much room; with none, just Sign Out. Kept, the files, their
  pins and their lyrics stay, and the song cache goes. Unlike the phone they
  do not play while signed out: Linux has no songs of its own, so there is
  nothing to use without an account. Whoever signs in next takes them over:
  someone else gets each file as a download of its own song, and the pins
  of the leaver's playlists and lists go; the library's pins (album, folder,
  artist, genre, a book or show) stay. Until that sign-in's first sync, no
  file is deleted for want of a pin. Signing in as someone else keeps the
  downloads too, as on the phone, where it used to delete them. They stay
  whatever the server's address, since one server is reached at two; a
  file whose song the server lacks, or holds at another size, goes or is
  fetched again, as any download.
- **The "downloaded" mark** (012) after the name of a downloaded album (in
  the grid), folder, artist, genre, book or show, and playlist (in the
  sidebar), and of such a result in search, as on the phone. It shows the
  download asked for, whether or not every file is here yet; the page's
  Download button says how far along it is.
- **The thin resume bar** (009) under a long file's title in every song
  list, as far along as its resume point; none under 1%, which every
  episode played a moment has. The resume points are kept in memory with
  the marks, so a list reads them without asking the database at each row,
  and a pause moves the bar at once.
- **A queue's search finds rather than filters** (032), as Musicolet's and
  the Mac's: "Find in queue" lists the matches under it with their number
  in the queue; ↑ and ↓ move through them, Return or a click picks one,
  and the queue scrolls to it, in the middle, selected. The queue stays
  whole, so a song can be dragged with its neighbours in view. GTK's
  `scroll_to` cannot centre a row, so the list is scrolled by hand first:
  its rows are of one height.
- **The playing-queue panel has the song menu and dragging** (020), as a
  queue's page has: a right-click opens the queue's song menu (Stop After
  This Song and Remove from Queue among it), and a song dragged onto
  another takes its place.

**Still missing against the phone:** renaming and removing from the
queue picker; the artist link under the title on the bar; Play and Shuffle
on Downloads, which also leaves out downloaded episodes.

## Open questions

None for the owner. The spike settled the Flatpak's `pitch` plugin: it
builds its own.
