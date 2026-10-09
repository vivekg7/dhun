# 025 — Phone-local songs on Android

**Status:** `MERGED` — in 0.9.0 on the owner's phone (2026-10-09); checked on the emulator
**Started:** 2026-10-08

## Problem

The last unbuilt Android item in the v1 scope: songs already on the phone,
played beside the NAS songs ([REQUIREMENTS](../REQUIREMENTS.md#phone-local-songs)).
Musicolet's whole library is phone-local, so a phone that holds music Dhun
cannot see is a step back from what the owner used for ten years.

The rule since the start is that phone songs and NAS songs are kept apart,
even when they are the same track, and that a phone song never reaches the
server ([007](007_client_architecture.md)). The code was built on the
opposite assumption, and that is what makes this risky:

- **Every song is a server song.** The ID is the server's `Long`; the
  catalogue is the `songs` table the server fills; playback turns an ID into
  `stream/{id}`; listens, resume points, favourites and per-song speed all
  send the ID.
- **The server rejects an op naming a song it does not know**
  (`songsExist`, `server/internal/api/sync.go`), and the phone drops a
  rejected op from the outbox. One phone song in a `queue.insert` would
  silently lose the whole edit, NAS songs included. Phone songs have to be
  taken out _before_ an op is written, never after.
- **A pull replaces a queue's song list** with the server's unless an op for
  it is pending, and play stats are cleared and rewritten. Either would
  erase the phone songs.

## Decisions with the owner (2026-10-08)

- **Mixed in, with a mark.** Phone songs appear in Albums, Artists, Genres
  and search beside the NAS songs, with a small phone mark. Folders shows
  an **On this phone** root beside the NAS tree.
- **All audio Android knows about, minus exclusions.** Settings gain
  excluded folders and a minimum length (default 30 s, so ringtones,
  notification sounds and voice notes stay out).
- **Queues only.** A phone song can be queued, but not added to a playlist.
  NAS playlists stay `.m3u8` files the server owns, with no phone-only
  entries hidden in them. Phone-only playlists can come later. This
  replaces "a queue or playlist may contain both" in REQUIREMENTS.
- **Favorites, Listen Later, play counts and resume points, on the phone
  only.** Never synced; lost if the app's data is cleared.

## Options

### Identity

- **A. MediaStore's `_ID`, negated.** Free and unique. But Android hands out
  new IDs when it rebuilds its media index (an OS update, "clear storage" on
  Media Storage), and every favourite and count would go with them.
- **B. A hash of the file's place, negated.** 63 bits of a hash of volume +
  `RELATIVE_PATH` + `DISPLAY_NAME`. It survives a rebuilt index, and a file
  that goes missing and comes back finds its data again, as in Musicolet's
  "excluded or missing" handling. Moving a file loses its data, as it would
  in Musicolet.
- **C. A separate `LocalSong` type** through the whole app.

**Chosen: B.** A negative `Long` fits every table, every comma-joined ID
list and every `mediaId` as they are: one type and one rule (`id < 0` means
phone). C would double the types in the catalogue, queues and player for no
behaviour gain. At a few thousand files a 63-bit collision is not a real
risk. The content URI is looked up from the hash's row at play time, so the
`_ID` is never stored. The NAS song rule of "user data points at an ID,
never a path" still holds; a phone song simply has no ID other than its
place.

### Where the list lives

- **A. Rows in the `songs` table.** The server pull writes that table, and
  `missing` and the library cursor are the server's.
- **B. Read from MediaStore into memory** at start and when its
  `ContentObserver` fires, then merged into `Catalog`.

**Chosen: B.** The index is already on the phone and a query of a few
thousand rows takes well under a second; a copy in Room would be a second
truth to keep in step. The catalogue is already built in memory.

## Plan

1. **Permission and setting.** `READ_MEDIA_AUDIO`, plus
   `READ_EXTERNAL_STORAGE` with `maxSdkVersion 32` (minSdk is 31).
   Settings → Library gains **Songs on this phone**, off until turned on;
   turning it on asks for the permission. Off, nothing is read and the app
   is as today. Excluded folders and the minimum length sit under it.
   Dhun's own downloads and cache are under `Android/data/`, which
   MediaStore does not index, so they never show as phone songs.
2. **Reading.** `data/LocalSongs.kt`: one query of
   `MediaStore.Audio.Media` (title, artist, album, album artist, composer,
   genre, year, track, duration, size, MIME type, bitrate, date added,
   relative path, name), filtered by the exclusions, mapped to `Song` with
   the hashed ID. Re-read on the observer, debounced.
3. **Catalogue.** `Catalog` takes both lists. Albums are keyed with the
   source, so a phone copy of a NAS album is a second album, not doubled
   tracks in one. Artists and genres are merged: they only group songs. A
   phone mark on songs and phone albums. Folders gains the **On this
   phone** root, built from `RELATIVE_PATH`.
4. **Playing.** A phone song's `MediaItem` gets its content URI, which
   `SongSource` already passes through to `DefaultDataSource`. Covers come
   from `ContentResolver.loadThumbnail`. Gapless, tempo and the sleep timer
   need nothing more. Downloads, the cache and prefetch skip phone songs:
   they are already on the phone.
5. **The sync guard — the part that can fail.** In `Store`, before an op is
   written:
   - lists (`queue.create`, `queue.replace`, `queue.insert`,
     `queue.remove`) drop phone songs; an op left with nothing to say is not
     written;
   - an `after` naming a phone song becomes the nearest NAS song before it
     (or the start);
   - `queue.move`, `queue.set_current`, `favorite`, `listen_later`,
     `resume` and `play` for a phone song are not written; marks, resume
     points and play stats are written straight to Room instead;
   - per-song speed for a phone song goes to a local preference, not to the
     synced `speed.<id>` setting.

   One test drives every op builder with a phone song and checks that no
   negative ID reaches the outbox.

6. **Pulls keep phone data.** A queue's server list is merged, not
   applied: each phone song goes back after the NAS song it followed, or
   at the start if that song has gone. Mark, resume and play-stat pulls
   keep rows with `song < 0`. A test pins the merge, including a queue
   whose NAS songs were all removed elsewhere.
7. **Playlists.** "Add to playlist" and "Save queue as playlist" leave
   phone songs out and say so ("2 songs on this phone were left out"). A
   selection of only phone songs does not offer it.
8. **Hand-off.** While a phone song plays, the phone does not report
   `playback.state`; there is nothing another device could continue. The
   other devices see the queue without its phone songs, which answers the
   question left open in [002](002_sync_and_handoff.md).

The macOS app needs nothing: it has no phone songs ([024](024_macos_app.md)),
and the queues it pulls simply lack them.

## How the build went (2026-10-09)

**Checked:** the unit tests (`PhoneSongsTest`: every op builder with a
phone song in it, the pull merge, the ID), and on the emulator against a
throwaway server: the switch asks for the permission; the media index's 22
songs appear beside the server's 37, under **On this phone** and with the
mark, without a restart once Android had scanned them; a phone album plays;
its songs offer neither Add to playlist nor Download; a favourite on one
stays on the phone. A NAS song added to the queue reached the server, as the
queue `[34]`. When another device then put two songs at its start, the
phone's next sync kept its five phone songs and the one playing.

**Changes from the plan above:**

- **One guard, not one per op.** `Store.forServer` sits in `record()`,
  where every op passes on its way to the outbox. An op added later is
  covered without anyone remembering to.
- **Not music, whatever its length.** Files Android flags as a ringtone,
  notification, alarm or recording are left out, as well as short ones.
  Podcasts and audiobooks stay.
- **The notification's cover** is Android's album-art URI for the file's
  album (kept in the song's `art`): the media session loads covers by URI,
  and the audio file's own URI is not an image.
- **Signing out** cleared the phone songs' favourites, counts and resume
  points with everything else, since the database is one. Since
  [026](026_without_an_account.md) it keeps them.
- **Waiting for the library.** Downloads and the reload of the last queue
  wait for the NAS songs, not for any song: phone songs alone would read as
  "the library is empty, delete every download".

**Known limit:** with the switch turned off, a queue's phone songs are not
loaded, so an edit to the playing queue then drops them from it.

## Rejected

- **Phone entries inside NAS playlists.** Every other device would see a
  different list, and the phone would have to keep its entries in place
  around the server's rewrites of the `.m3u8`. Rejected by the owner for
  now.
- **Matching a phone file to the same NAS song.** REQUIREMENTS keeps them
  apart; revisit only if duplicates become a nuisance.
- **Our own file scanner** (Musicolet offers one). MediaStore already reads
  the tags, and a scanner would need broad storage access.

## Open questions

- **Lyrics for phone songs** are out of this plan. Embedded lyrics need a
  tag reader on the phone, and a sibling `.lrc` is not audio, so
  `READ_MEDIA_AUDIO` does not reach it. Revisit after daily use.
- **"Open in Dhun"** from the open-with dialog
  ([021](021_open_from_other_apps.md)) becomes possible once a file has a
  phone-song ID. Left out until this plan is in use.
- **Signed out:** the app still needs a sign-in to start, so phone songs
  cannot be played without an account. Fine for one family; the owner can
  override.
