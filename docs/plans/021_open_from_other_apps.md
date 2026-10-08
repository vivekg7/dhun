# 021 — Playing audio files opened from other apps

**Status:** `MERGED` — in 0.7.0 on the owner's phone
**Started:** 2026-10-08

## Problem

Dhun did not appear in Android's "Open with" list for audio files. A song
in a file manager, a voice note saved from a chat, or an attachment could
not be played with it, so the phone still needed a second music player.
Musicolet both opens such files and plays them in a floating mini-player
over the app that asked (inventory §9, "Floating mini-player dialog"), and
the owner wants the same: a dialog over that app, never a jump into Dhun.

## Options

**Where it plays**

- **A dialog-themed activity of its own** _(chosen)_. Declared for
  `ACTION_VIEW` on `audio/*`, drawn as a card over a dimmed screen. With an
  empty `taskAffinity` it never joins Dhun's own task, so the app does not
  come forward behind it, and closing it returns to the app that asked.
  Needs no permission.
- **A real floating window** over every app (`SYSTEM_ALERT_WINDOW`).
  Rejected: a permission the user has to grant in system settings, for a
  bubble nobody asked to keep on screen after leaving the app.
- **Open `MainActivity`** and play it there. Rejected: exactly what the
  owner did not want.

**Which player**

- **Its own small ExoPlayer, in the activity** _(chosen)_. The file is not
  in the library: it has no song ID, so nothing about it can be synced,
  counted or put in a playlist, and the `content://` grant Android gives us
  ends with the activity anyway.
- **The queues' player, through `PlaybackService`**, with the file as a
  queue. Rejected: a one-off voice note would take one of the 20 queues and
  push the queue it interrupted aside, and the queue would hold a link that
  stops working. Queues are for songs being played through, not a glance
  at an attachment.

## Decision

The owner's answers (2026-10-08):

- **It makes no queue.** The dialog's player takes the audio focus, so
  whatever queue was playing pauses where it is, as for any other app. It
  is not resumed when the dialog closes; the user resumes it.
- **Closing stops it.** ✕, Back, a tap outside the card, or leaving for
  another app (Home, Recents) ends the file. With no notification, nothing
  could stop a file still playing after the dialog had gone. The screen
  turning off is the exception: the file plays on, and unlocking shows the
  dialog again.
- **No "Open in Dhun".** Phone-local songs are not built yet, so there is
  nothing in the app to open it into. Revisit with them.
- **Nothing is logged or synced**: no listen, no play count. The same rule
  as phone-local songs.

What it shows: the cover, title and artist from the file's tags (its
name when it has none), play/pause, and a seek bar with the time. A file
ExoPlayer cannot play (a `.m3u` playlist also arrives as `audio/*`) says
so instead of the artist. A second file opened while the dialog shows
replaces the first (`singleTop`).

The file starts playing in `onResume`, not `onCreate`: Android gives the
audio focus only to the app in front, and asked any earlier the request
was dropped at once and the file never started.

Only `content://` links: since Android 7 another app cannot hand over a
`file://` path, and reading one would need a storage permission Dhun does
not ask for. `application/ogg` is declared too, as some file managers
label Ogg files that way.

## Open questions

None.
