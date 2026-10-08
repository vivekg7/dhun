# 008 — Listening history

**Status:** `MERGED` — server side running on the NAS (`server-v0.1.5`); Android on the owner's phone ([011](011_android_app.md)), no GitHub release yet
**Started:** 2026-10-07

## Problem

The owner wants a music recommender later, built from what the family
actually listens to: "Tum Hi Ho at 10 AM today", and also the song left after
a few seconds. A recommender cannot be trained on history that was never
recorded, so the log has to be complete from the first day Dhun is used.

The first version of `plays` stored one row per play: user, song, device, a
time and the milliseconds played. That is enough for play counts but loses
what a recommender learns most from: how a listen ended, which part was
heard, the local time of day, and whether the song was chosen or arrived by
shuffle.

## Options

**A. Count plays only, past a threshold** (the first version, and what most
players keep). Skips are invisible, and skips are the clearest "not this,
not now" signal there is. Rejected.

**B. Log every listen as an event, and derive play counts from it.** One row
per listen, however short. Play counts become a query with a threshold, so
changing the rule later recounts all history instead of only new plays.

**C. Log raw player events** (play, pause, seek, skip) and reconstruct
listens from them. The most detail, but every client must send a stream of
events, and every reader must rebuild listens from it. Pauses and seeks
inside a listen matter little to a recommender. Rejected for v1.

## Decision

**B.** A listen starts when a song starts playing and ends when it stops
being the current song, or playback stops. It is sent as a `play` operation
through the sync outbox ([006](006_api_and_sync.md)), so a listen made
offline is uploaded later with the phone's own times.

| Field       | Column       | Why                                                                                                                                                                     |
| ----------- | ------------ | ----------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| `at`        | `at`         | When it started, UTC.                                                                                                                                                   |
| `endedAt`   | `ended_at`   | When it ended. With `ms`, shows long pauses.                                                                                                                            |
| `ms`        | `ms_played`  | Time actually heard, without pauses, as song time: a minute at 1.5× is 90 s ([016](016_speed_and_pitch.md)).                                                            |
| `fromMs`    | `from_ms`    | Where in the song it started: a listen that seeks to the chorus differs from one that plays from the start.                                                             |
| `toMs`      | `to_ms`      | Where it ended.                                                                                                                                                         |
| `end`       | `end_reason` | `finished`, `skipped` (next), `previous`, `switched` (another song or queue chosen), `stopped`, `interrupted` (the app was killed; recorded on the next start).         |
| `utcOffset` | `utc_offset` | The device's UTC offset in minutes. "10 AM" means local time, and stays right while travelling.                                                                         |
| `source`    | `source`     | What the queue was started from: `album:<id>`, `artist:<name>`, `playlist:<id>`, `folder:<path>`, `favorites`, `search`. A song chosen differs from one queued with it. |
| `queue`     | `queue_id`   | The queue it played in.                                                                                                                                                 |
| `shuffle`   | `shuffle`    | Whether shuffle picked it.                                                                                                                                              |

- **The server rejects nothing it can store.** An unknown `end` or `source`
  is kept as given (clipped to one short line): a newer app may send a value
  this server does not know, and the listen still happened. Times in the
  future become now, and an end before the start becomes the start.
- **The app writes the open listen to its own database as it plays.** If the
  app is killed or the phone dies, the next start closes it as
  `interrupted` with the last saved position, so nothing is lost.
- **Play counts:** a listen counts when at least **50%** of the song was heard
  (the owner's rule, 2026-10-07). Shorter listens are logged but not counted.
  Last played is the last counted listen. `GET /api/v1/plays` applies the rule
  at query time, so changing `countedPercent` recounts all history.
- Songs are never deleted and keep their ID through moves and upgrades
  ([004](004_curation_workflow.md)), so every listen stays attached to its
  song for good.
- **Phone-local songs are not logged**: they are not synced
  (`AGENTS.md`), and a recommender can only suggest NAS songs.

## Plan

1. Server: migration `0004` adds the columns to `plays`, the `play`
   operation takes the fields, and play counts apply the 50% rule. Done.
2. Android: record listens as above, from the player's own events. Built
   ([011](011_android_app.md)): a listen starts when the song first plays,
   so one loaded but never started is not logged.
3. Later: the recommender, with its own plan. It reads `plays` directly or
   through a new endpoint; the log does not need to change for it.

## Open questions

- None about recording. The recommender's design is its own plan.
