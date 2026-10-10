# 030 — The web client, at the other apps' level

**Status:** `MERGED` — checked in Chrome; not yet in Safari, Firefox, on a phone or on the NAS
**Started:** 2026-10-10

## Problem

REQUIREMENTS asked only for a minimal web client: sign in, browse, search,
play, switch queues, play playlists and hand-off. [007](007_client_architecture.md)
placed it in `server/web/`, as plain HTML, CSS and JavaScript embedded in
the Go binary, but nothing was built. The owner now wants it **as refined
as the Android and Mac apps, or better** (2026-10-10). It is for a family
member at a computer without the Mac app, for a phone browser, and for any
machine on Tailscale.

The owner answered three questions about behaviour (2026-10-10):

| Question     | Answer                                                                                                                                                                                                                |
| ------------ | --------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| Offline      | **Online-only**, as REQUIREMENTS says. No downloads and no play cache in the browser. Browsers allow service workers only over HTTPS, and the NAS is reached over plain HTTP, so the server can run as it does today. |
| Screen sizes | **Desktop and phone.** The Mac's sidebar window on a wide screen, and the phone's tabs with a mini player on a narrow one.                                                                                            |
| Pitch        | **Speed only, pitch kept.** Browsers change speed with the pitch kept; shifting the pitch alone would need a pitch shifter of our own in an AudioWorklet, with every song played through Web Audio.                   |

## What carries over

The Mac app is the closest model. It is the phone's logic ported to Swift,
and **its behaviour is the specification**: where this plan is silent, the
web does what the Mac does, for the reason the Mac's plan gives.

| Mac                                                                       | Web                                                                                                |
| ------------------------------------------------------------------------- | -------------------------------------------------------------------------------------------------- |
| `Catalog`: albums, artists, genres, the folder tree, natural sort         | Ported (`catalog.js`)                                                                              |
| Search (029): folding, sound keys, typos, field filters, lyrics on server | Ported (`search.js`), checked against `api/search-fold.tsv` like the other two                     |
| `Store`: a local write plus an outbox op                                  | The same, in memory; the outbox is also kept in `localStorage`                                     |
| `Sync`: library pull, push and pull, back-off                             | The same; no playlist downloads to move                                                            |
| `Playback`: 20 queues, listens, resume points, hand-off, sleep timer      | Ported; two `<audio>` elements underneath (below)                                                  |
| SQLite working copy                                                       | IndexedDB, as a **cache only**: the page opens at once on a reload, then syncs                     |
| Thumbnails by art key, covers by size                                     | The same, with the thumbnails kept in IndexedDB                                                    |
| Lyrics and the LRC parser                                                 | Ported                                                                                             |
| Speed and pitch                                                           | Speed only (`playbackRate`, `preservesPitch`); a song's own pitch is kept in its setting, not used |
| Media keys, Control Center                                                | The Media Session API: media keys, the OS's now-playing panel, a phone's lock screen               |
| Sidebar window, Now playing bar, queue and lyrics panel                   | The same on a wide screen                                                                          |
| Floating mini window                                                      | A Picture-in-Picture mini player where the browser has one (Chrome's Document PiP)                 |
| Menu bar control, Open With from Finder                                   | None: a browser has neither                                                                        |
| Downloads, song cache, storage settings                                   | None (online-only)                                                                                 |
| Pause when the output device changes                                      | None: a page is not told. The OS pauses through the media session where it does so itself          |
| Family members (admin)                                                    | The same, under Settings                                                                           |
| System, light and dark; eight palettes                                    | The same                                                                                           |

**Parity is a standing commitment**, as 024 made it for the Mac. A plan
for a phone feature also says what the web does, or why it leaves it out.

## Options

### Framework

- **Plain HTML, CSS and ES modules, no build step.** Chosen, as 007
  decided. The files the browser runs are the files in git. They are
  embedded in the server and need no toolchain beyond the Go one. Prettier,
  already the repo's formatter for Markdown and YAML, formats them. The
  views are small functions that build DOM nodes. A page is rebuilt when
  the state it shows changes, and the parts that change while playing
  (the seek bar, the playing row) are updated in place.
- **React, Svelte, Vue or Lit.** Rejected again, as in 007: a build
  toolchain and a dependency tree for a client of about 20 screens.
- **A tiny reactive library (Preact with htm, from a file).** Rejected.
  It needs no build step, but it is a dependency for the twenty lines of
  rendering we need.

### Playback

- **Two `<audio>` elements, one playing and one holding the next song.**
  Chosen. The stream endpoint already serves `Range` and the cookie, so
  the browser streams, seeks and buffers by itself. The next song is
  opened while the current one plays, and it starts on the current one's
  `ended` event. That is close to gapless (a few milliseconds' gap,
  depending on the browser) but not sample-exact.
- **Web Audio, decoding whole songs.** Rejected. It would give exact
  gapless playback, but a song must be fetched and decoded whole before
  it starts. A 10-hour audiobook would decode to gigabytes.
- **Media Source Extensions with append windows** (how video players do
  gapless). Rejected for now. Browsers accept only MP4, WebM and MP3 into
  MSE, and the library is also FLAC, Ogg and M4A with the index at the
  end. It would mean a demuxer of our own per format. Revisit if the gap
  bothers anyone.

**Formats.** Current Chrome, Firefox and Safari play MP3, AAC/M4A, FLAC,
Ogg Opus and WebM Opus natively, which covers what the owner's library
holds (024). A song the browser cannot play is skipped, as the other apps
skip a song that cannot be read.

### Keeping state across a reload

- **Memory only.** Rejected. A reload would fetch the whole library
  (about 2 MB of JSON, 400 KB gzipped) and every thumbnail again. An edit
  made just before a reload, still waiting to be sent, would be lost,
  against the rule that no edit is ever lost (AGENTS.md).
- **IndexedDB as a cache, `localStorage` for the outbox and this
  browser's settings.** Chosen. This is not offline mode. Nothing plays
  without the server. But the page draws at once from the cache and then
  pulls only what changed, as the apps do. IndexedDB and `localStorage`
  both work over plain HTTP.

### Signing in

The page is served by the server it talks to, so there is no server field.
Login returns the bearer token, which the page keeps in `localStorage` and
sends on every call. It also sets the HttpOnly cookie that `<audio>` and
`<img>` need ([006](006_api_and_sync.md)). The cookie still opens only
`/stream`, `/art` and `/lyrics`. The device name is the browser and the
system ("Chrome on macOS"), as shown in hand-off. A strict
Content-Security-Policy, allowing scripts only from the server itself,
keeps the token out of reach of injected script.

## Decision

A plain-JavaScript client in `server/web/`, embedded in the server and
served at `/`, built as described above. Technical defaults, which the
owner can override:

- **The layout follows the window's width.**
  - **900 px and wider: the Mac's window.** A sidebar has Queues; the
    library (Folders, Albums, Artists, Genres); the lists (Favorites,
    Listen Later and the five automatic views); and the playlists. Search
    is at the top, the page in the middle, and the queue or lyrics in a
    panel on the right. Now playing runs along the bottom, with the
    hand-off and notice bars above it.
  - **Narrower: the phone's structure.** Musicolet's icon tabs run along
    the bottom: Queues, Now playing, Folders, Albums, Artists, Genres,
    Playlists, Search and Settings. A slim mini player sits above them,
    and Now playing is a full page. Favorites, Listen Later and the
    automatic views are cards at the top of Playlists, as on the phone.
- **Every page has an address** (`#/album/…`, `#/queue/…`), so Back,
  Forward, a reload and a bookmark all work. Search is in the address
  too.
- **Mouse and keyboard as the Mac; touch as the phone.** A click selects a
  row, ⌘ or Ctrl adds to the selection, Shift selects a run, and a
  double-click or Return plays. A right-click, or the ⋯ button on each
  row, opens the phone's song menu. On a touch screen a tap plays. Queues
  and playlists reorder by dragging a row's handle. Songs dragged onto a
  playlist, Favorites or Listen Later in the sidebar are added to it.
- **Long lists are virtual:** only the rows on screen exist, so all 7,000
  songs scroll as smoothly as an album.
- **Keys:** Space plays and pauses. ⌘/Ctrl + ← and → go to the previous and
  next song, and `/` or ⌘/Ctrl + K searches. ⌘F stays the browser's own
  find.
- **When sync runs:** after every edit (a second's debounce), when the page
  becomes visible, when the network returns, and from **Sync now**. While
  a page is closing, the outbox is sent with `keepalive`.
- **Motion** uses the phone's timings ([027](027_motion_and_polish.md)):
  150 ms out, 280 ms in. Pages slide a fifth of the width. The next song
  comes in from the right and the previous from the left. A heart pops
  when turned on. Everything is still under `prefers-reduced-motion`.
- **Versioning.** The web client is part of the server's image and takes
  the server's version number: a release that changes the web is a server
  release. Settings shows that version.
- **Tooling.** Prettier formats the HTML, CSS and JavaScript (`make fmt`,
  the pre-commit hook). The logic ported from the Mac (search, catalogue,
  lyrics, queue order) has tests run by Node's own test runner
  (`make test-web`, in the server's CI job), with no packages.

## Plan

1. **Serving.** `server/web` embedded and served at `/`, with its own
   Content-Security-Policy, `no-cache` with ETags, and the API's headers
   unchanged. A test pins that the API routes still win over the page.
2. **The core.** The API client, the store and outbox, sync, the
   IndexedDB cache, the catalogue, search and lyrics, with the ported
   tests.
3. **Playing.** The two-element player under the ported `Playback`: queues,
   listens, resume points, hand-off, speed, the sleep timer and the Media
   Session.
4. **The window.** Sign-in, the sidebar, every page, search, the Now
   playing bar, the queue and lyrics panel, the song menu, multiple
   selection, drag to reorder and drag to add, and settings with family
   members.
5. **The phone layout.** Tabs, the mini player and the Now playing page.
6. **Checked** in Chrome, Safari and Firefox against the owner's NAS and on
   a phone. The Picture-in-Picture mini player comes last, where the
   browser has it.

## How the build went (2026-10-10)

Steps 1 to 5 were built in one go, as the Mac's were. The client is about
7,500 lines of JavaScript, as Prettier lays it out, and 1,900 of CSS,
with no dependency.

**Checked**, in headless Chrome driven over its DevTools protocol, against
a local server with a copy of the 194 test songs:

- Sign-in, every page in both layouts, light and dark.
- Playing an album: the next song starts by itself, **9 ms** after the last
  one ends. The edits reach the server and the outbox empties.
- Synced lyrics follow the song, in the panel and over the phone's cover.
- Search, typos and lyrics included, from the window's field and the
  phone's tab.
- A playlist made here comes back from the server with its id, matched by
  its `ref`. A favourite and the playing state reach the server.
- Hand-off from a second device: offered when this browser's own playback
  is older, and Continue picks up its queue, song and place.
- Dragging a queue's row: the rows slide aside, and the server's order
  matches.
- The sleep timer's "end of this song" pauses at the next song's start.
- Space and ⌘→, the speed (the element's rate follows) and sleep panels.
- A device removed from another device: the page asks to sign in again,
  and an edit made meanwhile reaches the server afterwards.
- On 7,000 made-up songs (Node), the catalogue builds in about 50 ms and a
  keystroke searches in 1 to 4 ms.

**A review of the code against the Mac's** found eight bugs, all fixed and
checked again:

- A deleted playing queue left a ghost row and broke playing from lists.
  The player now stops when sync removes its queue, and a new queue never
  pushes out the one playing.
- Any `<audio>` error skipped the song, so with the server down a reload
  walked the whole queue. Now the server is asked first. Only a song it
  serves but the browser cannot play is passed over. Otherwise the player
  holds its place and says why, and Play opens the song again.
- A failed preload of the next song left the player silent.
- A reload on an album's address showed "missing" before the cache loaded.
- A queue not playing that held songs gone from the library moved the
  wrong song on a drag, and lost those songs on a sort.
- The sync cursors were saved before the data they cover. They are now
  kept in the same IndexedDB record.
- A listener added on every layout change, and a wake lock never let go.
- The playing queue did not follow another device's edits, so a sort here
  undid them. **The Mac has the same gap**: its `queuesSynced` updates
  only the sleep timer. That is for a Mac plan to fix.

**Not checked yet:** Safari and Firefox (headless Firefox would not start
here), a real phone, the owner's NAS, and the Picture-in-Picture mini
player, which headless Chrome cannot open.

## Open questions

None.
