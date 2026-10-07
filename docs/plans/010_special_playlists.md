# 010 — Special playlists and automatic views

**Status:** `MERGED` — server side running on the NAS (`server-v0.1.3`); Android on the owner's phone ([011](011_android_app.md)), no GitHub release yet
**Started:** 2026-10-07

## Problem

Every user needs a few lists that are always there and that the app keeps
for them, as opposed to playlists they make:

- **Favorites.** One tap on ❤; the list Musicolet users live in.
- **Listen Later.** Episodes, books and songs to get to, which should leave
  the list on their own once heard.
- Lists Dhun can compute from what it already knows: what is in progress,
  new, recent, most played, or forgotten.

The owner also set a rule for the music share: **no user action should
write to it directly.** A tap on ❤ must not rewrite a file on the NAS.
Writing happens only in a scheduled job, once a day. Instant file updates
for other players are not needed.

## Options

**Where the two lists live:**

- **A. `.m3u8` files written on every change**, like normal playlists.
  Visible to other players at once, but every ❤ writes to the share, which
  the owner ruled out.
- **B. The database only.** Nothing touches the share, but the lists exist
  only inside Dhun, against the rule that playlists stay usable in any
  player.
- **C. The database, copied to files once a night.** The table is the
  truth; at midnight the server writes `Playlists/<user>/Favorites.m3u8`
  and `Listen Later.m3u8` from it. Taps never touch the share, and other
  players still get the lists, at most a day late. The owner's choice
  (2026-10-07).

**When Listen Later counts an item as finished:**

- **By time heard in one listen.** Strict, but a two-hour podcast heard in
  three sittings with auto-resume ([009](009_resume_long_files.md)) would
  never qualify.
- **By how far into the file a listen reached.** Covers several sittings.
  Skipping to the end also counts, which is acceptable for a list of things
  to get to. The owner's choice.

## Decision

**C, and finished by position.**

- **Favorites** is the existing `favorites` table, now listed newest first
  (each item carries `at`). `favorite.set` / `favorite.unset` stay as they
  are.
- **Listen Later** is a `listen_later` table of the same shape, with
  `listen_later.add` / `listen_later.remove`. The later change wins, as
  everywhere in sync.
- **Auto-removal is the server's job**, done when a `play` arrives. That is
  one implementation for every app, and it works for listens uploaded from
  an offline phone. A listen whose `toMs` reached the user's percentage of
  the song's length takes it off the list, as of the time the listen ended,
  so adding it again afterwards still wins. Two synced settings
  ([009](009_resume_long_files.md)):

  | Setting                       | Values         | Default |
  | ----------------------------- | -------------- | ------- |
  | `listenLater.autoRemove`      | `true`/`false` | `true`  |
  | `listenLater.finishedPercent` | 1–100          | `90`    |

  These are the only settings the server reads; all others are the apps'.

- **The nightly export** runs at midnight local time (`TZ`). For each user
  it writes both lists to `Playlists/<user>/`, newest first, in the same
  format as every other playlist (`#EXTINF`, paths relative to the file).
  A file is written only when its content changed, after the usual copy to
  `data/playlists/history/` ([003](003_deployment.md)). An empty list that
  never had a file gets none. The first line says the file is generated.
  **Edits made to it over SMB are replaced the next night** (the old version
  is kept in the history folder). The way to change the list is the app.
- **The copies are not playlists.** The scan skips the two files in each
  user's folder, so they never show up twice. A normal playlist cannot take
  either name: "Favorites" in a user's folder is refused, in any letter
  case, since a Mac sees the share without case.

**Automatic views.** These are computed by the apps from data they already
sync, with no files and no server work:

| View               | From                                                                                                                      |
| ------------------ | ------------------------------------------------------------------------------------------------------------------------- |
| Continue listening | resume points (009), newest first                                                                                         |
| Recently added     | the catalogue's `addedAt`                                                                                                 |
| Recently played    | `lastPlayedAt` from `/plays`                                                                                              |
| Most played        | `count` from `/plays` (the 50% rule, [008](008_listening_history.md))                                                     |
| Not played lately  | songs played at least 3 times and not in the last 90 days, most played first: the first, simple step towards recommending |

## Rejected

- **A and B above**, for the reasons given.
- **The apps removing finished items themselves:** three apps would each
  need the rule, and an app that missed it would leave the list stale
  everywhere.
- **Exporting the views as files:** they change with every listen and are
  cheap to compute; files would add churn to the share for nothing.
- **Shared (top-level) special lists:** the lists are personal; the admin's
  shared playlists stay ordinary playlists.

## Plan

1. Server: migration `0006`, the operations, auto-removal, the pull
   (`listenLater`, and `at` on favorites), and the nightly export. Done.
2. Android: the two lists pinned above the user's playlists, with the
   settings, and the five views below them, on the Playlists tab
   ([011](011_android_app.md)). Each view holds at most 100 songs. Built.
3. macOS and web: the same lists and views, read-only views first.

## Open questions

- The owner's reason for C applies to all playlists: normal playlist edits
  still write their `.m3u8` at once. Moving them to the same model (database
  first, nightly export) would also end conflicts with SMB edits, but makes
  the files a copy rather than the truth. That change to
  [005](005_storage_and_library_model.md) is for the owner to decide.
