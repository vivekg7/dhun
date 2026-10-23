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

/// Songs in a `GtkColumnView`: title, artist, album, time. Rows are built
/// only for what is on screen.
pub fn song_list(app: &Rc<App>, songs: &[SongRef], cols: Columns) -> gtk::ColumnView {
    let mut store = gio::ListStore::new::<glib::BoxedAnyObject>();
    store.extend(songs.iter().map(|s| glib::BoxedAnyObject::new(s.clone())));
    let view = gtk::ColumnView::new(Some(gtk::MultiSelection::new(Some(store.clone()))));
    view.set_show_column_separators(false);
    view.set_reorderable(false);
    view.add_css_class("data-table");
    let column = |title: &str, expand: bool, text: Box<dyn Fn(&SongRef) -> String>, dim: bool, numeric: bool| {
        let factory = gtk::SignalListItemFactory::new();
        factory.connect_setup(move |_, item| {
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
            item.downcast_ref::<gtk::ListItem>().unwrap().set_child(Some(&label));
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
    factory.connect_setup(|_, item| {
        let b = gtk::Box::builder().spacing(10).build();
        let pic =
            gtk::Picture::builder().content_fit(gtk::ContentFit::Cover).width_request(28).height_request(28).build();
        let frame = gtk::Frame::builder().child(&pic).css_classes(["cover"]).build();
        b.append(&frame);
        b.append(&gtk::Label::builder().xalign(0.0).ellipsize(gtk::pango::EllipsizeMode::End).build());
        item.downcast_ref::<gtk::ListItem>().unwrap().set_child(Some(&b));
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
    view
}

/// A song list that filters itself when it is long (plan 011), as search does (plan 029).
pub fn songs_page(app: &Rc<App>, top: Option<gtk::Widget>, songs: Vec<SongRef>, cols: Columns) -> gtk::Box {
    let b = gtk::Box::builder().orientation(gtk::Orientation::Vertical).build();
    if let Some(top) = top {
        b.append(&top);
    }
    let holder = adw::Bin::new();
    holder.set_vexpand(true);
    holder.set_child(Some(&scrolled(&song_list(app, &songs, cols))));
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
            h.set_child(Some(&scrolled(&song_list(&app, &shown, cols))));
        });
        b.append(&entry);
    }
    b.append(&holder);
    b
}
