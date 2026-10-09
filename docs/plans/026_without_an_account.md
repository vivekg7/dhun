# 026 — Using the Android app without an account

**Status:** `MERGED` — on `main`, checked on the emulator; not yet on the owner's phone
**Started:** 2026-10-09

## Problem

Since [025](025_phone_local_songs.md) the app plays songs that are already
on the phone, but it still opens on the sign-in screen and shows nothing
until a server answers. A phone with music on it, and no server, gets a
locked door.

Signing out makes it worse. It deletes this phone's downloads
([012](012_downloads.md)) and clears the whole database, including the
phone songs' favourites, counts and resume points, which the server never
had and cannot give back. Someone who signs out to lend the phone, or to
change servers, loses gigabytes of files that the same server would send
again unchanged.

## Decisions with the owner (2026-10-09)

- **Use without signing in.** The sign-in screen offers it. Phone songs
  play, queues work, and so do favourites and counts on phone songs.
  Signing in comes later, from Settings → Account.
- **Signing out asks about downloads, each time.** The dialog has
  **Keep downloads** and **Delete downloads**.
- **Kept downloads stay playable.** They show in Albums, Folders, Artists
  and so on beside the phone songs, and play without a server.
- **Phone data stays on sign-out.** The phone songs' favourites, Listen
  Later, counts, resume points and own speeds belong to the phone, not the
  account. So do the queues made only of phone songs. Signing out clears
  only what came from the account.
- **Another family member signing in keeps every kept download** until they
  remove it. The library is the family's, so the files are still the songs
  they say they are.

## Options

### What "not signed in" is

- **A. A guest account on the server.** It needs a server, which is the
  very thing missing.
- **B. A flag on the phone: "use without signing in".** The app opens its
  tabs with whatever is on the phone. Sync already does nothing without a
  token, and the outbox is on disk.

**Chosen: B.** Nothing new on the server, and the app already worked
offline.

### Edits made while signed out

Ops recorded with no account stay in the outbox, as offline ones always
have, and go to the account signed in next. Phone songs are already taken
out of every op ([025](025_phone_local_songs.md)), so what reaches the
server is a queue's NAS songs and marks on kept downloads.

- **Rejected: a separate outbox, or none, while signed out.** A queue
  started signed out, edited after signing in, would then be one the server
  does not know: its `queue.insert` is rejected and the NAS songs added to
  it are lost.

The queues kept at sign-out get a fresh `queue.create` each, after the
outbox is cleared, for the same reason. For the account they came from, the
server already has them, rejects the create as a duplicate, and the
rejected op is dropped. Someone else's sign-in gives each queue a new ID
first (see "How the build went").

### Kept downloads, and which songs they are

A downloaded file is named by the server's song ID. Kept, it keeps its song
row, so it can be shown and played. The risk is signing in to a
**different server**, where the same ID is another song.

- **A. Tie the files to the server address.** The same server is reached
  at its home address and at its Tailscale address
  ([003](003_deployment.md)), so this would delete good files.
- **B. A server identity in the API.** The right answer if this were
  common, but it needs a server release for a one-family edge case.
- **C. Check each kept song against the first full library pull.** A kept
  row stays if the server has that ID with the same path, or a duration
  within 2 seconds. A song moved on the NAS keeps its duration; an upgraded
  file keeps it too; a different server's song with that ID almost never
  has it. Kept songs the server does not have at all go.

**Chosen: C.** It needs no server change, and it is right for both
addresses.

### Downloads while the account is away

Downloads normally deletes every file no pin covers. With the playlists
and Favorites gone, a playlist pin covers nothing, so the next pass would
delete the kept files. So `keptFrom` (the name of the account the files
were kept from) **freezes** the files, with nothing deleted, from the
sign-out until the first full sync after the next sign-in. That sync
brings the playlists and marks back first.

When someone else signs in, every kept file becomes a pin of its own song,
and the playlist and Favorites pins go, because they named the other
person's lists. Album, folder, artist and genre pins are the library's and
stay. The new user removes what they don't want, as with any download.

## Decision

- **Sign-in screen:** a **Use without signing in** button. It asks for
  access to music, turns on phone songs when given, and opens the app.
- **Settings → Account:** signed out, it shows **Sign in**, which returns
  to the sign-in screen, where the same button comes back.
- **Sign out** asks: **Keep downloads**, **Delete downloads**, or
  **Cancel**.
- **What sign-out clears:**
  - always: the outbox, playlists, synced settings, the song cache, and
    the account's queues, marks, resume points and counts.
  - with **Delete downloads**: also the files, their pins, every NAS song
    row, lyrics and covers.
  - with **Keep downloads**: nothing of the downloaded songs: their rows,
    files, pins, lyrics and covers stay.
  - never: phone songs' data, and queues made only of phone songs.
- **The revoked token** ([023](023_users_on_android.md)) is unchanged: the
  phone keeps everything and asks for the password. A different name
  signing in there keeps the downloads, by the same rule.
- **Playback** reloads the last queue once the phone songs are read when
  there is no account, rather than waiting for NAS songs that will not come.

## Known limits

- A queue made while signed out, with only phone songs in it, is also on
  the next account's server as an empty queue. That is already true signed
  in ([025](025_phone_local_songs.md)).
- A favourite set signed out on a kept download goes to whoever signs in
  next.
- Signing back in to the first account, after another member had the
  phone, leaves that account two copies of a kept phone-only queue: the
  old one, empty on the server, and the one carried over under a new ID.

## How the build went (2026-10-09)

Checked on the emulator against a throwaway server:

- without an account: the phone's albums showed and played
- signed in: a queue made without an account reached the server
- an album downloaded, then signed out keeping it: its 15 songs stayed
  and played with no account
- another member signed in: every file became a song pin
- the other way round: a delete sign-out removed the files and kept the
  phone-only queue

The emulator found one bug the plan had missed. A queue's ID is unique
across the whole server, not per user, so a kept queue's `queue.create`
under another member was refused as "already exists", and so would every
edit to it be. So when someone else signs in, each local queue gets a
new ID and is created again as it is
(`Playback.reissueQueues`). For the same reason, `keptFrom` is now
recorded on every sign-out, not only one that keeps downloads.
