//! Keeps the working copy and the server in step (plan 006), as the phone's
//! and the Mac's `Sync` do: fetch catalogue changes, push the outbox, pull
//! what changed elsewhere. Blocking; the app runs a round on a worker thread.

use crate::api::{self, Api, SyncState};
use crate::db::Db;
use crate::model::{Bookmark, Mark, NowPlaying, PlayStat, Playlist, QueueRow, Resume};
use crate::store::{FAV, LATER, PLAYED};
use crate::time::parse;
use std::collections::HashSet;
use std::sync::Mutex;

/// Raised when songs gain a field: 1 is the art key (plan 019), 2 podcasts
/// and audiobooks, which the server sends only to a client that asks for
/// them from the start (plan 031).
pub const LIBRARY_FORMAT: i64 = 2;

/// What a round changed, so the app reloads only what it must.
#[derive(Default, Debug)]
pub struct Pulled {
    pub songs: usize,
    pub playlists: bool,
    /// Queues, marks, resume points, bookmarks or settings.
    pub state: bool,
    /// Sent only when it changed since the cursor: no news is not "nothing playing".
    pub now_playing: Option<NowPlaying>,
    /// A playlist made here that the server refused to create, dropped here too.
    pub refused: bool,
}

fn fail(e: rusqlite::Error) -> api::Error {
    api::Error::Network(format!("Could not save what the server sent: {e}"))
}

/// One whole round: the library, the outbox until it is empty, what changed
/// elsewhere, the play counts.
pub fn round(api: &Api, db: &Mutex<Db>) -> api::Result<Pulled> {
    let mut out = pull_library(api, db)?;
    // More than 500 ops (a long offline stretch) take several rounds.
    let mut pushed_playlists = false;
    loop {
        let (answered, playlists) = push_and_pull(api, db, &mut out)?;
        pushed_playlists |= playlists;
        if answered == 0 || db.lock().unwrap().outbox_size().map_err(fail)? == 0 {
            break;
        }
    }
    // The server rewrote those playlists: fetch them as written.
    if pushed_playlists {
        let again = pull_library(api, db)?;
        out.songs += again.songs;
        out.playlists |= again.playlists;
    }
    let plays = api.plays()?;
    let stats: Vec<PlayStat> = plays
        .into_iter()
        .map(|p| PlayStat {
            song: p.song,
            count: p.count.unwrap_or(0),
            last_played_at: parse(p.last_played_at.as_deref().unwrap_or("")),
        })
        .collect();
    db.lock().unwrap().replace_play_stats(&stats).map_err(fail)?;
    Ok(out)
}

/// One round trip; returns how many ops the server answered, and whether any touched a playlist.
fn push_and_pull(api: &Api, db: &Mutex<Db>, out: &mut Pulled) -> api::Result<(usize, bool)> {
    let ops = db.lock().unwrap().outbox(500).map_err(fail)?;
    let playlists = ops.iter().any(|(_, _, j)| j.contains("\"type\":\"playlist."));
    let json = ops.iter().map(|(_, _, j)| serde_json::from_str(j).unwrap_or_default()).collect();
    let since = api.prefs.get().sync_version;
    let state = api.sync(since, json)?;
    let mut db = db.lock().unwrap();
    // applied, duplicate and rejected all leave the outbox: a rejected op
    // can never succeed (api/openapi.yaml).
    let results = state.results.as_deref().unwrap_or_default();
    let done: Vec<String> = results.iter().map(|r| r.id.clone()).collect();
    // A playlist the server refused to create (a name it reserves) would
    // otherwise stay here forever as a local-only row.
    let refused: HashSet<&str> = results.iter().filter(|r| r.status == "rejected").map(|r| r.id.as_str()).collect();
    for (id, key, json) in &ops {
        if refused.contains(id.as_str()) && json.contains("\"type\":\"playlist.create\"") {
            if let Some(pl) = key.rsplit_once(":pl").and_then(|(_, n)| n.parse().ok()) {
                db.delete_playlist(pl).map_err(fail)?;
                out.refused = true;
            }
        }
    }
    if !done.is_empty() {
        db.ack_ops(&done).map_err(fail)?;
    }
    let version = state.version;
    apply(&db, state, out).map_err(fail)?;
    if let Some(v) = version {
        api.prefs.update(|p| p.sync_version = v);
    }
    Ok((done.len(), playlists))
}

/// Applies what the server says changed. Anything with an op still in the
/// outbox (made while the request was in flight) is left alone: that op
/// goes with the next sync and the server's answer will include it.
fn apply(db: &Db, s: SyncState, out: &mut Pulled) -> rusqlite::Result<()> {
    let pending: HashSet<String> = db.outbox_keys()?.into_iter().collect();
    for q in s.queues.unwrap_or_default() {
        out.state = true;
        if q.deleted == Some(true) {
            db.delete_queue(&q.id)?;
            continue;
        }
        if pending.iter().any(|k| k.ends_with(&format!(":{}", q.id))) {
            continue;
        }
        db.put_queue(&QueueRow {
            name: q.name.unwrap_or_default(),
            songs: q.songs.unwrap_or_default(),
            current_song: q.current_song.unwrap_or(0),
            position_ms: q.position_ms.unwrap_or(0),
            shuffle: q.shuffle.unwrap_or(false),
            repeat_mode: q.repeat.unwrap_or_else(|| "off".into()),
            used_at: parse(q.used_at.as_deref().unwrap_or("")),
            id: q.id,
        })?;
    }
    for (kind, items) in [(FAV, s.favorites), (LATER, s.listen_later), (PLAYED, s.played)] {
        for i in items.unwrap_or_default() {
            out.state = true;
            let at = parse(i.at.as_deref().unwrap_or(""));
            if pending.contains(&format!("{kind}:{}", i.song)) || db.mark(kind, i.song)?.is_some_and(|m| m.at > at) {
                continue;
            }
            db.put_mark(&Mark { kind: kind.into(), song: i.song, at, deleted: i.deleted.unwrap_or(false) })?;
        }
    }
    for r in s.resume.unwrap_or_default() {
        out.state = true;
        let at = parse(r.at.as_deref().unwrap_or(""));
        if pending.contains(&format!("resume:{}", r.song)) || db.resume(r.song)?.is_some_and(|l| l.at > at) {
            continue;
        }
        db.put_resume(&Resume {
            song: r.song,
            position_ms: r.position_ms.unwrap_or(0),
            at,
            deleted: r.deleted.unwrap_or(false),
        })?;
    }
    for b in s.bookmarks.unwrap_or_default() {
        out.state = true;
        let at = parse(b.at.as_deref().unwrap_or(""));
        let local = db.bookmark(&b.id)?;
        let deleted = b.deleted.unwrap_or(false);
        // A delete wins over any edit, as on the server (plan 033).
        if pending.contains(&format!("bookmark:{}", b.id))
            || local.as_ref().is_some_and(|l| l.deleted || (!deleted && l.at > at))
        {
            continue;
        }
        let kept = Bookmark {
            song: b.song.or(local.as_ref().map(|l| l.song)).unwrap_or(0),
            position_ms: b.position_ms.or(local.as_ref().map(|l| l.position_ms)).unwrap_or(0),
            note: if deleted { String::new() } else { b.note.unwrap_or_default() },
            at,
            deleted,
            id: b.id,
        };
        db.put_bookmark(&kept)?;
    }
    for (name, value) in s.settings.unwrap_or_default() {
        out.state = true;
        if !pending.contains(&format!("setting:{name}")) {
            db.put_setting(&name, &value)?;
        }
    }
    if s.now_playing.is_some() {
        out.now_playing = s.now_playing;
    }
    Ok(())
}

/// One pull of the library since the cursor. A playlist with an op still
/// in the outbox is left as edited here, and the cursor stays put so the
/// next pull brings it again once the op has gone (plan 013).
pub fn pull_library(api: &Api, db: &Mutex<Db>) -> api::Result<Pulled> {
    let prefs = &api.prefs;
    // A field added to songs comes only with the songs the server sends
    // again, and it would send none that had not changed.
    if prefs.get().library_format < LIBRARY_FORMAT {
        prefs.update(|p| p.library_version = 0);
    }
    let lib = api.library(prefs.get().library_version)?;
    let songs: Vec<_> = lib.songs.unwrap_or_default().into_iter().map(|s| s.song()).collect();
    let mut out = Pulled { songs: songs.len(), ..Default::default() };
    let mut db = db.lock().unwrap();
    if !songs.is_empty() {
        db.put_songs(&songs).map_err(fail)?;
    }
    let pending = db.outbox_keys().map_err(fail)?;
    let edited = |id: i64| pending.iter().any(|k| k.ends_with(&format!(":pl{id}")));
    let local: Vec<Playlist> = db.playlists().map_err(fail)?.into_iter().filter(|p| p.id < 0).collect();
    let mut skipped = false;
    for p in lib.playlists.unwrap_or_default() {
        out.playlists = true;
        if p.deleted == Some(true) {
            if edited(p.id) {
                skipped = true
            } else {
                db.delete_playlist(p.id).map_err(fail)?
            }
            continue;
        }
        let r#ref = p.r#ref.unwrap_or_default();
        let name = p.name.unwrap_or_default();
        // One made here, back from the server with its real id. A server
        // older than `ref` is matched by name, once its create has gone.
        let mine = local.iter().find(|l| !l.r#ref.is_empty() && l.r#ref == r#ref).or_else(|| {
            local.iter().find(|l| r#ref.is_empty() && p.shared != Some(true) && l.name == name && !edited(l.id))
        });
        if let Some(mine) = mine {
            if edited(mine.id) {
                skipped = true;
                continue;
            }
            db.delete_playlist(mine.id).map_err(fail)?;
            db.move_pin(&format!("playlist:{}", mine.id), &format!("playlist:{}", p.id), &p.id.to_string())
                .map_err(fail)?;
        } else if edited(p.id) {
            skipped = true;
            continue;
        }
        db.put_playlist(&Playlist {
            id: p.id,
            name,
            path: p.path.unwrap_or_default(),
            shared: p.shared.unwrap_or(false),
            songs: p.songs.unwrap_or_default(),
            r#ref,
        })
        .map_err(fail)?;
    }
    drop(db);
    prefs.update(|v| {
        if !skipped {
            v.library_version = lib.version.unwrap_or(v.library_version);
        }
        v.library_format = LIBRARY_FORMAT;
    });
    Ok(out)
}
