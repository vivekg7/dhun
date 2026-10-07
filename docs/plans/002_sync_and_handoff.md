# 002 — Sync, offline and hand-off

**Status:** `MERGED` — operation-log sync on the NAS ([006](006_api_and_sync.md));
resume hand-off in the Android app on the owner's phone
([017](017_handoff.md)); live transfer after v1
**Started:** 2026-10-06

## Problem

Three requirements pull against each other:

1. **The server is the source of truth, per user.** Queues, playlists,
   favorites and play counts belong to the user, not to a device.
2. **Clients must work offline.** On a trip with downloaded playlists, queues
   must still be editable and plays must still count.
3. **Playback should follow the user across devices.** Listening on the phone
   on the commute, then continuing on the MacBook at the office.

Decided with the owner: each client keeps a **working copy** of the user's data,
records changes made offline, and pushes them on reconnect. Still open: how
changes are recorded and merged, and how far hand-off goes.

## Options — recording and merging changes

**A. Last write wins per record.** Each queue or playlist is replaced as a
whole, and the newer timestamp wins. Simple, but an offline edit made on the
phone silently erases an edit made on the Mac. Rejected for queues and
playlists.

**B. Operation log (outbox).** The client records intent, such as "insert song
X after Y in queue Q", "rename Q" or "played X at T". On reconnect it sends the
ops in order. The server applies them to its current state with a rule per op
type and returns the new state and version. Play counts are append-only events
and never conflict. Edits to the same queue from two devices merge at the
granularity of a single song instead of overwriting each other.

**C. CRDTs.** Merges automatically with no rules to write. Rejected: a heavy
dependency and a hard-to-inspect data format, to solve concurrent editing that
a family of users with one active device each rarely produces.

Leaning **B**.

## Options — hand-off between devices

**H1. Resume where you left off.** The server keeps each user's _now playing_
state: the active queue, the song, the position, the time, and which device.
Opening another device offers "Continue from Phone at 2:13". This comes almost
free with the working copy. The playing device reports its position every few
seconds and on pause or song change.

**H2. Live transfer.** Devices that are open keep a live connection to the
server (WebSocket or SSE) and can see what the user's other devices are
playing. "Play here" pauses the other device and continues on this one at the
same position. Needs the live connection, and on Android it has to run inside
the playback service.

**H3. Remote control.** Control playback on another device, for example
skipping tracks on the phone from the Mac, in the style of Spotify Connect.
This is H2 plus commands relayed through the server.

Leaning: **H1 in v1**, because it falls out of the sync model, and **H2 after
v1**, built on the same live connection. H3 only if it turns out to be wanted.

## Decision

- **Hand-off (decided by the owner, 2026-10-06):** H1 (resume) in v1, and H2
  (live transfer) after v1. H3 (remote control) is not planned. H1's
  now-playing record is designed so that H2 can later push it over a live
  connection instead of the device polling for it.
- **Sync mechanism:** B (operation log), as detailed in
  [006](006_api_and_sync.md).

## Open questions

- ~~How often does the playing device report its position?~~ Every 30 s
  while playing, and on play, pause and each new song
  ([017](017_handoff.md)).
- ~~Should a hand-off move the whole queue state, or only the active
  queue?~~ Only the active queue: the others are synced anyway
  ([017](017_handoff.md)).
- Phone-local songs are not synced. When a queue mixing local and NAS songs is
  handed off, what does the other device show for the local ones? Open until
  phone-local songs are built.
