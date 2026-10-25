# 021 — Playing audio files opened from other apps

**Status:** `RELEASED` — in the 0.13.0 GitHub release (2026-10-12); in 0.7.0 on the owner's phone
**Started:** 2026-10-08

## Problem

Dhun was not in Android's "Open with" list for audio files, so a song in a
file manager, a voice note saved from a chat or an attachment needed a
second player. Musicolet plays such files in a floating mini-player over the
app that asked (inventory §9, "Floating mini-player dialog"), and the owner
wants the same: a dialog over that app, never a jump into Dhun.

## Options

**Where it plays**

- **A dialog-themed activity of its own** _(chosen)_. Declared for
  `ACTION_VIEW` on `audio/*`, drawn as a card over a dimmed screen. An
  empty `taskAffinity` keeps it out of Dhun's task, so the app does not come
  forward behind it and closing it returns to the app that asked. Needs no
  permission.
- **A real floating window** over every app (`SYSTEM_ALERT_WINDOW`).
  Rejected: a permission the user has to grant in system settings, for a
  bubble nobody asked to keep on screen after leaving the app.
- **Open `MainActivity`** and play it there. Rejected: exactly what the
  owner did not want.

**Which player**

- **Its own small ExoPlayer, in the activity** _(chosen)_. The file has no
  song ID, so nothing about it can be synced, counted or put in a playlist,
  and the `content://` grant ends with the activity anyway.
- **The queues' player, through `PlaybackService`**, with the file as a
  queue. Rejected: a one-off voice note would take one of the 20 queues,
  push aside the queue it interrupted, and hold a link that stops working.
  Queues are for songs played through, not a glance at an attachment.

## Decision

The owner's answers (2026-10-08):

- **It makes no queue.** The dialog's player takes the audio focus, so the
  playing queue pauses where it is, as for any other app, and is not
  resumed when the dialog closes.
- **Closing stops it.** ✕, Back, a tap outside the card, or leaving for
  another app (Home, Recents) ends the file: with no notification, nothing
  could stop it once the dialog had gone. The exception is the screen
  turning off: the file plays on, and unlocking shows the dialog again.
- **No "Open in Dhun".** There were no phone-local songs to open it into;
  [025](025_phone_local_songs.md) added them and leaves this open until
  they are in daily use.
- **Nothing is logged or synced**: no listen, no play count, as for
  phone-local songs.

It shows the cover, title and artist from the file's tags (its name when
it has none), play/pause, and a seek bar with the time. A file ExoPlayer
cannot play (a `.m3u` playlist also arrives as `audio/*`) says so instead
of the artist. A second file replaces the first (`singleTop`).

The file starts in `onResume`, not `onCreate`: Android gives the audio
focus only to the app in front, and an earlier request was dropped at once.

Only `content://` links: since Android 7 another app cannot hand over a
`file://` path, and reading one would need a storage permission Dhun does
not ask for. `application/ogg` is declared too, as some file managers label
Ogg files that way.

## Open questions

None.
