//! Downloads (plan 012), as on the phone and the Mac: the user pins things
//! (an album, a playlist, Favorites); the pins are turned into songs again
//! whenever the library or the lists change, and the files on disk follow.
//! What needs no display is here; the app runs it.

use crate::api::{Api, Error, Result};
use crate::catalog::{Catalog, Group, SongRef};
use crate::model::{Mark, Pin, Playlist, Song};
use std::collections::HashSet;
use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use std::sync::Arc;

pub const SONG: &str = "song";
pub const ALBUM: &str = "album";
pub const FOLDER: &str = "folder";
pub const ARTIST: &str = "artist";
pub const GENRE: &str = "genre";
pub const PLAYLIST: &str = "playlist";
/// Favorites or Listen Later; the pin's ref is the mark kind.
pub const LIST: &str = "list";
/// An audiobook or a podcast show (plan 031); the pin's ref is its shelf id.
pub const SHELF: &str = "shelf";

pub const GB: u64 = 1 << 30;
/// Left free on the disk, so a full disk does not break the rest of the computer.
pub const RESERVE: u64 = 200 << 20;

pub fn key(kind: &str, r#ref: &str) -> String {
    format!("{kind}:{}", r#ref)
}

/// The songs `pins` cover, newest pin first so what was just clicked starts
/// first, each once; None while the catalogue is empty, because an empty
/// catalogue would read as "delete every download".
pub fn covered(
    pins: &[Pin],
    c: &Catalog,
    playlists: &[Playlist],
    favs: &[Mark],
    later: &[Mark],
) -> Option<Vec<SongRef>> {
    if c.by_id.is_empty() {
        return None;
    }
    let mut ids = vec![];
    let mut seen = HashSet::new();
    let mut add = |id: i64| {
        if seen.insert(id) {
            ids.push(id);
        }
    };
    let mut pins: Vec<&Pin> = pins.iter().collect();
    pins.sort_by_key(|p| std::cmp::Reverse(p.at));
    for p in pins {
        let group = |list: &[Arc<Group>]| {
            let lower = p.r#ref.to_lowercase();
            list.iter().find(|g| g.id() == lower).map(|g| g.songs.iter().map(|s| s.id).collect::<Vec<_>>())
        };
        let songs: Vec<i64> = match p.kind.as_str() {
            SONG => p.r#ref.parse().into_iter().collect(),
            ALBUM => c.albums_by_id.get(&p.r#ref).map(|a| a.songs.iter().map(|s| s.id).collect()).unwrap_or_default(),
            FOLDER => c.all_songs(&p.r#ref).iter().map(|s| s.id).collect(),
            ARTIST => group(&c.artists).unwrap_or_default(),
            GENRE => group(&c.genres).unwrap_or_default(),
            PLAYLIST => {
                playlists.iter().find(|l| l.id.to_string() == p.r#ref).map(|l| l.songs.clone()).unwrap_or_default()
            }
            LIST => (if p.r#ref == crate::store::FAV { favs } else { later }).iter().map(|m| m.song).collect(),
            SHELF => c.shelves.get(&p.r#ref).map(|s| s.items.iter().map(|s| s.id).collect()).unwrap_or_default(),
            _ => vec![],
        };
        songs.into_iter().for_each(&mut add);
    }
    Some(c.songs_of(&ids))
}

/// Fetches one song into `dir` as `<id>.<ext>`, through a `.part` file so a
/// half-written file is never played; None if the server no longer has it.
/// A part left by a broken try is carried on from where it ends. `progress`
/// hears the bytes here so far.
pub fn fetch(api: &Api, s: &Song, dir: &Path, progress: impl Fn(u64)) -> Result<Option<PathBuf>> {
    let io = |e: std::io::Error| Error::Network(e.to_string());
    let file = dir.join(s.file_name());
    let part = dir.join(s.file_name() + ".part");
    let size = s.size.max(0) as u64;
    let mut out = std::fs::OpenOptions::new().create(true).append(true).open(&part).map_err(io)?;
    let mut have = out.metadata().map_err(io)?.len();
    // Longer than the song: the file changed on the server. Start again.
    if have > size {
        out.set_len(0).map_err(io)?;
        have = 0;
    }
    let url = api.stream_url(s.id);
    let mut buf = vec![0; 256 << 10];
    while have < size {
        let mut body = match api.range(&url, have) {
            Ok(b) => b,
            Err(Error::Http { code: 404 | 410, .. }) => {
                let _ = std::fs::remove_file(&part);
                return Ok(None);
            }
            Err(e) => return Err(e),
        };
        let before = have;
        loop {
            let n = body.read(&mut buf).map_err(io)?;
            if n == 0 {
                break;
            }
            out.write_all(&buf[..n]).map_err(io)?;
            have += n as u64;
            progress(have);
        }
        if have == before {
            break;
        }
    }
    // Against what the server sent: the song list may be older than the file.
    if have != size {
        let _ = std::fs::remove_file(&part);
        return Err(Error::Network(format!("Got {have} of {size} bytes")));
    }
    std::fs::rename(&part, &file).map_err(io)?;
    Ok(Some(file))
}

/// "1.2 GB", "340 MB"
pub fn format_bytes(n: u64) -> String {
    if n >= GB {
        format!("{:.1} GB", n as f64 / GB as f64)
    } else if n >= 1 << 20 {
        format!("{} MB", n >> 20)
    } else {
        format!("{} KB", n >> 10)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn song(id: i64, path: &str) -> SongRef {
        Arc::new(Song { id, path: path.into(), title: path.into(), format: "mpeg".into(), ..Default::default() })
    }

    fn pin(kind: &str, r#ref: &str, at: i64) -> Pin {
        Pin { key: key(kind, r#ref), kind: kind.into(), r#ref: r#ref.into(), name: r#ref.into(), at }
    }

    // Before the catalogue loads, nothing is covered: an empty answer would delete every file.
    #[test]
    fn an_empty_catalogue_covers_nothing_rather_than_no_songs() {
        assert_eq!(covered(&[pin(SONG, "1", 0)], &Catalog::empty(), &[], &[], &[]), None);
    }

    // A folder keeps its subfolders, a song in two pins is wanted once, and the newest pin comes first.
    #[test]
    fn pins_become_songs_once_newest_first() {
        let c = Catalog::new(vec![
            song(1, "Hindi/90s/a.mp3"),
            song(2, "Hindi/90s/Live/b.mp3"),
            song(3, "Hindi/c.mp3"),
            song(4, "English/d.mp3"),
        ]);
        let pins = [pin(FOLDER, "Hindi/90s", 1), pin(PLAYLIST, "7", 3), pin(LIST, crate::store::FAV, 2)];
        let playlists = [Playlist { id: 7, name: "Mix".into(), songs: vec![4, 2, 0], ..Default::default() }];
        let mark = |song| Mark { kind: crate::store::FAV.into(), song, at: 0, deleted: false };
        let ids: Vec<i64> =
            covered(&pins, &c, &playlists, &[mark(3), mark(1)], &[]).unwrap().iter().map(|s| s.id).collect();
        assert_eq!(ids, [4, 2, 3, 1]);
    }

    // An album's id holds a NUL between its name and its artist or folder; as
    // a C string the Mac's database cut it there, and the album's download
    // then matched no album.
    #[test]
    fn an_album_download_survives_the_database() {
        let s = Song { album: "Bravo".into(), album_artist: "Znaider".into(), ..(*song(1, "A/a.mp3")).clone() };
        let c = Catalog::new(vec![Arc::new(s)]);
        let id = c.albums[0].id.clone();
        let path = std::env::temp_dir().join(format!("dhun-pins-{}.db", std::process::id()));
        let db = crate::db::Db::open(&path).unwrap();
        db.put_pin(&pin(ALBUM, &id, 0)).unwrap();
        let ids: Vec<i64> = covered(&db.pins().unwrap(), &c, &[], &[], &[]).unwrap().iter().map(|s| s.id).collect();
        assert_eq!(ids, [1]);
        drop(db);
        for ext in ["", "-wal", "-shm"] {
            let _ = std::fs::remove_file(format!("{}{ext}", path.display()));
        }
    }
}
