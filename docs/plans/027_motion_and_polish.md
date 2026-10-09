# 027 — Motion and polish on Android

**Status:** `MERGED` — in 0.11.0 on the owner's phone (2026-10-09)
**Started:** 2026-10-09

## Problem

The owner compared the app with Musicolet and found it rough:

- **Dragging songs in a queue was not smooth.** Only the dragged row
  moved; the rows it passed stayed put until the finger lifted, the list
  did not scroll at its edges, and on release the row snapped back to where
  it started, then jumped to its new place once the database write came
  back.
- **Most changes cut rather than moved**: a new song on Now playing, the
  cover and the lyrics, play and pause, opening a page and going back,
  Settings, the mini player appearing as Now playing was swiped away.
- **Previous looked broken on another phone in the family**: "two small
  chips arranged differently".
- Small things that did not match: one of the three Playlists cards filled
  with the accent, empty-screen notes at the top of the screen, Play and
  Shuffle on an empty list.

## Options

- **A reorder library** (Calvin-LL's Reorderable and the like). Rejected:
  a dependency for a hundred lines (AGENTS.md), and the snap-back is ours,
  from the database write, which a library would not know about.
- **Our own reorder on the lazy list** _(chosen)_.
- **Compose's shared-element and predictive-back transitions.** Rejected
  for now: more than this needs, and still settling between releases.
- **Plain `AnimatedContent` with one set of timings** _(chosen)_.

## Decision

Decided while building; the owner may override any of it.

- **Reorder** (`Reorder.kt`, the queue, the queue picker and a playlist):
  the row follows the finger, lifted; the rows it passes slide aside as it
  crosses their middle; held within 64 dp of an edge the list scrolls,
  faster nearer the edge; a light haptic when it lifts. The new order stays
  on screen until the list given changes, so the drop never snaps back.
  Rows are matched by key, not by position, so a header above them (a
  playlist's) is left alone. The handle has no tooltip: holding it before
  dragging would show one over the row.
- **One set of timings** (`Motion.kt`): 150 ms to fade out, 280 ms to slide
  or fade in, 360 ms for a cover crossing the screen.
- **Now playing:** the covers move side by side as cards, the next coming
  from the right and the previous from the left (off the end of a
  repeating queue to its start is still forward). The title and artist
  slide a shorter way and fade. The covers either side are fetched ahead,
  so a real cover slides in, not the empty note. The lyrics fade in and out
  over the cover; Play and Pause turn into each other, and the button gives
  under the finger; Favorites and Listen Later pop when turned on, not when
  shown already on. "Continue from…" folds in and out.
- **Pages** slide in a fifth of the width from the side they open on, and
  back the way they came; Settings and its pages do the same.
- **The tab mark** follows the pages as they are swiped, and slides to a
  tab tapped. The tab itself changes only once the pager settles: setting
  it from a page passed on the way turned a long jump round, and a tap
  during it landed on the wrong tab. A jump further than next door ("Go to
  album" from Queues) is made at once, as tapping a tab is, rather than
  sliding through every tab between.
- **The mini player** folds in and out rather than appearing, as does the
  hand-off bar.
- **Covers fade in** over the note, and a cover already in memory is shown
  in the first frame, so a list scrolled back does not flash empty.
- **Removing a song** from a queue slides the rows below up into its place.
- **Previous's icon** was drawn with a relative move after a closed shape.
  Older Android measures that move from the last point drawn instead of
  from where the shape began, so its triangle sat below its bar. Every
  icon now starts a shape after a `z` with an absolute move.
- **The same look for the same thing:** the Favorites, Listen Later and
  Downloads cards are alike; a note that is the whole screen sits in its
  middle; Play and Shuffle show only when there is something to play;
  Search says what it searches before anything is typed.
