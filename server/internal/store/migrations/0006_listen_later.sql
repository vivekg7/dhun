-- Each user's Listen Later list, shaped like favorites: newest first by at,
-- and an item finished far enough is taken off by the server
-- (docs/plans/010_special_playlists.md).
CREATE TABLE listen_later (
    user_id INTEGER NOT NULL REFERENCES users (id) ON DELETE CASCADE,
    song_id INTEGER NOT NULL REFERENCES songs (id),
    at      TEXT    NOT NULL, -- of the latest add/remove; the later one wins
    deleted INTEGER NOT NULL DEFAULT 0,
    version INTEGER NOT NULL,
    PRIMARY KEY (user_id, song_id)
);
