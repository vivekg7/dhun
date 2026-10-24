//! Downloads (plan 012) on the main thread, as the Mac's `Downloads`: the
//! pins are turned into songs again whenever the library or the lists
//! change, and the files on disk follow. New songs are fetched one at a
//! time on a worker thread, songs no pin covers any more are deleted.

use crate::app::{App, Change};
use dhun_core::api;
use dhun_core::catalog::SongRef;
use dhun_core::downloads::{self as dl, RESERVE};
use dhun_core::model::{Download, Pin};
use dhun_core::store::{FAV, LATER};
use dhun_core::time::now_ms;
use gtk::prelude::*;
use gtk::{gio, glib};
use std::cell::{Cell, RefCell};
use std::collections::HashSet;
use std::path::PathBuf;
use std::rc::Rc;
use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::Duration;

#[derive(Clone, Copy, PartialEq, Default, Debug)]
pub enum State {
    #[default]
    Idle,
    Downloading,
    NoNetwork,
    /// Waiting for a network that is not metered.
    Metered,
    Full,
    NoSpace,
    Failed,
}

#[derive(Clone, Default)]
pub struct Status {
    pub state: State,
    /// Songs covered by pins, and how many of them are on disk.
    pub wanted: usize,
    pub done: usize,
    pub used: u64,
    /// Full or no space: how much more the rest needs.
    pub need: u64,
    pub error: String,
    pub current: Option<SongRef>,
    pub progress: f64,
}

pub struct Downloads {
    app: Rc<App>,
    pub dir: PathBuf,
    pub status: RefCell<Status>,
    running: Cell<bool>,
    again: Cell<bool>,
}

impl Downloads {
    pub fn new(app: &Rc<App>) -> Rc<Downloads> {
        let dir = app.dir.join("downloads");
        let _ = std::fs::create_dir_all(&dir);
        let d = Rc::new(Downloads {
            app: app.clone(),
            dir,
            status: RefCell::default(),
            running: Cell::new(false),
            again: Cell::new(false),
        });
        app.reload_downloads();
        let w = Rc::downgrade(&d);
        app.listen(move |c| {
            let Some(d) = w.upgrade() else { return false };
            if matches!(c, Change::Catalog | Change::Playlists) {
                d.poke();
            }
            true
        });
        let w = Rc::downgrade(&d);
        gio::NetworkMonitor::default().connect_network_metered_notify(move |_| {
            if let Some(d) = w.upgrade() {
                d.poke();
            }
        });
        d
    }

    pub fn pin(self: &Rc<Self>, kind: &str, r#ref: &str, name: &str) {
        let p =
            Pin { key: dl::key(kind, r#ref), kind: kind.into(), r#ref: r#ref.into(), name: name.into(), at: now_ms() };
        let _ = self.app.db.lock().unwrap().put_pin(&p);
        self.app.reload_pins();
        self.poke();
    }

    pub fn unpin(self: &Rc<Self>, key: &str) {
        let _ = self.app.db.lock().unwrap().delete_pin(key);
        self.app.reload_pins();
        self.poke();
    }

    pub fn pinned(&self, kind: &str, r#ref: &str) -> bool {
        let key = dl::key(kind, r#ref);
        self.app.pins.borrow().iter().any(|p| p.key == key)
    }

    /// The songs one download covers, for its button's "3/12".
    pub fn songs(&self, kind: &str, r#ref: &str) -> Vec<SongRef> {
        let key = dl::key(kind, r#ref);
        let pins: Vec<Pin> = self.app.pins.borrow().iter().filter(|p| p.key == key).cloned().collect();
        self.covered(&pins).unwrap_or_default()
    }

    fn covered(&self, pins: &[Pin]) -> Option<Vec<SongRef>> {
        let db = self.app.db.lock().unwrap();
        let (favs, later) = (db.marks(FAV).unwrap_or_default(), db.marks(LATER).unwrap_or_default());
        drop(db);
        dl::covered(pins, &self.app.catalog(), &self.app.playlists.borrow(), &favs, &later)
    }

    /// Looks at the pins and the files again: after a change, a new
    /// network, a sync that reached the server.
    pub fn poke(self: &Rc<Self>) {
        if self.running.get() {
            self.again.set(true);
            return;
        }
        let d = self.clone();
        glib::spawn_future_local(async move { d.run().await });
    }

    /// On sign-out: the files belong to the account being left.
    pub fn remove_all(&self) {
        let _ = std::fs::remove_dir_all(&self.dir);
        let _ = std::fs::create_dir_all(&self.dir);
        *self.status.borrow_mut() = Status::default();
    }

    async fn run(self: &Rc<Self>) {
        self.running.set(true);
        self.fetch_all().await;
        self.running.set(false);
        if self.again.replace(false) {
            self.poke();
        }
    }

    async fn fetch_all(self: &Rc<Self>) {
        let pins = self.app.pins.borrow().clone();
        let Some(songs) = self.covered(&pins) else { return };
        let want: std::collections::HashMap<i64, &SongRef> = songs.iter().map(|s| (s.id, s)).collect();
        // Files nothing covers any more: removed from a playlist, unpinned, gone from the library.
        {
            let db = self.app.db.lock().unwrap();
            for d in self.app.downloaded.borrow().values() {
                if !want.contains_key(&d.song) {
                    let _ = std::fs::remove_file(&d.path);
                    let _ = db.delete_download(d.song);
                } else if !std::path::Path::new(&d.path).exists() {
                    // Deleted behind our back: fetch it again.
                    let _ = db.delete_download(d.song);
                }
            }
        }
        self.app.reload_downloads();
        let have = |id: i64| self.app.downloaded.borrow().get(&id).map(|d| d.size);
        let todo: Vec<SongRef> =
            songs.iter().filter(|s| have(s.id).is_none_or(|size| s.size > 0 && size != s.size)).cloned().collect();
        let mut used: u64 = songs.iter().filter_map(|s| have(s.id)).map(|n| n.max(0) as u64).sum();
        // A part kept for carrying on whose song is no longer wanted.
        let resumable: HashSet<String> = todo.iter().map(|s| s.file_name() + ".part").collect();
        for f in std::fs::read_dir(&self.dir).into_iter().flatten().flatten() {
            let name = f.file_name().to_string_lossy().to_string();
            if name.ends_with(".part") && !resumable.contains(&name) {
                let _ = std::fs::remove_file(f.path());
            }
        }
        self.update(State::Idle, &songs, used, 0);
        if !self.app.signed_in() {
            return;
        }
        let prefs = self.app.prefs.get();
        for (i, s) in todo.iter().enumerate() {
            // Something changed while the last file was fetched: start over, so the newest pin goes first.
            if i > 0 && self.again.get() {
                return;
            }
            if self.app.offline.get() {
                return self.update(State::NoNetwork, &songs, used, 0);
            }
            if gio::NetworkMonitor::default().is_network_metered() && prefs.download_on_metered != Some(true) {
                return self.update(State::Metered, &songs, used, 0);
            }
            let size = s.size.max(0) as u64;
            let replacing = have(s.id).map_or(0, |n| n.max(0) as u64);
            let limit = prefs.download_limit();
            if prefs.download_limit_gb != Some(0) && used - replacing + size > limit {
                let rest: u64 = todo[i..].iter().map(|s| s.size.max(0) as u64).sum();
                return self.update(State::Full, &songs, used, used + rest - limit);
            }
            let free = free_space(&self.dir);
            if free < size + RESERVE {
                return self.update(State::NoSpace, &songs, used, size + RESERVE - free);
            }
            {
                let mut st = self.status.borrow_mut();
                st.state = State::Downloading;
                st.current = Some(s.clone());
                st.progress = 0.0;
            }
            self.app.changed(Change::Downloads);
            // A song in the song cache moves here rather than being fetched again.
            let file = match self.app.cache.promote(s, &self.dir) {
                Some(f) => Some(f),
                None => match self.fetch(s).await {
                    Ok(f) => f,
                    Err(e) => return self.fail(e),
                },
            };
            let Some(file) = file else { continue };
            let path = file.to_string_lossy().to_string();
            if let Some(old) = self.app.downloaded.borrow().get(&s.id).filter(|d| d.path != path) {
                let _ = std::fs::remove_file(&old.path);
            }
            let d = Download { song: s.id, path, size: s.size, at: now_ms() };
            let _ = self.app.db.lock().unwrap().put_download(&d);
            self.app.reload_downloads();
            used = used - replacing + size;
            let mut st = self.status.borrow_mut();
            st.done += 1;
            st.used = used;
        }
        // Their lyrics too, so a downloaded song shows them offline (plan 015).
        let kept: Vec<SongRef> = songs.iter().filter(|s| have(s.id).is_some()).cloned().collect();
        let (api, db) = (self.app.api.clone(), self.app.db.clone());
        let _ = gio::spawn_blocking(move || dhun_core::lyrics::keep(&api, &db, &kept)).await;
        self.update(State::Idle, &songs, used, 0);
    }

    /// One song, on a worker thread, its progress shown twice a second.
    async fn fetch(self: &Rc<Self>, s: &SongRef) -> api::Result<Option<PathBuf>> {
        let got = Arc::new(AtomicU64::new(0));
        let (g, w) = (got.clone(), Rc::downgrade(self));
        let size = s.size.max(1) as f64;
        let ticker = glib::timeout_add_local(Duration::from_millis(500), move || {
            let Some(d) = w.upgrade() else { return glib::ControlFlow::Break };
            d.status.borrow_mut().progress = g.load(Ordering::Relaxed) as f64 / size;
            d.app.changed(Change::Downloads);
            glib::ControlFlow::Continue
        });
        let (api, song, dir) = (self.app.api.clone(), s.clone(), self.dir.clone());
        let r = gio::spawn_blocking(move || dl::fetch(&api, &song, &dir, |n| got.store(n, Ordering::Relaxed))).await;
        ticker.remove();
        r.unwrap_or_else(|_| Err(api::Error::Network("The download stopped".into())))
    }

    fn fail(self: &Rc<Self>, e: api::Error) {
        {
            let mut st = self.status.borrow_mut();
            st.state = State::Failed;
            st.error = e.to_string();
            st.current = None;
        }
        self.app.changed(Change::Downloads);
        // A dropped connection mid-file: try again shortly.
        let d = self.clone();
        glib::timeout_add_seconds_local_once(30, move || d.poke());
    }

    fn update(&self, state: State, songs: &[SongRef], used: u64, need: u64) {
        let downloaded = self.app.downloaded.borrow();
        let done = songs.iter().filter(|s| downloaded.contains_key(&s.id)).count();
        drop(downloaded);
        *self.status.borrow_mut() = Status { state, wanted: songs.len(), done, used, need, ..Default::default() };
        self.app.changed(Change::Downloads);
    }
}

/// Free bytes on the disk that holds `dir`; unknown reads as plenty.
fn free_space(dir: &std::path::Path) -> u64 {
    gio::File::for_path(dir)
        .query_filesystem_info("filesystem::free", gio::Cancellable::NONE)
        .map_or(u64::MAX, |i| i.attribute_uint64("filesystem::free"))
}
