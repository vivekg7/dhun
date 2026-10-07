# 015 — Lyrics on Android

**Status:** `MERGED` — in 0.3.0 on the owner's phone
**Started:** 2026-10-07

## Problem

REQUIREMENTS lists synced and plain lyrics for v1. The server already serves
them (`GET /api/v1/lyrics/{id}`: the sibling `.lrc` first, then embedded
tags, [006](006_api_and_sync.md)), and the curation workflow fetches an
`.lrc` for most albums. The app does not show them.

## Decisions

Where they show, what a tap does, offline and the screen are the owner's
(2026-10-07).

### In place of the cover

**A tap on the cover swaps it for the lyrics**, in the same space, as in
Musicolet; a lyrics button on Now playing, next to the moon, does the same
and brings the cover back. The button is dimmed for a song with no lyrics.
The lyrics stay up from song to song until turned off. Rejected: a
separate full-screen page, which would hide the controls.

### Synced lyrics follow the song

- The playing line is highlighted and kept in the middle; half a screen of
  space at both ends lets the first and last lines get there.
- **A tap on a line plays from it.** The owner chose this over taps doing
  nothing. A tap anywhere else, or anywhere on plain lyrics, brings the
  cover back.
- Scrolling by hand stops the follow for 4 s, to read ahead.
- The position is read every 200 ms while the lyrics show, paused or not,
  so a tap on a line while paused moves the highlight too. For the same
  reason the seek bar now follows the position while paused as well.

### The parser

Our own, about forty lines ([007](007_client_architecture.md)): `[mm:ss.xx]`
stamps with one to three fraction digits, several stamps on one line for a
repeated chorus, `[offset:±ms]`, and enhanced-LRC word stamps
(`<mm:ss.xx>`), which are dropped; word-by-word highlighting is not worth
its code for this family. Text with no stamps is plain lyrics, shown
without `[ar:…]`-style tags. The server's `synced` flag is not trusted on
its own: lyrics are synced when the parser finds stamped lines.

### Offline: kept with downloads, and once viewed

Every copy fetched is kept in Room (`lyrics` table), so lyrics show without
the server for:

- **Downloaded songs**: the download run fetches the lyrics of every
  downloaded song that has some and is not kept yet, so songs downloaded
  before this, or that gain an `.lrc` later, catch up. Lyrics are a few
  kilobytes, so this runs on mobile data too, Wi-Fi-only or not, and a
  failure never stops the files.
- **Any song whose lyrics were viewed.** They are kept when the song is no
  longer downloaded; 7,000 songs' worth is a few megabytes at most.

The kept copy shows at once and the server's replaces it when it differs,
so an `.lrc` fixed on the NAS reaches the phone the next time it is viewed.
A song the server has no lyrics for (`hasLyrics` false, or a 404) says so;
one not kept, with the server out of reach, says they will show when it can
be reached. Sign-out clears the table with the rest of the database.

### The screen stays on

While the lyrics show **and** the song plays (the window's
`keepScreenOn`); pausing or going back to the cover lets the screen sleep.

## Open questions

None. Musicolet's lyrics editor, web search, and lyrics on its own lock
screen stay out ([REQUIREMENTS](../REQUIREMENTS.md) leaves the editor for
later); font size and alignment can follow if asked for.
