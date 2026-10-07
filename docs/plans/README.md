# Plans — index

One file per piece of work larger than a bug fix. Each records the problem, the
options, the decision and **why**, and the alternatives that were rejected.
The index lists the newest plan first.

Read this index before starting anything. The project spans a backend and three
or four clients built over months, mostly by agents. This record is what stops
a rejected idea from being proposed again, and what explains why a part is
shaped the way it is.

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

| Plan                                                                         | Status                                                      |
| ---------------------------------------------------------------------------- | ----------------------------------------------------------- |
| [013 — Editing playlists on Android](013_playlist_editing.md)                | `IN PROGRESS` — server in `server-v0.1.2`; app not released |
| [012 — Downloads on Android](012_downloads.md)                               | `MERGED` — in 0.2.0 on the owner's phone                    |
| [011 — The Android app: look, structure and first build](011_android_app.md) | `MERGED` — on the owner's phone; no GitHub release yet      |
| [010 — Special playlists and automatic views](010_special_playlists.md)      | `IN PROGRESS` — server on the NAS; Android not released     |
| [009 — Resume long files on every device](009_resume_long_files.md)          | `IN PROGRESS` — server on the NAS; Android not released     |
| [008 — Listening history](008_listening_history.md)                          | `IN PROGRESS` — server on the NAS; Android not released     |
| [007 — Client architecture and repo layout](007_client_architecture.md)      | `IN PROGRESS` — Android under way (011)                     |
| [006 — API and sync](006_api_and_sync.md)                                    | `RELEASED` — `server-v0.1.2` on the NAS since 2026-10-07    |
| [005 — Storage and library model](005_storage_and_library_model.md)          | `RELEASED` — `server-v0.1.2` on the NAS since 2026-10-07    |
| [004 — Dhun owns every change to the collection](004_curation_workflow.md)   | `ACCEPTED` — option C; no code yet                          |
| [003 — Deployment on the NAS with Docker Compose](003_deployment.md)         | `RELEASED` — `server-v0.1.2` on the NAS since 2026-10-07    |
| [002 — Sync, offline and hand-off](002_sync_and_handoff.md)                  | `ACCEPTED` — mechanics in 006                               |
| [001 — Our own backend, in Go](001_own_backend_in_go.md)                     | `RELEASED` — `server-v0.1.2` on the NAS since 2026-10-07    |
