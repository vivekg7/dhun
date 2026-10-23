//! A song's bytes as they arrive (plan 019): a file filled by Range
//! requests, which a reader may read anywhere in. A read past what has
//! arrived waits for it and tells the fetcher where it waits; the fetcher
//! jumps there when the reader is behind it or more than 1 MB ahead, as
//! the phone does. A failed request is tried again, so playback waits out
//! a network drop rather than giving up. The Mac's `Fetch` and `SparseFile`.

use crate::api::Api;
use std::fs::File;
use std::io::Read;
use std::os::unix::fs::FileExt;
use std::path::PathBuf;
use std::sync::{Arc, Condvar, Mutex};
use std::time::Duration;

const CHUNK: usize = 64 * 1024;
/// A reader this far ahead of the fetcher makes it jump, rather than wait for it to get there.
const JUMP: usize = (1 << 20) / CHUNK;

#[derive(Default)]
struct State {
    have: Vec<bool>,
    /// A chunk a reader waits on.
    want: Option<usize>,
    /// A flush (seek, stop) woke the readers: they give up.
    interrupted: bool,
    /// The last request failed; the fetch keeps trying.
    failing: bool,
}

pub struct Part {
    pub song: i64,
    pub len: u64,
    path: PathBuf,
    file: File,
    st: Mutex<State>,
    cv: Condvar,
}

impl Part {
    /// Starts fetching `song` (`len` bytes) into `dir`. The fetch stops when
    /// the file is complete or nothing else holds the part any more.
    pub fn fetch(api: Arc<Api>, dir: &std::path::Path, song: i64, len: u64) -> std::io::Result<Arc<Part>> {
        let part = Part::create(dir, song, len)?;
        let p = part.clone();
        std::thread::Builder::new().name(format!("fetch {song}")).spawn(move || fetcher(api, p))?;
        Ok(part)
    }

    fn create(dir: &std::path::Path, song: i64, len: u64) -> std::io::Result<Arc<Part>> {
        std::fs::create_dir_all(dir)?;
        let path = dir.join(format!("{song}.{}.part", crate::store::uuid()));
        let file = File::options().read(true).write(true).create(true).truncate(true).open(&path)?;
        file.set_len(len)?;
        let chunks = (len as usize).div_ceil(CHUNK);
        Ok(Arc::new(Part {
            song,
            len,
            path,
            file,
            st: Mutex::new(State { have: vec![false; chunks], ..Default::default() }),
            cv: Condvar::new(),
        }))
    }

    /// Up to `n` bytes at `pos`, once they are here; None when interrupted.
    pub fn read(&self, pos: u64, n: usize) -> Option<Vec<u8>> {
        let end = (pos + n as u64).min(self.len);
        if pos >= end {
            return Some(vec![]);
        }
        let (first, last) = (pos as usize / CHUNK, (end as usize).div_ceil(CHUNK));
        let mut g = self.st.lock().unwrap();
        loop {
            if g.interrupted {
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

    /// Wakes the readers, which give up, until it is turned off again.
    pub fn interrupt(&self, on: bool) {
        self.st.lock().unwrap().interrupted = on;
        self.cv.notify_all();
    }

    /// The fetch cannot reach the server at the moment.
    pub fn failing(&self) -> bool {
        self.st.lock().unwrap().failing
    }

    pub fn complete(&self) -> bool {
        self.st.lock().unwrap().have.iter().all(|&h| h)
    }

    /// The next chunk to fetch from `cursor`: a waiting reader's, when it is
    /// behind the cursor or far ahead of it, else the next missing one.
    fn next(&self, cursor: usize) -> Option<usize> {
        let g = self.st.lock().unwrap();
        if let Some(w) = g.want.filter(|&w| !g.have[w] && (w < cursor || w > cursor + JUMP)) {
            return Some(w);
        }
        let n = g.have.len();
        (cursor..n).chain(0..cursor.min(n)).find(|&c| !g.have[c])
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
}

impl Drop for Part {
    fn drop(&mut self) {
        let _ = std::fs::remove_file(&self.path);
    }
}

/// Fills `part` until it is complete or only this thread holds it.
fn fetcher(api: Arc<Api>, part: Arc<Part>) {
    let url = api.stream_url(part.song);
    let mut cursor = 0;
    let mut backoff = 1;
    let alone = |p: &Arc<Part>| Arc::strong_count(p) == 1;
    while let Some(start) = part.next(cursor) {
        if alone(&part) {
            return;
        }
        let from = (start * CHUNK) as u64;
        let mut body = match api.range(&url, from) {
            Ok(b) => b,
            Err(_) => {
                part.set_failing();
                std::thread::sleep(Duration::from_secs(backoff));
                backoff = (backoff * 2).min(8);
                continue;
            }
        };
        backoff = 1;
        let mut c = start;
        let window_end = ((from + crate::api::RANGE_WINDOW).min(part.len) as usize).div_ceil(CHUNK);
        loop {
            let want = CHUNK.min(part.len as usize - c * CHUNK);
            let mut buf = vec![0; want];
            if body.read_exact(&mut buf).is_err() || part.put(c, &buf).is_err() {
                part.set_failing();
                break;
            }
            c += 1;
            cursor = c;
            if c >= window_end || alone(&part) {
                break;
            }
            // A reader jumped elsewhere: drop this response and ask from there.
            if part.next(cursor) != Some(c) {
                break;
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // A read past what has arrived waits for it, says where it waits, and
    // gives up only when a flush interrupts it; the file goes with the part.
    #[test]
    fn a_reader_waits_for_its_bytes_and_a_flush_wakes_it() {
        let dir = std::env::temp_dir().join(format!("dhun-part-{}", std::process::id()));
        let part = Part::create(&dir, 7, (3 * CHUNK + 10) as u64).unwrap();
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
        let path = part.path.clone();
        drop(part);
        assert!(!path.exists());
        let _ = std::fs::remove_dir(&dir);
    }
}
