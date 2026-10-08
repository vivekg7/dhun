# 020 — The Queues tab, as Musicolet's

**Status:** `MERGED` — tried on the emulator; not yet on the owner's phone
**Started:** 2026-10-08

## Problem

Queues are the signature feature, and the owner chooses and edits them the
way ten years of Musicolet have taught. The first Queues tab
([011](011_android_app.md)) departed from it: every queue as a chip in a
row that scrolled sideways, most recently used first, so a queue's place
changed every time another one played. Only the playing queue could be
edited, and the tab had no sort, no search and no way to act on several
songs at once. The owner asked for Musicolet's queue screen and its way of
choosing a queue (2026-10-08).

What Musicolet does was read from the app running on the emulator and from
its layout and string resources (behaviour only, never code; AGENTS.md).

## Decisions

The order, the song rows and the extras are the owner's (2026-10-08); the
layout follows Musicolet.

### Choosing a queue

- **Queues have a fixed, numbered order.** A new queue goes last; dragging
  in the queue picker moves one. Musicolet numbers queues and keeps them in
  place, so "queue 3" stays queue 3. When a 21st queue is made, **the first
  one goes**, with a note saying which, as in Musicolet. A name that is
  already taken is still refilled in place (011), keeping its number.
- The order is the synced setting **`queues.order`**, a JSON array of queue
  ids, so every device numbers the queues the same. A queue it does not name
  yet (made on another device whose order has not arrived, or before 0.6.0)
  follows the named ones, the one used longest ago first, which is how
  Musicolet would have appended them. Ids of queues that are gone are
  ignored, and dropped on the next write. No server change: settings are the
  apps' business (006), and twenty ids fit the 4 KB cap.
- **The queue shown is in a box at the top**, "3. Parachutes ⌄", bold while
  it is the playing one, with **✕ to remove it** (after asking). Tapping
  the box opens **the queue picker**: every queue by number, a radio button
  on the one shown, ▶ on the one playing, and per row a drag handle, rename
  and remove; below, **Remove all others**, which keeps the playing queue.
- Picking a queue only shows it; playback carries on. **Tapping a song of a
  queue that is not playing starts that queue from that song**, and the tab
  stays where it is. **Resume** (▶ in the second row) starts it where it
  was left.
- **Add to a queue…** in every song menu lists the queues by number, plus
  **New queue**, which makes one without playing it. "Add to queue" is now
  "Add to playing queue", to tell the two apart.

Rejected: **keeping the chips**, which show more at once but give queues
no number and push most of them off screen; and **most recently used
first**, which renumbers every queue on each switch.

### The tab

- A second row, as Musicolet's: **Play/Pause** (or **Resume** for a queue
  not playing), **Sort**, where the queue is (**"4 / 6"** over "9:57 left of
  19:19"; a tap scrolls back to the current song), **Save as playlist**
  (adds to an existing playlist or makes one, named after the queue) and a
  menu with **Select multiple** and **Rename**. Shuffle left this row: it is
  on Now playing, as in Musicolet.
- **The current song is outlined**: in the accent in the playing queue; in
  grey and italics in another queue, where it marks where that queue will
  resume. Musicolet does the same; the old filled bar looked like a
  selection.
- Song rows stay **two lines** (title and artist), the owner's choice over
  Musicolet's three, so more of the queue fits on screen.
- **A search box at the bottom**, "Search in this queue…", filtering by
  title, artist or album. Dragging waits until it is cleared, since the
  positions shown are no longer the queue's.

### Editing any queue

Every queue can be reordered, sorted and edited, not only the playing one.
The playing queue goes through the player, which then holds its order; the
others are changed in the database. Both send the same ops (006), so the
server cannot tell them apart. Removing the current song of a queue that is
not playing makes the next one current.

- **Sort**: Randomize, Reverse, title, artist, album, folder and file name,
  year, length, recently added. A sort is one `queue.replace`. In the
  playing queue the playing song plays on: the songs around it are taken
  out and put back in the new order, four player calls instead of one move
  per song. Randomize shuffles the whole queue (Musicolet's default).
- **Select multiple**, from the menu or a long press: tap to select, All or
  None, then Play next, Add to a queue…, Add to playlist… or Remove from
  queue. Back leaves it.
- **Stop after this song**, in the menu of a song in the playing queue: a
  fifth sleep-timer mode ([014](014_sleep_timer.md)) that pauses where that
  song ends, wherever it is in the queue. The timer shows the song's name;
  choosing it again on that song turns it off.

Not taken from Musicolet for now: swipe to remove, share, export as
`.m3u`, preview, and saving a queue back to the playlist it came from.

## Open questions

None.
