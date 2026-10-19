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
