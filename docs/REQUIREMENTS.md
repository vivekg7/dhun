# Requirements

What the system must do. Decided items come from the owner. The reasoning
behind each architectural decision is in a numbered plan under
[`plans/`](plans/README.md). The [v1 scope](#v1-scope) is still a proposal,
and anything undecided is under [Open questions](#open-questions).

## Context

- About 6,800 songs (~53 GB, mostly MP3, M4A and Opus) that no longer fit
  comfortably on the phone. Today they sit on the phone's SD card, organised
  in folders (`Music/Hindi`, `Music/English`, `Music/Bhojpuri`, …). The full
  collection has been copied to the NAS.
- Musicolet has been the daily player for 10+ years. No existing streaming
  client matches it, which is why this project exists. See
  [research/musicolet-feature-inventory.md](research/musicolet-feature-inventory.md)
  and [research/existing-options.md](research/existing-options.md).
- **NAS:** Synology DS1525+ — AMD Ryzen V1500B (x86-64, 4 cores / 8 threads,
  2.2 GHz, no integrated GPU), 8 GB DDR4 ECC (max 32 GB), 2 × 2.5 GbE. It
  already runs **Jellyfin** (movies and TV) and **Immich**.
- **Music on the NAS:** the SMB share `home` on the NAS (`Gargantua`),
  folder `Media/Music` (`/Volumes/home/Media/Music` when mounted on the Mac).
  Surveyed 2026-10-06:
  - `Library/<Artist>/<Album (Year)>/D-TT - Title.ext`, with a `Singles/`
    folder per artist. Each album has a `cover.jpg` and a `.lrc` lyrics file
    next to each song.
  - `Playlists/` holds 19 `.m3u8` playlists, using paths relative to the
    playlist file. Of their 851 entries, 729 point into `Library/`; the rest
    point into `Apple Music/` (108), `Spotify/` (11) and `Collection/` (3).
  - Other top-level folders: `Album`, `Apple Music`, `Collection`, `Genre`,
    `Language`, `New`, `Source`, `Spotify`.
  - `_inbox/`, `_meta/` and `_trash/` belong to an existing curation workflow.
    It imports from downloaders into `_inbox`, deduplicates and upgrades
    files, fetches lyrics, and logs each action to `_meta/*.jsonl`.
- Remote access is over **Tailscale**, which already works reliably for Immich.
- **Users:** the owner and close family (spouse, children). All fully trusted.
- Named **Dhun** (धुन, "tune"). Open source under **GPL-3.0**, hosted at
  GitHub `vivekg7/dhun`. Distributed through **GitHub Releases** only. The Play Store
  is considered only if other people ask for it.

## Decided

### System

- **Our own backend, written in Go**, running on the NAS in Docker
  (`linux/amd64`), plus our own clients —
  [plans/001](plans/001_own_backend_in_go.md).
- **Jellyfin stays for video.** Our backend reads the same music folder
  directly; Jellyfin is not involved in music.
- **A Subsonic-compatible API comes after v1**, so existing apps (Symfonium,
  Feishin, …) can also connect. Our own API is the primary one.
- **Reached over Tailscale.** Nothing needs to be exposed publicly.
- **Deployed with Docker Compose** on the NAS: one container, one read-write
  mount of the `Music` folder (`MEDIA_PATH`), used for everything —
  [plans/003](plans/003_deployment.md).
- **Server is the source of truth, per user.** Playlists (as `.m3u8` files),
  favorites, queues and play counts live on the server, separately for each
  user. Each client
  keeps a **working copy** so it works offline, records changes made offline,
  and pushes them on reconnect — [plans/002](plans/002_sync_and_handoff.md).

### Users and the library

- **Roles:** one admin (the owner) and family members.
- **Everyone sees the whole main collection** — every folder under `Music/`
  except the curation workflow's own `_`-prefixed folders (`_inbox`, `_meta`,
  `_trash`), which hold unreviewed and deleted files.
- **Playlists are `.m3u8` files in `Music/Playlists/`**, stored and edited
  there by the app; no playlist lives only in the database.
  - `Playlists/*.m3u8` (top level) are **shared**: everyone sees them; the
    admin edits them.
  - `Playlists/<username>/*.m3u8` are **that user's own** playlists.
  - Entries keep the existing format: `#EXTINF` lines and paths relative to
    the playlist file, so the files stay usable in any other player.
  - There will only ever be 3–4 users, so this stays simple: no sharing
    permissions, no generic multi-tenant layout.
- Queues, favorites, play counts and hand-off state are per user and private,
  stored in Dhun's own data folder (`Music/_dhun/`).
- **Uploads:** each user can upload their own local collection to the NAS.
  Uploads go to a **per-user staging area**, separate from the main
  collection, until the admin reviews them (quality check, deduplication) and
  decides whether to move each one into the main collection. While waiting
  for review, an upload can be played **only by the person who uploaded it**.
- **Uploads come after v1.**
- **All curation happens in Dhun's admin panel.** That covers moving files,
  upgrading a song to a better-quality copy, removing files, and reviewing
  what arrives in `_inbox/`. It replaces the current curation tool, so Dhun
  always knows what changed and keeps everyone's play counts, favorites,
  queues and playlists attached to the song —
  [plans/004](plans/004_curation_workflow.md).
- **Nothing is deleted outright.** Most changes are moves or upgrades. A
  replaced or removed file goes to `_trash/`, every action is logged in
  `_meta/`, and emptying the trash is a separate, explicit admin action.
- Outside the admin panel, the app writes only `.m3u8` playlists and its own
  `_dhun/` folder. Songs are streamed and downloaded as **original files**.
  No transcoding is needed, so the NAS having no GPU doesn't matter.

### Cross-device playback

- **Hand-off between devices.** Example: listening on the phone on the way
  to the office, then continuing on the MacBook at the office.
  - **v1 — resume:** opening another device offers "Continue from Phone at
    2:13".
  - **After v1 — live transfer:** "Play here" pauses the other device and
    continues on this one.
  - Remote control of another device (Spotify Connect style) is not planned.

  See [plans/002](plans/002_sync_and_handoff.md).

### Platforms

| Platform                          | Scope                                                |
| --------------------------------- | ---------------------------------------------------- |
| Android (Samsung A35, Android 16) | **Full feature set, no compromise.** Primary target. |
| macOS                             | Minimal features are fine.                           |
| Web                               | Minimal features are fine.                           |
| Linux                             | Optional.                                            |

- **Native code, not Flutter.** The goal is a lightweight build with no bundled
  rendering engine. With LLM agents writing most of the code, separate native
  code per platform is cheap, so the usual argument for a cross-platform
  engine no longer outweighs its size and runtime overhead.

### Queues

- **Queues are separate from playlists**, and this is the most important
  feature. A queue is a temporary playlist that is being played right now.
  At most **20**, which is Musicolet's limit, confirmed from its APK.

### Phone-local songs

- **Songs already on the phone and NAS songs are kept separate** for now, even
  when they are the same track. A queue or playlist may contain both, but
  **only the NAS songs in it are synced** to the server. To be revisited later.

### Offline storage: Downloads and Cache

Two independent local stores, each with its **own storage limit** set by the
user (example: Downloads 10 GB, Cache 3 GB).

|                 | Downloads                                 | Cache                                           |
| --------------- | ----------------------------------------- | ----------------------------------------------- |
| Who controls it | The user, explicitly                      | The app, automatically ("smart")                |
| Visible to user | Yes — user picks playlists, albums, songs | No — hidden                                     |
| Typical use     | A trip: take whole playlists offline      | Make normal listening fast and resilient        |
| When full       | New downloads may evict older downloads   | Evicts its own entries                          |
| Overlap         | —                                         | Never holds a song that is already in Downloads |
| File stored     | Original file                             | Original file                                   |

- Downloading a song that is already cached **moves** it from Cache to
  Downloads instead of fetching it again.
- Smart cache strategies are **user settings, all on by default**: cache what
  is played, pre-fetch the next songs of the current queue, pre-fetch the rest
  of the current queue, and predict from listening history.
- **Downloads are in v1. Smart cache comes later.**

### Delivery

- **v1 ships only the essential features.** The rest of the Musicolet
  inventory stays on the list for later releases.
- **We credit the projects we take inspiration from**, in
  [INSPIRATIONS.md](INSPIRATIONS.md), and **check them regularly** for new
  features, issues and bugs worth adopting (the `upstream-watch` skill).

## v1 scope

Proposed. Awaiting the owner's confirmation.

**Backend (NAS)**

- Scan the music share: tags, embedded art or `cover.jpg`, embedded lyrics
  and `.lrc` files. Rescan on demand and periodically.
- Users, login, and the admin and member roles.
- Stream original files with HTTP range requests, so seeking works.
- Store each user's queues (up to 20, each with its current song and
  position), playlists, favorites and play counts / last played.
- Sync endpoint for the clients' offline working copies.

**Android**

- Connect to the server over its Tailscale address and sign in.
- Browse **Folders** (the real NAS folder tree), Albums, Artists, Genres and
  Playlists. Search by title, album or artist.
- **Multiple queues.** Playing from any list starts a new queue instead of
  replacing the current one. You can:
  - switch to or resume a queue, rename it, remove it;
  - drag songs to reorder them, remove a song;
  - play a song next, add songs to a queue;
  - save a queue as a playlist.

  Each queue keeps its own current song and position.

- Playlists (create, rename, edit, delete) and Favorites.
- Now playing: play / pause / next / previous / seek, shuffle, repeat.
- **Sleep timer:** after a set time, after N songs, or after a specific song.
- **Lyrics:** synced and plain, as provided by the server.
- **Play speed and pitch.**
- Gapless playback.
- Media notification, lock-screen and Bluetooth / headset controls. Pause on
  disconnect, and audio focus (pause during calls).
- Play counts recorded to the server.
- **Downloads** with a storage limit.
- **Phone-local songs**, kept separate from NAS songs as described above.
- An offline working copy of the synced data, pushed back on reconnect.
- **Hand-off (resume):** offer to continue what another device was playing.

**macOS and Web (minimal)**

- Sign in, browse, search, play, switch between queues, and play playlists.
- Hand-off (resume), so a session started on the phone continues here.

**Later (not v1)** — The admin panel (moves, upgrades, `_inbox` review;
the current curation tool runs until then), uploads into that review, live hand-off ("Play
here"), Smart cache, the Subsonic-compatible API, Android Auto,
Musicolet backup import, equalizer, ReplayGain, crossfade, A-B repeat,
bookmarks, widgets, most-played stats, lyrics editor, tag editor, audio cutter,
and the rest of the inventory.

## Open questions

None at the moment.
