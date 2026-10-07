# 016 — Play speed and pitch on Android

**Status:** `MERGED` — in 0.3.0 on the owner's phone; the settings cap on
the NAS in `server-v0.1.3`; tested live there on 2026-10-07
**Started:** 2026-10-07

## Problem

REQUIREMENTS lists play speed and pitch for v1. Musicolet sets them
independently, for everything or per file. The family listens to
audiobooks and podcasts (plan [009](009_resume_long_files.md)) as well as
music, and the two want different speeds.

## Decisions

The scope, the controls, what syncs and where it lives are the owner's
(2026-10-07).

### An everyday setting, and a song's own

- **Everyday:** one speed and pitch for every song without its own.
- **A song's own** overrides it: an audiobook at 1.5× while music stays at
  1×. Set from the dialog with **Only for this song**; unticking it lets
  the song follow the everyday setting again.

Rejected: everyday only (an audiobook would drag music along), and per song
only (a slow-listening habit would mean setting every song).

### Independent speed and pitch

ExoPlayer's `PlaybackParameters` time-stretch: a faster speed keeps the
voice's pitch, and the pitch moves on its own.

- **Speed** 0.5× to 2×, in 0.05 steps, with 0.75×, 1×, 1.25×, 1.5× and 2×
  one tap away.
- **Pitch** in semitones, −6 to +6: the steps a singer thinks in, for
  singing along in another key. Rejected: a pitch multiplier, which says
  nothing to a listener.

Rejected: speed only, and speed and pitch linked like a record.

### What syncs

- **A song's own setting follows the user** to every device, as a synced
  setting `speed.<song id>` = `{"speed": 1.5, "semitones": 0}`. One setting
  per song, so two devices changing different songs offline never undo
  each other. Cleared, it is set to `null`.
- **The everyday setting stays on the device** (`Prefs`): the phone in the
  car and the laptop at a desk need not agree.

The server takes any setting the app names, so it needed only one change:
its cap of 100 settings per user, meant to stop one user filling the
database, would have been reached by songs given a speed over the years.
The cap is now **1,000**, and **settings set back to `null` no longer
count** (`server-v0.1.3`). A server older than that keeps the old cap; a
setting past it is rejected and the song keeps its speed on that phone
only.

### Where it lives

A **speed button on Now playing**, next to repeat. At normal speed and
pitch it is an icon; otherwise it shows the speed in the accent colour
("1.5×"), or the pitch ("+2") when only the pitch is changed. One value,
not both, so the row of buttons still fits a phone's width with the sleep
timer counting down beside it; the dialog shows both. **Normal** in the
dialog resets whichever setting is being edited.

### Listens count song time

A listen's `ms` is the time of the **song** heard, not of the clock: two
minutes at 1.5× heard three minutes of the song. The play count's "half
the song heard" compares `ms` with the song's length
([008](008_listening_history.md)); counted by the clock, a song played
through at 2× would just miss it.

## Open questions

None. Speed and pitch set from a song's menu without playing it, and a
pitch finer than a semitone, can follow if asked for.
