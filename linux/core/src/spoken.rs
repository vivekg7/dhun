//! What the user has heard of the spoken files (plan 031): those marked
//! played, and where the others were left. As the Mac's `Heard`.

use crate::catalog::Shelf;
use crate::model::Song;
use std::collections::{HashMap, HashSet};

#[derive(Default)]
pub struct Heard {
    pub played: HashSet<i64>,
    /// Where each long file was left, by song (plan 009).
    pub left: HashMap<i64, i64>,
}

impl Heard {
    pub fn heard_ms(&self, s: &Song) -> i64 {
        if self.played.contains(&s.id) {
            s.duration_ms
        } else {
            self.left.get(&s.id).copied().unwrap_or(0).clamp(0, s.duration_ms.max(0))
        }
    }

    /// Where a book carries on: its first file not played, or from the start once all are.
    pub fn next<'a>(&self, shelf: &'a Shelf) -> &'a Song {
        shelf.items.iter().find(|s| !self.played.contains(&s.id)).unwrap_or(&shelf.items[0])
    }

    pub fn finished(&self, shelf: &Shelf) -> bool {
        shelf.items.iter().all(|s| self.played.contains(&s.id))
    }

    /// Begun and not finished: its Play button says Continue. Finished, it starts again from the first.
    pub fn begun(&self, shelf: &Shelf) -> bool {
        shelf.items.iter().any(|s| self.heard_ms(s) > 0) && !self.finished(shelf)
    }

    /// A book's progress, or its length before it is begun.
    pub fn progress(&self, shelf: &Shelf) -> String {
        let total: i64 = shelf.items.iter().map(|s| s.duration_ms).sum();
        let heard: i64 = shelf.items.iter().map(|s| self.heard_ms(s)).sum();
        if self.finished(shelf) {
            "Finished".into()
        } else if heard <= 0 || total <= 0 {
            length(total)
        } else {
            format!("{}% · {} left", heard * 100 / total, length(total - heard))
        }
    }

    /// A file's line beside its title: played, or how much is left of one begun.
    pub fn state(&self, s: &Song) -> String {
        if self.played.contains(&s.id) {
            return "Played".into();
        }
        let h = self.heard_ms(s);
        if h > 0 { format!("{} left", length(s.duration_ms - h)) } else { String::new() }
    }
}

/// "48 min", "12 h 5 min"
pub fn length(ms: i64) -> String {
    let min = ms / 60_000;
    if min >= 60 { format!("{} h {} min", min / 60, min % 60) } else { format!("{min} min") }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::catalog::Catalog;
    use crate::model::AUDIOBOOK;
    use std::sync::Arc;

    #[test]
    fn a_book_carries_on_from_its_first_file_not_played() {
        let song = |id: i64| {
            Arc::new(Song {
                id,
                path: format!("Dune/{id}.mp3"),
                title: id.to_string(),
                duration_ms: 600_000,
                kind: AUDIOBOOK.into(),
                group_key: "dune".into(),
                group_title: "DUNE".into(),
                group_index: id,
                ..Default::default()
            })
        };
        let c = Catalog::new((1..=3).map(song).collect());
        let book = &c.books[0];
        let heard = Heard { played: [1].into(), left: [(2, 300_000)].into() };
        assert_eq!(heard.next(book).id, 2);
        assert_eq!(heard.progress(book), "50% · 15 min left");
        assert!(heard.begun(book));
        let done = Heard { played: [1, 2, 3].into(), left: HashMap::new() };
        assert_eq!(done.next(book).id, 1);
        // Finished, it starts again: Play, not Continue.
        assert!(!done.begun(book));
    }
}
