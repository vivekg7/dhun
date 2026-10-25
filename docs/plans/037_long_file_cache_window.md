# 037 — A window, not the whole file, for long files in the cache

**Status:** `RELEASED` — in the 0.16.1 GitHub release (2026-10-21); 0.16.1 on the owner's phone and Mac; the server stays at `server-v0.16.0`
**Started:** 2026-10-20

## Problem

The song cache ([019](019_networking_and_caching.md)) fetches every file
whole, at full speed, the moment it starts playing, and keeps it until it is
the oldest thing in a 3 GB budget: right for a song, a few megabytes, over
in a second and likely to be played again, but wrong for a podcast or an
audiobook, as the owner noticed on starting a book (2026-10-20):

- **It evicts the music.** A 10-hour book is 300–600 MB at the usual 64–128
  kbps; a listener who gets through a few books and a daily podcast churns
  the whole music cache in a week or two, and every song evicted is a
  refetch at the next play.
- **It fetches what may never be heard.** Nobody replays a book, so keeping
  it after the listen buys nothing, and starting one on mobile data pulls
  hundreds of megabytes for an hour's listening.
- **A resume far into the file is not helped.** On Android the part is
  written in order from byte zero, so a book resumed at hour eight plays
  from the stream while the cache fills the first eight hours nobody wants.

## What other apps do

Nobody persists a window. Long-form apps do one of two things: an explicit
whole download (Audible, Apple Podcasts, Overcast, Audiobookshelf's
offline mode) or plain streaming with the player's buffer, under a minute
in ExoPlayer and a few minutes at most in Pocket Casts or Spotify. Dhun
makes a promise those apps do not: a network drop never skips, the player
waits. A window of tens of minutes is what keeps that promise through a
tunnel or a stretch of flight mode without a download.

## Options

**Which files get a window**

- **By collection**: podcasts and audiobooks, music whole. Simple, but a
  90-minute mix in `Music/`, listened to like an episode, would be fetched
  whole. Rejected.
- **By the definition of a long file the product already has** (chosen):
  spoken audio, or a file of at least `longFiles.minMinutes` (plan 009,
  15 by default). One meaning of "long" for resume, the seek buttons and
  the cache.

**What the window is**

- **A fixed number of megabytes.** Means nothing to the listener: 30 MB is
  an hour of one book and ten minutes of another. Rejected.
- **Minutes ahead and behind the playing position** (chosen), turned into
  bytes with the file's average bitrate (size over duration): exact for
  constant-bitrate files, and close enough for the rest, since the
  decoder's own index decides where a seek lands. Ahead is a setting;
  behind is fixed at 10 minutes, since rewinds in a book are seconds to a
  couple of minutes and a second knob is not worth its row in Settings.
- **Fetch behind too.** Rejected: what was just played is already on disk,
  so "behind" is retention, not a fetch. A resume at hour eight fetches
  nothing before it, and a rewind past the window waits for the fetcher to
  jump there, as a seek ahead does.

**Where the setting lives**

- **Per device, beside the cache limit.** Rejected: the window is about
  the network as much as the device, but the listener wants to set it
  once.
- **Synced, under Long files** (chosen): `longFiles.cacheAhead`, minutes,
  default 30, with 10, 15 and 60 on offer, and 0 for the whole file as
  before.

**A different window on Wi-Fi.** Rejected: the bytes are small either way,
and a larger Wi-Fi window would reintroduce the eviction this plan stops.

**Prefetch of the next long file.** The same rule, read from byte zero:
the first `cacheAhead` minutes of the next episode or chapter file, so a
book split into chapter files goes from one to the next without waiting.

**How the window is bounded on disk**

- **A sparse file that keeps growing**, dropped when the file leaves the
  player. Bounded by the session, not the book, but a phone's process lives
  for days and the current file stays current across restarts. Rejected.
- **Punching holes behind the window.** APFS can; Android has no Java API
  for it, and the app has no native code. Rejected.
- **Rolling the file** (chosen): when more than twice the behind window
  sits before the playing position, the bytes from the behind window on
  are copied into a fresh file that takes the old one's place. A copy of
  at most some 50 MB, once every ten or so minutes of listening, bounds
  the file at about `2 × behind + ahead` minutes.

## Decision

A long file is cached as a **window around the playing position**: fetched
from the reader's position forward for `longFiles.cacheAhead` minutes (30
by default), kept for 10 minutes behind it, rolled when more than 20 minutes
sit behind, and dropped when the file leaves the player's plan or the app
starts. A window is never a cached song: it does not count against the
music budget, does not mark the file as playable offline, and is never
promoted to a download; downloading a long file fetches it whole, as before.
With `cacheAhead` at 0 a long file is cached whole, as songs are.

The fetcher follows the reader: a seek before the window or more than 1 MB
past what has arrived moves the fetch there at once, so the player never
falls back to the stream for a long file. The Mac already works this way
through `SparseFile` and `Fetch`; Android gains the same for windows, while
its music parts stay sequential and resumable across restarts.

A window file is named `<id>.<ext>.window`, next to the `.part` of a song
being fetched whole, so each platform's start-up, trim and offline rules
can tell them apart by name alone.

## Plan

- [x] Android: `SongCache.Entry` keeps the ranges that have arrived; the
      fetcher jumps to where a reader waits and stops at the window's end;
      the roll; windows outside the music budget, dropped when unwanted;
      `SongSource` reopens a rolled file; tests for the out-of-order reader
      and the roll.
- [x] macOS: `SparseFile.drop(before:)` for the roll and a read hook so the
      fetcher knows where the reader is; `Fetch` takes the window and pauses
      at its end; `SongCache` names, trims and drops windows; tests.
- [x] `longFiles.cacheAhead` in the three synced tables and both Settings
      pages, under Long files.
- [x] Docs: REQUIREMENTS, the guide, plan 009's table, plan 019's status.

## What building it taught

- **A decoder finding its place reads a page here and a page there.**
  Media3's Ogg seeker bisects the file for a seek target, opening it at the
  middle, a quarter, and so on. With the window anchored at every open, the
  first try fetched 142 MB of a 212 MB episode in 20 seconds, a window at
  each probe. The window now anchors only where the reader has read on
  in one piece for 256 KB, which real playback does within a second and a
  probe never does; a reader waiting anywhere else gets 256 KB and no more.
  Measured after the fix on the emulator: 31 MB for a 30-minute window
  after 20 s, growing by about a megabyte a minute, and a seek from 7 min
  to 2 h 36 min played on within 12 s and left 30 MB on disk, the old
  window rolled away.
- **A bounded Range answer's Content-Length is the range's, not the
  file's.** The phone's fetcher took it for the file's length and renamed a
  256 KB window as the whole song; the length now comes from
  `Content-Range`.
- **Episodes fetched whole by earlier versions were still in the cache**,
  723 MB of them on the emulator. A long file cached whole is dropped once
  it leaves the plan, as a window is, unless `cacheAhead` is 0.

## Open questions

- Whether the behind window should be a setting after all. Leave it fixed
  until someone asks.
- Predicting from history which long file comes next (REQUIREMENTS) would
  let the next episode's first minutes be fetched before it is queued.
