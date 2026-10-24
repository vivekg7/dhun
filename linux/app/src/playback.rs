//! The player and the queues around it, the Mac's `Playback` and the
//! phone's (plans 007, 024, 039). The engine holds the songs being heard;
//! this holds the active queue's play order and loads only that queue, so
//! switching saves the outgoing queue's song and position and loads the
//! incoming one where it was left. Every listen is logged (plan 008) and
//! long files keep a resume point (plan 009). Speed and pitch (016) are
//! here; the sleep timer (014) is in `sleep`.

mod sleep;
pub use sleep::Sleep;

use crate::app::{App, Change};
use crate::engine::{Engine, Event, Source};
use dhun_core::catalog::SongRef;
use dhun_core::model::{NowPlaying, QueueRow};
use dhun_core::queue::{self, END_MARGIN_MS, MAX_QUEUES, RESTART_WINDOW, Upcoming, distinct, random};
use dhun_core::store::{self, Listen};
use dhun_core::synced;
use dhun_core::tempo::{self, Tempo};
use dhun_core::time::{now_ms, parse};
use gtk::glib;
use gtk::prelude::*;
use serde::{Deserialize, Serialize};
use std::cell::{Cell, RefCell};
use std::collections::{HashMap, HashSet};
use std::rc::Rc;
use std::time::{Duration, Instant};

type Notice = Box<dyn Fn(&str)>;

/// A listen still open (plan 008): saved every few seconds, so a crash
/// loses at most those.
#[derive(Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
struct Open {
    song: i64,
    /// When it began to play: a song loaded but never started is not a listen yet.
    started_at: i64,
    from_ms: i64,
    queue: String,
    source: String,
    shuffle: bool,
    heard_ms: i64,
    last_pos: i64,
    last_at: i64,
    /// When the current stretch of playing began; not saved.
    #[serde(skip)]
    since: Option<Instant>,
    /// The speed it plays at: a minute at 2× is two minutes of the song heard. Not saved.
    #[serde(skip)]
    speed: f64,
}

#[derive(Default)]
struct St {
    active_id: String,
    current: Option<SongRef>,
    playing: bool,
    position_ms: i64,
    decoded_ms: i64,
    /// Held up because the song cannot be fetched: the fetch keeps trying (plan 019).
    waiting: bool,
    /// A long file with a resume point, waiting for the user's answer ("ask", plan 009).
    offer_resume: Option<(SongRef, i64)>,
    /// The active queue's songs as loaded, in queue order; `order` is the
    /// play order of the same ids, shuffled or not.
    items: Vec<i64>,
    order: Vec<i64>,
    restored: bool,
    ended: bool,
    needs_restart: bool,
    failures: usize,
    /// The song whose failure was dealt with: a pipeline tells one failure
    /// several times, and each after the first is not another.
    failed: Option<i64>,
    open: Option<Open>,
    ticks: u64,
    /// A seek or a new song was just asked for: the engine's place is not
    /// yet the new one, so it is not read back until then.
    settle_until: Option<Instant>,
    /// When the file playing last stopped, so going on after a while goes
    /// back a little; 0 while it plays and after a seek made while stopped.
    stopped_at: i64,
    /// When the place `start_position` last chose was heard; 0 for none.
    heard_at: i64,
    state_at: i64,
    dismissed: String,
}

pub struct Playback {
    app: Rc<App>,
    engine: Engine,
    st: RefCell<St>,
    me: std::rc::Weak<Playback>,
    /// A look at what the song cache fetches is due.
    cache_due: Cell<bool>,
    /// A note for the user (a queue removed to make room), shown briefly.
    pub notices: RefCell<Vec<Notice>>,
    /// The sleep timer (plan 014), and the clock that fades it out.
    sleep: RefCell<Option<Sleep>>,
    fading: RefCell<Option<glib::SourceId>>,
}

impl Playback {
    pub fn new(app: &Rc<App>) -> Rc<Playback> {
        // Where earlier builds of this first version kept the songs arriving.
        let _ = std::fs::remove_dir_all(app.dir.join("stream"));
        let cache = app.cache.clone();
        let open = Box::new(move |s: &SongRef| cache.open(s).map(|part| Source { part, format: s.format.clone() }));
        let pb = Rc::new_cyclic(|weak: &std::rc::Weak<Playback>| {
            let w = weak.clone();
            let engine = Engine::new(open, move |e| {
                if let Some(p) = w.upgrade() {
                    p.event(e);
                }
            });
            let prefs = app.prefs.get();
            Playback {
                app: app.clone(),
                engine,
                st: RefCell::new(St {
                    active_id: prefs.active_queue,
                    state_at: prefs.state_at,
                    dismissed: prefs.handoff_dismissed,
                    ..Default::default()
                }),
                me: weak.clone(),
                cache_due: Cell::new(false),
                notices: RefCell::new(vec![]),
                sleep: RefCell::default(),
                fading: RefCell::default(),
            }
        });
        let w = Rc::downgrade(&pb);
        glib::timeout_add_local(Duration::from_millis(250), move || match w.upgrade() {
            Some(p) => {
                p.tick();
                glib::ControlFlow::Continue
            }
            None => glib::ControlFlow::Break,
        });
        let w = Rc::downgrade(&pb);
        app.listen(move |c| {
            let Some(p) = w.upgrade() else { return false };
            if c == Change::Catalog {
                p.catalog_changed();
            }
            true
        });
        pb.close_interrupted();
        pb
    }

    // Derived state, for the views.

    pub fn current(&self) -> Option<SongRef> {
        self.st.borrow().current.clone()
    }
    pub fn is_playing(&self) -> bool {
        self.st.borrow().playing
    }
    pub fn position_ms(&self) -> i64 {
        self.st.borrow().position_ms
    }
    pub fn waiting(&self) -> bool {
        self.st.borrow().waiting
    }
    pub fn active_id(&self) -> String {
        self.st.borrow().active_id.clone()
    }
    pub fn items(&self) -> Vec<i64> {
        self.st.borrow().items.clone()
    }
    pub fn offer_resume(&self) -> Option<(SongRef, i64)> {
        self.st.borrow().offer_resume.clone()
    }

    pub fn duration_ms(&self) -> i64 {
        let st = self.st.borrow();
        st.current.as_ref().map(|s| s.duration_ms).filter(|&d| d > 0).unwrap_or(st.decoded_ms)
    }

    pub fn active(&self) -> Option<QueueRow> {
        let id = self.active_id();
        self.app.queues.borrow().iter().find(|q| q.id == id).cloned()
    }

    /// The queues in the user's order, numbered from 1 as in Musicolet (plan 020).
    pub fn ordered(&self) -> Vec<QueueRow> {
        queue::order_of(&self.app.queues.borrow(), self.app.settings.borrow().get(synced::QUEUE_ORDER))
    }

    pub fn shuffle(&self) -> bool {
        self.active().is_some_and(|q| q.shuffle)
    }
    pub fn repeat_mode(&self) -> String {
        self.active().map_or("off".into(), |q| q.repeat_mode)
    }

    /// Every episode and book file keeps its place, however short (plan 031).
    pub fn is_long(&self, s: &SongRef) -> bool {
        s.spoken() || s.duration_ms >= synced::MIN_MINUTES.get(&self.app.settings.borrow()) * 60_000
    }

    fn current_index(&self) -> Option<usize> {
        let st = self.st.borrow();
        let id = st.current.as_ref()?.id;
        st.items.iter().position(|&i| i == id)
    }

    /// "Continue from Pixel": another device's playback, offered while this
    /// one is not playing, when it is newer than anything this device played
    /// and its queue and song are here (plan 017).
    pub fn handoff(&self) -> Option<NowPlaying> {
        let np = self.app.now_playing.borrow().clone()?;
        let st = self.st.borrow();
        let me = self.app.prefs.get().device_id;
        let ok = !st.playing
            && me != 0
            && np.device_id != me
            && parse(&np.at) > st.state_at
            && np.at != st.dismissed
            && self.app.queues.borrow().iter().any(|q| q.id == np.queue)
            && self.app.catalog().by_id.contains_key(&np.song);
        ok.then_some(np)
    }

    fn changed(&self) {
        self.app.changed(Change::Playback);
    }

    pub fn notice(&self, text: &str) {
        for f in self.notices.borrow().iter() {
            f(text);
        }
    }

    // Transport.

    pub fn toggle(self: &Rc<Self>) {
        if self.is_playing() { self.pause() } else { self.play() }
    }

    pub fn play(self: &Rc<Self>) {
        let Some(s) = self.current() else { return };
        let back = self.continued(&s);
        let (ended, restart, pos) = {
            let st = self.st.borrow();
            (st.ended, st.needs_restart, st.position_ms)
        };
        if ended || restart {
            // The queue ran out: play its song again, as Media3 does on Play.
            self.start(&s, if ended { 0 } else { back.unwrap_or(pos) }, false);
            return;
        }
        if let Some(b) = back {
            self.seek(b);
        }
        self.engine.resume();
        self.set_playing(true);
    }

    pub fn pause(self: &Rc<Self>) {
        if !self.is_playing() {
            return;
        }
        self.engine.pause();
        self.set_playing(false);
    }

    /// The next file. From the media keys, in a long file, it may seek
    /// forward instead ("longFiles.next"), as Previous may seek back.
    pub fn next(self: &Rc<Self>, on_screen: bool) {
        if let Some(s) = self.current() {
            if !on_screen && self.is_long(&s) && synced::NEXT.get(&self.app.settings.borrow()) == "seek" {
                return self.seek_forward();
            }
        }
        let wrap = self.repeat_mode() == "queue";
        if let Some(n) = self.current_index().and_then(|i| self.next_index(i, wrap)) {
            self.jump(n, "skipped", None);
        }
    }

    /// Back to the start of the song, or to the one before within its first
    /// three seconds. In a long file never back to its start: the restart
    /// would be saved as the place to resume (plan 009).
    pub fn previous(self: &Rc<Self>, on_screen: bool) {
        let (Some(i), Some(s)) = (self.current_index(), self.current()) else { return };
        let settings = self.app.settings.borrow().clone();
        let long = synced::PREVIOUS.get(&settings).to_string();
        let keep = self.is_long(&s) && long != "songs";
        if keep && !on_screen && long == "ignore" {
            return;
        }
        if keep && !on_screen && long == "seek" {
            return self.seek_back();
        }
        let restart = !keep && synced::PREVIOUS_RESTART.get(&settings);
        if restart && self.position_ms() as f64 > RESTART_WINDOW * 1000.0 {
            return self.seek(0);
        }
        if let Some(p) = self.previous_index(i) {
            self.jump(p, "previous", None);
        } else if restart {
            self.seek(0);
        }
    }

    /// Back or on within the file, by the seconds set for long files (plan 009).
    pub fn seek_back(self: &Rc<Self>) {
        let by = synced::SEEK_BACK.get(&self.app.settings.borrow());
        self.seek_by(-by * 1000);
    }
    pub fn seek_forward(self: &Rc<Self>) {
        let by = synced::SEEK_FORWARD.get(&self.app.settings.borrow());
        self.seek_by(by * 1000);
    }

    fn seek_by(self: &Rc<Self>, ms: i64) {
        let d = self.duration_ms();
        let to = (self.position_ms() + ms).max(0);
        self.seek(if d > 0 { to.min(d) } else { to });
    }

    pub fn seek(self: &Rc<Self>, ms: i64) {
        let Some(s) = self.current() else { return };
        let (playing, ended, restart) = {
            let mut st = self.st.borrow_mut();
            st.position_ms = ms.max(0);
            st.settle_until = Some(Instant::now() + Duration::from_millis(400));
            // Stopped, a seek chooses the place to go on from: nothing rewinds it.
            if !st.playing {
                st.stopped_at = 0;
            }
            (st.playing, st.ended, st.needs_restart)
        };
        // Paused, nothing else saves where it now is.
        if !playing {
            if let Some(q) = self.active() {
                self.save_place(q, &s, ms);
            }
        }
        if ended || restart {
            self.start(&s, ms, !playing);
        } else {
            self.engine.seek(ms);
        }
        self.app.changed(Change::Position);
    }

    /// Plays `index` of the active queue. A long file already there goes on
    /// from its place: starting it again would lose the place to a click.
    pub fn play_at(self: &Rc<Self>, index: usize) {
        if index >= self.st.borrow().items.len() {
            return;
        }
        if Some(index) == self.current_index() && self.current().is_some_and(|s| self.is_long(&s)) {
            return self.play();
        }
        self.jump(index, "switched", Some(true));
    }

    // Queues.

    /// Plays queue `id` from `song`: the playing queue jumps to it; another
    /// is switched to with that song as its place.
    pub fn play_queue_at(self: &Rc<Self>, id: &str, song: i64) {
        if id == self.active_id() {
            let i = self.st.borrow().items.iter().position(|&s| s == song);
            if let Some(i) = i {
                self.play_at(i);
            }
            return;
        }
        let Some(mut q) = self.app.queues.borrow().iter().find(|q| q.id == id).cloned() else { return };
        self.save_active();
        q.current_song = song;
        q.position_ms = 0;
        q.used_at = now_ms();
        self.load(q, true, None);
    }

    /// Plays `songs` from `start` in a new queue named `name` (AGENTS.md:
    /// playing from a list never overwrites the current queue). A queue
    /// already called `name` is reused and refilled rather than duplicated,
    /// so playing songs in one album does not leave "Album (2)", "Album (3)".
    pub fn play_list(self: &Rc<Self>, name: &str, source: &str, songs: &[SongRef], start: usize) {
        if songs.is_empty() {
            return;
        }
        self.save_active();
        let first = songs[start.min(songs.len() - 1)].id;
        let ids = distinct(songs.iter().map(|s| s.id));
        let now = now_ms();
        let same = self.app.queues.borrow().iter().find(|q| q.name.to_lowercase() == name.to_lowercase()).cloned();
        let q = match same {
            Some(mut q) => {
                q.songs = ids;
                q.current_song = first;
                q.position_ms = 0;
                q.used_at = now;
                self.app.store(|db| {
                    store::replace_queue(db, &q)?;
                    store::set_current(db, &q)
                });
                q
            }
            None => self.create(name, ids, first, now),
        };
        let (id, src) = (q.id.clone(), source.to_string());
        self.app.prefs.update(|p| {
            p.sources.insert(id, src);
        });
        self.app.reload_queues();
        self.load(q, true, None);
    }

    /// A new queue, last in the order. At most `MAX_QUEUES`: as in Musicolet,
    /// the first queue goes to make room, and a note says so.
    fn create(self: &Rc<Self>, name: &str, ids: Vec<i64>, current: i64, now: i64) -> QueueRow {
        let order = self.ordered();
        let over = order.len().saturating_sub(MAX_QUEUES - 1);
        let q = QueueRow {
            id: store::uuid(),
            name: name.into(),
            songs: ids,
            current_song: current,
            position_ms: 0,
            shuffle: false,
            repeat_mode: "off".into(),
            used_at: now,
        };
        let mut ids: Vec<String> = order.iter().skip(over).map(|q| q.id.clone()).collect();
        ids.push(q.id.clone());
        self.app.store(|db| {
            for old in order.iter().take(over) {
                store::delete_queue(db, &old.id)?;
            }
            store::create_queue(db, &q)?;
            store::setting(db, synced::QUEUE_ORDER, ids.into())
        });
        if over > 0 {
            self.notice(&format!("At most {MAX_QUEUES} queues: “{}” was removed", order[0].name));
        }
        q
    }

    fn set_order(self: &Rc<Self>, ids: Vec<String>) {
        self.app.store(|db| store::setting(db, synced::QUEUE_ORDER, ids.into()));
        self.app.reload_queues();
    }

    /// A queue of `songs` that does not start playing. The name is made
    /// unique the way the server does it, so the two agree.
    pub fn new_queue(self: &Rc<Self>, name: &str, songs: &[SongRef]) {
        let taken: HashSet<String> = self.app.queues.borrow().iter().map(|q| q.name.to_lowercase()).collect();
        let mut unique = name.to_string();
        let mut n = 2;
        while taken.contains(&unique.to_lowercase()) {
            unique = format!("{name} ({n})");
            n += 1;
        }
        let c = self.app.catalog();
        let adding = distinct(songs.iter().map(|s| s.id).filter(|i| c.by_id.contains_key(i)));
        let first = adding.first().copied().unwrap_or(0);
        self.create(&unique, adding, first, now_ms());
        self.app.reload_queues();
    }

    /// Drag in the queue list: queue `from` goes to `to`, counting from 0.
    pub fn move_queue(self: &Rc<Self>, from: usize, to: usize) {
        let mut ids: Vec<String> = self.ordered().into_iter().map(|q| q.id).collect();
        if from >= ids.len() || to >= ids.len() || from == to {
            return;
        }
        let id = ids.remove(from);
        ids.insert(to, id);
        self.set_order(ids);
    }

    /// Every queue but `keep` (Musicolet's "Remove all other queues").
    pub fn delete_others(self: &Rc<Self>, keep: &str) {
        if keep != self.active_id() {
            self.switch_to(keep);
        }
        let others: Vec<String> = self.app.queues.borrow().iter().map(|q| q.id.clone()).filter(|q| q != keep).collect();
        self.app.store(|db| others.iter().try_for_each(|q| store::delete_queue(db, q)));
        self.set_order(vec![keep.to_string()]);
    }

    /// Adds `songs` to the end of queue `id`; songs already in it move there.
    pub fn add_to(self: &Rc<Self>, id: &str, songs: &[SongRef]) {
        if id == self.active_id() {
            return self.add_to_queue(songs);
        }
        let Some(mut q) = self.app.queues.borrow().iter().find(|q| q.id == id).cloned() else { return };
        let c = self.app.catalog();
        let adding = distinct(songs.iter().map(|s| s.id).filter(|i| c.by_id.contains_key(i)));
        if adding.is_empty() {
            return;
        }
        q.songs.retain(|s| !adding.contains(s));
        q.songs.extend(&adding);
        if q.current_song == 0 {
            q.current_song = q.songs[0];
        }
        self.app.store(|db| store::insert_into_queue(db, &q, &adding, None));
        self.app.reload_queues();
    }

    /// A drag in queue `id`: `song` goes where `target` is, after it when
    /// moving down and before it when moving up.
    pub fn move_in(self: &Rc<Self>, id: &str, song: i64, target: i64) {
        let Some(mut q) = self.app.queues.borrow().iter().find(|q| q.id == id).cloned() else { return };
        let active = id == self.active_id();
        let mut list = if active { self.items() } else { q.songs.clone() };
        let (Some(from), Some(to)) = (list.iter().position(|&s| s == song), list.iter().position(|&s| s == target))
        else {
            return;
        };
        if from == to {
            return;
        }
        list.remove(from);
        list.insert(to, song);
        if active {
            self.st.borrow_mut().items = list.clone();
            self.rebuild_order(true, None);
        }
        q.songs = list;
        let after = if to == 0 { 0 } else { q.songs[to - 1] };
        self.app.store(|db| store::move_in_queue(db, &q, song, after));
        self.app.reload_queues();
        self.changed();
    }

    /// Queue `id` in a new order of the same songs (a sort, Randomize,
    /// Reverse). The playing song plays on.
    pub fn reorder(self: &Rc<Self>, id: &str, order: Vec<i64>) {
        let Some(mut q) = self.app.queues.borrow().iter().find(|q| q.id == id).cloned() else { return };
        if id == self.active_id() {
            let loaded: HashSet<i64> = self.items().into_iter().collect();
            let items: Vec<i64> = order.into_iter().filter(|s| loaded.contains(s)).collect();
            self.st.borrow_mut().items = items.clone();
            self.rebuild_order(true, None);
            q.songs = items;
        } else {
            q.songs = order;
        }
        self.app.store(|db| store::replace_queue(db, &q));
        self.app.reload_queues();
        self.changed();
    }

    pub fn switch_to(self: &Rc<Self>, id: &str) {
        if id == self.active_id() {
            return;
        }
        let Some(mut q) = self.app.queues.borrow().iter().find(|q| q.id == id).cloned() else { return };
        self.save_active();
        q.used_at = now_ms();
        self.load(q, true, None);
    }

    /// After the current song; songs already queued are moved, never duplicated.
    pub fn play_next(self: &Rc<Self>, songs: &[SongRef]) {
        self.insert(songs, true);
    }
    pub fn add_to_queue(self: &Rc<Self>, songs: &[SongRef]) {
        self.insert(songs, false);
    }

    fn insert(self: &Rc<Self>, songs: &[SongRef], next: bool) {
        let Some(mut q) = self.active() else { return self.play_list("Queue", "", songs, 0) };
        let playing = self.current().map(|s| s.id);
        let c = self.app.catalog();
        let adding = distinct(songs.iter().map(|s| s.id).filter(|&i| Some(i) != playing && c.by_id.contains_key(&i)));
        if adding.is_empty() {
            return;
        }
        let at = {
            let set: HashSet<i64> = adding.iter().copied().collect();
            let mut st = self.st.borrow_mut();
            st.items.retain(|i| !set.contains(i));
            // The playing song's place once the moved songs are out, as on the Mac.
            let cur = playing.and_then(|p| st.items.iter().position(|&i| i == p));
            let at = if next { cur.map_or(0, |i| i + 1) } else { st.items.len() };
            st.items.splice(at..at, adding.iter().copied());
            q.songs = st.items.clone();
            at
        };
        self.rebuild_order(true, None);
        let after = if at == 0 { 0 } else { q.songs[at - 1] };
        self.app.store(|db| store::insert_into_queue(db, &q, &adding, Some(after)));
        self.app.reload_queues();
        self.changed();
    }

    pub fn remove_from(self: &Rc<Self>, id: &str, songs: &HashSet<i64>) {
        let Some(mut q) = self.app.queues.borrow().iter().find(|q| q.id == id).cloned() else { return };
        if songs.is_empty() {
            return;
        }
        let removed: Vec<i64> = songs.iter().copied().collect();
        if id == self.active_id() {
            let was_current = self.current().is_some_and(|s| songs.contains(&s.id));
            let following = {
                let st = self.st.borrow();
                self.current_index().and_then(|i| st.items[i + 1..].iter().find(|s| !songs.contains(s)).copied())
            };
            {
                let mut st = self.st.borrow_mut();
                st.items.retain(|s| !songs.contains(s));
                q.songs = st.items.clone();
            }
            self.rebuild_order(true, None);
            self.app.store(|db| store::remove_from_queue(db, &q, &removed));
            self.app.reload_queues();
            if was_current {
                // The player goes on with the song after it, as Media3 does.
                self.close("skipped", None);
                let next = following.or_else(|| self.st.borrow().items.first().copied());
                match next.and_then(|n| self.app.catalog().by_id.get(&n).cloned()) {
                    Some(s) => {
                        let paused = !self.is_playing();
                        self.start(&s, 0, paused);
                    }
                    None => self.stop_all(),
                }
            }
            self.changed();
            return;
        }
        let old = q.songs.clone();
        q.songs.retain(|s| !songs.contains(s));
        let current_gone = songs.contains(&q.current_song);
        if current_gone {
            let from = old.iter().position(|&s| s == q.current_song).map_or(0, |i| i + 1);
            q.current_song = old[from.min(old.len())..]
                .iter()
                .find(|s| !songs.contains(s))
                .or(q.songs.first())
                .copied()
                .unwrap_or(0);
            q.position_ms = 0;
        }
        self.app.store(|db| {
            store::remove_from_queue(db, &q, &removed)?;
            if current_gone { store::set_current(db, &q) } else { Ok(()) }
        });
        self.app.reload_queues();
    }

    pub fn rename(self: &Rc<Self>, id: &str, name: &str) {
        let Some(mut q) = self.app.queues.borrow().iter().find(|q| q.id == id).cloned() else { return };
        q.name = name.into();
        self.app.store(|db| store::rename_queue(db, &q));
        self.app.reload_queues();
    }

    pub fn delete(self: &Rc<Self>, id: &str) {
        let order: Vec<String> = self.ordered().into_iter().map(|q| q.id).filter(|q| q != id).collect();
        self.set_order(order);
        if id == self.active_id() {
            self.close("stopped", None);
            // Gone before stop_all, so the active id is cleared when no queue is left.
            self.app.store(|db| store::delete_queue(db, id));
            self.app.reload_queues();
            self.stop_all();
            // The one used most recently takes its place, paused.
            let next = self.app.queues.borrow().iter().find(|q| q.id != id).cloned();
            if let Some(q) = next {
                self.load(q, false, None);
            }
        } else {
            self.app.store(|db| store::delete_queue(db, id));
            self.app.reload_queues();
        }
    }

    pub fn toggle_shuffle(self: &Rc<Self>) {
        let Some(mut q) = self.active() else { return };
        q.shuffle = !q.shuffle;
        self.app.store(|db| store::set_mode(db, &q));
        self.app.reload_queues();
        self.rebuild_order(false, None);
        self.changed();
    }

    /// off → whole queue → this song → off.
    pub fn cycle_repeat(self: &Rc<Self>) {
        let Some(mut q) = self.active() else { return };
        q.repeat_mode = match q.repeat_mode.as_str() {
            "off" => "queue",
            "queue" => "song",
            _ => "off",
        }
        .into();
        self.app.store(|db| store::set_mode(db, &q));
        self.app.reload_queues();
        self.publish_upcoming();
        self.changed();
    }

    pub fn answer_resume(self: &Rc<Self>, accept: bool) {
        let Some((song, ms)) = self.st.borrow_mut().offer_resume.take() else { return };
        if accept && self.current().is_some_and(|s| s.id == song.id) {
            self.seek(ms);
        }
        self.changed();
    }

    // Chapters (plan 031).

    /// The next chapter, never the next file; nothing without chapters.
    pub fn chapter_on(self: &Rc<Self>) {
        let starts = self.chapter_starts();
        if let Some(at) = queue::chapter_after(&starts, self.position_ms()) {
            self.seek(at);
        }
    }

    /// Back to this chapter's start, or within its first seconds to the one before.
    pub fn chapter_back(self: &Rc<Self>) {
        let starts = self.chapter_starts();
        if let Some(at) = queue::chapter_before(&starts, self.position_ms(), (RESTART_WINDOW * 1000.0) as i64) {
            self.seek(at);
        }
    }

    fn chapter_starts(&self) -> Vec<i64> {
        self.current().map(|s| s.chapters.iter().map(|c| c.start_ms).collect()).unwrap_or_default()
    }

    // Speed and pitch (plan 016).

    /// The playing song's own speed and pitch, synced; None when it follows another.
    pub fn song_tempo(&self) -> Option<Tempo> {
        let s = self.current()?;
        Tempo::of(self.app.settings.borrow().get(&tempo::song_setting(s.id)))
    }

    pub fn tempo(&self) -> Tempo {
        self.current().map_or_else(|| self.app.prefs.get().everyday(), |s| self.tempo_of(&s))
    }

    fn tempo_of(&self, s: &SongRef) -> Tempo {
        tempo_in(s, &self.app.settings.borrow(), self.app.prefs.get().everyday())
    }

    /// For the playing song only (synced, as its own setting), for every
    /// episode or every book (synced) when it is one, or as this computer's
    /// everyday one. Normal speed is stored for a collection rather than
    /// cleared, so episodes do not fall back to the everyday speed for music.
    pub fn set_tempo(self: &Rc<Self>, t: Tempo, only_this_song: bool) {
        match self.current() {
            Some(s) if only_this_song => {
                self.app.store(|db| store::setting(db, &tempo::song_setting(s.id), t.json()));
            }
            Some(s) if s.spoken() => {
                self.app.store(|db| store::setting(db, &tempo::kind_setting(&s.kind), t.json()));
            }
            _ => self.app.prefs.update(|p| {
                p.speed = t.speed;
                p.semitones = t.semitones;
            }),
        }
        self.app.reload_queues();
        self.apply_tempo();
    }

    /// The playing song follows the everyday speed and pitch again.
    pub fn clear_song_tempo(self: &Rc<Self>) {
        let Some(s) = self.current() else { return };
        self.app.store(|db| store::setting(db, &tempo::song_setting(s.id), serde_json::Value::Null));
        self.app.reload_queues();
        self.apply_tempo();
    }

    /// The playing song at its speed and pitch. A pitch shift put in or
    /// taken out opens it again where it is, which the listen does not see.
    fn apply_tempo(self: &Rc<Self>) {
        let Some(s) = self.current() else { return };
        let t = self.tempo_of(&s);
        if let Some(o) = self.st.borrow_mut().open.as_mut() {
            // Time heard so far counts at the old speed.
            count(o);
            o.speed = t.speed;
        }
        if self.engine.set_tempo(s.id, t.speed, t.semitones, true) {
            let (pos, paused) = (self.position_ms(), !self.is_playing());
            self.st.borrow_mut().settle_until = Some(Instant::now() + Duration::from_millis(600));
            if let Some(part) = self.app.cache.open(&s) {
                self.engine.play(s.id, Source { part, format: s.format.clone() }, pos, paused);
            }
        }
        self.publish_upcoming();
        self.changed();
    }

    // Hand-off (plan 017).

    /// Where a hand-off continues: the other device's place, moved on by the
    /// time passed if it was still playing, and never at the very end.
    pub fn place_of(&self, np: &NowPlaying) -> i64 {
        let mut pos = np.position_ms;
        if np.playing {
            pos += (now_ms() - parse(&np.at)).max(0);
        }
        if let Some(len) = self.app.catalog().by_id.get(&np.song).map(|s| s.duration_ms).filter(|&l| l > 0) {
            pos = pos.min(len - END_MARGIN_MS);
        }
        pos.max(0)
    }

    /// Takes over from another device: its queue (queues are synced), its song, and its place.
    pub fn continue_from(self: &Rc<Self>, np: &NowPlaying) {
        let Some(mut q) = self.app.queues.borrow().iter().find(|q| q.id == np.queue).cloned() else { return };
        if !self.app.catalog().by_id.contains_key(&np.song) {
            return;
        }
        let pos = self.place_of(np);
        if q.id != self.active_id() {
            self.save_active();
        }
        q.current_song = np.song;
        q.position_ms = pos;
        q.used_at = now_ms();
        self.load(q, true, Some(pos));
    }

    /// ✕ on the offer: not offered again, though a newer playback on that device will be.
    pub fn dismiss_handoff(&self, np: &NowPlaying) {
        let at = np.at.clone();
        self.st.borrow_mut().dismissed = at.clone();
        self.app.prefs.update(|p| p.handoff_dismissed = at);
        self.app.changed(Change::Handoff);
    }

    /// Tells the server what this device plays, for the others' hand-off.
    fn report(self: &Rc<Self>, q: &QueueRow, s: &SongRef, pos: i64, playing: bool) {
        let now = now_ms();
        self.st.borrow_mut().state_at = now;
        self.app.prefs.update(|p| p.state_at = now);
        self.app.store(|db| store::playback_state(db, &q.id, s.id, pos, playing));
    }

    /// On sign-out: the open listen belongs to the account being left, and is dropped.
    pub fn reset(self: &Rc<Self>) {
        self.st.borrow_mut().open = None;
        self.app.prefs.update(|p| p.open_listen.clear());
        self.stop_all();
        let mut st = self.st.borrow_mut();
        st.state_at = 0;
        st.dismissed.clear();
        st.offer_resume = None;
        st.restored = false;
        st.active_id.clear();
    }

    // Loading.

    /// The catalogue arrived: reload the queue that was playing when the app
    /// last stopped, paused, once.
    fn catalog_changed(self: &Rc<Self>) {
        if self.st.borrow().restored || self.app.catalog().by_id.is_empty() {
            return;
        }
        self.st.borrow_mut().restored = true;
        self.app.reload_queues();
        if self.current().is_none() {
            if let Some(q) = self.active() {
                self.load(q, false, None);
            }
        }
    }

    /// Another device's queue edits arrived. The playing queue may be gone
    /// (deleted elsewhere), or hold other songs (added on the phone): the
    /// player follows, so a later edit here does not undo theirs.
    pub fn queues_synced(self: &Rc<Self>) {
        if self.active_id().is_empty() || !self.st.borrow().restored {
            return;
        }
        let Some(q) = self.active() else {
            if self.current().is_none() {
                return self.set_active("");
            }
            self.close("stopped", None);
            self.stop_all();
            self.notice("The queue that was playing was removed on another device");
            return;
        };
        let fresh: Vec<i64> = self.app.catalog().songs_of(&q.songs).iter().map(|s| s.id).collect();
        // Removing the playing song elsewhere does not stop it here.
        let keep = self.current().is_none_or(|s| fresh.contains(&s.id));
        if fresh != self.st.borrow().items && keep {
            self.st.borrow_mut().items = fresh;
            self.rebuild_order(true, None);
        }
        self.publish_upcoming();
        self.changed();
    }

    fn load(self: &Rc<Self>, q: QueueRow, play: bool, position: Option<i64>) {
        self.close("switched", None);
        let songs = self.app.catalog().songs_of(&q.songs);
        self.set_active(&q.id);
        if songs.is_empty() {
            return self.stop_all();
        }
        let index = songs.iter().position(|s| s.id == q.current_song).unwrap_or(0);
        let song = songs[index].clone();
        self.st.borrow_mut().heard_at = 0;
        let mut start = position;
        if start.is_some() {
            self.st.borrow_mut().offer_resume = None;
        } else {
            start = self.start_position(&song);
        }
        let mut start = start.unwrap_or(if song.id == q.current_song { q.position_ms } else { 0 });
        // Saved at the very end: start the song over, as Media3 does on Play,
        // rather than finishing it again at once.
        if song.duration_ms > 0 && start >= song.duration_ms - 1000 {
            start = 0;
        }
        self.st.borrow_mut().items = songs.iter().map(|s| s.id).collect();
        self.rebuild_order(false, Some(q.shuffle));
        self.app.store(|db| db.put_queue(&q));
        self.app.reload_queues();
        self.start(&song, start, !play);
        if !play {
            let since = self.stopped_since();
            self.st.borrow_mut().stopped_at = since;
        }
    }

    /// Opens `s` in the engine at `ms`; the listen opens with it.
    fn start(self: &Rc<Self>, s: &SongRef, ms: i64, paused: bool) {
        {
            let mut st = self.st.borrow_mut();
            st.current = Some(s.clone());
            st.failed = None;
            st.position_ms = ms;
            st.decoded_ms = 0;
            st.ended = false;
            st.needs_restart = false;
            st.settle_until = Some(Instant::now() + Duration::from_millis(600));
        }
        self.publish_upcoming();
        self.open_listen(s, ms);
        self.tell_cache_long();
        let t = self.tempo_of(s);
        self.engine.set_tempo(s.id, t.speed, t.semitones, false);
        match self.app.cache.open(s) {
            Some(part) => self.engine.play(s.id, Source { part, format: s.format.clone() }, ms, paused),
            None => return self.song_failed(s.id, paused),
        }
        self.set_playing(!paused);
        // A new song, or the same one opened again: any pause before was another place's.
        self.st.borrow_mut().stopped_at = 0;
        self.changed();
    }

    fn jump(self: &Rc<Self>, index: usize, end: &str, play: Option<bool>) {
        let Some(id) = self.st.borrow().items.get(index).copied() else { return };
        let Some(s) = self.app.catalog().by_id.get(&id).cloned() else { return };
        let finished = self.current();
        let at = self.position_ms();
        self.close(end, Some(at));
        // Still stopped where it was heard: its point was saved then (save_active).
        if let Some(f) = &finished {
            if self.st.borrow().stopped_at == 0 {
                self.save_resume(f, at);
            }
        }
        self.st.borrow_mut().heard_at = 0;
        let mut start = self.start_position(&s).unwrap_or(0);
        if finished.as_ref().is_some_and(|f| f.id == s.id) {
            start = 0;
        }
        let paused = !play.unwrap_or(self.is_playing());
        self.start(&s, start, paused);
        // Next while paused: the file has been stopped as long as its place.
        if paused {
            let since = self.stopped_since();
            self.st.borrow_mut().stopped_at = since;
        }
        self.after_song_change(&s, start);
    }

    fn after_song_change(self: &Rc<Self>, s: &SongRef, start_ms: i64) {
        self.sleep_next_song();
        let Some(mut q) = self.active() else { return };
        q.current_song = s.id;
        q.position_ms = start_ms;
        self.app.store(|db| store::set_current(db, &q));
        self.app.reload_queues();
        // The next song, for a hand-off from another device.
        if self.is_playing() {
            self.report(&q, s, start_ms, true);
        }
    }

    fn stop_all(self: &Rc<Self>) {
        self.engine.stop();
        {
            let mut st = self.st.borrow_mut();
            st.items.clear();
            st.order.clear();
            st.current = None;
            st.position_ms = 0;
        }
        self.set_playing(false);
        self.publish_upcoming();
        let active = self.active_id();
        if !active.is_empty() && !self.app.queues.borrow().iter().any(|q| q.id == active) {
            self.set_active("");
        }
        self.changed();
    }

    fn set_active(&self, id: &str) {
        self.st.borrow_mut().active_id = id.into();
        let id = id.to_string();
        self.app.prefs.update(|p| p.active_queue = id);
    }

    /// Records where the outgoing queue was, before another is loaded. A
    /// long file still stopped where it was heard keeps its point.
    fn save_active(self: &Rc<Self>) {
        let (Some(mut q), Some(s)) = (self.active(), self.current()) else { return };
        let pos = self.position_ms();
        q.current_song = s.id;
        q.position_ms = pos;
        self.app.store(|db| store::set_current(db, &q));
        if self.st.borrow().stopped_at == 0 {
            self.save_resume(&s, pos);
        }
    }

    // Play order.

    /// Shuffled, a queue keeps its play order across edits: songs keep
    /// their places, new ones go in at random after the playing one.
    /// Shuffling afresh puts the playing song first, as Media3 does.
    fn rebuild_order(self: &Rc<Self>, keeping: bool, shuffle: Option<bool>) {
        let shuffle = shuffle.unwrap_or_else(|| self.shuffle());
        let cur = self.current().map(|s| s.id);
        {
            let mut st = self.st.borrow_mut();
            if !shuffle {
                st.order = st.items.clone();
            } else if keeping && !st.order.is_empty() {
                let present: HashSet<i64> = st.items.iter().copied().collect();
                let mut kept: Vec<i64> = st.order.iter().copied().filter(|i| present.contains(i)).collect();
                let have: HashSet<i64> = kept.iter().copied().collect();
                let at = cur.and_then(|c| kept.iter().position(|&i| i == c)).map_or(0, |i| i + 1);
                for id in st.items.clone() {
                    if !have.contains(&id) {
                        let r = at + random(kept.len() + 1 - at);
                        kept.insert(r, id);
                    }
                }
                st.order = kept;
            } else {
                let mut rest: Vec<i64> = st.items.iter().copied().filter(|&i| Some(i) != cur).collect();
                for i in (1..rest.len()).rev() {
                    rest.swap(i, random(i + 1));
                }
                if let Some(c) = cur.filter(|c| st.items.contains(c)) {
                    rest.insert(0, c);
                }
                st.order = rest;
            }
        }
        self.publish_upcoming();
    }

    /// The index in `items` of the song after `items[i]` in play order.
    fn next_index(&self, i: usize, wrap: bool) -> Option<usize> {
        let st = self.st.borrow();
        let item = *st.items.get(i)?;
        let at = st.order.iter().position(|&s| s == item)?;
        let id = st.order.get(at + 1).or(if wrap { st.order.first() } else { None })?;
        st.items.iter().position(|s| s == id)
    }

    fn previous_index(&self, i: usize) -> Option<usize> {
        let st = self.st.borrow();
        let item = *st.items.get(i)?;
        let at = st.order.iter().position(|&s| s == item)?;
        let id = st.order.get(at.checked_sub(1)?)?;
        st.items.iter().position(|s| s == id)
    }

    /// What the engine reads for the song after the one it decodes.
    fn publish_upcoming(&self) {
        let st = self.st.borrow();
        let up = Upcoming { order: st.order.clone(), repeat_mode: self.repeat_mode(), pause_after: self.pause_after() };
        let c = self.app.catalog();
        let songs: HashMap<i64, SongRef> =
            st.items.iter().filter_map(|i| c.by_id.get(i).map(|s| (*i, s.clone()))).collect();
        // Each song's speed, for a gapless start at it.
        let (settings, everyday) = (self.app.settings.borrow(), self.app.prefs.get().everyday());
        let speeds = songs.values().map(|s| (s.id, tempo_in(s, &settings, everyday).speed));
        self.engine.set_speeds(speeds.filter(|x| x.1 != 1.0).collect());
        drop(settings);
        self.engine.set_upcoming(up, songs);
        drop(st);
        self.poke_cache();
    }

    /// The song playing and the `ahead` after it in play order, each once.
    fn upcoming(&self, ahead: usize) -> Vec<SongRef> {
        let Some(i) = self.current_index() else { return vec![] };
        let wrap = self.repeat_mode() == "queue";
        let mut out = vec![i];
        let mut at = i;
        while out.len() <= ahead {
            match self.next_index(at, wrap) {
                Some(n) if !out.contains(&n) => {
                    out.push(n);
                    at = n;
                }
                _ => break,
            }
        }
        let (c, st) = (self.app.catalog(), self.st.borrow());
        out.iter().filter_map(|&i| c.by_id.get(&st.items[i]).cloned()).collect()
    }

    /// Which files the cache keeps as a window, and how wide (plan 037), as the settings are now.
    fn tell_cache_long(&self) {
        let settings = self.app.settings.borrow();
        self.app.cache.set_long(synced::MIN_MINUTES.get(&settings), synced::CACHE_AHEAD.get(&settings));
    }

    /// Looks again, soon, at what the song cache fetches: the song or the
    /// queue changed, play started or stopped, a fetch ended. While playing,
    /// the next songs are fetched ahead; while stopped, only the current one.
    pub fn poke_cache(&self) {
        if self.cache_due.replace(true) {
            return;
        }
        let me = self.me.clone();
        glib::idle_add_local_once(move || {
            let Some(p) = me.upgrade() else { return };
            p.cache_due.set(false);
            let ahead = match (p.is_playing(), gtk::gio::NetworkMonitor::default().is_network_metered()) {
                (false, _) => 0,
                (true, true) => dhun_core::cache::AHEAD_METERED,
                (true, false) => dhun_core::cache::AHEAD,
            };
            let plan = p.upcoming(ahead);
            let cache = &p.app.cache;
            p.tell_cache_long();
            cache.run(&plan, p.app.prefs.get().cache_limit(), &p.app.catalog().by_id);
            // Online every song plays: what the cache holds shows only offline.
            if p.app.offline.get() {
                p.app.changed(Change::Playable);
            }
        });
    }

    // Following the engine.

    fn event(self: &Rc<Self>, e: Event) {
        match e {
            Event::Started(song) => {
                glib::g_info!("Dhun", "playing {song}");
                if self.current().is_some_and(|c| c.id != song) {
                    if let Some(s) = self.app.catalog().by_id.get(&song).cloned() {
                        self.song_changed_by_itself(&s);
                    }
                }
            }
            Event::Ended => {
                glib::g_info!("Dhun", "end of the queue");
                self.reached_end()
            }
            Event::Failed(song, why, output) => {
                glib::g_warning!("Dhun", "song {song} failed: {why}");
                if output && self.is_playing() {
                    self.notice("Stopped: the sound output is not working");
                }
                // With no sound output every song would fail in turn; paused, nothing moves on.
                self.song_failed(song, output || !self.is_playing());
            }
            Event::OutputChanged => {
                // Headphones out, or another output: pause, as the phone does
                // when its audio becomes "noisy".
                if self.is_playing() {
                    self.engine.pause();
                    self.set_playing(false);
                }
            }
        }
    }

    fn tick(self: &Rc<Self>) {
        if self.app.cache.take_ended() {
            self.poke_cache();
        }
        if self.current().is_none() {
            return;
        }
        let settling = self.st.borrow().settle_until.is_some_and(|t| Instant::now() < t);
        if !settling {
            if let Some((pos, dur)) = self.engine.position() {
                let mut st = self.st.borrow_mut();
                st.position_ms = pos;
                st.decoded_ms = dur;
            }
        }
        let song = self.current().map_or(0, |s| s.id);
        let failing = self.app.cache.failing(song);
        let playing = self.is_playing();
        let was = self.st.borrow().waiting;
        self.st.borrow_mut().waiting = playing && failing;
        if was != (playing && failing) {
            self.changed();
        }
        if !playing {
            return;
        }
        self.app.changed(Change::Position);
        let ticks = {
            let mut st = self.st.borrow_mut();
            st.ticks += 1;
            st.ticks
        };
        // Every 10 s of playing: save the open listen; every 30 s the positions.
        if ticks % 40 == 0 {
            self.save_listen();
        }
        if ticks % 120 == 0 {
            if let (Some(mut q), Some(s)) = (self.active(), self.current()) {
                let pos = self.position_ms();
                q.current_song = s.id;
                q.position_ms = pos;
                self.app.store(|db| store::set_current(db, &q));
                self.save_resume(&s, pos);
                self.report(&q, &s, pos, true);
            }
        }
    }

    /// The engine went on to the next song by itself: the last one finished.
    fn song_changed_by_itself(self: &Rc<Self>, s: &SongRef) {
        if let Some(finished) = self.current() {
            // Where the player got to, not the song's length: a file that
            // cannot be read past "finishes" early (plan 031).
            let at = self.position_ms();
            self.close("finished", Some(at));
            // Heard to the end: a long file's resume point is done with.
            if self.is_long(&finished) {
                let id = finished.id;
                self.app.store(|db| store::resume(db, id, None));
            }
        }
        {
            let mut st = self.st.borrow_mut();
            st.current = Some(s.clone());
            st.stopped_at = 0;
            st.failures = 0;
            st.position_ms = 0;
        }
        let start = self.start_position(s);
        if let Some(ms) = start {
            self.seek(ms);
        }
        self.open_listen(s, start.unwrap_or(0));
        self.apply_tempo();
        self.after_song_change(s, start.unwrap_or(0));
        self.changed();
    }

    /// Everything scheduled has played: the end of the queue, or a sleep timer's song.
    fn reached_end(self: &Rc<Self>) {
        let s = self.current();
        let at = self.position_ms();
        self.close("finished", Some(at));
        if let Some(s) = s.as_ref().filter(|s| self.is_long(s)) {
            let id = s.id;
            self.app.store(|db| store::resume(db, id, None));
        }
        let by_sleep = self.pause_after().is_some();
        self.set_playing(false);
        let wrap = self.repeat_mode() == "queue";
        let next = self.current_index().and_then(|i| self.next_index(i, wrap));
        let next = next.and_then(|n| self.st.borrow().items.get(n).copied());
        if let (true, Some(next)) = (by_sleep, next.and_then(|n| self.app.catalog().by_id.get(&n).cloned())) {
            // Paused where the song ended; Play goes on with the next one.
            self.sleep_paused_at_end();
            self.start(&next, 0, true);
            self.after_song_change(&next, 0);
            return;
        }
        self.sleep_paused_at_end();
        self.st.borrow_mut().ended = true;
        self.changed();
    }

    /// A song that cannot be opened or read is passed over, as the phone does;
    /// a queue of them is not tried forever.
    /// `stop` stays on the song rather than passing over it.
    fn song_failed(self: &Rc<Self>, song: i64, stop: bool) {
        if self.st.borrow_mut().failed.replace(song) == Some(song) {
            return;
        }
        if stop {
            self.set_playing(false);
            return self.changed();
        }
        let failures = {
            let mut st = self.st.borrow_mut();
            st.failures += 1;
            st.failures
        };
        let wrap = self.repeat_mode() == "queue";
        if self.current().is_none_or(|c| c.id != song) || failures >= self.st.borrow().items.len().max(1) {
            self.set_playing(false);
            return self.changed();
        }
        if let Some(n) = self.current_index().and_then(|i| self.next_index(i, wrap)) {
            self.jump(n, "skipped", None);
        }
    }

    fn set_playing(self: &Rc<Self>, playing: bool) {
        let changed = {
            let mut st = self.st.borrow_mut();
            let changed = playing != st.playing;
            st.playing = playing;
            if playing {
                st.failures = 0;
            }
            if changed && !playing {
                st.stopped_at = now_ms();
            }
            if let Some(o) = st.open.as_mut() {
                if playing {
                    started(o);
                } else if o.since.is_some() {
                    count(o);
                    o.since = None;
                }
            }
            changed
        };
        if changed {
            self.poke_cache();
        }
        let (Some(q), Some(s)) = (self.active(), self.current()) else { return };
        if !changed {
            return;
        }
        let pos = self.position_ms();
        self.report(&q, &s, pos, playing);
        if !playing {
            self.save_place(q, &s, pos);
        }
        self.app.changed(Change::Handoff);
        self.changed();
    }

    /// Where queue `q` is, kept and synced: on pause, and on a seek while paused.
    fn save_place(self: &Rc<Self>, mut q: QueueRow, s: &SongRef, pos: i64) {
        q.current_song = s.id;
        q.position_ms = pos;
        self.app.store(|db| store::set_current(db, &q));
        self.save_resume(s, pos);
    }

    // Long files (plan 009).

    /// Where to start `s`: its resume point under "auto"; under "ask", offered instead.
    fn start_position(self: &Rc<Self>, s: &SongRef) -> Option<i64> {
        self.st.borrow_mut().offer_resume = None;
        if !self.is_long(s) {
            return None;
        }
        let r = self.app.db.lock().unwrap().resume(s.id).ok().flatten()?;
        if r.deleted || r.position_ms <= 0 {
            return None;
        }
        // Left there when the point was saved: on another device, or before the app quit.
        let pos = self.rewind(r.position_ms, r.at);
        let mode = synced::RESUME.get(&self.app.settings.borrow()).to_string();
        match mode.as_str() {
            "auto" => {
                self.st.borrow_mut().heard_at = if pos != r.position_ms { now_ms() } else { r.at };
                Some(pos)
            }
            "ask" => {
                self.st.borrow_mut().offer_resume = Some((s.clone(), pos));
                None
            }
            _ => None,
        }
    }

    /// Since when a file just put in place without playing counts as stopped.
    fn stopped_since(&self) -> i64 {
        let h = self.st.borrow().heard_at;
        if h > 0 { h } else { now_ms() }
    }

    fn rewind(&self, pos: i64, since: i64) -> i64 {
        let s = self.app.settings.borrow();
        queue::rewound(pos, now_ms() - since, synced::REWIND.get(&s) * 1000, synced::REWIND_AFTER.get(&s) * 60_000)
    }

    /// Where Play picks up a long file after a long pause: a little before
    /// where it stopped, so the sentence cut off is heard from its start.
    fn continued(&self, s: &SongRef) -> Option<i64> {
        let since = std::mem::take(&mut self.st.borrow_mut().stopped_at);
        if since == 0 || !self.is_long(s) {
            return None;
        }
        let pos = self.position_ms();
        let to = self.rewind(pos, since);
        (to != pos).then_some(to)
    }

    /// Not in the last seconds: a file that plays to its end also stops
    /// playing, and the pause would save its end as the place to resume.
    fn save_resume(self: &Rc<Self>, s: &SongRef, pos: i64) {
        if self.is_long(s) && pos > 0 && (s.duration_ms <= 0 || pos < s.duration_ms - END_MARGIN_MS) {
            let id = s.id;
            self.app.store(|db| store::resume(db, id, Some(pos)));
        }
    }

    // Listens (plan 008).

    fn open_listen(&self, s: &SongRef, from: i64) {
        let q = self.active();
        let source = q.as_ref().and_then(|q| self.app.prefs.get().sources.get(&q.id).cloned()).unwrap_or_default();
        let mut o = Open {
            song: s.id,
            from_ms: from,
            queue: q.map(|q| q.id).unwrap_or_default(),
            source,
            shuffle: self.shuffle(),
            last_pos: from,
            speed: self.tempo_of(s).speed,
            ..Default::default()
        };
        let mut st = self.st.borrow_mut();
        if st.playing {
            started(&mut o);
        }
        st.open = Some(o);
    }

    fn close(self: &Rc<Self>, end: &str, to_ms: Option<i64>) {
        let Some(mut o) = self.st.borrow_mut().open.take() else { return };
        self.app.prefs.update(|p| p.open_listen.clear());
        count(&mut o);
        // A song that never actually played was not a listen.
        if o.heard_ms <= 0 {
            return;
        }
        let l = listen(&o, end, to_ms.unwrap_or_else(|| self.position_ms()), now_ms());
        self.mark_played(&l);
        self.app.store(|db| store::play(db, &l));
    }

    /// An episode or a book's file is played once a listen reaches 90% into
    /// it. The server marks it itself from the listen; this shows it at once,
    /// offline too, as of the same time, so the two agree (plan 031).
    fn mark_played(&self, l: &Listen) {
        let Some(s) = self.app.catalog().by_id.get(&l.song).cloned() else { return };
        if !s.spoken() || s.duration_ms <= 0 || l.to_ms * 100 < s.duration_ms * store::PLAYED_PERCENT {
            return;
        }
        let db = self.app.db.lock().unwrap();
        if db.mark(store::PLAYED, l.song).ok().flatten().is_some_and(|m| !m.deleted) {
            return;
        }
        let m = dhun_core::model::Mark { kind: store::PLAYED.into(), song: l.song, at: l.ended_at, deleted: false };
        let _ = db.put_mark(&m);
        drop(db);
        self.app.reload_marks();
    }

    fn save_listen(&self) {
        let text = {
            let mut st = self.st.borrow_mut();
            let pos = st.position_ms;
            let Some(o) = st.open.as_mut() else { return };
            count(o);
            o.last_pos = pos;
            o.last_at = now_ms();
            serde_json::to_string(o).unwrap_or_default()
        };
        self.app.prefs.update(|p| p.open_listen = text);
    }

    /// A listen still open when the app last quit (or crashed) is closed as interrupted.
    fn close_interrupted(self: &Rc<Self>) {
        let text = self.app.prefs.get().open_listen;
        if text.is_empty() {
            return;
        }
        self.app.prefs.update(|p| p.open_listen.clear());
        let Ok(o) = serde_json::from_str::<Open>(&text) else { return };
        if o.heard_ms > 0 {
            let l = listen(&o, "interrupted", o.last_pos, o.last_at);
            self.app.store(|db| store::play(db, &l));
        }
    }

    /// On quitting: the open listen and the positions are saved, as on pause.
    pub fn quitting(self: &Rc<Self>) {
        if self.is_playing() {
            self.pause();
        }
        self.save_listen();
        self.engine.stop();
    }
}

fn started(o: &mut Open) {
    o.since = Some(Instant::now());
    if o.started_at == 0 {
        o.started_at = now_ms();
    }
}

/// A song's own speed and pitch, else for an episode or a book's file its
/// collection's (`speed.podcast`, `speed.audiobook`), synced, so a 39-file
/// book is not set 39 times; else this computer's everyday one.
fn tempo_in(s: &SongRef, settings: &synced::Settings, everyday: Tempo) -> Tempo {
    Tempo::of(settings.get(&tempo::song_setting(s.id)))
        .or_else(|| if s.spoken() { Tempo::of(settings.get(&tempo::kind_setting(&s.kind))) } else { None })
        .unwrap_or(everyday)
}

/// Adds the song time played since the last count: the clock's, times the speed (plan 008).
fn count(o: &mut Open) {
    if let Some(since) = o.since {
        let now = Instant::now();
        let speed = if o.speed > 0.0 { o.speed } else { 1.0 };
        o.heard_ms += (now.duration_since(since).as_millis() as f64 * speed) as i64;
        o.since = Some(now);
    }
}

fn listen(o: &Open, end: &str, to_ms: i64, ended_at: i64) -> Listen {
    let offset = glib::DateTime::from_unix_local(o.started_at / 1000).map_or(0, |d| d.utc_offset().as_seconds() / 60);
    Listen {
        song: o.song,
        started_at: o.started_at,
        ended_at,
        ms: o.heard_ms,
        from_ms: o.from_ms,
        to_ms,
        end: end.into(),
        source: o.source.clone(),
        queue: o.queue.clone(),
        shuffle: o.shuffle,
        utc_offset: offset,
    }
}
