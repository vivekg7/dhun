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
- **Reached on the home network, and over Tailscale away from home**, like
  the NAS's other apps. Nothing is exposed publicly.
- **Deployed with Docker Compose** on the NAS, like the owner's other apps:
  one project folder in the `docker` share holds the compose file and all of
  Dhun's own data. **The collection must be safe from Dhun:** `Music` is
  mounted read-only except `Playlists/`, and every playlist Dhun changes or deletes is kept as a copy —
  [plans/003](plans/003_deployment.md).
- **Server is the source of truth, per user.** Playlists (as `.m3u8` files),
  favorites, queues and play counts live on the server, separately for each
  user. Each client
  keeps a **working copy** so it works offline, records changes made offline,
  and pushes them on reconnect — [plans/002](plans/002_sync_and_handoff.md).

### Users and the library

- **Roles:** one admin (the owner) and family members.
  - The admin account is created from `docker-compose.yml` on first start.
    Family members are added from the admin panel; there is no sign-up.
- **Everyone sees the whole main collection** — every folder under `Music/`
  except the curation workflow's own `_`-prefixed folders (`_inbox`, `_meta`,
  `_trash`), which hold unreviewed and deleted files.
- **Playlists are `.m3u8` files in `Music/Playlists/`**, stored and edited
  there by the app; no playlist lives only in the database. The two
  special lists below are kept in the database and copied to files nightly.
  - `Playlists/*.m3u8` (top level) are **shared**: everyone sees them; the
    admin edits them, on the NAS: the Android app keeps them read-only
    (owner, 2026-10-07; [plan 013](plans/013_playlist_editing.md)).
  - `Playlists/<username>/*.m3u8` are **that user's own** playlists.
  - In the app, adding songs **skips those already in the playlist** and
    says how many; the format allows duplicates, but none are made by
    accident.
  - Entries keep the existing format: `#EXTINF` lines and paths relative to
    the playlist file, so the files stay usable in any other player.
  - There will only ever be 3–4 users, so this stays simple: no sharing
    permissions, no generic multi-tenant layout.
- Queues, favorites, listening history and hand-off state are per user and
  private, stored in Dhun's own data folder (outside `Music`).
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
- Outside the admin panel, the app writes only `.m3u8` playlists (keeping a
  copy of each before changing it) and its own data folder. Songs are streamed and downloaded as **original files**.
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

| Platform                                                | Scope                                                                     |
| ------------------------------------------------------- | ------------------------------------------------------------------------- |
| Android 12 and newer (owner's: Samsung A35, Android 16) | **Full feature set, no compromise.** Primary target.                      |
| macOS                                                   | Minimal features are fine. **Online-only** (no offline mode).             |
| Web                                                     | Minimal features are fine. **Online-only**; served by the backend itself. |
| Linux                                                   | Optional.                                                                 |

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
| When full       | Downloads stop and say how much is needed | Evicts its own entries                          |
| Overlap         | —                                         | Never holds a song that is already in Downloads |
| File stored     | Original file                             | Original file                                   |

- Downloading a song that is already cached **moves** it from Cache to
  Downloads instead of fetching it again.
- Smart cache strategies: cache what is played, pre-fetch the next songs of
  the current queue, and later predict from listening history. The first
  two are fixed rules rather than separate settings; the one setting is
  the cache's size, or off.
- **Downloads and the cache are both in v1** (the cache since plan 019).
- The cache is **3 GB by default**, set in Settings → Downloads. It keeps every song played, plus the
  next two songs of the queue on any network and the next ten on Wi-Fi. Full, it drops the songs played longest ago, never one in the
  queue ahead (owner, 2026-10-07; [plan 019](plans/019_networking_and_caching.md)).
- **A network drop never skips a song.** The player says it is waiting
  for the network, keeps trying, and carries on from the same place when
  the network is back (owner, 2026-10-07).
- **Every cover has a 128 px thumbnail on the phone from the first sync**,
  so lists never wait for art, offline included. **The 500 full covers
  used most recently** are kept too, for the album grid and Now playing,
  each fetched when first shown, and those of downloaded songs always. A
  kept cover never expires and shows offline (owner, 2026-10-08).
- A downloaded playlist, album, folder, artist, genre, Favorites or Listen
  Later **stays in step** with the server: songs added later are fetched,
  songs removed are deleted unless something else downloaded has them. The
  automatic views cannot be downloaded (owner, 2026-10-07;
  [plan 012](plans/012_downloads.md)).
- Downloads use **Wi-Fi only** by default, with a setting to allow mobile
  data. At the storage limit (10 GB by default) they **stop and say so**;
  nothing downloaded is deleted to make room.
- Downloaded songs carry a mark, and a **Downloads** card sits in the
  Playlists tab. **Offline**, songs not on the phone (downloaded or
  cached) are dimmed; the player waits for the network rather than skip
  them (see the cache above).

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
- Users, login, and the admin and member roles. The admin comes from
  `docker-compose.yml`; the admin adds the others.
- Stream original files with HTTP range requests, so seeking works.
- Store each user's queues (up to 20, each with its current song and
  position), playlists, favorites and play counts / last played.
- Sync endpoint for the clients' offline working copies.

**Android**

- **Looks and works like Musicolet:** its tabs in its order, flat lists
  with a search box on each, plus a slim mini player. Light, dark and black
  themes (or following the system, or by time of day) and eight colour
  palettes, Dull Orange first —
  [plans/011](plans/011_android_app.md).
- Connect to the server (its home address, or its Tailscale address away)
  and sign in.
- Browse **Folders** (the real NAS folder tree), Albums, Artists, Genres and
  Playlists. Search by title, album or artist.
- **Multiple queues.** Playing from any list starts a new queue instead of
  replacing the current one. You can:
  - switch to or resume a queue, rename it, remove it;
  - drag songs to reorder them, remove a song;
  - play a song next, add songs to a queue;
  - save a queue as a playlist.

  Each queue keeps its own current song and position.

- Playlists (create, rename, edit, delete).
- **Special lists** every user has: **Favorites**, and **Listen Later**,
  which drops an item once a listen reaches 90% of the way into it (both
  the rule and the percentage are settings). Kept in the database; a
  nightly job at midnight copies them to `Playlists/<user>/`, so no tap
  writes to the music share. Automatic views: Continue listening, Recently
  added, Recently played, Most played, Not played lately —
  [plans/010](plans/010_special_playlists.md).
- Now playing: play / pause / next / previous / seek, shuffle, repeat.
- **Sleep timer:** after a set time (presets or any number of minutes),
  at the end of this song, after N songs, or at the end of the queue. By
  the clock it fades out and pauses on time; by songs it pauses where a
  song ends. Shown on Now playing and in the notification (owner,
  2026-10-07; [plan 014](plans/014_sleep_timer.md)).
- **Lyrics:** synced and plain, as provided by the server, in place of the
  cover on Now playing (tap the cover, or the lyrics button). Synced lines
  follow the song and a tap on one plays from it; the screen stays on while
  they show and the song plays. Kept on the phone with downloads and once
  viewed, so they show offline (owner, 2026-10-07;
  [plan 015](plans/015_lyrics.md)).
- **Play speed and pitch**, independent of each other: speed 0.5×–2×,
  pitch ±6 semitones. An everyday setting on each device, and a song's own
  (an audiobook at 1.5×) that follows the user to every device. Set from
  Now playing (owner, 2026-10-07; [plan 016](plans/016_speed_and_pitch.md)).
- Gapless playback.
- Media notification, lock-screen and Bluetooth / headset controls. Pause on
  disconnect, and audio focus (pause during calls).
- **Every listen logged** to the server, skips included, with its local
  time and how it ended, for a future recommender. A listen adds to the
  play count only when at least half the song was heard —
  [plans/008](plans/008_listening_history.md).
- **Long files resume** (audiobooks, podcasts): a file of at least 15
  minutes continues where it was left, on any device, however much else was
  played in between. The length, and whether to resume automatically, ask or
  not at all, are settings that follow the user to every device —
  [plans/009](plans/009_resume_long_files.md).
- **Downloads** with a storage limit.
- **Phone-local songs**, kept separate from NAS songs as described above.
- An offline working copy of the synced data, pushed back on reconnect.
- **Hand-off (resume):** offer to continue what another device was playing,
  in a bar above the mini player, while this device is not playing and when
  that playback is newer than anything played here. Continue switches to
  the same queue, song and place (owner, 2026-10-07;
  [plan 017](plans/017_handoff.md)).

**macOS and Web (minimal)**

- Sign in, browse, search, play, switch between queues, and play playlists.
- Hand-off (resume), so a session started on the phone continues here, and
  long files resume where they were left on any device.
- Web only: the admin panel's first screen, **Users** — add a family member
  and reset a forgotten password.

**Later (not v1)** — The rest of the admin panel (moves, upgrades, `_inbox` review;
the current curation tool runs until then), uploads into that review, live hand-off ("Play
here"), predicting songs to cache from history, the Subsonic-compatible API, Android Auto,
Musicolet backup import, equalizer, ReplayGain, crossfade, A-B repeat,
bookmarks, widgets, most-played stats, lyrics editor, tag editor, audio cutter,
and the rest of the inventory.

## Open questions

None at the moment.
