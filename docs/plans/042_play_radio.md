# 042 — Play radio: a song and the songs like it

**Status:** `PROPOSED` — no code yet
**Started:** 2026-11-11

## Problem

Tapping a song plays the list it is in. That is right on an album, an artist
or a playlist, and in search when the search was for an artist. It is wrong
when the search was for one song by name: the other matches are songs that
happen to share words with it ("Tum Hi Ho", "Tum Hi Ho Bandhu", "Tum Ho
To"), and after the first song the queue plays nonsense.

The owner wants a way to play a song with 5–10 similar ones after it,
picked by popularity, year, artist, genre and so on (2026-11-11). The data
for it exists: an outside popularity score for 93% of the songs
([041](041_search_filters_and_popularity.md)), the family's listening
history with skips ([008](008_listening_history.md)), and 233 playlists,
many of them curated by era and mood ("90s Romance", "Sufi", "Punjabi
Wedding").

## Options

### How it is started

- **A. Tap a found song and get similar songs** instead of the other
  matches. That breaks the artist and album case, where the matches are
  what is wanted. Rejected as the default.
- **B. Guess**: radio when the search was for the song's title, matches
  otherwise. The owner judged it not worth building first (2026-11-11). It
  is cheap with 029's ranking, which already knows when the query is the
  song's title or its start, so it is offered as a setting below, off by
  default.
- **C. "Play radio" in the song menu** (owner, 2026-11-11, after YouTube
  Music's "Start radio"). Explicit, and works from any list, not only
  search.

### Where it runs

- **A. On the server.** One implementation for four clients. It can use
  the whole family's listening and the index files the server already reads
  for 041. It needs the server. The owner accepted that (2026-11-11): offline,
  radio is unavailable.
- **B. On each client**, over its copy of the catalogue. Four ports, as with
  search, and no access to other users' listening. Rejected.

## Decision

C, on the server, with B as a setting.

### What the user sees

- **"Play radio"** in the song menu of every list, and in Now playing's
  menu. It makes a **new queue** named "Radio · <title>": the song, then
  10 similar songs. Queues are not playlists, and this is a queue like any
  other: editable, saved, synced.
- **"Add similar songs"** in Now playing's menu appends 10 more to the
  current queue. That is "keep going" without a whole endless-radio
  machinery.
- **Setting, Search → "Tapping a song found by its title"**: _Play the
  matches_ (default) or _Play radio_. "Found by its title" is 029's rule:
  the query is the song's title, or its start.
- **Offline or on an older server**, the menu item is shown disabled with
  "Needs the server". Radio never silently plays something else.

### The endpoint

`GET /api/v1/radio/{id}?n=10&exclude=1,2,3` returns song IDs, best first.
`exclude` carries what is already in the queue, so "Add similar songs"
never repeats.

### How songs are picked

Each candidate gets a weighted sum of signals against the seed. The weights
are constants in one place, to be tuned by listening:

| Signal          | Why                                                                               |
| --------------- | --------------------------------------------------------------------------------- |
| Shared playlist | Curated lists ("90s Romance") are the strongest "goes with" the library has.      |
| Played together | Songs the family played within 30 minutes of each other, skips excluded.          |
| Shared artist   | The obvious one, capped so radio is not an artist page.                           |
| Same composer   | For film music the music director matters as much as the singer.                  |
| Genre overlap   | Coarse ("Bollywood" is 15,000 songs) but it separates bhajans from party songs.   |
| Year distance   | Close years score more, fading over about ten years: a 90s song wants 90s songs.  |
| Score distance  | A hit followed by obscure B-sides feels wrong; similar popularity keeps the mood. |
| Score itself    | A small pull towards songs people know, so radio does not dig for rarities.       |
| Skipped by user | Songs this user skips early, and often, score less.                               |

Then, for variety:

- **At most 2 songs per artist**, and none by the seed's main artist in the
  first two slots.
- **No other version of the seed**: same title once folded (029's folding),
  so the reprise and the lo-fi are not "similar songs".
- **Not played by this user in the last 24 hours.**
- **A little chance**: pick from the top 40 weighted by score, not the top
  10 in order, so radio from the same song differs each time.

Phone-local songs, podcasts and audiobooks are never candidates; the seed
can only be a NAS song, since the server knows nothing else.

### Cost

In memory on the server: per song, its artist, composer, genre and year
numbers, its score, its playlists, and its top 50 "played together"
neighbours. The neighbours are built from `plays` at startup and once a
night. A request scores every song: 100,000 cheap comparisons, a few
milliseconds in Go. No new table.

### Recording

A queue made by radio sends `source = "radio"` with its plays (the column
exists, [008](008_listening_history.md)). Skips of radio songs are then the
feedback for tuning the weights, and later for a better recommender.

## Plan

1. Server: the in-memory features (sharing 041's index read), the
   neighbours from `plays`, `GET /api/v1/radio/{id}`, documented in `api/`.
   Tests: no version of the seed, the per-artist cap, `exclude` honoured,
   an unknown song is a 404.
2. Android: "Play radio" and "Add similar songs", the search setting, the
   offline state.
3. macOS, web, Linux: the same menu items.
4. Listen for a week on the owner's account, then tune the weights.

## Rejected

- **Endless radio** that tops itself up as it nears the end. It needs a
  queue that changes by itself, which the sync model treats as edits from
  nowhere. "Add similar songs" gives the same with one tap.
- **Audio analysis** (tempo, key, embeddings). It means reading every file,
  a model, and minutes of NAS CPU per thousand songs. The metadata and our
  listening should be tried first.
- **Spotify's or YouTube's own recommendations.** It would call an outside
  service on every radio, and send it what the family plays.

## Open questions

- **Language.** It is the most useful signal missing: a Punjabi song should
  bring Punjabi songs. It is added when 041 gets the data.
- **Size.** 10 after the seed, within the owner's 5–10. A setting only if
  10 turns out wrong.
