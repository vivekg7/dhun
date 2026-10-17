// Podcasts and audiobooks (plan 031), the phone's and the Mac's SpokenTest, so the clients keep the same rules.

import { test } from "node:test";
import assert from "node:assert/strict";
import { Catalog, Heard, song, day } from "../js/catalog.js";
import { chapterAfter, chapterBefore, rewound } from "../js/queue.js";

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
  // Folders shows them under a root of their own, after the music's.
  assert.deepEqual(
    c.folders.get("").children.map((f) => f.path),
    ["Lex", "Podcasts"],
  );
  assert.deepEqual(
    c.folders.get("Podcasts/Lex").songs.map((x) => x.id),
    [3, 2],
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

test("a long press goes by chapter, and never off the file", () => {
  const starts = [0, 60_000, 120_000];
  assert.equal(chapterBefore(starts, 90_000, 3000), 60_000, "this chapter's start");
  assert.equal(chapterBefore(starts, 61_000, 3000), 0, "just into it: the one before");
  assert.equal(
    chapterBefore(starts, 1000, 3000),
    null,
    "the first chapter's start stays",
  );
  assert.equal(chapterBefore([], 90_000, 3000), null);
  // Just sought to a chapter, a little before its start: the one after, not that one again.
  assert.equal(chapterAfter(starts, 59_900), 120_000);
  assert.equal(chapterAfter(starts, 130_000), null, "the last chapter stays");
});

// Stopped mid-sentence in an episode: going on later starts the sentence again, a short pause does not.
test("a long pause goes back a little", () => {
  const minute = 60_000;
  assert.equal(rewound(90_000, 6 * minute, 10_000, 5 * minute), 80_000);
  assert.equal(rewound(90_000, 30_000, 10_000, 5 * minute), 90_000, "a moment's pause");
  assert.equal(
    rewound(4000, 6 * minute, 10_000, 5 * minute),
    0,
    "never before the start",
  );
  assert.equal(rewound(90_000, 6 * minute, 0, 5 * minute), 90_000, "turned off");
});
