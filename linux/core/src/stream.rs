//! A song's bytes as they arrive (plan 019): a file filled by Range
//! requests, which a reader may read anywhere in. A read past what has
//! arrived waits for it and tells the fetcher where it waits; the fetcher
//! jumps there when the reader is behind it or more than 1 MB ahead, as
//! the phone does. A failed request is tried again, so playback waits out
//! a network drop rather than giving up. The Mac's `Fetch` and `SparseFile`.
//!
//! A song is fetched whole into `<id>.<ext>.part` and renamed to
//! `<id>.<ext>` once all of it is here: the song cache keeps it. A long
//! file is a window instead (plan 037), `<id>.<ext>.window`: fetched a
//! number of minutes ahead of the reader, kept ten minutes behind it, and
//! never kept. Behind the window the file's blocks are given back to the
//! disk by punching a hole, where the phone and the Mac copy what they keep
//! into a new file: Linux file systems can, and nothing has to reopen.

use crate::api::{Api, Error};
use std::fs::File;
use std::io::Read;
use std::os::fd::AsRawFd;
use std::os::unix::fs::FileExt;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Condvar, Mutex};
use std::time::Duration;

const CHUNK: usize = 64 * 1024;
/// A reader this far ahead of the fetcher makes it jump, rather than wait for it to get there.
const JUMP: usize = (1 << 20) / CHUNK;
/// A jump to a reader waiting outside the window fetches this much, whatever the window says.
const MIN_FETCH: usize = (256 << 10) / CHUNK;
/// A window that has all it wants is fetched on once the reader has used this much of it.
const REFILL: usize = (1 << 20) / CHUNK;
/// A reader that has read on in one piece this far has settled, and the window
/// anchors there: a decoder finding its place reads a page here and a page
/// there, and a window at each probe would fetch the whole file (plan 037).
const ANCHOR: u64 = 256 << 10;

/// How a fetch ended.
#[derive(Clone, Copy, PartialEq, Debug)]
pub enum End {
    /// All of it is here, kept under its own name.
    Whole,
    /// The server no longer has the song (404, 410): final.
    Gone,
}

#[derive(Default)]
struct State {
    have: Vec<bool>,
    /// A chunk a reader waits on.
    want: Option<usize>,
    /// A flush (seek, stop) woke the readers: they give up.
    interrupted: bool,
    /// The last request failed; the fetch keeps trying.
    failing: bool,
    /// The chunk a window is fetched ahead of and kept behind; None until a reader settles.
    read_at: Option<usize>,
    /// Where the reader's last read ended, and where its run of reads in one piece began.
    last_end: u64,
    run_start: u64,
    /// The fetcher has all it wants for now.
    idle: bool,
    ended: Option<End>,
}

pub struct Part {
    pub song: i64,
    pub len: u64,
    /// The file while it is being filled; None once it is kept, or for a file opened whole.
    path: Mutex<Option<PathBuf>>,
    file: File,
    /// A window's size in chunks, ahead of the reader and behind it; 0 fetches the whole file.
    ahead: usize,
    behind: usize,
    st: Mutex<State>,
    cv: Condvar,
}

impl Part {
    /// A file that is all here (a download, a cached song): read, never fetched.
    pub fn whole(path: &Path, song: i64) -> std::io::Result<Arc<Part>> {
        let file = File::open(path)?;
        let len = file.metadata()?.len();
        let chunks = (len as usize).div_ceil(CHUNK);
        Ok(Arc::new(Part {
            song,
            len,
            path: Mutex::new(None),
            file,
            ahead: 0,
            behind: 0,
            st: Mutex::new(State { have: vec![true; chunks], ended: Some(End::Whole), ..Default::default() }),
            cv: Condvar::new(),
        }))
    }

    /// Starts fetching `song` (`len` bytes) into `path`, a `.part` kept under
    /// its own name once whole, or a `.window` of `window` bytes ahead of
    /// the reader and behind it. `ended` is set when the fetch ends. It also
    /// stops when nothing else holds the part, and the file goes with it.
    pub fn fetch(
        api: Arc<Api>,
        path: PathBuf,
        song: i64,
        len: u64,
        window: (u64, u64),
        ended: Arc<AtomicBool>,
    ) -> std::io::Result<Arc<Part>> {
        let part = Part::create(path, song, len, window)?;
        let p = part.clone();
        std::thread::Builder::new().name(format!("fetch {song}")).spawn(move || fetcher(api, p, ended))?;
        Ok(part)
    }

    fn create(path: PathBuf, song: i64, len: u64, (ahead, behind): (u64, u64)) -> std::io::Result<Arc<Part>> {
        let file = File::options().read(true).write(true).create(true).truncate(true).open(&path)?;
        file.set_len(len)?;
        let chunks = (len as usize).div_ceil(CHUNK);
        let ahead = (ahead as usize).div_ceil(CHUNK);
        Ok(Arc::new(Part {
            song,
            len,
            path: Mutex::new(Some(path)),
            file,
            ahead,
            behind: if ahead > 0 { (behind as usize).div_ceil(CHUNK) } else { 0 },
            st: Mutex::new(State { have: vec![false; chunks], ..Default::default() }),
            cv: Condvar::new(),
        }))
    }

    /// Up to `n` bytes at `pos`, once they are here; None when interrupted,
    /// or when the song is gone from the server.
    pub fn read(&self, pos: u64, n: usize) -> Option<Vec<u8>> {
        let end = (pos + n as u64).min(self.len);
        if pos >= end {
            return Some(vec![]);
        }
        let (first, last) = (pos as usize / CHUNK, (end as usize).div_ceil(CHUNK));
        let mut g = self.st.lock().unwrap();
        if self.ahead > 0 {
            self.reading(&mut g, pos, end);
        }
        loop {
            if g.interrupted || g.ended == Some(End::Gone) {
                return None;
            }
            match (first..last).find(|&c| !g.have[c]) {
                None => break,
                Some(c) => {
                    g.want = Some(c);
                    self.cv.notify_all();
                    g = self.cv.wait(g).unwrap();
                }
            }
        }
        drop(g);
        let mut buf = vec![0; (end - pos) as usize];
        self.file.read_exact_at(&mut buf, pos).ok()?;
        Some(buf)
    }

    /// Follows the reader of a window: it anchors where the reader has read
    /// on in one piece for `ANCHOR`, and the fetcher hears of each move.
    fn reading(&self, g: &mut State, pos: u64, end: u64) {
        if pos != g.last_end {
            g.run_start = pos;
        } else if pos - g.run_start >= ANCHOR {
            g.read_at = Some(pos as usize / CHUNK);
            self.cv.notify_all();
        }
        g.last_end = end;
    }

    /// Where a window starts before any reader has settled: the start, for
    /// the next file's prefetch.
    pub fn anchor_at_start(&self) {
        let mut g = self.st.lock().unwrap();
        if g.read_at.is_none() {
            g.read_at = Some(0);
            self.cv.notify_all();
        }
    }

    /// Wakes the readers, which give up, until it is turned off again.
    pub fn interrupt(&self, on: bool) {
        self.st.lock().unwrap().interrupted = on;
        self.cv.notify_all();
    }

    /// The fetch cannot reach the server at the moment.
    pub fn failing(&self) -> bool {
        self.st.lock().unwrap().failing
    }

    pub fn ended(&self) -> Option<End> {
        self.st.lock().unwrap().ended
    }

    /// Fetching, rather than finished or waiting for the reader to move on.
    pub fn busy(&self) -> bool {
        let g = self.st.lock().unwrap();
        g.ended.is_none() && !g.idle
    }

    fn complete(&self) -> bool {
        self.st.lock().unwrap().have.iter().all(|&h| h)
    }

    /// The next chunk to fetch from `cursor`, or None when nothing is wanted
    /// now. A reader waiting behind the cursor or far ahead of it comes first.
    /// Otherwise a song goes on from the cursor, and a window from its reader
    /// to its end; once full, it waits for the reader to use up `REFILL`.
    fn next(&self, cursor: usize) -> Option<usize> {
        let mut g = self.st.lock().unwrap();
        let n = g.have.len();
        let on = if self.ahead == 0 {
            (cursor..n).chain(0..cursor.min(n)).find(|&c| !g.have[c])
        } else {
            g.read_at.and_then(|r| {
                let a = (r..n).find(|&c| !g.have[c])?;
                let room = if g.idle { self.ahead.saturating_sub(REFILL).max(1) } else { self.ahead };
                (a < r + room).then_some(a)
            })
        };
        let waiting = g.want.filter(|&w| !g.have[w] && (on.is_none() || w < cursor || w > cursor + JUMP));
        let next = waiting.or(on);
        g.idle = next.is_none();
        next
    }

    /// Where a request from chunk `start` ends (exclusive): the file's end
    /// for a song; for a window its end when `start` is inside it, else
    /// `MIN_FETCH` on, enough for a reader waiting elsewhere and no more.
    fn goal(&self, start: usize) -> usize {
        let g = self.st.lock().unwrap();
        let n = g.have.len();
        if self.ahead == 0 {
            return n;
        }
        match g.read_at {
            Some(r) if start >= r && start < r + self.ahead => n.min(r + self.ahead),
            _ => n.min(start + MIN_FETCH),
        }
    }

    /// Forgets what sits more than `behind` before a window's reader, once
    /// more than twice that does, and gives its blocks back to the disk.
    fn drop_behind(&self) {
        let mut g = self.st.lock().unwrap();
        let Some(r) = g.read_at.filter(|_| self.ahead > 0) else { return };
        let keep_from = r.saturating_sub(self.behind);
        let due = g.have[..r.saturating_sub(2 * self.behind).min(g.have.len())].iter().any(|&h| h);
        if !due {
            return;
        }
        g.have[..keep_from].fill(false);
        drop(g);
        // A file system that cannot punch keeps the bytes: only the disk use grows.
        unsafe {
            libc::fallocate(
                self.file.as_raw_fd(),
                libc::FALLOC_FL_PUNCH_HOLE | libc::FALLOC_FL_KEEP_SIZE,
                0,
                (keep_from * CHUNK) as libc::off_t,
            );
        }
    }

    fn put(&self, chunk: usize, bytes: &[u8]) -> std::io::Result<()> {
        self.file.write_all_at(bytes, (chunk * CHUNK) as u64)?;
        let mut g = self.st.lock().unwrap();
        g.have[chunk] = true;
        g.failing = false;
        if g.want == Some(chunk) {
            g.want = None;
        }
        self.cv.notify_all();
        Ok(())
    }

    fn set_failing(&self) {
        self.st.lock().unwrap().failing = true;
    }

    /// A song all here takes its own name; the reader keeps its descriptor across the rename.
    fn end(&self, end: End) {
        if end == End::Whole {
            let mut path = self.path.lock().unwrap();
            let kept = path.as_ref().and_then(|p| p.to_str()?.strip_suffix(".part").map(PathBuf::from));
            if let Some(to) = kept {
                if std::fs::rename(path.as_ref().unwrap(), to).is_ok() {
                    *path = None;
                }
            }
        }
        self.st.lock().unwrap().ended = Some(end);
        self.cv.notify_all();
    }

    /// Waits a little for a reader to move or a flush, while nothing is wanted.
    fn wait(&self) {
        let g = self.st.lock().unwrap();
        let _ = self.cv.wait_timeout(g, Duration::from_secs(1)).unwrap();
    }
}

impl Drop for Part {
    fn drop(&mut self) {
        if let Some(p) = self.path.lock().unwrap().take() {
            let _ = std::fs::remove_file(p);
        }
    }
}

/// Fills `part` until a song is whole, the server no longer has it, or
/// only this thread holds it.
fn fetcher(api: Arc<Api>, part: Arc<Part>, ended: Arc<AtomicBool>) {
    let url = api.stream_url(part.song);
    let mut cursor = 0;
    let mut backoff = 1;
    let alone = |p: &Arc<Part>| Arc::strong_count(p) == 1;
    let end = loop {
        if alone(&part) {
            return;
        }
        if part.ahead == 0 && part.complete() {
            break End::Whole;
        }
        let Some(start) = part.next(cursor) else {
            part.wait();
            continue;
        };
        part.drop_behind();
        let from = (start * CHUNK) as u64;
        let mut body = match api.range(&url, from) {
            Ok(b) => b,
            Err(Error::Http { code: 404 | 410, .. }) => break End::Gone,
            Err(_) => {
                part.set_failing();
                std::thread::sleep(Duration::from_secs(backoff));
                backoff = (backoff * 2).min(8);
                continue;
            }
        };
        backoff = 1;
        let mut c = start;
        let stop = part.goal(start).min(((from + crate::api::RANGE_WINDOW).min(part.len) as usize).div_ceil(CHUNK));
        loop {
            let want = CHUNK.min(part.len as usize - c * CHUNK);
            let mut buf = vec![0; want];
            if body.read_exact(&mut buf).is_err() || part.put(c, &buf).is_err() {
                part.set_failing();
                break;
            }
            c += 1;
            cursor = c;
            if c >= stop || alone(&part) {
                break;
            }
            // A reader jumped elsewhere: drop this response and ask from there.
            if part.next(cursor) != Some(c) {
                break;
            }
        }
    };
    part.end(end);
    ended.store(true, Ordering::SeqCst);
}

#[cfg(test)]
mod tests {
    use super::*;

    fn dir(name: &str) -> PathBuf {
        let d = std::env::temp_dir().join(format!("dhun-{name}-{}", std::process::id()));
        std::fs::create_dir_all(&d).unwrap();
        d
    }

    // A read past what has arrived waits for it, says where it waits, and
    // gives up only when a flush interrupts it; the file goes with the part.
    #[test]
    fn a_reader_waits_for_its_bytes_and_a_flush_wakes_it() {
        let dir = dir("part");
        let part = Part::create(dir.join("7.mp3.part"), 7, (3 * CHUNK + 10) as u64, (0, 0)).unwrap();
        let p = part.clone();
        let reader = std::thread::spawn(move || p.read((2 * CHUNK + 5) as u64, 100));
        while part.st.lock().unwrap().want.is_none() {
            std::thread::yield_now();
        }
        // Close ahead, the fetcher goes on in order; behind it, it jumps back.
        assert_eq!(part.next(0), Some(0));
        assert_eq!(part.next(3), Some(2));
        part.put(2, &vec![9; CHUNK]).unwrap();
        assert_eq!(reader.join().unwrap().unwrap(), vec![9; 100]);
        let p = part.clone();
        let waiting = std::thread::spawn(move || p.read(0, 10));
        while part.st.lock().unwrap().want.is_none() {
            std::thread::yield_now();
        }
        part.interrupt(true);
        assert_eq!(waiting.join().unwrap(), None);
        let path = dir.join("7.mp3.part");
        drop(part);
        assert!(!path.exists());
        let _ = std::fs::remove_dir_all(&dir);
    }

    // A whole song takes its own name and stays when the part goes: the
    // cache keeps it. A window is never kept.
    #[test]
    fn a_whole_song_is_kept_and_a_window_is_not() {
        let dir = dir("keep");
        let part = Part::create(dir.join("7.mp3.part"), 7, 10, (0, 0)).unwrap();
        part.put(0, &[1; 10]).unwrap();
        part.end(End::Whole);
        drop(part);
        assert_eq!(std::fs::read(dir.join("7.mp3")).unwrap(), vec![1; 10]);
        let window = Part::create(dir.join("8.m4b.window"), 8, 10, (CHUNK as u64, 0)).unwrap();
        window.put(0, &[1; 10]).unwrap();
        window.end(End::Whole);
        drop(window);
        assert!(std::fs::read_dir(&dir).unwrap().count() == 1);
        let _ = std::fs::remove_dir_all(&dir);
    }

    // A window waits for a reader to settle, stops its length ahead of it,
    // goes on once the reader has used up `REFILL` of it, and a reader
    // waiting far away gets `MIN_FETCH` and no more (plan 037).
    #[test]
    fn a_window_follows_the_reader() {
        let dir = dir("window");
        let ahead = 2 * REFILL;
        let part =
            Part::create(dir.join("1.m4b.window"), 1, (200 * CHUNK) as u64, ((ahead * CHUNK) as u64, 0)).unwrap();
        assert_eq!(part.next(0), None, "no reader yet");
        part.anchor_at_start();
        assert_eq!(part.next(0), Some(0));
        assert_eq!(part.goal(0), ahead);
        for c in 0..ahead {
            part.put(c, &vec![0; CHUNK]).unwrap();
        }
        assert_eq!(part.next(ahead), None, "the window is full");
        part.st.lock().unwrap().read_at = Some(REFILL - 1);
        assert_eq!(part.next(ahead), None, "less than REFILL used");
        part.st.lock().unwrap().read_at = Some(REFILL + 1);
        assert_eq!(part.next(ahead), Some(ahead));
        assert_eq!(part.goal(ahead), REFILL + 1 + ahead);
        // Reads in one piece settle the reader; a probe elsewhere does not.
        let mut g = part.st.lock().unwrap();
        part.reading(&mut g, (150 * CHUNK) as u64, (150 * CHUNK) as u64 + 4096);
        assert_eq!(g.read_at, Some(REFILL + 1));
        g.want = Some(150);
        drop(g);
        assert_eq!(part.next(ahead), Some(150));
        assert_eq!(part.goal(150), 150 + MIN_FETCH);
        let _ = std::fs::remove_dir_all(&dir);
    }

    // Once more than twice `behind` sits before the reader, all but `behind`
    // of it is forgotten; what is kept still reads.
    #[test]
    fn a_window_forgets_what_is_far_behind() {
        let dir = dir("behind");
        let part =
            Part::create(dir.join("1.m4b.window"), 1, (64 * CHUNK) as u64, ((32 * CHUNK) as u64, (4 * CHUNK) as u64))
                .unwrap();
        for c in 0..32 {
            part.put(c, &vec![c as u8; CHUNK]).unwrap();
        }
        part.st.lock().unwrap().read_at = Some(8);
        part.drop_behind();
        assert!(part.st.lock().unwrap().have[0], "only 8 behind: kept");
        part.st.lock().unwrap().read_at = Some(17);
        part.drop_behind();
        let have = part.st.lock().unwrap().have.clone();
        assert!(!have[..13].iter().any(|&h| h) && have[13..32].iter().all(|&h| h));
        assert_eq!(part.read((13 * CHUNK) as u64, 3).unwrap(), vec![13; 3]);
        let _ = std::fs::remove_dir_all(&dir);
    }
}
