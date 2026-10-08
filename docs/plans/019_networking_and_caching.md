# 019 — Networking, retries and caching on Android

**Status:** `IN PROGRESS` — steps 1 and 2 on `main`; covers next
**Started:** 2026-10-07

## Problem

The app should feel like it plays songs stored on the phone. On a network
that comes and goes (a train, a weak signal, a hand-over from Wi-Fi to mobile
data) it does not. An audit of the 0.4.0 app found:

1. **Nothing streamed is kept.** Replaying a song, going back to the last
   one or starting a queue again fetches every song again, and every song
   start waits on the NAS. The player keeps about 50 s ahead of where it is
   playing (Media3's default). The Cache in
   [REQUIREMENTS](../REQUIREMENTS.md#offline-storage-downloads-and-cache) was
   left for after v1 ([012](012_downloads.md)).
2. **The player gives up fast and never comes back.** Media3 retries a
   failed load about four times over some six seconds, then fails. On that,
   the app marked the server unreachable and jumped to the next downloaded
   song, or stopped with an error if there was none. A gap longer than the
   buffer (a tunnel) lost the song, and nothing restarted when the network
   came back.
3. **"Offline" stuck after a blip.** One failed request set the app offline,
   which dims every song not downloaded. The next try came after a delay
   that doubled up to 15 minutes, and only a _change_ of network cut it
   short. A Wi-Fi to mobile hand-over dimmed everything for a moment too.
4. **Covers:**
   - fetched per song: an album of twelve songs was twelve downloads, twelve
     cache entries and twelve decoded bitmaps of one picture;
   - fresh for seven days by the server's `max-age`, after which showing one
     needs the server; offline, they disappeared, downloaded songs included;
   - in a 64 MB cache shared with lyrics, in a folder Android may clear;
   - a cover scrolled past kept loading, ahead of the ones on screen.
5. **A broken download restarted from zero**, although the server serves
   ranges. That costs most on large FLAC files over a weak network.

## What the big services do

From what is publicly described of them:

- **Spotify** keeps the songs it has played in a cache with a size limit,
  fetches the whole song and the start of the next well before it is
  needed, retries quietly and plays from the cache while the network is
  away.
- **Apple Music** keeps the library's details and artwork on the device,
  keeps recently played songs until the space is needed, and fetches the
  next songs ahead.
- **YouTube Music**'s smart downloads fetch what it predicts, on Wi-Fi.

They are bounded by catalogues of a hundred million songs and by licences.
Dhun's library is finite and the owner's own, so it can do more:

- **every cover** of the library can live on the phone;
- the next ten songs of the queue can be fetched ahead on Wi-Fi;
- the cache holds the original files, so a cached song becomes a download
  by renaming it ([007](007_client_architecture.md));
- a song is **never skipped** for a blip.

## Options

**On a network drop mid-song**

- **Always wait:** keep retrying while the song is wanted, show that the
  app is waiting, and carry on from the same place when the network is
  back. Nothing is skipped.
- **Wait, then skip:** retry while the phone has a network; with none, go
  to the next song on the phone.
- **Skip at once:** what 0.4.0 did.

**What the cache fetches ahead**

- The next two songs on any network, the rest of the queue on Wi-Fi.
- Wi-Fi only.
- The whole queue on any network.

A queue can hold a thousand songs, so "the rest of the queue" was capped
at the next ten (owner, 2026-10-07): enough for well over half an hour
away from Wi-Fi, without filling the cache with songs that may never play.

**Covers**

- Every cover of the library, fetched in the background on Wi-Fi.
- Only covers seen, kept without expiry.

**How a song is fetched and played at once**

- **The fetcher writes the file, the player reads it as it grows.** One
  download per song, at the network's full speed: the whole song is on the
  phone seconds after it starts, so a later gap does not matter. A seek
  past what has arrived reads that part directly from the server.
- **Stream and fetch side by side.** Simple, but the song playing comes
  down twice.
- **Media3's `SimpleCache`.** Rejected in [007](007_client_architecture.md):
  its files are byte ranges in its own format, so a cached song cannot
  become a download without fetching it again.

## Decision

The owner chose (2026-10-07):

- **Always wait.** A song that cannot be fetched is retried, without limit,
  while it is wanted, and Now playing says it is waiting for the network.
  Only an answer that cannot change (the server no longer has the song)
  moves to the next song.
- **The next two songs on any network, the next ten on Wi-Fi.** Every
  song played is kept.
- **A 3 GB cache by default**, set in Settings → Downloads. Full, it drops
  the songs played longest ago, never one in the queue ahead.
- **Every cover on the phone,** fetched in the background on Wi-Fi.

And, for how a song is fetched: the fetcher writes the file and the player
reads it as it grows.

## Plan

Each step works without the next. Steps 1 and 2 went in together: both
turn on what a failed load means.

1. **The player waits.** A load-error policy that retries a network error
   without limit, backing off to five seconds between tries; a 4xx answer
   other than 408 and 429 still fails at once. A stalled stream is noticed
   after 10 s instead of OkHttp's 30. "Waiting for the network" shows on
   Now playing and the mini player while playback is held up.
2. **Offline means offline.** The app is offline when the phone has no
   network, or the last request failed and no later one succeeded. A
   failed sync is tried again after 5 s, doubling up to 2 minutes rather
   than 15, and any successful request (a song's bytes included) brings the
   app back at once: the API client's interceptor, which every request to
   the server passes, keeps the flag. A lost network counts only when no other one takes its
   place within a few seconds.
3. **Covers by content.** The server gives each song an `art` key, a hash
   of its cover's bytes, filled in by the scan; songs sharing a cover share
   the key. The app keeps covers in a folder of its own, by key, without
   expiry, and fetches every missing one at 512 px on Wi-Fi in the
   background. A list row decodes it at a quarter of the size. Now playing
   fetches the 1024 px copy when shown. A request for a cover no longer on
   screen is cancelled. A server without the key falls back to the per-song
   art as in 0.4.0.
4. **The song cache.** A folder of original files next to Downloads, with a
   Room table of size and last play. The fetcher keeps the current song and
   the next two, or the next ten on Wi-Fi, resuming a broken
   file with a range request. The player resolves a song to a download,
   then the cache (read while it grows), then the stream. A download of a
   cached song is a rename.
5. **Downloads resume** a broken file from where it stopped.

## Open questions

- Predicting songs from the listening history, the fourth strategy in
  REQUIREMENTS, waits until the history has been in use for a while.
- The server never transcodes ([006](006_api_and_sync.md)), so a FLAC on a
  weak mobile network is slow to arrive. A lower-quality stream for mobile
  data would need transcoding on the NAS; not planned.
