# 009 — Resume long files on every device

**Status:** `RELEASED` — in the 0.13.0 GitHub release (2026-10-12); server side running on the NAS (`server-v0.1.5`); Android on the owner's phone ([011](011_android_app.md))
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

  | Setting                 | Values                                    | Default  |
  | ----------------------- | ----------------------------------------- | -------- |
  | `longFiles.minMinutes`  | a number of minutes                       | `15`     |
  | `longFiles.resume`      | `"auto"`, `"ask"` (offer it), `"off"`     | `"auto"` |
  | `longFiles.previous`    | `"keep"`, `"seek"`, `"ignore"`, `"songs"` | `"keep"` |
  | `longFiles.next`        | `"next"`, `"seek"`                        | `"next"` |
  | `previous.restart`      | `true`, `false`                           | `true`   |
  | `longFiles.seekBack`    | seconds                                   | `10`     |
  | `longFiles.seekForward` | seconds                                   | `30`     |
  | `longFiles.rewind`      | seconds, `0` for none                     | `10`     |
  | `longFiles.rewindAfter` | minutes                                   | `5`      |

  `longFiles.next`, the last four and `longFiles.previous: "seek"` came
  later (see below).

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
- Choosing the file a queue is already on, in that queue's list, plays it
  on from where it is, not from its start: a tap meant to start it playing
  would otherwise cost the place.

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
  chapter playing in a long file. Since 2026-10-17 they go by file
  everywhere (below).

Rejected: ignoring Previous in long files everywhere (a button on screen
that does nothing looks broken), and an undo after a restart (the place is
already gone by the time it is noticed, in a pocket).

### Seeking, and going on after a pause (owner, 2026-10-17)

What the owner missed with podcasts:

- **Next and Previous never go by chapter,** on the screen either. A
  chapter skip on Now playing moved the place in an episode just as the
  headset's did, and the owner lost their place to it. Next goes to the
  next file, and Previous as set above; a chapter is reached from the
  chapter list, which the chapter line under the title opens.
- **Seek buttons for long files** on Now playing, either side of Play:
  back `longFiles.seekBack` (10 s) and forward `longFiles.seekForward`
  (30 s), the number drawn inside the arrow. Not for songs, nor in the
  mini players, where there is no room. Seeking forward stops at the
  file's end, which finishes it as reaching the end always does.
- **`longFiles.previous: "seek"`**: Previous from a headset, buds, the
  notification or the lock screen seeks back by `longFiles.seekBack`;
  on the screen it goes to the previous file, as `keep`. And
  **`longFiles.next: "seek"`**, the same for Next: from a headset it seeks
  forward by `longFiles.seekForward`.
- **The notification and lock screen seek too** in a long file: back and
  forward either side of Play (Media3's `SLOT_BACK` and `SLOT_FORWARD`),
  with the seconds in the icon for 5, 10, 15 and 30, and Previous and
  Next moved to the overflow. A song keeps Previous and Next there.
- **A long press on Previous or Next goes by chapter,** on Now playing
  and in the mini players, in a file with chapters. A long press is
  meant, as a tap may not be, so it may move the place: Next to the next
  chapter, Previous to this chapter's start, or within its first 3 s to
  the one before. It never leaves the file. In a file without chapters
  it does nothing, and the button keeps its tooltip.
- **Going on after a pause** of at least `longFiles.rewindAfter` minutes
  (5) starts `longFiles.rewind` seconds (10) before where the file
  stopped, so a sentence cut off is heard again from its start; `0` turns
  it off. It counts from when the file stopped playing, whatever stopped
  it (Pause, a call, a lost connection), and for a file started again
  from its resume point, from that point's time, so it holds across
  devices and restarts of the app. A file put in place without playing
  (a queue opened paused, Next while paused) counts as stopped since that
  place was heard: its resume point's time, or the moment it was put
  there if it went back for the point already or has none. Leaving a
  file still stopped where it was heard keeps its point as it was saved
  at the stop, since saving it again would date it to when the file was
  left. A seek while stopped chose the place, so nothing goes back from
  it. Never before the file's start.

Rejected: rewinding by how long the pause was (10 s after five minutes,
more after a day). The owner asked for a fixed amount, and one number is
easier to predict than a curve.

**On the Mac and the web** (built the same day), the same rules, in each
platform's way:

- **Chapters by name, not by holding, on the Mac:** "Next Chapter" and
  "Previous Chapter" in the Controls menu (⌥⌘→, ⌥⌘←) and on a right
  click of the Next and Previous buttons. A long press on a SwiftUI
  button does not sit reliably beside its click on macOS, and a menu is
  how a Mac offers a second action. Seeking is in the Controls menu too
  (⇧⌘←, ⇧⌘→), and the seek buttons sit in the Now playing bar and the
  menu-bar player, not the compact mini player.
- **The web holds for chapters** (500 ms, mouse, touch or pen; the
  click that follows is swallowed), and takes the Mac's keys: ⇧⌘←/→
  seeks and ⌥⌘←/→ goes by chapter (Ctrl for ⌘ elsewhere). Chrome keeps
  ⌥⌘←/→ for changing tabs and never passes it to the page, so with Shift
  added, ⌥⇧⌘←/→, goes by chapter too. It first had ⇧⌘←/→ go by chapter,
  so the same keys did different things on the Mac and the web. Its
  labels say "media keys and headsets", as its Ignore choice already did,
  since media keys take the same path there.
- **System controls:** Control Center on the Mac and the Media Session on
  the web get skip back and forward for a long file, with the seconds set,
  and treat their Previous and Next as a headset's. The web registers them
  again when a setting changes. The Mac cannot: Control Center draws the
  seconds the first time its skip buttons appear and keeps them until the
  app is relaunched, and it notices a file becoming long only at the next
  play or pause (tried, 2026-10-18, macOS 27). A skip there still goes by
  the seconds set now; only the number drawn is old.

### Progress in lists (owner, 2026-10-15)

Every song row with a resume point shows a thin bar under its title, how
far in the point is: in Continue listening, search, queues and any other
list. Not on Now playing, which has its seek bar. A point in the first 1%
draws no bar, since every episode played for a moment has one. The phone
reads every resume point once, as a map kept in memory, rather than each
row asking the database.

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
4. Previous: built on Android, macOS and the web.
