//! The sleep timer (plan 014), as on the phone and the Mac: after some
//! minutes, at the end of this song, after a number of songs, at the end
//! of the queue, or after a song picked in the queue (plan 020). A timer by
//! the clock fades out over its last seconds and pauses on time, unless it
//! lets the song finish; the others pause where a song ends, which needs no
//! fade.

use super::Playback;
use dhun_core::time::now_ms;
use gtk::glib;
use std::rc::Rc;
use std::time::Duration;

#[derive(Clone, PartialEq, Debug)]
pub enum Sleep {
    /// At this time, in ms since the epoch.
    At(i64),
    EndOfSong,
    /// This many songs to go, the playing one included.
    Songs(usize),
    EndOfQueue,
    /// Musicolet's "Stop after this song", for a song further down the queue.
    AfterSong(i64, String),
}

/// How long the volume takes to fall to nothing before a timer by the clock pauses.
const FADE_MS: i64 = 10_000;

impl Playback {
    pub fn sleep(&self) -> Option<Sleep> {
        self.sleep.borrow().clone()
    }

    /// With `finish_song`, the time up turns the timer into "at the end of
    /// this song", so the song ends on its own and nothing fades (plan 034).
    pub fn sleep_minutes(self: &Rc<Self>, minutes: i64, finish_song: bool) {
        let end = now_ms() + minutes * 60_000;
        self.set_sleep(Sleep::At(end));
        let w = Rc::downgrade(self);
        let id = glib::timeout_add_local(Duration::from_millis(100), move || {
            let Some(p) = w.upgrade() else { return glib::ControlFlow::Break };
            let left = end - now_ms();
            if finish_song {
                if left > 0 {
                    return glib::ControlFlow::Continue;
                }
                p.fading.borrow_mut().take();
                // Paused already, there is no song to finish.
                if p.is_playing() {
                    p.set_sleep(Sleep::EndOfSong)
                } else {
                    p.sleep_cancel()
                }
                return glib::ControlFlow::Break;
            }
            if left > FADE_MS {
                return glib::ControlFlow::Continue;
            }
            if left > 0 {
                p.engine.set_volume(left as f64 / FADE_MS as f64);
                return glib::ControlFlow::Continue;
            }
            p.fading.borrow_mut().take();
            p.pause();
            p.sleep_cancel();
            glib::ControlFlow::Break
        });
        *self.fading.borrow_mut() = Some(id);
    }

    pub fn sleep_end_of_song(self: &Rc<Self>) {
        self.set_sleep(Sleep::EndOfSong);
    }
    pub fn sleep_songs(self: &Rc<Self>, n: usize) {
        self.set_sleep(if n <= 1 { Sleep::EndOfSong } else { Sleep::Songs(n) });
    }
    pub fn sleep_end_of_queue(self: &Rc<Self>) {
        self.set_sleep(Sleep::EndOfQueue);
    }
    pub fn sleep_after_song(self: &Rc<Self>, song: i64, title: &str) {
        self.set_sleep(Sleep::AfterSong(song, title.into()));
    }

    pub fn sleep_cancel(self: &Rc<Self>) {
        if let Some(id) = self.fading.borrow_mut().take() {
            id.remove();
        }
        self.engine.set_volume(1.0);
        self.sleep.borrow_mut().take();
        self.sleep_changed();
    }

    fn set_sleep(self: &Rc<Self>, m: Sleep) {
        self.sleep_cancel();
        *self.sleep.borrow_mut() = Some(m);
        self.sleep_changed();
    }

    /// "in 23 min", "after this song", …, or None with no timer.
    pub fn sleep_label(&self) -> Option<String> {
        Some(match self.sleep.borrow().as_ref()? {
            Sleep::At(end) => format!("in {} min", ((end - now_ms() + 59_999) / 60_000).max(1)),
            Sleep::EndOfSong => "after this song".into(),
            Sleep::Songs(n) => format!("after {n} songs"),
            Sleep::EndOfQueue => "at the end of the queue".into(),
            Sleep::AfterSong(_, title) => format!("after “{title}”"),
        })
    }

    /// The song the player stops after, when the timer says the playing one is the last.
    pub(super) fn pause_after(&self) -> Option<i64> {
        let current = self.current()?.id;
        let stop = match self.sleep.borrow().as_ref()? {
            Sleep::AfterSong(song, _) => *song == current,
            Sleep::EndOfSong => true,
            Sleep::EndOfQueue => {
                let wrap = self.repeat_mode() == "queue";
                self.current_index().and_then(|i| self.next_index(i, wrap)).is_none()
            }
            _ => false,
        };
        stop.then_some(current)
    }

    /// A song started, on its own or skipped to: one fewer to go, and maybe this is the last.
    pub(super) fn sleep_next_song(self: &Rc<Self>) {
        let now = match self.sleep.borrow().as_ref() {
            Some(Sleep::Songs(left)) if *left <= 2 => Some(Sleep::EndOfSong),
            Some(Sleep::Songs(left)) => Some(Sleep::Songs(left - 1)),
            _ => None,
        };
        if let Some(m) = now {
            *self.sleep.borrow_mut() = Some(m);
        }
        self.publish_upcoming();
    }

    /// The player paused itself at the end of a song: a timer by the song is done.
    pub(super) fn sleep_paused_at_end(self: &Rc<Self>) {
        if self.sleep.borrow().as_ref().is_some_and(|m| !matches!(m, Sleep::At(_))) {
            self.sleep_cancel();
        }
    }

    fn sleep_changed(self: &Rc<Self>) {
        self.publish_upcoming();
        self.changed();
    }
}
