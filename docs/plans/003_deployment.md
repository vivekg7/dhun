# 003 — Deployment on the NAS with Docker Compose

**Status:** `IN PROGRESS` — `deploy/Dockerfile` and `deploy/docker-compose.yml`; image not yet published
**Started:** 2026-10-06

## Problem

The backend ([001](001_own_backend_in_go.md)) runs on a Synology DS1525+ next
to Jellyfin and Immich. It must use the existing `Music` folder in place, and
installing it should take one file and one edited line.

## Decision

**One container, one read-write mount.** `MEDIA_PATH` is the `Music` folder.
Dhun uses it for everything:

| Path under `MEDIA_PATH`            | Use                                                                                                    |
| ---------------------------------- | ------------------------------------------------------------------------------------------------------ |
| Every folder not starting with `_` | The collection, scanned and streamed. Changed only by admin actions ([004](004_curation_workflow.md)). |
| `Playlists/*.m3u8`                 | Shared playlists, read and written by Dhun.                                                            |
| `Playlists/<username>/*.m3u8`      | Each user's playlists.                                                                                 |
| `_dhun/`                           | Dhun's own state: SQLite database, art cache, logs.                                                    |
| `_inbox/`, `_meta/`, `_trash/`     | Review inbox, action log, trash; not scanned as the collection.                                        |

`docker-compose.yml`. The only line to edit is the one marked `MEDIA_PATH`:

```yaml
services:
  dhun:
    container_name: dhun
    image: ghcr.io/vivekg7/dhun:latest
    user: "1026:100" # first DSM user : group "users"; files Dhun writes stay editable over SMB
    ports:
      - "8585:8585"
    volumes:
      - /volume1/homes/you/Media/Music:/media # MEDIA_PATH: your Music folder
      - /etc/localtime:/etc/localtime:ro
    environment:
      - TZ=Asia/Kolkata
    restart: unless-stopped
```

Why each choice:

- **One mount, read-write.** Playlists are `.m3u8` files that Dhun edits,
  and its state lives beside the music, so a single mount covers both. It is
  also backed up wherever `Music` already is. The admin panel's moves and
  upgrades ([004](004_curation_workflow.md)) need write access anyway.
- **SQLite in `_dhun/`, no database container.** 3–4 users and about 7,000
  songs don't need Postgres. Immich warns that a database must not sit on a
  network share. That is satisfied here, because `MEDIA_PATH` is a local NAS
  volume; it only looks like a network share from the Mac.
- **`user: 1026:100`** (Jellyfin's practice of not running as root). 1026 is
  the first DSM user and 100 the `users` group, so playlists Dhun writes
  remain editable from the Mac. Change it if your user's `id` differs.
- **`restart: unless-stopped`** (Jellyfin) and **`/etc/localtime` plus `TZ`**
  (Immich): play history is time-of-day data.
- **Port `8585`** avoids Jellyfin (8096), Immich (2283) and Subsonic servers
  (4040, 4533). Reached over Tailscale, like Immich.

## Rejected

- **Separate mounts for the library, the data and the playlists, with paths
  in `.env` (Immich's style).** More flexible, but more to configure, and it
  buys nothing for one NAS and one family.
- **Mounting only `Library/` and `Playlists/`.** 122 playlist entries point
  into `Apple Music/`, `Spotify/` and `Collection/`.
- **Postgres** and **a native Synology package**: see above. Container
  Manager is already how the other apps run.
