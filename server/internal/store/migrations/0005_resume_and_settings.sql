-- Where each user stopped in a long file (an audiobook, a podcast), so any
-- device can continue it later, whatever was played in between
-- (docs/plans/009_resume_long_files.md).
CREATE TABLE resume_points (
    user_id     INTEGER NOT NULL REFERENCES users (id) ON DELETE CASCADE,
    song_id     INTEGER NOT NULL REFERENCES songs (id),
    position_ms INTEGER NOT NULL,
    at          TEXT    NOT NULL, -- of the latest set/unset; the later one wins
    deleted     INTEGER NOT NULL DEFAULT 0,
    version     INTEGER NOT NULL,
    PRIMARY KEY (user_id, song_id)
);

-- App settings that follow the user to every device. Values are JSON, read
-- only by the apps; the server stores them and never interprets them.
CREATE TABLE settings (
    user_id INTEGER NOT NULL REFERENCES users (id) ON DELETE CASCADE,
    name    TEXT    NOT NULL,
    value   TEXT    NOT NULL,
    at      TEXT    NOT NULL,
    version INTEGER NOT NULL,
    PRIMARY KEY (user_id, name)
);
