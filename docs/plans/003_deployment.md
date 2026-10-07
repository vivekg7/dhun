# 003 — Deployment on the NAS with Docker Compose

**Status:** `IN PROGRESS` — `server-v0.1.0` published to `ghcr.io/vivekg7/dhun` (amd64, arm64); not yet running on the NAS
**Started:** 2026-10-06 · **Revised:** 2026-10-07 (safety review before the first install)

## Problem

The backend ([001](001_own_backend_in_go.md)) runs on a Synology DS1525+ next
to Jellyfin, Immich and Vaultwarden. The `Music` folder is a collection built
over years and must not be put at risk: Dhun has to use it in place, be
unable to damage it, and be reachable only by the family.

The owner runs every container the same way: a project folder per app in
the `docker` shared folder (`/volume1/docker/<app>/`), holding the compose
file and _all_ of the app's data, config and cache, so deleting and
re-creating the project in Container Manager loses nothing.

## Decision

**Music is mounted read-only, except `Playlists/`. Dhun's state lives in its
project folder.**

| Mount                                        | In the container   | Use                                                                                                                                      |
| -------------------------------------------- | ------------------ | ---------------------------------------------------------------------------------------------------------------------------------------- |
| `/volume1/docker/dhun/data`                  | `/data`, rw        | The database and its nightly backups, the art cache, kept copies of changed and deleted playlists, the tag reader's compiled code cache. |
| `/volume1/homes/<you>/Media/Music`           | `/media`, **ro**   | The collection, scanned and streamed. The kernel refuses every write, whatever the code does.                                            |
| `/volume1/homes/<you>/Media/Music/Playlists` | `/media/Playlists` | The `.m3u8` playlists, the only files in `Music` that Dhun edits.                                                                        |

`deploy/docker-compose.yml` is the file to copy. Why each choice:

- **Read-only Music.** In v1 Dhun never changes audio, so it should not be
  able to: a bug, a bad upgrade or an attacker inside the container cannot
  delete, rename or rewrite a song. Jellyfin mounts the same folder `:ro`.
  When the admin panel's moves and upgrades arrive ([004](004_curation_workflow.md)),
  that plan decides how much write access they get.
- **`Playlists/` writable, and every change kept.** Before Dhun rewrites,
  renames or deletes a playlist file, it copies it to
  `data/playlists/history/<date>/` (the day's first version) or
  `data/playlists/deleted/<date>/` (every deletion). An edit changes only the
  lines it is about ([005](005_storage_and_library_model.md)).
- **State in the project folder**, the owner's convention: the database is
  not inside the music share (Immich warns against databases on shares the
  Mac mounts over SMB), deleting the Container Manager project keeps it, and
  the music folder holds only music.
- **Port `8585` on the home network, plain HTTP, like DSM (5001) and
  Jellyfin (8096) on this NAS.** At home the apps connect directly; away,
  over Tailscale to the same port. The owner's decision (2026-10-07): the
  home network is trusted. What protects the server on it is its own login
  (argon2 passwords, guessing slowed per name, device tokens) rather than
  the network. The trade-off: on the home Wi-Fi the password at sign-in and
  the device token travel unencrypted. Optional HTTPS needs no change to
  Dhun:

  ```sh
  tailscale serve --bg --https=8586 http://127.0.0.1:8585
  # → https://gargantua.<tailnet>.ts.net:8586; the cookie becomes Secure
  ```

- **Hardened container.** The image is one static binary on `scratch` (no
  shell, no package manager) and runs as `nobody` unless told otherwise.
  Compose adds: `user: 1026:100` (the owner, so files Dhun writes stay
  editable over SMB), `read_only: true`, `cap_drop: [ALL]`,
  `no-new-privileges`, `mem_limit: 1g` (steady state is about 30 MB, the
  first scan about 150 MB) and `pids_limit`.
- **A fixed image tag** (`server-vX.Y.Z`, not `latest`): an upgrade happens
  when the owner edits the tag, never on a pull.
- **The admin from the environment** (as Immich and Navidrome seed their
  first account). The server refuses to start with no users and no admin
  configured, so a fresh install is never left open. The variables are read
  only while there are no users; after the first start the password line
  should be deleted, and changing them never resets an account.
- **`restart: unless-stopped`** (Jellyfin) and **`/etc/localtime` plus `TZ`**
  (Immich): play history is time-of-day data.
- **Port `8585`** avoids Jellyfin (8096), Immich (2283) and Subsonic servers
  (4040, 4533).

**Safety nets in the server**, independent of the mounts:

- A mistyped or unmounted path gives Docker an empty folder. A scan that
  finds no audio while songs are known, or no `Playlists/` while playlists
  are known, stops without changing anything.
- Songs are never deleted from the database: a vanished file is marked
  missing and keeps its ID, plays and queue places until it returns.
- Synology's `#snapshot`, `#recycle` and `@eaDir` folders are never scanned.

## Install

1. In File Station, create `docker/dhun/data` as your own user. (If Docker
   creates `data`, it belongs to root and Dhun cannot write to it.)
2. Copy `deploy/docker-compose.yml` to `docker/dhun/` and edit the lines
   marked `EDIT`: the two Music paths, the `user:` IDs (`id <you>` over SSH),
   and the admin's name and password.
3. Container Manager → Project → Create, path `docker/dhun`.
4. Check the log for `scan done … failed=0`.
5. Delete the `DHUN_ADMIN_PASSWORD` line and rebuild the project.

## Rejected

- **One read-write mount of `Music` for everything, with state in
  `Music/_dhun/`** (the first version of this plan). One line to edit, but
  any bug could touch any song, and it broke the owner's convention of
  keeping app state in the project folder. Superseded on 2026-10-07.
- **Separate mounts for each top-level folder, with paths in `.env`
  (Immich's style).** More to configure, and the read-only parent mount
  already gives the protection.
- **Mounting only `Library/` and `Playlists/`.** 122 playlist entries point
  into `Apple Music/`, `Spotify/` and `Collection/`.
- **Publishing on `127.0.0.1` only, reached through `tailscale serve`**
  (proposed in the safety review). HTTPS everywhere and nothing on the LAN,
  but every family device would need Tailscale running even at home, unlike
  the NAS's other apps. Kept as an option above.
- **Postgres** and **a native Synology package**: 3–4 users and about 7,000
  songs don't need a database server, and Container Manager is already how
  the other apps run.
