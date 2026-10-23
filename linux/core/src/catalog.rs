//! The library as the sidebar shows it, derived from the song list in
//! memory, as on the phone and the Mac. About 30,000 songs today, designed
//! for 100,000 (plan 036): a whole rebuild off the main thread still beats
//! keeping albums, artists and folders as tables, as long as nothing in it
//! is done once per comparison or once per row drawn. Built whole and never
//! changed after, so any thread may read it.

use crate::model::{AUDIOBOOK, PODCAST, Song};
use crate::search::{self, Index, Query, field};
use std::collections::{HashMap, HashSet};
use std::sync::{Arc, OnceLock};

pub type SongRef = Arc<Song>;

pub struct Album {
    pub id: String,
    pub name: String,
    pub artist: String,
    pub year: i64,
    pub songs: Vec<SongRef>,
}

pub struct Group {
    pub name: String,
    pub songs: Vec<SongRef>,
}

impl Group {
    pub fn id(&self) -> String {
        self.name.to_lowercase()
    }
}

/// An audiobook or a podcast show (plan 031): a book's files in reading
/// order, a show's episodes oldest first.
pub struct Shelf {
    pub id: String,
    pub kind: String,
    pub title: String,
    /// The author, or the host; "" when the tags name none.
    pub author: String,
    pub items: Vec<SongRef>,
}

impl Shelf {
    pub fn is_book(&self) -> bool {
        self.kind == AUDIOBOOK
    }
    /// The first file with a cover: the one its files without one show.
    pub fn cover(&self) -> Option<&SongRef> {
        self.items.iter().find(|s| s.has_art)
    }
}

pub struct Folder {
    pub path: String,
    /// The paths of its subfolders, in folder order.
    pub children: Vec<String>,
    pub songs: Vec<SongRef>,
    /// This folder's songs and every subfolder's, counted once when the catalogue is built.
    pub song_count: usize,
    pub duration_ms: i64,
}

impl Folder {
    pub fn name(&self) -> &str {
        match self.path.rfind('/') {
            Some(i) => &self.path[i + 1..],
            None if self.path.is_empty() => "Music",
            None => &self.path,
        }
    }
}

pub const PODCASTS: &str = "Podcasts";
pub const AUDIOBOOKS: &str = "Audiobooks";

/// Each automatic view shows at most this many songs, as on the phone.
pub const VIEW_SIZE: usize = 100;

pub struct Catalog {
    /// The music: what every song list, view and search of songs shows.
    /// Podcasts and audiobooks have their own pages (`books`, `shows`), so
    /// an episode never turns up in an artist or a shuffle (plan 031).
    pub songs: Vec<SongRef>,
    /// Every song, episodes and books' files too: what queues, Favorites and Listen Later name.
    pub by_id: HashMap<i64, SongRef>,
    pub albums: Vec<Arc<Album>>,
    pub albums_by_id: HashMap<String, Arc<Album>>,
    /// Each song's album, for "Go to Album" and an artist's album strip.
    pub album_of: HashMap<i64, Arc<Album>>,
    pub artists: Vec<Arc<Group>>,
    pub genres: Vec<Arc<Group>>,
    pub folders: HashMap<String, Folder>,
    pub books: Vec<Arc<Shelf>>,
    pub shows: Vec<Arc<Shelf>>,
    pub shelves: HashMap<String, Arc<Shelf>>,
    /// The newest `VIEW_SIZE` songs, newest first: the automatic view (plan 010).
    pub recently_added: Vec<SongRef>,
    song_index: OnceLock<Index<SongRef>>,
    album_index: OnceLock<Index<Arc<Album>>>,
    artist_index: OnceLock<Index<Arc<Group>>>,
    genre_index: OnceLock<Index<Arc<Group>>>,
    folder_index: OnceLock<Index<String>>,
    book_index: OnceLock<Index<Arc<Shelf>>>,
    show_index: OnceLock<Index<Arc<Shelf>>>,
    episode_index: OnceLock<Index<SongRef>>,
}

impl Catalog {
    pub fn new(all: Vec<SongRef>) -> Catalog {
        let songs = natural_sorted(all.iter().filter(|s| !s.spoken()).cloned().collect(), |s| &s.title);
        let mut by_id = HashMap::with_capacity(all.len());
        for s in &all {
            by_id.entry(s.id).or_insert_with(|| s.clone());
        }
        let albums = albums(&songs);
        let mut albums_by_id = HashMap::with_capacity(albums.len());
        let mut album_of = HashMap::with_capacity(songs.len());
        for a in &albums {
            albums_by_id.entry(a.id.clone()).or_insert_with(|| a.clone());
            for s in &a.songs {
                album_of.insert(s.id, a.clone());
            }
        }
        let artists =
            group(
                &songs,
                |s| {
                    if s.artists.is_empty() { vec![s.display_artist().to_string()] } else { s.artists.clone() }
                },
            );
        let genres =
            group(&songs, |s| if s.genres.is_empty() { vec!["Unknown genre".into()] } else { s.genres.clone() });
        let spoken: Vec<SongRef> = all.iter().filter(|s| s.spoken()).cloned().collect();
        let folders = folders(&songs, &spoken);
        let books = shelves(&spoken, AUDIOBOOK);
        let shows = shelves(&spoken, PODCAST);
        let shelves = books.iter().chain(&shows).map(|s| (s.id.clone(), s.clone())).collect();
        // Indices, not songs: the sort moves only numbers.
        let mut order: Vec<usize> = (0..songs.len()).collect();
        order.sort_by_key(|&i| std::cmp::Reverse(songs[i].added_at));
        let recently_added = order.into_iter().take(VIEW_SIZE).map(|i| songs[i].clone()).collect();
        Catalog {
            songs,
            by_id,
            albums,
            albums_by_id,
            album_of,
            artists,
            genres,
            folders,
            books,
            shows,
            shelves,
            recently_added,
            song_index: OnceLock::new(),
            album_index: OnceLock::new(),
            artist_index: OnceLock::new(),
            genre_index: OnceLock::new(),
            folder_index: OnceLock::new(),
            book_index: OnceLock::new(),
            show_index: OnceLock::new(),
            episode_index: OnceLock::new(),
        }
    }

    pub fn empty() -> Catalog {
        Catalog::new(vec![])
    }

    #[cfg(test)]
    fn of(songs: Vec<Song>) -> Catalog {
        Catalog::new(songs.into_iter().map(Arc::new).collect())
    }

    /// The shelf `s` is on, or None for music.
    pub fn shelf_of(&self, s: &Song) -> Option<&Arc<Shelf>> {
        if s.spoken() { self.shelves.get(&shelf_key(s)) } else { None }
    }

    /// The song whose cover `s` shows: an episode or a book's file with no
    /// cover of its own shows its show's or book's.
    pub fn art_of<'a>(&'a self, s: &'a SongRef) -> &'a SongRef {
        if !s.spoken() || s.has_art {
            return s;
        }
        self.shelf_of(s).and_then(|sh| sh.cover()).unwrap_or(s)
    }

    pub fn songs_of(&self, ids: &[i64]) -> Vec<SongRef> {
        ids.iter().filter_map(|id| self.by_id.get(id).cloned()).collect()
    }

    /// This folder's songs and every subfolder's, in folder order: what "play folder" plays.
    pub fn all_songs(&self, path: &str) -> Vec<SongRef> {
        let mut out = Vec::new();
        fn walk(c: &Catalog, path: &str, out: &mut Vec<SongRef>) {
            if let Some(f) = c.folders.get(path) {
                out.extend(f.songs.iter().cloned());
                for child in &f.children {
                    walk(c, child, out);
                }
            }
        }
        walk(self, path, &mut out);
        out
    }

    // Search indexes (plan 029), each built on first use. The app builds the
    // songs' and albums' with the catalogue, off the main thread, so the
    // first keystroke in a filter finds them ready (plan 036).
    pub fn song_index(&self) -> &Index<SongRef> {
        self.song_index.get_or_init(|| song_index(self.songs.clone()))
    }
    pub fn album_index(&self) -> &Index<Arc<Album>> {
        self.album_index.get_or_init(|| {
            Index::new(
                self.albums.clone(),
                vec![
                    field("album", Some(0), |a: &Arc<Album>| a.name.clone()),
                    field("artist", Some(1), |a: &Arc<Album>| a.artist.clone()),
                ],
            )
        })
    }
    pub fn artist_index(&self) -> &Index<Arc<Group>> {
        self.artist_index.get_or_init(|| search::names(self.artists.clone(), "artist", |g| g.name.clone()))
    }
    pub fn genre_index(&self) -> &Index<Arc<Group>> {
        self.genre_index.get_or_init(|| search::names(self.genres.clone(), "genre", |g| g.name.clone()))
    }
    /// Every folder but the top, by path.
    pub fn folder_index(&self) -> &Index<String> {
        self.folder_index.get_or_init(|| {
            let paths = natural_sorted(self.folders.keys().filter(|p| !p.is_empty()).cloned().collect(), |p| p);
            search::names(paths, "folder", |p| p.rsplit('/').next().unwrap_or(p).to_string())
        })
    }
    pub fn book_index(&self) -> &Index<Arc<Shelf>> {
        self.book_index.get_or_init(|| {
            Index::new(
                self.books.clone(),
                vec![
                    field("book", Some(0), |b: &Arc<Shelf>| b.title.clone()),
                    field("artist", Some(1), |b: &Arc<Shelf>| b.author.clone()),
                ],
            )
        })
    }
    pub fn show_index(&self) -> &Index<Arc<Shelf>> {
        self.show_index.get_or_init(|| search::names(self.shows.clone(), "show", |s| s.title.clone()))
    }
    pub fn episode_index(&self) -> &Index<SongRef> {
        self.episode_index.get_or_init(|| song_index(self.shows.iter().flat_map(|s| s.items.iter().cloned()).collect()))
    }

    /// The songs of `list` that `query` finds, in the list's order: what a page's filter box shows.
    pub fn filter(&self, list: &[SongRef], query: &str) -> Vec<SongRef> {
        let q = Query::new(query);
        if q.is_empty() {
            return list.to_vec();
        }
        // Favorites, Listen Later and queues may hold episodes, which the music's index leaves out.
        let own;
        let index = if list.iter().any(|s| s.spoken()) {
            own = song_index(list.to_vec());
            &own
        } else {
            self.song_index()
        };
        let found: HashSet<i64> = index.filter(&q).into_iter().map(|i| index.items[i].id).collect();
        list.iter().filter(|s| found.contains(&s.id)).cloned().collect()
    }
}

/// A song's fields, most telling first (docs/plans/029_search.md).
pub fn song_index(songs: Vec<SongRef>) -> Index<SongRef> {
    Index::new(
        songs,
        vec![
            field("title", Some(0), |s: &SongRef| s.title.clone()),
            field("artist", Some(1), |s: &SongRef| {
                let mut a = s.artists.join(" ");
                a.push(' ');
                a.push_str(&s.artist);
                a
            }),
            field("album", Some(2), |s: &SongRef| s.album.clone()),
            field("artist", Some(3), |s: &SongRef| s.album_artist.clone()),
            field("composer", Some(3), |s: &SongRef| s.composer.clone()),
            field("genre", Some(4), |s: &SongRef| s.genres.join(" ")),
            field("year", Some(4), |s: &SongRef| if s.year > 0 { s.year.to_string() } else { String::new() }),
            field("folder", None, |s: &SongRef| s.folder().to_string()),
        ],
    )
}

/// A file the server did not group (none should be) is a shelf of its folder.
fn shelf_key(s: &Song) -> String {
    if s.group_key.is_empty() { format!("{}\0{}", s.kind, s.folder()) } else { s.group_key.clone() }
}

fn shelves(spoken: &[SongRef], kind: &str) -> Vec<Arc<Shelf>> {
    let mut groups: HashMap<String, Vec<SongRef>> = HashMap::new();
    for s in spoken.iter().filter(|s| s.kind == kind) {
        groups.entry(shelf_key(s)).or_default().push(s.clone());
    }
    let shelves = groups
        .into_iter()
        .map(|(id, list)| {
            let items = path_order(list, |s| (s.group_index, 0));
            let first = &items[0];
            let title = if first.group_title.is_empty() {
                first.folder().rsplit('/').next().unwrap_or("").to_string()
            } else {
                first.group_title.clone()
            };
            let authors = items
                .iter()
                .map(|s| if s.artist.is_empty() { s.album_artist.as_str() } else { s.artist.as_str() })
                .filter(|a| !a.is_empty());
            let author = most_common(authors);
            Arc::new(Shelf { id, kind: kind.into(), title, author, items })
        })
        .collect();
    natural_sorted(shelves, |s| &s.title)
}

/// Two songs are on one album when they share the album name and either
/// the album artist or, without one, the folder. A bare album name would
/// merge every "Greatest Hits" in the collection.
fn albums(songs: &[SongRef]) -> Vec<Arc<Album>> {
    let mut groups: HashMap<String, Vec<SongRef>> = HashMap::new();
    for s in songs.iter().filter(|s| !s.album.is_empty()) {
        let by = if s.album_artist.is_empty() { s.folder().to_string() } else { s.album_artist.to_lowercase() };
        groups.entry(format!("{}\0{}", s.album.to_lowercase(), by)).or_default().push(s.clone());
    }
    let albums = groups
        .into_iter()
        .map(|(id, list)| {
            let sorted = track_order(list);
            let first = &sorted[0];
            let artist = if first.album_artist.is_empty() {
                most_common(sorted.iter().map(|s| s.display_artist()))
            } else {
                first.album_artist.clone()
            };
            Arc::new(Album {
                id,
                name: first.album.clone(),
                artist,
                year: sorted.iter().map(|s| s.year).max().unwrap_or(0),
                songs: sorted,
            })
        })
        .collect();
    natural_sorted(albums, |a| &a.name)
}

/// Every folder, keyed by its path relative to the Music folder ("" is
/// the top). Episodes and books' files sit under a Podcasts and an
/// Audiobooks root after the music's folders, as on the phone: their
/// paths are relative to folders of their own, so a show's folder could
/// share a music folder's name.
fn folders(songs: &[SongRef], spoken: &[SongRef]) -> HashMap<String, Folder> {
    let mut map: HashMap<String, Folder> = HashMap::new();
    fn ensure(map: &mut HashMap<String, Folder>, path: &str) {
        if map.contains_key(path) {
            return;
        }
        map.insert(
            path.into(),
            Folder { path: path.into(), children: vec![], songs: vec![], song_count: 0, duration_ms: 0 },
        );
        if !path.is_empty() {
            let parent = path.rfind('/').map_or("", |i| &path[..i]);
            ensure(map, parent);
            map.get_mut(parent).unwrap().children.push(path.into());
        }
    }
    for s in songs {
        ensure(&mut map, s.folder());
        map.get_mut(s.folder()).unwrap().songs.push(s.clone());
    }
    for s in spoken {
        let root = if s.kind == PODCAST { PODCASTS } else { AUDIOBOOKS };
        let path = if s.folder().is_empty() { root.to_string() } else { format!("{root}/{}", s.folder()) };
        ensure(&mut map, &path);
        map.get_mut(&path).unwrap().songs.push(s.clone());
    }
    for f in map.values_mut() {
        let mut keyed: Vec<(bool, String, String)> = std::mem::take(&mut f.children)
            .into_iter()
            .map(|c| {
                let name = c.rsplit('/').next().unwrap_or(&c).to_string();
                (c == PODCASTS || c == AUDIOBOOKS, sort_key(&name), c)
            })
            .collect();
        keyed.sort();
        f.children = keyed.into_iter().map(|(_, _, c)| c).collect();
        f.songs = path_order(std::mem::take(&mut f.songs), |s| (s.disc, s.track));
    }
    // Each folder's total, children first, so a row never flattens its subtree to count it.
    fn total(map: &mut HashMap<String, Folder>, path: &str) -> (usize, i64) {
        let children = map[path].children.clone();
        let f = &map[path];
        let (mut n, mut ms) = (f.songs.len(), f.songs.iter().map(|s| s.duration_ms).sum::<i64>());
        for c in children {
            let (cn, cms) = total(map, &c);
            n += cn;
            ms += cms;
        }
        let f = map.get_mut(path).unwrap();
        f.song_count = n;
        f.duration_ms = ms;
        (n, ms)
    }
    ensure(&mut map, "");
    total(&mut map, "");
    map
}

fn group(songs: &[SongRef], keys: impl Fn(&Song) -> Vec<String>) -> Vec<Arc<Group>> {
    let mut map: HashMap<String, (String, Vec<SongRef>)> = HashMap::new();
    for s in songs {
        for k in keys(s) {
            map.entry(k.to_lowercase()).or_insert_with(|| (k, vec![])).1.push(s.clone());
        }
    }
    natural_sorted(map.into_values().map(|(name, songs)| Arc::new(Group { name, songs })).collect(), |g| &g.name)
}

/// An album's order: disc and track, then the title for untagged files.
pub fn track_order(songs: Vec<SongRef>) -> Vec<SongRef> {
    let mut keyed: Vec<_> = songs.into_iter().map(|s| (s.disc, s.track, sort_key(&s.title), s)).collect();
    keyed.sort_by(|a, b| (a.0, a.1, &a.2).cmp(&(b.0, b.1, &b.2)));
    keyed.into_iter().map(|k| k.3).collect()
}

/// An artist's or genre's songs by album, then disc and track: how they
/// were released, as on the phone.
pub fn album_order(songs: &[SongRef]) -> Vec<SongRef> {
    let mut keyed: Vec<_> = songs.iter().map(|s| (s.album.to_lowercase(), s.disc, s.track, s.clone())).collect();
    keyed.sort_by(|a, b| (&a.0, a.1, a.2).cmp(&(&b.0, b.1, b.2)));
    keyed.into_iter().map(|k| k.3).collect()
}

/// `rank` first, then the path in natural order: a folder's or a shelf's files.
fn path_order(songs: Vec<SongRef>, rank: impl Fn(&Song) -> (i64, i64)) -> Vec<SongRef> {
    let mut keyed: Vec<_> = songs.into_iter().map(|s| (rank(&s), sort_key(&s.path), s)).collect();
    keyed.sort_by(|a, b| (a.0, &a.1).cmp(&(b.0, &b.1)));
    keyed.into_iter().map(|k| k.2).collect()
}

/// `items` in natural, case-blind order of `text`: "Track 2" before "Track 10".
/// The key is made once per item, not once per comparison: a sort of n
/// items compares about n log n times (plan 036).
pub fn natural_sorted<T>(items: Vec<T>, text: impl Fn(&T) -> &str) -> Vec<T> {
    let mut keyed: Vec<(String, T)> = items.into_iter().map(|x| (sort_key(text(&x)), x)).collect();
    keyed.sort_by(|a, b| a.0.cmp(&b.0));
    keyed.into_iter().map(|k| k.1).collect()
}

/// Longer runs of digits sort by their text, which is wrong past this many; no title has them.
const DIGIT_WIDTH: usize = 20;

/// A string's natural-order key: lower case, each run of digits without
/// its leading zeros and padded to a fixed width, so plain `<` on two keys
/// puts 2 before 10 and ignores case.
pub fn sort_key(s: &str) -> String {
    let mut out = String::with_capacity(s.len() + 8);
    let mut digits = String::new();
    fn flush(out: &mut String, digits: &mut String) {
        if digits.is_empty() {
            return;
        }
        let run = digits.trim_start_matches('0');
        let n = run.chars().count();
        out.extend(std::iter::repeat_n('0', DIGIT_WIDTH.saturating_sub(n)));
        out.push_str(run);
        digits.clear();
    }
    for c in s.chars().flat_map(char::to_lowercase) {
        if c.is_numeric() {
            digits.push(c);
        } else {
            flush(&mut out, &mut digits);
            out.push(c);
        }
    }
    flush(&mut out, &mut digits);
    out
}

/// The name that comes most often; the first of them on a tie.
fn most_common<'a>(names: impl Iterator<Item = &'a str>) -> String {
    let mut counts: Vec<(&str, usize)> = Vec::new();
    let mut index: HashMap<&str, usize> = HashMap::new();
    for n in names {
        match index.get(n) {
            Some(&i) => counts[i].1 += 1,
            None => {
                index.insert(n, counts.len());
                counts.push((n, 1));
            }
        }
    }
    let best = counts.iter().map(|c| c.1).max().unwrap_or(0);
    counts.iter().find(|c| c.1 == best).map_or(String::new(), |c| c.0.to_string())
}

#[cfg(test)]
mod tests {
    // Ported from the phone's CatalogTest and the Mac's LibraryTests, so the apps keep the same rules.
    use super::*;

    fn song(id: i64, path: &str, title: &str, album: &str, artist: &str, album_artist: &str, track: i64) -> Song {
        Song {
            id,
            path: path.into(),
            title: if title.is_empty() { format!("Song {id}") } else { title.into() },
            artist: artist.into(),
            artists: if artist.is_empty() { vec![] } else { vec![artist.into()] },
            album: album.into(),
            album_artist: album_artist.into(),
            track,
            duration_ms: 180_000,
            format: "mpeg".into(),
            ..Default::default()
        }
    }

    #[test]
    fn albums_with_the_same_name_by_different_artists_stay_separate() {
        let c = Catalog::of(vec![
            song(1, "A/Greatest Hits/01.mp3", "One", "Greatest Hits", "Queen", "", 0),
            song(2, "B/Greatest Hits/01.mp3", "Two", "Greatest Hits", "ABBA", "", 0),
            song(3, "A/Greatest Hits/02.mp3", "Three", "Greatest Hits", "Queen", "", 0),
        ]);
        let mut counts: Vec<usize> = c.albums.iter().map(|a| a.songs.len()).collect();
        counts.sort();
        assert_eq!(counts, [1, 2]);
    }

    #[test]
    fn a_compilation_tagged_with_an_album_artist_is_one_album_across_folders() {
        let c = Catalog::of(vec![
            song(1, "x/01.mp3", "One", "Rockstar", "Mohit", "A.R. Rahman", 1),
            song(2, "y/02.mp3", "Two", "Rockstar", "Javed", "A.R. Rahman", 2),
        ]);
        assert_eq!(c.albums.len(), 1);
        assert_eq!(c.albums[0].songs.iter().map(|s| s.id).collect::<Vec<_>>(), [1, 2]);
    }

    #[test]
    fn numbers_sort_as_numbers_and_case_does_not_count() {
        let sorted = natural_sorted(vec!["Track 10", "track 2", "Track 1", "b", "A", "09", "9a"], |s| s);
        assert_eq!(sorted, ["09", "9a", "A", "b", "Track 1", "track 2", "Track 10"]);
    }

    #[test]
    fn folders_hold_their_subfolders_songs() {
        let c = Catalog::of(vec![
            song(1, "Hindi/Arijit/01.mp3", "", "", "", "", 0),
            song(2, "Hindi/02.mp3", "", "", "", "", 0),
        ]);
        let mut ids: Vec<i64> = c.all_songs("Hindi").iter().map(|s| s.id).collect();
        ids.sort();
        assert_eq!(ids, [1, 2]);
        assert_eq!(c.folders[""].children, ["Hindi"]);
        assert_eq!(c.folders["Hindi"].song_count, 2);
        assert_eq!(c.folders[""].duration_ms, 360_000);
    }

    // Episodes keep out of the music, under roots of their own, after the music's folders.
    #[test]
    fn podcasts_and_books_have_their_own_shelves_and_roots() {
        let mut ep = song(3, "Show/ep1.mp3", "Ep 1", "", "Host", "", 0);
        ep.kind = PODCAST.into();
        ep.group_key = "show:1".into();
        ep.group_title = "The Show".into();
        let c = Catalog::of(vec![song(1, "Zed/01.mp3", "", "", "", "", 0), ep]);
        assert_eq!(c.songs.len(), 1);
        assert_eq!(c.by_id.len(), 2);
        assert_eq!(c.shows.len(), 1);
        assert_eq!(c.shows[0].title, "The Show");
        assert_eq!(c.shows[0].author, "Host");
        assert_eq!(c.folders[""].children, ["Zed", "Podcasts"]);
        assert!(c.artists.iter().all(|a| a.name != "Host"));
    }
}
