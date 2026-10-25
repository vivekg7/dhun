# 029 — Search that forgives spelling, scripts and word order

**Status:** `RELEASED` — in the 0.13.0 GitHub release (2026-10-12); Mac 0.12.0 and phone 0.12.1 on the owner's devices; server side in `server-v0.12.0` on the NAS (2026-10-10)
**Started:** 2026-10-10

## Problem

Every search box looked for the query as one substring of the title, the
artist or the album, ignoring case and accents. That fails in the ways
this family searches most:

- **Words in different fields or out of order.** "arijit tum" found
  nothing, because no single field holds both words.
- **Romanised spelling is not fixed.** The library is largely Hindi and
  Punjabi film music, and its tags write the same word several ways:
  Kabhi and Kabhie, Ishq and Ishk, Main and Mein, Pyar and Pyaar.
- **Two scripts.** Some songs are tagged in Devanagari, so "dil" never
  found "दिल".
- **Classical titles in other alphabets.** "Für Elise" folded to "fur",
  but "Fuer Elise" did not, nor did Tchaikovsky's other spellings.
- **Typos.** "arjit" found nothing.
- **Only songs.** The Search tab could not find an artist, an album or a
  folder as such, and did not remember earlier searches, as Musicolet
  does. It did not search the composer, the genre or lyrics.

The owner asked for all of it (2026-10-10).

## Decision

### One way of matching, everywhere

`data/Search.kt` holds the folding, the query and an index, and every box
uses them: the Search tab, the tabs' top boxes, the bottom boxes of folders
and song pages, Downloads and the queue. A box that matched differently
from the next would be a bug report waiting to happen.

**Folding** turns text into lower-case words:

1. Devanagari is written in Latin letters, with the inherent _a_ dropped
   where Hindi drops it: at the end of a word, and in the middle where a
   vowel comes before and a consonant with a vowel after (धड़कन is
   _dhadkan_, not _dharakana_). The words are worked from the right, so
   each decision sees the one after it.
2. Letters with no decomposition are spelt out (ß ss, æ ae, ø o, ł l),
   then accents are dropped.
3. Apostrophes join ("don't" is _dont_), "&" is the word _and_, and any
   other punctuation splits words ("AC/DC" is _ac dc_).

**A sound key** for each word then evens out romanised spelling: _ph_ is
_f_, _w_ is _v_, _q_ is _k_, _z_ is _j_, a _c_ that is not _ch_ is _k_;
the _h_ of _kh gh th dh bh jh_ goes; _ee_ is _i_, _oo_ is _u_, _ie_ is
_i_, _ai ei ay ey_ are _e_, _ue_ is _u_ and _oe_ is _o_ (German), _y_ is
_i_; doubled letters are single, and a final _h_ after a vowel goes. So
Kabhie and Kabhi are both _kabi_, Main and Mein _men_, Pyaar and प्यार
_piar_, Fuer and Für _fur_, Tschaikowsky and Tchaikovsky _chekovski_.

### Matching and ranking

The query is split into words, and **every word must match somewhere in
the item**, in any field and any order. A word matches a word of the item
in one of five ways, best first:

| Quality | How                                                   |
| ------- | ----------------------------------------------------- |
| 0       | the same word                                         |
| 1       | the start of the word                                 |
| 2       | inside the word, or inside the field run together     |
| 3       | the start of the word's sound key                     |
| 4       | a typo: one edit in 4–7 letters, two from 8, by sound |

"Run together" is how "acdc" finds AC/DC. **Typos are tried only for a
query word that matches nothing better anywhere in the list**: a typo
usually makes a word that matches nothing, while "love" matching "live",
"lose" and "move" would bury the songs actually called love.

Songs are searched in their title, artist and featured artists, album,
album artist, composer, genre and year. An item's score adds up each query
word's best quality, times ten, plus the field's weight: the title 0, the
artists 1, the album 2, the album artist and composer 3, the genre and
year 4. A title that is the whole query, or starts with it, comes first.
Among equal scores, favourites and songs played often come first; they
never jump a better match.

Tab lists and the bottom boxes keep their own order and only filter;
only the Search tab ranks.

### Speed: an index, built once

Folding all 7,000 songs' fields on every keystroke was most of the old
search's cost. The catalog now builds an index, rebuilt with it when the
library changes: each distinct word once with its sound key, and each
song as the word numbers of its fields. A query compares its words with
the distinct words (about twenty thousand) once, and then each song is a
few array lookups. The pages build a small index of their own list when
first searched. The index was built at the first keystroke;
[036](036_fast_at_100k.md) builds it right after the catalogue, off the
main thread, and slims it to what is read.

### The Search tab finds more than songs

Results come in sections: **Artists, Albums, Genres, Folders, Playlists,
Songs and In lyrics**, ordered by their best match, so "arijit" shows
Artists first and "tum hi ho" Songs first; In lyrics always comes last.
Each section but Songs shows three results and then "Show all", since one
strong song match should not sit under twenty albums. A tapped artist,
album, genre, folder or playlist opens in the Search tab's own stack, so
Back returns to the results. A song plays the songs found, up to the
first 300.

**Chips jump to a section** (owner, 2026-10-18). Below the search box, a
row of chips names the sections found in their order ("Artists",
"Albums" … "Songs", "Lyrics"), and a tap scrolls to the section's heading,
so the songs under twenty albums, or the lyrics under 300 songs, are one
tap away. The row shows only with two sections or more, as with one there
is nowhere to jump. It stays in view under the search box while the
results scroll, so a second jump needs no scrolling back, and the chip of
the section at the top is selected, saying where you are. Names only, no
counts: "Show all" already says how many. "Lyrics" joins the row once its
results arrive. On the Mac and the web the same row sits above the
results.

### Filters

`title:` `artist:` `album:` `composer:` `genre:` `year:` `folder:` and
`lyrics:` narrow a word to that field. `artist:"arijit singh"` takes
several words, and `year:2010-2015` takes a range. A filter applies to
the sections as well: `artist:arijit` also lists Arijit's albums, and no
folders. `lyrics:` searches only the lyrics. The filters are for whoever
wants them; the box never needs them.

### Recent searches

When the box is empty, the last ten searches show, each with a ✕, plus
"Clear". A search is kept when something it found is tapped, not on every
keystroke, as a half-typed "arij" is not history. It stays on the phone,
not synced: Musicolet keeps it per device too, and a family member's
searches are their own phone's business.

### In lyrics: the server, or what the phone kept

Lyrics live on the NAS (`.lrc` files and tags, [015](015_lyrics.md)); the
phone keeps only those of downloaded songs and of songs whose lyrics were
shown. So **lyrics search is the server's**: `GET /api/v1/search/lyrics?q=`
returns songs and the line that matched, up to 50. The app asks 400 ms
after typing stops, for a query of 3 letters or more. A line matches when
every query word matches a word of it by the first four qualities. Typos
are left out, as among thousands of lyric lines a typo would match many.
Lines that hold the whole query as typed come first.

**Offline, or on a server too old for the endpoint,** the app searches the
lyrics it has kept, by the same rules. That is the written reason this
part of search needs the server: the lyrics themselves do.

The server keeps every song's lyrics lines in memory, a few megabytes for
this library, refreshed after each scan. A song is read again only when
its row changed or its `.lrc` file's size or time did, so a refresh costs
one directory listing per folder. The first search after a start waits
for the first refresh.

Each language has its own copy of the folding, about eighty lines: Go,
Kotlin and Swift here, then JavaScript for the web
([030](030_web_client.md)) and Rust for Linux ([039](039_linux_app.md)).
`api/search-fold.tsv` lists inputs with their expected words and keys, and
**every copy's tests read it**, so the copies cannot drift apart.

### On the Mac

The Mac app has the same search (owner, 2026-10-10) in
`DhunKit/Data/Search.swift`, a line-for-line port with the phone's tests.
Where the window differs from the phone's tabs:

- **The window's search field is the Search tab**, with the same sections,
  three each and "Show all". A click opens an artist, album, genre, folder
  or playlist over the results, and Back returns to them. Double-click or
  Return plays a song, as in every Mac list, and the right click has the
  song menu. Typing again shows the results over any page opened from
  them.
- **Recent searches are the field's suggestions** when it is empty, as in
  Music and Safari, with "Clear Recent Searches", and are this Mac's own.
- **The filters sit in the toolbar**, as the Mac's tab filters already
  did, not at the foot of the page: a text field in the page would take
  the focus, and Space must play and pause. Pages get one past 12 rows, as
  on the phone. The queue keeps its own field.
- **A page's songs are filtered through the catalog's index**: the songs
  the query finds in the whole library, kept in the page's order. An index
  per page on each keystroke would cost more than it saves. The one
  difference is that "matches nothing better" for typos is judged over the
  library, not the page.
- **Words are compared as bytes** (`memcmp`, `memmem`). With Swift's own
  string search one keystroke over 7,000 songs took up to 0.2 s, on the
  main thread where the filters run; as bytes it takes 1 to 2 ms. Typos
  still count letters as the phone does.

## Rejected

- **A search library** (Lucene, SQLite FTS on the phone). FTS5 does
  prefixes but not sound keys, Devanagari or typos, which are the point
  here, and Lucene is megabytes. The index above is about two hundred
  lines.
- **Server-side search for everything.** Search must work offline and on
  phone-local songs, and the phone already has the whole library.
- **A full phonetic algorithm** (Soundex, Metaphone). They are built for
  English names and merge too much: Soundex makes _tere_ and _tera_ the
  same. Hindi lyrics depend on those vowels.
- **Typos on every word.** See above: common words would match a crowd.
- **Syncing recent searches.** Nothing would use them on another device.
