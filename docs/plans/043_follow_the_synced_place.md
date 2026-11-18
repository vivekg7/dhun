# 043 — A paused player follows the place synced from another device

**Status:** `RELEASED` — 0.21.1, in the GitHub release with every platform
(2026-11-18); Linux tried on a laptop against a test server, the phone and a
laptop together not yet
**Started:** 2026-11-18

## Problem

An audiobook is paused on the laptop at 2h37m, played on the phone to
3h22m, and the laptop opened again. The laptop shows 2h37m and offers
"Continue from phone" at 3h22m ([017](017_handoff.md)). Continue is right.
**Play is wrong:** it carries on from 2h37m, and the pause that follows
saves 2h41m as the newest place, queue position and resume point alike.
Both are "later `at` wins" on the server, so the phone takes 2h41m on its
next sync and 3h22m is gone everywhere, kept only in the listening history.

It happens on every app, for the same reason. The app loads its kept copy
paused, so it opens at once, then syncs. The sync writes the phone's newer
place into the working copy but never moves the player already loaded from
the old one. Waking a laptop that slept with the app open does the same.

The same goes for songs: paused at song 5 here, played on to song 7 on the
phone, Play here plays song 5.

It breaks the rule that the server holds the truth: a device that only
_shows_ an old place should not get to write it back as the newest.

## Options

- **A. Follow the synced place while paused** (owner, 2026-11-18). When a
  sync changes where the open queue is, or the playing long file's resume
  point, a paused player moves there. Play then continues from the newest
  place, and Continue is needed only to switch queues.
- **B. Make Play act as Continue while the bar shows.** Fixes only the case
  where the bar shows; a dismissed bar, or a newer place within the same
  queue the bar does not cover, still plays the old one. Rejected.
- **C. Ask on Play** ("Continue from phone at 3h22m?"). One more dialog in
  front of the most common button, for an answer that is nearly always yes.
  Rejected.
- **D. Sync before loading the queue at start.** The app would open with
  nothing playable until the server answers, and offline not at all; and
  it does nothing for a device woken from sleep. Rejected; a short wait on
  Play covers the start instead (below).

## Decision

**A**, by the owner's rules (2026-11-18):

- **Same queue: follow, with no offer.** While this device is not playing,
  a sync that changes the open queue's current song or position, or the
  resume point of the long file it holds, puts the player there, paused, as
  if the app had just been opened with that data: a long file at its resume
  point (rewound, or offered under "ask", as plan 009 says), anything else
  at the queue's song and place. Paused at song 5, played to song 7 on the
  phone: here it is now at song 7, at the phone's place.
- **A playing device never moves.** It is where the user is listening, and
  its next save is the newest.
- **The newer save wins, as on the server.** A seek or a skip here after the
  phone's last save is newer, and stays. Only a sync that _changes_ the
  stored place moves the player: comparing the stored place before and after
  the sync, not the player's own position, so this device's own saves coming
  back, a rewind on loading, or an edit still in the outbox (which the sync
  leaves alone) never move it.
- **The other device's last saved place, as it is.** It saves every 30 s
  while playing; being up to 30 s behind is fine (owner). Unlike Continue,
  which moves the place on by the time passed, no estimate.
- **"Continue from …" only switches queues.** It is offered, as before, only
  when the other device's queue is not the one open here: another existing
  queue, or one made there. When it is the open queue, the place has already
  been followed.
- **Play waits for a fresh sync.** On a paused player, unless a sync
  finished in the last minute, Play syncs first (or waits for the sync under
  way) for up to 2 s (owner), then plays from wherever the player is. A
  Pause meanwhile takes the press back.
  Offline, or past the 2 s, it plays the place it has. First built as a
  wait for the sync that opening, coming to the foreground or waking
  starts; but a headset, lock-screen or media-key Play reaches an app that
  was never brought forward, and played the old place just the same. The
  minute keeps Play instant while listening goes on here.

## Plan

1. Plan, requirements and plan 017 updated.
2. Each app, the same three parts: follow after a sync, the bar only for
   another queue, Play's wait for a fresh sync. Linux, Android, macOS,
   web.
3. Tests where the app has them for sync or the player's decisions: the
   comparison (changed place moves, own save does not, playing does not).
4. On the devices: the 2h37m / 3h22m case with a long file, and the song 5 /
   song 7 case, each way between the phone and a laptop.

Two things each app needed besides:

- **A seek while paused is saved**, queue position and resume point, as a
  pause is. Linux, the Mac and the web did; the phone did not, so a synced place
  would have taken back a seek made there.
- **A sync asked for while one runs waits for it** (Mac and web; Android
  and Linux already ran one at a time behind a lock, or count finished
  syncs). Otherwise Play's wait could end with a sync that started before
  the press.

What can go wrong: following re-opens the file, which closes the listen
open on it. A paused listen holds only what was heard before the pause, so
nothing is counted twice.

## Rejected

- Exposing the queue position's `currentAt` from the server so clients
  compare times. The server already keeps the newer save; what reaches the
  working copy after a sync is that newer save, so comparing before and
  after the sync is enough.
- Estimating where a still-playing device is by now, as Continue does: the
  owner does not need the exact second.

## Open questions

None.
