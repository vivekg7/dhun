//! The song cache (plan 019), as on the phone and the Mac: original files,
//! fetched whole at the network's full speed, so a song is here seconds
//! after it starts and a later gap in the network does not matter. It keeps
//! the song playing (every song played is kept) and, while playing, the next
//! ten of the queue, or two on a metered network. The player reads a file
//! while it is still arriving.
//!
//! A long file is a window around where it plays instead (plan 037; see
//! `stream`): never a cached song, outside the cache's limit, dropped as
//! soon as it leaves the plan, never promoted to a download.
//!
//! Plain files beside Downloads, so a cached song becomes a download by a
//! rename; a file's modification time is when it was last played, for
//! dropping the oldest at the limit. Never holds a downloaded song. A part
//! or window left by an earlier run is dropped: which of its bytes arrived
//! was only ever known in memory.
//!
//! The player opens songs from GStreamer's thread, so this is shared and
//! locked, where the Mac's lives on the main actor.

use crate::api::Api;
use crate::catalog::SongRef;
use crate::model::Song;
use crate::stream::{End, Part};
use std::collections::{HashMap, HashSet};
use std::os::unix::fs::MetadataExt;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::time::SystemTime;

/// How many songs after the playing one are fetched ahead, and on a metered network.
pub const AHEAD: usize = 10;
pub const AHEAD_METERED: usize = 2;
/// How much of a long file is kept behind the reader (plan 037).
const BEHIND_MINUTES: i64 = 10;
const PARTIAL: [&str; 2] = [".part", ".window"];

pub struct SongCache {
    pub dir: PathBuf,
    api: Arc<Api>,
    inner: Mutex<Inner>,
    /// A fetch ended: the plan is looked at again.
    ended: Arc<AtomicBool>,
}

#[derive(Default)]
struct Inner {
    entries: HashMap<i64, Arc<Part>>,
    /// Downloaded songs' files, which open before anything here.
    downloaded: HashMap<i64, PathBuf>,
    /// `longFiles.minMinutes` and `longFiles.cacheAhead`, for which files get a window.
    long: (i64, i64),
    used: u64,
    songs: HashSet<i64>,
}

impl SongCache {
    pub fn new(api: Arc<Api>, dir: PathBuf) -> SongCache {
        let _ = std::fs::create_dir_all(&dir);
        for f in std::fs::read_dir(&dir).into_iter().flatten().flatten() {
            if !is_whole(&f.file_name().to_string_lossy()) {
                let _ = std::fs::remove_file(f.path());
            }
        }
        let long = (crate::synced::MIN_MINUTES.1, crate::synced::CACHE_AHEAD.1);
        let c = SongCache { dir, api, inner: Mutex::new(Inner { long, ..Default::default() }), ended: Arc::default() };
        c.measure(&mut c.inner.lock().unwrap());
        c
    }

    /// Where a song's bytes come from for the player: a download, then a
    /// whole cached file, then one the cache fills while it plays.
    pub fn open(&self, s: &Song) -> Option<Arc<Part>> {
        let mut g = self.inner.lock().unwrap();
        if let Some(p) = g.downloaded.get(&s.id).and_then(|f| Part::whole(f, s.id).ok()) {
            return Some(p);
        }
        if let Some(f) = self.complete(s) {
            return Part::whole(&f, s.id).ok();
        }
        if let Some(p) = g.entries.get(&s.id) {
            return Some(p.clone());
        }
        self.start(&mut g, s)
    }

    /// The song is still arriving and the last try to fetch it failed: the player is waiting for the network.
    pub fn failing(&self, song: i64) -> bool {
        self.inner.lock().unwrap().entries.get(&song).is_some_and(|p| p.failing())
    }

    /// A fetch ended since the last call.
    pub fn take_ended(&self) -> bool {
        self.ended.swap(false, Ordering::SeqCst)
    }

    /// Bytes on disk, for Settings.
    pub fn used(&self) -> u64 {
        self.inner.lock().unwrap().used
    }

    /// Whole here: offline, it plays.
    pub fn has(&self, song: i64) -> bool {
        self.inner.lock().unwrap().songs.contains(&song)
    }

    pub fn set_downloaded(&self, files: HashMap<i64, PathBuf>) {
        self.inner.lock().unwrap().downloaded = files;
    }

    /// `longFiles.minMinutes` and `longFiles.cacheAhead`, as they are now.
    pub fn set_long(&self, min_minutes: i64, ahead_minutes: i64) {
        self.inner.lock().unwrap().long = (min_minutes, ahead_minutes);
    }

    /// Moves a cached song into `dir`, as a download (REQUIREMENTS: never
    /// fetched twice); None if it is not all here.
    pub fn promote(&self, s: &Song, dir: &Path) -> Option<PathBuf> {
        let mut g = self.inner.lock().unwrap();
        let from = self.complete(s)?;
        let size = std::fs::metadata(&from).ok()?.len();
        if s.size > 0 && size != s.size as u64 {
            return None;
        }
        let to = dir.join(s.file_name());
        std::fs::rename(&from, &to).ok()?;
        self.measure(&mut g);
        Some(to)
    }

    /// On sign-out: the songs belong to the account being left.
    pub fn remove_all(&self) {
        let mut g = self.inner.lock().unwrap();
        g.entries.clear();
        g.downloaded.clear();
        let _ = std::fs::remove_dir_all(&self.dir);
        let _ = std::fs::create_dir_all(&self.dir);
        self.measure(&mut g);
    }

    /// Looks again at what to fetch: `plan` is the song playing, then those
    /// after it to have ready; `limit` is the cache's size in bytes. A fetch
    /// nothing wants any more stops (a skip, another queue); the playing
    /// song is first in the plan, so it never does. One at a time, in the
    /// plan's order, as the phone fetches. `songs` is the catalogue's, by id.
    pub fn run(&self, plan: &[SongRef], limit: u64, songs: &HashMap<i64, SongRef>) {
        let mut g = self.inner.lock().unwrap();
        let plan: Vec<&SongRef> = plan.iter().filter(|s| !g.downloaded.contains_key(&s.id)).collect();
        let wanted: HashSet<i64> = plan.iter().map(|s| s.id).collect();
        g.entries.retain(|_, p| match p.ended() {
            // With the cache off, a song is fetched only to play it.
            Some(End::Whole) if limit == 0 => {
                if let Some(s) = songs.get(&p.song) {
                    let _ = std::fs::remove_file(self.dir.join(s.file_name()));
                }
                false
            }
            Some(_) => false,
            None => wanted.contains(&p.song),
        });
        self.trim(&mut g, limit, &wanted, songs);
        // A window with all it wants for now does not count: the next file is fetched meanwhile.
        if !g.entries.values().any(|p| p.busy()) {
            let next = plan.iter().find(|s| !g.entries.contains_key(&s.id) && !self.dir.join(s.file_name()).exists());
            if let Some(next) = next.filter(|n| limit > 0 || n.id == plan[0].id) {
                // The next file's window starts at its start; the one playing anchors on its reader.
                if let Some(p) = self.start(&mut g, next).filter(|_| next.id != plan[0].id) {
                    p.anchor_at_start();
                }
            }
        }
        self.measure(&mut g);
    }

    fn start(&self, g: &mut Inner, s: &Song) -> Option<Arc<Part>> {
        if s.size <= 0 {
            return None;
        }
        let window = window(g.long, s);
        let path = self.dir.join(s.file_name() + if window.0 > 0 { ".window" } else { ".part" });
        let p = Part::fetch(self.api.clone(), path, s.id, s.size as u64, window, self.ended.clone()).ok()?;
        g.entries.insert(s.id, p.clone());
        Some(p)
    }

    /// A cached song's whole file, marked as just played; None if it is not all here.
    fn complete(&self, s: &Song) -> Option<PathBuf> {
        let f = self.dir.join(s.file_name());
        let file = std::fs::File::options().write(true).open(&f).ok()?;
        let _ = file.set_modified(SystemTime::now());
        Some(f)
    }

    /// Drops the songs played longest ago until the cache is within `limit`;
    /// never one in the plan. Also drops any song since downloaded, and,
    /// once it has left the plan, a long file an earlier setting kept whole,
    /// which is a window now. Parts and windows are not songs: `run` drops
    /// them with their fetch.
    fn trim(&self, g: &mut Inner, limit: u64, keep: &HashSet<i64>, songs: &HashMap<i64, SongRef>) {
        let mut rest = vec![];
        for f in std::fs::read_dir(&self.dir).into_iter().flatten().flatten() {
            let name = f.file_name().to_string_lossy().into_owned();
            let Some(id) = song_id(&name).filter(|_| is_whole(&name)) else { continue };
            let windowed = songs.get(&id).is_some_and(|s| window(g.long, s).0 > 0);
            if g.downloaded.contains_key(&id) || (windowed && !keep.contains(&id)) {
                let _ = std::fs::remove_file(f.path());
                continue;
            }
            let Ok(m) = f.metadata() else { continue };
            rest.push((f.path(), m.modified().unwrap_or(SystemTime::UNIX_EPOCH), m.len(), keep.contains(&id)));
        }
        rest.sort_by_key(|r| r.1);
        let mut total: u64 = rest.iter().filter(|r| !r.3).map(|r| r.2).sum();
        for (path, _, size, kept) in rest {
            if total <= limit {
                break;
            }
            if !kept {
                let _ = std::fs::remove_file(path);
                total -= size;
            }
        }
    }

    fn measure(&self, g: &mut Inner) {
        let (mut used, mut songs) = (0, HashSet::new());
        for f in std::fs::read_dir(&self.dir).into_iter().flatten().flatten() {
            // Blocks, not length: a window is a sparse file as long as the whole book.
            used += f.metadata().map_or(0, |m| m.blocks() * 512);
            let name = f.file_name().to_string_lossy().into_owned();
            if is_whole(&name) {
                songs.extend(song_id(&name));
            }
        }
        g.used = used;
        g.songs = songs;
    }
}

/// The bytes fetched ahead of the reader and kept behind it: a window for a
/// long file whose size and length are known, else (0, 0), fetched whole.
fn window((min_minutes, ahead): (i64, i64), s: &Song) -> (u64, u64) {
    let per_minute = if s.size > 0 && s.duration_ms > 0 { s.size * 60_000 / s.duration_ms } else { 0 };
    let long = s.spoken() || s.duration_ms >= min_minutes * 60_000;
    if per_minute <= 0 || !long || ahead <= 0 {
        return (0, 0);
    }
    ((ahead * per_minute) as u64, (BEHIND_MINUTES * per_minute) as u64)
}

/// A file by its final name, not a part or a window.
fn is_whole(name: &str) -> bool {
    !PARTIAL.iter().any(|p| name.ends_with(p))
}

fn song_id(name: &str) -> Option<i64> {
    name.split('.').next()?.parse().ok()
}
