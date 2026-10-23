//! Cover art (plan 019), as on the phone and the Mac: kept as files named
//! by the server's art key, so an album's cover is fetched and stored once.
//! Every cover has a 128 px thumbnail in the database, fetched after a
//! sync, so list rows never wait; larger covers are files fetched when
//! first shown, and the `KEEP` used most recently stay. The server already
//! resizes, so no image library.

use crate::api::Api;
use crate::base64;
use crate::db::Db;
use crate::model::Song;
use std::collections::HashSet;
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::Mutex;
use std::time::SystemTime;

pub const THUMB: u32 = 128;
/// The phone's figure: enough full covers for everything played in a long while.
const KEEP: usize = 500;
pub const SIZES: [u32; 4] = [128, 256, 512, 1024];

/// A server without art keys (before 0.1.4): one cover per song.
pub fn key(s: &Song) -> String {
    if s.art.is_empty() { format!("s{}", s.id) } else { s.art.clone() }
}

/// The size to fetch for a cover shown `px` across.
pub fn size_for(px: u32) -> u32 {
    SIZES.into_iter().find(|&s| s >= px).unwrap_or(SIZES[3])
}

/// Fetches every cover's thumbnail not yet here, after each sync: all of
/// them the first time, then only new covers. A failure stops the run; the
/// next sync carries on. Thumbnails no song shows any more are dropped.
/// Returns how many arrived.
pub fn fetch_thumbs(api: &Api, db: &Mutex<Db>) -> usize {
    let (keys, have) = {
        let db = db.lock().unwrap();
        let keys: HashSet<String> = db.art_keys().unwrap_or_default().into_iter().collect();
        let have: HashSet<String> = db.thumb_keys().unwrap_or_default().into_iter().collect();
        (keys, have)
    };
    if keys.is_empty() {
        return 0;
    }
    let gone: Vec<String> = have.difference(&keys).cloned().collect();
    if !gone.is_empty() {
        let _ = db.lock().unwrap().delete_thumbs(&gone);
    }
    let missing: Vec<String> = keys.difference(&have).cloned().collect();
    let mut got = 0;
    for batch in missing.chunks(50) {
        let Ok(thumbs) = api.thumbs(batch) else { break };
        let rows: Vec<(String, Vec<u8>)> =
            thumbs.into_iter().map(|(k, v)| (k, base64::decode(&v).unwrap_or_default())).collect();
        got += rows.len();
        let _ = db.lock().unwrap().put_thumbs(&rows);
    }
    got
}

pub struct Files {
    dir: PathBuf,
    written: Mutex<usize>,
}

impl Files {
    pub fn new(dir: PathBuf) -> Files {
        let _ = fs::create_dir_all(&dir);
        Files { dir, written: Mutex::new(0) }
    }

    /// The kept file for `key` of at least `size` px. One per key: a larger fetch replaces it.
    pub fn kept(&self, key: &str, size: u32) -> Option<PathBuf> {
        SIZES.into_iter().filter(|&s| s >= size).map(|s| self.dir.join(format!("{key}.{s}"))).find(|f| {
            // Touched, so eviction keeps what was shown recently.
            f.exists()
                && fs::File::options().append(true).open(f).and_then(|h| h.set_modified(SystemTime::now())).is_ok()
        })
    }

    /// Blocking: fetches the cover, keeps it and drops smaller copies.
    pub fn fetch(&self, api: &Api, song: i64, key: &str, size: u32) -> Option<PathBuf> {
        let data = api.art(song, size).ok()??;
        let file = self.dir.join(format!("{key}.{size}"));
        let tmp = self.dir.join(format!(".{key}.{size}.tmp"));
        fs::write(&tmp, data).ok()?;
        fs::rename(&tmp, &file).ok()?;
        for s in SIZES.into_iter().filter(|&s| s < size) {
            let _ = fs::remove_file(self.dir.join(format!("{key}.{s}")));
        }
        let n = {
            let mut w = self.written.lock().unwrap();
            *w += 1;
            *w
        };
        if n % 25 == 0 {
            evict(&self.dir);
        }
        Some(file)
    }
}

/// Drops the covers used longest ago beyond `KEEP`.
fn evict(dir: &Path) {
    let Ok(entries) = fs::read_dir(dir) else { return };
    let mut files: Vec<(SystemTime, PathBuf)> =
        entries.flatten().filter_map(|e| Some((e.metadata().ok()?.modified().ok()?, e.path()))).collect();
    if files.len() <= KEEP {
        return;
    }
    files.sort();
    let extra = files.len() - KEEP;
    for (_, f) in files.into_iter().take(extra) {
        let _ = fs::remove_file(f);
    }
}
