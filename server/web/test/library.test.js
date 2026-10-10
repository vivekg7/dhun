// The phone's and the Mac's library tests (plan 024), so the clients keep the same rules.

import { test } from "node:test";
import assert from "node:assert/strict";
import { Catalog, song, allSongs } from "../js/catalog.js";
import { natural } from "../js/util.js";
import { parseLyrics, lineAt } from "../js/lyrics.js";
import { orderOf, playOrder, nextIn, tempoOf, formatSpeed } from "../js/queue.js";

const s = (
  id,
  path,
  { title = "", album = "", artist = "", albumArtist = "", track = 0 } = {},
) =>
  song({
    id,
    path,
    title: title || `Song ${id}`,
    album,
    artist,
    artists: artist ? [artist] : [],
    albumArtist,
    track,
    durationMs: 180_000,
  });

test("albums with the same name by different artists stay separate", () => {
  const c = new Catalog([
    s(1, "A/Greatest Hits/01.mp3", { album: "Greatest Hits", artist: "Queen" }),
    s(2, "B/Greatest Hits/01.mp3", { album: "Greatest Hits", artist: "ABBA" }),
    s(3, "A/Greatest Hits/02.mp3", { album: "Greatest Hits", artist: "Queen" }),
  ]);
  assert.deepEqual(c.albums.map((a) => a.songs.length).sort(), [1, 2]);
});

test("a compilation tagged with an album artist is one album across folders", () => {
  const c = new Catalog([
    s(1, "x/01.mp3", {
      album: "Rockstar",
      artist: "Mohit",
      albumArtist: "A.R. Rahman",
      track: 1,
    }),
    s(2, "y/02.mp3", {
      album: "Rockstar",
      artist: "Javed",
      albumArtist: "A.R. Rahman",
      track: 2,
    }),
  ]);
  assert.equal(c.albums.length, 1);
  assert.deepEqual(
    c.albums[0].songs.map((x) => x.id),
    [1, 2],
  );
});

test("numbers sort as numbers", () => {
  assert.deepEqual(["Track 10", "track 2", "Track 1"].sort(natural), [
    "Track 1",
    "track 2",
    "Track 10",
  ]);
});

test("folders hold their subfolders' songs", () => {
  const c = new Catalog([s(1, "Hindi/Arijit/01.mp3"), s(2, "Hindi/02.mp3")]);
  assert.deepEqual(
    new Set(allSongs(c.folders.get("Hindi")).map((x) => x.id)),
    new Set([1, 2]),
  );
  assert.deepEqual(
    c.folders.get("").children.map((f) => f.name),
    ["Hindi"],
  );
});

test("missing songs are hidden", () => {
  const c = new Catalog([
    s(1, "a.mp3"),
    song({ id: 2, path: "b.mp3", title: "b", missing: true }),
  ]);
  assert.deepEqual(
    c.songs.map((x) => x.id),
    [1],
  );
});

// Lines as the curation workflow's .lrc files have them: tags, a repeated
// chorus on one line, two- and three-digit fractions, word stamps.
test("synced lines are timed, sorted and clean", () => {
  const p = parseLyrics(
    "[ar:Someone]\n[ti:Saans]\n[00:05.5]First\n[00:10.25][00:30.250]Chorus\n[00:20.00]<00:20.00>Word <00:21.50>stamps\n[00:25.00]",
  );
  assert.deepEqual(p.lines, [
    { ms: 5_500, text: "First" },
    { ms: 10_250, text: "Chorus" },
    { ms: 20_000, text: "Word stamps" },
    { ms: 25_000, text: "" },
    { ms: 30_250, text: "Chorus" },
  ]);
  assert.equal(lineAt(p, 5_000), -1);
  assert.equal(lineAt(p, 19_999), 1);
  assert.equal(lineAt(p, 99_000), 4);
});

test("an offset shows lines earlier", () => {
  assert.deepEqual(parseLyrics("[offset:+500]\n[00:01.00]a\n[00:00.20]b").lines, [
    { ms: 0, text: "b" },
    { ms: 500, text: "a" },
  ]);
});

// Embedded lyrics are often plain; a stray tag must not make them "synced" or show up as a verse.
test("plain lyrics keep their lines but not tags", () => {
  const p = parseLyrics("\n[ar:Someone]\nOne\n\nTwo\n\n");
  assert.equal(p.synced, false);
  assert.deepEqual(
    p.lines.map((l) => l.text),
    ["One", "", "Two"],
  );
  assert.equal(lineAt(p, 10_000), -1);
});

// A browser in German would otherwise show "1,25×".
test("speeds read the same in every locale", () => {
  assert.deepEqual([0.75, 1, 1.25, 1.5, 2].map(formatSpeed), [
    "0.75",
    "1",
    "1.25",
    "1.5",
    "2",
  ]);
});

// A song's setting comes from another device, maybe a newer app: never trusted to be in range.
test("a song's setting is read defensively", () => {
  assert.deepEqual(tempoOf({ speed: 1.5, semitones: -2 }), {
    speed: 1.5,
    semitones: -2,
  });
  assert.deepEqual(tempoOf({ speed: 9, semitones: 40 }), { speed: 2, semitones: 6 });
  assert.deepEqual(tempoOf({ speed: 1.25 }), { speed: 1.25, semitones: 0 });
  assert.equal(tempoOf(1.5), null);
  assert.equal(tempoOf(null), null);
});

// The order is a synced setting: it can name queues deleted elsewhere, miss
// ones made on another device, or come from a newer app in another shape.
test("the order never loses or invents a queue", () => {
  const q = (id, usedAt) => ({ id, usedAt });
  const queues = [q("a", 30), q("b", 10), q("c", 20), q("d", 5)];
  assert.deepEqual(
    orderOf(queues, ["c", "gone", "a", "c", 7]).map((x) => x.id),
    ["c", "a", "d", "b"],
  );
  assert.deepEqual(
    orderOf(queues, null).map((x) => x.id),
    ["d", "b", "c", "a"],
  );
  assert.deepEqual(
    orderOf(queues, "a").map((x) => x.id),
    ["d", "b", "c", "a"],
  );
});

// Shuffled, an edit must not reshuffle what was already ahead: the playing
// song keeps its place and new songs land after it.
test("a shuffled queue keeps its order across edits", () => {
  const items = [1, 2, 3, 4, 5];
  const first = playOrder(items, [], 3, true, false);
  assert.equal(first[0], 3);
  assert.deepEqual([...first].sort(), items);
  const kept = playOrder([...items, 6], first, 3, true, true);
  assert.deepEqual(
    kept.filter((x) => x !== 6),
    first,
  );
  assert.ok(kept.indexOf(6) > kept.indexOf(3));
  assert.deepEqual(playOrder(items, first, 3, false, true), items);
});

test("repeat decides what follows the last song", () => {
  assert.equal(nextIn([1, 2, 3], 3, "off"), null);
  assert.equal(nextIn([1, 2, 3], 3, "queue"), 1);
  assert.equal(nextIn([1, 2, 3], 2, "song"), 2);
  assert.equal(nextIn([1, 2, 3], 9, "off"), null);
});
