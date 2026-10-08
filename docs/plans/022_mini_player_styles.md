# 022 — A choice of mini player on Android

**Status:** `MERGED` — not yet in a release
**Started:** 2026-10-08

## Problem

The slim mini player above the tab row ([011](011_android_app.md)) was a
deliberate departure from Musicolet, so pausing would not need a tab
switch. But it takes a row of every tab, and Musicolet, which the owner
has used for ten years, does without one: Now playing is a tab, and the
notification pauses. One fixed answer does not suit everyone in the
family.

## Options

- **A setting** _(chosen)_: none, the bar, or a floating pill.
- **Keep the bar only.** Rejected: the owner asked for the choice.
- **Floating with several looks**, the owner's first idea (a cover bubble
  with a progress ring, a pill with the title, buttons only). Each look is
  another size to keep on screen, another hit area and another layout to
  test.
- **Two floating looks, tried and merged** (2026-10-08). First the owner's
  Previous / Play / Next pill, then the same pill with a small cover above
  Play/Pause, as a fourth value of the setting. The owner then put the
  cover _in place of_ Play/Pause: between Previous and Next, a tap on it
  reads as play or pause without a label, and one look serves both. A
  small Play/Pause above the cover was rejected: two stacked targets, one
  of them small, and a taller pill.

## Decision

The owner chose the setting and the floating look (2026-10-08):
**Previous, the cover and Next** on a rounded pill, the cover being
Play/Pause, and a **long press on the cover opening Now playing**. The rest
was decided while building, and the owner may override it:

- **In Appearance**, as "Mini player": None, Bar at the bottom (the
  default, so nothing changes for anyone who does not look), Floating. A
  setting of this phone, not synced, like the theme: a tablet and a phone
  may want different ones.
- **None** is Musicolet's way. The hand-off bar
  ([017](017_handoff.md)) keeps its place above the tab row whichever is
  chosen.
- **A cover alone cannot say whether the song plays**, so it shows it:
  paused, the cover is dimmed under a play icon, asking to be pressed;
  playing, it is clear, inside a thin ring that fills with the song's
  progress in the accent colour. Waiting for the network counts as
  playing, as in the bar. A song without a cover shows the note, the same
  way.
- **Dragged anywhere over the tabs**, a drag that starts on a button
  included (it moves the pill instead of pressing it). Where it is left is
  kept as fractions of the room it has, so it stays in its corner on
  rotation and can never end up off the screen. It starts on the right, a
  little up: at the very bottom it covered every tab's search box.
- Like the bar, it hides on Now playing, which has the same controls, and
  while nothing is loaded.

Not done: making room for the pill in the lists. It floats over whatever
is under it, and the owner moves it off what they need.

## Open questions

None.
