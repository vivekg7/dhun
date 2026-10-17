-- Places a user marked in a long file, each with an optional note, synced
-- one by one so bookmarks added on two devices are both kept
-- (docs/plans/033_bookmarks.md).
CREATE TABLE bookmarks (
    user_id     INTEGER NOT NULL REFERENCES users (id) ON DELETE CASCADE,
    id          TEXT    NOT NULL, -- made by the client, so one made offline has it at once
    song_id     INTEGER NOT NULL REFERENCES songs (id),
    position_ms INTEGER NOT NULL,
    note        TEXT    NOT NULL DEFAULT '',
    at          TEXT    NOT NULL, -- of the latest change; the later one wins
    deleted     INTEGER NOT NULL DEFAULT 0, -- wins over any edit
    version     INTEGER NOT NULL,
    PRIMARY KEY (user_id, id)
);
