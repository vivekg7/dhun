# 017 — Hand-off (resume) on Android

**Status:** `MERGED` — in 0.3.0 on the owner's phone
**Started:** 2026-10-07

## Problem

v1's hand-off is H1, resume ([002](002_sync_and_handoff.md)): opening a
device offers to continue what another device was playing. The server has
had it from the start: every device's `playback.state` op updates the
user's one `now_playing` record, which comes back in each sync and from
`GET /api/v1/now-playing` ([006](006_api_and_sync.md)). The app sent its
state but never offered anything.

## Decisions

Where the offer shows, when, how old it may be, and which queue continues
are the owner's (2026-10-07).

### The offer

**A bar above the mini player, on every tab**: "Continue from MacBook —
Shiver · 2:13", with **Continue** and **✕**. Seen wherever the app opens,
blocking nothing. Rejected: on Now playing only (missed when the app opens
elsewhere), and a dialog (in the way).

It is offered when all of these hold:

- **This device is not playing.** If it is, this is where the user is
  listening.
- The playback is **another device's** and **newer than anything this device
  reported**. No age limit beyond that, by the owner's choice: last night's
  laptop session is still offered in the morning if the phone has played
  nothing since. Comparing with this device's own last report is what makes
  a pause here, or accepting, end the offer.
- It was not dismissed. ✕ remembers that one playback; the other device's
  next report is offered again.
- Its queue and song are on this device (both sync).

### Continue: the same queue

Queues are synced, so **this device switches to the queue the other was
playing**, at its song and place, and plays. Rejected: a copy of the queue,
which would multiply queues for every hand-off. Only that queue moves; the
others are already the same everywhere (002's open question).

If the other device was still playing at its last report, the place has
moved on since by the time passed; it is capped 5 s before the song's end,
since the device may have been closed mid-song. The bar shows the place
Continue will use.

### What each device reports

`playback.state` on play and pause, on each new song while playing, and
every 30 s while playing, with the 30-second save of the queue's position
(002's open question: often enough that a hand-off resumes within half a
minute, rarely enough to cost nothing). It goes through the outbox under one
key, so only the latest waits while offline.

### Knowing which device is this one

The app keeps its device id from sign-in. A device signed in before this
asks `GET /api/v1/me` once. Without it, the app's own playback would be
offered back to it.

### Fetched on coming to the foreground

The sync returns `now_playing` only when it changed since the cursor, so
after a restart the app would never see it again. On coming to the
foreground the app asks `GET /api/v1/now-playing`, before syncing, and a
sync with no news leaves the last one in place instead of clearing it.

## Open questions

None for v1. Live transfer ("Play here", H2) and phone-local songs in a
handed-off queue come with those features.
