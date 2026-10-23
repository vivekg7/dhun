//! Keeps the working copy and the server in step (plan 006), as the phone's
//! and the Mac's `Sync` do. Step 1 of plan 039 pulls the library; the
//! outbox's push and the synced state join in step 3.

use crate::api::{self, Api};
use crate::db::Db;
use crate::model::Playlist;
use std::sync::Mutex;

/// Raised when songs gain a field: 1 is the art key (plan 019), 2 podcasts
/// and audiobooks, which the server sends only to a client that asks for
/// them from the start (plan 031).
pub const LIBRARY_FORMAT: i64 = 2;

/// What a pull changed, so the app rebuilds only what it must.
#[derive(Default, Debug)]
pub struct Pulled {
    pub songs: usize,
    pub playlists: bool,
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
    let mut out = Pulled { songs: songs.len(), playlists: false };
    let mut db = db.lock().unwrap();
    let fail = |e: rusqlite::Error| api::Error::Network(format!("Could not save the library: {e}"));
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
