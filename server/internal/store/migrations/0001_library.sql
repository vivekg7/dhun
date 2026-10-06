-- Library, users and devices. docs/plans/005_storage_and_library_model.md

CREATE TABLE meta (
    key   TEXT PRIMARY KEY,
    value INTEGER NOT NULL
);

-- Bumped on every change to songs or playlists; clients pull
-- "everything with version > my cursor" (docs/plans/006_api_and_sync.md).
INSERT INTO meta (key, value) VALUES ('library_version', 0);

CREATE TABLE users (
    id            INTEGER PRIMARY KEY,
    name          TEXT    NOT NULL UNIQUE COLLATE NOCASE,
    password_hash TEXT    NOT NULL,
    is_admin      INTEGER NOT NULL DEFAULT 0,
    created_at    TEXT    NOT NULL
);

CREATE TABLE devices (
    id           INTEGER PRIMARY KEY,
    user_id      INTEGER NOT NULL REFERENCES users (id) ON DELETE CASCADE,
    name         TEXT    NOT NULL,
    token_hash   BLOB    NOT NULL UNIQUE,
    created_at   TEXT    NOT NULL,
    last_seen_at TEXT    NOT NULL
);

-- AUTOINCREMENT, not plain rowid: a song ID is never reused, because user
-- data on some device may still point at it.
CREATE TABLE songs (
    id            INTEGER PRIMARY KEY AUTOINCREMENT,
    path          TEXT    NOT NULL UNIQUE, -- relative to the media root, '/'-separated
    size          INTEGER NOT NULL,
    mtime_ns      INTEGER NOT NULL,
    quick_hash    BLOB    NOT NULL,

    title         TEXT    NOT NULL,
    artist        TEXT    NOT NULL DEFAULT '',
    artists       TEXT    NOT NULL DEFAULT '[]', -- JSON array, split from artist
    album         TEXT    NOT NULL DEFAULT '',
    album_artist  TEXT    NOT NULL DEFAULT '',
    composer      TEXT    NOT NULL DEFAULT '',
    genre         TEXT    NOT NULL DEFAULT '',
    genres        TEXT    NOT NULL DEFAULT '[]', -- JSON array
    year          INTEGER NOT NULL DEFAULT 0,
    track         INTEGER NOT NULL DEFAULT 0,
    disc          INTEGER NOT NULL DEFAULT 0,

    duration_ms   INTEGER NOT NULL DEFAULT 0,
    format        TEXT    NOT NULL DEFAULT '',
    codec         TEXT    NOT NULL DEFAULT '',
    bitrate       INTEGER NOT NULL DEFAULT 0, -- kbit/s
    sample_rate   INTEGER NOT NULL DEFAULT 0,
    bit_depth     INTEGER NOT NULL DEFAULT 0,
    channels      INTEGER NOT NULL DEFAULT 0,

    embedded_art    INTEGER NOT NULL DEFAULT 0,
    embedded_lyrics INTEGER NOT NULL DEFAULT 0,
    folder_art      TEXT    NOT NULL DEFAULT '', -- sibling cover file name, if any
    lrc             INTEGER NOT NULL DEFAULT 0,  -- sibling .lrc with the same base name

    added_at      TEXT    NOT NULL,
    updated_at    TEXT    NOT NULL,
    -- Set when the file vanished without a matching move. The row and every
    -- user's data about it stay, so the song comes back intact if it returns.
    missing_since TEXT,
    version       INTEGER NOT NULL
);

CREATE INDEX songs_version    ON songs (version);
CREATE INDEX songs_quick_hash ON songs (quick_hash);

-- An index over the .m3u8 files, which stay the source of truth.
CREATE TABLE playlists (
    id       INTEGER PRIMARY KEY AUTOINCREMENT,
    path     TEXT    NOT NULL UNIQUE, -- relative to the media root
    owner_id INTEGER REFERENCES users (id) ON DELETE SET NULL, -- NULL: shared
    name     TEXT    NOT NULL,
    size     INTEGER NOT NULL,
    mtime_ns INTEGER NOT NULL,
    deleted  INTEGER NOT NULL DEFAULT 0, -- tombstone, so clients learn of removals
    version  INTEGER NOT NULL
);

CREATE TABLE playlist_items (
    playlist_id INTEGER NOT NULL REFERENCES playlists (id) ON DELETE CASCADE,
    pos         INTEGER NOT NULL,
    song_id     INTEGER REFERENCES songs (id), -- NULL: entry did not resolve
    raw_path    TEXT    NOT NULL,              -- the line as written in the file
    title       TEXT    NOT NULL DEFAULT '',   -- from #EXTINF
    duration_s  INTEGER NOT NULL DEFAULT -1,   -- from #EXTINF
    PRIMARY KEY (playlist_id, pos)
);
