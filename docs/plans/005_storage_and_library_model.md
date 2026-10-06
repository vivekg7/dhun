# 005 — Storage and library model

**Status:** `IN PROGRESS` — implemented in `server/` (migrations 0001–0002); not yet on `main`
**Started:** 2026-10-06

## Problem

The backend ([001](001_own_backend_in_go.md)) has to:

- index about 7,000 files under `Music/`, excluding `_`-prefixed folders;
- keep each user's queues, favorites, play history and now-playing state;
- read and write `.m3u8` playlists that remain the source of truth for
  playlists;
- keep all of that attached to the right song when files are moved, upgraded
  or changed outside Dhun ([004](004_curation_workflow.md)).

The state lives in `Music/_dhun/` ([003](003_deployment.md)).

## Decisions

### Database: SQLite, pure-Go driver

- **SQLite in WAL mode** at `_dhun/dhun.db`. A few users and a few thousand
  rows need no database server.
- **`modernc.org/sqlite`** (pure Go) rather than `mattn/go-sqlite3` (cgo).
  With no cgo the binary stays static and the Docker image can be `scratch`
  or distroless. Its FTS5 support gives us search.
- **Migrations are numbered `.sql` files** embedded in the binary and applied
  at startup inside a transaction, tracked in a `schema_version` table. That
  is about 30 lines of our own code instead of a migration library.
- **Backups:** each night, `VACUUM INTO '_dhun/backups/dhun-YYYY-MM-DD.db'`,
  keeping 14. Copying a live WAL database file is not guaranteed to be
  consistent, so whatever backs up `Music` (Hyper Backup, Snapshot
  Replication) picks up these snapshots instead.

### Song identity

- **`songs.id` is a stable integer** that never changes and is never reused.
  All user data references it, never a path.
- A song row records its current `path`, plus `size`, `mtime` and a
  **quick hash**: SHA-256 of the size, the first 64 KB and the last 64 KB.
  Hashing every 53 GB file on each scan would be slow on HDDs; the quick hash
  identifies the same file at a new path with no meaningful collision risk at
  this scale.
- **Changes made through Dhun** (admin moves and upgrades, after v1) update
  the row directly. The ID stays and the path or file changes.
- **Changes made outside Dhun** (SMB, or the old curation tool until it is
  retired) are reconciled on rescan:
  1. Same path with the same size and mtime → unchanged; skip without reading.
  2. Same path but changed → re-read the tags; same ID.
  3. A path that disappeared and a new path with the same quick hash → a
     move; same ID, path updated.
  4. A path that disappeared with no hash match → keep the row and its user
     data, set `missing_since`, and hide the song. If it reappears, it comes
     back with its play counts and favorites (Musicolet does this too).
  5. A new path with no match → a new song.
- Duplicate detection by audio fingerprint is part of the admin panel's
  review (after v1), not of the scanner.

### What a song row holds

Tags read on scan: title, artist(s), album, album artist, composer, genre(s),
year, track and disc numbers, duration, codec, bitrate, sample rate, plus
embedded-art and lyrics flags (embedded or a sibling `.lrc`). There are no
separate artist or album tables: at 7,000 rows, grouping songs by
`album_artist, album` is instant, and a table that only mirrors grouped
columns would go stale. Multi-value tags (`Artist A, Artist B`) are stored
both raw and split, so browsing by artist works.

Folders are not stored. A folder view comes from the path prefixes.

**Tag reader:** taglib through a pure-Go WebAssembly build
(`go.senan.xyz/taglib`, by the author of Gonic). It covers MP3, M4A, Opus and
FLAC, including duration, without cgo. To verify at implementation time; the
fallback is `dhowden/tag` for tags plus our own duration parsing.

### Per-user data

| Table                | Holds                                                                                                                                                                                                                                                                                                                                       |
| -------------------- | ------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| `users`              | Name (also the `Playlists/<name>/` folder), password hash (argon2id), `is_admin`.                                                                                                                                                                                                                                                           |
| `devices`            | One row per signed-in app install: a name ("Pixel", "MacBook"), a token hash, `last_seen`. Shown in hand-off ("Continue from Phone").                                                                                                                                                                                                       |
| `queues`             | Per user, at most 20, with a **client-generated ID** (so a queue made offline can be referred to before the server sees it): name (unique per user), the songs as a JSON array of IDs (no duplicates, Musicolet), current song, position, shuffle and repeat, `used_at`. A 21st queue removes the least recently used one, as in Musicolet. |
| `favorites`          | `(user_id, song_id, at)`.                                                                                                                                                                                                                                                                                                                   |
| `plays`              | Append-only `(user_id, song_id, device_id, at, ms_played)`. Play counts, last played and most played are queries over it; nothing is stored twice.                                                                                                                                                                                          |
| `now_playing`        | One row per user: device, queue, song, position, playing or paused, `updated_at`. This is what hand-off reads ([002](002_sync_and_handoff.md)).                                                                                                                                                                                             |
| `users.data_version` | A per-user counter; every queue, favorite and now-playing row records the version it last changed at, so a client can ask "what changed since N?" ([006](006_api_and_sync.md)).                                                                                                                                                             |
| `applied_ops`        | IDs of client operations already applied, so a retried sync is harmless.                                                                                                                                                                                                                                                                    |

### Playlists: `.m3u8` is the truth, the database is an index

- `Playlists/*.m3u8` are shared; `Playlists/<user>/*.m3u8` belong to that
  user ([REQUIREMENTS](../REQUIREMENTS.md#users-and-the-library)).
- On scan, and whenever a playlist file's mtime changes, Dhun parses it into
  `playlists` and `playlist_items`, resolving each relative path to a song ID.
  An entry that resolves to nothing is kept as its raw line and shown as
  unavailable; it is never dropped, because the file may be fixed later.
- **When Dhun edits a playlist** (for example, a user adds a song), it
  rewrites the file atomically (write a temporary file, then rename),
  preserving the header and any comments, and writing `#EXTINF` lines and
  relative paths in the existing style. Then it re-indexes the playlist.
- When an admin action moves a song, every playlist that contains it is
  rewritten in the same operation ([004](004_curation_workflow.md)).
- Two edits to the same playlist are serialised by a per-file lock in the
  server; with 3–4 users, contention is not a concern.

## Changed during implementation (2026-10-06)

- **Queue songs are one JSON array per queue**, not a `queue_items` table
  with sparse sort keys. Queues hold at most a few thousand songs, so
  rewriting the array on each edit costs nothing, and every edit becomes a
  single-row update with no ordering arithmetic.
- **A per-user `data_version` counter replaces the `changes` table.** Each
  object records the version it last changed at, which answers "what changed
  since N?" without a second table that has to stay consistent with the
  first.
- **The duration accuracy of the taglib WebAssembly library is confirmed**:
  within 50 ms of ffprobe for MP3, M4A, Opus and FLAC.

## Rejected

- **Postgres:** a second container to run and back up, for no gain at this
  scale.
- **Path as identity:** every move or upgrade would orphan play counts.
- **Full content hash:** reading 53 GB on every scan of an HDD array, to beat a
  collision risk that the quick hash already makes negligible.
- **Artist and album tables:** they duplicate what grouping already gives us.
  Revisit if we ever need per-artist metadata, such as images or bios.
- **Playlists only in the database:** the owner wants them as files that are
  usable in any player.

## Open questions

None.
