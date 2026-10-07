# 012 — Downloads on Android

**Status:** `MERGED` — in 0.2.0 on the owner's phone since 2026-10-07; no GitHub release yet
**Started:** 2026-10-07

## Problem

The app streams every song from the NAS. On a flight, in a basement or
when Tailscale drops, the music stops; on mobile data, each listen costs
data. REQUIREMENTS puts **Downloads in v1** (with a storage limit, original
files, and a Cache after v1), and [007](007_client_architecture.md) already
decided the storage: **plain files** in the app's own folder, preferably on
the SD card, tracked in Room, and resolved before the stream. This plan
decides the rest: what can be downloaded, when it is fetched, what happens
when space runs out, and how the app shows it.

## Decisions

Product choices are the owner's (2026-10-07); the mechanics follow from them.

### What is downloaded: things, not just songs

You download **a song, an album, a folder, an artist, a genre, a playlist,
Favorites or Listen Later**. Each is remembered as a **pin** (kind and
reference, such as `playlist:12` or `folder:Bollywood/90s`), not as the list
of songs it had at the time.

The pins **stay in step** with the server: on every change to the
catalogue, the playlists or the marks, the app recomputes the songs the pins
cover. Songs that joined are fetched; files that no pin covers any more are
deleted. A song covered by two pins is stored once and stays until neither
covers it. A file the server replaced (the size changed, as when curation
upgrades a file) is fetched again.

Rejected: **a one-time snapshot.** A downloaded playlist that silently goes
stale is the kind of surprise you discover on the plane.

The automatic views (Recently added, Most played, …) **cannot be
downloaded**: they change after every listen, and a download that churns
that often wastes data and flash.

### When it is fetched

- **Wi-Fi only by default**, with a setting to allow mobile data. "Wi-Fi"
  means the default network is not cellular; through Tailscale, Android
  reports the transport of the network underneath. Streaming is unaffected.
- One song at a time, in pin order (the newest pin first, so what you just
  tapped starts at once). Written to `<id>.<ext>.part`, checked against the
  catalogue's size, then renamed, so a half-written file is never played.
- A **foreground service** (`dataSync`) shows the progress and keeps the
  process alive while downloads run. It is started from the app; when
  Android forbids starting it from the background (Android 12+), the
  downloads still run while the app or playback is alive, and the rest
  waits for the next time the app is opened. A network that comes back or a
  failure 30 s ago starts the queue again.
- Rejected: **WorkManager**, for the reason in [011](011_android_app.md)
  (a second copy of Room), and **user-initiated data transfer jobs**, which
  need Android 14 and a fallback for 12 and 13 that would be this same
  service.

### When space runs out

A storage limit, **10 GB by default**, set on the device (2, 5, 10, 20,
50 GB or none). When the next song would go over it, or over the free
space on the card, downloads **stop and say how much more is needed**.
Nothing already downloaded is deleted to make room.

Rejected: **evicting older downloads**, which REQUIREMENTS allowed: the
album you downloaded for a trip should not disappear because you downloaded
another one.

The limit and Wi-Fi-only are **device settings, not synced**: they are
about this phone's storage and network.

### Where the files are

`Android/data/io.github.vivekg7.dhun/files/downloads/` on the SD card when
one is present, otherwise on the phone, and no storage permission is
needed for either. Each file's path is stored with it, so a card inserted
later only receives new downloads. Signing out deletes them, as it deletes
everything else of the account; uninstalling deletes them too.

### Playing them

The player keeps the stream URL as each item's address. A
`ResolvingDataSource` swaps in the local file when the song is downloaded,
at the moment the song is opened, so a download that finishes while the
queue is loaded is used the next time that song plays.

### How it shows

- A **download mark** on downloaded songs, and on the playlists, folders,
  artists, genres and albums that are pinned.
- A **Download button** on every album, folder, artist, genre and playlist
  page, and on Favorites and Listen Later; and Download / Remove download
  in the song menu. Removing a pin asks first.
- A **Downloads card** in the Playlists tab, under Favorites and Listen
  Later: the space used, what is happening (downloading, waiting for Wi-Fi,
  full), the pins, and every downloaded song.
- **Offline**, songs that are not downloaded are **dimmed**, and the player
  skips them: when a song fails to load for lack of a network, it moves to
  the next downloaded one. "Offline" means the last sync could not reach
  the server, or there is no network at all.

### On the way: the stream stays out of the art cache

The server serves songs with `Last-Modified` and no `Cache-Control`, which
OkHttp treats as cacheable. A song streamed without a Range header was
written into the 64 MB HTTP cache meant for art, pushing the art out.
Stream and download requests now ask OkHttp not to store them.

## Open questions

None. The Cache (smart caching) is after v1; promoting a cached song to a
download will be a rename into this folder (007).
