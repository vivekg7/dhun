// Podcasts and audiobooks (plan 031), the phone's and the Mac's SpokenTest, so the clients keep the same rules.

import { test } from "node:test";
import assert from "node:assert/strict";
import { Catalog, Heard, song, day } from "../js/catalog.js";
import { nextChapter, previousChapter } from "../js/queue.js";

const s = (id, path, { kind = "", group = "", index = 0 } = {}) =>
  song({
    id,
    path,
    title: path.slice(path.lastIndexOf("/") + 1),
    artist: "Host",
    artists: ["Host"],
    album: "Show",
    genres: ["Podcast"],
    durationMs: 600_000,
    kind: kind || undefined,
    group,
    groupTitle: group.toUpperCase(),
    groupIndex: index,
  });

// Each collection's paths are relative to its own folder, so an episode's
// folder can be named like a music folder; it must still stay out of every
// music list.
test("episodes stay out of music, but queues find them", () => {
  const c = new Catalog([
    s(1, "Lex/song.mp3"),
    s(2, "Lex/002.opus", { kind: "podcast", group: "lex", index: 1 }),
    s(3, "Lex/001.opus", { kind: "podcast", group: "lex", index: 0 }),
  ]);
  assert.deepEqual(
    c.songs.map((x) => x.id),
    [1],
  );
  assert.deepEqual(
    c.folders.get("Lex").songs.map((x) => x.id),
    [1],
  );
  assert.ok(c.artists.every((g) => g.songs.every((x) => !x.spoken)));
  assert.deepEqual(
    c.shows.map((x) => x.items.map((y) => y.id)),
    [[3, 2]],
  );
  assert.ok(c.byId.has(2));
  assert.equal(c.shelfOf(c.byId.get(2)).title, "LEX");
  // A list holding an episode is filtered over its own songs, not the music's.
  assert.deepEqual(
    c.filter([c.byId.get(2), c.byId.get(1)], "002").map((x) => x.id),
    [2],
  );
});

test("a book carries on from its first file not played", () => {
  const c = new Catalog(
    [1, 2, 3].map((i) =>
      s(i, `Dune/${i}.mp3`, { kind: "audiobook", group: "dune", index: i }),
    ),
  );
  const book = c.books[0];
  const heard = new Heard(new Set([1]), new Map([[2, 300_000]]));
  assert.equal(heard.next(book).id, 2);
  assert.equal(heard.progress(book), "50% · 15 min left");
  assert.ok(heard.begun(book));
  const done = new Heard(new Set([1, 2, 3]), new Map());
  assert.equal(done.next(book).id, 1);
  assert.ok(!done.begun(book), "finished, it starts again: Play, not Continue");
});

// Some podcast tools write the date tag without dashes.
test("a compact date reads as a day", () => {
  const d = (date) => day(song({ id: 1, path: "x.mp3", date }));
  assert.equal(d("20251230"), "2025-12-30");
  assert.equal(d("2025-12-30T08:00:00Z"), "2025-12-30");
  assert.equal(d("2025"), "2025");
});

test("Previous goes to this chapter, then the one before", () => {
  const starts = [0, 60_000, 120_000];
  assert.equal(previousChapter(starts, 90_000, 3000), 60_000);
  assert.equal(previousChapter(starts, 61_000, 3000), 0);
  assert.equal(
    previousChapter(starts, 1000, 3000),
    null,
    "the first chapter goes to the previous file",
  );
  assert.equal(previousChapter([], 90_000, 3000), null);
  // Just sought to a chapter, a little before its start: Next goes to the one after.
  assert.equal(nextChapter(starts, 59_900), 120_000);
  assert.equal(
    nextChapter(starts, 130_000),
    null,
    "the last chapter goes on to the next file",
  );
});

test("Previous never restarts a long file", () => {
  // One mistaken press restarted a book half heard, and its place was lost.
  assert.equal(
    previousChapter([0], 5 * 3_600_000, Infinity),
    null,
    "no chapters: the previous file",
  );
  assert.equal(
    previousChapter([0, 60_000], 90_000, Infinity),
    0,
    "chapters: the one before, not this one's start",
  );
  assert.equal(previousChapter([0], 90_000, 3000), 0, "a song still restarts");
});
