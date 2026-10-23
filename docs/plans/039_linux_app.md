# 039 — The Linux app, at the Android app's level

**Status:** `ACCEPTED` — Rust, GTK4 and Flatpak chosen (owner, 2026-10-23); no code yet
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

| Android                                                                               | Linux                                                                                                                              |
| ------------------------------------------------------------------------------------- | ---------------------------------------------------------------------------------------------------------------------------------- |
| Room tables, `Store` (local write + outbox op), `Sync`                                | The same tables in SQLite, ported unchanged in behaviour                                                                           |
| `Playback`: 20 queues, listens, resume points, hand-off                               | Ported; GStreamer underneath (below)                                                                                               |
| `SongSource`: download → cache → growing `.part` → stream                             | The same order                                                                                                                     |
| Downloads, the song cache with prefetch, covers                                       | The same, under the app's data folder                                                                                              |
| Sleep timer, speed and pitch, lyrics, bookmarks (033)                                 | The same                                                                                                                           |
| Podcasts and audiobooks ([031](031_podcasts_and_audiobooks.md))                       | The same                                                                                                                           |
| Tabs, mini bar or floating pill                                                       | A sidebar window with a Now playing bar, as the Mac; it narrows to the phone's tabs in a small window. A small mini window (below) |
| Media session, notification, headset controls                                         | **MPRIS** on D-Bus: media keys, the panel's media controls (COSMIC, GNOME, KDE), Bluetooth headset buttons                         |
| Pause when headphones are unplugged                                                   | Pause when the output device goes away                                                                                             |
| `PlayFileActivity` (open from other apps)                                             | **Open With** from the file manager, in a small window of its own                                                                  |
| Family members (admin only, [023](023_users_on_android.md))                           | The same, under Settings                                                                                                           |
| Light, dark, **black** and **by-time** themes; 8 palettes                             | All four, and the eight palettes as the accent. The Mac left out Black and by-time; Linux can do both cheaply, so it has them      |
| Wi-Fi only for downloads; prefetch 2 on mobile, 10 on Wi-Fi                           | "Metered network" (GIO's `network-metered`) in place of mobile data, as the Mac's "expensive network"                              |
| App shortcuts: Search, Resume, Shuffle all ([038](038_voice_search_and_assistant.md)) | The same three, as `Actions` in the `.desktop` file (right-click on the launcher icon)                                             |
| Voice search, "play it on Dhun" (038)                                                 | **Left out.** Linux has no system speech recogniser and no assistant to hand a request to                                          |
| The headset button with no player running (038)                                       | **Left out.** MPRIS exists only while the app runs; nothing on Linux remembers a player to start                                   |
| Phone-local songs ([025](025_phone_local_songs.md))                                   | **None** (owner, 2026-10-23)                                                                                                       |
| The fast scroller ([035](035_fast_scroller.md))                                       | **Left out**, as on the Mac and the web: a desktop scrollbar can already be dragged                                                |

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

- **A. GStreamer**, `playbin3` fed by an `appsrc` in random-access mode.
  Chosen. GStreamer is in the GNOME runtime and on every desktop, and it
  plays MP3, AAC/M4A, FLAC, Ogg Opus and WebM Opus (`matroskademux`) as
  they are. Our `appsrc` reads from a download, a cache file or a cache
  file still arriving, and its `seek-data` signal is the Mac's read
  callback: a read past what has arrived waits for it and tells the fetcher
  where it waits. So, as on the Mac, one path serves all four sources.
  `about-to-finish` queues the next song for gapless playback, and
  `scaletempo` changes the speed while keeping the pitch.
  - **Pitch alone** needs GStreamer's `pitch` element, which lives in
    gst-plugins-bad and links libsoundtouch. Ubuntu's packages include it;
    the GNOME runtime may not. If it does not, the Flatpak manifest builds
    libsoundtouch and that one plugin as modules of its own, a few hundred
    kilobytes. The spike settles which.
  - **Gapless trimming** (the LAME header in MP3, iTunes' `iTunSMPB` and
    edit lists in M4A, Opus pre-skip) is left to GStreamer's parsers. The
    spike measures each format with the Mac spike's split-tone test before
    anything is built on it.
- **B. libmpv.** Rejected. mpv plays gapless and has a good time-stretcher,
  but it brings FFmpeg, which the GNOME runtime does not ship. The Flatpak
  would carry tens of megabytes of codecs that GStreamer already provides.
- **C. Symphonia (pure-Rust decoders) with PipeWire or cpal for output.**
  Rejected. Symphonia has no Opus decoder, and we would own the resampler,
  the time-stretcher and the gapless joins: thousands of lines that
  GStreamer has already debugged.

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
  the lyrics. Below a width breakpoint the window becomes the phone's tabs
  with its mini bar, so a narrow window stays usable. Keyboard: Space plays
  or pauses, Ctrl+← and Ctrl+→ skip, Ctrl+F searches, Ctrl+Q quits.
- **Closing the window does not stop the music** (owner, 2026-10-23). The
  app holds itself running, and asks the Background portal for leave to
  do so, so a Flatpak is not killed on close. Opening the app again from
  the launcher brings back the same window.
- **The tray icon** is a StatusNotifierItem, which COSMIC and KDE show
  (GNOME needs its AppIndicator extension). Its menu: play or pause,
  previous, next, the current song, Show Dhun, Quit. Written over GIO's
  D-Bus, about 200 lines, with no crate.
- **The mini window** is a small window with the cover, title and controls,
  opened from the menu. A Wayland app cannot keep itself above other
  windows, so the owner pins it with the desktop's own "Always on top"
  window menu item. This replaces the Mac's floating mini window and the
  phone's pill.
- **MPRIS** (`org.mpris.MediaPlayer2.dhun`) over GIO's D-Bus: play, pause,
  next, previous, seek, position, cover and title, and `Raise` to show the
  window.
- **Pause when the output goes away**: a `GstDeviceMonitor` on audio sinks;
  when the sink Dhun plays to disappears (headphones out, Bluetooth gone),
  playback pauses, as on the phone.
- **Open With.** The `.desktop` file declares the audio types the server
  indexes. A file opened from the file manager plays in a small window of
  its own, through a second player. It makes no queue and logs no listen;
  the playing queue pauses where it is (021).
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
  the system's SQLite (not its bundled copy); `ureq` for HTTP, with its TLS
  off because the NAS is reached over plain HTTP, on worker threads so no
  async runtime joins; `serde` and `serde_json`. D-Bus, the network monitor
  and file paths come from GIO, so `zbus`, `dirs` and the like stay out.
  `Cargo.lock` is committed.
- **Two crates in one workspace.** `linux/core` holds what needs no display:
  the API client, the database, `Store` and `Sync`, the catalogue and
  search, the cache and the fetcher, lyrics. `linux/app` holds the window,
  the player, MPRIS and the tray. The split exists for the tests, which
  run `cargo test` on `core` without GTK, GStreamer or a display, as
  `DhunKit` is tested apart from the Mac's window.
- **The Flatpak builds offline**, as Flatpak requires: the crates come from
  a generated `cargo-sources.json` (flatpak-builder-tools' cargo
  generator), committed beside the manifest and regenerated when
  `Cargo.lock` changes. Sandbox permissions: network, Wayland with X11 as a
  fallback, PulseAudio (PipeWire answers it), the GPU, the MPRIS name, the
  StatusNotifierItem watcher and logind's sleep signal. No file system
  access: Open With hands files in through the document portal.
- **Versioning.** Linux joins the shared sequence (README, Versions). Its
  version lives in `linux/app/Cargo.toml`. Its first release takes the next
  free patch number, for Linux alone.
- **Releases.** `make linux` builds the Flatpak and archives it in `local/`
  as `dhun-linux-v<version>.flatpak` with its `.sha256`, under the same
  naming rules as `make apk` and `make mac`. A GitHub release carries it;
  `flatpak install --user dhun-linux-v<version>.flatpak` installs it.
- **Tooling.** `cargo fmt` joins `make fmt`; `lint-linux` (`cargo fmt
--check`, `cargo clippy -D warnings`) and `test-linux` (`cargo test`) join
  the `Makefile`, and a CI job runs them on `ubuntu-24.04` with the GTK and
  GStreamer development packages. `scripts/check-settings.sh` learns the
  Linux app's copy of the synced defaults and the guide (034).
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
   stream, through `appsrc`. It measures:
   - gapless joins with the Mac spike's split-tone test, for MP3, AAC (index
     at either end), FLAC, Ogg Opus and WebM Opus;
   - speed with `scaletempo`, and pitch alone with `pitch` (and whether the
     runtime has it);
   - a seek into an M4A whose index is at the end, and into a 30 MB WebM
     arriving at 64 KB/s;
   - starting time.

   **Can fail:** a format GStreamer does not trim exactly. Then that format
   gets a trimming step of our own, as the Mac's WebM reader trims Opus
   pre-skip. The results are written into this plan before step 1.

1. **Skeleton.** The workspace, `make linux`, `make linux-install`, the
   Makefile targets and CI. Sign-in, the token file, the schema, the library
   pull, the catalogue (natural sort, groupings, the folder tree, search),
   browsing with covers. Ported tests: the catalogue's. Measured against
   the speed targets before going further.
2. **Playing.** The player under `Playback`: the 20 queues, the queue panel
   and picker, listens, resume points (009), hand-off (017), MPRIS, pause
   on an output change. **First release**, the first install.
3. **Offline.** The outbox and the push/pull loop with back-off; a revoked
   token keeps unsent edits. The song cache with prefetch, then Downloads
   and pins with promotion by rename. Ported tests: queue order, downloads,
   the waiting reader.
4. **The rest of the phone.** Playlist editing (013), Favorites, Listen
   Later and the automatic views (010), podcasts and audiobooks (031),
   bookmarks (033), lyrics (015), speed and pitch (016), the sleep timer
   (014), settings with all four themes and the eight palettes (018, 034),
   family members (023). Ported tests: lyrics, tempo.
5. **Linux-only pieces.** The tray icon, the mini window, Open With, the
   launcher's three shortcuts, the Background portal.
6. **Measure and release.** The Linux section of `performance-baseline.md`;
   REQUIREMENTS, README and this plan's status brought up to date.

## Open questions

None for the owner. Whether the Flatpak builds its own `pitch` plugin is
for the spike to answer.
