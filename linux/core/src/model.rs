use serde::{Deserialize, Serialize};

/// A song from the library (api/openapi.yaml `Song`), as the working copy keeps it.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Song {
    pub id: i64,
    pub path: String,
    pub title: String,
    pub artist: String,
    pub artists: Vec<String>,
    pub album: String,
    pub album_artist: String,
    pub composer: String,
    pub genres: Vec<String>,
    pub year: i64,
    pub track: i64,
    pub disc: i64,
    pub duration_ms: i64,
    pub format: String,
    pub bitrate: i64,
    pub sample_rate: i64,
    pub bit_depth: i64,
    pub size: i64,
    pub has_art: bool,
    pub has_lyrics: bool,
    pub added_at: i64,
    pub missing: bool,
    /// Which cover it shows, shared by songs showing the same one (plan 019).
    pub art: String,
    /// "" for music, else `podcast` or `audiobook` (plan 031).
    pub kind: String,
    /// The release date in full, for an episode.
    pub date: String,
    pub notes: String,
    pub chapters: Vec<Chapter>,
    /// The book or show it belongs to, as the server grouped them, and its place there.
    pub group_key: String,
    pub group_title: String,
    pub group_index: i64,
}

pub const PODCAST: &str = "podcast";
pub const AUDIOBOOK: &str = "audiobook";

impl Song {
    /// A podcast episode or an audiobook's file: kept out of music lists and playlists.
    pub fn spoken(&self) -> bool {
        !self.kind.is_empty()
    }

    pub fn folder(&self) -> &str {
        self.path.rfind('/').map_or("", |i| &self.path[..i])
    }

    pub fn display_artist(&self) -> &str {
        if !self.artist.is_empty() {
            &self.artist
        } else if !self.album_artist.is_empty() {
            &self.album_artist
        } else {
            "Unknown artist"
        }
    }

    /// An episode's date tag as a day. Some tools write 20251230 for 2025-12-30.
    pub fn day(&self) -> String {
        let d = &self.date;
        if d.len() == 8 && d.bytes().all(|b| b.is_ascii_digit()) {
            return format!("{}-{}-{}", &d[..4], &d[4..6], &d[6..]);
        }
        d.chars().take(10).collect()
    }
}

/// Where a chapter of an episode or a book's file starts.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct Chapter {
    pub start_ms: i64,
    pub title: String,
}

/// A playlist as the server indexed it. One made here has a negative id
/// until the server's comes back with the same `ref` (plan 013).
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Playlist {
    pub id: i64,
    pub name: String,
    pub path: String,
    pub shared: bool,
    pub songs: Vec<i64>,
    pub r#ref: String,
}

#[derive(Clone, Debug, Default, Deserialize)]
#[serde(default)]
pub struct User {
    pub id: i64,
    pub name: String,
    pub admin: bool,
}
