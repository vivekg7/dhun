//! What the window's search field finds (plan 029), as the Mac's
//! `SearchView`: artists, albums, genres, folders, playlists, books, shows,
//! episodes and songs, the section with the best match first; then the
//! songs whose lyrics hold the words, which the server searches.

use crate::pages::{Nav, Route};
use crate::widgets::{self, count, cover, row};
use adw::prelude::*;
use dhun_core::catalog::SongRef;
use dhun_core::downloads as dl;
use dhun_core::search::{self, Query};
use gtk::{gio, glib};
use std::cell::{Cell, RefCell};
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

#[derive(Clone)]
enum Action {
    Open(Route),
    Play(String, Rc<Vec<SongRef>>, usize),
    /// A recent search, into the field again.
    Search(String),
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
    // A favourite, or a song played often, comes first among equal matches.
    let favs: HashSet<i64> = nav.app.marks.borrow().favs.iter().copied().collect();
    let plays = nav.app.db.lock().unwrap().play_stats().unwrap_or_default();
    let ix = c.song_index();
    let boost =
        |s: &SongRef| (if favs.contains(&s.id) { 3 } else { 0 }) + plays.get(&s.id).map_or(0, |p| (p.count / 5).min(2));
    section("Songs", ix.search(&q, boost), &|i| {
        let s = &ix.items[i];
        let detail =
            [s.display_artist(), &s.album].into_iter().filter(|x| !x.is_empty()).collect::<Vec<_>>().join(" · ");
        Hit { title: s.title.clone(), detail, song: Some(s.clone()), icon: None, route: None }
    });
    // Stable, so a tie keeps the order above.
    found.sort_by_key(|f| f.best);
    found
}

/// What a result's download is, by the page it opens.
fn pin_of(r: &Route) -> Option<(&'static str, String)> {
    match r {
        Route::Artist(n) => Some((dl::ARTIST, n.clone())),
        Route::Album(id) => Some((dl::ALBUM, id.clone())),
        Route::Genre(n) => Some((dl::GENRE, n.clone())),
        Route::Folder(p) => Some((dl::FOLDER, p.clone())),
        Route::Playlist(id) => Some((dl::PLAYLIST, id.to_string())),
        Route::Shelf(id) => Some((dl::SHELF, id.clone())),
        _ => None,
    }
}

/// A section's heading, a row of its own.
fn heading(text: &str) -> gtk::ListBoxRow {
    let l =
        gtk::Label::builder().label(text).xalign(0.0).css_classes(["heading"]).margin_top(12).margin_start(6).build();
    gtk::ListBoxRow::builder().child(&l).activatable(false).selectable(false).build()
}

/// A right-click on a song found opens the song menu for it, as in any song list.
fn menu_on(nav: &Nav, r: &gtk::ListBoxRow, song: &SongRef, picked: &Rc<RefCell<Vec<SongRef>>>) {
    let g = gtk::GestureClick::builder().button(3).build();
    let (app, song, picked, row) = (nav.app.clone(), song.clone(), picked.clone(), r.downgrade());
    g.connect_pressed(move |_, _, x, y| {
        let Some(row) = row.upgrade() else { return };
        *picked.borrow_mut() = vec![song.clone()];
        let menu = gio::Menu::new();
        let play = widgets::Play::List { name: String::new(), source: "search".into() };
        widgets::song_menu(&app, &menu, std::slice::from_ref(&song), &play, &widgets::Edit::None);
        widgets::popup(&menu, &row, x, y);
    });
    r.add_controller(g);
}

/// The last searches, each a click away; or, with none yet, what the field searches (plan 029).
fn recent(nav: &Rc<Nav>, list: &gtk::ListBox, routes: &mut Vec<Option<Action>>) {
    let searches = nav.app.prefs.get().recent_searches;
    if searches.is_empty() {
        let songs = nav.app.catalog().songs.len();
        if songs > 0 {
            let help = adw::StatusPage::builder()
                .title("Search")
                .icon_name("edit-find-symbolic")
                .description(format!(
                    "Search {songs} songs by title, artist, album, composer, genre or lyrics.\n\
                    Narrow a word with artist:, album:, title:, genre:, year: or lyrics:"
                ))
                .build();
            list.append(&help);
            routes.push(None);
        }
        return;
    }
    let head = gtk::Box::builder().margin_top(12).margin_start(6).build();
    head.append(
        &gtk::Label::builder().label("Recent searches").xalign(0.0).hexpand(true).css_classes(["heading"]).build(),
    );
    let clear = gtk::Button::builder().label("Clear").css_classes(["flat"]).build();
    let n = Rc::downgrade(nav);
    clear.connect_clicked(move |_| {
        if let Some(n) = n.upgrade() {
            n.app.prefs.forget_search(None);
            n.refill_search();
        }
    });
    head.append(&clear);
    list.append(&gtk::ListBoxRow::builder().child(&head).activatable(false).selectable(false).build());
    routes.push(None);
    for q in searches {
        let b = gtk::Box::builder().spacing(12).margin_start(6).build();
        b.append(&gtk::Image::from_icon_name("document-open-recent-symbolic"));
        b.append(
            &gtk::Label::builder()
                .label(&q)
                .xalign(0.0)
                .hexpand(true)
                .ellipsize(gtk::pango::EllipsizeMode::End)
                .build(),
        );
        let forget = gtk::Button::builder()
            .icon_name("window-close-symbolic")
            .tooltip_text("Forget")
            .css_classes(["flat", "circular"])
            .build();
        let (n, q2) = (Rc::downgrade(nav), q.clone());
        forget.connect_clicked(move |_| {
            if let Some(n) = n.upgrade() {
                n.app.prefs.forget_search(Some(&q2));
                n.refill_search();
            }
        });
        b.append(&forget);
        list.append(&gtk::ListBoxRow::builder().child(&b).build());
        routes.push(Some(Action::Search(q)));
    }
}

/// The results page. The window's search field fills it; Nav keeps it current.
pub fn page(nav: &Rc<Nav>) -> gtk::Widget {
    let picked: Rc<RefCell<Vec<SongRef>>> = Rc::default();
    let list =
        gtk::ListBox::builder().selection_mode(gtk::SelectionMode::None).css_classes(["navigation-sidebar"]).build();
    let expanded: Rc<RefCell<HashSet<&'static str>>> = Rc::default();
    // What each row does, by its index in the list.
    let routes: Rc<RefCell<Vec<Option<Action>>>> = Rc::default();
    let fill: Rc<dyn Fn()> = {
        let (nav, list, expanded, routes, picked) =
            (Rc::downgrade(nav), list.downgrade(), expanded.clone(), routes.clone(), picked.clone());
        Rc::new(move || {
            let (Some(nav), Some(list)) = (nav.upgrade(), list.upgrade()) else { return };
            list.remove_all();
            let mut routes = routes.borrow_mut();
            routes.clear();
            let text = nav.search.text();
            if text.trim().is_empty() {
                return recent(&nav, &list, &mut routes);
            }
            let found = find(&nav, &text);
            // Words long enough to search lyrics wait for that answer before saying so.
            if found.is_empty() && !text.is_empty() && Query::new(&text).lyrics.chars().count() < 3 {
                let empty = adw::StatusPage::builder().title("No results").icon_name("edit-find-symbolic").build();
                list.append(&empty);
                routes.push(None);
            }
            for f in found {
                list.append(&heading(f.kind));
                routes.push(None);
                let all = expanded.borrow().contains(f.kind);
                let total = f.hits.len();
                // A song found plays the songs found, from it, as a queue named after the search.
                let found: Rc<Vec<SongRef>> =
                    Rc::new(f.hits.iter().filter(|h| h.route.is_none()).filter_map(|h| h.song.clone()).collect());
                for (i, h) in f.hits.into_iter().take(if all { usize::MAX } else { SHOWN }).enumerate() {
                    let image = match (&h.song, h.icon) {
                        (_, Some(icon)) => {
                            gtk::Image::builder().icon_name(icon).pixel_size(24).width_request(36).build().upcast()
                        }
                        (song, None) => cover(&nav.app, song.as_ref(), 36),
                    };
                    let b = row(&image, &h.title, &h.detail);
                    // An episode opens its show, whose download is not the episode's.
                    if let Some((kind, r#ref)) = h.route.as_ref().filter(|_| f.kind != "Episodes").and_then(pin_of) {
                        b.append(&widgets::pin_mark(&nav.app, kind, &r#ref));
                    }
                    let r = gtk::ListBoxRow::builder().child(&b).build();
                    if let Some(s) = &h.song {
                        menu_on(&nav, &r, s, &picked);
                    }
                    list.append(&r);
                    routes.push(Some(match h.route {
                        Some(r) => Action::Open(r),
                        None => Action::Play(text.to_string(), found.clone(), i),
                    }));
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
        let action = routes.borrow().get(r.index() as usize).cloned().flatten();
        let Some(n) = n.upgrade() else { return };
        // Kept once something it found is picked, so half-typed words are not.
        if matches!(action, Some(Action::Open(_) | Action::Play(..))) {
            n.app.prefs.keep_search(&n.search.text());
        }
        match action {
            Some(Action::Open(route)) => n.push(route),
            Some(Action::Play(name, songs, i)) => n.app.playback().play_list(&name, "search", &songs, i),
            Some(Action::Search(q)) => n.search.set_text(&q),
            None => {}
        }
    });
    // The songs whose lyrics hold the words, after the rest: the server
    // answers a moment after typing stops.
    let lyrics_list =
        gtk::ListBox::builder().selection_mode(gtk::SelectionMode::None).css_classes(["navigation-sidebar"]).build();
    let found_in: Rc<RefCell<(String, Rc<Vec<SongRef>>)>> = Rc::default();
    let pending: Rc<Cell<Option<glib::SourceId>>> = Rc::default();
    let fill_lyrics: Rc<dyn Fn()> = {
        let (nav, l, found_in, main, picked) =
            (Rc::downgrade(nav), lyrics_list.downgrade(), found_in.clone(), list.downgrade(), picked.clone());
        Rc::new(move || {
            let (Some(nav), Some(l)) = (nav.upgrade(), l.upgrade()) else { return };
            l.remove_all();
            if let Some(id) = pending.take() {
                id.remove();
            }
            let text = nav.search.text().to_string();
            let q = Query::new(&text);
            if q.lyrics.chars().count() < 3 {
                return;
            }
            l.append(&heading("In lyrics"));
            l.append(&heading("Searching lyrics…"));
            let (nav2, l2, found_in, pending2, main, picked) =
                (nav, l.downgrade(), found_in.clone(), pending.clone(), main.clone(), picked.clone());
            pending.set(Some(glib::timeout_add_local_once(std::time::Duration::from_millis(400), move || {
                pending2.take();
                glib::spawn_future_local(async move {
                    let hits = crate::lyrics::search(&nav2.app, q).await;
                    let Some(l) = l2.upgrade().filter(|_| nav2.search.text() == text) else { return };
                    l.remove_all();
                    let c = nav2.app.catalog();
                    let hits: Vec<(SongRef, String)> =
                        hits.into_iter().filter_map(|h| c.by_id.get(&h.song).map(|s| (s.clone(), h.line))).collect();
                    if hits.is_empty() {
                        if main.upgrade().is_some_and(|m| m.first_child().is_none()) {
                            l.append(
                                &adw::StatusPage::builder().title("No results").icon_name("edit-find-symbolic").build(),
                            );
                        }
                        return;
                    }
                    l.append(&heading("In lyrics"));
                    for (s, line) in &hits {
                        let detail = format!("“{line}” · {}", s.display_artist());
                        let r = gtk::ListBoxRow::builder()
                            .child(&row(&cover(&nav2.app, Some(s), 36), &s.title, &detail))
                            .build();
                        menu_on(&nav2, &r, s, &picked);
                        l.append(&r);
                    }
                    *found_in.borrow_mut() =
                        (format!("Lyrics: {text}"), Rc::new(hits.into_iter().map(|h| h.0).collect()));
                });
            })));
        })
    };
    let n = Rc::downgrade(nav);
    lyrics_list.connect_row_activated(move |_, r| {
        let Some(n) = n.upgrade() else { return };
        let (name, songs) = found_in.borrow().clone();
        // The heading is the first row.
        if let Some(i) = (r.index() as usize).checked_sub(1).filter(|&i| i < songs.len()) {
            n.app.prefs.keep_search(&n.search.text());
            n.app.playback().play_list(&name, "search", &songs, i);
        }
    });
    let (f, fl) = (fill.clone(), fill_lyrics.clone());
    nav.set_search_fill(Rc::new(move || {
        f();
        fl();
    }));
    fill();
    fill_lyrics();
    let both = gtk::Box::builder().orientation(gtk::Orientation::Vertical).build();
    both.append(&list);
    both.append(&lyrics_list);
    let group = widgets::song_actions(
        &nav.app,
        both.upcast_ref(),
        Rc::new(move || picked.borrow().clone()),
        Rc::new(Vec::new),
        &widgets::Play::List { name: String::new(), source: "search".into() },
        &widgets::Edit::None,
    );
    both.insert_action_group("songs", Some(&group));
    widgets::scrolled(&both).upcast()
}
