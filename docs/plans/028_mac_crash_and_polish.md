# 028 — The Mac app's layout crash, motion and polish

**Status:** `MERGED` — in 0.2.0 on the owner's Mac (2026-10-09)
**Started:** 2026-10-09

## Problem

The owner still saw the Mac app crash "in some very specific situations"
after 024's fixes, and asked for the polish the phone had just had (027):
motion, and the same look for the same thing.

- **The crash.** Nineteen reports from 2026-10-08, one of them in the
  installed 0.1.0, all aborts from AppKit: "The window has been marked as
  needing another Update Constraints in Window pass, but it has already had
  more Update Constraints in Window passes than there are views in the
  window." Driving the app showed it: with the queue or lyrics open on the
  right, opening a playlist aborted every time, and walking through the
  sidebar aborted on one page or another. It needs SwiftUI's `.inspector`
  beside a `NavigationSplitView` whose page changes: with the inspector
  holding only a line of text, and with no toolbar items, banners or Now
  playing bar, it still aborted every run; with the inspector closed it
  never did. Queues side by side with the inspector (024) was one case of
  it, not its own cause.
- **Little moved.** A new song, play and pause, Favorites, the banners and
  the panel on the right all cut rather than moved.
- **Things alike looked different.** The panel's toggle in the toolbar
  shared a capsule with the filter field on Albums, Artists and Genres, so
  it sat somewhere else on those pages than on the rest. The sidebar's
  icons were the system's blue in an orange app. An empty list showed Play
  and Shuffle over nothing. One Settings tab had no heading; the song
  cache had none either.

## Options

For the crash:

- **Pin the column sizes** (minimum widths and heights on each column).
  Rejected: fewer crashes, not none.
- **Keep `.inspector`, close it while some pages are open.** Rejected: the
  crash came on ordinary pages too.
- **The panel inside the detail column.** Rejected: a page pushed onto the
  stack replaces the whole column, panel and all.
- **Our own panel beside the split view** _(chosen)_.

For the toolbar:

- **The filter in the page**, as a queue's page has it. Rejected: the
  window's first text field takes the focus when it opens, and Space then
  types instead of playing; default focus elsewhere does not win against
  it.
- **The filter beside the title.** Rejected: it then comes before the
  page's name.
- **No panel toggle in the toolbar** _(chosen)_: the queue and lyrics
  buttons in Now playing open and close the panel, as on the phone, and so
  do ⌥⌘U and ⌥⌘L. A toolbar spacer, other placements and hiding the shared
  background did not keep the toggle out of the filter's capsule.

## Decision

Decided while building; the owner may override any of it.

- **The panel** (`MainView`): the queue or the lyrics, 320 points wide,
  beside the split view and below the toolbar; it slides in from the
  right. The window's minimum width is 1000 points while it is open and
  820 while it is shut, and the window widens to fit it rather than
  squeezing the list until its buttons are cut. The panel is no longer
  dragged wider or narrower.
- **Now playing:** the next song slides in from the right, the previous
  from the left, as on the phone; the song leaving only fades, quickly,
  since SwiftUI keeps the transition a view was last drawn with and a
  slide of its own would go the way the change before went. Play and
  Pause, repeat and the sleep timer turn into each other. Favorites and
  Listen Later fill and bounce when turned on, and only empty when turned
  off; a song shown that has one on already does not bounce.
- **Covers** fade in over the thumbnail, and a cover already in memory is
  shown in the first frame, so a grid scrolled back does not flash.
- **The banners** (hand-off, "continue from…?", notes) fold in and out.
  Queue and Lyrics fade into each other.
- **The same look for the same thing:** the sidebar's icons in the app's
  colour; Play and Shuffle only when there is something to play, and a
  note on an empty list (Download stays: Favorites kept on this Mac fills
  as songs are added); every Settings section has a heading.

Found while testing, and fixed: **a seek made while paused was lost**.
Only pausing saved the queue's place, so quitting, or another device
taking over, went back to where the song was paused. A seek while paused
now saves the place, as pausing does.

Left over from that testing, and fixed after 0.2.0:

- **The mini window's empty strip.** Its window buttons sat in a title bar
  strip above the player. The window has no title bar now: it is dragged
  by anywhere on it, and a close button shows over the cover while the
  pointer is on it. Hiding only the buttons left the strip, at the bottom.
  A window without a title bar never becomes the key window, so ⌘W does
  not reach it.

Tested by driving the app with real clicks and keys on the owner's Mac: a
walk through every sidebar section, playlists, a queue's page, album and
artist pages and back, with the panel open, crashed every run before and
in none of the runs after; resizing to the smallest window with the panel
open and shut; next and previous; Favorites on and off; reordering the
queue in the panel; Settings, the menu bar control and the mini window.
