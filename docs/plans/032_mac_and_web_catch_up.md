# 032 — Mac and web catch up with the phone

**Status:** `RELEASED` — in the 0.14.0 GitHub release (2026-10-15); not yet installed or deployed
**Started:** 2026-10-15

## Problem

On 2026-10-15 the owner found five things wrong or missing on the phone.
Four were fixed on Android alone; the fifth, raw captions in place of an
episode's transcript, was fixed on the server and so for every app. The
Mac and web apps still lack the other four:

1. **Queue search filters instead of finding.** Musicolet's queue search
   drops down the matches, and a click scrolls the queue to that song
   ([020](020_queue_screen.md#changed-after-release-2026-10-15)). The Mac
   (`Queues.swift`, a `TextField("Filter")`) and the web (`pages.js`,
   `filterBox("Filter queue")`) both narrow the list to the matches, and
   block dragging while they do. Neither is broken, but neither is what the
   owner asked for.
2. **Folders leaves out podcasts and audiobooks.** Both build the folder
   tree from music only: `Catalog.folders(songs)` in `Catalog.swift` and
   `folders(this.songs)` in `catalog.js`
   ([031](031_podcasts_and_audiobooks.md#changed-after-release-2026-10-15)).
3. **Now playing does not link to the artist or album.** Both show
   "artist · queue name" as plain text, the Mac in the bar's subtitle
   (`NowPlaying.swift`), the web in `nowTitle()` (`player.js`). Neither
   shows the album at all. Only a double click on the song goes to its
   album.
4. **No progress under a long file in a list.** The phone draws a thin bar
   under any song with a resume point
   ([009](009_resume_long_files.md#progress-in-lists-owner-2026-10-15)). The
   Mac's song table and queue rows, and the web's `SongList` rows, show
   only the length.

## Options

### Queue search

- **Keep the filter.** Rejected: the owner asked for Musicolet's way, and
  on every app; a filter that hides the queue's neighbours is also what
  made dragging wait.
- **The toolbar's search field, with suggestions.** Rejected: the Mac's
  toolbar field searches the whole library
  ([029](029_search.md)), and "the queue keeps its own field".
- **The queue's own field, with a list of matches under it** (number,
  title, artist). A click, or Return on the first match, scrolls the queue
  to that song in the middle of the list, selects it, and clears the
  field. The list always shows the whole queue. **Chosen.**

### Links on Now playing

- **One line of text with links in it**: on the Mac an `AttributedString`
  whose links use our own scheme (`dhun://artist/<name>`), opened through
  the view's `openURL` action rather than by the system; on the web,
  `<a>` elements whose click calls `ctx.nav`. The line still truncates as
  one line. **Chosen.**
- **A row of buttons.** Rejected: each button truncates on its own, and a
  long artist name pushes the album out of a 250-point bar entirely.

### Progress in lists

- **Under the title, in the title cell.** The Mac's song table has
  one-line rows, so the bar is drawn along the bottom of the title cell;
  the queue rows and the web's rows have a second line and take it there,
  as the phone does. **Chosen.**
- **A Progress column.** Rejected: it would be empty for nearly every
  song.

## Decision

The phone's behaviour, in each app's own idiom, with the same thresholds:

- **Queue search:** as above. On the web, ↑ and ↓ move through the
  matches, Return picks, Esc closes; on the Mac, ↓ moves into the matches.
  Dragging works while the field has text. The matched row is selected
  after the scroll, so it is clear which one it was.
- **Folders:** a **Podcasts** and an **Audiobooks** root after the music
  folders, their paths prefixed with the root's name, as on the phone.
  Folder play, download (Mac) and filtering go through the folder map and
  need nothing else.
- **Now playing shows "artist · album"** (owner, 2026-10-15), as the
  phone does, in place of "artist · queue name". The sidebar and the
  queue panel already name the playing queue; the web's full-screen
  player names it above the cover, as the phone does.
- **Now playing links:** each artist links to the Artists page, the album to the
  album, an episode's show or a book's title to its page. On the web, in
  the bar and the full-screen player at phone width; the Picture in
  Picture mini player stays plain text, since a click there would have to
  reach another window. A link's click does not reach the double-click
  that goes to the album.
- **Progress:** a 3-point bar in the accent colour, for a resume point
  past the first 1% of the file, from the resume points each app already
  holds (`app.heard` on the Mac, `app.resumeMap` on the web). Not on Now
  playing, which has its seek bar.

## Plan

One commit per item and app, in this order:

1. **Web**, first, because it ships with the server, which needs a
   release anyway for the captions fix (`c9d97c2`, `af7f02b`):
   1. Folders, with a case in `test/library.test.js`.
   2. Progress in `SongList` rows.
   3. Links in `nowTitle()`. Built as `<a href="#/artist/…">`, so
      `nav.artist` needed no change, and a link opens in a new tab like
      any other.
   4. The queue's find field. **Can fail:** the list is virtual
      (`list.list.scrollTo`), so a row scrolled to may not exist until it
      is drawn; select it after the scroll, by index, not by element.
      Built so: the matches are a fixed layer on the page, placed under
      the field, so the side panel's list cannot clip them, and at least
      440 pixels wide, since the panel's field is narrower than a title.
   5. Checked in Chrome on the test account (`local/test_account`): the
      Lex Fridman queue, a podcast folder, a song's links.
2. **Mac**:
   1. Folders, with a case in `LibraryTests.swift`.
   2. Progress in the song table and the queue rows.
   3. Links in the bar's subtitle. **Can fail:** a `Text` with links
      inside a view that has a double-tap gesture may swallow the single
      click; if so, the gesture moves to the cover alone. It did not: a
      click on a link opens it. But a link to another section lost its
      page, as Go to Artist always had: the sidebar's selection, following
      the section `nav.open` set, opened the section again and emptied
      the page. The sidebar now opens only a section other than the one
      showing. In the 250-point bar a long list of artists leaves the
      album cut off; the double-click still goes there.
   4. The queue's find field, through the `ScrollViewReader` already
      there (`proxy.scrollTo(id, anchor: .center)`). Built with the
      matches in a short list between the field and the queue rather
      than a popover, which would take the keyboard from the field; ↑ and
      ↓ move through them from the field, as on the web. The matches are
      a view of their own: built in a method of the queue view they
      stopped updating after the first letter. The scroll waits for the
      next turn of the run loop, after the matches have closed; in the
      same update the list did not move.
   5. Run on the test account, the same three checks, from a copy of the
      app with its own bundle id and home folder, so the owner's install
      and data were not touched.
3. **Release** every platform together as **0.14.0**: the Android fixes
   already on `main`, these, and the server's captions fixes. They add
   behaviour to every app, which makes it a minor release.

## Open questions

None. The queue name leaves the Mac's and web's Now playing bar for the
album (owner, 2026-10-15), under Decision.
