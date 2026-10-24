//! The settings that follow the user to every device: each one's name, the
//! value used while it is unset, and the values Settings offers (plan 034).
//! The phone, the Mac and the web keep the same table, and
//! `scripts/check-settings.sh` fails when a default differs.

use serde_json::Value;
use std::collections::HashMap;

pub type Settings = HashMap<String, Value>;

pub struct Int(pub &'static str, pub i64, pub &'static [i64]);
pub struct Bool(pub &'static str, pub bool);
pub struct Text(pub &'static str, pub &'static str, pub &'static [&'static str]);

impl Int {
    pub fn get(&self, s: &Settings) -> i64 {
        s.get(self.0).and_then(Value::as_i64).unwrap_or(self.1)
    }
}
impl Bool {
    pub fn get(&self, s: &Settings) -> bool {
        s.get(self.0).and_then(Value::as_bool).unwrap_or(self.1)
    }
}
impl Text {
    pub fn get<'a>(&'a self, s: &'a Settings) -> &'a str {
        s.get(self.0).and_then(Value::as_str).unwrap_or(self.1)
    }
}

pub const PREVIOUS_RESTART: Bool = Bool("previous.restart", true);
pub const MIN_MINUTES: Int = Int("longFiles.minMinutes", 15, &[5, 10, 15, 20, 30, 60]);
pub const RESUME: Text = Text("longFiles.resume", "auto", &["auto", "ask", "off"]);
pub const PREVIOUS: Text = Text("longFiles.previous", "keep", &["keep", "seek", "ignore", "songs"]);
pub const NEXT: Text = Text("longFiles.next", "next", &["next", "seek"]);
pub const SEEK_BACK: Int = Int("longFiles.seekBack", 10, &[5, 10, 15, 30, 60]);
pub const SEEK_FORWARD: Int = Int("longFiles.seekForward", 30, &[10, 15, 30, 60, 120]);
pub const REWIND: Int = Int("longFiles.rewind", 10, &[0, 5, 10, 15, 30]);
pub const REWIND_AFTER: Int = Int("longFiles.rewindAfter", 5, &[1, 5, 10, 30, 60]);
pub const CACHE_AHEAD: Int = Int("longFiles.cacheAhead", 30, &[10, 15, 30, 60, 0]);
pub const AUTO_REMOVE: Bool = Bool("listenLater.autoRemove", true);
pub const FINISHED_PERCENT: Int = Int("listenLater.finishedPercent", 90, &[80, 90, 95, 100]);
/// Whether a show's page leaves out the episodes already played: set on that page, not in Settings.
pub const HIDE_PLAYED: Bool = Bool("podcasts.hidePlayed", true);

/// The ones on the Playback page, which its Reset to Defaults clears.
pub const PLAYBACK: [&str; 12] = [
    PREVIOUS_RESTART.0,
    MIN_MINUTES.0,
    RESUME.0,
    PREVIOUS.0,
    NEXT.0,
    SEEK_BACK.0,
    SEEK_FORWARD.0,
    REWIND.0,
    REWIND_AFTER.0,
    CACHE_AHEAD.0,
    AUTO_REMOVE.0,
    FINISHED_PERCENT.0,
];

/// The synced setting with the queues' order: their ids, as a JSON array.
pub const QUEUE_ORDER: &str = "queues.order";
