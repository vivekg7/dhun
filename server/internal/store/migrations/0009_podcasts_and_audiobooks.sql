-- foreign_keys: off
-- Podcasts and audiobooks, beside the music (docs/plans/031_podcasts_and_audiobooks.md).
-- Each collection has its own root, so a path is unique within its kind
-- only, and SQLite can change that constraint only by rebuilding the table.
-- The IDs are copied as they are: every user's data points at them.

CREATE TABLE songs_new (
    id            INTEGER PRIMARY KEY AUTOINCREMENT,
    kind          TEXT    NOT NULL DEFAULT 'music', -- music | podcast | audiobook: which root path is under
    path          TEXT    NOT NULL, -- relative to its kind's root, '/'-separated
    size          INTEGER NOT NULL,
    mtime_ns      INTEGER NOT NULL,
    quick_hash    BLOB    NOT NULL,

    title         TEXT    NOT NULL,
    artist        TEXT    NOT NULL DEFAULT '',
    artists       TEXT    NOT NULL DEFAULT '[]',
    album         TEXT    NOT NULL DEFAULT '',
    album_artist  TEXT    NOT NULL DEFAULT '',
    composer      TEXT    NOT NULL DEFAULT '',
    genre         TEXT    NOT NULL DEFAULT '',
    genres        TEXT    NOT NULL DEFAULT '[]',
    year          INTEGER NOT NULL DEFAULT 0,
    track         INTEGER NOT NULL DEFAULT 0,
    disc          INTEGER NOT NULL DEFAULT 0,

    duration_ms   INTEGER NOT NULL DEFAULT 0,
    format        TEXT    NOT NULL DEFAULT '',
    codec         TEXT    NOT NULL DEFAULT '',
    bitrate       INTEGER NOT NULL DEFAULT 0,
    sample_rate   INTEGER NOT NULL DEFAULT 0,
    bit_depth     INTEGER NOT NULL DEFAULT 0,
    channels      INTEGER NOT NULL DEFAULT 0,

    embedded_art    INTEGER NOT NULL DEFAULT 0,
    embedded_lyrics INTEGER NOT NULL DEFAULT 0,
    folder_art      TEXT    NOT NULL DEFAULT '',
    lrc             INTEGER NOT NULL DEFAULT 0,
    art             TEXT    NOT NULL DEFAULT '',

    -- Podcasts and audiobooks only; empty for music.
    date        TEXT    NOT NULL DEFAULT '',   -- the date tag as written, e.g. 2020-08-16
    notes       TEXT    NOT NULL DEFAULT '',   -- the comment tag: an episode's show notes
    chapters    TEXT    NOT NULL DEFAULT '[]', -- JSON [{"startMs", "title"}], from inside the file
    transcript  TEXT    NOT NULL DEFAULT '',   -- sibling .transcript.md or .vtt file name
    group_key   TEXT    NOT NULL DEFAULT '',   -- the book or show this file belongs to
    group_title TEXT    NOT NULL DEFAULT '',
    group_index INTEGER NOT NULL DEFAULT 0,    -- order within the group

    added_at      TEXT    NOT NULL,
    updated_at    TEXT    NOT NULL,
    missing_since TEXT,
    version       INTEGER NOT NULL,
    UNIQUE (kind, path)
);

INSERT INTO songs_new (id, path, size, mtime_ns, quick_hash, title, artist, artists, album, album_artist,
    composer, genre, genres, year, track, disc, duration_ms, format, codec, bitrate, sample_rate, bit_depth,
    channels, embedded_art, embedded_lyrics, folder_art, lrc, art, added_at, updated_at, missing_since, version)
SELECT id, path, size, mtime_ns, quick_hash, title, artist, artists, album, album_artist,
    composer, genre, genres, year, track, disc, duration_ms, format, codec, bitrate, sample_rate, bit_depth,
    channels, embedded_art, embedded_lyrics, folder_art, lrc, art, added_at, updated_at, missing_since, version
FROM songs;

-- AUTOINCREMENT's counter, so an ID is never reused (0001).
UPDATE sqlite_sequence SET seq = (SELECT seq FROM sqlite_sequence WHERE name = 'songs') WHERE name = 'songs_new';

DROP TABLE songs;
ALTER TABLE songs_new RENAME TO songs;
CREATE INDEX songs_version    ON songs (version);
CREATE INDEX songs_quick_hash ON songs (quick_hash);

-- Which episodes and book files each user has heard, shaped like favorites:
-- deleted = marked unplayed again. The later change wins.
CREATE TABLE played (
    user_id INTEGER NOT NULL REFERENCES users (id) ON DELETE CASCADE,
    song_id INTEGER NOT NULL REFERENCES songs (id),
    at      TEXT    NOT NULL,
    deleted INTEGER NOT NULL DEFAULT 0,
    version INTEGER NOT NULL,
    PRIMARY KEY (user_id, song_id)
);
