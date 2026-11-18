// A paused player follows the place synced from another device
// (docs/plans/043_follow_the_synced_place.md): what counts as a change.

import { test } from "node:test";
import assert from "node:assert/strict";
import { storedPlace } from "../js/queue.js";

const q = { id: "a", songs: [1, 2, 3], currentSong: 2, positionMs: 60_000 };
const resumes = (pos) => new Map([[2, { positionMs: pos, at: 1, deleted: false }]]);

// Moving reopens the file and closes the listen on it, so only a new place may
// move it: not songs added to the queue elsewhere, nor another file's resume
// point, nor this device's own save coming back with a newer time.
test("only a new place for the open queue or the held file counts", () => {
  const before = storedPlace(q, 2, resumes(60_000));
  const same = (after) => assert.equal(after, before);
  same(storedPlace({ ...q, songs: [1, 2, 3, 4], usedAt: 9 }, 2, resumes(60_000)));
  same(storedPlace(q, 2, new Map([...resumes(60_000), [3, { positionMs: 5, at: 9 }]])));
  same(
    storedPlace(q, 2, new Map([[2, { positionMs: 60_000, at: 9, deleted: false }]])),
  );

  const moved = (after) => assert.notEqual(after, before);
  moved(storedPlace({ ...q, currentSong: 3, positionMs: 0 }, 2, resumes(60_000)));
  moved(storedPlace({ ...q, positionMs: 90_000 }, 2, resumes(60_000)));
  moved(storedPlace(q, 2, resumes(12_000_000)));
  moved(
    storedPlace(q, 2, new Map([[2, { positionMs: 60_000, at: 9, deleted: true }]])),
  );
});
