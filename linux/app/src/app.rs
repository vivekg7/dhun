//! The app's state on the main thread: the working copy, the catalogue in
//! memory, and the sync schedule (plan 006), as the Mac's `AppModel` and
//! `Sync`. Work that can wait on the network or touch every song runs on a
//! worker thread through `gio::spawn_blocking`; the main thread only swaps
//! in what it built.

use dhun_core::api::Api;
use dhun_core::cache::SongCache;
use dhun_core::catalog::{Catalog, SongRef};
use dhun_core::covers::{self, Files};
use dhun_core::db::Db;
use dhun_core::model::{Download, NowPlaying, Pin, Playlist, QueueRow};
use dhun_core::prefs::Prefs;
use dhun_core::synced::Settings;
use dhun_core::{store, sync};
use gtk::prelude::*;
use gtk::{gdk, gio, glib};
use std::cell::{Cell, RefCell};
use std::collections::{HashMap, HashSet, VecDeque};
use std::path::PathBuf;
use std::rc::Rc;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

/// What changed, for the views that show it.
#[derive(Clone, Copy, PartialEq, Debug)]
pub enum Change {
    Catalog,
    Playlists,
    Thumbs,
    Status,
    Account,
    /// The queues, or the synced settings, changed.
    Queues,
    /// The song, play or pause, the queue's songs.
    Playback,
    /// The place in the song, four times a second while playing.
    Position,
    /// Another device's playback, for hand-off.
    Handoff,
    /// Another sidebar section is showing.
    Section,
    /// The pins, or the downloads' progress.
    Downloads,
    /// Which songs play without the server: a download done or gone, or the server lost or found.
    Playable,
    /// Favorites, Listen Later, the played marks or the bookmarks.
    Marks,
}

/// The songs in Favorites and Listen Later, newest first, and those played
/// (plans 010, 031).
#[derive(Default)]
pub struct Marks {
    pub favs: Vec<i64>,
    pub later: Vec<i64>,
    pub played: HashSet<i64>,
    /// Every long file's bookmarks, by place (plan 033).
    pub bookmarks: Vec<dhun_core::model::Bookmark>,
}

impl Marks {
    pub fn has(&self, kind: &str, song: i64) -> bool {
        match kind {
            store::FAV => self.favs.contains(&song),
            store::LATER => self.later.contains(&song),
            _ => self.played.contains(&song),
        }
    }
}

type Listener = Rc<dyn Fn(Change) -> bool>;
type CoverWaiter = Box<dyn FnOnce(&gdk::Texture)>;

pub struct App {
    /// The app's folder: the database, covers, songs arriving.
    pub dir: PathBuf,
    pub prefs: Arc<Prefs>,
    pub api: Arc<Api>,
    pub db: Arc<Mutex<Db>>,
    pub files: Arc<Files>,
    pub catalog: RefCell<Arc<Catalog>>,
    pub playlists: RefCell<Vec<Playlist>>,
    pub queues: RefCell<Vec<QueueRow>>,
    pub settings: RefCell<Settings>,
    pub marks: RefCell<Marks>,
    pub now_playing: RefCell<Option<NowPlaying>>,
    pub playback: std::cell::OnceCell<Rc<crate::playback::Playback>>,
    pub cache: Arc<SongCache>,
    pub downloads: std::cell::OnceCell<Rc<crate::downloads::Downloads>>,
    pub pins: RefCell<Vec<Pin>>,
    pub downloaded: RefCell<HashMap<i64, Download>>,
    pub syncing: Cell<bool>,
    /// The server could not be reached on the last try.
    pub offline: Cell<bool>,
    pub sync_error: RefCell<Option<String>>,
    listeners: RefCell<Vec<Listener>>,
    pending: RefCell<Option<glib::SourceId>>,
    running: Cell<bool>,
    again: Cell<bool>,
    retry: Cell<u64>,
    thumbs: RefCell<HashMap<String, Option<gdk::Texture>>>,
    covers: RefCell<Covers>,
}

#[derive(Default)]
struct Covers {
    kept: HashMap<String, gdk::Texture>,
    order: VecDeque<String>,
    bytes: usize,
    waiting: HashMap<String, Vec<CoverWaiter>>,
}

/// Decoded covers kept in memory, by their pixels: 128 covers at 256 px.
const COVER_BYTES: usize = 32 << 20;

// A blip is over in seconds; the 15 minutes this once grew to on the phone
// left it offline long after the network was back (plan 019).
const RETRY_MIN: u64 = 5;
const RETRY_MAX: u64 = 120;

impl App {
    pub fn new(dir: PathBuf) -> Rc<App> {
        std::fs::create_dir_all(&dir).expect("the app's data folder");
        let prefs = Arc::new(Prefs::open(&dir));
        let db = Db::open(&dir.join("dhun.db")).expect("the app's database");
        let api = Arc::new(Api::new(prefs.clone()));
        let pins = db.pins().unwrap_or_default();
        Rc::new(App {
            dir: dir.clone(),
            cache: Arc::new(SongCache::new(api.clone(), dir.join("cache"))),
            api,
            prefs,
            db: Arc::new(Mutex::new(db)),
            files: Arc::new(Files::new(dir.join("covers"))),
            catalog: RefCell::new(Arc::new(Catalog::empty())),
            playlists: RefCell::new(vec![]),
            queues: RefCell::new(vec![]),
            settings: RefCell::new(Settings::new()),
            marks: RefCell::default(),
            now_playing: RefCell::new(None),
            playback: std::cell::OnceCell::new(),
            downloads: std::cell::OnceCell::new(),
            pins: RefCell::new(pins),
            downloaded: RefCell::default(),
            syncing: Cell::new(false),
            offline: Cell::new(false),
            sync_error: RefCell::new(None),
            listeners: RefCell::new(vec![]),
            pending: RefCell::new(None),
            running: Cell::new(false),
            again: Cell::new(false),
            retry: Cell::new(RETRY_MIN),
            thumbs: RefCell::new(HashMap::new()),
            covers: RefCell::new(Covers::default()),
        })
    }

    pub fn signed_in(&self) -> bool {
        !self.prefs.token().is_empty()
    }

    /// Calls `f` on every change while it returns true: a view's listener
    /// returns false once the view is gone.
    pub fn listen(&self, f: impl Fn(Change) -> bool + 'static) {
        self.listeners.borrow_mut().push(Rc::new(f));
    }

    /// Tells every listener. A listener may announce a change of its own,
    /// which reaches them all too: they are called from a copy of the list.
    pub fn changed(&self, c: Change) {
        let all = self.listeners.borrow().clone();
        let done: Vec<Listener> = all.into_iter().filter(|f| !f(c)).collect();
        if !done.is_empty() {
            self.listeners.borrow_mut().retain(|f| !done.iter().any(|d| Rc::ptr_eq(d, f)));
        }
    }

    pub fn catalog(&self) -> Arc<Catalog> {
        self.catalog.borrow().clone()
    }

    /// Reads the songs and builds the catalogue off the main thread, then
    /// swaps it in, and builds the first search indexes after, so the
    /// library shows before them and the first keystroke finds them ready
    /// (plan 036).
    pub async fn reload_songs(self: &Rc<Self>) {
        let db = self.db.clone();
        let built = gio::spawn_blocking(move || {
            let t = Instant::now();
            let songs = db.lock().unwrap().songs().unwrap_or_default();
            let read = t.elapsed();
            let t = Instant::now();
            let c = Catalog::new(songs);
            glib::g_info!("Dhun", "catalogue: {} songs, read {:?}, built {:?}", c.by_id.len(), read, t.elapsed());
            Arc::new(c)
        })
        .await;
        let Ok(c) = built else { return };
        *self.catalog.borrow_mut() = c.clone();
        self.changed(Change::Catalog);
        let _ = gio::spawn_blocking(move || {
            let t = Instant::now();
            let _ = c.song_index();
            let _ = c.album_index();
            glib::g_info!("Dhun", "search indexes: {:?}", t.elapsed());
        })
        .await;
    }

    pub fn playback(&self) -> &Rc<crate::playback::Playback> {
        self.playback.get().expect("playback is made at start")
    }

    pub fn reload_queues(&self) {
        let db = self.db.lock().unwrap();
        *self.queues.borrow_mut() = db.queues().unwrap_or_default();
        *self.settings.borrow_mut() = db.settings().unwrap_or_default();
        drop(db);
        self.changed(Change::Queues);
    }

    /// A change to the user's synced data, made through `dhun_core::store`:
    /// in the working copy at once, in the outbox for the next sync. An
    /// edit the outbox could not take is said where sync errors show.
    pub fn store(self: &Rc<Self>, f: impl FnOnce(&mut Db) -> dhun_core::db::Result<()>) {
        let r = f(&mut self.db.lock().unwrap());
        if let Err(e) = r {
            *self.sync_error.borrow_mut() = Some(format!("A change could not be kept for sending: {e}"));
            self.changed(Change::Status);
        }
        self.sync_soon(Duration::from_secs(2));
    }

    pub fn downloads(&self) -> &Rc<crate::downloads::Downloads> {
        self.downloads.get().expect("downloads are made at start")
    }

    pub fn reload_pins(&self) {
        *self.pins.borrow_mut() = self.db.lock().unwrap().pins().unwrap_or_default();
        self.changed(Change::Downloads);
    }

    /// The songs in Downloads, which the player opens before the cache.
    pub fn reload_downloads(&self) {
        let now = self.db.lock().unwrap().downloads().unwrap_or_default();
        let same = {
            let before = self.downloaded.borrow();
            before.len() == now.len() && now.keys().all(|k| before.contains_key(k))
        };
        self.cache.set_downloaded(now.values().map(|d| (d.song, PathBuf::from(&d.path))).collect());
        *self.downloaded.borrow_mut() = now;
        if !same {
            self.changed(Change::Playable);
        }
    }

    /// Whether a song can play now: the server is there, or the song is on this computer.
    pub fn playable(&self, song: i64) -> bool {
        !self.offline.get() || self.downloaded.borrow().contains_key(&song) || self.cache.has(song)
    }

    fn set_offline(&self, offline: bool) {
        if self.offline.replace(offline) != offline {
            self.changed(Change::Playable);
        }
    }

    /// The songs kept here belong to the account being left.
    fn forget_files(&self) {
        if let Some(d) = self.downloads.get() {
            d.remove_all();
        }
        self.cache.remove_all();
        self.pins.borrow_mut().clear();
        self.downloaded.borrow_mut().clear();
        self.changed(Change::Downloads);
        self.changed(Change::Playable);
    }

    pub fn reload_marks(&self) {
        let db = self.db.lock().unwrap();
        let ids = |kind| db.marks(kind).unwrap_or_default().into_iter().map(|m| m.song);
        let m = Marks {
            favs: ids(store::FAV).collect(),
            later: ids(store::LATER).collect(),
            played: ids(store::PLAYED).collect(),
            bookmarks: db.bookmarks().unwrap_or_default(),
        };
        drop(db);
        *self.marks.borrow_mut() = m;
        self.changed(Change::Marks);
    }

    /// What has been heard of the spoken files: played, and where each was left (plan 031).
    pub fn heard(&self) -> dhun_core::spoken::Heard {
        let resumes = self.db.lock().unwrap().resumes().unwrap_or_default();
        dhun_core::spoken::Heard {
            played: self.marks.borrow().played.clone(),
            left: resumes.into_iter().map(|r| (r.song, r.position_ms)).collect(),
        }
    }

    /// Puts `songs` into a list or takes them out: Favorites, Listen Later, played.
    pub fn mark(self: &Rc<Self>, kind: &str, songs: &[i64], on: bool) {
        self.store(|db| songs.iter().try_for_each(|&s| store::mark(db, kind, s, on)));
        self.reload_marks();
    }

    pub fn reload_playlists(&self) {
        *self.playlists.borrow_mut() = self.db.lock().unwrap().playlists().unwrap_or_default();
        self.changed(Change::Playlists);
    }

    /// Debounced: a burst of events becomes one sync. A failed sync retries
    /// with a growing delay while the app runs.
    pub fn sync_soon(self: &Rc<Self>, after: Duration) {
        if let Some(id) = self.pending.borrow_mut().take() {
            id.remove();
        }
        let app = self.clone();
        let id = glib::timeout_add_local_once(after, move || {
            app.pending.borrow_mut().take();
            let app2 = app.clone();
            glib::spawn_future_local(async move {
                if app2.sync_now().await {
                    app2.retry.set(RETRY_MIN);
                } else {
                    let d = (app2.retry.get() * 2).min(RETRY_MAX);
                    app2.retry.set(d);
                    app2.sync_soon(Duration::from_secs(d));
                }
            });
        });
        *self.pending.borrow_mut() = Some(id);
    }

    /// One at a time; a request while one runs makes it run once more.
    /// False when the server could not be reached, so it is tried again.
    pub async fn sync_now(self: &Rc<Self>) -> bool {
        if !self.signed_in() {
            return true;
        }
        if self.running.get() {
            self.again.set(true);
            return true;
        }
        self.running.set(true);
        self.syncing.set(true);
        self.changed(Change::Status);
        let mut ok;
        loop {
            self.again.set(false);
            ok = self.round().await;
            if !(self.again.get() && ok) {
                break;
            }
        }
        self.running.set(false);
        self.syncing.set(false);
        self.changed(Change::Status);
        ok
    }

    async fn round(self: &Rc<Self>) -> bool {
        let (api, db) = (self.api.clone(), self.db.clone());
        let pulled = gio::spawn_blocking(move || sync::round(&api, &db)).await;
        let pulled = match pulled {
            Ok(Ok(p)) => p,
            Ok(Err(e)) => {
                self.set_offline(e.code() == 0);
                *self.sync_error.borrow_mut() = Some(e.to_string());
                // A revoked token (password reset, device removed): sign in
                // again, but keep everything, unsent edits included (plan 023).
                if e.code() == 401 {
                    self.prefs.set_token("");
                    self.changed(Change::Account);
                }
                return e.code() != 0 && !(500..600).contains(&e.code());
            }
            Err(_) => return false,
        };
        self.set_offline(false);
        *self.sync_error.borrow_mut() = None;
        if pulled.playlists || pulled.refused {
            self.reload_playlists();
        }
        if pulled.state {
            self.reload_marks();
            self.reload_queues();
            if let Some(p) = self.playback.get() {
                p.queues_synced();
            }
        }
        if let Some(np) = pulled.now_playing {
            *self.now_playing.borrow_mut() = Some(np);
            self.changed(Change::Handoff);
        }
        if pulled.songs > 0 || self.catalog.borrow().by_id.is_empty() {
            self.reload_songs().await;
        }
        // The server is there: whatever waited for it can go on.
        if let Some(d) = self.downloads.get() {
            d.poke();
        }
        let (api, db) = (self.api.clone(), self.db.clone());
        let app = self.clone();
        glib::spawn_future_local(async move {
            if let Ok(n) = gio::spawn_blocking(move || covers::fetch_thumbs(&api, &db)).await {
                if n > 0 {
                    app.thumbs.borrow_mut().retain(|_, t| t.is_some());
                    app.changed(Change::Thumbs);
                }
            }
        });
        true
    }

    /// A cover's 128 px thumbnail from the database, None until it has been
    /// fetched. Cheap enough for a list row.
    pub fn thumbnail(&self, s: &SongRef) -> Option<gdk::Texture> {
        let s = self.catalog().art_of(s).clone();
        if !s.has_art || s.art.is_empty() {
            return None;
        }
        if let Some(t) = self.thumbs.borrow().get(&s.art) {
            return t.clone();
        }
        let data = self.db.lock().unwrap().thumb(&s.art).ok().flatten();
        let t = data.filter(|d| !d.is_empty()).and_then(|d| gdk::Texture::from_bytes(&glib::Bytes::from_owned(d)).ok());
        self.thumbs.borrow_mut().insert(s.art.clone(), t.clone());
        t
    }

    /// A cover at least `px` across, given to `done` once it is here: at
    /// once from memory, else fetched and decoded off the main thread. One
    /// request per cover, however many views want it.
    pub fn cover(self: &Rc<Self>, s: &SongRef, px: u32, done: impl FnOnce(&gdk::Texture) + 'static) {
        let s = self.catalog().art_of(s).clone();
        if !s.has_art {
            return;
        }
        let size = covers::size_for(px);
        let name = format!("{}.{size}", covers::key(&s));
        {
            let mut c = self.covers.borrow_mut();
            // Given after the borrow ends: `done` may ask for a cover itself.
            if let Some(t) = c.kept.get(&name).cloned() {
                drop(c);
                done(&t);
                return;
            }
            if let Some(w) = c.waiting.get_mut(&name) {
                w.push(Box::new(done));
                return;
            }
            c.waiting.insert(name.clone(), vec![Box::new(done)]);
        }
        let (api, files) = (self.api.clone(), self.files.clone());
        let app = self.clone();
        glib::spawn_future_local(async move {
            let key = covers::key(&s);
            let bytes = gio::spawn_blocking(move || {
                // Offline, a smaller kept copy beats none.
                let file = files
                    .kept(&key, size)
                    .or_else(|| files.fetch(&api, s.id, &key, size))
                    .or_else(|| files.kept(&key, 0));
                file.and_then(|f| std::fs::read(f).ok())
            })
            .await
            .ok()
            .flatten();
            let texture = bytes.and_then(|b| gdk::Texture::from_bytes(&glib::Bytes::from_owned(b)).ok());
            let mut c = app.covers.borrow_mut();
            let waiting = c.waiting.remove(&name).unwrap_or_default();
            let Some(t) = texture else { return };
            c.bytes += pixels(&t);
            c.kept.insert(name.clone(), t.clone());
            c.order.push_back(name);
            while c.bytes > COVER_BYTES {
                let Some(old) = c.order.pop_front() else { break };
                if let Some(t) = c.kept.remove(&old) {
                    c.bytes -= pixels(&t);
                }
            }
            drop(c);
            for f in waiting {
                f(&t);
            }
        });
    }

    /// What the user's devices played last, asked directly, because after a
    /// restart the sync cursor is already past it (plan 017). Also learns
    /// this device's id, which an older sign-in may not have kept.
    pub async fn check_handoff(self: &Rc<Self>) {
        let api = self.api.clone();
        let known = self.prefs.get().device_id != 0 && !self.prefs.get().user.is_empty();
        let got = gio::spawn_blocking(move || {
            let me = if known { None } else { api.me().ok() };
            (me, api.now_playing().ok().flatten())
        })
        .await;
        let Ok((me, np)) = got else { return };
        if let Some(me) = me {
            self.prefs.update(|p| {
                p.device_id = me.device_id.unwrap_or(0);
                p.admin = me.user.as_ref().is_some_and(|u| u.admin);
                if p.user.is_empty() {
                    p.user = me.user.as_ref().map(|u| u.name.clone()).unwrap_or_default();
                }
            });
        }
        if np.is_some() {
            *self.now_playing.borrow_mut() = np;
            self.changed(Change::Handoff);
        }
    }

    /// Signs in and starts the first sync. Another account's data, its
    /// unsent edits included, is cleared first, as on the phone.
    pub async fn sign_in(self: &Rc<Self>, server: String, user: String, password: String) -> Result<(), String> {
        let api = self.api.clone();
        let device = glib::host_name().to_string();
        let (s, u) = (server.clone(), user.clone());
        let login = gio::spawn_blocking(move || api.login(&s, &u, &password, &device))
            .await
            .map_err(|_| "Sign-in failed".to_string())?
            .map_err(|e| match e.code() {
                401 => "Wrong name or password".to_string(),
                0 => format!("Cannot reach {server}: {e}"),
                _ => e.to_string(),
            })?;
        let same = self.prefs.get().user == user && self.prefs.get().server == server;
        if !same {
            let _ = self.db.lock().unwrap().wipe();
            self.prefs.forget_account();
            *self.catalog.borrow_mut() = Arc::new(Catalog::empty());
            self.forget_files();
        }
        self.prefs.set_token(&login.token);
        self.prefs.update(|p| {
            p.server = server;
            p.user = user;
            p.device_id = login.device_id.unwrap_or(0);
            p.admin = login.user.as_ref().is_some_and(|u| u.admin);
        });
        self.changed(Change::Account);
        self.sync_soon(Duration::ZERO);
        let a = self.clone();
        glib::spawn_future_local(async move { a.check_handoff().await });
        Ok(())
    }

    /// Forgets this account here: its data, unsent edits and the token,
    /// which the server forgets too when it can be reached.
    pub async fn sign_out(&self) {
        let api = self.api.clone();
        let _ = gio::spawn_blocking(move || api.logout()).await;
        self.prefs.set_token("");
        let _ = self.db.lock().unwrap().wipe();
        self.prefs.forget_account();
        *self.catalog.borrow_mut() = Arc::new(Catalog::empty());
        self.forget_files();
        self.playlists.borrow_mut().clear();
        self.queues.borrow_mut().clear();
        self.settings.borrow_mut().clear();
        *self.marks.borrow_mut() = Marks::default();
        self.now_playing.borrow_mut().take();
        if let Some(p) = self.playback.get() {
            p.reset();
        }
        self.changed(Change::Account);
    }
}

fn pixels(t: &gdk::Texture) -> usize {
    (t.width() * t.height() * 4) as usize
}
