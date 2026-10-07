# 014 — The sleep timer on Android

**Status:** `MERGED` — in 0.3.0 on the owner's phone
**Started:** 2026-10-07

## Problem

REQUIREMENTS lists a sleep timer for v1: stop after a set time, after N
songs, or after a song. Musicolet has one, and people fall asleep to music.
The app has none.

## Decisions

The ways to set it, how it stops and where it shows are the owner's
(2026-10-07).

### What it can do

The dialog offers three, one picked by its radio button and set with
**Start** (the owner's design, 2026-10-07):

- **After a time**, as hours and minutes, 00:30 at first. Each box can be
  typed in or stepped with a small arrow above and below it: hours by 1,
  minutes by 5, carrying into the hours (00:55 up is 01:00).
- **After N songs**, 5 at first, stepped by 1; the playing song counts, and
  every song that starts counts, skips included. **1 is the end of this
  song**, so that needs no option of its own.
- **At the end of this queue:** stop after its last song instead of
  repeating.

Rejected, both tried first: presets (15 to 90 minutes, 2 to 10 songs) as
chips, which wrapped into clutter, and the same presets as a list of rows
with "Other…" for the rest, twelve rows long. Two steppers hold every value
in less room, and the usual one is a tap or two from the default.

A device-only, in-memory setting: it is about this night on this phone, so
it is neither synced nor kept across an app restart.

### How it stops

- **By the clock: a fade, then a pause on time.** The volume falls over
  the last 10 s and playback pauses when the time is up, mid-song if need
  be; the volume is back to normal for the next play. The owner chose this
  over letting the song finish (the timer could then run several minutes
  over) and over a hard stop.
- **By songs: a pause where the song ends**, using ExoPlayer's
  `pauseAtEndOfMediaItems`, which needs no fade and never cuts a song.
  Pressing play then goes on to the next song. "End of the queue" turns
  this on only while the playing song is the last one, rechecked whenever
  the queue, its order or shuffle changes.

Manually pausing does not stop the clock: the timer is about when you fall
asleep, not how much music you hear.

### Where it shows

- **A moon on Now playing**, next to repeat and shuffle, accented while a
  timer runs, with the time left (or the songs left) beside it. It opens
  the timer, which also turns it off.
- **In the notification and on the lock screen**, after the artist:
  "Coldplay · Sleep in 23 min". The session sees the player through a thin
  wrapper that adds this to the metadata and refreshes it every 30 s.
  Bluetooth displays that show the artist show it too.

Rejected: a custom notification button. Android's media notification shows
custom actions as icons only, so it could not say how long is left.

## Open questions

None.
