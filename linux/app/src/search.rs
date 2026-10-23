//! What the window's search field finds (plan 029), as the Mac's
//! `SearchView`: artists, albums, genres, folders, playlists, books, shows,
//! episodes and songs, the section with the best match first. Lyrics,
//! which the server searches, join with the lyrics in step 4 of plan 039.

use crate::pages::{Nav, Route};
use crate::widgets::{self, count, cover, row};
use adw::prelude::*;
use dhun_core::catalog::SongRef;
use dhun_core::search::{self, Query};
use std::cell::RefCell;
use std::collections::HashSet;
use std::rc::Rc;

/// Results a section shows before "Show all": one strong song should not sit under twenty albums.
const SHOWN: usize = 3;
/// Songs found are many in a big library; past this many, the query is too loose to read through.
const SONGS_SHOWN: usize = 300;

struct Hit {
    title: String,
    detail: String,
    song: Option<SongRef>,
    icon: Option<&'static str>,
    route: Option<Route>,
}

struct Found {
    kind: &'static str,
    hits: Vec<Hit>,
    best: i32,
}

fn find(nav: &Nav, text: &str) -> Vec<Found> {
    let q = Query::new(text);
    if q.is_empty() {
        return vec![];
    }
    let c = nav.app.catalog();
    let mut found = Vec::new();
    let mut section = |kind: &'static str, scored: Vec<(usize, i32)>, hit: &dyn Fn(usize) -> Hit| {
        if let Some(&(_, best)) = scored.first() {
            let n = if kind == "Songs" { SONGS_SHOWN } else { scored.len() };
            found.push(Found { kind, hits: scored.iter().take(n).map(|&(i, _)| hit(i)).collect(), best });
        }
    };
    let ix = c.artist_index();
    section("Artists", ix.search(&q, |_| 0), &|i| {
        let g = &ix.items[i];
        Hit {
            title: g.name.clone(),
            detail: count(g.songs.len()),
            song: g.songs.first().cloned(),
            icon: None,
            route: Some(Route::Artist(g.name.clone())),
        }
    });
    let ix = c.album_index();
    section("Albums", ix.search(&q, |_| 0), &|i| {
        let a = &ix.items[i];
        let detail = if a.year > 0 { format!("{} · {}", a.artist, a.year) } else { a.artist.clone() };
        Hit {
            title: a.name.clone(),
            detail,
            song: a.songs.first().cloned(),
            icon: None,
            route: Some(Route::Album(a.id.clone())),
        }
    });
    let ix = c.genre_index();
    section("Genres", ix.search(&q, |_| 0), &|i| {
        let g = &ix.items[i];
        Hit {
            title: g.name.clone(),
            detail: count(g.songs.len()),
            song: g.songs.first().cloned(),
            icon: None,
            route: Some(Route::Genre(g.name.clone())),
        }
    });
    // Many folders share a name ("CD1"), so the path says which.
    let ix = c.folder_index();
    section("Folders", ix.search(&q, |_| 0), &|i| {
        let p = &ix.items[i];
        let (parent, name) = p.rsplit_once('/').unwrap_or(("Music", p));
        Hit {
            title: name.into(),
            detail: parent.replace('/', " › "),
            song: None,
            icon: Some("folder-symbolic"),
            route: Some(Route::Folder(p.clone())),
        }
    });
    let playlists = search::names(nav.app.playlists.borrow().clone(), "playlist", |p| p.name.clone());
    section("Playlists", playlists.search(&q, |_| 0), &|i| {
        let p = &playlists.items[i];
        Hit {
            title: p.name.clone(),
            detail: count(c.songs_of(&p.songs).len()),
            song: None,
            icon: Some("view-list-symbolic"),
            route: Some(Route::Playlist(p.id)),
        }
    });
    let ix = c.book_index();
    section("Audiobooks", ix.search(&q, |_| 0), &|i| {
        let b = &ix.items[i];
        Hit {
            title: b.title.clone(),
            detail: b.author.clone(),
            song: b.cover().or(b.items.first()).cloned(),
            icon: None,
            route: Some(Route::Shelf(b.id.clone())),
        }
    });
    let ix = c.show_index();
    section("Podcasts", ix.search(&q, |_| 0), &|i| {
        let s = &ix.items[i];
        Hit {
            title: s.title.clone(),
            detail: "Podcast".into(),
            song: s.cover().or(s.items.first()).cloned(),
            icon: None,
            route: Some(Route::Shelf(s.id.clone())),
        }
    });
    let ix = c.episode_index();
    section("Episodes", ix.search(&q, |_| 0), &|i| {
        let e = &ix.items[i];
        let detail =
            [e.group_title.clone(), e.day()].into_iter().filter(|s| !s.is_empty()).collect::<Vec<_>>().join(" · ");
        Hit {
            title: e.title.clone(),
            detail,
            song: Some(e.clone()),
            icon: None,
            route: c.shelf_of(e).map(|s| Route::Shelf(s.id.clone())),
        }
    });
    let ix = c.song_index();
    section("Songs", ix.search(&q, |_| 0), &|i| {
        let s = &ix.items[i];
        let detail =
            [s.display_artist(), &s.album].into_iter().filter(|x| !x.is_empty()).collect::<Vec<_>>().join(" · ");
        Hit { title: s.title.clone(), detail, song: Some(s.clone()), icon: None, route: None }
    });
    // Stable, so a tie keeps the order above.
    found.sort_by_key(|f| f.best);
    found
}

/// The results page. The window's search field fills it; Nav keeps it current.
pub fn page(nav: &Rc<Nav>) -> gtk::Widget {
    let list =
        gtk::ListBox::builder().selection_mode(gtk::SelectionMode::None).css_classes(["navigation-sidebar"]).build();
    let expanded: Rc<RefCell<HashSet<&'static str>>> = Rc::default();
    // What each row opens, by its index in the list.
    let routes: Rc<RefCell<Vec<Option<Route>>>> = Rc::default();
    let fill: Rc<dyn Fn()> = {
        let (nav, list, expanded, routes) = (Rc::downgrade(nav), list.downgrade(), expanded.clone(), routes.clone());
        Rc::new(move || {
            let (Some(nav), Some(list)) = (nav.upgrade(), list.upgrade()) else { return };
            list.remove_all();
            let mut routes = routes.borrow_mut();
            routes.clear();
            let text = nav.search.text();
            let found = find(&nav, &text);
            if found.is_empty() && !text.is_empty() {
                let empty = adw::StatusPage::builder().title("No results").icon_name("edit-find-symbolic").build();
                list.append(&empty);
                routes.push(None);
            }
            for f in found {
                let heading = gtk::Label::builder()
                    .label(f.kind)
                    .xalign(0.0)
                    .css_classes(["heading"])
                    .margin_top(12)
                    .margin_start(6)
                    .build();
                let r = gtk::ListBoxRow::builder().child(&heading).activatable(false).selectable(false).build();
                list.append(&r);
                routes.push(None);
                let all = expanded.borrow().contains(f.kind);
                let total = f.hits.len();
                for h in f.hits.into_iter().take(if all { usize::MAX } else { SHOWN }) {
                    let image = match (&h.song, h.icon) {
                        (_, Some(icon)) => {
                            gtk::Image::builder().icon_name(icon).pixel_size(24).width_request(36).build().upcast()
                        }
                        (song, None) => cover(&nav.app, song.as_ref(), 36),
                    };
                    list.append(&gtk::ListBoxRow::builder().child(&row(&image, &h.title, &h.detail)).build());
                    routes.push(h.route);
                }
                if total > SHOWN && !all {
                    let more = gtk::Button::builder()
                        .label(format!("Show all {total}"))
                        .css_classes(["flat"])
                        .halign(gtk::Align::Start)
                        .build();
                    let (kind, expanded, nav) = (f.kind, expanded.clone(), nav.clone());
                    more.connect_clicked(move |_| {
                        expanded.borrow_mut().insert(kind);
                        nav.refill_search();
                    });
                    list.append(&gtk::ListBoxRow::builder().child(&more).activatable(false).build());
                    routes.push(None);
                }
            }
        })
    };
    let n = Rc::downgrade(nav);
    list.connect_row_activated(move |_, r| {
        let route = routes.borrow().get(r.index() as usize).cloned().flatten();
        if let (Some(n), Some(route)) = (n.upgrade(), route) {
            n.push(route);
        }
    });
    nav.set_search_fill(fill.clone());
    fill();
    widgets::scrolled(&list).upcast()
}
