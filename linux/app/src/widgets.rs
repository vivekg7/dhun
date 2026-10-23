//! Pieces every page uses: covers, rows, the song list, the filter box.

use crate::app::{App, Change};
use adw::prelude::*;
use dhun_core::catalog::SongRef;
use gtk::{gio, glib};
use std::rc::Rc;

/// A square cover: the thumbnail at once, a larger copy fetched when the
/// size calls for it (plan 019), a placeholder for a song with none.
pub fn cover(app: &Rc<App>, song: Option<&SongRef>, size: i32) -> gtk::Widget {
    // An image of a fixed pixel size: a picture's natural size is its
    // image's, 256 px or more for a fetched cover, and it would grow to it.
    let image = gtk::Image::builder().pixel_size(size).build();
    let frame = gtk::Frame::builder()
        .child(&image)
        .halign(gtk::Align::Start)
        .valign(gtk::Align::Center)
        .css_classes(["cover"])
        .build();
    set_cover(app, &image, song, size, None);
    frame.upcast()
}

/// Shows `song`'s cover in `target`, a picture or an image. `still` says
/// whether it still shows that song when a larger copy arrives: a recycled
/// grid cell may show another by then.
pub fn set_cover(
    app: &Rc<App>,
    target: &impl IsA<gtk::Widget>,
    song: Option<&SongRef>,
    size: i32,
    still: Option<Rc<dyn Fn() -> bool>>,
) {
    let target = target.as_ref().clone();
    let Some(song) = song else {
        placeholder(&target);
        return;
    };
    match app.thumbnail(song) {
        Some(t) => put(&target, &t),
        None => placeholder(&target),
    }
    let px = (size * target.scale_factor().max(1)) as u32;
    if px <= dhun_core::covers::THUMB || !song.has_art {
        return;
    }
    let weak = target.downgrade();
    app.cover(song, px, move |t| {
        if let Some(w) = weak.upgrade() {
            if still.as_ref().is_none_or(|f| f()) {
                put(&w, t);
            }
        }
    });
}

fn put(target: &gtk::Widget, p: &impl IsA<gtk::gdk::Paintable>) {
    if let Some(i) = target.downcast_ref::<gtk::Image>() {
        i.set_paintable(Some(p));
    } else if let Some(pic) = target.downcast_ref::<gtk::Picture>() {
        pic.set_paintable(Some(p));
    }
}

fn placeholder(target: &gtk::Widget) {
    let theme = gtk::IconTheme::for_display(&target.display());
    let icon = theme.lookup_icon(
        "folder-music-symbolic",
        &[],
        48,
        target.scale_factor(),
        gtk::TextDirection::None,
        gtk::IconLookupFlags::empty(),
    );
    put(target, &icon);
}

/// "1 song", "12 songs"
pub fn count(n: usize) -> String {
    if n == 1 { "1 song".into() } else { format!("{n} songs") }
}

/// 3:05, or 1:02:03 past an hour.
pub fn clock(ms: i64) -> String {
    let s = ms.max(0) / 1000;
    if s >= 3600 {
        format!("{}:{:02}:{:02}", s / 3600, s / 60 % 60, s % 60)
    } else {
        format!("{}:{:02}", s / 60, s % 60)
    }
}

/// "12 songs · 48:10"
pub fn summary(songs: &[SongRef]) -> String {
    summary_of(songs.len(), songs.iter().map(|s| s.duration_ms).sum())
}

pub fn summary_of(n: usize, ms: i64) -> String {
    format!("{} · {}", count(n), clock(ms))
}

/// Pages get a filter only past this many rows: a shorter list fits on about one screen.
pub const FILTER_OVER: usize = 12;

/// A row of the lists of artists, folders, books: an image, a title and a line under it.
pub fn row(image: &gtk::Widget, title: &str, subtitle: &str) -> gtk::Box {
    let b = gtk::Box::builder().spacing(12).margin_top(6).margin_bottom(6).margin_start(6).margin_end(6).build();
    b.append(image);
    let text = gtk::Box::builder().orientation(gtk::Orientation::Vertical).valign(gtk::Align::Center).build();
    text.append(&gtk::Label::builder().label(title).xalign(0.0).ellipsize(gtk::pango::EllipsizeMode::End).build());
    text.append(
        &gtk::Label::builder()
            .label(subtitle)
            .xalign(0.0)
            .css_classes(["caption", "dim-label"])
            .ellipsize(gtk::pango::EllipsizeMode::End)
            .build(),
    );
    b.append(&text);
    b
}

/// A header over a list: cover, title and what the list holds.
pub fn header(app: &Rc<App>, song: Option<&SongRef>, title: &str, subtitle: &str) -> gtk::Box {
    let b = gtk::Box::builder().spacing(18).margin_top(18).margin_bottom(12).margin_start(18).margin_end(18).build();
    if song.is_some() {
        b.append(&cover(app, song, 140));
    }
    let text =
        gtk::Box::builder().orientation(gtk::Orientation::Vertical).valign(gtk::Align::Center).spacing(6).build();
    text.append(&gtk::Label::builder().label(title).xalign(0.0).wrap(true).css_classes(["title-2"]).build());
    text.append(&gtk::Label::builder().label(subtitle).xalign(0.0).wrap(true).css_classes(["dim-label"]).build());
    b.append(&text);
    b
}

/// A list of things in a `GtkListView`, which builds rows only for what is
/// on screen: the artists are thousands, the songs tens of thousands.
pub fn list<T: Clone + 'static>(
    app: &App,
    items: &[T],
    bind: impl Fn(&T) -> gtk::Widget + 'static,
    activate: impl Fn(&T) + 'static,
) -> gtk::ListView {
    let mut store = gio::ListStore::new::<glib::BoxedAnyObject>();
    store.extend(items.iter().map(|i| glib::BoxedAnyObject::new(i.clone())));
    let factory = gtk::SignalListItemFactory::new();
    factory.connect_bind(move |_, item| {
        let item = item.downcast_ref::<gtk::ListItem>().unwrap();
        let obj = item.item().and_downcast::<glib::BoxedAnyObject>().unwrap();
        item.set_child(Some(&bind(&obj.borrow::<T>())));
    });
    let view = gtk::ListView::new(Some(gtk::NoSelection::new(Some(store.clone()))), Some(factory));
    view.set_single_click_activate(true);
    view.add_css_class("navigation-sidebar");
    rebind_on_thumbs(app, &store);
    view.connect_activate(move |_, pos| {
        if let Some(obj) = store.item(pos).and_downcast::<glib::BoxedAnyObject>() {
            activate(&obj.borrow::<T>());
        }
    });
    view
}

/// Binds every row of `store` again when thumbnails arrive, while the list lives.
pub fn rebind_on_thumbs(app: &App, store: &gio::ListStore) {
    let weak = store.downgrade();
    app.listen(move |c| {
        let Some(store) = weak.upgrade() else { return false };
        if c == Change::Thumbs {
            let n = store.n_items();
            store.items_changed(0, n, n);
        }
        true
    });
}

pub fn scrolled(child: &impl IsA<gtk::Widget>) -> gtk::ScrolledWindow {
    gtk::ScrolledWindow::builder().child(child).vexpand(true).hscrollbar_policy(gtk::PolicyType::Never).build()
}

/// The columns of a song list. An album shows track numbers and leaves the
/// album out; elsewhere the album shows.
#[derive(Clone, Copy)]
pub struct Columns {
    pub track: bool,
    pub album: bool,
}

/// What activating a song in a list does.
#[derive(Clone)]
pub enum Play {
    /// Plays the list as a queue named after it (AGENTS.md: never into the current queue).
    List { name: String, source: String },
    /// The list is queue `id`: plays it from that song.
    Queue(String),
}

/// Songs in a `GtkColumnView`: title, artist, album, time. Rows are built
/// only for what is on screen. Activating a row plays; its menu queues.
pub fn song_list(app: &Rc<App>, songs: &[SongRef], cols: Columns, play: Play) -> gtk::ColumnView {
    let mut store = gio::ListStore::new::<glib::BoxedAnyObject>();
    store.extend(songs.iter().map(|s| glib::BoxedAnyObject::new(s.clone())));
    let sel = gtk::MultiSelection::new(Some(store.clone()));
    let view = gtk::ColumnView::new(Some(sel.clone()));
    view.set_show_column_separators(false);
    view.set_reorderable(false);
    view.add_css_class("data-table");
    let menu = gio::Menu::new();
    menu.append(Some("Play Next"), Some("songs.play-next"));
    menu.append(Some("Add to Queue"), Some("songs.add-to-queue"));
    if matches!(play, Play::Queue(_)) {
        menu.append(Some("Remove from Queue"), Some("songs.remove"));
    }
    // A right-click selects the row under it, unless it is in the selection already, and opens the menu.
    let menu_on = {
        let (sel, menu) = (sel.clone(), menu.clone());
        move |cell: &gtk::Widget, item: &gtk::ListItem| {
            let g = gtk::GestureClick::builder().button(3).build();
            let (sel, menu, item, cell2) = (sel.clone(), menu.clone(), item.downgrade(), cell.downgrade());
            g.connect_pressed(move |_, _, x, y| {
                let (Some(item), Some(cell)) = (item.upgrade(), cell2.upgrade()) else { return };
                let pos = item.position();
                if !sel.is_selected(pos) {
                    sel.select_item(pos, true);
                }
                let pop = gtk::PopoverMenu::from_model(Some(&menu));
                pop.set_parent(&cell);
                pop.set_has_arrow(false);
                pop.set_pointing_to(Some(&gtk::gdk::Rectangle::new(x as i32, y as i32, 1, 1)));
                pop.connect_closed(|p| {
                    let p = p.clone();
                    glib::idle_add_local_once(move || p.unparent());
                });
                pop.popup();
            });
            cell.add_controller(g);
        }
    };
    let column = |title: &str, expand: bool, text: Box<dyn Fn(&SongRef) -> String>, dim: bool, numeric: bool| {
        let factory = gtk::SignalListItemFactory::new();
        let menu_on = menu_on.clone();
        factory.connect_setup(move |_, item| {
            let item = item.downcast_ref::<gtk::ListItem>().unwrap();
            let label = gtk::Label::builder()
                .xalign(if numeric { 1.0 } else { 0.0 })
                .ellipsize(gtk::pango::EllipsizeMode::End)
                .build();
            if dim {
                label.add_css_class("dim-label");
            }
            if numeric {
                label.add_css_class("numeric");
            }
            menu_on(label.upcast_ref(), item);
            item.set_child(Some(&label));
        });
        factory.connect_bind(move |_, item| {
            let item = item.downcast_ref::<gtk::ListItem>().unwrap();
            let obj = item.item().and_downcast::<glib::BoxedAnyObject>().unwrap();
            let label = item.child().and_downcast::<gtk::Label>().unwrap();
            label.set_label(&text(&obj.borrow::<SongRef>()));
        });
        let c = gtk::ColumnViewColumn::new(Some(title), Some(factory));
        c.set_expand(expand);
        c.set_resizable(true);
        c
    };
    if cols.track {
        view.append_column(&column(
            "#",
            false,
            Box::new(|s| if s.track > 0 { s.track.to_string() } else { String::new() }),
            true,
            true,
        ));
    }
    // The cover thumbnail with the title, as the Mac's table shows it.
    let factory = gtk::SignalListItemFactory::new();
    let m = menu_on.clone();
    factory.connect_setup(move |_, item| {
        let item = item.downcast_ref::<gtk::ListItem>().unwrap();
        let b = gtk::Box::builder().spacing(10).build();
        let pic =
            gtk::Picture::builder().content_fit(gtk::ContentFit::Cover).width_request(28).height_request(28).build();
        let frame = gtk::Frame::builder().child(&pic).css_classes(["cover"]).build();
        b.append(&frame);
        b.append(&gtk::Label::builder().xalign(0.0).ellipsize(gtk::pango::EllipsizeMode::End).build());
        m(b.upcast_ref(), item);
        item.set_child(Some(&b));
    });
    let a = app.clone();
    factory.connect_bind(move |_, item| {
        let item = item.downcast_ref::<gtk::ListItem>().unwrap();
        let obj = item.item().and_downcast::<glib::BoxedAnyObject>().unwrap();
        let song = obj.borrow::<SongRef>().clone();
        let b = item.child().and_downcast::<gtk::Box>().unwrap();
        let pic = b.first_child().and_downcast::<gtk::Frame>().unwrap().child().and_downcast::<gtk::Picture>().unwrap();
        set_cover(&a, &pic, Some(&song), 28, None);
        b.last_child().and_downcast::<gtk::Label>().unwrap().set_label(&song.title);
    });
    let title = gtk::ColumnViewColumn::new(Some("Title"), Some(factory));
    title.set_expand(true);
    title.set_resizable(true);
    view.append_column(&title);
    view.append_column(&column("Artist", true, Box::new(|s| s.display_artist().to_string()), false, false));
    if cols.album {
        view.append_column(&column("Album", true, Box::new(|s| s.album.clone()), true, false));
    }
    view.append_column(&column("Time", false, Box::new(|s| clock(s.duration_ms)), true, true));
    // New thumbnails: rows showing placeholders are bound again.
    rebind_on_thumbs(app, &store);

    let list: Rc<Vec<SongRef>> = Rc::new(songs.to_vec());
    let (a, l, p) = (app.clone(), list.clone(), play.clone());
    view.connect_activate(move |_, pos| {
        let pb = a.playback();
        match &p {
            Play::List { name, source } => pb.play_list(name, source, &l, pos as usize),
            Play::Queue(id) => pb.play_queue_at(id, l[pos as usize].id),
        }
    });
    let selected = {
        let (sel, list) = (sel.clone(), list.clone());
        move || -> Vec<SongRef> {
            (0..list.len() as u32).filter(|&i| sel.is_selected(i)).map(|i| list[i as usize].clone()).collect()
        }
    };
    let group = gio::SimpleActionGroup::new();
    let action = |name: &str, f: Box<dyn Fn(Vec<SongRef>)>| {
        let a = gio::SimpleAction::new(name, None);
        let selected = selected.clone();
        a.connect_activate(move |_, _| f(selected()));
        group.add_action(&a);
    };
    let a = app.clone();
    action("play-next", Box::new(move |s| a.playback().play_next(&s)));
    let a = app.clone();
    action("add-to-queue", Box::new(move |s| a.playback().add_to_queue(&s)));
    if let Play::Queue(id) = play {
        let a = app.clone();
        action("remove", Box::new(move |s| a.playback().remove_from(&id, &s.iter().map(|s| s.id).collect())));
    }
    view.insert_action_group("songs", Some(&group));
    view
}

/// A song list that filters itself when it is long (plan 011), as search does (plan 029).
pub fn songs_page(app: &Rc<App>, top: Option<gtk::Widget>, songs: Vec<SongRef>, cols: Columns, play: Play) -> gtk::Box {
    let b = gtk::Box::builder().orientation(gtk::Orientation::Vertical).build();
    if let Some(top) = top {
        b.append(&top);
    }
    let holder = adw::Bin::new();
    holder.set_vexpand(true);
    holder.set_child(Some(&scrolled(&song_list(app, &songs, cols, play.clone()))));
    if songs.len() > FILTER_OVER {
        let entry = gtk::SearchEntry::builder()
            .placeholder_text("Filter songs")
            .margin_start(18)
            .margin_end(18)
            .margin_bottom(6)
            .build();
        let (app, h) = (app.clone(), holder.clone());
        entry.connect_search_changed(move |e| {
            let shown = app.catalog().filter(&songs, &e.text());
            h.set_child(Some(&scrolled(&song_list(&app, &shown, cols, play.clone()))));
        });
        b.append(&entry);
    }
    b.append(&holder);
    b
}
