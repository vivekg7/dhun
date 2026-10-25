# 040 — At most 1,000 songs in a queue

**Status:** `MERGED` — server, Android, Mac, web and Linux
**Started:** 2026-10-25

## Problem

Tapping a song plays the whole list it is in as a new queue, starting at that
song. On the Songs tab, the top folder or "Shuffle all", that list is the
whole library: about 31,000 songs in October 2026. The new queue reaches the
server as one `queue.create` naming every song, and the server refused any op
naming more than 20,000 songs, a limit set when the library was smaller
([006](006_api_and_sync.md)).

A refused op can never succeed, so the client drops it without a word. The
queue then lived on that one device: it never reached the server, so no other
device or hand-off saw it, and every later edit to it (the current song, a
move, a sort) was refused too, as an edit to a queue the server did not have.
Nothing was lost, but nothing said so either.

## Options

- **Raise the per-op limit to 100,000**, the size we design for. It fixes the
  refusal, but a queue of the whole library is still a 200 KB JSON array in
  every pull, sort and save, on every device, for a list nobody reads past its
  first few hundred songs. And it moves the cliff rather than removing it.
- **Cap the queue itself.** A queue is what is being played now, not an index
  of the library. Musicolet's queues are not capped, but they never leave the
  phone.

## Decision

A queue holds **at most 1,000 songs** (owner, 2026-10-25: any cap is fine;
5,000 was a placeholder, 500 or 1,000 equally good). 1,000 is about 70 hours
of music: more than any album, artist or playlist one would play through, and
small enough that a queue costs nothing to sync. One rule everywhere, so the
server and every client trim alike:

- **Playing a longer list** queues the song played and the 999 after it,
  wrapping round to the start of the list. The queue starts at the song
  chosen, as before, and "Shuffle" (a shuffled list played from its first
  song) becomes 1,000 songs at random from the whole list, so shuffling the
  library still reaches all of it over time. A note says how many were left
  out.
- **Adding to a queue** adds new songs only while there is room; songs already
  in it still move, as they never grow it. A note says how many did not fit.
- **The server applies the same rule** to `queue.create`, `queue.replace`
  (from the queue's current song) and `queue.insert`, rather than refusing.
  So an app from before the cap still syncs: its queue comes back trimmed on
  the next pull. Migration `0012` trims the queues already on the server the
  same way.
- **The per-op limit rises to 100,000 songs**, the library size we design
  for. It now only bounds the work one op can ask for; playlists, which are
  not queues, keep no cap of their own.

## Rejected

- **Refusing a longer queue with an error.** Correct, but a tap on a song in
  a long list would do nothing useful; trimming plays what was asked.
- **Keeping the first 1,000 of the list.** A tap on song 5,000 of the Songs
  tab would then not even queue the song tapped.
- **A window around the song tapped** (some before it, some after). The songs
  before it are the ones least likely to be wanted next, and the queue would
  no longer start where the user chose.
- **A setting for the cap.** Like the 20-queue limit, a number nobody needs
  to tune.
