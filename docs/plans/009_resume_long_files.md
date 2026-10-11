# 009 — Resume long files on every device

**Status:** `MERGED` — server side running on the NAS (`server-v0.1.5`); Android on the owner's phone ([011](011_android_app.md)), no GitHub release yet
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
  | `longFiles.previous`   | `"keep"`, `"ignore"`, `"songs"`       | `"keep"` |
  | `previous.restart`     | `true`, `false`                       | `true`   |

  A missing setting, or `null`, means the default. Names are 1–64 letters,
  digits, `.`, `_` or `-`; at most 1,000 settings of 4 KB each per user,
  not counting those set back to `null` ([016](016_speed_and_pitch.md)).

**What the apps do** (Android first; macOS and web the same way):

- When a file at least `longFiles.minMinutes` long starts and has a resume
  point, it continues from there (`auto`) or offers to (`ask`). Every
  podcast and audiobook file counts as long, whatever its length
  ([031](031_podcasts_and_audiobooks.md)).
- While such a file plays, the app records `resume.set` on pause, on
  switching away, on stop, and every 30 s. The outbox keeps only the latest
  `resume.set` per song, so a long offline listen does not fill it.
- When the file plays to its end, `resume.unset`.
- The resume point wins over the queue's own position for a long file: it
  is the one kept up to date across devices.

The listening history ([008](008_listening_history.md)) is unchanged: each
sitting with a book is one listen.

### Previous (owner, 2026-10-11)

Previous restarted a file played past its first 3 s, as it does a song.
In a book that lost the place: the restart plays on from 0:00, and the next
`resume.set` records it. Buds miscount taps when they are damp, so one
mistaken Previous could cost the owner their place hours into a book. So:

- **`longFiles.previous`**, for the files that resume. `keep` (the
  default): Previous never goes back in the file. It goes to the previous
  file, the place saved as it leaves, so coming back continues there.
  `ignore`: the same on the screen, and nothing from a headset, the
  notification or the lock screen. `songs`: as with songs, the old
  behaviour. The default changed, rather than the old behaviour staying,
  because nobody finds this trap until they have lost their place.
- **`previous.restart`**, for everything else: `false` makes Previous
  always go to the previous song, never restart this one.
- **Chapters only on the screen.** A headset, the notification and the
  lock screen go by file, Next as well as Previous: a chapter skip moves
  the place within the file just as a restart does. Now playing and the
  mini players still go by chapter, and Previous there never restarts the
  chapter playing in a long file.

Rejected: ignoring Previous in long files everywhere (a button on screen
that does nothing looks broken), and an undo after a restart (the place is
already gone by the time it is noticed, in a pocket).

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
4. Previous: Android built; macOS and web to follow.
