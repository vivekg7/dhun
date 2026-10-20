# 036 — Meeting the speed targets at 100,000 songs

**Status:** `RELEASED` — in the 0.16.0 GitHub release (2026-10-20); 0.16.0 on the owner's phone and Mac; server and web in `server-v0.16.0` on the NAS (2026-10-20)
**Started:** 2026-10-20

## Problem

The collection grew from about 7,000 songs to about 30,000, and
[REQUIREMENTS](../REQUIREMENTS.md#lightweight-and-fast) now sets targets at
100,000: search under 50 ms a keystroke, a usable library under 1 s after
start, a server under 256 MB. The
[baseline](../research/performance-baseline.md) shows where the code stands:
the server at 346 MB steady at 100k, the web client at 1.6 s to build its
catalogue, 2.3 s to its first search and 105 ms at the worst keystroke.
Android and the Mac share the web's design (plan 029) and were not measured.

A review of all four code bases against the targets (2026-10-20) found
that the misses have a few shared causes, not many separate ones, and a
handful of correctness bugs next to them:

- **The three clients sort with a comparator that allocates on every
  comparison.** `natural()` turns both strings into arrays per call. A sort
  of n titles makes about n log n calls; at 100k that is half the catalogue
  build. The Mac's version also lowercases one character at a time through
  `String`.
- **The three clients build the search index lazily, on the main thread,
  at the first keystroke.** The "first search ready as soon as the library"
  target cannot be met that way.
- **The index keeps more than it reads.** Eight strings per song in `texts`
  and eight more in `runs`, when only the title text and the year are ever
  read back, and a run only matters for a field of two or more words. At
  100k songs that is 1.6 million strings and about 160 MB of heap in the
  browser.
- **The server builds the whole `/library` response in memory twice**: a
  slice of every row, then one `json.Encode` buffer over it. Measured on a
  100k replica: 67 MB live and 240 MB allocated for one response. No Go
  memory limit is set, so the runtime keeps it.
- **Correctness, found on the way:** the server's `/library` loop never
  checks `rows.Err()`, so a query that fails midway sends a partial
  catalogue with a valid cursor, and the client never fetches the rest.
  On Android, a local edit cancels a sync already running, and playback
  makes an edit every 30 s, so a long first pull can be killed over and
  over. On the Mac, an outbox insert that fails is swallowed.
- **Smaller wastes with the same root**, a design sized for 7,000 songs:
  Room upserts that throw one exception per existing row, sync writes of
  one row per transaction each re-running every observing query, folder
  rows that flatten their whole subtree per draw, a "recently added" view
  that sorts the whole library to print a count, an artist page on the Mac
  that filters albums with three nested loops, one SQLite connection on
  the Mac behind one lock for both the main thread and bulk writes, a web
  page that stays blank until the catalogue is built, and no index on
  `songs.art` for a query the clients make hundreds of times at first
  start.

The comments in all three clients still justify the in-memory design with
"about 7,000 songs group in a few milliseconds". That premise is gone.

## Options

**How the clients get fast**

- **An incremental catalogue**: apply each changed song to the existing
  sorted lists and indexes instead of rebuilding. Removes the rebuild cost
  entirely, but every grouping, every sort and every index needs an update
  path, with its own bugs, in three code bases. Rejected for now: the
  measurements say a whole rebuild fits the targets once the comparator
  and the index stop allocating. Revisit if a rebuild at 100k is still
  seconds on the phone after this plan.
- **Paging through SQL** instead of a catalogue in memory, as the first
  Android design considered. Rejected in plan 011 and again here: the
  search of plan 029 needs the whole index in memory anyway, and the
  catalogue is a few tens of megabytes.
- **A faster whole rebuild** (chosen): one precomputed sort key per item,
  the index built eagerly off the main thread right after the catalogue,
  the index slimmed to what is read, and the same strings folded once
  rather than once per song that shares them. Each is a local change with
  a test, and the same change three times.

**Where the index is built**

- **A Web Worker or a separate process.** Moves the cost off the main thread
  entirely, but the index must then be transferred, and Android and the Mac
  already have background threads. Not needed once the build is under half
  a second; kept as the next step if a phone measurement says otherwise.
- **Right after the catalogue, on the catalogue's own background task**
  (chosen). On Android that is the `combine` that builds the `Catalog`; on
  the Mac the detached task in `reloadSongs`; on the web idle time after
  the catalogue is set, since the page has no other thread.

**How the server sends `/library`**

- **Pages** (`?after=<id>&limit=`). Bounded memory per request and
  resumable, but the clients' cursor is a version, not an id, and a sync
  that straddles a scan would need both. More protocol for the same bytes.
- **Streaming one row at a time into the gzip writer** (chosen). Same
  response, same contract; peak memory is one row plus the gzip window. The
  cost is that an error after the header is written cannot become a 500:
  the handler aborts the connection instead, so the client sees a truncated
  body and retries, never a plausible partial catalogue.

**How the server stays under 256 MB**

- `mem_limit` alone (1 GB today) only kills the process; it does not make
  the Go collector work harder. A **`GOMEMLIMIT` near the target** does,
  set in the image so every deployment gets it and the compose file can
  override it. Taken together with streaming; a limit alone would trade
  memory for GC time on the 240 MB encode buffer.

## Decision

Do the shared fixes once per client, the server fixes, and the correctness
fixes, as one plan, measured against the baseline when done. In order:

1. **Correctness first.** `rows.Err()` on every server loop that builds a
   response; Android's `Sync.soon()` cancels only the delay, never a sync
   in flight; the Mac's `addOp` fails loudly.
2. **Server memory.** Stream `/library`; `GOMEMLIMIT` in the image; an
   index on `songs.art`; lazy path lookup in `indexPlaylists` so a rescan
   with no playlist change does not load every path under the write lock;
   chunked fetch in `songEntries`.
3. **Clients, the shared three.** A sort key per song, album, group and
   folder; the song index built eagerly off the main thread; the index
   keeps the title text and a numeric year and builds runs only for
   multi-word fields.
4. **Clients, the rest.** Transactions around sync writes and plain
   `REPLACE` inserts on Android; a second read connection and chunked
   writes on the Mac; folder counts computed once; "recently added" as a
   top-100 selection on the catalogue; the web shell built before the
   catalogue so the page paints at once, with the sync waiting for the
   kept copy to land, or it would merge into an empty catalogue; a shared index for lists that hold
   episodes; the duplications and dead code the review listed.
5. **Docs.** The nine "7,000 songs" comments say what is true now; plan 003's
   memory figure matches the baseline; the compose file's limit matches the
   target.

Rerun the baseline on the desktop and add a dated section; the phone and
Mac measurements the baseline asks for still need the devices.

## Plan

- [x] Server: `rows.Err()`, streaming `/library`, `GOMEMLIMIT`, art index,
      lazy playlist paths, chunked `songEntries`.
- [x] Web: sort keys, memoised fold, slim index, idle-time index, shell
      first, cache in catalogue order, shared `all` index, list counts,
      duplications.
- [x] Android: sync cancellation, `REPLACE` inserts, transactions,
      `distinctUntilChanged` and warm-up off main, sort keys, slim index,
      cached folder summaries, recently-added on the catalogue, dropped
      dependencies, duplications.
- [x] macOS: sort keys, eager index, read connection and chunked writes,
      sync work off the main actor, artist page lookup, folder counts,
      `dhun-play` removed, `addOp` throws, duplications.
- [x] Baseline rerun on the Mac; comments and plan 003 updated; the Mac app
      and the Android app (emulator) measured on 2026-10-20 with the timing
      lines both apps now log. The owner's phone is still to measure.

## Open questions

- Whether the Android cold start at 100k fits the 1 s target with the whole
  `song` table read into memory, including `notes` and `chapters`. On the
  emulator at 29,000 songs the catalogue builds in about 300 ms; measure
  on the phone before slimming the projection.
- The Mac's catalogue build: 750 ms for 29,000 songs against 74 ms for the
  same work on the web. `sortKey` builds a `String` per item through
  `unicodeScalars` and the groupings go through `Dictionary(grouping:)`;
  profile before guessing.
- Android searches some keystrokes twice in a row (a recomposition asks
  again); cheap today, worth removing.
- Whether the web should cache the slimmed index in IndexedDB across starts.
  Only worth it if the idle-time build is still noticeable on a phone.
- Server items left to measure before acting: SQLite `cache_size` and
  `mmap_size` for the scan's full-table reads; the lyrics index, which keeps
  every `.lrc` line as its own string; the first scan, which holds every
  changed file's tags until its one transaction. The second and third are
  the likely peaks at 100,000 songs on a first scan, not in daily use.
