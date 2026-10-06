# Musicolet feature inventory

Reference list of everything Musicolet does, so we can decide feature-by-feature
what our app needs. Musicolet is the benchmark the Android client is measured
against.

**Source:** Musicolet **6.14.1 (build 540)**, pulled from the Samsung A35 on
2026-10-06 and decompiled with jadx. Features were read from the settings
screen definition, option dialogs, layouts and the string table — not guessed
from screenshots. Raw dumps live in the git-ignored `tmp/` folder
(`tmp/musicolet-settings-tree.txt`, `tmp/musicolet-strings.txt`); they are
derived from proprietary code and must not be committed.

Musicolet facts worth knowing up front:

- 100 % offline — it does not request the Internet permission at all.
- Ships its own native decoder (`lib_musicolet.so`) alongside the system one.
- Uses Android's `MediaBrowserService`, so it works in Android Auto.
- Free core with a paid "Pro features" tier (one-time or subscription).

---

## 1. Queues (the signature feature)

Queues are **temporary, named, play-order lists**, entirely separate from
playlists.

| Behaviour                   | Detail                                                                                                                                                                                                                                                            |
| --------------------------- | ----------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| Multiple queues             | Up to **20**. Creating a 21st deletes the oldest ("Max. queues limit reached. First queue is deleted.").                                                                                                                                                          |
| Per-queue position          | Each queue remembers its current song (and position) independently.                                                                                                                                                                                               |
| Auto-created                | Playing from an album/playlist/folder/search creates a **new** queue instead of wrecking the current one.                                                                                                                                                         |
| Name conflict rule          | Setting: when a queue with the source's name already exists → replace it / merge into it / ask.                                                                                                                                                                   |
| Unique names, no duplicates | Queue names are unique; a song can appear at most once per queue.                                                                                                                                                                                                 |
| Snapshot, not live          | A queue named after an album does not update when the library changes.                                                                                                                                                                                            |
| Queue actions               | Resume, rename, remove, remove all others, sort (incl. randomize), search inside, select multiple, share songs, export as `.m3u`, save as a new playlist, save changes back to its source playlist.                                                               |
| Song actions in a queue     | Remove, play next ("Play after current song"), add to another queue, add to playlists, **stop after this song**, preview, drag to reorder, swipe right to remove.                                                                                                 |
| Queue end behaviour         | When a song ends: stop / play next / load next and pause / repeat song. When a queue ends: set first song current, re-shuffle, then stop / jump to next queue / repeat queue; optionally resume next queue from its last position; wrap from last queue to first. |
| Shuffle semantics           | Shuffle mode either shuffles the whole queue or only the songs **below** the current one (configurable; same for "Randomize" sort).                                                                                                                               |
| Restore                     | Queues are included in backups.                                                                                                                                                                                                                                   |

## 2. Library browsing

Tabs (re-orderable, each can be hidden; tab bar top or bottom):
**Queues, Now playing, Folders, Albums, Artists, Album-Artists, Composers,
Genres, Playlists.**

- **Folders:** linear (all folders flat) or hierarchical. A "main music folder"
  becomes the home of the hierarchical view. Rename folder, exclude folder,
  home-screen shortcut to a folder.
- **Albums:** list, list with thumbnail, or grid (adjustable size).
  Configurable "album merge strategy" decides when two albums are the same.
- **Sub-categories:** an album can be split by artist/album-artist/composer; an
  artist by album; a genre by album/artist/composer. Long-press shows just one
  sub-category.
- **Multi-value tags:** artist, album-artist, composer and genre can each be
  split on custom separators (`,  &  /  ;  feat.`).
- **Built-in smart playlists:** All songs, Favorites, Recently added, Recently
  played, Most played.
- **Sorting:** title, album, artist, album-artist, composer, genre, year, track
  number, duration, file name, file path, folder, date added, date modified,
  date last played, number of songs, play count — each ascending/descending.
  Options: ignore leading words ("The", "A"), alphanumeric (natural) sort.
- **Quick search box** on every tab; global search by title/album/artist with
  history.
- **"Extra info"** columns configurable per tab.
- **Multi-select:** select all/none, invert, select range between two, select
  all songs of an album/artist/folder (incl. sub-folders).

## 3. Playlists and favorites

- User playlists: create, rename, edit (drag reorder, sort, remove unavailable
  songs), delete, merge when importing a same-name playlist.
- Import external `.m3u` / `.m3u8` / `.pls`; export as `.m3u`; read playlists
  shared through Android MediaStore.
- Favorites (toggle from anywhere, including notification and widget).
- Play counts and play history; "minimum duration to count as played"
  (10/30/50/90 %). Clear history per song.

## 4. Now playing and playback controls

- Shuffle, repeat (simple and advanced — see queue end behaviour above).
- Previous button: always previous, or restart if > 10 s played.
- Fast-forward / rewind buttons (configurable durations); long-press next/prev
  to seek. Option to make external next/prev act as FF/RW.
- Seek-to dialog (hh:mm:ss), seekbar helper buttons, "touch only the knob" to
  avoid accidental seeks. Remaining-time display.
- **A-B repeat.**
- **Bookmarks / notes** per song (stored in a side file).
- **Remember last position** of long tracks (minimum length configurable; ask
  or auto-resume) — for audiobooks/podcasts.
- Swipe up/down for next/previous; blurred album-art background; tap album art
  → lyrics / view art / play-pause.
- Keep screen on while on Now Playing or showing lyrics.
- Customisable "Quick access" shortcut row on Now Playing.
- Screenshot + share card of the current song.

## 5. Sleep timer

- Stop after **hh:mm**, after **N songs**, or **after a specific song**.
- Option to let the last song finish, and to gradually fade out its volume.

## 6. Lyrics

- Embedded lyrics and `.lrc` files; preferred source configurable.
- **Synced (karaoke-style) and plain lyrics**, shown on Now Playing and on the
  Musicolet lock screen.
- **Built-in lyrics editor:** plain or synced, line-by-line timing.
- Lyrics appearance (font size, alignment).
- Search lyrics/album art on the web with a configurable engine (Google,
  DuckDuckGo, Bing, Kagi, …) and query template.

## 7. Audio

- **Play speed and pitch** — independently; global or **per file**.
- Gapless playback; **crossfade**; fade-in on play, fade-out on pause.
- **Equalizer:** own EQ (bands, presets, save preset), bass boost, surround,
  preamp, reverb (experimental), or the system EQ.
- Channel balance (L/R), mono/stereo.
- **ReplayGain:** track gain / album gain, read tags or **calculate RG** by
  analysing the file; separate preamp for files with/without RG; pre-compute
  for the next song.
- Decoder choice: Musicolet decoder (wide format support incl. FLAC, APE, WV,
  DSD, Opus) or system decoder; multi-threaded decoding.
- Audio focus levels: pause on calls, pause/duck for other apps, resume after
  call.
- Volume: block playback at volume 0, pause on mute, resume on unmute.

## 8. Headset, Bluetooth, car

- Pause on disconnect; resume on Bluetooth / wired connect (optionally only if
  paused by disconnect).
- **Prevent unwanted autoplay** — ignore play commands for a few seconds after
  a device connects/disconnects (buggy car head units).
- Earphone button: double-press / triple-press actions, 4+ presses fast-forward.
- Android Auto (via MediaBrowserService).
- Chromecast (via a separate cast plug-in) and screencast mode with a
  screen-saver.

## 9. Notification, lock screen, widgets

- Three notification styles; custom button arrangement for compact and
  expanded; background colour from album art; show upcoming songs in the
  expanded notification.
- **Own lock-screen** overlay with controls, queue and lyrics.
- Home-screen widgets 4×1, 4×1 advanced, 4×3 (shows the current queue); themes
  including colour from album art / wallpaper; transparency.
- App shortcuts: Search, Shuffle all, Resume playback.
- Floating mini-player dialog.

## 10. Tag editing and file management

- **Tag editor** (single and batch): title, album, artist, album-artist,
  composer, genre, lyricist, track, disc, year, comment, album art
  (pick/remove/search), embedded lyrics.
- Song info: format, bitrate, sample rate, bit depth, channels, size, RG, dates,
  play count.
- Move/copy to folder, delete permanently, set as ringtone, share files.
- **Audio cutter:** trim a range and export as M4A (128/320 kbps) or FLAC.
- "Hide website junk from tags" (e.g. strip `| DownloadMp3Site.com`).
- Show file names instead of titles.

## 11. Library scanning

- Folders to scan, excluded folders, exclude short files, scan hidden and
  `.nomedia` folders, scan video files as audio.
- Choice of Musicolet scanner or Android's MediaStore scanner.
- Use `folder.jpg` / `cover.jpg` when no embedded art.
- "Excluded or missing" cleaner: keeps play data of missing files so it comes
  back when the files reappear; option to purge it.

## 12. Stats

- **Most played** songs/artists by week, month, year, all time, with a shareable
  "story" card; reminder at month/year end.
- Automatic CSV export of most-played data weekly/monthly/yearly.

## 13. Backup and settings

- Manual and automatic backup (zip) of playlists, favorites, play counts,
  queues and settings — **not** audio. Selective restore.
- Themes: light, dark, AMOLED black, follow system, day/night by time; accent
  colour. Language picker. Searchable settings.
- Every option menu (song, album, folder, multi-select, …) can have items
  shown/hidden.
