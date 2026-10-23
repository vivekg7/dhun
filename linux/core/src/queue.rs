//! The parts of playback that need no player: the queues' order, the play
//! order the engine reads, and where long files go on from (plans 009, 020).
//! The app's `Playback` holds the rest, as the Mac's does.

use crate::model::QueueRow;
use serde_json::Value;
use std::collections::{HashMap, HashSet};

pub const MAX_QUEUES: usize = 20;
/// A hand-off never starts closer than this to the song's end, nor is a resume point kept there.
pub const END_MARGIN_MS: i64 = 5000;
/// Previous restarts a song played past this many seconds, as Media3's default.
pub const RESTART_WINDOW: f64 = 3.0;

/// The queues in the user's order: those the synced order names, in it,
/// then the rest by use, oldest first, as Musicolet appends.
pub fn order_of(queues: &[QueueRow], order: Option<&Value>) -> Vec<QueueRow> {
    let ids: Vec<&str> =
        order.and_then(Value::as_array).map_or(vec![], |a| a.iter().filter_map(Value::as_str).collect());
    let by_id: HashMap<&str, &QueueRow> = queues.iter().map(|q| (q.id.as_str(), q)).collect();
    let mut seen = HashSet::new();
    let mut out: Vec<QueueRow> =
        ids.into_iter().filter(|id| seen.insert(*id)).filter_map(|id| by_id.get(id).map(|q| (*q).clone())).collect();
    let named: HashSet<String> = out.iter().map(|q| q.id.clone()).collect();
    let mut rest: Vec<QueueRow> = queues.iter().filter(|q| !named.contains(&q.id)).cloned().collect();
    rest.sort_by_key(|q| q.used_at);
    out.extend(rest);
    out
}

/// Each id once, in first-seen order.
pub fn distinct(ids: impl IntoIterator<Item = i64>) -> Vec<i64> {
    let mut seen = HashSet::new();
    ids.into_iter().filter(|i| seen.insert(*i)).collect()
}

/// The play order as the engine reads it, to pick the song after the one
/// it has decoded to the end; set by the player, read from GStreamer's thread.
#[derive(Clone, Default)]
pub struct Upcoming {
    pub order: Vec<i64>,
    /// off, queue or song.
    pub repeat_mode: String,
    /// The sleep timer's "after this song": nothing follows it.
    pub pause_after: Option<i64>,
}

impl Upcoming {
    pub fn next(&self, after: i64) -> Option<i64> {
        if self.pause_after == Some(after) {
            return None;
        }
        if self.repeat_mode == "song" {
            return Some(after);
        }
        let at = self.order.iter().position(|&s| s == after)?;
        match self.order.get(at + 1) {
            Some(&n) => Some(n),
            None if self.repeat_mode == "queue" => self.order.first().copied(),
            None => None,
        }
    }
}

/// Where to continue a long file stopped at `pos` `stopped_for` ago:
/// `back` earlier once that is at least `after`, never before the start.
pub fn rewound(pos: i64, stopped_for: i64, back: i64, after: i64) -> i64 {
    if back > 0 && stopped_for >= after { (pos - back).max(0) } else { pos }
}

/// The chapter after `pos` among chapters starting at `starts`, or None
/// after the last. One starting within a second is the one just sought to.
pub fn chapter_after(starts: &[i64], pos: i64) -> Option<i64> {
    starts.iter().copied().find(|&s| s > pos + 1000)
}

/// The start of the chapter `pos` is in, or within `window` of it the one
/// before; None at the first chapter's start, or with none.
pub fn chapter_before(starts: &[i64], pos: i64, window: i64) -> Option<i64> {
    let i = starts.iter().rposition(|&s| s <= pos)?;
    if pos - starts[i] > window {
        Some(starts[i])
    } else if i > 0 {
        Some(starts[i - 1])
    } else {
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn q(id: &str, used_at: i64) -> QueueRow {
        QueueRow { id: id.into(), name: id.into(), used_at, repeat_mode: "off".into(), ..Default::default() }
    }

    // The order is a synced setting: it can name queues deleted elsewhere, miss
    // ones made on another device, or come from a newer app in another shape.
    #[test]
    fn the_order_never_loses_or_invents_a_queue() {
        let queues = [q("a", 30), q("b", 10), q("c", 20), q("d", 5)];
        let ids = |o: Option<Value>| order_of(&queues, o.as_ref()).into_iter().map(|q| q.id).collect::<Vec<_>>();
        // Named first, in order; the rest by use, oldest first, as Musicolet appends.
        assert_eq!(ids(serde_json::from_str(r#"["c", "gone", "a", "c", 7]"#).ok()), ["c", "a", "d", "b"]);
        assert_eq!(ids(None), ["d", "b", "c", "a"]);
        assert_eq!(ids(Some(Value::from("a"))), ["d", "b", "c", "a"]);
    }

    #[test]
    fn what_plays_next_follows_repeat_and_the_sleep_timer() {
        let mut u = Upcoming { order: vec![3, 1, 2], repeat_mode: "off".into(), pause_after: None };
        assert_eq!(u.next(3), Some(1));
        assert_eq!(u.next(2), None);
        u.repeat_mode = "queue".into();
        assert_eq!(u.next(2), Some(3));
        u.repeat_mode = "song".into();
        assert_eq!(u.next(1), Some(1));
        u.pause_after = Some(1);
        assert_eq!(u.next(1), None);
    }

    // Going on after a long pause backs up a little; never before the start.
    #[test]
    fn a_long_pause_rewinds_a_little() {
        assert_eq!(rewound(60_000, 10 * 60_000, 10_000, 5 * 60_000), 50_000);
        assert_eq!(rewound(60_000, 60_000, 10_000, 5 * 60_000), 60_000);
        assert_eq!(rewound(4_000, 10 * 60_000, 10_000, 5 * 60_000), 0);
    }

    #[test]
    fn chapters_go_on_and_back() {
        let s = [0, 60_000, 120_000];
        assert_eq!(chapter_after(&s, 59_500), Some(120_000));
        assert_eq!(chapter_after(&s, 130_000), None);
        assert_eq!(chapter_before(&s, 90_000, 3000), Some(60_000));
        assert_eq!(chapter_before(&s, 61_000, 3000), Some(0));
        assert_eq!(chapter_before(&s, 1000, 3000), None);
    }
}
