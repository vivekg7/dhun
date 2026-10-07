# 006 — API and sync

**Status:** `RELEASED` — implemented in `server/`, contract in `api/openapi.yaml`; running on the NAS as `server-v0.1.2` since 2026-10-07
**Started:** 2026-10-06

## Problem

The API is the only boundary between the backend and the clients (`AGENTS.md`).
It has to:

- let the Android app work offline: browse, search, edit queues and
  playlists, record plays;
- merge those offline changes without losing any of them
  ([002](002_sync_and_handoff.md));
- support resume-style hand-off;
- stream original files with seeking.

## Decisions

### Transport and auth

- **JSON over HTTP, under `/api/v1/`**, served by Go's standard `net/http`
  with its built-in route patterns. No web framework.
- **Plain HTTP**, on the home network and over Tailscale, which encrypts
  the traffic away from home; the home network is trusted (the owner's
  decision, [003](003_deployment.md)). For HTTPS (for example, for the web client's secure-context
  features), use `tailscale serve` in front, with no change to Dhun.
- **Sign-in:** username and password → a long-lived **device token**. One
  token per app install; only its hash is stored, in `devices`
  ([005](005_storage_and_library_model.md)).
  - Apps send `Authorization: Bearer <token>`, including on stream requests.
    ExoPlayer and URLSession both support headers.
  - The web client keeps the token and sends it the same way. It also gets
    it as an `HttpOnly`, `SameSite=Strict` cookie (`Secure` over HTTPS),
    because `<audio>` and `<img>` cannot send headers; the server accepts
    the cookie **only** on `GET` stream, art and lyrics. Cookies are scoped
    to a host, not a port, so every other service on the NAS's name receives
    it too; at most it could fetch a song.
  - Removing a device in the settings revokes its token.
- **The contract is a hand-written `api/openapi.yaml`** in the repo,
  reviewed like code. Clients are hand-written against it, which for about
  20 endpoints is less code than a generator plus its runtime. Server tests
  check responses against the spec.

### The library is synced to the client, not browsed remotely

The whole catalogue (about 7,000 songs × roughly 300 bytes ≈ 2 MB of JSON,
under 0.5 MB gzipped) is small enough for each client to **keep a full copy**:

- `GET /api/v1/library?since=<cursor>` returns new, changed and removed songs
  since the client's cursor. The first call returns everything.
- The Android app then **browses folders, albums, artists and genres, and
  searches, entirely locally**. Every screen works offline and opens
  instantly, and the server needs no paging, sorting or search endpoints.
  This is the biggest simplification in the design.
- Cover art (`GET /api/v1/art/{song_id}?size=…`) and lyrics
  (`GET /api/v1/lyrics/{song_id}`, from a `.lrc` file or embedded) are
  fetched on demand and cached.
- `GET /api/v1/stream/{song_id}` serves the original file with HTTP Range
  support. Downloads use the same endpoint.

### User data: pull changes, push operations

**Pull:** `GET /api/v1/sync?since=<cursor>` returns everything about this
user that changed since `cursor`, as whole objects: queues with their songs,
favorites, and `now_playing`. Each object carries the user's data version it
last changed at ([005](005_storage_and_library_model.md)).

**Playlists come through `/library`, not `/sync`.** They are files, indexed
under the library version, and their visibility (shared, plus the caller's
own) is decided there. Playlist operations still go through `POST /sync`.
They rewrite the file and bump the library version, so the next library pull
carries the result.

**Push:** `POST /api/v1/sync` with an ordered list of **operations** recorded
by the client (option B in [002](002_sync_and_handoff.md)):

```json
{
  "ops": [
    { "id": "3f2…", "at": "2026-10-06T08:12:03Z", "type": "queue.insert",
      "queue": "q_17", "after": 1234, "songs": [881, 902] },
    { "id": "3f3…", "at": "2026-10-06T08:12:10Z", "type": "play",
      "song": 881, "ms": 214000, "endedAt": "2026-10-06T08:15:44Z",
      "end": "finished", "utcOffset": 330, "source": "playlist:12" }
  ]
}
```

- Every operation has a client-generated ID. The server records applied IDs
  (`applied_ops`), so sending the same batch twice is harmless, which
  matters for flaky mobile connections.
- Operations address songs by ID, and positions **relative to a neighbouring
  song** (`after: 1234`) rather than by index. An index goes stale as soon as
  another device edits the same queue; a neighbour usually doesn't.
- The response holds the results plus the same changes a pull would return,
  so one round trip both pushes and pulls.

Operations and how each merges:

| Operation                                               | Merge rule                                                                                                                                         |
| ------------------------------------------------------- | -------------------------------------------------------------------------------------------------------------------------------------------------- |
| `queue.create`, `queue.rename`, `queue.delete`          | Delete wins over later edits. A name clash on create or rename gets a suffix (`Hindi (2)`). The 20-queue limit is enforced by the server.          |
| `queue.insert`, `queue.remove`, `queue.move`            | Applied by song ID. A missing `after` song → append at the end. Inserting a song already in the queue → it is moved there instead (no duplicates). |
| `queue.set_current`                                     | The latest `at` wins.                                                                                                                              |
| `playlist.create`, `playlist.rename`, `playlist.delete` | Same as queues. The server rewrites or renames the `.m3u8` file ([005](005_storage_and_library_model.md)).                                         |
| `playlist.insert`, `playlist.remove`, `playlist.move`   | Same as queues. Playlists may contain duplicates, so they address an item by `(song, occurrence)`.                                                 |
| `favorite.set`, `favorite.unset`                        | The latest `at` wins.                                                                                                                              |
| `listen_later.add`, `listen_later.remove`               | The latest `at` wins. A `play` that reaches 90% into a listed song removes it ([010](010_special_playlists.md)).                                   |
| `resume.set`, `resume.unset`, `setting.set`             | The latest `at` wins, as for favorites ([009](009_resume_long_files.md)).                                                                          |
| `play`                                                  | Append only; never conflicts. One per listen, skips included; fields in [008](008_listening_history.md).                                           |
| `playback.state`                                        | Updates `now_playing` if newer. Sent while playing (at most every 15 s, and on pause, skip or queue switch), not stored in the outbox.             |

Clients apply their own operations locally at once (optimistic), keep them in
an **outbox** until the server confirms them, and replace the local copy with
the server's version after each sync. An offline trip just means a longer
outbox.

### Hand-off (v1: resume)

`GET /api/v1/now-playing` returns the user's latest playback state and the
device it came from. When an app comes to the foreground and that state is
from a different device and newer than its own, it offers **"Continue from
Phone — Song, 2:13"**. Accepting switches to that queue and position.

After v1, live transfer adds `GET /api/v1/events` (Server-Sent Events). It
pushes `now_playing` and "pause, another device took over" without changing
anything above.

### Admin and uploads

Everything the admin panel calls is under `/api/v1/admin/…` and needs the
admin. v1 has user management (list, add a member, reset a password) and
rescan; curation and uploads come after v1 ([004](004_curation_workflow.md)).

## Added during implementation (2026-10-06)

- **Users are managed over the API, not a command.** The admin is created
  from `DHUN_ADMIN_USER` and `DHUN_ADMIN_PASSWORD` on first start, and only
  while there are no users, so editing the variables later never resets an
  account. Members are added with `POST /api/v1/admin/users`; there is one
  admin, so the endpoint cannot create another. Deleting a user is left out
  until there is a need: their playlists, queues and plays would need a
  decision. `dhun user passwd` remains as the way back in for an admin who
  forgets their own password, since nobody else can reset it.

- `GET /api/v1/plays` returns per-song play counts and last-played times,
  so clients can sort by most or recently played without receiving the raw
  play log. Only listens of at least half the song count
  ([008](008_listening_history.md)).
- Operations not in the table above: `queue.replace` (a sort or shuffle sends
  the whole order; last writer wins), `queue.set_mode` (shuffle, repeat) and
  `playlist.replace`.
- An operation's `at` is stored in a fixed-width format. "The later change
  wins" compares these as strings, and RFC 3339 with variable fractions sorts
  `03.5Z` before `03Z`.

## Hardened before the first install (2026-10-07)

An adversarial review before running on the NAS added:

- **Login:** at most two password checks at once (argon2 takes 64 MiB
  each), an unknown name costs the same time as a known one, and after five
  wrong passwords for a name each try waits twice as long, up to 15 minutes
  (`429` with `Retry-After`).
- **Requests:** bodies must be `application/json` (a cross-site form cannot
  send that without a preflight, which is never granted); browsers' cross-
  origin state-changing requests are refused (`http.CrossOriginProtection`);
  every response has `nosniff`, `no-referrer` and `DENY` framing, and API
  responses a `sandbox` CSP. Read and idle timeouts are set; there is no write
  timeout, because a stream may be long.
- **Sync:** at most 500 operations are applied per request (the rest stay in
  the outbox for the next one) and 20,000 songs per operation. A client time
  in the future counts as now, so a phone with a fast clock cannot win every
  later conflict.
- **Art:** sizes round up to 128, 256, 512 or 1024; an image over 40
  megapixels is served unscaled rather than decoded (a 70-byte file can
  claim 16000×16000); cache files are written atomically and replace the
  song's older versions.

## Rejected

- **Subsonic API as the primary API:** it has one queue per user and no
  operation-based sync. It is planned as an extra layer after v1
  (REQUIREMENTS).
- **GraphQL / gRPC:** a schema runtime on every client, for about 20
  endpoints.
- **Server-side browse and search endpoints:** pointless once the client
  holds the whole catalogue, and they would make offline browsing a second
  code path.
- **Whole-object last-write-wins sync:** loses concurrent edits
  ([002](002_sync_and_handoff.md), option A).
- **Generated API clients:** a generator plus a runtime dependency per
  client, to save a little hand-written code.

## Open questions

None for the owner.
