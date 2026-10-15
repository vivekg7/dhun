# 031 — Podcasts and audiobooks

**Status:** `RELEASED` — in the 0.13.0 GitHub release (2026-10-12); 0.13.0 on the owner's phone and Mac; server and web in `server-v0.13.0` on the NAS (2026-10-12)
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
  with foreign keys off; see below). Moves are matched by quick hash within
  a kind.
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
    name. Order: disc, then the part number taken from the album, then
    track, then the path in natural order (so `P2` before `P10`).
  - Podcast: the show is the folder the episodes are in, titled with the
    album tag most of them carry, else the folder's name. Episodes order
    by the date tag, then the file name; one with no date yet sorts first.
- **More tags kept:** the date in full (not only the year) and the comment
  as notes, both only for podcasts and audiobooks.
- **Chapters** are read into a JSON column and sent in the catalogue: ID3v2
  `CHAP` frames in MP3 (Dune, The Three-Body Problem), `CHAPTERnnn`
  comments in Ogg (the episodes), and the Chapters element in Matroska
  (our parser in `matroska.go`). taglib returns the Ogg comments but not
  the `CHAP` frames, so `chapters.go` walks the ID3v2 frame headers and
  reads only those frames, skipping a large cover. A book whose files are
  its chapters needs nothing stored: its files are listed in order.
- **Transcripts** come through the lyrics endpoint, which gains the sources
  `transcript` and `vtt`. A `.transcript.md` beside the file wins, then
  `.<lang>.vtt`, then `.<lang>.auto.vtt`. Both are converted on the server
  into the LRC-style synced text the apps already parse, so no app needs a
  new parser. YouTube's automatic captions repeat each line as it scrolls,
  and carry word timings: the conversion keeps each line once, at its first
  timestamp. The Markdown keeps its speaker names and drops its headings.
- **Played marks:** a table `played` shaped like `favorites` (`deleted`
  is "marked unplayed again") and the operations `played.set` and
  `played.unset {song}`, where the later `at` wins, as for favorites. The server sets it itself when a `play` reaches 90% into a
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
  Recently added. Favorites, Listen Later and queues take any kind, since
  they are lists the user built. Playlists hold music only: each is an
  `.m3u8` in `Music/` whose paths are relative to it, and inside the
  container it cannot point into `Podcasts/`. The server refuses an
  episode in a playlist, and the nightly Favorites and Listen Later files
  leave them out.
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

## Changed during implementation (2026-10-10)

- **The rebuild needs foreign keys off.** Dropping `songs` breaks every
  reference to it even with `defer_foreign_keys` on: SQLite counts the
  violations at the drop, and renaming the new table does not undo them,
  so the commit fails (tried). The pragma that turns the checks off has no
  effect inside a transaction, so a migration whose first line is
  `-- foreign_keys: off` runs with them off on its own connection, and the
  runner refuses to commit it while `PRAGMA foreign_key_check` reports
  anything. That is SQLite's documented procedure for changing a table.
  A test runs the rebuild on a database with users, favorites and a
  deleted song, and checks that IDs, references and the never-reuse
  counter all survive.
- **A show is its folder, not its album tag.** The ~25 episodes the
  owner's tool has not tagged yet would otherwise be a second show beside
  the tagged ones.
- **A part number in the album orders the parts.** Mistborn 2's three
  files all have track 1; `pt 1`, `pt 2` and `pt 3` put them in order.
- **A lone image is a book's cover** when a podcast or audiobook folder has
  none of the usual cover names (`The Three-Body Problem.jpg`). Not for
  music, whose folders hold scans and booklets.
- **Rolling back is safe but noisy.** An older server runs on the new
  schema, but its scanner knows only `Music/`, so it marks every episode
  and book file missing. Their rows and user data stay, and the next scan
  by this server brings them back.

## Changed during implementation: Android (2026-10-11)

- **The two sections are cards on the Playlists tab,** beside Favorites
  and Listen Later, each shown once its collection has something in it
  (the owner's choice). The tab bar already has eight tabs and the menu;
  one more would leave each 36 dp on a phone, two more 33 dp.
- **Every episode and book file keeps a resume point,** however short.
  `longFiles.minMinutes` (009) is for music: a book split into 10-minute
  files would otherwise start each one from the beginning.
- **Next and Previous go by chapter** in a file that has chapters, on Now
  playing and in the mini players. Previous goes to the chapter before,
  never back to the start of the one playing, as it never restarts a long
  file. A headset, the notification and the lock screen go by file (the
  session's player tells the app where a press came from), so a tap
  miscounted by buds never moves the place
  ([009](009_resume_long_files.md#previous-owner-2026-10-11)).
- **Now playing:** in Shuffle's place, a Chapters and notes button; under
  the title, the chapter playing, which opens the same list. The episode's
  notes are in it, after the chapters. A book is not shuffled.
- **Hiding played episodes** is a synced setting, `podcasts.hidePlayed`,
  on by default, toggled from the show's menu.
- **The collection's speed** is set from the speed dialog when the
  "Only for this episode" switch is off. Setting it back to normal stores
  1×, rather than clearing it, so episodes don't fall back to the phone's
  everyday speed for music.
- **The app marks a file played itself** when it sends a `play` that
  reaches 90%, as of the listen's end, so the mark shows offline. The
  server makes the same mark at the same time, so the two never disagree.
- **A whole book can be downloaded** (a pin of kind `shelf`). Downloaded
  episodes and book files are listed apart in Downloads, so playing every
  download plays music only.
- **Found on the owner's library:** a book's files are numbered ("1 of 3")
  under their titles, because tags such as Mistborn 2's differ only in a
  `pt N` at the end, which a phone cuts off. A file with no cover of its
  own shows its book's or show's. In Search, the new sections come before
  Songs, which can run to hundreds and would bury them.
- **A transcript can be in the lyrics tag.** The owner's tool writes an
  episode's transcript into its lyrics tag as well, as the same Markdown,
  and a tag is read before the files beside it, so the app showed it raw.
  The server converts Markdown turns wherever it finds them; song lyrics
  never look like `**Name** (00:00:31): …`.
- **A file played to its end kept its end as the place to resume:** the
  pause at the end saved it, racing the end's clearing of it, and the file
  then reopened finished. No resume point is kept in the last 5 seconds.
  This was true of long music files before; every spoken file has a
  resume point now, so it showed at once.
- **A finished listen says where the player got to,** not the song
  list's length. One book's MP3s carry 300 KB of junk after their first
  ten frames, which ExoPlayer cannot read past; each part "finished" at
  0:00, and with the length as its end it was marked played. The owner
  remuxes such files; the app no longer takes an early end for a whole
  one.
- **An episode streamed through the song cache could not be sought.** A
  file still arriving had no length, and an Ogg file opened without one
  stays unseekable for as long as it is loaded. The cache now gives the
  song list's size until the server sends its own.
- **Some tools write the date as `20251230`;** the app shows it as
  2025-12-30. A finished book's button says Play, as it starts again.
- **The phone fetches the whole catalogue once** (library format 2), which
  `kinds=all` requires: songs it already has would not be sent again.

## Changed during implementation: macOS (2026-10-11)

- **Audiobooks and Podcasts are sidebar entries** under Library, each
  shown once its collection has something in it. The Mac's sidebar has
  room the phone's tab bar does not.
- **A book's or show's page is a table:** title, then the date for an
  episode or "3 of 19" for a book's file, then Played or the time left,
  then the length. Hiding played episodes is a button in the show's
  header.
- **Now playing has one line under the title,** so the chapter goes at
  its start: "Chapter 4 of 18 · Frank Herbert". The Chapters and notes
  button in Shuffle's place opens a popover; the lyrics panel's tab reads
  Transcript.
- **Next and Previous go by chapter in `Playback` itself,** so the
  buttons, the Controls menu, ⌘→ and ⌘←, the media keys and Control
  Center all do.
- **The playlist rule is kept in `Store`,** which drops episodes and
  books' files from every playlist edit: the song menu, a drag onto a
  playlist in the sidebar, and Save as Playlist from a queue.
- **The phone's fixes apply here too:** a finished listen says where the
  player got to, no resume point is kept in the last 5 seconds, and every
  spoken file keeps one. The song cache's fix was not needed: the Mac
  already opens a file still arriving with the song list's size.
- **A far seek into an MP3 still arriving waited for every byte before
  it.** Core Audio finds a place in an MP3 by walking every frame before
  it, and without a Xing or Info frame walks them all just to open the
  file. Dune resumed at 6:01 h after about 170 MB had arrived, a minute
  over Tailscale; a 7.8 h book without such a frame would not start until
  all 900 MB had. While the bytes before the place are missing, the Mac
  now estimates where it is as Media3 does, from the bitrate or the Xing
  table of contents, and decodes from the first frame there
  (`Mp3.swift`). The slice is opened behind the file's own Xing or Info
  frame, or one made up from the bitrate, so Core Audio has the length
  without counting. Dune now resumes in 0.1 s.
  - **How close it lands.** A constant bitrate lands within a frame. A
    Xing table only places a point to about 1/256 of the file, so a VBR
    file can land that far out (up to a minute in a generated 3 h test
    file), at the same audio the phone plays. A file with neither frame
    is taken to keep its first frame's bitrate, as Media3 takes it. In
    the test account's library the long books are constant bitrate; the
    VBR ones run 2 h at most. Once the bytes have arrived, the seek is
    Core Audio's own, and exact.
  - **Ten frames are decoded ahead of the place:** a frame's audio can
    begin in the frames before it (the bit reservoir), and without them
    the first tenth of a second was silent.
  - **Rejected: decoding MP3 frames ourselves** with `AudioFileStream`
    and a converter. It would replace Core Audio's reading, gapless
    trimming included, for every MP3, to fix the one case a slice
    covers.
- **A transcript already in LRC repeated its lines.** Some episodes
  carry YouTube's automatic captions as a `.lrc` file beside them,
  converted by FFmpeg, so each line came three times with a blank line
  between, as the captions scroll. The server collapsed them only in a
  `.vtt` file. It now keeps each line once in an episode's or a book's
  LRC too, from the file or the lyrics tag, by the same rule (a line
  among the last three is not written again). A song's LRC is sent as it
  is, since a chorus may repeat a line straight after itself.

## Changed during implementation: web (2026-10-11)

- **Audiobooks and Podcasts are sidebar entries** on a wide window, as on
  the Mac, and cards on the Playlists tab at phone width, as on the phone.
  A book's or show's page is the usual song list, with the date or "3 of
  19" and Played or the time left where a song shows its artist.
- **The Artists icon is now a person,** as on the phone. It was a
  microphone, which is Podcasts' icon.
- **The other apps' rules and fixes are here too:** the playlist rule in
  `App`, played marks made from a listen that reaches 90%, Next and
  Previous by chapter, the collection's speed, a resume point for every
  spoken file but none in the last 5 seconds, and a finished listen that
  says where the player got to.
- **A far seek does not stall here:** the browser asks the server for
  the bytes at the place, so Dune resumed at 6:01 h within seconds.
- **"Waiting for the network…" stayed under a file paused while a seek
  was still loading,** since the `playing` event it waits for never came.
  It now ends when the data arrives (`canplay`). Long files made this
  common; it was true of songs before.

## Changed after release (2026-10-15)

- **Captions in the lyrics tag came through raw.** Most episodes carry
  YouTube's WebVTT captions in their lyrics tag rather than the Markdown
  transcript, and the server converted only Markdown there; every app
  showed the VTT as plain text. A tag starting `WEBVTT` now goes through
  the same conversion as a `.vtt` file. That copy has lost the blank
  lines between cues, so the conversion now reads line by line rather
  than by blank-separated blocks. A server fix: no app changes, and no
  need to re-encode the files as `.lrc`.
- **An `.lrc` beside an episode or a book's file comes first** (owner),
  before the lyrics tag and any transcript or `.vtt`; that was already
  the order. It may now also carry a language as subtitle tools name it,
  `<name>.en.lrc`, before `<name>.en.auto.lrc`; the exact `<name>.lrc`
  still wins. A song's `.lrc` must still match its name exactly, so
  `Track 1.5.lrc` never becomes `Track 1`'s. Its repeated caption lines
  are dropped as before.

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
   the live database. Migrations run when the server opens the database,
   before any backup of that day, so the migration is tried on a copy of
   the newest snapshot in `data/backups/` before the release. It runs in one
   transaction: if it fails, nothing changed and the server does not start.
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
