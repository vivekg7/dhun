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
