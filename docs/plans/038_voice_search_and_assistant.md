# 038 — Voice search, "play it on Dhun", and shortcuts on the icon

**Status:** `RELEASED` — in the 0.16.2 GitHub release (2026-10-21); Android only; not yet installed
**Started:** 2026-10-21

## Problem

Every search in the app is typed. On a phone that is the slow way to ask
for a song whose spelling nobody agrees on, and no way at all while
driving or cooking. Android now recognises speech on the device, well and
in Hindi, and the owner wants to use it (2026-10-21):

- a **mic in the search box**, so a song, an artist or a line of lyrics
  can be said instead of typed;
- **"Hey Google, play Kishore Kumar on Dhun"** from the phone's assistant,
  with the app closed, so it opens and plays;
- and whatever else comes cheap in the same vein.

The constraint is the one the whole project has: **lightweight first**. If
any of this makes the app heavy, none of it ships.

## What it costs

Nothing new is bundled. Speech recognition is a system service, the
assistant talks to the media session the app already has, and app
shortcuts are an XML file. The release APK grows by a few kilobytes on
4.9 MB, and no dependency is added. A bundled speech model (Vosk,
whisper.cpp) would be 40–100 MB and is rejected before it is weighed.

## Options

**How the app listens**

- **`RecognizerIntent.ACTION_RECOGNIZE_SPEECH`** (chosen): the system shows
  its own mic sheet, recognises, and hands back the text. No permission of
  our own (the recogniser holds it), no UI to draw, some twenty lines.
  With `EXTRA_PREFER_OFFLINE` it uses the on-device model where there is
  one, and the online one on an Android 12 phone that has none, so every
  phone we support gets the same button.
- **`SpeechRecognizer.createOnDeviceSpeechRecognizer`** (API 31, our
  minimum): our own mic UI with partial results typed live into the box.
  Needs `RECORD_AUDIO` and an availability check, and gains a nicer
  animation. Rejected for now: the sheet is the system's and the listener
  knows it; revisit if the sheet annoys.
- **Say it, play it.** Rejected: the recognised text goes into the search
  box, and the listener picks from what it finds, as with a typed search.
  A misheard word then costs a glance, not a wrong song.

Hindi needs nothing extra: a transcription in Devanagari folds to Latin
letters in search already ([029](029_search.md)), so "कभी कभी" finds the
romanised "Kabhi Kabhie". The recogniser uses the phone's language; a
language picker of our own is not worth a row until someone asks.

**How the assistant reaches the app**

Android's documented path for "play X on app" has two halves, and the app
needs both:

- **Closed app**: the assistant launches the activity with
  `android.media.action.MEDIA_PLAY_FROM_SEARCH`, a query, and hints for
  what kind of thing was named (artist, album, title, genre, playlist).
- **Running app**: the assistant sends the session's `playFromSearch`,
  which Media3 delivers to `MediaSession.Callback.onSetMediaItems` as a
  `MediaItem` whose request metadata carries the search query and extras.

Both resolve the request against the catalogue on the phone, so they work
offline, and both end in `Playback.play`, a **new queue named after what
was asked** ("Kishore Kumar", "Aashiqui 2"), never a change to the current
one (AGENTS.md: queues are not playlists). An empty query ("play
something on Dhun") resumes what was playing, or shuffles the library when
nothing was.

Media3 would normally take the items the callback returns and set them on
the player itself. That would bypass the queues. The callback starts the
queue through `Playback` and returns a failed future: the legacy stub
treats that as "the session chose to ignore the request" and touches
nothing, which is exactly right.

- **Play-published App Actions** (`shortcuts.xml` capabilities). Rejected:
  they need a Play Console listing to deploy, and Dhun is sideloaded.
- **A `MediaLibraryService` with a browse tree** (Android Auto). Not now:
  more code than everything in this plan together, and still on the
  deferred list in REQUIREMENTS. The voice half built here is reused by
  it unchanged.

**The uncertainty.** Google's own Assistant delivers both halves, and the
Android docs still describe them. Gemini, which replaced it as the default
on new phones, documents "play on app" only for Spotify, Pandora and
iHeartRadio, and nothing says whether it forwards to other apps that
declare the intent. There is no third-party way around that. The plan is
built and then tried on the owner's phone; the outcome goes in "What
building it taught" either way.

**What else is cheap**

- **App shortcuts** (chosen): long-press the icon for Search, Resume and
  Shuffle all, as Musicolet offers. A static XML file and three small
  icons. Each is an action on the main activity, handled with the
  assistant's intent by the same code.
- Pause, next, previous and "what's playing" by voice already work through
  the session. Nothing to do.
- **The headset button with no player running** (chosen; owner,
  2026-10-21): it opened Musicolet, not Dhun, however much more Dhun was
  used. Android sends the press to the receiver of the last media session
  that registered one, and Media3 registers one only when the app
  declares its `MediaButtonReceiver`; Dhun had none, so the remembered
  receiver stayed Musicolet's. Declaring it, Dhun becomes the remembered
  app whenever it played last. The receiver starts the service and Media3
  asks `onPlaybackResumption` for something to play; Playback restores
  the last queue on its own, so the callback waits for that and presses
  Play through the same `resume` as the shortcut, and fails its future so
  Media3 sets nothing itself.
- **Mac**: dictation already types into the search field (fn fn), so a mic
  of our own adds nothing. Siri App Intents could do "play X in Dhun"
  later; a plan of its own if wanted.
- **Web**: the browser's speech API is Chrome-only and cloud-backed on a
  desktop. Not worth a button that works for one browser.

## Decision

Android only. A **mic in the Search tab's box** fires the system
recogniser and puts the text in the box. The activity declares
**`MEDIA_PLAY_FROM_SEARCH`** and the session callback handles
**`playFromSearch`**; both resolve the request against the catalogue and
play a new queue named after it. **Three app shortcuts**: Search (the box
focused), Resume, Shuffle all. A **`MediaButtonReceiver`** in the
manifest, so the headset button with no player running starts Dhun when
it played last. Requests from outside the app funnel
through one small class, `play/Intents.kt`, so the activity, the session
and the shortcuts share the resolution and the "show Now playing" step.

How a spoken request is resolved, in order:

1. With a focus hint from the assistant, the named field: the artist's
   songs, the album's, the genre's, the playlist's, or the songs matching
   the title.
2. Otherwise the query is searched the way the Search tab does, section
   by section (artists, albums, genres, folders, playlists, books, shows,
   episodes, songs) and the best-scoring section wins: an artist name
   plays that artist; a title plays the songs found, best first.
3. Nothing found: a toast says so, and nothing changes.

## Plan

- [x] `Icons.Mic`; the mic as the Search box's trailing button, shown only
      when a recogniser is installed (`<queries>` for the intent).
- [x] `play/Intents.kt`: the request funnel, the resolver, the channel that
      tells the shell which tab to show; a unit test for the resolver.
- [x] Manifest: the `MEDIA_PLAY_FROM_SEARCH` filter, the shortcuts
      meta-data, `res/xml/shortcuts.xml` and its icons; `MainActivity`
      hands its intents to the funnel; `Shell` follows the channel.
- [x] `PlaybackService`: `onSetMediaItems` for a request with a search
      query; `onPlaybackResumption` and the `MediaButtonReceiver` for the
      headset button with no player running.
- [x] Docs: REQUIREMENTS, the guide, the index.
- [x] On the emulator (Android 17, the test account's library): the
      intent with the app closed and running, hinted and plain; the empty
      query; a query matching nothing; the three shortcuts. The mic shows,
      but tapping it hangs the emulator itself (QEMU stops answering the
      moment the recogniser opens the microphone), so the sheet is for the
      phone. The session half cannot be sent from `adb`; it rests on the
      Media3 source read below until the phone test.
- [ ] Try on the owner's phone: the mic in Hindi and English; "Hey Google,
      play … on Dhun" with the app closed and running; the three
      shortcuts; the earphone button with Dhun closed, after Dhun played
      last.

## What building it taught

- **A shortcut's intent names no package.** `shortcuts.xml` wants
  `targetPackage` written in, and a debug build's id ends in `.debug`. A
  string resource there is not resolved: the system parses the file
  against its own resources, and the shortcut came out aimed at
  `@2131296260`. So each shortcut carries only its action, and the
  activity has an intent filter for the three. That is also one less
  place the package name is spelt.
- **Media3 does leave the player alone.** The legacy stub's handler for
  the callback's future says, on failure, "the session is free to ignore
  these requests", and does nothing: a queue started through `Playback`
  from `onSetMediaItems` plays on untouched.
- **The headset button, from a dead process, on the emulator**: the
  system log shows the press sent to Dhun's receiver, the process
  started, the last queue restored at its position and playing 2.3 s
  after the press, with the launcher still in front. Media3 logs the
  failed resumption future as an error with a stack trace ("Dhun restored
  its own queue"); it is the chosen path, not a fault. Returning the
  items instead would have Media3 set them on a player that already holds
  them, and Playback's listeners would see a second load.
- **Measured on the emulator**: "Arijit Singh" with the app closed played
  the artist's songs 9 s after the intent, most of it the cold start and
  the catalogue load; "kabhi kabhi" plain picked the song over the two
  albums and the artist with that word, as the Search tab's order does;
  "beethoven xyzzy" changed nothing. (A first run seemed to play
  Beethoven for it: `adb shell` had split the query at the space, and
  the app had been asked for "beethoven".)

## Open questions

- Whether Gemini forwards "play X on Dhun" at all (see above).
- An in-app recogniser with live partial results, if the system sheet
  proves annoying.
- Android Auto, which would reuse the session half of this plan.
