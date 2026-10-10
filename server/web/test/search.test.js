// The phone's SearchTest and the Mac's SearchTests, so the three copies match alike (plan 029).

import { test } from "node:test";
import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import { text, words, key, Query, songIndex } from "../js/search.js";
import { song } from "../js/catalog.js";

const s = (id, title, { artist = "", album = "", composer = "", year = 0 } = {}) =>
  song({
    id,
    path: `a/${id}.mp3`,
    title,
    artist,
    artists: artist ? [artist] : [],
    album,
    composer,
    year,
    durationMs: 180_000,
  });

const index = songIndex([
  s(1, "Tum Hi Ho", { artist: "Arijit Singh", album: "Aashiqui 2", year: 2013 }),
  s(2, "Live and Let Die", { artist: "Wings" }),
  s(3, "Love Me Do", { artist: "The Beatles" }),
  s(4, "दिल से", {
    artist: "A.R. Rahman",
    album: "दिल से",
    composer: "A.R. Rahman",
    year: 1998,
  }),
  s(5, "Highway to Hell", { artist: "AC/DC" }),
  s(6, "Kabhie Kabhie", { artist: "Mukesh", year: 1976 }),
  s(7, "Hello", { artist: "Adele", album: "Tum Hi Ho Hits" }),
]);

const find = (q) => index.search(new Query(q)).map((r) => r.item.id);

test("folds as the phone, the Mac and the server do", () => {
  const file = new URL("../../../api/search-fold.tsv", import.meta.url);
  const lines = readFileSync(file, "utf8")
    .split("\n")
    .filter((l) => l && !l.startsWith("#"));
  assert.ok(lines.length > 20);
  for (const line of lines) {
    const [input, folded, keys] = line.split("\t");
    assert.equal(text(input), folded, input);
    assert.equal(words(input).map(key).join(" "), keys, input);
  }
});

test("words match across fields in any order", () => {
  assert.deepEqual(find("arijit tum"), [1]);
  assert.deepEqual(find("ho tum arijit"), [1]);
});

test("the title ranks above the album", () => {
  // Song 7 only has it in its album name.
  assert.deepEqual(find("tum hi ho"), [1, 7]);
});

test("typos only when nothing better matches", () => {
  assert.deepEqual(find("arjit"), [1]);
  // "love" matches a word, so "live" is not offered as a typo.
  assert.deepEqual(find("love"), [3]);
});

test("spelling and scripts meet", () => {
  assert.deepEqual(find("dil se"), [4]);
  assert.deepEqual(find("kabhi"), [6]);
  assert.deepEqual(find("acdc"), [5]);
});

test("filters narrow a word to a field", () => {
  assert.deepEqual(find("composer:rahman"), [4]);
  assert.deepEqual(find("title:arijit"), []);
  assert.deepEqual(find("year:1970-2000").sort(), [4, 6]);
  assert.deepEqual(find('artist:"arijit singh" tum'), [1]);
});
