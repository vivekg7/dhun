//! Favorites, Listen Later and the automatic views (plan 010), the Mac's
//! `ListKind`: what each holds, worked out when its page is built.

use crate::app::App;
use dhun_core::catalog::{SongRef, VIEW_SIZE};
use dhun_core::model::PlayStat;
use dhun_core::store;
use dhun_core::time::now_ms;

#[derive(Clone, Copy, PartialEq, Debug)]
pub enum ListKind {
    Favorites,
    ListenLater,
    ContinueListening,
    RecentlyAdded,
    RecentlyPlayed,
    MostPlayed,
    NotLately,
}

impl ListKind {
    pub const ALL: [ListKind; 7] = [
        ListKind::Favorites,
        ListKind::ListenLater,
        ListKind::ContinueListening,
        ListKind::RecentlyAdded,
        ListKind::RecentlyPlayed,
        ListKind::MostPlayed,
        ListKind::NotLately,
    ];

    pub fn title(self) -> &'static str {
        match self {
            ListKind::Favorites => "Favorites",
            ListKind::ListenLater => "Listen Later",
            ListKind::ContinueListening => "Continue listening",
            ListKind::RecentlyAdded => "Recently added",
            ListKind::RecentlyPlayed => "Recently played",
            ListKind::MostPlayed => "Most played",
            ListKind::NotLately => "Not played lately",
        }
    }

    pub fn icon(self) -> &'static str {
        match self {
            ListKind::Favorites => "starred-symbolic",
            ListKind::ListenLater => "alarm-symbolic",
            ListKind::ContinueListening => "media-playback-start-symbolic",
            ListKind::RecentlyAdded => "list-add-symbolic",
            ListKind::RecentlyPlayed => "document-open-recent-symbolic",
            ListKind::MostPlayed => "view-list-ordered-symbolic",
            ListKind::NotLately => "preferences-system-time-symbolic",
        }
    }

    /// The queue source the server logs with each listen (plan 008).
    pub fn source(self) -> &'static str {
        match self {
            ListKind::Favorites => "favorites",
            ListKind::ListenLater => "listen_later",
            ListKind::ContinueListening => "continue",
            ListKind::RecentlyAdded => "recently_added",
            ListKind::RecentlyPlayed => "recently_played",
            ListKind::MostPlayed => "most_played",
            ListKind::NotLately => "not_played_lately",
        }
    }

    /// The mark kind of the two lists the user edits.
    pub fn mark(self) -> Option<&'static str> {
        match self {
            ListKind::Favorites => Some(store::FAV),
            ListKind::ListenLater => Some(store::LATER),
            _ => None,
        }
    }

    pub fn songs(self, app: &App) -> Vec<SongRef> {
        let c = app.catalog();
        let m = app.marks.borrow();
        let db = app.db.lock().unwrap();
        // The songs played whose stats `keep` passes, most first by `key`.
        let top = |keep: &dyn Fn(&PlayStat) -> bool, key: fn(&PlayStat) -> i64| -> Vec<SongRef> {
            let mut s: Vec<_> = db.play_stats().unwrap_or_default().into_values().filter(|s| keep(s)).collect();
            s.sort_by_key(|s| (std::cmp::Reverse(key(s)), s.song));
            c.songs_of(&s.iter().map(|s| s.song).collect::<Vec<_>>()).into_iter().take(VIEW_SIZE).collect()
        };
        match self {
            ListKind::Favorites => c.songs_of(&m.favs),
            ListKind::ListenLater => c.songs_of(&m.later),
            ListKind::ContinueListening => {
                c.songs_of(&db.resumes().unwrap_or_default().iter().map(|r| r.song).collect::<Vec<_>>())
            }
            ListKind::RecentlyAdded => c.recently_added.clone(),
            ListKind::RecentlyPlayed => top(&|s| s.last_played_at > 0, |s| s.last_played_at),
            ListKind::MostPlayed => top(&|s| s.count > 0, |s| s.count),
            ListKind::NotLately => {
                // Played at least 3 times, and not in the last 90 days: favourites you have drifted away from.
                let cutoff = now_ms() - 90 * 86_400_000;
                top(&|s| s.count >= 3 && s.last_played_at < cutoff, |s| s.count)
            }
        }
    }
}
