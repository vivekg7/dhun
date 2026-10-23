//! The working copy (plan 006): the catalogue, the user's synced data, and
//! the outbox of changes not yet on the server. The same tables as the
//! phone's Room database and the Mac's (plan 024). Lists of song IDs are
//! kept as comma-separated text: they are only ever read and written whole.

use crate::model::{Chapter, Playlist, Song};
use rusqlite::{Connection, OptionalExtension, params};
use std::sync::Arc;

pub struct Db {
    pub sql: Connection,
}

pub type Result<T> = rusqlite::Result<T>;

/// Each step runs once, in order; a new one goes at the end. No step
/// touches the outbox: an upgrade never loses an offline edit (AGENTS.md).
const STEPS: [&str; 3] = [
    "CREATE TABLE song (id INTEGER PRIMARY KEY, path TEXT NOT NULL, title TEXT NOT NULL,
      artist TEXT NOT NULL, artists TEXT NOT NULL, album TEXT NOT NULL, albumArtist TEXT NOT NULL,
      composer TEXT NOT NULL, genres TEXT NOT NULL, year INTEGER NOT NULL, track INTEGER NOT NULL,
      disc INTEGER NOT NULL, durationMs INTEGER NOT NULL, format TEXT NOT NULL, bitrate INTEGER NOT NULL,
      sampleRate INTEGER NOT NULL, bitDepth INTEGER NOT NULL, size INTEGER NOT NULL,
      hasArt INTEGER NOT NULL, hasLyrics INTEGER NOT NULL, addedAt INTEGER NOT NULL,
      missing INTEGER NOT NULL, art TEXT NOT NULL);
    CREATE TABLE playlist (id INTEGER PRIMARY KEY, name TEXT NOT NULL, path TEXT NOT NULL,
      shared INTEGER NOT NULL, songs TEXT NOT NULL, ref TEXT NOT NULL);
    CREATE TABLE queue (id TEXT PRIMARY KEY, name TEXT NOT NULL, songs TEXT NOT NULL,
      currentSong INTEGER NOT NULL, positionMs INTEGER NOT NULL, shuffle INTEGER NOT NULL,
      repeat TEXT NOT NULL, usedAt INTEGER NOT NULL);
    CREATE TABLE mark (kind TEXT NOT NULL, song INTEGER NOT NULL, at INTEGER NOT NULL,
      deleted INTEGER NOT NULL, PRIMARY KEY (kind, song));
    CREATE TABLE resume (song INTEGER PRIMARY KEY, positionMs INTEGER NOT NULL, at INTEGER NOT NULL,
      deleted INTEGER NOT NULL);
    CREATE TABLE setting (name TEXT PRIMARY KEY, value TEXT NOT NULL);
    CREATE TABLE outbox (seq INTEGER PRIMARY KEY AUTOINCREMENT, id TEXT NOT NULL, key TEXT NOT NULL,
      json TEXT NOT NULL);
    CREATE TABLE play_stat (song INTEGER PRIMARY KEY, count INTEGER NOT NULL,
      lastPlayedAt INTEGER NOT NULL);
    CREATE TABLE pin (key TEXT PRIMARY KEY, kind TEXT NOT NULL, ref TEXT NOT NULL, name TEXT NOT NULL,
      at INTEGER NOT NULL);
    CREATE TABLE download (song INTEGER PRIMARY KEY, path TEXT NOT NULL, size INTEGER NOT NULL,
      at INTEGER NOT NULL);
    CREATE TABLE lyrics (song INTEGER PRIMARY KEY, text TEXT NOT NULL, at INTEGER NOT NULL);
    CREATE TABLE thumb (key TEXT PRIMARY KEY, data BLOB NOT NULL);",
    // Podcasts and audiobooks (plan 031). Chapters are the server's JSON, "" for none.
    "ALTER TABLE song ADD COLUMN kind TEXT NOT NULL DEFAULT '';
    ALTER TABLE song ADD COLUMN date TEXT NOT NULL DEFAULT '';
    ALTER TABLE song ADD COLUMN notes TEXT NOT NULL DEFAULT '';
    ALTER TABLE song ADD COLUMN chapters TEXT NOT NULL DEFAULT '';
    ALTER TABLE song ADD COLUMN groupKey TEXT NOT NULL DEFAULT '';
    ALTER TABLE song ADD COLUMN groupTitle TEXT NOT NULL DEFAULT '';
    ALTER TABLE song ADD COLUMN groupIndex INTEGER NOT NULL DEFAULT 0;",
    // Bookmarks in long files (plan 033).
    "CREATE TABLE bookmark (id TEXT PRIMARY KEY, song INTEGER NOT NULL, positionMs INTEGER NOT NULL,
      note TEXT NOT NULL, at INTEGER NOT NULL, deleted INTEGER NOT NULL);",
];

/// Joins and splits the text form of a list (artists, genres).
const SEP: &str = "\u{1f}";

impl Db {
    pub fn open(path: &std::path::Path) -> Result<Db> {
        let sql = Connection::open(path)?;
        // WAL: the main thread's reads do not wait for a sync's writes.
        sql.pragma_update(None, "journal_mode", "WAL")?;
        sql.pragma_update(None, "synchronous", "NORMAL")?;
        let mut db = Db { sql };
        db.migrate()?;
        Ok(db)
    }

    fn migrate(&mut self) -> Result<()> {
        let have: i64 = self.sql.pragma_query_value(None, "user_version", |r| r.get(0))?;
        for (i, step) in STEPS.iter().enumerate().skip(have as usize) {
            let tx = self.sql.transaction()?;
            tx.execute_batch(step)?;
            tx.pragma_update(None, "user_version", i as i64 + 1)?;
            tx.commit()?;
        }
        Ok(())
    }

    /// Everything this account kept, its unsent edits included: used when the
    /// user signs out or another signs in, as the phone's `forgetAccount`. A
    /// refused token keeps everything instead (plan 023), so an offline edit
    /// is only ever dropped by the person who made it.
    pub fn wipe(&self) -> Result<()> {
        self.sql.execute_batch(
            "DELETE FROM song; DELETE FROM playlist; DELETE FROM queue; DELETE FROM mark; DELETE FROM resume;
            DELETE FROM setting; DELETE FROM outbox; DELETE FROM play_stat; DELETE FROM pin; DELETE FROM download;
            DELETE FROM lyrics; DELETE FROM bookmark;",
        )
    }

    // Songs.

    /// Shared from the start, so the catalogue keeps them as they are read
    /// rather than moving each into an allocation of its own.
    pub fn songs(&self) -> Result<Vec<Arc<Song>>> {
        let mut st = self.sql.prepare(
            "SELECT id, path, title, artist, artists, album, albumArtist, composer, genres, year, track, disc,
            durationMs, format, bitrate, sampleRate, bitDepth, size, hasArt, hasLyrics, addedAt, missing, art,
            kind, date, notes, chapters, groupKey, groupTitle, groupIndex FROM song WHERE missing = 0",
        )?;
        let rows = st.query_map([], |r| {
            Ok(Arc::new(Song {
                id: r.get(0)?,
                path: r.get(1)?,
                title: r.get(2)?,
                artist: r.get(3)?,
                artists: split(&r.get::<_, String>(4)?),
                album: r.get(5)?,
                album_artist: r.get(6)?,
                composer: r.get(7)?,
                genres: split(&r.get::<_, String>(8)?),
                year: r.get(9)?,
                track: r.get(10)?,
                disc: r.get(11)?,
                duration_ms: r.get(12)?,
                format: r.get(13)?,
                bitrate: r.get(14)?,
                sample_rate: r.get(15)?,
                bit_depth: r.get(16)?,
                size: r.get(17)?,
                has_art: r.get(18)?,
                has_lyrics: r.get(19)?,
                added_at: r.get(20)?,
                missing: r.get(21)?,
                art: r.get(22)?,
                kind: r.get(23)?,
                date: r.get(24)?,
                notes: r.get(25)?,
                chapters: chapters(&r.get::<_, String>(26)?),
                group_key: r.get(27)?,
                group_title: r.get(28)?,
                group_index: r.get(29)?,
            }))
        })?;
        rows.collect()
    }

    /// In transactions of a couple of thousand rows: the first pull is a
    /// hundred thousand, and a write on the main thread waits for the
    /// transaction in hand, not for all of them.
    pub fn put_songs(&mut self, songs: &[Song]) -> Result<()> {
        for batch in songs.chunks(2000) {
            let tx = self.sql.transaction()?;
            {
                let mut st = tx.prepare_cached(
                    "INSERT OR REPLACE INTO song (id, path, title, artist, artists, album, albumArtist, composer,
                    genres, year, track, disc, durationMs, format, bitrate, sampleRate, bitDepth, size, hasArt,
                    hasLyrics, addedAt, missing, art, kind, date, notes, chapters, groupKey, groupTitle,
                    groupIndex) VALUES (?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?)",
                )?;
                for s in batch {
                    st.execute(params![
                        s.id,
                        s.path,
                        s.title,
                        s.artist,
                        s.artists.join(SEP),
                        s.album,
                        s.album_artist,
                        s.composer,
                        s.genres.join(SEP),
                        s.year,
                        s.track,
                        s.disc,
                        s.duration_ms,
                        s.format,
                        s.bitrate,
                        s.sample_rate,
                        s.bit_depth,
                        s.size,
                        s.has_art,
                        s.has_lyrics,
                        s.added_at,
                        s.missing,
                        s.art,
                        s.kind,
                        s.date,
                        s.notes,
                        chapters_text(&s.chapters),
                        s.group_key,
                        s.group_title,
                        s.group_index,
                    ])?;
                }
            }
            tx.commit()?;
        }
        Ok(())
    }

    // Playlists.

    pub fn playlists(&self) -> Result<Vec<Playlist>> {
        let mut st = self
            .sql
            .prepare("SELECT id, name, path, shared, songs, ref FROM playlist ORDER BY shared, name COLLATE NOCASE")?;
        let rows = st.query_map([], |r| {
            Ok(Playlist {
                id: r.get(0)?,
                name: r.get(1)?,
                path: r.get(2)?,
                shared: r.get(3)?,
                songs: ids(&r.get::<_, String>(4)?),
                r#ref: r.get(5)?,
            })
        })?;
        rows.collect()
    }

    pub fn put_playlist(&self, p: &Playlist) -> Result<()> {
        self.sql.execute(
            "INSERT OR REPLACE INTO playlist VALUES (?,?,?,?,?,?)",
            params![p.id, p.name, p.path, p.shared, join(&p.songs), p.r#ref],
        )?;
        Ok(())
    }

    pub fn delete_playlist(&self, id: i64) -> Result<()> {
        self.sql.execute("DELETE FROM playlist WHERE id = ?", [id])?;
        Ok(())
    }

    // Outbox: the keys of ops not yet sent, which a pull must not overwrite.

    pub fn outbox_keys(&self) -> Result<Vec<String>> {
        let mut st = self.sql.prepare("SELECT key FROM outbox ORDER BY seq")?;
        let rows = st.query_map([], |r| r.get(0))?;
        rows.collect()
    }

    pub fn move_pin(&self, old: &str, key: &str, r#ref: &str) -> Result<()> {
        self.sql.execute("UPDATE pin SET key = ?, ref = ? WHERE key = ?", params![key, r#ref, old])?;
        Ok(())
    }

    // Thumbnails.

    pub fn thumb(&self, key: &str) -> Result<Option<Vec<u8>>> {
        self.sql.query_row("SELECT data FROM thumb WHERE key = ?", [key], |r| r.get(0)).optional()
    }

    pub fn art_keys(&self) -> Result<Vec<String>> {
        let mut st =
            self.sql.prepare("SELECT DISTINCT art FROM song WHERE missing = 0 AND hasArt = 1 AND art != ''")?;
        let rows = st.query_map([], |r| r.get(0))?;
        rows.collect()
    }

    pub fn thumb_keys(&self) -> Result<Vec<String>> {
        let mut st = self.sql.prepare("SELECT key FROM thumb")?;
        let rows = st.query_map([], |r| r.get(0))?;
        rows.collect()
    }

    pub fn put_thumbs(&mut self, thumbs: &[(String, Vec<u8>)]) -> Result<()> {
        let tx = self.sql.transaction()?;
        {
            let mut st = tx.prepare_cached("INSERT OR REPLACE INTO thumb VALUES (?,?)")?;
            for (k, d) in thumbs {
                st.execute(params![k, d])?;
            }
        }
        tx.commit()
    }

    pub fn delete_thumbs(&mut self, keys: &[String]) -> Result<()> {
        let tx = self.sql.transaction()?;
        {
            let mut st = tx.prepare_cached("DELETE FROM thumb WHERE key = ?")?;
            for k in keys {
                st.execute([k])?;
            }
        }
        tx.commit()
    }
}

fn split(s: &str) -> Vec<String> {
    if s.is_empty() { vec![] } else { s.split(SEP).map(String::from).collect() }
}

fn chapters(s: &str) -> Vec<Chapter> {
    if s.is_empty() { vec![] } else { serde_json::from_str(s).unwrap_or_default() }
}

fn chapters_text(c: &[Chapter]) -> String {
    if c.is_empty() { String::new() } else { serde_json::to_string(c).unwrap_or_default() }
}

pub fn ids(s: &str) -> Vec<i64> {
    s.split(',').filter_map(|x| x.parse().ok()).collect()
}

pub fn join(ids: &[i64]) -> String {
    ids.iter().map(i64::to_string).collect::<Vec<_>>().join(",")
}
