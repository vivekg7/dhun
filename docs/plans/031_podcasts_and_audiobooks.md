# 031 — Podcasts and audiobooks

**Status:** `ACCEPTED` — behaviour decided by the owner; no code yet
**Started:** 2026-10-10

## Problem

The NAS holds two more audio collections beside `Music/`, and Dhun sees
neither:

- **`Podcasts/`** (12 GB): so far one show, the Lex Fridman Podcast, about
  140 episodes. The owner's own tool downloads them. A processed episode is
  Ogg Opus named `001 - Guest - Topic.opus`, with standard tags (title,
  album = the show, artist, date, genre `Podcast`, the show notes in
  `comment`), chapters and a cover inside the file, and a `.en.vtt` or
  `.en.auto.vtt` subtitle file beside it; one also has a `.transcript.md`.
  About 25 episodes are not yet processed: no tags, the title only in the
  file name. Empty category folders (`Tech/`, `News/`, …) wait for more
  shows.
- **`Audiobooks/`** (22 GB, 172 MP3s, about 30 books, growing). A book comes
  in five shapes: one file per book (Dune: 18 chapters inside the file;
  Amish Tripathi: none), one file in a folder with a cover and a `.txt`
  (The Three-Body Problem), one file per chapter (Harry Potter), a few
  parts of eight hours each (Mistborn 2, 3, 6), and parts in nested folders
  `… (1 of 3)/MISTBORN0101P01.mp3` with no tags at all (Mistborn 1). Tags
  disagree from book to book: the narrator is sometimes the album artist,
  Mistborn 2's artist is "Mistborn".

Plan [009](009_resume_long_files.md) already resumes any long file across
devices, and [016](016_speed_and_pitch.md) gives a file its own speed. What
is missing is everything that treats these files as something other than
songs. Mounted as they are, a 21-hour Dune would be a song in Albums, in
shuffle-all and in most played; a book of 39 files would be 39 songs with
nothing tying them together; an episode would have no played mark, no
chapters and no transcript.

The behaviour is in [REQUIREMENTS](../REQUIREMENTS.md#podcasts-and-audiobooks)
(owner, 2026-10-10).

## Options

### How the server holds them

**A. Separate tables** for episodes and book files. Every feature that
takes a song ID (streaming, art, queues, favorites, Listen Later, resume
points, per-file speed, downloads, the listening history) would need a
second ID space or a second code path. Rejected.

**B. The same `songs` table, with a `kind` column** (`music`, `podcast`,
`audiobook`). Everything keyed by song ID works unchanged, which is most of
the system. The clients filter by kind wherever music is listed.

**C. Mount them inside `Music/`.** No server change at all, and every
problem listed above. Rejected.

### What makes a book

**Folder only** (Audiobookshelf's rule: one folder, one book) fails on
`Dune/` and the Amish folders, which hold several books side by side.
**Album tag only** fails on Mistborn 2, whose parts are tagged `… pt 1`,
`… pt 2`, and on Mistborn 1, which has no tags. **Album tag within a
folder, with part markers ignored and part folders rolled up**, fits every
book on the NAS but one (below). Checked against the NAS on 2026-10-10.

### Where the grouping happens

In each client, the way folders come from path prefixes today
([005](005_storage_and_library_model.md)), or once on the server. The rule
above has enough edge cases that three implementations (Kotlin, Swift,
JavaScript) would drift apart. **The server decides**, and sends the result
in the catalogue.

## Decision

**B**, with the grouping and the chapters worked out on the server.

### Server

- **Two optional mounts,** `DHUN_PODCASTS` (`/podcasts`) and
  `DHUN_AUDIOBOOKS` (`/audiobooks`), read-only, scanned like `DHUN_MEDIA`.
  An unset or missing one is simply not scanned.
- **`songs.kind`**, and paths unique per kind rather than overall, each
  relative to its own root. SQLite cannot change a `UNIQUE` constraint in
  place, so the migration rebuilds `songs` (create, copy, drop, rename,
  with foreign keys off). Moves are matched by quick hash within a kind.
- **The safety check is per root.** Today a scan that finds no audio while
  songs are known changes nothing, in case `Music` is not mounted. Each
  root gets the same check on its own, so an unmounted `Podcasts` never
  marks every episode missing.
- **Grouping**, stored on each row as `group_key`, `group_title` and
  `group_index`:
  - Audiobook: files in one folder whose album tags match, after removing
    part markers (`pt N`, `part N`, `(N of M)`, `disc N`, `CD N`) and
    surrounding spaces, are one book. A folder named only as a part
    (`… (N of M)`, `CD N`, `Disc N`, `Part N`) belongs to its parent.
    Untagged files make one book per folder, titled with the folder's
    name. Order: disc, then track, then the path in natural order (so
    `P2` before `P10`).
  - Podcast: the show is the album tag, else the folder. Episodes order
    by the date tag, else the file name.
- **More tags kept:** the date in full (not only the year) and the comment
  as notes, both only for podcasts and audiobooks.
- **Chapters** are read into a JSON column and sent in the catalogue: ID3v2
  `CHAP` frames in MP3 (Dune, The Three-Body Problem), `CHAPTERnnn`
  comments in Ogg (the episodes), and the Chapters element in Matroska
  (our parser in `matroska.go`). Whether taglib's WebAssembly build exposes
  any of these is to be checked; the `CHAP` reader is ours either way, and
  is small: a frame ID, a start time and a title frame. A book whose files
  are its chapters needs nothing stored: its files are listed in order.
- **Transcripts** come through the lyrics endpoint, which gains the sources
  `transcript` and `vtt`. A `.transcript.md` beside the file wins, then
  `.<lang>.vtt`, then `.<lang>.auto.vtt`. Both are converted on the server
  into the LRC-style synced text the apps already parse, so no app needs a
  new parser. YouTube's automatic captions repeat each line as it scrolls,
  and carry word timings: the conversion keeps each line once, at its first
  timestamp. The Markdown keeps its speaker names and drops its headings.
- **Played marks:** a table `played (user_id, song_id, played, at)` and
  the operation `played.set {song, played}`, where the later `at` wins, as
  for favorites. The server sets it itself when a `play` reaches 90% into a
  podcast or audiobook file, the threshold Listen Later already uses
  ([010](010_special_playlists.md)). It is pulled with the rest of the
  user's data.
- **Older apps see no change.** `/library` returns only music unless the
  client asks for `?kinds=all`. An app released before this plan would
  otherwise show every episode as a song.

### Apps (Android first, then macOS and web)

- **Podcasts** and **Audiobooks** sections, each hidden while its
  collection is empty. Podcasts: shows, then a show's episodes newest
  first, with played ones hidden by a switch. Audiobooks: books with
  cover, author and progress, then a book's files.
- **Everywhere music is listed**, kind `music` only: songs, albums,
  artists, genres, folders, shuffle-all, most and recently played,
  Recently added. Favorites, Listen Later, playlists and queues take any
  kind, since they are lists the user built.
- **Playing** follows the queue rule: playing from a book makes a new queue
  named after the book with its files in order, starting at the first
  unplayed file and its resume point; playing an episode makes a queue of
  the show's episodes from that one on.
- **Now playing** shows chapters and skips by chapter. The resume and
  speed rules of 009 and 016 apply. Two synced settings,
  `speed.podcast` and `speed.audiobook`, set the speed for a whole
  collection, and a file's own speed still wins. Without them, a 39-file
  book would need its speed set 39 times.
- **Search** finds shows, episodes and books under their own headings.

## Rejected

- **Fetching feeds on the server.** The owner's tool already downloads
  episodes, and fetching would make Dhun write into the collection, which
  it never does ([REQUIREMENTS](../REQUIREMENTS.md#users-and-the-library)).
- **Reading the tool's `_meta/*.jsonl`.** Rich, but it ties Dhun to one
  tool's log format. Everything in it that Dhun shows (title, show, date,
  notes, chapters) is also in the processed file's tags.
- **Telling kinds apart by genre or length.** Genre tags are unreliable
  here (an Amish book is tagged `Other`), and a long DJ mix is not a book.
  The folder a file sits in is exact. Plan 009's length rule stays what it
  is: the rule for resuming, for any kind.
- **A `played` flag derived from the listening history.** It could not be
  undone by hand without deleting listens.

## Plan

1. **Server**, one release: migration (`kind`, the rebuilt `songs`, the
   group columns, `chapters`, `played`), the two mounts, per-root safety,
   grouping, the chapter readers, transcript conversion, `played.set`,
   `?kinds=all`, and `api/openapi.yaml`. Test against copies of the real
   shapes: Mistborn 1's nested parts, Mistborn 2's `pt N`, Dune's chapters,
   an episode with automatic captions. **Can fail:** the table rebuild on
   the live database. The nightly backup runs first, and the migration is
   tried on a copy of `dhun.db` before the release.
2. **`deploy/docker-compose.yml`** and the NAS: two more read-only volumes
   and their variables; [003](003_deployment.md) updated.
3. **Android**, then **macOS** and **web**, as above. One minor version for
   every platform together (0.13.0).

## Open questions

None for the owner. Things to put right on the NAS, not in Dhun:

- _The Immortals of Meluha_ and _The Oath of the Vayuputras_ are both
  tagged with the album "Shiva Trilogy", so the rule makes them one book.
  Their album tags should be the book titles, as the other Amish books'
  are.
- The unprocessed episodes show their file name as the title until the
  owner's tool tags them.
