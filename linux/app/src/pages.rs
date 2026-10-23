//! The library's pages, the Mac's `Library.swift` in GTK: what each
//! sidebar section shows and the pages opened from it.

use crate::app::{App, Change};
use crate::widgets::{self, Columns, Play, cover, header, row, songs_page, summary, summary_of};
use adw::prelude::*;
use dhun_core::catalog::{self, Album, Group, Shelf, SongRef};
use dhun_core::search::Query;
use gtk::{gio, glib};
use std::cell::RefCell;
use std::rc::Rc;
use std::sync::Arc;

/// A page: the sidebar's sections and what opens from them, as the Mac's
/// `Section` and `Route`.
#[derive(Clone, PartialEq, Debug)]
pub enum Route {
    Folder(String),
    Albums,
    Album(String),
    Artists,
    Artist(String),
    Genres,
    Genre(String),
    Books,
    Shows,
    Shelf(String),
    Playlist(i64),
    Search,
    Queues,
    Queue(String),
}

/// The content pane's page stack. It keeps the routes, so a new catalogue
/// rebuilds the same pages from it rather than sending the user back to
/// the top.
pub struct Nav {
    pub view: adw::NavigationView,
    routes: RefCell<Vec<Route>>,
    pub app: Rc<App>,
    pub search: gtk::SearchEntry,
    /// Fills the search page showing, if one is.
    search_fill: RefCell<Option<Rc<dyn Fn()>>>,
}

impl Nav {
    pub fn new(app: &Rc<App>) -> Rc<Nav> {
        let nav = Rc::new(Nav {
            view: adw::NavigationView::new(),
            routes: RefCell::new(vec![]),
            app: app.clone(),
            search: gtk::SearchEntry::builder().placeholder_text("Search").hexpand(true).build(),
            search_fill: RefCell::new(None),
        });
        let weak = Rc::downgrade(&nav);
        nav.search.connect_search_changed(move |e| {
            let Some(n) = weak.upgrade() else { return };
            if e.text().is_empty() {
                n.refill_search();
            } else if n.current() != Some(Route::Search) {
                n.open(Route::Search);
            } else {
                // Back to the results from a page opened from them.
                let depth = n.routes.borrow().len();
                if depth > 1 {
                    n.open(Route::Search);
                } else {
                    n.refill_search();
                }
            }
        });
        let weak = Rc::downgrade(&nav);
        nav.view.connect_popped(move |_, _| {
            if let Some(n) = weak.upgrade() {
                n.routes.borrow_mut().pop();
            }
        });
        let weak = Rc::downgrade(&nav);
        app.listen(move |c| {
            let Some(n) = weak.upgrade() else { return false };
            let routes = n.routes.borrow().clone();
            let shows = |f: fn(&Route) -> bool| routes.iter().any(f);
            if c == Change::Catalog
                || (c == Change::Playlists && shows(|r| matches!(r, Route::Playlist(_))))
                || (c == Change::Queues && shows(|r| matches!(r, Route::Queues | Route::Queue(_))))
            {
                n.rebuild();
            }
            true
        });
        nav
    }

    /// A sidebar section: its page alone. The sidebar follows, however it was opened.
    pub fn open(self: &Rc<Self>, r: Route) {
        *self.routes.borrow_mut() = vec![r.clone()];
        self.view.replace(&[self.build(&r)]);
        self.app.changed(Change::Section);
    }

    pub fn push(self: &Rc<Self>, r: Route) {
        self.routes.borrow_mut().push(r.clone());
        self.view.push(&self.build(&r));
    }

    pub fn set_search_fill(&self, f: Rc<dyn Fn()>) {
        *self.search_fill.borrow_mut() = Some(f);
    }

    pub fn refill_search(&self) {
        let f = self.search_fill.borrow().clone();
        if let Some(f) = f {
            f();
        }
    }

    pub fn current(&self) -> Option<Route> {
        self.routes.borrow().first().cloned()
    }

    fn rebuild(self: &Rc<Self>) {
        let routes = self.routes.borrow().clone();
        let pages: Vec<_> = routes.iter().map(|r| self.build(r)).collect();
        self.view.replace(&pages);
    }

    fn build(self: &Rc<Self>, r: &Route) -> adw::NavigationPage {
        let app = &self.app;
        let c = app.catalog();
        let (title, child): (String, gtk::Widget) = match r {
            Route::Albums => ("Albums".into(), albums_grid(self, c.albums.clone())),
            Route::Album(id) => match c.albums_by_id.get(id) {
                Some(a) => (a.name.clone(), album_page(app, a)),
                None => ("Album".into(), empty(app)),
            },
            Route::Artists => ("Artists".into(), groups_page(self, &c.artists, true)),
            Route::Genres => ("Genres".into(), groups_page(self, &c.genres, false)),
            Route::Artist(name) | Route::Genre(name) => {
                let artist = matches!(r, Route::Artist(_));
                let list = if artist { &c.artists } else { &c.genres };
                let lower = name.to_lowercase();
                match list.iter().find(|g| g.id() == lower) {
                    Some(g) => (g.name.clone(), group_page(self, g, artist)),
                    None => (name.clone(), empty(app)),
                }
            }
            Route::Folder(path) => match c.folders.get(path) {
                Some(f) => {
                    (if path.is_empty() { "Folders".into() } else { f.name().to_string() }, folder_page(self, path))
                }
                None => ("Folders".into(), empty(app)),
            },
            Route::Books => ("Audiobooks".into(), shelves_page(self, &c.books)),
            Route::Shows => ("Podcasts".into(), shelves_page(self, &c.shows)),
            Route::Shelf(id) => match c.shelves.get(id) {
                Some(s) => (s.title.clone(), shelf_page(app, s)),
                None => ("Shelf".into(), empty(app)),
            },
            Route::Playlist(id) => match app.playlists.borrow().iter().find(|p| p.id == *id) {
                Some(p) => {
                    let songs = c.songs_of(&p.songs);
                    let top = header(app, songs.first(), &p.name, &summary(&songs));
                    let play = Play::List { name: p.name.clone(), source: format!("playlist:{}", p.id) };
                    let cols = Columns { track: false, album: true };
                    (p.name.clone(), songs_page(app, Some(top.upcast()), songs, cols, play).upcast())
                }
                None => ("Playlist".into(), empty(app)),
            },
            Route::Search => ("Search".into(), crate::search::page(self)),
            Route::Queues => ("Queues".into(), queues_page(self)),
            Route::Queue(id) => match app.queues.borrow().iter().find(|q| &q.id == id).cloned() {
                Some(q) => (q.name.clone(), queue_page(self, &q)),
                None => ("Queue".into(), empty(app)),
            },
        };
        let toolbar = adw::ToolbarView::new();
        toolbar.add_top_bar(&adw::HeaderBar::new());
        toolbar.set_content(Some(&child));
        adw::NavigationPage::builder().title(title).child(&toolbar).build()
    }
}

/// Shown while the library is still arriving, or empty.
fn empty(app: &Rc<App>) -> gtk::Widget {
    if app.catalog().by_id.is_empty() && app.syncing.get() {
        let b = adw::StatusPage::builder().title("Fetching the library…").build();
        b.set_child(Some(&gtk::Spinner::builder().spinning(true).width_request(32).height_request(32).build()));
        b.upcast()
    } else {
        adw::StatusPage::builder().title("Nothing here").icon_name("folder-music-symbolic").build().upcast()
    }
}

fn filter_entry(placeholder: &str) -> gtk::SearchEntry {
    gtk::SearchEntry::builder()
        .placeholder_text(placeholder)
        .margin_start(18)
        .margin_end(18)
        .margin_top(6)
        .margin_bottom(6)
        .build()
}

/// Every album as a grid of covers, filtered as search matches (plan 029).
fn albums_grid(nav: &Rc<Nav>, albums: Vec<Arc<Album>>) -> gtk::Widget {
    let app = &nav.app;
    if albums.is_empty() {
        return empty(app);
    }
    let mut store = gio::ListStore::new::<glib::BoxedAnyObject>();
    store.extend(albums.iter().map(|a| glib::BoxedAnyObject::new(a.clone())));
    let factory = gtk::SignalListItemFactory::new();
    const SIZE: i32 = 150;
    factory.connect_setup(|_, item| {
        let b = gtk::Box::builder()
            .orientation(gtk::Orientation::Vertical)
            .spacing(3)
            .width_request(SIZE)
            .margin_bottom(12)
            .build();
        let pic = gtk::Picture::builder()
            .content_fit(gtk::ContentFit::Cover)
            .width_request(SIZE)
            .height_request(SIZE)
            .build();
        b.append(&gtk::Frame::builder().child(&pic).css_classes(["cover"]).margin_bottom(4).build());
        for class in ["heading", "caption", "caption"] {
            let l =
                gtk::Label::builder().xalign(0.0).ellipsize(gtk::pango::EllipsizeMode::End).max_width_chars(1).build();
            l.add_css_class(class);
            if class == "caption" {
                l.add_css_class("dim-label");
            }
            b.append(&l);
        }
        item.downcast_ref::<gtk::ListItem>().unwrap().set_child(Some(&b));
    });
    let a = app.clone();
    factory.connect_bind(move |_, item| {
        let item = item.downcast_ref::<gtk::ListItem>().unwrap();
        let obj = item.item().and_downcast::<glib::BoxedAnyObject>().unwrap();
        let album = obj.borrow::<Arc<Album>>().clone();
        let b = item.child().and_downcast::<gtk::Box>().unwrap();
        let pic = b.first_child().and_downcast::<gtk::Frame>().unwrap().child().and_downcast::<gtk::Picture>().unwrap();
        // A recycled cell may show another album when the cover arrives.
        let weak = item.downgrade();
        let still = Rc::new(move || {
            weak.upgrade().and_then(|i| i.item()).is_some_and(|o| o == obj.clone().upcast::<glib::Object>())
        });
        widgets::set_cover(&a, &pic, album.songs.first(), SIZE, Some(still));
        let mut l = pic.parent().unwrap().next_sibling().and_downcast::<gtk::Label>();
        let year = if album.year > 0 { format!("{} · ", album.year) } else { String::new() };
        for text in [album.name.clone(), album.artist.clone(), format!("{year}{}", summary(&album.songs))] {
            let label = l.unwrap();
            label.set_label(&text);
            l = label.next_sibling().and_downcast::<gtk::Label>();
        }
    });
    let grid = gtk::GridView::new(Some(gtk::NoSelection::new(Some(store.clone()))), Some(factory));
    grid.set_max_columns(12);
    grid.set_single_click_activate(true);
    grid.add_css_class("album-grid");
    widgets::rebind_on_thumbs(app, &store);
    let n = nav.clone();
    grid.connect_activate(move |g, pos| {
        let obj = g.model().and_then(|m| m.item(pos)).and_downcast::<glib::BoxedAnyObject>();
        if let Some(obj) = obj {
            n.push(Route::Album(obj.borrow::<Arc<Album>>().id.clone()));
        }
    });
    let entry = filter_entry("Filter albums");
    let (c, s) = (app.catalog(), store.clone());
    entry.connect_search_changed(move |e| {
        let q = Query::new(&e.text());
        let index = c.album_index();
        let shown: Vec<_> =
            index.filter(&q).into_iter().map(|i| glib::BoxedAnyObject::new(index.items[i].clone())).collect();
        s.splice(0, s.n_items(), &shown);
    });
    let b = gtk::Box::builder().orientation(gtk::Orientation::Vertical).build();
    b.append(&entry);
    b.append(&widgets::scrolled(&grid));
    b.upcast()
}

fn album_page(app: &Rc<App>, a: &Album) -> gtk::Widget {
    let year = if a.year > 0 { format!(" · {}", a.year) } else { String::new() };
    let top = header(app, a.songs.first(), &a.name, &format!("{}{year} · {}", a.artist, summary(&a.songs)));
    let play = Play::List { name: a.name.clone(), source: format!("album:{}", a.id) };
    songs_page(app, Some(top.upcast()), a.songs.clone(), Columns { track: true, album: false }, play).upcast()
}

/// Artists or genres: a filtered list, each opening its songs.
fn groups_page(nav: &Rc<Nav>, groups: &[Arc<Group>], artists: bool) -> gtk::Widget {
    let app = &nav.app;
    if groups.is_empty() {
        return empty(app);
    }
    let a = app.clone();
    let n = nav.clone();
    let make = move |list: &[Arc<Group>]| {
        let (a, n) = (a.clone(), n.clone());
        let view = widgets::list(
            &a,
            list,
            {
                let a = a.clone();
                move |g: &Arc<Group>| row(&cover(&a, g.songs.first(), 36), &g.name, &summary(&g.songs)).upcast()
            },
            move |g| n.push(if artists { Route::Artist(g.name.clone()) } else { Route::Genre(g.name.clone()) }),
        );
        widgets::scrolled(&view)
    };
    filtered(groups.to_vec(), if artists { "Filter artists" } else { "Filter genres" }, make, {
        let c = app.catalog();
        move |q| {
            let index = if artists { c.artist_index() } else { c.genre_index() };
            index.filter(q).into_iter().map(|i| index.items[i].clone()).collect()
        }
    })
}

/// A list with a filter box over it, rebuilt as the filter changes.
fn filtered<T: Clone + 'static>(
    all: Vec<T>,
    placeholder: &str,
    make: impl Fn(&[T]) -> gtk::ScrolledWindow + 'static,
    find: impl Fn(&Query) -> Vec<T> + 'static,
) -> gtk::Widget {
    let b = gtk::Box::builder().orientation(gtk::Orientation::Vertical).build();
    let holder = adw::Bin::builder().vexpand(true).child(&make(&all)).build();
    if all.len() > widgets::FILTER_OVER {
        let entry = filter_entry(placeholder);
        let h = holder.clone();
        entry.connect_search_changed(move |e| {
            let q = Query::new(&e.text());
            let shown = if q.is_empty() { all.clone() } else { find(&q) };
            h.set_child(Some(&make(&shown)));
        });
        b.append(&entry);
    }
    b.append(&holder);
    b.upcast()
}

/// An artist's or genre's songs by album, then disc and track: how they
/// were released, as on the phone. An artist's albums in a strip above.
fn group_page(nav: &Rc<Nav>, g: &Group, artist: bool) -> gtk::Widget {
    let app = &nav.app;
    let c = app.catalog();
    let songs = catalog::album_order(&g.songs);
    let ids: std::collections::HashSet<&str> =
        g.songs.iter().filter_map(|s| c.album_of.get(&s.id)).map(|a| a.id.as_str()).collect();
    let albums: Vec<&Arc<Album>> = c.albums.iter().filter(|a| ids.contains(a.id.as_str())).collect();
    let top = gtk::Box::builder().orientation(gtk::Orientation::Vertical).build();
    let what = if artist { format!("{} albums · ", albums.len()) } else { String::new() };
    top.append(&header(app, songs.first(), &g.name, &format!("{what}{}", summary(&songs))));
    if artist && albums.len() > 1 {
        let strip = gtk::Box::builder().spacing(14).margin_start(18).margin_end(18).margin_bottom(12).build();
        for a in albums {
            let b = gtk::Box::builder().orientation(gtk::Orientation::Vertical).spacing(4).width_request(96).build();
            b.append(&cover(app, a.songs.first(), 96));
            b.append(
                &gtk::Label::builder()
                    .label(&a.name)
                    .xalign(0.0)
                    .ellipsize(gtk::pango::EllipsizeMode::End)
                    .max_width_chars(1)
                    .css_classes(["caption"])
                    .build(),
            );
            let button = gtk::Button::builder().child(&b).css_classes(["flat"]).build();
            let (n, id) = (nav.clone(), a.id.clone());
            button.connect_clicked(move |_| n.push(Route::Album(id.clone())));
            strip.append(&button);
        }
        top.append(&gtk::ScrolledWindow::builder().child(&strip).vscrollbar_policy(gtk::PolicyType::Never).build());
    }
    let play =
        Play::List { name: g.name.clone(), source: format!("{}:{}", if artist { "artist" } else { "genre" }, g.name) };
    songs_page(app, Some(top.upcast()), songs, Columns { track: false, album: true }, play).upcast()
}

/// The real NAS folder tree: subfolders, then the folder's own songs.
fn folder_page(nav: &Rc<Nav>, path: &str) -> gtk::Widget {
    let app = &nav.app;
    let c = app.catalog();
    let f = &c.folders[path];
    let b = gtk::Box::builder().orientation(gtk::Orientation::Vertical).build();
    // A folder's header, even one holding only folders: it holds them all.
    if !path.is_empty() {
        let n = f.children.len();
        let folders = match n {
            0 => String::new(),
            1 => "1 folder · ".into(),
            _ => format!("{n} folders · "),
        };
        b.append(&header(app, None, f.name(), &format!("{folders}{}", summary_of(f.song_count, f.duration_ms))));
    }
    if !f.children.is_empty() {
        let n = nav.clone();
        let cat = c.clone();
        let list = widgets::list(
            app,
            &f.children,
            move |p: &String| {
                let child = &cat.folders[p];
                let icon = match p.as_str() {
                    catalog::PODCASTS => "audio-input-microphone-symbolic",
                    catalog::AUDIOBOOKS => "accessories-dictionary-symbolic",
                    _ => "folder-symbolic",
                };
                let image = gtk::Image::builder().icon_name(icon).pixel_size(24).width_request(36).build();
                row(&image.upcast(), child.name(), &summary_of(child.song_count, child.duration_ms)).upcast()
            },
            move |p| n.push(Route::Folder(p.clone())),
        );
        let scroll = widgets::scrolled(&list);
        if !f.songs.is_empty() {
            scroll.set_vexpand(false);
            scroll.set_min_content_height(120);
            scroll.set_max_content_height(260);
            scroll.set_propagate_natural_height(true);
        }
        b.append(&scroll);
    }
    if !f.songs.is_empty() {
        if !f.children.is_empty() {
            b.append(&gtk::Separator::new(gtk::Orientation::Horizontal));
        }
        let play = Play::List { name: f.name().to_string(), source: format!("folder:{}", f.path) };
        b.append(&songs_page(app, None, f.songs.clone(), Columns { track: false, album: true }, play));
    }
    b.upcast()
}

/// Every audiobook, or every podcast show (plan 031).
fn shelves_page(nav: &Rc<Nav>, shelves: &[Arc<Shelf>]) -> gtk::Widget {
    let app = &nav.app;
    if shelves.is_empty() {
        return empty(app);
    }
    let (a, n) = (app.clone(), nav.clone());
    let view = widgets::list(
        app,
        shelves,
        move |s: &Arc<Shelf>| {
            let what =
                if s.author.is_empty() { summary(&s.items) } else { format!("{} · {}", s.author, summary(&s.items)) };
            row(&cover(&a, s.items.first(), 48), &s.title, &what).upcast()
        },
        move |s| n.push(Route::Shelf(s.id.clone())),
    );
    widgets::scrolled(&view).upcast()
}

/// One book's files in reading order, or one show's episodes oldest first.
fn shelf_page(app: &Rc<App>, s: &Shelf) -> gtk::Widget {
    let what = if s.author.is_empty() { summary(&s.items) } else { format!("{} · {}", s.author, summary(&s.items)) };
    let top = header(app, s.items.first(), &s.title, &what);
    let songs: Vec<SongRef> = s.items.clone();
    // A book plays whole from the file picked, its resume point doing the rest (plan 031).
    let source = format!("{}:{}", if s.is_book() { "book" } else { "show" }, s.title);
    let play = Play::List { name: s.title.clone(), source };
    songs_page(app, Some(top.upcast()), songs, Columns { track: false, album: false }, play).upcast()
}

/// The queues in the user's order, numbered from 1 as in Musicolet (plan 020).
fn queues_page(nav: &Rc<Nav>) -> gtk::Widget {
    let app = &nav.app;
    let pb = app.playback();
    let queues = pb.ordered();
    if queues.is_empty() {
        let s = adw::StatusPage::builder().title("No queues").icon_name("view-list-symbolic").build();
        s.set_description(Some("Play an album, a playlist or any list of songs, and it becomes a queue."));
        return s.upcast();
    }
    let active = pb.active_id();
    let (a, n) = (app.clone(), nav.clone());
    let numbered: Vec<(usize, dhun_core::model::QueueRow)> = queues.into_iter().enumerate().collect();
    let view = widgets::list(
        app,
        &numbered,
        move |(i, q)| {
            let songs = a.catalog().songs_of(&q.songs);
            let mark = if q.id == active { " · playing" } else { "" };
            let r = row(
                &cover(&a, songs.first(), 40),
                &format!("{}. {}", i + 1, q.name),
                &format!("{}{mark}", summary(&songs)),
            );
            r.upcast()
        },
        move |(_, q)| n.push(Route::Queue(q.id.clone())),
    );
    widgets::scrolled(&view).upcast()
}

/// One queue: its songs, played from the one picked; renamed or removed from its header.
fn queue_page(nav: &Rc<Nav>, q: &dhun_core::model::QueueRow) -> gtk::Widget {
    let app = &nav.app;
    let songs = app.catalog().songs_of(&q.songs);
    let top = header(app, songs.first(), &q.name, &summary(&songs));
    let buttons = gtk::Box::builder().spacing(8).valign(gtk::Align::Center).build();
    let play = gtk::Button::builder().label("Play").css_classes(["pill", "suggested-action"]).build();
    let rename = gtk::Button::builder()
        .icon_name("document-edit-symbolic")
        .tooltip_text("Rename")
        .css_classes(["circular"])
        .build();
    let delete = gtk::Button::builder()
        .icon_name("user-trash-symbolic")
        .tooltip_text("Remove the queue")
        .css_classes(["circular"])
        .build();
    buttons.append(&play);
    buttons.append(&rename);
    buttons.append(&delete);
    top.append(&buttons);
    let (a, id) = (app.clone(), q.id.clone());
    play.connect_clicked(move |_| {
        let pb = a.playback();
        if pb.active_id() == id { pb.play() } else { pb.switch_to(&id) }
    });
    let (a, id, n) = (app.clone(), q.id.clone(), nav.clone());
    delete.connect_clicked(move |_| {
        a.playback().delete(&id);
        n.open(Route::Queues);
    });
    let (a, id, name) = (app.clone(), q.id.clone(), q.name.clone());
    rename.connect_clicked(move |b| {
        let entry = gtk::Entry::builder().text(&name).activates_default(true).build();
        let dialog = adw::AlertDialog::builder().heading("Rename Queue").extra_child(&entry).build();
        dialog.add_responses(&[("cancel", "Cancel"), ("rename", "Rename")]);
        dialog.set_default_response(Some("rename"));
        dialog.set_response_appearance("rename", adw::ResponseAppearance::Suggested);
        let (a, id) = (a.clone(), id.clone());
        dialog.connect_response(Some("rename"), move |_, _| {
            let name = entry.text().trim().to_string();
            if !name.is_empty() {
                a.playback().rename(&id, &name);
            }
        });
        dialog.present(Some(b));
    });
    songs_page(app, Some(top.upcast()), songs, Columns { track: false, album: true }, Play::Queue(q.id.clone()))
        .upcast()
}
