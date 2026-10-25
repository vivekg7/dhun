# 004 — Dhun owns every change to the collection

**Status:** `ACCEPTED` — option C; no code yet
**Started:** 2026-10-06

## Problem

An existing workflow maintains the collection. Its footprint on the NAS (seen
on 2026-10-06):

- `_inbox/<source>-<date>/` holds new files from downloaders and from recovery and merge runs.
- `_meta/*.jsonl` is an append-only action log: per-source imports and
  rejects, `upgrades.jsonl`, `lyrics.jsonl` (lyrics lookups per file), and
  `library-merges.jsonl` (`delete` / `move` / `rmdir` entries; a `delete`
  records the removed path, the `kept` path, the source, the source ID, the
  bitrate and the time).
- `_trash/<action>-<date>/` holds files removed by dedupe, merge and renumber
  runs.
- It rebuilds `.m3u8` playlists when files move. One playlist says "rebuilt
  2026-10-05 from the legacy folder Collection/Motivation".

Dhun stores data that points at songs: play counts, favorites, queues,
playlists. When this workflow moves, renames, upgrades or deduplicates a
file, that data must follow the song. After v1, family uploads need the same
review this workflow already does: a quality check and deduplication.

## Options

**A. Separate. Dhun only rescans.** The workflow stays as it is. On
rescan, Dhun matches a file that vanished from one path and appeared at
another by audio fingerprint, falling back to tags and duration, and moves
its data across. Simple, with no coupling, but heuristic: an **upgrade**
(same song, better file) or a **dedupe** (two copies → one) is exactly where
fingerprint and tags differ, so play counts can be lost or attached to the
wrong file.

**B. Separate, but Dhun reads the workflow's log.** A, plus Dhun reads new
lines in `_meta/library-merges.jsonl` and `upgrades.jsonl` (`delete X, kept
Y`; `move X → Y`) and moves the data exactly, using A's matching only for
changes the log does not explain. Exact for everything the workflow does,
but Dhun depends on the log format: if the workflow changes it, Dhun's
importer must change too, so the format would need writing down as a
contract.

**C. Dhun absorbs the workflow.** The import, review, dedupe, upgrade and
lyrics steps move into Dhun, as an admin screen on the web client, and the
current tool is retired. Family uploads after v1 land in
`_inbox/<username>-<date>/` and go through the same review as downloader
imports. One system, with song identity kept exactly because Dhun makes
every change itself, and uploads come almost free. The most work, and it
rebuilds something that already works: not v1 material.

**D. Reverse it: the workflow calls Dhun.** The tool tells Dhun's API "I
moved X to Y" as it acts. Exact like B, but it needs changes to the tool and
an always-reachable Dhun, where B gets the same from data the tool already
writes.

## Decision

**C — Dhun absorbs the workflow** (decided by the owner, 2026-10-06). Every
move, upgrade and removal of a file in the collection is done from Dhun's
admin panel. Dhun always knows what happened, so it carries play counts,
favorites, queue entries and playlist entries across the change exactly.
B and D are not needed, and A remains only as a fallback for changes made
behind Dhun's back (for example, files dropped in over SMB).

How that shapes the design:

- **A song has a stable ID that is independent of its path.** Every user's
  data points at the song ID, never at a file path. A move changes the path
  on the song; an upgrade swaps the file under the same ID; neither
  rewrites any user data. `.m3u8` playlists, which must store paths, are
  rewritten by Dhun in the same action.
- **Nothing is ever hard-deleted.** An **upgrade** (a better-quality copy)
  and a **removal** both move the old file to `_trash/<action>-<date>/`.
  Every action is appended to a log in `_meta/`, like the current tool's,
  so it can be audited and undone. Emptying the trash is a separate,
  explicit admin action.

- **Review flows share one place.** Downloader imports and, after v1,
  family uploads arrive in `_inbox/<source>-<date>/`. The admin reviews them
  (quality, duplicates against the library) and accepts each file into the
  collection, upgrades an existing song with it, or rejects it to `_trash/`.
- **The current tool keeps running until the admin panel replaces it.**
  Until then, Dhun relies on rescans with matching (option A) for changes the
  tool makes. Its existing `_meta/*.jsonl` history is kept as-is; Dhun does
  not need to import it.

## Plan

Detailed in its own plan when the admin panel is built: the action log
format, the `_trash` layout, the review screen, and duplicate detection
(audio fingerprint plus tags and duration).
