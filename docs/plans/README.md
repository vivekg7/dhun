# Plans — index

One file per piece of work larger than a bug fix. Each records the problem, the
options, the decision and **why**, and the alternatives that were rejected.

Read the index (newest first) before starting anything. The project spans a
backend and four clients, built over months, mostly by agents. This record is
what stops a rejected idea from being proposed again, and what explains why a
part is shaped the way it is.

## Filenames

```
docs/plans/NNN_short_slug.md      e.g. 001_own_backend_in_go.md
```

- **`NNN` is a zero-padded sequence number.** Take the next unused one
  (`ls docs/plans/`). A number is never reused or renumbered, not even for an
  abandoned plan. A gap in the sequence tells you something; a reused number
  breaks every citation of it.
- The slug may be renamed if every reference to it changes in the same commit.
  The number never changes.
- Code comments and docs cite a plan by its full path
  (`docs/plans/001_own_backend_in_go.md`).

## Status vocabulary

| Status        | Means                                                                   |
| ------------- | ----------------------------------------------------------------------- |
| `PROPOSED`    | Options and trade-offs written up. No decision yet, no code.            |
| `ACCEPTED`    | Decided, but nobody is building it yet.                                 |
| `IN PROGRESS` | Being built. Not yet on `main`.                                         |
| `MERGED`      | On `main`, but not yet in anyone's hands.                               |
| `RELEASED`    | Running for real: backend deployed on the NAS, app in a GitHub release. |
| `ABANDONED`   | Deliberately dropped. Kept for the reasoning; say why at the top.       |

**The vocabulary is closed.** A made-up status (`DONE`, `READY`) forces the
reader to open the plan to learn whether the thing is actually running. If none
of the six words fits, write the nuance after the em dash on the status line.

`MERGED` and `RELEASED` are different facts. Code on `main` does nothing until
the NAS container is updated or a release is cut and installed.

## Writing a plan

A plan is a decision record, not a task list. It should still be worth reading
a year from now, when the code has moved on and only the reasoning still
matters.

```markdown
# NNN — Title

**Status:** `PROPOSED` — one line on where it actually stands
**Started:** YYYY-MM-DD

## Problem

What is missing or broken, and what that costs us.

## Options

Each one with its trade-offs. Include the options we reject; writing them down
is what stops them coming back.

## Decision

What we are doing, and why this option rather than the others.

## Plan

The steps in order, with the parts that can fail called out.

## Open questions

Anything unresolved. Better found here than mid-build.
```

Update a plan's status line the moment it changes, and update this index in
the same commit. A stale index is worse than none, because it is confidently
wrong.

## Index

| Plan                                                                                                  | Status                                                                                                                                                                                                                             |
| ----------------------------------------------------------------------------------------------------- | ---------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| [040 — At most 1,000 songs in a queue](040_queue_size_cap.md)                                         | `MERGED` — server, Android, Mac, web and Linux                                                                                                                                                                                     |
| [039 — The Linux app, at the Android app's level](039_linux_app.md)                                   | `RELEASED` — 0.17.0, the first Linux release, in the GitHub release with every platform (2026-10-24); 0.17.1, a Linux-only patch (2026-10-25); 0.18.0 built and handed to the Mac for the release with every platform (2026-10-25) |
| [038 — Voice search, "play it on Dhun", and shortcuts on the icon](038_voice_search_and_assistant.md) | `RELEASED` — in the 0.16.2 GitHub release (2026-10-21); Android only; 0.16.2 on the owner's phone                                                                                                                                  |
| [037 — A window, not the whole file, for long files in the cache](037_long_file_cache_window.md)      | `RELEASED` — in the 0.16.1 GitHub release (2026-10-21); 0.16.1 on the owner's phone and Mac; the server stays at `server-v0.16.0`                                                                                                  |
| [036 — Meeting the speed targets at 100,000 songs](036_fast_at_100k.md)                               | `RELEASED` — in the 0.16.0 GitHub release (2026-10-20); 0.16.0 on the owner's phone and Mac; server and web in `server-v0.16.0` on the NAS (2026-10-20)                                                                            |
| [035 — A fast scroller on Android's long lists](035_fast_scroller.md)                                 | `RELEASED` — 0.15.1 on the owner's phone (2026-10-19); Android only, no GitHub release                                                                                                                                             |
| [034 — Shorter settings text, marked defaults, reset](034_settings_text_defaults_reset.md)            | `RELEASED` — in the 0.15.0 GitHub release (2026-10-18); 0.15.0 on the owner's phone and Mac; server and web in `server-v0.15.0` on the NAS (2026-10-18)                                                                            |
| [033 — Bookmarks in long files](033_bookmarks.md)                                                     | `RELEASED` — in the 0.15.0 GitHub release (2026-10-18); 0.15.0 on the owner's phone and Mac; server and web in `server-v0.15.0` on the NAS (2026-10-18)                                                                            |
| [032 — Mac and web catch up with the phone](032_mac_and_web_catch_up.md)                              | `RELEASED` — in the 0.14.0 GitHub release (2026-10-15); 0.14.0 on the owner's phone and Mac; server and web in `server-v0.14.0` on the NAS (2026-10-15)                                                                            |
| [031 — Podcasts and audiobooks](031_podcasts_and_audiobooks.md)                                       | `RELEASED` — in the 0.13.0 GitHub release (2026-10-12); 0.13.0 on the owner's phone and Mac; server and web in `server-v0.13.0` on the NAS (2026-10-12)                                                                            |
| [030 — The web client, at the other apps' level](030_web_client.md)                                   | `RELEASED` — `server-v0.12.3` on the NAS; checked in Chrome, not yet in Safari, Firefox or on a phone                                                                                                                              |
| [029 — Search that forgives spelling, scripts and word order](029_search.md)                          | `RELEASED` — in the 0.13.0 GitHub release (2026-10-12); Mac 0.12.0, phone 0.12.1; server side in `server-v0.12.0` on the NAS                                                                                                       |
| [028 — The Mac app's layout crash, motion and polish](028_mac_crash_and_polish.md)                    | `RELEASED` — in the 0.13.0 GitHub release (2026-10-12); in 0.2.1 on the owner's Mac                                                                                                                                                |
| [027 — Motion and polish on Android](027_motion_and_polish.md)                                        | `RELEASED` — in the 0.13.0 GitHub release (2026-10-12); in 0.11.0 on the owner's phone                                                                                                                                             |
| [026 — Using the Android app without an account](026_without_an_account.md)                           | `RELEASED` — in the 0.13.0 GitHub release (2026-10-12); in 0.10.0 on the owner's phone                                                                                                                                             |
| [025 — Phone-local songs on Android](025_phone_local_songs.md)                                        | `RELEASED` — in the 0.13.0 GitHub release (2026-10-12); in 0.9.0 on the owner's phone                                                                                                                                              |
| [024 — The macOS app, at the Android app's level](024_macos_app.md)                                   | `RELEASED` — in the 0.13.0 GitHub release (2026-10-12); 0.1.0 on the owner's Mac                                                                                                                                                   |
| [023 — Managing family members on Android](023_users_on_android.md)                                   | `RELEASED` — in the 0.13.0 GitHub release (2026-10-12); in 0.8.0 on the owner's phone                                                                                                                                              |
| [022 — A choice of mini player on Android](022_mini_player_styles.md)                                 | `RELEASED` — in the 0.13.0 GitHub release (2026-10-12); in 0.7.0 on the owner's phone                                                                                                                                              |
| [021 — Playing audio files opened from other apps](021_open_from_other_apps.md)                       | `RELEASED` — in the 0.13.0 GitHub release (2026-10-12); in 0.7.0 on the owner's phone                                                                                                                                              |
| [020 — The Queues tab, as Musicolet's](020_queue_screen.md)                                           | `RELEASED` — in the 0.13.0 GitHub release (2026-10-12); in 0.6.0 on the owner's phone                                                                                                                                              |
| [019 — Networking, retries and caching on Android](019_networking_and_caching.md)                     | `RELEASED` — in the 0.13.0 GitHub release (2026-10-12); app in 0.5.0 on the owner's phone; server side in `server-v0.1.4` on the NAS                                                                                               |
| [018 — The settings screen on Android](018_settings_layout.md)                                        | `RELEASED` — in the 0.13.0 GitHub release (2026-10-12); in 0.4.0 on the owner's phone                                                                                                                                              |
| [017 — Hand-off (resume) on Android](017_handoff.md)                                                  | `RELEASED` — in the 0.13.0 GitHub release (2026-10-12); in 0.3.0 on the owner's phone                                                                                                                                              |
| [016 — Play speed and pitch on Android](016_speed_and_pitch.md)                                       | `RELEASED` — in the 0.13.0 GitHub release (2026-10-12); in 0.3.0; settings cap on the NAS (`server-v0.1.3`)                                                                                                                        |
| [015 — Lyrics on Android](015_lyrics.md)                                                              | `RELEASED` — in the 0.13.0 GitHub release (2026-10-12); in 0.3.0 on the owner's phone                                                                                                                                              |
| [014 — The sleep timer on Android](014_sleep_timer.md)                                                | `RELEASED` — in the 0.13.0 GitHub release (2026-10-12); in 0.3.0; redesigned in 0.4.0                                                                                                                                              |
| [013 — Editing playlists on Android](013_playlist_editing.md)                                         | `RELEASED` — in the 0.13.0 GitHub release (2026-10-12); server in `server-v0.1.2`; app in 0.3.0                                                                                                                                    |
| [012 — Downloads on Android](012_downloads.md)                                                        | `RELEASED` — in the 0.13.0 GitHub release (2026-10-12); in 0.2.0 on the owner's phone                                                                                                                                              |
| [011 — The Android app: look, structure and first build](011_android_app.md)                          | `RELEASED` — in the 0.13.0 GitHub release (2026-10-12); on the owner's phone                                                                                                                                                       |
| [010 — Special playlists and automatic views](010_special_playlists.md)                               | `RELEASED` — in the 0.13.0 GitHub release (2026-10-12); server on the NAS; Android on the owner's phone                                                                                                                            |
| [009 — Resume long files on every device](009_resume_long_files.md)                                   | `RELEASED` — in the 0.13.0 GitHub release (2026-10-12); server on the NAS; Android on the owner's phone                                                                                                                            |
| [008 — Listening history](008_listening_history.md)                                                   | `RELEASED` — in the 0.13.0 GitHub release (2026-10-12); server on the NAS; Android on the owner's phone                                                                                                                            |
| [007 — Client architecture and repo layout](007_client_architecture.md)                               | `RELEASED` — in the 0.13.0 GitHub release (2026-10-12); Android built (011); macOS replaced by 024, web by 030                                                                                                                     |
| [006 — API and sync](006_api_and_sync.md)                                                             | `RELEASED` — on the NAS since `server-v0.1.5` (2026-10-08)                                                                                                                                                                         |
| [005 — Storage and library model](005_storage_and_library_model.md)                                   | `RELEASED` — on the NAS since `server-v0.1.5` (2026-10-08)                                                                                                                                                                         |
| [004 — Dhun owns every change to the collection](004_curation_workflow.md)                            | `ACCEPTED` — option C; no code yet                                                                                                                                                                                                 |
| [003 — Deployment on the NAS with Docker Compose](003_deployment.md)                                  | `RELEASED` — on the NAS since `server-v0.1.5` (2026-10-08)                                                                                                                                                                         |
| [002 — Sync, offline and hand-off](002_sync_and_handoff.md)                                           | `RELEASED` — in the 0.13.0 GitHub release (2026-10-12); resume hand-off in 017; live transfer after v1                                                                                                                             |
| [001 — Our own backend, in Go](001_own_backend_in_go.md)                                              | `RELEASED` — on the NAS since `server-v0.1.5` (2026-10-08)                                                                                                                                                                         |
