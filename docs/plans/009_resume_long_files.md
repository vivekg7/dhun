# 009 — Resume long files on every device

**Status:** `MERGED` — server side running on the NAS (`server-v0.1.4`); Android on the owner's phone ([011](011_android_app.md)), no GitHub release yet
**Started:** 2026-10-07

## Problem

The collection holds audiobooks and podcasts as well as songs. Listening to
one is spread over days and broken up by music: an hour of a book, then
songs, then the book again on the laptop. A queue's position does not cover
this. The book may be in several queues or none, and playing songs moves the
queue on. Musicolet solves it on one phone ("remember last position of long
tracks", with a configurable minimum length, set to resume automatically or
to ask). Dhun has to do the same across the phone, the Mac and the web.

## Options

**A. Keep it on each device.** Each device remembers where it stopped, as
Musicolet does. Rejected: a book paused on the phone would start from the
beginning on the laptop. Continuing across devices is the point.

**B. Use the queue's current position.** No new state, but it is lost the
moment another song plays in that queue, and a book that is in two queues
has two positions.

**C. A resume point per user and song, synced like favorites.** It belongs
to the file, not to a queue, so it survives everything played in between,
and every device sees the same one.

## Decision

**C**, plus the settings that control it, synced the same way.

- **Resume points.** `resume.set {song, positionMs}` records where the user
  stopped; `resume.unset {song}` forgets it once the file is finished. Like
  favorites, the later `at` wins, so an offline phone reporting an older
  place later never moves the book back. A pull returns the points changed
  since the cursor, each with its `at`, so an app can show "Continue — 2 days
  ago".
- **The server does not decide what is long.** It stores a point for any
  song a client sends. The length rule lives in the apps, which know each
  song's duration from the catalogue. That keeps the rule configurable
  without a server change.
- **Synced settings.** `setting.set {name, value}` stores one app setting
  per user, as JSON, and every pull returns those that changed. The server
  reads only the Listen Later ones ([010](010_special_playlists.md)). This is the first setting a user expects to follow
  them to every device, so it is built generally rather than as two special
  columns:

  | Setting                | Values                                | Default  |
  | ---------------------- | ------------------------------------- | -------- |
  | `longFiles.minMinutes` | a number of minutes                   | `15`     |
  | `longFiles.resume`     | `"auto"`, `"ask"` (offer it), `"off"` | `"auto"` |

  A missing setting, or `null`, means the default. Names are 1–64 letters,
  digits, `.`, `_` or `-`; at most 1,000 settings of 4 KB each per user,
  not counting those set back to `null` ([016](016_speed_and_pitch.md)).

**What the apps do** (Android first; macOS and web the same way):

- When a file at least `longFiles.minMinutes` long starts and has a resume
  point, it continues from there (`auto`) or offers to (`ask`).
- While such a file plays, the app records `resume.set` on pause, on
  switching away, on stop, and every 30 s. The outbox keeps only the latest
  `resume.set` per song, so a long offline listen does not fill it.
- When the file plays to its end, `resume.unset`.
- The resume point wins over the queue's own position for a long file: it
  is the one kept up to date across devices.

The listening history ([008](008_listening_history.md)) is unchanged: each
sitting with a book is one listen.

## Rejected

- **Storing the length rule on the server**, and deciding there what is
  long: a server release for a preference, and the apps would still need the
  rule to act on it offline.
- **Two settings columns on `users`**: the next synced setting would need a
  migration of its own.
- **Telling podcasts and audiobooks apart by genre or folder:** the length
  rule is what Musicolet uses, needs no tagging, and covers long DJ mixes
  too. A folder rule can be added as a setting later if needed.

## Plan

1. Server: migration `0005`, the three operations, and the pull fields.
   Done.
2. Android: the behaviour above, and the two settings in the app's settings.
   Built ([011](011_android_app.md)).
3. macOS and web: the same, with the settings read from the sync.
