//! Every change to the user's synced data goes through here, as on the
//! phone and the Mac: it is applied to the working copy at once and
//! recorded in the outbox, which `sync` pushes when the server is
//! reachable. Offline edits are never lost (AGENTS.md).

use crate::db::{Db, Result};
use crate::model::{Bookmark, Mark, Playlist, QueueRow, Resume};
use crate::time;
use serde_json::{Map, Value, json};

pub const FAV: &str = "fav";
pub const LATER: &str = "later";
/// An episode or a book's file heard (plan 031).
pub const PLAYED: &str = "played";
pub const PLAYED_PERCENT: i64 = 90;

/// One listen (plan 008).
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Listen {
    pub song: i64,
    pub started_at: i64,
    pub ended_at: i64,
    pub ms: i64,
    pub from_ms: i64,
    pub to_ms: i64,
    pub end: String,
    pub source: String,
    pub queue: String,
    pub shuffle: bool,
    pub utc_offset: i64,
}

/// `key` replaces earlier ops with the same key. `tag` marks an op as
/// touching a queue (`:<id>`) or a playlist (`:pl<id>`), so a pull does not
/// overwrite it with an older server copy while the op is still unsent.
fn record(db: &mut Db, kind: &str, key: Option<String>, tag: &str, at: i64, fields: Value) -> Result<()> {
    let id = uuid();
    let mut op: Map<String, Value> = match fields {
        Value::Object(m) => m,
        _ => Map::new(),
    };
    op.insert("id".into(), id.clone().into());
    op.insert("type".into(), kind.into());
    op.insert("at".into(), time::format(at).into());
    let replacing = key.is_some();
    let key = key.unwrap_or_else(|| format!("{id}{tag}"));
    db.add_op(&id, &key, &Value::Object(op).to_string(), replacing)
}

fn now() -> i64 {
    time::now_ms()
}

/// A song into or out of Favorites, Listen Later or the played marks; only
/// the last toggle of a song is worth sending.
pub fn mark(db: &mut Db, kind: &str, song: i64, on: bool) -> Result<()> {
    let at = now();
    db.put_mark(&Mark { kind: kind.into(), song, at, deleted: !on })?;
    let kind_op = match (kind, on) {
        (FAV, true) => "favorite.set",
        (FAV, false) => "favorite.unset",
        (PLAYED, true) => "played.set",
        (PLAYED, false) => "played.unset",
        (_, true) => "listen_later.add",
        (_, false) => "listen_later.remove",
    };
    record(db, kind_op, Some(format!("{kind}:{song}")), "", at, json!({ "song": song }))
}

/// Makes, changes or removes a bookmark (plan 033), sent whole each time.
pub fn bookmark(db: &mut Db, b: &Bookmark) -> Result<()> {
    let at = now();
    db.put_bookmark(&Bookmark { at, ..b.clone() })?;
    let key = Some(format!("bookmark:{}", b.id));
    if b.deleted {
        record(db, "bookmark.unset", key, "", at, json!({ "bookmark": b.id }))
    } else {
        let f = json!({ "bookmark": b.id, "song": b.song, "positionMs": b.position_ms, "note": b.note });
        record(db, "bookmark.set", key, "", at, f)
    }
}

pub fn setting(db: &mut Db, name: &str, value: Value) -> Result<()> {
    db.put_setting(name, &value)?;
    record(db, "setting.set", Some(format!("setting:{name}")), "", now(), json!({ "name": name, "value": value }))
}

/// Where a long file was left, or None once it was heard to the end (plan 009).
pub fn resume(db: &mut Db, song: i64, position_ms: Option<i64>) -> Result<()> {
    let at = now();
    db.put_resume(&Resume { song, position_ms: position_ms.unwrap_or(0), at, deleted: position_ms.is_none() })?;
    let key = Some(format!("resume:{song}"));
    match position_ms {
        Some(p) => record(db, "resume.set", key, "", at, json!({ "song": song, "positionMs": p })),
        None => record(db, "resume.unset", key, "", at, json!({ "song": song })),
    }
}

// Queues.

pub fn create_queue(db: &mut Db, q: &QueueRow) -> Result<()> {
    db.put_queue(q)?;
    let f =
        json!({ "queue": q.id, "name": q.name, "songs": q.songs, "song": q.current_song, "positionMs": q.position_ms });
    record(db, "queue.create", None, &format!(":{}", q.id), now(), f)
}

pub fn replace_queue(db: &mut Db, q: &QueueRow) -> Result<()> {
    db.put_queue(q)?;
    record(
        db,
        "queue.replace",
        Some(format!("replace:{}", q.id)),
        "",
        now(),
        json!({ "queue": q.id, "songs": q.songs }),
    )
}

pub fn rename_queue(db: &mut Db, q: &QueueRow) -> Result<()> {
    db.put_queue(q)?;
    record(db, "queue.rename", Some(format!("rename:{}", q.id)), "", now(), json!({ "queue": q.id, "name": q.name }))
}

pub fn delete_queue(db: &mut Db, id: &str) -> Result<()> {
    db.delete_queue(id)?;
    record(db, "queue.delete", None, &format!(":{id}"), now(), json!({ "queue": id }))
}

/// The queue's current song and position; only the latest is worth sending.
pub fn set_current(db: &mut Db, q: &QueueRow) -> Result<()> {
    db.put_queue(q)?;
    let f = json!({ "queue": q.id, "song": q.current_song, "positionMs": q.position_ms });
    record(db, "queue.set_current", Some(format!("current:{}", q.id)), "", now(), f)
}

pub fn set_mode(db: &mut Db, q: &QueueRow) -> Result<()> {
    db.put_queue(q)?;
    let f = json!({ "queue": q.id, "shuffle": q.shuffle, "repeat": q.repeat_mode });
    record(db, "queue.set_mode", Some(format!("mode:{}", q.id)), "", now(), f)
}

pub fn insert_into_queue(db: &mut Db, q: &QueueRow, songs: &[i64], after: Option<i64>) -> Result<()> {
    db.put_queue(q)?;
    let mut f = json!({ "queue": q.id, "songs": songs });
    if let Some(a) = after {
        f["after"] = a.into();
    }
    record(db, "queue.insert", None, &format!(":{}", q.id), now(), f)
}

pub fn remove_from_queue(db: &mut Db, q: &QueueRow, songs: &[i64]) -> Result<()> {
    db.put_queue(q)?;
    record(db, "queue.remove", None, &format!(":{}", q.id), now(), json!({ "queue": q.id, "songs": songs }))
}

pub fn move_in_queue(db: &mut Db, q: &QueueRow, song: i64, after: i64) -> Result<()> {
    db.put_queue(q)?;
    let f = json!({ "queue": q.id, "song": song, "after": after });
    record(db, "queue.move", None, &format!(":{}", q.id), now(), f)
}

// Playlists (plan 013). The songs given are music only: a playlist is an
// `.m3u8` in Music, which cannot name a podcast or a book (plan 031).

/// How an op names a playlist: its id, or its ref until the server has given it one.
fn target(p: &Playlist) -> String {
    if p.id > 0 { p.id.to_string() } else { format!("ref:{}", p.r#ref) }
}

fn record_on(db: &mut Db, p: &Playlist, kind: &str, mut fields: Value) -> Result<()> {
    fields["playlist"] = target(p).into();
    record(db, kind, None, &format!(":pl{}", p.id), now(), fields)
}

/// A new playlist of the user's own, with a negative id until the server
/// answers with the same ref.
pub fn create_playlist(db: &mut Db, name: &str, songs: &[i64]) -> Result<Playlist> {
    let id = -(i64::from_str_radix(&uuid().replace('-', "")[..15], 16).unwrap_or(1).max(1));
    let p = Playlist {
        id,
        name: name.into(),
        songs: crate::queue::distinct(songs.iter().copied()),
        r#ref: uuid(),
        ..Default::default()
    };
    db.put_playlist(&p)?;
    let f = json!({ "ref": p.r#ref, "name": name, "songs": p.songs });
    record(db, "playlist.create", None, &format!(":pl{}", p.id), now(), f)?;
    Ok(p)
}

/// Adds `songs` at the end, skipping those already in it; returns how many were skipped.
pub fn add_to_playlist(db: &mut Db, p: &Playlist, songs: &[i64]) -> Result<usize> {
    let distinct = crate::queue::distinct(songs.iter().copied());
    let adding: Vec<i64> = distinct.iter().copied().filter(|s| !p.songs.contains(s)).collect();
    if !adding.is_empty() {
        let mut q = p.clone();
        q.songs.extend(&adding);
        db.put_playlist(&q)?;
        record_on(db, p, "playlist.insert", json!({ "songs": adding }))?;
    }
    Ok(distinct.len() - adding.len())
}

/// Removes the entry at `index` of the playlist's own list, unmatched entries included.
pub fn remove_from_playlist(db: &mut Db, p: &Playlist, index: usize) -> Result<Playlist> {
    let mut q = p.clone();
    if index >= p.songs.len() {
        return Ok(q);
    }
    let song = p.songs[index];
    let occurrence = p.songs[..index].iter().filter(|&&s| s == song).count();
    q.songs.remove(index);
    db.put_playlist(&q)?;
    record_on(db, p, "playlist.remove", json!({ "song": song, "occurrence": occurrence }))?;
    Ok(q)
}

/// Moves the entry at `from` to `to`, both indexes into the playlist's own
/// list. A song can be in a playlist twice, so each end is named by its
/// song and which of its entries it is.
pub fn move_in_playlist(db: &mut Db, p: &Playlist, from: usize, to: usize) -> Result<()> {
    let mut list = p.songs.clone();
    if from >= list.len() || to >= list.len() || from == to {
        return Ok(());
    }
    let song = list[from];
    let occurrence = list[..from].iter().filter(|&&s| s == song).count();
    let moved = list.remove(from);
    list.insert(to, moved);
    db.put_playlist(&Playlist { songs: list.clone(), ..p.clone() })?;
    let after = if to == 0 { 0 } else { list[to - 1] };
    if to > 0 && after == 0 {
        // An unmatched entry cannot be named as "after" (0 means the start): send the order.
        return record_on(db, p, "playlist.replace", json!({ "songs": list }));
    }
    let mut f = json!({ "song": song, "occurrence": occurrence, "after": after });
    if to > 0 {
        f["afterOccurrence"] = list[..to - 1].iter().filter(|&&s| s == after).count().into();
    }
    record_on(db, p, "playlist.move", f)
}

/// The whole order at once: a sort or a reverse.
pub fn replace_playlist(db: &mut Db, p: &Playlist, songs: &[i64]) -> Result<()> {
    db.put_playlist(&Playlist { songs: songs.to_vec(), ..p.clone() })?;
    record_on(db, p, "playlist.replace", json!({ "songs": songs }))
}

pub fn rename_playlist(db: &mut Db, p: &Playlist, name: &str) -> Result<()> {
    db.put_playlist(&Playlist { name: name.into(), ..p.clone() })?;
    record_on(db, p, "playlist.rename", json!({ "name": name }))
}

/// The server keeps a copy of the file; a download of it goes too.
pub fn delete_playlist(db: &mut Db, p: &Playlist) -> Result<()> {
    db.delete_playlist(p.id)?;
    db.delete_pin(&crate::downloads::key(crate::downloads::PLAYLIST, &p.id.to_string()))?;
    record_on(db, p, "playlist.delete", json!({}))
}

/// Feeds /now-playing, for "Continue from …" on another device.
pub fn playback_state(db: &mut Db, queue: &str, song: i64, position_ms: i64, playing: bool) -> Result<()> {
    let f = json!({ "queue": queue, "song": song, "positionMs": position_ms, "playing": playing });
    record(db, "playback.state", Some("playback".into()), "", now(), f)
}

/// One listen (plan 008).
pub fn play(db: &mut Db, l: &Listen) -> Result<()> {
    let mut f = json!({
        "song": l.song, "ms": l.ms, "endedAt": time::format(l.ended_at), "fromMs": l.from_ms, "toMs": l.to_ms,
        "end": l.end, "shuffle": l.shuffle, "utcOffset": l.utc_offset,
    });
    if !l.source.is_empty() {
        f["source"] = l.source.clone().into();
    }
    if !l.queue.is_empty() {
        f["queue"] = l.queue.clone().into();
    }
    record(db, "play", None, "", l.started_at, f)
}

/// A random (version 4) UUID, as the other apps make ids for queues and ops.
pub fn uuid() -> String {
    let mut b = [0u8; 16];
    if let Ok(mut f) = std::fs::File::open("/dev/urandom") {
        let _ = std::io::Read::read_exact(&mut f, &mut b);
    }
    b[6] = (b[6] & 0x0f) | 0x40;
    b[8] = (b[8] & 0x3f) | 0x80;
    let h: String = b.iter().map(|x| format!("{x:02x}")).collect();
    format!("{}-{}-{}-{}-{}", &h[..8], &h[8..12], &h[12..16], &h[16..20], &h[20..])
}

#[cfg(test)]
mod tests {
    use super::*;

    fn db() -> Db {
        Db::open(std::path::Path::new(":memory:")).unwrap()
    }

    // Only the latest of a keyed op reaches the server; every listen does.
    #[test]
    fn keyed_ops_replace_and_listens_add_up() {
        let mut d = db();
        let q = QueueRow {
            id: "q1".into(),
            name: "Album".into(),
            songs: vec![1, 2],
            current_song: 1,
            ..Default::default()
        };
        create_queue(&mut d, &q).unwrap();
        for pos in [1000, 2000, 3000] {
            set_current(&mut d, &QueueRow { position_ms: pos, ..q.clone() }).unwrap();
        }
        let l = Listen { song: 1, started_at: 1, ended_at: 2, ms: 5000, end: "finished".into(), ..Default::default() };
        play(&mut d, &l).unwrap();
        play(&mut d, &l).unwrap();
        let ops = d.outbox(100).unwrap();
        let types: Vec<String> = ops
            .iter()
            .map(|(_, _, j)| serde_json::from_str::<Value>(j).unwrap()["type"].as_str().unwrap().to_string())
            .collect();
        assert_eq!(types, ["queue.create", "queue.set_current", "play", "play"]);
        let current: Value = serde_json::from_str(&ops[1].2).unwrap();
        assert_eq!(current["positionMs"], 3000);
        assert_eq!(d.queues().unwrap()[0].position_ms, 3000);
        // Tagged with its queue, so a pull leaves the queue as edited here.
        assert!(ops[0].1.ends_with(":q1"));
    }

    // A song twice in a playlist: the server is told which entry moved and
    // after which entry. Next to an entry that matches no song (0), there
    // is no entry to name, so the whole order goes.
    #[test]
    fn a_move_names_entries_not_songs() {
        let mut d = db();
        let p = Playlist { id: 5, songs: vec![7, 8, 7, 0, 9], ..Default::default() };
        d.put_playlist(&p).unwrap();
        move_in_playlist(&mut d, &p, 2, 1).unwrap();
        move_in_playlist(&mut d, &p, 4, 4).unwrap();
        let moved = Playlist { songs: vec![7, 7, 8, 0, 9], ..p.clone() };
        move_in_playlist(&mut d, &moved, 2, 3).unwrap();
        let ops: Vec<Value> = d.outbox(100).unwrap().iter().map(|(_, _, j)| serde_json::from_str(j).unwrap()).collect();
        assert_eq!(ops.len(), 2);
        assert_eq!(ops[0]["type"], "playlist.move");
        assert_eq!((ops[0]["song"].clone(), ops[0]["occurrence"].clone()), (json!(7), json!(1)));
        assert_eq!((ops[0]["after"].clone(), ops[0]["afterOccurrence"].clone()), (json!(7), json!(0)));
        assert_eq!(ops[1]["type"], "playlist.replace");
        assert_eq!(ops[1]["songs"], json!([7, 7, 0, 8, 9]));
        assert_eq!(d.playlists().unwrap()[0].songs, [7, 7, 0, 8, 9]);
    }
}
