# 041 — Search filters, popularity and "In playlists"

**Status:** `IN PROGRESS` — server side built (score, genre fix); the apps next. Language waits on the curation tool
**Started:** 2026-11-11

## Problem

Search ([029](029_search.md)) finds the right songs, but three things make it
weaker than it should be for a 35,000-song library:

- **Equal matches come out in an arbitrary order.** Two songs titled
  exactly the same, or a dozen that start with the query, all have the same
  match quality. Favourites and play counts break the tie, but most songs
  have neither, so the famous one is as likely to be fifth as first.
- **The filters are invisible.** `year:1990-1999` and
  `artist:"kumar sanu"` already work, but nothing on screen says so beyond
  one hint line, and they need a query word: "every 90s Kumar Sanu song"
  cannot be asked without typing a word to search for.
- **A song's playlists cannot be seen.** 12,049 songs sit in at least one of
  the 233 playlists, one in 14 of them. Nothing shows which ones.

The owner asked for year, artist and language filters, popularity in the
ranking, and a song's playlists (2026-11-11).

### What the data holds (measured 2026-11-11)

The curation tool, outside this repo, writes `_meta/index/stats.jsonl` on
the NAS: one line per song with its plays on JioSaavn, Spotify and YouTube
Music, Apple's "popular" flag, and a **`score` from 9 to 98**, spread almost
evenly (median 52), with `score_sources` saying how many sites it came from.
Its `song_id` maps to a path through `_meta/index/songs.jsonl`.

- 32,741 of the 35,282 songs in the catalogue get a score by path; 372 more
  have a line but no score. About 7% have none.
- The top of the list is right: Closer, Faded, Believer, Agar Tum Saath Ho,
  Tum Hi Ho, Kesariya.
- Year is on 35,272 songs, genre on 28,807, composer on 33,332.
- **No language anywhere.** No tag holds one. JioSaavn's album files in
  `_meta/jiosaavn.com/collections/` give one per song, for songs that came
  from JioSaavn.

## Options

### Where the score lives

- **A. The server reads it and adds `score` to each song in
  `GET /api/v1/library`.** Every client already holds the catalogue and
  searches it offline, so the score travels with the song. One new integer
  column.
- **B. Clients fetch `GET /api/v1/scores` separately.** That makes a second
  sync path for one number. Rejected.
- **C. Write it into the files' tags.** Files change only through
  admin-panel actions (`AGENTS.md`). Rejected.

### How popularity ranks

- **A. A third tie-break**: match quality first, then favourites and plays,
  then score. It keeps 029's rule that a weaker match never jumps a better
  one, and the user's own taste still beats the world's.
- **B. Blend score into the match score**, so a very popular song can beat a
  slightly better match. Then "tum hi" could put "Tum Hi Ho" above a song
  titled exactly "Tum Hi", which reads as search being wrong. Rejected.

### How filters are applied

- **A. Filter chips under the search box**, building the same query terms
  the typed syntax builds. One matching path, and a typed filter shows up as
  a chip.
- **B. A separate filter screen.** More taps, and it hides what is applied.
  Rejected.

## Decision

### Score

The server reads `_meta/index/stats.jsonl` and `_meta/index/songs.jsonl`
in a scan, joins them by path, and stores the score
**rounded to an integer** in a new `songs.score` column (0 for none). A row
whose score changes gets a new `version`, so clients pick it up through the
normal incremental pull. The file refreshes about monthly; rounding keeps
that refresh to the songs that actually moved, not all 33,000. A missing or
broken file leaves the scores as they were and logs one line. Search must
never fail because the curation tool has not run yet.

The files are read again only when their size or time changed, or when the
scan added or moved songs, which need their score. The hourly rescan with
nothing changed stays a directory walk: reading both files is about 0.1 s
and 11 MB of passing allocation on a Mac at 35,000 songs (2026-11-11). A
file that does not parse, most likely caught while the tool writes it, is
tried again on the next scan. The first scan after the upgrade scores about
33,000 songs at once, so every app pulls the library once more.

The catalogue's song JSON gains `score` (omitted when 0). Clients store it
with the song.

### Ranking

- **Songs:** score is the tie-break after favourites and plays, as above.
- **Artists and albums:** among equal matches, the one whose songs are more
  popular comes first (its best song's score). So "kumar" lists Kumar Sanu
  before a Kumar with three songs.
- **Typos:** a typo match is already last. Among typo matches, popular songs
  come first, which is usually what was meant.

### Filters

A **Filters** chip at the start of the section-chip row opens a sheet with:

| Filter     | Control                                                | Term built                 |
| ---------- | ------------------------------------------------------ | -------------------------- |
| Year       | decade chips (50s … 20s) plus a from–to range          | `year:1990-1999`           |
| Artist     | searchable list, several allowed (any of them matches) | `artist:"…"` (OR'd)        |
| Genre      | the genres in the library, several allowed             | `genre:"…"` (OR'd)         |
| Popularity | "Popular only": score 70 or more (about the top third) | `popular:`                 |
| Language   | waits on the data; see Open questions                  | `language:hindi`           |
| Sort       | Best match (default), Most popular, Newest, Oldest     | none; a sort, not a filter |

Applied filters show as chips with a ✕ next to the box. **Filters work with
an empty box**: the Songs section then lists every song that passes, in the
chosen sort. "Most popular" is the default sort with no query. That makes
search a browser too: 90s, Kumar Sanu, most popular first. A song tapped
there plays the list, as any list does.

Several artists or genres OR together; different filters AND. That is the
reading of "Arijit or Atif, from the 2010s" that people mean.

The typed syntax stays and gains `popular:`. Filters are part of the query,
so they go into recent searches with it.

### In playlists

The song's playlists show in three places (owner, 2026-11-11):

- **Song info**: a new sheet from the song menu ("Info"), with the tags,
  format, plays, score and **In playlists**. A tap opens the playlist.
  No client has a song-info view today; this adds one, kept to one screen.
- **Song menu**: "In playlists…", shown only when there is at least one,
  opening the same list.
- **Search**: the Playlists section also lists playlists that hold the top
  songs found, after the ones whose names match, so "kesariya" shows
  "Arijit Singh - Top 200", "Pritam - Top 200" and "Wedding". Only the top 3 songs are
  looked up, or a broad query would list every playlist.

Membership comes from the playlists in the catalogue, which list their
songs. Each client builds a song → playlists map with the search index.

### Found while measuring

The server splits genres on `&`, so "R&B/Soul" is stored as "R" and
"B/Soul" (152 songs). Fixed here, since the genre filter would list both.
In a genre only a spaced `&` splits now: "Sufi & Ghazals" and "Devotional &
Spiritual" (Apple's names, 1,762 songs) still give two genres each, as the
Genres tab has shown them, while "R&B/Soul" stays one. Artists still split
on any `&`. A migration makes the next scan read those 152 files again.

## Plan

1. Server: migration adds `songs.score`; read the two index files after each
   scan; `score` in the library JSON; genre split fix; tests for a missing
   file, a changed score bumping the version and an unchanged one not.
2. `api/`: document `score` in the library response.
3. Android: store `score`; tie-break; Filters sheet and chips; empty-query
   browse; song info; "In playlists…"; playlists in search.
4. macOS, web, Linux: the same, as in 029. Filters on the Mac sit in the
   toolbar's search menu, not in the page.
5. Measure against the 50 ms per keystroke target at 100,000 songs
   ([036](036_fast_at_100k.md)). The filters are array lookups, but an empty
   query with a sort sorts the whole library.

## Rejected

- **Score from our own listening only.** 3–4 people have heard a small part
  of 35,000 songs; the outside score is what knows the rest. Our plays
  already rank through the favourites-and-plays tie-break.
- **Hiding unpopular songs by default.** The archive is the point. Popularity
  orders and can filter, but never hides on its own.

## Open questions

- **Language.** Paused (owner, 2026-11-11): the curation tool will be asked
  to write a language per song. Once it does, read it the way `score` is
  read, add `songs.language`, and turn on the filter. Where the tool writes
  it is still to be seen. `_meta/index/stats.jsonl` next to `score` is the
  natural place.
