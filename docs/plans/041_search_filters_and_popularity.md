# 041 — Search filters, popularity and "In playlists"

**Status:** `IN PROGRESS` — server in `server-v0.18.2` on the NAS (2026-11-12); built on Android, macOS, web and Linux, to ship together as 0.19.0. Language waits on the curation tool
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

- **A. Filter chips building the typed syntax's terms** (`artist:"…"`).
  One matching path, but every typed word must match, so two artists would
  mean songs by both, and a name picked from a list needs no fuzzy match.
  Rejected while building.
- **B. Filter chips testing the song's own values.** Exact, and several
  picks can mean any of them. Chosen; see Filters.
- **C. A separate filter screen.** More taps, and it hides what is applied.
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

A row of **filter chips** sits under the search box, always in view: Year,
Artist, Genre, Popular and Sort. A chip names what it is set to ("1990s",
"Kumar Sanu +1", "Most popular") and opens a small dialog to set it. A
single sheet holding all of them was the first idea; the app's pattern is
dialogs ([018](018_settings_layout.md)), and a chip per filter keeps what is
applied on screen with one tap fewer.

| Filter   | Dialog                                                                |
| -------- | --------------------------------------------------------------------- |
| Year     | each decade from the 2020s to the 1950s, "Before 1950", or a from–to  |
| Artist   | the library's artists, searched as the tab searches, most songs first |
| Genre    | the library's genres, the same way                                    |
| Popular  | no dialog: a toggle for score 70 or more, about the top third         |
| Sort     | Best match (default), Most popular, Newest, Oldest                    |
| Language | waits on the data; see Open questions                                 |

**Filters test the song itself, not words.** An artist or a genre is picked
from the library's own list, so it is compared with the song's values,
ignoring case, as the Artists and Genres tabs group them. The typed
`artist:` and `genre:` words stay as they were. They could not have done
this job: every typed word must match, so two artists would mean songs by
both. Picked ones mean **any of them**, and different filters all apply:
"Arijit or Atif, from the 2010s". A song with no year is in no range, and
sorts last by year.

**Filters work with an empty box**: the Songs section then lists every
song that passes, most popular first unless another sort is picked. That
makes search a browser too: 90s, Kumar Sanu, most popular first. A song
tapped there plays the list, as any list does, and the queue is named
after the query and the filters ("Search: 1990s · Kumar Sanu +1"). With a
filter set, **only songs are found**, all of them rather than the first
300: the question is about songs, and an album or folder section would
answer a different one. A sort alone keeps every section and reorders the
songs.

The filters stay set while the tab is away, as the box keeps its text, but
not across a restart, and they are not kept in recent searches, which stay
plain text. A typed `popular:` was planned and dropped: the chip does it,
and every port would have needed it.

Measured on the Pixel emulator, release build, 35,282 songs (2026-11-11):
the 1990s with an empty box, 6,903 songs, takes 11–41 ms off the main
thread. A keystroke over songs is 6–34 ms with one 75 ms outlier, against
8–28 ms for the build before, within the emulator's noise.

### In playlists

The song's playlists show in three places (owner, 2026-11-11):

- **Song info**: "Info" in the song menu opens a dialog with the tags, the
  file, the folder, plays, popularity and **In playlists**. A tap opens the
  playlist. No client had a song-info view; this adds one, kept to one
  screen.
- **Song menu**: "In playlists…", shown only when there is at least one,
  for NAS songs.
- **Search**: the playlists that hold the top 3 songs found join the
  Playlists section, after the ones whose names match, so "kesariya" shows
  "Arijit Singh - Top 200", "Pritam - Top 200" and "Wedding". Only the top
  3, or a broad query would list every playlist. When no playlist's name
  matched, the section comes right after the songs, not before: a search
  for a song's title wants the song first.

Membership comes from the playlists in the catalogue, which list their
songs, some 18,000 entries over 233 playlists. The phone works out each
song's playlists once per change to the list, not on every keystroke.

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
3. Android: store `score`, and pull the library again once
   (`LIBRARY_FORMAT` 3), since a phone whose cursor is past the server's
   scoring would never get the scores; tie-break; filter chips; empty-box
   browse; song info; "In playlists…"; playlists in search. Built.
4. macOS, web, Linux: the same, as in 029, each with its own
   `LIBRARY_FORMAT` 3 and a migration adding the score. Built, with the
   guide's "Searching" section, which waited for every app to have the
   filters. Where they differ from the phone:
   - **Mac:** SwiftUI's toolbar search field has no menu of its own to
     extend, so the filters sit in a filter button beside it; its icon
     fills while one is applied and the results' subtitle names them.
     "Get Info" and an "In Playlists" submenu, as the Mac names them.
   - **Web:** the chips sit outside the scroller, so they stay in view;
     Sort opens the web's own menu. The artist and genre lists show 200
     names and say how many more are found by typing: a dialog there has
     no lazy list, and the library has some 6,800 artists. Info keeps the
     track, codec, bit depth and path it showed before.
   - **Linux:** the search field is in the sidebar, so the filter row heads
     the Search page. Filtered songs show as a song table, not result
     rows: they run to thousands and the results list builds every row.
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
