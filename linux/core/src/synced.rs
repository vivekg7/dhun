//! The settings that follow the user to every device: each one's name, and
//! the value used while it is unset (plan 034). The phone, the Mac and the
//! web keep the same table, and `scripts/check-settings.sh` fails when a
//! default differs.

use serde_json::Value;
use std::collections::HashMap;

pub type Settings = HashMap<String, Value>;

pub struct Int(pub &'static str, pub i64);
pub struct Bool(pub &'static str, pub bool);
pub struct Text(pub &'static str, pub &'static str);

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
pub const MIN_MINUTES: Int = Int("longFiles.minMinutes", 15);
/// auto, ask or off.
pub const RESUME: Text = Text("longFiles.resume", "auto");
/// keep, seek, ignore or songs.
pub const PREVIOUS: Text = Text("longFiles.previous", "keep");
/// next or seek.
pub const NEXT: Text = Text("longFiles.next", "next");
pub const SEEK_BACK: Int = Int("longFiles.seekBack", 10);
pub const SEEK_FORWARD: Int = Int("longFiles.seekForward", 30);
pub const REWIND: Int = Int("longFiles.rewind", 10);
pub const REWIND_AFTER: Int = Int("longFiles.rewindAfter", 5);
pub const CACHE_AHEAD: Int = Int("longFiles.cacheAhead", 30);
pub const AUTO_REMOVE: Bool = Bool("listenLater.autoRemove", true);
pub const FINISHED_PERCENT: Int = Int("listenLater.finishedPercent", 90);
pub const HIDE_PLAYED: Bool = Bool("podcasts.hidePlayed", true);

/// The synced setting with the queues' order: their ids, as a JSON array.
pub const QUEUE_ORDER: &str = "queues.order";
