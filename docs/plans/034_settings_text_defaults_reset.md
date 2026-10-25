# 034 — Shorter settings text, marked defaults, reset, and which settings to add

**Status:** `RELEASED` — in the 0.15.0 GitHub release (2026-10-18); 0.15.0 on the owner's phone and Mac; server and web in `server-v0.15.0` on the NAS (2026-10-18)
**Started:** 2026-10-18

## Problem

The owner asked for three changes (2026-10-18):

1. **Settings explain too much.** Rows and dialogs say why and when to
   change a setting. Other players, Musicolet among them, keep a setting
   to a short name and a short value. The explanation should go to a
   guide, with an info icon where one is needed.
2. **Defaults are invisible, and nothing resets them.** Once a value is
   changed, nothing shows what it was, and putting a page back means
   remembering every value.
3. **More things could be settings.** Several are hardcoded in the
   clients; whether each is worth a setting is part of this plan.

What we have now:

- **Android** already moved each explanation from the row into its
  dialog ([018](018_settings_layout.md)), but the notes are still
  paragraphs (three sentences for "Previous in one"), titles and choice
  labels are sentences ("Going on after a pause", "Previous file, keep my
  place"), and every synced row ends in "· on all your devices", eleven
  times on one page.
- **macOS and the web** show the explanations on the page itself, as
  section footers and group notes; the one under Long files is six
  sentences.
- **Each default is a literal wherever the setting is read**:
  `longFiles.seekBack`'s 10 appears 13 times across Kotlin, Swift and JS,
  and the server reads the Listen Later defaults too. They agree today, but
  nothing keeps them agreeing, and nothing could mark the default or reset
  to it.

## Options

**Where the explanation goes**

- **A guide page, with an ⓘ on the few settings that need it** (chosen).
  Settings stay one short line each. The guide also has room for what is
  not a setting at all: queues against playlists, Downloads against the
  cache, what follows the user to other devices.
- **Tooltips everywhere.** Android has no hover, and nobody finds a
  long-press tooltip. On the Mac `.help()` is native and costs a line, so
  we use it there as well, but it cannot be the only place the text lives.
- **An ⓘ on every row.** The same clutter, in icon form. Rejected.

**How the three clients get the guide**

- **One file, copied into each app as it is built** (chosen). Works
  offline, and there is one text to keep up to date.
- **Served by the server, opened from the apps.** Fails offline, and
  "a feature that works only online needs a reason"; this one has none.
- **Written three times, once per client.** The three copies would drift.
  Rejected.

**Marking the default**

- **A muted "Default" beside that option in the dialog** (chosen on
  Android). The accent already marks the selected option, so a second
  colour would compete with it.
- On the Mac and the web the option lists are native menus and `<select>`,
  which cannot be styled per item. There, the label ends in "(default)".
- On/off switches have no options to mark. Reset covers them.

**Reset**

- **One "Reset to defaults" per page, after asking** (chosen): in the
  page's ⋮ menu on Android, a "Reset to Defaults…" button at the foot of
  each Settings tab on the Mac (the platform's place for it), and a button
  at the end of each page on the web.
- **One reset for everything**, from the main Settings list. A big action
  to take by mistake, and nobody asked for it. Rejected for now.
- **A reset per row.** Picking the option marked Default does the same.
  Rejected.

**Saying what syncs**

- **One line at the top of Playback:** "These follow you to all your
  devices" (chosen). Every synced setting is on that page and no other
  page has one, so one line says it all. It replaces 018's per-row suffix.
- **Keep the suffix on every row:** eleven repeats of the same words.
  Rejected.

## Decision

The owner decided (2026-10-18):

- **Settings are terse:** a short title and its value. A dialog note
  stays only where the title and the options could be misread, and is
  one line. Everything else moves to a **How it works** page, the last
  row of the Settings list on every client.
- **The default option is marked** in every list of options.
- **Every settings page with preferences can be reset to its defaults.**
- **At most 20 queues stays fixed.** A setting would add nothing, and a
  list longer than twenty is hard to scroll anyway. This confirms the
  product rule in `AGENTS.md` and [020](020_queue_screen.md).

The owner also accepted the wording table, the sync line and the verdicts
on new settings below, and said no to the three settings left as
questions: when a queue ends Dhun stops, playback does not resume when a
headset connects, and Shuffle shuffles the whole queue.

### The wording

Titles and values. Synced settings keep their names in storage; only the
labels change.

| Before (title → value)                          | Proposed                                                                               | Note kept in the dialog                                     |
| ----------------------------------------------- | -------------------------------------------------------------------------------------- | ----------------------------------------------------------- |
| Previous → Restart the song first               | Previous → Restart song first / Previous song                                          | —                                                           |
| Continue long files → 15 minutes and longer     | Remember position → 15 min and longer                                                  | "For audiobooks, podcasts and mixes."                       |
| Playing one again → Continue where you left it  | When played again → Resume / Ask / Start over                                          | —                                                           |
| Previous in one → Previous file, keep my place  | Previous button → Previous file / Seek back (headset) / Ignore headset / Same as songs | "On the screen, Previous always goes to the previous file." |
| Next in one → Next file                         | Next button → Next file / Seek forward (headset)                                       | —                                                           |
| Seek back / Seek forward → 10 seconds           | Seek back / Seek forward → 10 s                                                        | —                                                           |
| Going on after a pause → Go back 10 seconds     | Rewind on resume → Off / 5 s / 10 s …                                                  | —                                                           |
| A pause counts after → 5 minutes                | After pauses of → 1 min / 5 min …                                                      | —                                                           |
| Remove what you finish · On all your devices    | Remove when finished                                                                   | —                                                           |
| Mini player (note on Floating)                  | Mini player → Bar / Floating / None                                                    | —                                                           |
| Songs on this phone: shorter files are left out | Skip files shorter than → 30 s                                                         | —                                                           |
| Downloads limit (note)                          | Storage limit → 10 GB                                                                  | "At the limit, downloads stop. Nothing is deleted."         |
| Wi-Fi only · Downloads wait for Wi-Fi           | Wi-Fi only                                                                             | —                                                           |

The rows under the Long files heading drop "long files" from their
titles, since the heading says it. The Downloads note stays because it
answers the question people actually have ("will it delete my songs?").
macOS and the web take the same words, and their section footers go.

### How it works

- **One source, `docs/GUIDE.md`**: one section per settings page, plus
  Queues, Downloads and the cache, and what syncs. Written for a family
  member, not a maintainer, and changed in the same commit as the
  behaviour it describes.
- **Each app carries a committed copy**, because Gradle's assets, SwiftPM
  and `go:embed` each read only their own folders: Android's in its
  assets, the web's beside `index.html`, and the Mac's as a Swift string
  (a SwiftPM resource would need a resource bundle copied into the app by
  hand); the Linux app's, since [039](039_linux_app.md), in `linux/app/src`.
  `scripts/guide.sh` writes them from `docs/GUIDE.md`; `make fmt` runs it,
  and `make check` fails when a copy is out of date.
- **Markdown limited to headings, paragraphs, lists and bold.** Each client
  renders that in about twenty lines (on the Mac, `AttributedString` per
  paragraph), with no Markdown library.
- **The ⓘ goes only on settings whose guide section says more than their
  dialog does**: Remember position, Previous button, Next button, the
  cache, and on Android the mini player (its Floating style is not obvious
  from the name). On Android it sits in the dialog's title; on the Mac and
  the web beside the row, with `.help()` or `title` holding the one-line
  note. It opens the guide at that setting's section.

### Defaults and reset

- **One table of synced settings per platform** (`Synced.kt`,
  `Synced.swift`, `synced.js`), listing each key, its default and its
  options. Every read goes through it, with no literals, and the dialog's
  "Default" mark and the reset both come from it. It is written one key to
  a line, so `scripts/check-settings.sh`, in `make check`, can compare the
  keys and defaults across Kotlin, Swift, JS and the server's
  `readSetting` calls: where they disagree, the same user's phone and Mac
  behave differently.
- **Resetting a synced setting writes `null`.** The server's `readSetting`
  and both apps already treat `null` as missing, so the server needs no
  change. Because the key is unset, not pinned to today's value, a later
  change of default reaches it. A device-local setting removes its stored
  key.
- **What resets per page**:
  - Appearance: theme, accent and mini player.
  - Playback: every synced setting there.
  - Songs on this phone: the switch and the minimum length.
  - Downloads: both limits and Wi-Fi only.

  Account, Family and About hold no preferences, so they have no reset.

- **The confirmation says what happens.** For Playback: "Reset Playback
  settings? They follow you, so all your devices change." For Downloads:
  "Nothing downloaded is deleted." Bringing a larger cache back to 3 GB
  lets it drop the songs played longest ago, which is what a cache does.

### Which settings to add

A setting is worth adding only if it passes all three tests:

- A family member would really choose differently from the default. A
  tuning knob only a developer would touch doesn't count.
- It isn't already a decision the owner made.
- It's worth building three times.

**Hardcoded values: none become settings.**

| Value                                                                              | Where                          | Verdict                                                                                         |
| ---------------------------------------------------------------------------------- | ------------------------------ | ----------------------------------------------------------------------------------------------- |
| At most 20 queues                                                                  | all clients                    | No: the owner's decision above                                                                  |
| Previous restarts after 3 s                                                        | ExoPlayer's default on Android | No: decided (owner, 2026-10-11); Musicolet doesn't expose it either                             |
| Sleep timer fades over 10 s                                                        | `SleepTimer.kt`, `playback.js` | No: no one picks a fade length                                                                  |
| Speed 0.5×–2×, pitch ±6                                                            | `Tempo`, `queue.js`            | No setting. Widening speed to 3× for podcasts is a separate small change, if wanted             |
| A listen counts at half the song                                                   | server                         | No: one rule keeps the history meaningful for the recommender ([008](008_listening_history.md)) |
| Headset 1/2/3 presses                                                              | Android                        | No: decided (owner, 2026-10-11)                                                                 |
| Sync retry, thumbnail batch, end margin, preload, lyrics follow delay, note length | various                        | No: internals                                                                                   |

**From Musicolet's settings**
([inventory](../research/musicolet-feature-inventory.md)):

| Setting                                                                         | Verdict                                                                                                                                                                                                                                         |
| ------------------------------------------------------------------------------- | ----------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| Sleep timer by the clock: **let the song finish** instead of fading out on time | **Yes.** People choose differently here (music at bedtime vs an audiobook), and "pause where a song ends" already exists for the other timers. It goes in the sleep timer dialog, as Musicolet has it, and is remembered there, not in Settings |
| When a queue ends: **play the next queue**                                      | No (owner, 2026-10-18)                                                                                                                                                                                                                          |
| Resume when Bluetooth or a headset connects                                     | No (owner, 2026-10-18)                                                                                                                                                                                                                          |
| Shuffle only the songs after the current one                                    | No (owner, 2026-10-18)                                                                                                                                                                                                                          |
| Hide or reorder tabs, start tab                                                 | Later: real, but costs the most (the Mac sidebar, the web) and nobody has asked                                                                                                                                                                 |
| Same-name queue: replace, merge or ask                                          | No: refilled in place, decided in 020                                                                                                                                                                                                           |
| Pause on disconnect: off                                                        | No: never the wrong behaviour                                                                                                                                                                                                                   |
| Lyrics text size                                                                | No: follows the system's font size                                                                                                                                                                                                              |
| Equalizer, ReplayGain, crossfade                                                | Not settings but features, already on the Later list in `REQUIREMENTS.md`                                                                                                                                                                       |

So "is adding more customisations worth it" is **mostly no**. Dhun's
hardcoded values are decisions already made or internals, and settings for
them would bring back the clutter point 1 removes. The settings worth
having are Musicolet behaviours we lack, and of those the owner wants only
the sleep timer's.

## Plan

In this order, because the table makes the rest cheap:

1. **The settings table on each platform**, the drift check in
   `make check`, and every read site moved onto the table, with no change
   for the user. **Can fail** if a read site is missed; grepping for each
   key in quotes finds them all.
2. **The "Default" mark and Reset to defaults**, on Android, the Mac and
   the web.
3. **The wording pass and the sync line**, on all three, with 018's
   "Saying what syncs" amended.
4. **`docs/GUIDE.md`, its copies, the renderer and the ⓘ links.**
5. **The sleep timer's "Let the song finish"**, on all three. When the
   time is up, the timer turns into "after this song", so the song ends
   on its own and nothing fades.

## Open questions

None: the three this plan asked were answered no (owner, 2026-10-18).
