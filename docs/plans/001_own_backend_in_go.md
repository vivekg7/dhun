# 001 — Our own backend, in Go

**Status:** `RELEASED` — server in `server/`; running on the NAS as `server-v0.1.4` since 2026-10-08
**Started:** 2026-10-06

## Problem

The clients need a server on the NAS to index about 7,000 songs, stream them,
and hold each user's queues, playlists, favorites and play counts. The
project's core features are not supported by any existing server API:

- **Multiple named queues per user.** Subsonic's `savePlayQueue` and
  OpenSubsonic's `indexBasedQueue` both store **one** queue per user, and no
  OpenSubsonic extension covers more
  ([research](../research/existing-options.md)).
- **The real folder tree.** The library is organised by folder. Navidrome, the
  strongest Subsonic server, deliberately exposes only a tag-based tree.
- **Per-user upload staging with admin review**
  ([REQUIREMENTS](../REQUIREMENTS.md#users-and-the-library)). No server has it.
- **Offline working copies that sync back, and hand-off between devices**
  ([002](002_sync_and_handoff.md)). These need a sync API that Subsonic does
  not have.

## Options

**A. Navidrome (or Gonic / LMS) plus Subsonic clients.** Mature, low effort,
real folders with Gonic or LMS. Rejected: queues, uploads and sync would all
have to be faked on top of an API that cannot express them — named queues
stored as hidden playlists, positions in a side store. Every client would carry
those workarounds, and the server could never validate them.

**B. Jellyfin.** A common choice for video on a NAS. Rejected: its API is
built for video, it is the heaviest option, it still has a single play queue,
and tying music to it couples two unrelated upgrade cycles.

**C. Our own backend.** Chosen. A music server is much simpler than the app:
scan files, read tags, serve byte ranges, and store per-user rows. Owning the
API lets it express queues, uploads and sync directly. A Subsonic-compatible
layer can be added later for other apps to use (planned after v1).

Language, if C:

- **Go.** Chosen. Produces one static binary and a small image. It is
  light on RAM and good at concurrent streaming. Navidrome and Gonic are
  written in Go, so mature tag-reading and scanning approaches exist to learn
  from or reuse, and both are GPL-3.0 like this project.
- **Kotlin (JVM, Ktor).** Would let the server share data models with an
  Android/KMP client. Rejected: a JVM idles at roughly 150 MB or more on a NAS
  that also runs other apps, and sharing models only saves a little
  work, because the API contract is the boundary anyway.
- **Rust.** The lowest resource use, but slower to iterate on, and the NAS has
  no shortage of CPU for this workload.

## Decision

Write our own backend in Go. Ship it as a `linux/amd64` Docker image for
Synology Container Manager. The target NAS is a recent x86-64 Synology with no
integrated GPU. That is fine, because we stream original files and never
transcode.

## Plan

To be detailed in a later plan, once the API and storage design are written:

1. Data model and storage (SQLite is the likely default for a single-node,
   family-sized server — to be decided in its own plan).
2. API design: auth, library browsing (folders and tags), streaming, the
   per-user data, the sync endpoint.
3. Scanner: tags, art, lyrics; incremental rescans.
4. Packaging: Dockerfile, Container Manager setup, the volume mounts (library
   read-only, a writable data directory and upload staging area).

## Open questions

- Database choice and migration tool.
- How the API contract is shared with the clients (OpenAPI spec with generated
  clients, or written by hand).
- Where the music share and the upload staging area live on the NAS.
