# Performance baseline

Where Dhun stands against the speed targets in
[REQUIREMENTS](../REQUIREMENTS.md#lightweight-and-fast), measured 2026-10-20
at `0.15.0`. Rerun it when a change touches the scan, the catalogue or
search, and add a dated section rather than editing this one.

## How it was measured

- **Libraries:** two generated ones, shaped like `Music/Library/`:
  `<Artist>/<Album> (<Year>)/1-TT - <Title>.mp3`, a `cover.jpg` per album,
  half the files with embedded art, a third with an `.lrc`, a fifth of the
  titles in Devanagari. 27,000 songs (1.7 GB) and 100,000 songs (6.1 GB).
  Each file is two seconds of audio, so tag reading is realistic and
  hashing is not.
- **Server:** the `dhun:dev` image, limited to 4 CPUs like the NAS, on the
  Linux desktop's SSD with a warm page cache. The NAS reads from HDDs, so
  its scans are slower: plan 003 records 3.5 minutes for the first scan of
  7,000 real songs, about 30 ms a file against 1 ms here.
- **Memory** is the process's resident set (`VmRSS`, peak `VmHWM`). Docker's
  figure is far higher because it counts the page cache of every file read.
- **Web client:** `catalog.js` and `search.js` run in Node 24 on the desktop,
  fed the 100k server's own `/library` response, sliced to 27,000 for the
  smaller row. A phone browser is several times slower than this.
- **Not measured:** the Android and macOS apps, and the real NAS. No phone,
  JDK or Mac was at hand, and `local/test_account` is not on the desktop.

## Results

| What                                   | 27,000 songs | 100,000 songs | Target               |
| -------------------------------------- | ------------ | ------------- | -------------------- |
| First scan                             | 25 s         | 1 min 42 s    | (once; not a target) |
| Rescan, nothing changed                | 0.5 s        | 2.4 s         | a few seconds ✅     |
| Server memory, steady / peak           | 70 / 129 MB  | 346 / 456 MB  | under 256 MB ❌ 100k |
| Database                               | 15 MB        | 54 MB         | —                    |
| `/library` first sync, size (gzip/raw) | 0.8 / 13 MB  | 2.8 / 49 MB   | —                    |
| `/library` first sync, server time     | 0.4 s        | 1.5 s         | —                    |
| `/library`, nothing changed            | 10 ms        | 37 ms         | —                    |
| Lyrics search                          | 5 ms         | 25 ms         | under 50 ms ✅       |
| Web: build the catalogue at start      | 0.45 s       | 1.6 s         | under 1 s ❌ 100k    |
| Web: song index, first search          | 0.64 s       | 2.3 s         | under 1 s ❌ 100k    |
| Web: one keystroke, mean / worst       | 23 / 52 ms   | 75 / 105 ms   | under 50 ms ❌       |

The gzip size is optimistic: generated titles repeat a small vocabulary.
Real tags compress about fivefold (see `writeJSON`), which puts the real
first sync nearer 3 MB at 27,000 songs and 10 MB at 100,000.

## What the numbers say

1. **The server is fine at today's size and has headroom.** The scan only
   reads changed files, so a growing library costs time once per new song.
   Memory is what grows: steady memory went from 70 MB to 346 MB at 100k,
   after one first sync and one rescan. Two likely reasons, not yet
   profiled: `writeJSON` builds the whole response in memory (about 50 MB
   at 100k, more while encoding), and no `GOMEMLIMIT` is set, so the Go
   runtime keeps freed memory.
2. **The clients are the bottleneck.** Every web start builds the whole
   catalogue again, and the first search builds the song index. At 27,000
   songs that is about 1 s on the desktop, before a phone's slower CPU.
   Typing already misses 50 ms at its worst. In the profile, the largest
   single cost is `natural()` in `util.js`, the sort comparator. It splits
   both strings into arrays on every comparison, and a sort of n titles
   makes about n log n comparisons. Next are `words()` and `devanagari()`,
   which fold each field for the index.
3. **Android and the Mac use the same design, unmeasured.** They keep the
   whole catalogue, sort it and build the same search index (plan 029).
   Android's `Catalog` is rebuilt from scratch each time the songs table or
   the phone's own songs change (`App.kt`). Its comparator does not
   allocate as the web's does, but the size of the work is the same.

## Next measurements

- The same catalogue and search code on the owner's phone and Mac: time
  `Catalog(...)`, the song index and a typed query, at 30,000 and 100,000
  songs.
- App cold start to a usable library list, on the phone.
- The NAS's own `scan done … took=` lines for the real 30,000 songs.

## 2026-10-20, after plan 036's server changes

Measured on the Mac (Apple Silicon), not the Linux desktop, so only the
before/after pairs compare; the rows are not comparable with the table above.

**`/library` first sync at 100,000 songs**, generated rows inserted straight
into the database, gzip on, the Go heap sampled every 2 ms during the request
([plan 036](../plans/036_fast_at_100k.md)):

| Server                   | Peak heap in use | Heap after the request | Allocated | Time   |
| ------------------------ | ---------------- | ---------------------- | --------- | ------ |
| 0.15.1, whole response   | 265 MB           | 65 MB                  | 649 MB    | 0.41 s |
| Plan 036, one row a time | 10 MB            | 4 MB                   | 110 MB    | 0.41 s |

The 65 MB the old server kept after the request is `encoding/json`'s pooled
encode buffer, which explains why the baseline's steady memory grew with the
library. The image now also sets `GOMEMLIMIT=192MiB`.

**The web client at 27,000 and 100,000 songs**, the same generated
libraries and `catalog.js`/`search.js` under Node 24 on the Mac, the second
run of each. Before is `0.15.1`, after is plan 036: one sort key per item,
the index built from each distinct string once and keeping only word ids,
the title and a numeric year.

| What                         | 27,000 before → after | 100,000 before → after |
| ---------------------------- | --------------------- | ---------------------- |
| Build the catalogue          | 183 → 74 ms           | 685 → 318 ms           |
| Song index, cold             | 161 → 40 ms           | 568 → 220 ms           |
| Index heap                   | 46 → 7 MB             | 168 → 23 MB            |
| One keystroke, "ari", median | 4.3 → 4 ms            | 20.8 → 15.6 ms         |
| One keystroke, "tum hi ho"   | 3.7 → 4 ms            | 18.5 → 14.4 ms         |

Old and new `search()` return the same ids and scores on the queries tried.
The index is now built in idle time right after the catalogue, so the first
keystroke no longer pays for it, and the page paints before the catalogue
is built. Android and the Mac got the same changes and are still unmeasured
on their devices.

**The web client against the NAS**, 2026-10-20: the checkout's `server/web`
served from a local proxy in front of the NAS (`server-v0.15.0`), signed in
as the test account, Chrome on the Mac, driven and timed through Time
Machine. The library was 31,360 songs, 858 of them episodes and book files,
and 5,488 albums.

| What                                                    | Measured      |
| ------------------------------------------------------- | ------------- |
| First sync, `/library?since=0`, raw / on the wire       | 21 MB / 1.1 s |
| Warm reload: page painted                               | 0.1 s         |
| Warm reload: albums page drawn, library usable          | 0.8 s         |
| One keystroke, typing "tum hi ho", including the draw   | 26–43 ms      |
| One keystroke, "arjit sing" (misspelt, the typo pass)   | 50 ms         |
| Console errors across sign-in, browsing, search, reload | none          |

The reload numbers are the main thread's long tasks since navigation: the
cached library lands and the catalogue is built in the first, the albums
page is drawn in the second. The keystroke figures are the synchronous cost
of each `input` event in the real page, results drawn included. The browser
run also caught a start-order race that the shell-first change had opened:
a sync triggered while the kept copy was still loading would have merged
the server's changes into an empty catalogue. The sync now waits for the
load.

## 2026-10-20, the apps on their devices

Both apps now log how long the catalogue takes, so these can be rerun:
Android prints `catalogue:` and `search` lines under the logcat tag `Dhun`,
the Mac prints `catalogue:` under the log subsystem `io.github.vivekg7.dhun`
(`/usr/bin/log stream --level info --predicate 'subsystem == "io.github.vivekg7.dhun"'`,
the full path because `log` is also a zsh builtin). Both were signed in as
the test account against the NAS: 29,359 music songs plus 858 episodes and
book files.

**Mac**, the plan 036 build, release, Apple Silicon, on its own data folder
(`DHUN_DATA`), five launches:

| What                            | Measured   |
| ------------------------------- | ---------- |
| Read the songs from SQLite      | 52–62 ms   |
| Build the catalogue             | 723–809 ms |
| Song and album search indexes   | 230–266 ms |
| First sync of the whole library | about 16 s |

The build is the one to look at next: three quarters of a second for
29,000 songs is within the 1 s target with the read and the index, but
not at 100,000. The web does the same work in 74 ms at 27,000 in Node, so
the Swift `sortKey` and groupings deserve a profile.

**Android**, the plan 036 build and the released 0.15.1, both release
builds, on the Pixel emulator (API 36, Apple Silicon host). Cold starts
were screen-recorded at 20 frames a second with Albums tapped 0.4 s after
launch; the time is from the app's first frame to the grid settling, so
it includes the tap.

| What                                                 | 0.15.1    | Plan 036   |
| ---------------------------------------------------- | --------- | ---------- |
| Cold start to the albums grid                        | 0.9–1.0 s | 0.7–1.0 s  |
| Catalogue built (29,359 songs, off the main thread)  | —         | 240–362 ms |
| Groupings and song index warmed, off the main thread | —         | 0.9–1.0 s  |
| One keystroke, songs index, "t" to "tum hi ho"       | —         | 7–28 ms    |
| One keystroke, "arjit sing" (misspelt)               | —         | 9 ms       |

The emulator does not show the phone's own CPU, and at 29,000 songs the
two builds reach the grid in about the same time; the difference the
plan was after is in what the main thread does afterwards, which the
emulator video cannot show. The keystroke lines also show each query
searched twice in a row on some keystrokes (a recomposition asks again),
cheap at these numbers but worth a look. No errors or crashes in logcat
across sign-in, browsing, search and playback on either build.

## 2026-10-23, the Linux app (plan 039, step 1)

The first Linux build, release, on the Linux desktop, signed in as the test
account against the NAS: 31,212 songs once the missing are left out. The
Linux app logs `catalogue:` and `search indexes:` under the GLib log domain
`Dhun` (`G_MESSAGES_DEBUG=Dhun`).

| What                                               | Measured                                 |
| -------------------------------------------------- | ---------------------------------------- |
| First pull of the library, 32,434 songs            | 1.5 s                                    |
| Read the songs from SQLite                         | 115–135 ms                               |
| Build the catalogue                                | 145–165 ms                               |
| Song and album search indexes, after the catalogue | 315–370 ms                               |
| Process start to the window, then to the library   | 0.8 s, then about 1.1 s                  |
| One keystroke, songs index, "t" to "tum hi ho"     | 6–15 ms                                  |
| One keystroke, "arjit sing" (misspelt)             | 15–20 ms                                 |
| Memory, nothing signed in / library and album grid | 126 MB / about 280 MB, on Wayland        |
| The library's own memory, without GTK              | 71 MB: songs 39, catalogue 13, index 13  |
| The Flatpak, as a bundle / installed               | 793 KB / 2.5 MB, on the GNOME 51 runtime |

The catalogue build is five times faster than the Mac's for the same
songs, which supports profiling the Mac's `sortKey` and groupings, as the
section above suggests. Memory is the number to watch: GTK's GPU renderer
alone is 126 MB, and decoded covers add up to 32 MB. 100,000 songs are
measured below; the native build is not.

## 2026-10-24, the Linux app with every feature (plan 039, step 6)

The same desktop and test account, now 31,239 songs, with steps 2 to 5 in
(the queue panel, playback, the cache, the tray). The release binary ran in
the installed Flatpak's sandbox on a data folder of its own (`DHUN_DATA`),
three launches each. For 100,000 songs the test account's songs were copied
with new ids and " (2)", " (3)" added to titles and albums, so the albums
grow too, then opened offline. Times are from the launch; "drawn" is when
the main thread is next free after the catalogue, found by a 20 ms GLib
timer that logs any longer gap. Measure through the installed app: in the
bare GNOME SDK, GTK's GPU renderer held the first frame for about a second
more.

| What                                            | 31,239 songs         | 100,000 songs        |
| ----------------------------------------------- | -------------------- | -------------------- |
| Window shown                                    | 0.37–0.42 s          | 0.41–0.42 s          |
| Songs read from SQLite / catalogue built        | 155–180 / 150–160 ms | 425–440 / 495–505 ms |
| Library drawn, the albums grid and the queue    | 1.0–1.2 s            | 1.85–1.87 s          |
| Search indexes ready                            | 1.35–1.45 s          | 2.46–2.50 s          |
| One keystroke, "t" to "tum hi ho", "arjit sing" | 6–17 ms              | 17–37 ms             |
| Memory, signed out / library drawn / playing    | 95 / 352 / 362 MB    | — / 427 / — MB       |
| The library's own memory, without GTK           | 72 MB                | 201 MB               |
| The Flatpak, as a bundle / installed            | 1.4 MB / 4.6 MB      |                      |

**Against the targets:** a keystroke is well under 50 ms at both sizes. At
today's size the library is drawn in 1.0–1.2 s, just over the 1 s
target; at 100,000 songs it is well over (1.9 s, search at 2.5 s).
The Mac builds the same catalogue about five times slower, so this is
not the platform furthest behind.

**Where the time goes at 31,000 songs**, after the catalogue is built off
the main thread: 250–290 ms of views made again on the main thread (the
page 130 ms, the queue panel 80 ms, 560 thumbnails decoded at about
0.18 ms each, since GTK binds up to 200 rows of a list before it knows
their height), then 110–170 ms of GTK's first layout of them. The read
from SQLite is about 5 µs a row and the catalogue build another 5;
both grow with the library, the views do not.

**Found and fixed while measuring:**

- The queue panel made all its rows again on every play, pause and next
  song, and three times at start: 75 ms at 31,000 songs with "All songs"
  as the queue, about 250 ms at 100,000. Now only a new queue or new
  songs make them again; the playing mark moves by replacing two rows.
- Thumbnails that arrived after a list was drawn never showed in it: the
  list was told its rows had changed, but GTK keeps a row whose item is
  the same object. A cover drawn as a placeholder now waits for the
  thumbnails itself.

**Not fixed, for the next change:**

- Next and Previous held the main thread for 80–185 ms; fixed in 0.17.1
  (below).
- About 1.4 s after launch, online, the main thread stops for 210–290 ms
  while the first sync runs; not yet traced.
- Memory rose by 70 MB since step 1, most of it decoded 128 px
  thumbnails (64 KB each) for the rows GTK binds up front.
- The native build is not measured.

## 2026-10-24, the Linux app's resources over a session (0.17.0)

The installed 0.17.0 Flatpak, release, on the Linux desktop (i5-8250U,
Intel graphics, COSMIC on Wayland), signed in as the test account on a data
folder of its own, 31,239 songs. Sound went to a `fakesink` that keeps
time, so decoding is real and nothing was heard. Each run started from
the same synced folder and was sampled every second from `/proc`: CPU as a
share of one core, the resident set, wakeups (context switches), reads
and writes, and the GPU's time and memory from the i915 `fdinfo`. Pages
were opened over D-Bus (`win.go-album`, `win.go-artist`) and songs skipped
over MPRIS. Scrolling and typing need test steps the release lacks, so
those runs used a release build of the same commit with two added steps,
run in the app's own sandbox; idle there matched the installed app.

| Situation                                         | CPU, mean / peak | Memory       | Other                         |
| ------------------------------------------------- | ---------------- | ------------ | ----------------------------- |
| Signed out, window open                           | 0 %              | 92 MB        |                               |
| First sync, empty folder                          | 146 % for 2 s    | 287 MB       | library in 2.4 s              |
| … then the album thumbnails                       | 10–20 % for 50 s | 287 MB       | 4 MB/s written; DB 64 MB      |
| Idle, the albums grid on screen                   | 0.0 %            | 303 MB       | 4 wakeups/s, no I/O, no GPU   |
| Idle, closed to the tray                          | 0.1 %            | 298 MB       | 4 wakeups/s                   |
| Playing, streamed, window open / closed           | 5.2 / 4.6 %      | 326 / 319 MB | 180 wakeups/s; caches ahead   |
| Playing from the cache / in the mini player       | 5.6 / 6.2 %      | 326 / 324 MB |                               |
| Playing at 1.25×, +2 semitones                    | 7.2 % / 34 %     | 325 MB       |                               |
| Next every 3 s through a shuffle, 26 songs        | 15 % / 46 %      | 362 MB       | 4 MB/s into the cache         |
| Downloading a 12-song album, 101 MB               | 5–17 % for 30 s  | 243 MB       | then 0 %                      |
| Typing "arijit singh" a key every 150 ms, 5 times | 2.5 % / 11 %     | 302 MB       | no growth                     |
| Scrolling the albums grid, 3,000 px/s             | 83 % / 115 %     | 337→389 MB   | GPU 37 ms/s, 55→138 MB        |
| Scrolling the albums grid, 1,000 px/s             | 97 % / 111 %     | 314→407 MB   | 22 MB/s read, 5,000 wakeups/s |
| Scrolling the artists list, 3,000 px/s            | 42 % / 47 %      | 287→320 MB   | GPU 66 ms/s, 55→268 MB        |
| Two album or artist pages a second, 90 s          | 53 % / 102 %     | 302→460 MB   |                               |
| Four a second, 240 pages, then 90 s idle          | 42 % / 105 %     | 305→1,228 MB | not given back                |

**Idle and playback are good:** nothing runs while idle, in the window or
the tray, and playing costs about 5 % of one core. The rest of the memory
is GTK's renderer (92 MB signed out) and the library (about 72 MB).

**Found and fixed, in 0.17.1:**

- **A leak on navigation.** A page pushed on the stack and then dropped by
  `NavigationView::replace` was never freed (popped, it is), so every "Go
  to Album" or "Go to Artist", and every sidebar section opened from an
  album or artist page, kept 2–5 MB until the app quit. `Nav` now pops the
  pushed pages, without animation, before it replaces the stack. A minute
  of opening the grid and pushing an album went from 305→430 MB to
  303→316 MB, and the 240 pages from 1,228 MB to 511 MB. What is left is
  the 128 px thumbnails, 64 KB each decoded, kept without a limit and now
  to 1,000, and the 32 MB of covers.
- **"Go to Album" and "Go to Artist" took 370–430 ms of the main
  thread.** Most went on the albums or artists page built again under the
  pushed one: 20–40 ms to build, 60–80 ms to put in the stack, the rest to
  lay out a grid nobody saw. `Nav::open_at` now keeps the section already
  at the bottom of the stack, and otherwise builds it only when Back shows
  it. From Albums an album opens in 25–40 ms, from another section in
  70–150 ms; 240 pages at four a second take a third of the CPU and end at
  459 MB. An artist with many albums took up to 900 ms more, for the strip
  of them over the songs, built at once (Alka Yagnik's 1,179 albums,
  280 ms before any layout); it is built as it scrolls now, 30 at a time,
  in 5 ms.
- **The albums grid was slow to scroll and to open.** GTK keeps 32 rows of
  cells at the grid's most columns (385 at 12 for the 12 on screen), and
  while scrolling gives each cell another album at every frame, some 1,200
  binds a second. Each bind asked for its 256 px cover; with 128 kept in
  memory, nearly every one was read from disk and decoded again: 22 MB/s
  and a whole core. A cell now shows the cover only from memory, else the
  thumbnail, and the cells on screen ask for theirs 150 ms after the grid
  stops; the most columns is what fits, plus one. A scroll at 1,000 px/s
  went from 110 % of a core to 42 % on average, with almost nothing read;
  a fast one from 100 % to 55 %, and its memory from 524 to 259 MB.
  Opening the grid makes 161 cells, not 385, and asks for 12 covers, not
  384: 200 visits took 40 % of a core, not 80 %, reading 2 MB/s, not 16.5.
  Showing the grid takes 238 MB, not 306.
- **Next and Previous held the window 50–145 ms.** Not playbin3: going to
  READY takes 2–8 ms. The rest was ours, with the 31,000 songs of "All
  songs" as the queue. Each song change handed the player every song in
  the queue with its speed, twice, 15–20 ms each; wrote the whole queue,
  its ids joined, to change its current song, then read every queue back,
  6–8 ms; copied the queue to read whether it shuffles; and made the bar's
  cover again three times. The hidden queue panel replaced two rows, which
  GTK styles even when hidden; the media controls' cover, arriving, told
  every view the song had changed; and each list cell's listener upgraded
  two weak references at every change, four a second while playing. Now
  the player gets the playing song and the three after it, all it reaches
  before the next change; only the place is written; the panel catches up
  when shown; and a cell's listener goes with the cell. On the main thread,
  through a muted Pulse output, the longest pause at a Next went from
  46–144 ms to 16–30 ms, and songs still follow each other gaplessly.
- **Scrolling the artists list kept the GPU's memory up.** The renderer
  keeps a copy of each texture and frees what goes only as it draws: the
  1,000 thumbnails in memory, 128 px for a 36 px row, held 140 MB there
  after a 30 s scroll (55 at the start, up to 214 during it). A row of 64
  device pixels or fewer now gets the thumbnail halved, averaged by 20
  lines of our own (gdk-pixbuf's scaler goes through glycin's sandbox, 3 ms
  a thumbnail): 0.5 ms with the decode instead of 0.3, and the GPU stays
  at 59 MB, memory 216 MB instead of 252.

**Left:**

- **The albums grid still takes 42–55 % of a core to scroll**, and it is
  GTK's. It gives cells other albums at every frame, about 20 a frame,
  where a new row comes in every 15 frames ([GTK #4633][gtk-4633]). Our
  bind is 7 % of the main thread; layout, shaping the labels' new text
  (Pango and HarfBuzz, 27 %), styling and drawing are the rest. Nothing
  short of a list of our own would bring it down.
- The scroll runs need the display on: with it asleep, the compositor
  draws no frames and the page does not move.

[gtk-4633]: https://gitlab.gnome.org/GNOME/gtk/-/issues/4633
