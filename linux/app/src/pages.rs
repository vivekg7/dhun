//! The library's pages, the Mac's `Library.swift` in GTK: what each
//! sidebar section shows and the pages opened from it.

use crate::app::{App, Change};
use crate::lists::ListKind;
use crate::widgets::{self, Columns, Edit, Play, cover, header, row, songs_page, summary, summary_of};
use adw::prelude::*;
use dhun_core::catalog::{self, Album, Group, Shelf, SongRef};
use dhun_core::downloads::{self as dl, format_bytes};
use dhun_core::model::Playlist;
use dhun_core::queue;
use dhun_core::search::Query;
use dhun_core::spoken::Heard;
use dhun_core::store;
use dhun_core::synced;
use gtk::{gio, glib};
use std::cell::RefCell;
use std::rc::Rc;
use std::sync::Arc;

/// A page's menu item: given the page, to put a dialog over, and its target.
type PageAction = Box<dyn Fn(&gtk::Widget, Option<&glib::Variant>)>;
/// The same for a playlist's page, given the playlist as it is now.
type PlaylistAction = Box<dyn Fn(&gtk::Widget, &Playlist, Option<&glib::Variant>)>;

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
    List(ListKind),
    Search,
    Queues,
    Queue(String),
    Downloads,
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
    /// What the Downloads page was last built with, its pins and songs: it is
    /// built again when they change, between files rather than at each one.
    pins_shown: RefCell<Vec<String>>,
    /// What the queue page showing was built with, its name and songs: the
    /// queues change at each song as it plays, which leaves it as it was.
    queue_shown: RefCell<Option<(String, Vec<i64>)>>,
}

impl Nav {
    pub fn new(app: &Rc<App>) -> Rc<Nav> {
        let nav = Rc::new(Nav {
            view: adw::NavigationView::new(),
            routes: RefCell::new(vec![]),
            app: app.clone(),
            search: gtk::SearchEntry::builder().placeholder_text("Search").hexpand(true).build(),
            search_fill: RefCell::new(None),
            pins_shown: RefCell::default(),
            queue_shown: RefCell::default(),
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
        // A click into the empty field shows the recent searches, as Ctrl+F does.
        let click = gtk::GestureClick::new();
        click.set_propagation_phase(gtk::PropagationPhase::Capture);
        let weak = Rc::downgrade(&nav);
        click.connect_pressed(move |_, _, _, _| {
            if let Some(n) = weak.upgrade() {
                n.show_recent();
            }
        });
        nav.search.add_controller(click);
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
                || (c == Change::Queues && shows(|r| *r == Route::Queues))
                || (c == Change::Queues
                    && shows(|r| matches!(r, Route::Queue(_)))
                    && *n.queue_shown.borrow() != queue_key(&n.app, &routes))
                || (c == Change::Marks
                    && shows(|r| matches!(r, Route::List(k) if k.mark().is_some()) || matches!(r, Route::Shelf(_))))
                || (c == Change::Downloads
                    && shows(|r| *r == Route::Downloads)
                    && n.app.downloads().status.borrow().state != crate::downloads::State::Downloading
                    && *n.pins_shown.borrow() != pin_keys(&n.app))
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
        self.replace(&[self.build(&r)]);
        self.app.changed(Change::Section);
    }

    pub fn push(self: &Rc<Self>, r: Route) {
        self.routes.borrow_mut().push(r.clone());
        self.view.push(&self.build(&r));
    }

    /// The search page while the field is empty: the recent searches (plan 029).
    pub fn show_recent(self: &Rc<Self>) {
        if self.search.text().is_empty() && self.current() != Some(Route::Search) {
            self.open(Route::Search);
        }
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

    /// The same pages built again, scrolled where they were: an edit to a
    /// queue or a playlist must not send the user back to its top.
    fn rebuild(self: &Rc<Self>) {
        let routes = self.routes.borrow().clone();
        let at = self.view.visible_page().and_then(|p| first::<gtk::ScrolledWindow>(p.upcast_ref()));
        let at = at.map(|s| s.vadjustment().value()).filter(|&v| v > 0.0);
        let pages: Vec<_> = routes.iter().map(|r| self.build(r)).collect();
        self.replace(&pages);
        let new = pages.last().and_then(|p| first::<gtk::ScrolledWindow>(p.upcast_ref()));
        if let (Some(v), Some(sw)) = (at, new) {
            // Once the new list is laid out tall enough to be there.
            let adj = sw.vadjustment();
            let id: Rc<RefCell<Option<glib::SignalHandlerId>>> = Rc::default();
            let i = id.clone();
            *id.borrow_mut() = Some(adj.connect_changed(move |a| {
                if a.upper() - a.page_size() >= v {
                    a.set_value(v);
                    if let Some(h) = i.borrow_mut().take() {
                        a.disconnect(h);
                    }
                }
            }));
        }
    }

    /// The stack made `pages`. A pushed page that `NavigationView::replace`
    /// drops is never freed, 2–5 MB each, where a popped one is (measured
    /// 2026-10-24, libadwaita 1.10): so the pushed pages are popped first,
    /// at once. Their pops leave the routes as they were.
    fn replace(&self, pages: &[adw::NavigationPage]) {
        let routes = self.routes.borrow().clone();
        self.view.set_animate_transitions(false);
        while self.view.pop() {}
        self.view.set_animate_transitions(true);
        *self.routes.borrow_mut() = routes;
        self.view.replace(pages);
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
            Route::Playlist(id) => {
                // Looked up first, so the page is built with nothing borrowed.
                let found = app.playlists.borrow().iter().find(|p| p.id == *id).cloned();
                match found {
                    Some(p) => (p.name.clone(), playlist_page(self, &p)),
                    None => ("Playlist".into(), empty(app)),
                }
            }
            Route::List(k) => (k.title().into(), list_page(app, *k)),
            Route::Search => ("Search".into(), crate::search::page(self)),
            Route::Downloads => ("Downloads".into(), downloads_page(self)),
            Route::Queues => ("Queues".into(), queues_page(self)),
            Route::Queue(id) => {
                let found = app.queues.borrow().iter().find(|q| &q.id == id).cloned();
                *self.queue_shown.borrow_mut() = found.as_ref().map(|q| (q.name.clone(), q.songs.clone()));
                match found {
                    Some(q) => (q.name.clone(), queue_page(self, &q)),
                    None => ("Queue".into(), empty(app)),
                }
            }
        };
        let toolbar = adw::ToolbarView::new();
        toolbar.add_top_bar(&adw::HeaderBar::new());
        toolbar.set_content(Some(&child));
        adw::NavigationPage::builder().title(title).child(&toolbar).build()
    }
}

/// The first `T` under `w`, depth first.
fn first<T: IsA<gtk::Widget>>(w: &gtk::Widget) -> Option<T> {
    if let Some(v) = w.downcast_ref::<T>() {
        return Some(v.clone());
    }
    let mut c = w.first_child();
    while let Some(x) = c {
        if let Some(v) = first(&x) {
            return Some(v);
        }
        c = x.next_sibling();
    }
    None
}

/// The name and songs of the queue on the page stack, to tell an edit from a song played.
fn queue_key(app: &App, routes: &[Route]) -> Option<(String, Vec<i64>)> {
    let id = routes.iter().find_map(|r| if let Route::Queue(id) = r { Some(id) } else { None })?;
    app.queues.borrow().iter().find(|q| &q.id == id).map(|q| (q.name.clone(), q.songs.clone()))
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
    let a = app.clone();
    factory.connect_setup(move |_, item| {
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
        // The name, with the "downloaded" mark after it, then two lines under.
        let name = gtk::Box::builder().spacing(4).build();
        for class in ["heading", "caption", "caption"] {
            let l =
                gtk::Label::builder().xalign(0.0).ellipsize(gtk::pango::EllipsizeMode::End).max_width_chars(1).build();
            l.add_css_class(class);
            if class == "caption" {
                l.add_css_class("dim-label");
                b.append(&l);
            } else {
                l.set_hexpand(true);
                name.append(&l);
                b.append(&name);
            }
        }
        let mark = widgets::downloaded_mark();
        name.append(&mark);
        let item = item.downcast_ref::<gtk::ListItem>().unwrap();
        item.set_child(Some(&b));
        // The cell is recycled: the mark follows the album it shows.
        let (a, w, m) = (a.clone(), item.downgrade(), mark.downgrade());
        a.clone().listen(move |c| {
            let (Some(item), Some(mark)) = (w.upgrade(), m.upgrade()) else { return false };
            if c == Change::Downloads {
                album_mark(&a, &item, &mark);
            }
            true
        });
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
        let name = pic.parent().unwrap().next_sibling().unwrap();
        let title = name.first_child().and_downcast::<gtk::Label>().unwrap();
        title.set_label(&album.name);
        album_mark(&a, item, &title.next_sibling().and_downcast::<gtk::Image>().unwrap());
        let mut l = name.next_sibling().and_downcast::<gtk::Label>();
        let year = if album.year > 0 { format!("{} · ", album.year) } else { String::new() };
        for text in [album.artist.clone(), format!("{year}{}", summary(&album.songs))] {
            let label = l.unwrap();
            label.set_label(&text);
            l = label.next_sibling().and_downcast::<gtk::Label>();
        }
    });
    let grid = gtk::GridView::new(Some(gtk::NoSelection::new(Some(store.clone()))), Some(factory));
    grid.set_max_columns(12);
    grid.set_single_click_activate(true);
    grid.add_css_class("album-grid");
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

/// An album cell's "downloaded" mark, for the album bound to it now.
fn album_mark(app: &App, item: &gtk::ListItem, mark: &gtk::Image) {
    let Some(obj) = item.item().and_downcast::<glib::BoxedAnyObject>() else { return };
    let key = dl::key(dl::ALBUM, &obj.borrow::<Arc<Album>>().id);
    mark.set_visible(app.pins.borrow().iter().any(|p| p.key == key));
}

fn album_page(app: &Rc<App>, a: &Album) -> gtk::Widget {
    let year = if a.year > 0 { format!(" · {}", a.year) } else { String::new() };
    let what = format!("{}{year} · {}", a.artist, summary(&a.songs));
    let top = header(app, a.songs.first(), &a.name, &what, Some((dl::ALBUM, a.id.clone())));
    let play = Play::List { name: a.name.clone(), source: format!("album:{}", a.id) };
    songs_page(
        app,
        Some(top.upcast()),
        a.songs.clone(),
        Columns { track: true, album: false, ..Default::default() },
        play,
        Edit::None,
    )
    .upcast()
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
            list,
            {
                let a = a.clone();
                move |g: &Arc<Group>| {
                    let r = row(&cover(&a, g.songs.first(), 36), &g.name, &summary(&g.songs));
                    r.append(&widgets::pin_mark(&a, if artists { dl::ARTIST } else { dl::GENRE }, &g.name));
                    r.upcast()
                }
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
    let pin = Some((if artist { dl::ARTIST } else { dl::GENRE }, g.name.clone()));
    top.append(&header(app, songs.first(), &g.name, &format!("{what}{}", summary(&songs)), pin));
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
    songs_page(
        app,
        Some(top.upcast()),
        songs,
        Columns { track: false, album: true, ..Default::default() },
        play,
        Edit::None,
    )
    .upcast()
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
        let what = format!("{folders}{}", summary_of(f.song_count, f.duration_ms));
        let top = header(app, None, f.name(), &what, Some((dl::FOLDER, f.path.clone())));
        widgets::play_buttons(app, &top, f.name(), &format!("folder:{}", f.path), &c.all_songs(&f.path));
        b.append(&top);
    }
    if !f.children.is_empty() {
        let (n, a) = (nav.clone(), app.clone());
        let cat = c.clone();
        let list = widgets::list(
            &f.children,
            move |p: &String| {
                let child = &cat.folders[p];
                let icon = match p.as_str() {
                    catalog::PODCASTS => "audio-input-microphone-symbolic",
                    catalog::AUDIOBOOKS => "accessories-dictionary-symbolic",
                    _ => "folder-symbolic",
                };
                let image = gtk::Image::builder().icon_name(icon).pixel_size(24).width_request(36).build();
                let r = row(&image.upcast(), child.name(), &summary_of(child.song_count, child.duration_ms));
                r.append(&widgets::pin_mark(&a, dl::FOLDER, p));
                r.upcast()
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
        b.append(&songs_page(
            app,
            None,
            f.songs.clone(),
            Columns { track: false, album: true, ..Default::default() },
            play,
            Edit::None,
        ));
    }
    b.upcast()
}

/// Every audiobook, or every podcast show (plan 031).
fn shelves_page(nav: &Rc<Nav>, shelves: &[Arc<Shelf>]) -> gtk::Widget {
    let app = &nav.app;
    if shelves.is_empty() {
        return empty(app);
    }
    let heard = Rc::new(app.heard());
    let (a, n) = (app.clone(), nav.clone());
    let view = widgets::list(
        shelves,
        move |s: &Arc<Shelf>| {
            let r = row(&cover(&a, s.cover().or(s.items.first()), 48), &s.title, &shelf_line(s, &heard));
            r.append(&widgets::pin_mark(&a, dl::SHELF, &s.id));
            r.upcast()
        },
        move |s| n.push(Route::Shelf(s.id.clone())),
    );
    widgets::scrolled(&view).upcast()
}

/// A book's line under its title, its author and how far along; a show's, its episodes not yet played.
fn shelf_line(s: &Shelf, heard: &Heard) -> String {
    let parts = if s.is_book() {
        vec![s.author.clone(), heard.progress(s)]
    } else {
        let new = s.items.iter().filter(|e| !heard.played.contains(&e.id)).count();
        let n = s.items.len();
        vec![
            format!("{n} episode{}", if n == 1 { "" } else { "s" }),
            if new > 0 { format!("{new} not played") } else { String::new() },
        ]
    };
    parts.into_iter().filter(|p| !p.is_empty()).collect::<Vec<_>>().join(" · ")
}

/// Playing from a book makes a queue of the whole book, named after it,
/// from the file picked or the first one not played; its resume point does
/// the rest. Playing an episode makes a queue of the show's episodes from
/// that one on, those already played left out.
pub fn play_shelf(app: &Rc<App>, shelf: &Shelf, from: Option<&SongRef>) {
    let heard = app.heard();
    let start = from.map_or_else(|| heard.next(shelf).id, |s| s.id);
    let source = format!("{}:{}", if shelf.is_book() { "book" } else { "show" }, shelf.title);
    let i = shelf.items.iter().position(|s| s.id == start).unwrap_or(0);
    if shelf.is_book() {
        app.playback().play_list(&shelf.title, &source, &shelf.items, i);
    } else {
        let rest: Vec<SongRef> =
            shelf.items[i..].iter().filter(|s| s.id == start || !heard.played.contains(&s.id)).cloned().collect();
        app.playback().play_list(&shelf.title, &source, &rest, 0);
    }
}

/// A book, its files in reading order; or a show, its episodes newest
/// first, those played hidden unless asked for.
fn shelf_page(app: &Rc<App>, s: &Arc<Shelf>) -> gtk::Widget {
    let heard = app.heard();
    let hide = !s.is_book() && synced::HIDE_PLAYED.get(&app.settings.borrow());
    let songs: Vec<SongRef> = if s.is_book() {
        s.items.clone()
    } else {
        s.items.iter().rev().filter(|e| !hide || !heard.played.contains(&e.id)).cloned().collect()
    };
    let top = header(app, s.cover().or(s.items.first()), &s.title, &shelf_line(s, &heard), None);
    let buttons = widgets::buttons(&top).unwrap();
    let play = gtk::Button::builder()
        .label(if heard.begun(s) { "Continue" } else { "Play" })
        .css_classes(["pill", "suggested-action"])
        .build();
    let (a, shelf) = (app.clone(), s.clone());
    play.connect_clicked(move |_| play_shelf(&a, &shelf, None));
    buttons.append(&play);
    if s.is_book() {
        buttons.append(&widgets::pin_button(app, dl::SHELF, s.id.clone(), &s.title));
    } else {
        let toggle = gtk::Button::builder()
            .label(if hide { "Show Played" } else { "Hide Played" })
            .css_classes(["pill"])
            .build();
        let a = app.clone();
        toggle.connect_clicked(move |_| {
            a.store(|db| store::setting(db, synced::HIDE_PLAYED.0, (!hide).into()));
            a.reload_queues();
            a.changed(Change::Marks);
        });
        buttons.append(&toggle);
    }
    if songs.is_empty() {
        let b = gtk::Box::builder().orientation(gtk::Orientation::Vertical).build();
        b.append(&top);
        b.append(
            &adw::StatusPage::builder()
                .title("Every episode is played")
                .icon_name("object-select-symbolic")
                .vexpand(true)
                .build(),
        );
        return b.upcast();
    }
    // A book's files are often titled alike but for a part number at the end.
    let part = if s.is_book() && songs.len() > 1 {
        songs.iter().enumerate().map(|(i, x)| (x.id, format!("{} of {}", i + 1, songs.len()))).collect()
    } else {
        Default::default()
    };
    let shelf = Some(Rc::new(widgets::ShelfColumns { date: !s.is_book(), part, heard }));
    let cols = Columns { shelf, ..Default::default() };
    songs_page(app, Some(top.upcast()), songs, cols, Play::Shelf(s.id.clone()), Edit::None).upcast()
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
        &numbered,
        move |(i, q)| {
            let songs = a.catalog().songs_of(&q.songs);
            let mark = if q.id == active { " · playing" } else { "" };
            let r = row(
                &cover(&a, songs.first(), 40),
                &format!("{}. {}", i + 1, q.name),
                &format!("{}{mark}", summary(&songs)),
            );
            // Dragged into the user's order (plan 020).
            let (i, p) = (*i, a.playback().clone());
            widgets::drag_rows(&r, move || Some(i), Rc::new(move |from, to| p.move_queue(from, to)));
            r.upcast()
        },
        move |(_, q)| n.push(Route::Queue(q.id.clone())),
    );
    widgets::scrolled(&view).upcast()
}

/// One queue: its songs, played from the one picked and dragged into
/// another order; sorted, renamed, saved as a playlist or removed from its header.
fn queue_page(nav: &Rc<Nav>, q: &dhun_core::model::QueueRow) -> gtk::Widget {
    let app = &nav.app;
    let songs = app.catalog().songs_of(&q.songs);
    let top = header(app, songs.first(), &q.name, &summary(&songs), None);
    let buttons = widgets::buttons(&top).unwrap();
    let play = gtk::Button::builder().label("Play").css_classes(["pill", "suggested-action"]).build();
    buttons.append(&play);
    let sort = gtk::MenuButton::builder()
        .icon_name("view-sort-descending-symbolic")
        .tooltip_text("Sort")
        .menu_model(&sort_menu("queue.sort"))
        .css_classes(["circular"])
        .sensitive(songs.len() > 1)
        .build();
    buttons.append(&sort);
    let more = gio::Menu::new();
    more.append(Some("Rename…"), Some("queue.rename"));
    more.append(Some("Save as Playlist…"), Some("queue.save"));
    let gone = gio::Menu::new();
    gone.append(Some("Remove Queue…"), Some("queue.delete"));
    gone.append(Some("Remove All Other Queues…"), Some("queue.delete-others"));
    more.append_section(None, &gone);
    buttons.append(
        &gtk::MenuButton::builder().icon_name("view-more-symbolic").menu_model(&more).css_classes(["circular"]).build(),
    );
    let (a, id) = (app.clone(), q.id.clone());
    play.connect_clicked(move |_| {
        let pb = a.playback();
        if pb.active_id() == id { pb.play() } else { pb.switch_to(&id) }
    });
    let group = gio::SimpleActionGroup::new();
    let action = |name: &str, arg: Option<&glib::VariantTy>, f: PageAction| {
        let act = gio::SimpleAction::new(name, arg);
        let t = top.downgrade();
        act.connect_activate(move |_, v| {
            if let Some(t) = t.upgrade() {
                f(t.upcast_ref(), v)
            }
        });
        group.add_action(&act);
    };
    let (a, id, list) = (app.clone(), q.id.clone(), songs.clone());
    action(
        "sort",
        Some(glib::VariantTy::INT64),
        Box::new(move |_, v| {
            let by = v.and_then(|v| v.get::<i64>()).unwrap_or(0) as usize;
            a.playback().reorder(&id, queue::sorted(by, &list).iter().map(|s| s.id).collect());
        }),
    );
    let (a, id, name) = (app.clone(), q.id.clone(), q.name.clone());
    action(
        "rename",
        None,
        Box::new(move |w, _| {
            let (a, id) = (a.clone(), id.clone());
            widgets::ask_name(w, "Rename Queue", &name, "Rename", move |n| a.playback().rename(&id, &n));
        }),
    );
    let (a, name, ids) = (app.clone(), q.name.clone(), q.songs.clone());
    action(
        "save",
        None,
        Box::new(move |w, _| {
            let (a, ids) = (a.clone(), ids.clone());
            widgets::ask_name(w, "Save as Playlist", &name, "Save", move |n| {
                let c = a.catalog();
                let music: Vec<i64> =
                    ids.iter().copied().filter(|i| c.by_id.get(i).is_some_and(|s| !s.spoken())).collect();
                a.store(|db| store::create_playlist(db, &n, &music).map(|_| ()));
                a.reload_playlists();
            });
        }),
    );
    let (a, id, name, n) = (app.clone(), q.id.clone(), q.name.clone(), nav.clone());
    action(
        "delete",
        None,
        Box::new(move |w, _| {
            let (a, id, n) = (a.clone(), id.clone(), n.clone());
            let heading = format!("Remove “{name}”?");
            widgets::confirm(w, &heading, "The queue goes; its songs stay in your library.", "Remove", move || {
                a.playback().delete(&id);
                n.open(Route::Queues);
            });
        }),
    );
    let (a, id, name) = (app.clone(), q.id.clone(), q.name.clone());
    action(
        "delete-others",
        None,
        Box::new(move |w, _| {
            let (a, id) = (a.clone(), id.clone());
            let heading = format!("Remove every queue but “{name}”?");
            widgets::confirm(w, &heading, "", "Remove", move || a.playback().delete_others(&id));
        }),
    );
    let page = songs_page(
        app,
        Some(top.clone().upcast()),
        songs,
        Columns { track: false, album: true, ..Default::default() },
        Play::Queue(q.id.clone()),
        Edit::None,
    );
    page.insert_action_group("queue", Some(&group));
    page.upcast()
}

/// The sort orders as a menu, each item `action` with its number.
fn sort_menu(action: &str) -> gio::Menu {
    // Randomize and Reverse, then by the tags.
    let (m, shuffles, tags) = (gio::Menu::new(), gio::Menu::new(), gio::Menu::new());
    for (i, label) in queue::SORTS.iter().enumerate() {
        let item = gio::MenuItem::new(Some(label), None);
        item.set_action_and_target_value(Some(action), Some(&(i as i64).to_variant()));
        if i < 2 { &shuffles } else { &tags }.append_item(&item);
    }
    m.append_section(None, &shuffles);
    m.append_section(None, &tags);
    m
}

/// A playlist (plan 013): the user's own to edit, its songs dragged into
/// another order; a shared one is read-only in the app.
fn playlist_page(nav: &Rc<Nav>, p: &Playlist) -> gtk::Widget {
    let app = &nav.app;
    let c = app.catalog();
    let songs = c.songs_of(&p.songs);
    let shared = if p.shared { "Shared · " } else { "" };
    // A playlist made here has no download until the server knows it.
    let pin = (p.id > 0).then(|| (dl::PLAYLIST, p.id.to_string()));
    let top = header(app, songs.first(), &p.name, &format!("{shared}{}", summary(&songs)), pin);
    let play = Play::List { name: p.name.clone(), source: format!("playlist:{}", p.id) };
    let cols = Columns { track: false, album: true, ..Default::default() };
    if p.shared {
        return songs_page(app, Some(top.upcast()), songs, cols, play, Edit::None).upcast();
    }
    let edit = gio::Menu::new();
    edit.append(Some("Rename…"), Some("playlist.rename"));
    edit.append_submenu(Some("Sort"), &sort_menu("playlist.sort"));
    let gone = gio::Menu::new();
    gone.append(Some("Delete Playlist…"), Some("playlist.delete"));
    edit.append_section(None, &gone);
    widgets::play_buttons(app, &top, &p.name, &format!("playlist:{}", p.id), &songs);
    widgets::buttons(&top).unwrap().append(
        &gtk::MenuButton::builder()
            .icon_name("document-edit-symbolic")
            .tooltip_text("Edit")
            .menu_model(&edit)
            .css_classes(["circular"])
            .valign(gtk::Align::Center)
            .build(),
    );
    let group = gio::SimpleActionGroup::new();
    let action = |name: &str, arg: Option<&glib::VariantTy>, f: PlaylistAction| {
        let act = gio::SimpleAction::new(name, arg);
        let (t, a, id) = (top.downgrade(), app.clone(), p.id);
        act.connect_activate(move |_, v| {
            let found = a.playlists.borrow().iter().find(|p| p.id == id).cloned();
            if let (Some(t), Some(p)) = (t.upgrade(), found) {
                f(t.upcast_ref(), &p, v)
            }
        });
        group.add_action(&act);
    };
    let a = app.clone();
    action(
        "rename",
        None,
        Box::new(move |w, p, _| {
            let (a, p2) = (a.clone(), p.clone());
            widgets::ask_name(w, "Rename Playlist", &p.name, "Rename", move |n| {
                a.store(|db| store::rename_playlist(db, &p2, &n));
                a.reload_playlists();
            });
        }),
    );
    let a = app.clone();
    action(
        "sort",
        Some(glib::VariantTy::INT64),
        Box::new(move |_, p, v| {
            let by = v.and_then(|v| v.get::<i64>()).unwrap_or(0) as usize;
            let c = a.catalog();
            // Entries that match no song stay, at the end.
            let unmatched = p.songs.iter().copied().filter(|i| !c.by_id.contains_key(i));
            let order: Vec<i64> =
                queue::sorted(by, &c.songs_of(&p.songs)).iter().map(|s| s.id).chain(unmatched).collect();
            a.store(|db| store::replace_playlist(db, p, &order));
            a.reload_playlists();
        }),
    );
    let (a, n) = (app.clone(), nav.clone());
    action(
        "delete",
        None,
        Box::new(move |w, p, _| {
            let (a, n, p2) = (a.clone(), n.clone(), p.clone());
            let heading = format!("Delete “{}”?", p.name);
            widgets::confirm(w, &heading, "The server keeps a copy of the file.", "Delete", move || {
                a.store(|db| store::delete_playlist(db, &p2));
                a.reload_playlists();
                a.reload_pins();
                n.open(Route::Albums);
            });
        }),
    );
    // Rows are the entries that match a song, each knowing its place in the playlist.
    let entries: Vec<(usize, SongRef)> =
        p.songs.iter().enumerate().filter_map(|(i, id)| c.by_id.get(id).map(|s| (i, s.clone()))).collect();
    let b = gtk::Box::builder().orientation(gtk::Orientation::Vertical).build();
    b.append(&top);
    let holder = adw::Bin::builder().vexpand(true).build();
    let fill = {
        let (app, holder, id) = (app.clone(), holder.clone(), p.id);
        move |query: &str| {
            let hits: std::collections::HashSet<i64> = if query.is_empty() {
                Default::default()
            } else {
                let all: Vec<SongRef> = entries.iter().map(|e| e.1.clone()).collect();
                app.catalog().filter(&all, query).iter().map(|s| s.id).collect()
            };
            let shown: Vec<&(usize, SongRef)> =
                entries.iter().filter(|e| query.is_empty() || hits.contains(&e.1.id)).collect();
            let rows: Vec<SongRef> = shown.iter().map(|e| e.1.clone()).collect();
            let edit = Edit::Playlist { id, entries: Rc::new(shown.iter().map(|e| e.0).collect()) };
            holder.set_child(Some(&widgets::scrolled(&widgets::song_list(
                &app,
                &rows,
                cols.clone(),
                play.clone(),
                edit,
            ))));
        }
    };
    fill("");
    if p.songs.len() > widgets::FILTER_OVER {
        let entry = filter_entry("Filter songs");
        entry.connect_search_changed(move |e| fill(&e.text()));
        b.append(&entry);
    }
    b.append(&holder);
    b.insert_action_group("playlist", Some(&group));
    b.upcast()
}

/// Favorites, Listen Later or one of the automatic views (plan 010).
fn list_page(app: &Rc<App>, k: ListKind) -> gtk::Widget {
    let songs = k.songs(app);
    let pin = k.mark().map(|m| (dl::LIST, m.to_string()));
    let top = header(app, songs.first(), k.title(), &summary(&songs), pin);
    let play = Play::List { name: k.title().into(), source: k.source().into() };
    let edit = k.mark().map_or(Edit::None, |m| Edit::Mark(m, k.title()));
    songs_page(app, Some(top.upcast()), songs, Columns { track: false, album: true, ..Default::default() }, play, edit)
        .upcast()
}

fn pin_keys(app: &App) -> Vec<String> {
    let mut songs: Vec<i64> = app.downloaded.borrow().keys().copied().collect();
    songs.sort();
    let pins = app.pins.borrow().iter().map(|p| p.key.clone()).collect::<Vec<_>>();
    pins.into_iter().chain(songs.iter().map(|s| s.to_string())).collect()
}

/// What is kept on this computer (plan 012): how far along it is, each
/// download by name, and the songs themselves to play without the server.
fn downloads_page(nav: &Rc<Nav>) -> gtk::Widget {
    let app = &nav.app;
    *nav.pins_shown.borrow_mut() = pin_keys(app);
    let b = gtk::Box::builder().orientation(gtk::Orientation::Vertical).build();
    let top = gtk::Box::builder()
        .orientation(gtk::Orientation::Vertical)
        .spacing(6)
        .margin_top(18)
        .margin_bottom(12)
        .margin_start(18)
        .margin_end(18)
        .build();
    top.append(&gtk::Label::builder().label("Downloads").xalign(0.0).css_classes(["title-2"]).build());
    let line = gtk::Label::builder().xalign(0.0).wrap(true).css_classes(["dim-label"]).build();
    let state = gtk::Label::builder().xalign(0.0).wrap(true).build();
    let bar = gtk::ProgressBar::builder().visible(false).build();
    top.append(&line);
    top.append(&state);
    top.append(&bar);
    top.append(&gtk::Box::builder().spacing(8).margin_top(6).name("buttons").build());
    b.append(&top);
    let c = app.catalog();
    let downloaded = app.downloaded.borrow().clone();
    // Apart from the songs, so playing every download plays music only (plan 031).
    let mut songs = c.songs_of(&downloaded.keys().copied().collect::<Vec<_>>());
    songs.retain(|s| !s.spoken());
    let songs = catalog::natural_sorted(songs, |s| s.title.as_str());
    // A book's files and a show's episodes, by book and show, in their order.
    let spoken: Vec<SongRef> = c
        .books
        .iter()
        .chain(&c.shows)
        .flat_map(|s| s.items.iter())
        .filter(|s| downloaded.contains_key(&s.id))
        .cloned()
        .collect();
    widgets::play_buttons(app, &top, "Downloads", "downloads", &songs);
    let show = {
        let (app, line, state, bar) = (app.clone(), line.downgrade(), state.downgrade(), bar.downgrade());
        move || {
            let (Some(line), Some(state), Some(bar)) = (line.upgrade(), state.upgrade(), bar.upgrade()) else {
                return false;
            };
            let st = app.downloads().status.borrow().clone();
            line.set_label(&format!(
                "{} of {} songs on this computer · {} used",
                st.done,
                st.wanted,
                format_bytes(st.used)
            ));
            use crate::downloads::State;
            let (text, class) = match st.state {
                State::Downloading => (st.current.as_ref().map(|s| format!("Downloading “{}”", s.title)), None),
                State::NoNetwork => (Some("Waiting for the server".into()), Some("warning")),
                State::Metered => (Some("Waiting for a network that is not metered".into()), Some("warning")),
                State::Full => {
                    (Some(format!("At the storage limit: {} more needed", format_bytes(st.need))), Some("warning"))
                }
                State::NoSpace => {
                    (Some(format!("The disk is full: {} more needed", format_bytes(st.need))), Some("warning"))
                }
                State::Failed => (Some(format!("Failed: {}. Trying again shortly.", st.error)), Some("error")),
                State::Idle => (None, None),
            };
            state.set_visible(text.is_some());
            state.set_label(text.as_deref().unwrap_or(""));
            state.set_css_classes(class.as_slice());
            bar.set_visible(st.state == State::Downloading);
            bar.set_fraction(st.progress.clamp(0.0, 1.0));
            true
        }
    };
    show();
    app.listen(move |c| if matches!(c, Change::Downloads | Change::Playable) { show() } else { true });

    let pins = app.pins.borrow().clone();
    if pins.is_empty() {
        let s = adw::StatusPage::builder()
            .title("Nothing downloaded")
            .icon_name("folder-download-symbolic")
            .description("Download an album, a playlist or a folder to play it without the server.")
            .vexpand(true)
            .build();
        b.append(&s);
        return b.upcast();
    }
    let a = app.clone();
    let list = widgets::list(
        &pins,
        move |p| {
            let (icon, kind) = match p.kind.as_str() {
                dl::ALBUM => ("media-optical-symbolic", "Album"),
                dl::FOLDER => ("folder-symbolic", "Folder"),
                dl::ARTIST => ("avatar-default-symbolic", "Artist"),
                dl::GENRE => ("view-grid-symbolic", "Genre"),
                dl::PLAYLIST => ("view-list-symbolic", "Playlist"),
                dl::LIST => ("starred-symbolic", "List"),
                dl::SHELF if c.shelves.get(&p.r#ref).is_some_and(|s| !s.is_book()) => {
                    ("audio-input-microphone-symbolic", "Podcast")
                }
                dl::SHELF => ("accessories-dictionary-symbolic", "Audiobook"),
                _ => ("audio-x-generic-symbolic", "Song"),
            };
            let image = gtk::Image::builder().icon_name(icon).pixel_size(24).width_request(36).build();
            // A song taken over from someone else's downloads is named once the catalogue is here.
            let title = match p.r#ref.parse().ok().and_then(|id: i64| c.by_id.get(&id)) {
                Some(s) if p.name.is_empty() && p.kind == dl::SONG => s.title.clone(),
                _ => p.name.clone(),
            };
            let r = row(&image.upcast(), &title, kind);
            let remove = gtk::Button::builder()
                .icon_name("window-close-symbolic")
                .tooltip_text("Remove the download")
                .css_classes(["flat", "circular"])
                .valign(gtk::Align::Center)
                .hexpand(true)
                .halign(gtk::Align::End)
                .build();
            let (a, key, name) = (a.clone(), p.key.clone(), p.name.clone());
            remove.connect_clicked(move |b| {
                let (d, key) = (a.downloads().clone(), key.clone());
                widgets::confirm_unpin(b.upcast_ref(), &name, move || d.unpin(&key));
            });
            r.append(&remove);
            r.upcast()
        },
        |_| {},
    );
    let scroll = widgets::scrolled(&list);
    let bounded = |scroll: &gtk::ScrolledWindow| {
        scroll.set_vexpand(false);
        scroll.set_min_content_height(120);
        scroll.set_max_content_height(220);
        scroll.set_propagate_natural_height(true);
    };
    if !songs.is_empty() || !spoken.is_empty() {
        bounded(&scroll);
    }
    b.append(&scroll);
    let heading = |text: String| {
        gtk::Label::builder().label(text).xalign(0.0).margin_start(18).margin_top(12).css_classes(["heading"]).build()
    };
    let any_songs = !songs.is_empty();
    if any_songs {
        b.append(&gtk::Separator::new(gtk::Orientation::Horizontal));
        b.append(&heading(format!("Songs on this computer · {}", summary(&songs))));
        let play = Play::List { name: "Downloads".into(), source: "downloads".into() };
        b.append(&songs_page(
            app,
            None,
            songs,
            Columns { track: false, album: true, ..Default::default() },
            play,
            Edit::None,
        ));
    }
    // Each plays its own show or book from it, as on their pages.
    if !spoken.is_empty() {
        b.append(&gtk::Separator::new(gtk::Orientation::Horizontal));
        let (n, ms) = (spoken.len(), spoken.iter().map(|s| s.duration_ms).sum());
        let files = if n == 1 { "1 file".to_string() } else { format!("{n} files") };
        b.append(&heading(format!("Podcasts and audiobooks · {files} · {}", widgets::clock(ms))));
        let cols = Columns { track: false, album: true, ..Default::default() };
        let list = widgets::scrolled(&widgets::song_list(app, &spoken, cols, Play::Spoken, Edit::None));
        if any_songs {
            bounded(&list);
        }
        b.append(&list);
    }
    b.upcast()
}
