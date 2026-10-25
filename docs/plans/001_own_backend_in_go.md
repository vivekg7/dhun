# 001 — Our own backend, in Go

**Status:** `RELEASED` — server in `server/`; on the NAS since `server-v0.1.5` (2026-10-08)
**Started:** 2026-10-06

## Problem

The clients need a server on the NAS to index the collection (about 7,000
songs then, 30,000 now), stream it, and hold each user's queues, playlists,
favorites and play counts. No existing server API supports the project's
core features:

- **Multiple named queues per user.** Subsonic's `savePlayQueue` and
  OpenSubsonic's `indexBasedQueue` both store **one** queue per user, and no
  OpenSubsonic extension covers more
  ([research](../research/existing-options.md)).
- **The real folder tree.** The library is organised by folder. Navidrome, the
  strongest Subsonic server, deliberately exposes only a tag-based tree.
- **Per-user upload staging with admin review**
  ([REQUIREMENTS](../REQUIREMENTS.md#users-and-the-library)). No server has it.
- **Offline working copies that sync back, and hand-off between devices**
  ([002](002_sync_and_handoff.md)), which need a sync API Subsonic lacks.

## Options

**A. Navidrome (or Gonic / LMS) plus Subsonic clients.** Mature, low effort,
real folders with Gonic or LMS. Rejected: queues, uploads and sync would all
be faked on top of an API that cannot express them (named queues as hidden
playlists, positions in a side store). Every client would carry those
workarounds, and the server could never validate them.

**B. Jellyfin.** A common choice for video on a NAS. Rejected: its API is
built for video, it is the heaviest option, it still has a single play queue,
and tying music to it couples two unrelated upgrade cycles.

**C. Our own backend.** Chosen. A music server is much simpler than the app:
scan files, read tags, serve byte ranges, store per-user rows. Owning the
API lets it express queues, uploads and sync directly. A Subsonic-compatible
layer can be added for other apps after v1.

Language, if C:

- **Go.** Chosen. One static binary and a small image, light on RAM and good
  at concurrent streaming. Navidrome and Gonic are written in Go, so mature
  tag-reading and scanning approaches exist to learn from or reuse, and both
  are GPL-3.0 like this project.
- **Kotlin (JVM, Ktor).** Would share data models with an Android/KMP
  client. Rejected: a JVM idles at 150 MB or more on a NAS that also runs
  other apps, and sharing models saves little, because the API contract is
  the boundary anyway.
- **Rust.** The lowest resource use, but slower to iterate on, and the NAS has
  CPU to spare for this workload.

## Decision

Write our own backend in Go, shipped as a `linux/amd64` Docker image for
Synology Container Manager. The target NAS is a recent x86-64 Synology with
no integrated GPU, which is fine: we stream original files and never
transcode.

## Plan

Each part got its own plan: storage and the scanner in
[005](005_storage_and_library_model.md) (SQLite), the API and sync in
[006](006_api_and_sync.md), packaging and the mounts in
[003](003_deployment.md).

## Open questions

- ~~Database choice and migration tool.~~ SQLite, with numbered `.sql`
  migrations of our own ([005](005_storage_and_library_model.md)).
- ~~How the API contract is shared with the clients.~~ A hand-written
  `api/openapi.yaml`, with hand-written clients ([006](006_api_and_sync.md)).
- ~~Where the music share and the upload staging area live on the NAS.~~ The
  share is mounted read-only ([003](003_deployment.md)); uploads, after v1,
  land in `_inbox/<user>-<date>/` ([004](004_curation_workflow.md)).
