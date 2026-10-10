# 011 — The Android app: look, structure and first build

**Status:** `MERGED` — the first build is on `main` and on the owner's phone against the NAS (installed by hand); no GitHub release yet
**Started:** 2026-10-07

## Problem

[007](007_client_architecture.md) chose the Android stack: Compose, Media3,
Room, OkHttp, and one module. It did not decide what the app looks like,
how it is laid out, or what the first build contains. The owner has used
Musicolet daily for ten years. The app has to feel familiar to someone with
that habit, without copying anything of Musicolet's but its ideas.

The owner set four constraints (2026-10-07):

- The design is **fully inspired by Musicolet**.
- **Light and dark themes** from the start, and about **eight colour
  palettes**, including a dull orange.
- The **latest version of everything**, and the app **as light as
  possible**.
- **Android 12 (API 31) and newer only.**

## Options and decisions

### Layout: Musicolet's structure

Musicolet has one row of icon-only tabs at the bottom: Queues, Now playing,
Folders, Albums, Artists, Genres, Playlists, then Search and the menu. You
swipe between tabs, every list has its own search box, rows are flat and
dense, and the accent colour marks only the current song and the active tab.
Dhun keeps that structure tab for tab. Each tab keeps its place: a page
opened in it (an album, an artist), and the search and scroll of whatever
that page covers, until you go back. Rotation or Android reclaiming the app
loses none of it. Albums, Artists, Genres and Playlists have their search
box at the top. Every folder, and every page a tab opens (an album, an
artist, a genre, a playlist, Favorites, Listen Later, the automatic lists
and Downloads), has it at the bottom, as the queue does. The bottom box
shows only when the list has more than 12 rows, as in Musicolet, because a
shorter list fits on about one screen and needs no search (owner,
2026-10-10). A song found there still plays the whole list from that song.
While a playlist is being searched, its songs cannot be dragged: moving one
among the matches would drop it in the wrong place in the full playlist.
Every icon-only control, the tabs included, shows its name
when long-pressed ("Add to Favorites"), the same words TalkBack reads, as
Android's own icon buttons do (the owner's request, 2026-10-07). Plain bottom navigation (four or five
labelled tabs) was rejected because it would hide Folders and Genres, which
the owner uses daily.

Where Dhun departs from Musicolet, by the owner's choice:

- **A slim mini player** above the tab row on every tab except Now playing:
  art, title, play/pause and next. Tapping it opens Now playing. Musicolet
  has none, and makes you switch tabs just to pause. Since
  [022](022_mini_player_styles.md) a setting also offers none, or a
  floating pill.
- **Favorites and Listen Later** are two cards pinned at the top of the
  Playlists tab. Below them come the five automatic views
  ([010](010_special_playlists.md)), then the playlists. This is where
  Musicolet keeps its built-in smart playlists. A separate Home tab with
  shelves was rejected as less Musicolet-like.
- **Queues were chips** across the top of the Queues tab until 0.6.0,
  most recently used first. Since [020](020_queue_screen.md) the tab is
  Musicolet's again: numbered queues in a fixed order, picked from a box
  that opens a dialog. A queue picked there stays shown when you leave the
  tab and come back, by the owner's choice (2026-10-07): you may have been
  reordering it. When another queue starts playing, the tab shows that one
  instead.

The designs are in the Paper file "Dhun — Android app design" (the Logo and
Screens pages).

### Themes and palettes

**Theme modes:** Follow system (the default), Light, Dark, Black (AMOLED)
and Day/Night by time. Day/Night uses light from 07:00 to 19:00. Material
You wallpaper colours were offered and not chosen.

**Palettes:** eight muted, earthy accents. The owner chose these over a set
of vivid Musicolet-like colours.

| Palette     | Light    | Dark / Black |
| ----------- | -------- | ------------ |
| Dull Orange | `C27B45` | `D9925C`     |
| Sage        | `7E9C7A` | `98B594`     |
| Slate Blue  | `6A7FA8` | `8C9FC6`     |
| Teal        | `4E9A96` | `6FB6B1`     |
| Dusty Rose  | `B9707F` | `D08D9B`     |
| Mauve       | `9479A8` | `B097C2`     |
| Olive       | `8F9152` | `ADAF6F`     |
| Sand        | `B59E73` | `CBB68C`     |

Dull Orange is the default. A palette sets only the accent. The neutral
greys are the same for every palette, so each new palette costs one line
and not a hand-tuned scheme of 30 colours. Text in the accent colour
(section labels) uses a darker shade on light backgrounds, for contrast.

The theme belongs to **the device and is not synced**. A phone can be dark
while the web app is light.

### The icon

Three directions were drawn: a melodic line, a play triangle cut into
three slices, and a diya. The owner chose **the sliced play triangle**: three
stacked queues that together form a play button. It is an adaptive icon with
a white glyph on the accent background, plus a monochrome layer for
Android 13's themed icons.

### Keeping it light

The rule from AGENTS.md is that each dependency must earn its place. For the
app:

| Need            | Choice                                                                                                                                             | Rejected, and why                                                                                                                                                                                               |
| --------------- | -------------------------------------------------------------------------------------------------------------------------------------------------- | --------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| Icons           | Our own ~40 vector paths (Material Symbols, Apache 2.0)                                                                                            | `material-icons-extended`: thousands of icons we don't use                                                                                                                                                      |
| Album art       | A small loader of our own: files by the server's art key ([019](019_networking_and_caching.md)), an in-memory `LruCache`, and a downsampled decode | Coil: a framework for a single endpoint that already serves resized JPEGs (`/art/{id}?size=`)                                                                                                                   |
| Navigation      | A tab pager plus a small back stack of our own                                                                                                     | Navigation Compose / Navigation 3: the app is nine tabs and a few detail pages                                                                                                                                  |
| Preferences     | `SharedPreferences` for device settings (theme, token)                                                                                             | DataStore: a library for half a dozen values                                                                                                                                                                    |
| Database        | Room 3 with the framework SQLite driver                                                                                                            | Room's bundled SQLite: about 1 MB of native code per ABI for nothing we need                                                                                                                                    |
| Background sync | A debounced coroutine with retry, plus a "network is back" callback, while the app or its playback service runs                                    | WorkManager, which 007 planned: it depends on Room 2, so the APK carried two copies of Room. What it adds, syncing with the app fully closed, is not needed: the outbox is on disk and goes with the next start |
| Fonts           | The system font (Roboto)                                                                                                                           | A bundled font                                                                                                                                                                                                  |
| DI              | `AppGraph`, as in 007                                                                                                                              | Hilt                                                                                                                                                                                                            |

Release builds use R8 with resource shrinking, keep only English
resources, and leave out the dependency metadata blob. The first release
APK is 4.3 MB.

Kotlin is formatted and linted by ktlint (its official style), run from its
own jar through two Gradle tasks rather than a third-party plugin; Android
lint runs too. Both are in `make lint`.

**Android 12+ only (`minSdk 31`)** removes the compatibility code: the
splash screen and notification permission APIs are platform APIs, and no
`AppCompat` is needed. This replaces 007's `minSdk 29`.

**Android 17 (`targetSdk 37`) asks before the home network.** An app
targeting it needs the runtime permission `ACCESS_LOCAL_NETWORK` to reach
any LAN address, and the NAS is one. Without it, connections just time out.
The sign-in screen asks for it (the system shows it as "Nearby devices"),
where the reason is plain.

The app ID is `io.github.vivekg7.dhun`, the usual form for an app published
on GitHub; debug builds add `.debug`, so both can be installed at once.

## First build

What the first build contains, in order:

1. The Gradle project, the theme (palettes and modes), the icon, and the tab
   shell with the mini player.
2. Sign-in (server address, user name, password). The device name is the
   phone's model.
3. The catalogue in Room (`/library`), and the browsing tabs: Folders,
   Albums, Artists, Genres, Playlists, and Search.
4. Playback: a `MediaSessionService` with ExoPlayer streaming over OkHttp
   with the device token. Multiple queues, with playing from a list
   creating a new queue (AGENTS.md). A queue already named after the list
   (an album played twice) is refilled rather than duplicated, so queues
   do not pile up as "Parachutes (2)", "(3)"; this is Musicolet's "replace"
   choice for a name clash. The other choices, merge and ask, can become a
   setting.
5. Sync: the outbox and `/sync`, so that queues, favorites, Listen Later,
   resume points, settings and listens all reach the server
   ([008](008_listening_history.md), [009](009_resume_long_files.md),
   [010](010_special_playlists.md)).

Steps 1–5 are built and work against the local server: sign-in, browsing,
search, playback with the notification, queues, Favorites and Listen Later,
the automatic views, settings, and listens logged with their end reason.
Since 2026-10-07 the signed release build (`make apk`) runs on the owner's
phone against the NAS and the real collection.

Downloads ([012](012_downloads.md)), playlist editing
([013](013_playlist_editing.md)), the sleep timer
([014](014_sleep_timer.md)), lyrics ([015](015_lyrics.md)), speed and
pitch ([016](016_speed_and_pitch.md)) and hand-off ([017](017_handoff.md))
followed, each in its own step.

**Testing without the NAS:** a local `dhun` server on the Mac
(`DHUN_MEDIA` pointing at a few generated, tagged silent tracks, and a
throwaway admin password). The emulator reaches the Mac at
`10.0.2.2:8585`. Development never touches the NAS or the real collection.

## Open questions

None.
