# 033 — Bookmarks in long files

**Status:** `RELEASED` — in the 0.15.0 GitHub release (2026-10-18); 0.15.0 on the owner's phone and Mac; server and web in `server-v0.15.0` on the NAS (2026-10-18)
**Started:** 2026-10-17

## Problem

A podcast or a book has moments worth coming back to: a recommendation,
a passage to quote, a chapter to play to someone. The resume point
([009](009_resume_long_files.md)) remembers only where the listener
stopped, and the chapters only where the author put them. Musicolet keeps
bookmarks per song in a side file on one phone. Dhun's would have to
follow the user to every device, as everything else they mark does.

What a bookmark is (owner, 2026-10-17):

- **A place and an optional note.** Adding one saves it at once,
  labelled by its time and chapter; a note can be written or changed
  later.
- **Long files only:** podcasts, audiobooks and anything long enough to
  resume, the rule the seek buttons use.
- **Listed in the file's chapter list,** the Chapters and notes sheet,
  and a tap goes to it. No page of every bookmark across files.
- **Added from a button on Now playing,** beside Favorites and Listen
  Later. Not from the notification or a headset.

## Options

**A. Store them in a synced setting** (`bookmarks.<song>`, a JSON list).
No server change. But a setting is one value where the later change
wins: two devices adding a bookmark to the same file while one is
offline would lose one of them, and a setting is capped at 4 KB.

**B. A table of bookmarks, one row each,** with a client-made ID, synced
like resume points. Each bookmark merges on its own, so adding on two
devices keeps both. Costs a migration, two operations and a field in the
pull.

## Decision

**B.**

- **Table `bookmarks`:** `user_id`, `id` (made by the client, so one made
  offline has its ID at once), `song_id`, `position_ms`, `note`, `at`,
  `deleted`, `version`.
- **`bookmark.set {bookmark, song, positionMs, note}`** makes one or
  changes it, sending the whole bookmark each time; the later `at` wins.
  **`bookmark.unset {bookmark}`** deletes it, and a delete wins over any
  edit, later or not, as for queues: a bookmark deleted on one device and
  edited on another offline stays deleted.
- **Limits** keep one user from filling the database: an ID of 1–64
  characters, a note of at most 1,000 characters, and 10,000 bookmarks
  per user not counting deleted ones.
- **The pull** returns `bookmarks` changed since the cursor, each with
  its `id`, and only `id` and `deleted` for a deleted one.
- The server never decides what is long. It stores a bookmark for any
  song, as it does a resume point.

**What the apps do:**

- **Now playing** has a bookmark button beside Favorites and Listen
  Later, for a long file only. A tap adds a bookmark where it is playing
  and says so briefly.
- **The chapter list** gains a Bookmarks section, by place in the file:
  the time, the chapter it falls in, and the note. A tap goes to it,
  playing on if the file was playing, as a tap on a chapter does; a menu
  edits the note or deletes it.
- **The line under the title** that names the chapter opens the sheet.
  A file without chapters that has bookmarks shows "2 bookmarks" there
  instead, so a long music file (a DJ mix), which keeps Shuffle where a
  book has its Chapters and notes button, still reaches them.

**The server goes out first.** An older server refuses the whole push
that carries a bookmark, not just that operation: it reads the request
strictly, and `bookmark` is a field it does not know. Nothing is lost,
since the operations stay in the outbox, but nothing else from that
device syncs either, its queues and resume points included, until the
server is updated. Then it all goes up at once.

## Rejected

- **A setting per file (A):** loses a bookmark added on two devices at
  once, and is size-limited.
- **A page of every bookmark across files:** the owner wanted them with
  the file's chapters.
- **Adding from the notification or a headset:** the owner chose the
  screen only. A headset gesture would also cost a press pattern buds
  cannot send.
- **Bookmarks on any song:** songs are short, and their place is not kept.

## Plan

1. Server: migration `0010_bookmarks.sql`, the two operations, the pull
   field, `api/openapi.yaml`, tests.
2. Android: the table, the outbox ops, the button and the sheet.
3. macOS and the web: the same. On the Mac a note is written in place
   in the popover's row, and the row's menu is also its right-click menu;
   on the web each row has edit and delete buttons rather than a menu,
   since the web's menu closes any open panel and would close this one.
