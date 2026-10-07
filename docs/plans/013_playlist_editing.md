# 013 — Editing playlists on Android

**Status:** `MERGED` — server side (`ref` in `/library`) on the NAS in `server-v0.1.2`; the app in 0.3.0 on the owner's phone
**Started:** 2026-10-07

## Problem

The first build ([011](011_android_app.md)) shows playlists and plays them,
but cannot change them. Musicolet lets you add songs to playlists from
anywhere, create one on the spot, save a queue as one, and reorder, remove,
rename and delete. The server already supports every one of these as a
sync op (`playlist.create` … `playlist.replace`, [006](006_api_and_sync.md)),
rewriting the `.m3u8` file; only the app is missing.

## Decisions

The scope and two behaviours are the owner's (2026-10-07).

### What the app can do

**Full editing**, Musicolet's set minus import and export:

- **Add to playlist** from a song's menu, and **add all** from an album,
  folder, artist, genre, Favorites, Listen Later or one of the automatic
  views, through the page's menu. The picker lists your playlists and
  **New playlist**.
- **Save a queue as a playlist**, from the queue's menu.
- **New playlist** at the top of the Playlists section of the Playlists tab.
- On your own playlist's page: **drag to reorder**, **remove** a song,
  **rename** and **delete**.

### Duplicates: skipped, and said so

A song already in the playlist is **left where it is**, and a short note says
how many were skipped ("Added 3 to Road trip · 2 already there"). The
`.m3u8` format and the server allow duplicates; the owner chose not to make
them by accident. Rejected: asking each time, and always adding.

### Shared playlists stay read-only in the app

Shared playlists (top-level `Playlists/`) are edited only by the admin, on
the server's rules. The owner chose to **keep them read-only in the app**,
for every user: they are not in the picker and their pages have no edit
controls. They are edited on the NAS.

### Playlists made offline

A new playlist gets a client `ref` and a temporary negative id, and every
later op names it as `ref:<ref>` until the server's id is known; the server
already accepts that. To learn the id, **`/library` now returns each
playlist's `ref`**: when the playlist the app made comes back, the
temporary row is replaced, and a download pin on it follows to the new id
([012](012_downloads.md)). A server that predates `ref` is matched by name
among the user's own playlists.

### Local edits are not overwritten by an older copy

The app edits its copy at once and queues the op, as for queues. A library
pull skips a playlist with an op still in the outbox, and then **does not
advance the library cursor**, so the next pull brings that playlist again
once the op has gone. After pushing playlist ops, the app pulls the library
again in the same sync, so the server's version (the `.m3u8` as written)
arrives at once.

### How edits are sent

- Add: `playlist.insert` at the end, with only the songs not already there.
- Remove: `playlist.remove` with the song and which copy of it
  (`occurrence`), counted the way the server counts, unmatched entries (0)
  included.
- Reorder: `playlist.move` after the new previous song. When that neighbour
  is an entry that matches no song (0, which the op cannot name), the whole
  order is sent with `playlist.replace`.

## Open questions

None. Import and export of `.m3u` files, sorting, and "remove unavailable
songs" are Musicolet features left for later.
