# 035 — A fast scroller on Android's long lists

**Status:** `RELEASED` — 0.15.1 on the owner's phone (2026-10-19); Android only, no GitHub release
**Started:** 2026-10-19

## Problem

No list on Android had a scrollbar (owner, 2026-10-19). Compose's lazy lists
draw none by default, and nothing in the app added one. With about 7,000
songs, a few hundred artists and albums, and podcast feeds hundreds of
episodes long, flinging was the only way to cross a list. Musicolet has a
thumb you can drag, and our feature inventory never listed it, so it was
missed.

The Mac and web clients already have one: SwiftUI's `List` and the
browser's scrollbars are both draggable.

## Options

**Where the thumb comes from**

- **Material3's scrollbar.** Not in the stable Material3 in our BOM (1.4).
  Waiting for it leaves the gap open, and an alpha artifact is a dependency
  we would have to track.
- **A library.** Several exist. Each is a dependency for about a hundred
  lines of code, against the rule on dependencies.
- **Our own, on foundation's `scrollIndicatorState`** (chosen). Foundation
  1.12 gives every lazy list and grid a scroll offset, a content size and a
  viewport size. That covers the hard part, which is where to draw the
  thumb. What is left is drawing it and the drag.

**What a drag does**

- **Scroll by pixels.** It works with any `ScrollableState`, but a jump
  across 7,000 rows by pixels lays out every row in between.
- **Jump to an item** (chosen). A drag maps the thumb's place to an item
  index and calls `scrollToItem`. That needs the list or grid state itself,
  so there is one overload for each.

**Which lists get the letter bubble**

Only lists in name order: Artists, Genres, Albums, playlists, a folder's
subfolders, and the audiobooks and podcasts pages. The letter would mislead
on lists in any other order: an album in track order, a queue, search
results by relevance, Favorites by when a song was added.

## Decision

`ui/FastScroll.kt` wraps a list. The thumb sits at the end edge:

- **It shows only on a long list**, one that holds more than three screens.
  A shorter list scrolls well enough by hand.
- **It shows only while the list moves**, and fades out 1.5 s after it
  stops. A thumb that is always there would cover the rows' menu buttons.
  While it is hidden, it takes no touches.
- **Dragging the thumb jumps through the list.** While you drag, the thumb
  follows your finger, not the list's estimate of its position, which
  changes as rows of different heights are laid out and would make the
  thumb twitch.
- **On a list in name order, a bubble beside the thumb shows the letter of
  the row at the top.** A digit or a sign shows as `#`. Devanagari shows
  its own letter, since the natural sort puts it after Z.

It covers every full-screen list: the Folders, Albums, Artists, Genres and
Playlists tabs, each album, artist, genre and playlist, the automatic
lists, Downloads, the queue, Search, and the audiobooks and podcasts pages.
It leaves out dialogs, whose lists are capped at a few rows, and Lyrics and
Settings, which are short.

## Open questions

- The Mac and web clients keep their platforms' own scrollbars, which have
  no letter bubble. Add one if the owner misses it there.
