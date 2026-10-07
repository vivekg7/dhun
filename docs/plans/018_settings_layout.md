# 018 — The settings screen on Android

**Status:** `MERGED` — on the owner's phone, in a build still numbered 0.3.0
**Started:** 2026-10-07

## Problem

Settings was one long page. Every choice showed all of its values as chips,
with hints between them, so eight settings filled two screens and read as
clutter. More settings are coming (Musicolet has well over a hundred), and
that page would not hold them.

## Options

**Structure**

- **Categories, then screens** (Musicolet's): a short list of categories,
  each opening a screen of its own. It scales: a new setting goes into the
  category it belongs to, and the first screen stays five rows.
- **One page, grouped**: every setting one row, under group headers. Fewer
  taps today, but it becomes the long page again as settings are added.

**Changing a choice**

- **A row and a dialog** (Musicolet's): the row shows the value chosen;
  tapping it opens a dialog of radio buttons. One row per setting, however
  many values it has.
- **Inline chips** (what we had): one tap to change, but several lines per
  setting, and the cause of the clutter.

**Saying what syncs**

Some settings follow the user to every device (long files, Listen Later;
[009](009_resume_long_files.md)), others belong to the phone (theme,
downloads). Saying so on the row, or not at all.

## Decision

The owner chose (2026-10-07):

- **Categories, then screens:** Appearance, Playback, Downloads, Account and
  About. Each row on the first screen shows what is set where that is
  short ("Dark · Teal", "10 GB · Wi-Fi only", "vivek on gargantua"), so
  most questions are answered without opening anything. Icons appear on
  this list only; inside a category, rows are text, so titles line up.
- **A row and a dialog** for every choice. Picking a value applies it and
  closes the dialog; Cancel leaves it. A note that explains a setting ("At
  the limit, downloads stop. Nothing is deleted to make room.") moves into
  its dialog, off the page. On/off settings are a row with a switch.
- **Synced settings say so:** "· on all your devices" after the value. A
  change that shows up on the laptop is then no surprise. Settings of this
  phone say nothing, as the default.

Details decided while building:

- **Sign out asks first.** It deletes this phone's downloads, and was one
  tap away.
- The accent colour's dialog shows the swatches, not their names in a list:
  a colour is chosen by looking at it.
- The open category survives rotation and Android reclaiming the app, like
  the tabs' pages ([011](011_android_app.md)). Back goes from a category to
  the list, then to the tabs. "Change the limit in Settings", on a full
  Downloads page, opens the Downloads category directly.

## Every dialog the same way

Set as part of this plan, at the owner's request (2026-10-07): the sleep
timer and speed dialogs had grown their own looks (chips, headings indented
twice, sliders with a dot per step, a text box in the middle of the
options). Every dialog now has a title, perhaps one line saying what it
does or what is set, one-line rows flush with the title, and text buttons
below. The pieces are in `ui/Dialogs.kt`; a new dialog uses them rather than
its own layout. Where a value is set rather than picked from a list, the
row holds its control on the right: the sleep timer's steppers
([014](014_sleep_timer.md)), the speed and pitch sliders.

## Adding a setting

Put it in the category it belongs to; add a category only when no existing
one fits and it will hold several settings. A choice is a `ChoiceRow`, an
on/off a `SwitchRow`; pass `synced = true` when it is a synced setting.
Musicolet's grouping (Interface, Audio, Library, Headset, Notifications,
Advanced, Backup) is the guide for where future settings go.

## Open questions

None. A search over settings, as Musicolet has, can follow once there are
enough settings to need it.
