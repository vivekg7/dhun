//! Pieces every page uses: covers, rows, the song list, the filter box.

use crate::app::{App, Change};
use adw::prelude::*;
use dhun_core::catalog::SongRef;
use dhun_core::downloads as dl;
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

/// How a song that cannot play offline shows.
const UNPLAYABLE: f64 = 0.45;

/// Dims a song list's cell when its song cannot play now, and marks the
/// title of one kept in Downloads.
fn show_kept(app: &App, item: &gtk::ListItem) {
    let (Some(obj), Some(cell)) = (item.item().and_downcast::<glib::BoxedAnyObject>(), item.child()) else { return };
    let id = obj.borrow::<SongRef>().id;
    cell.set_opacity(if app.playable(id) { 1.0 } else { UNPLAYABLE });
    // The title cell: cover, title, mark, and any menu opened on it after these.
    if let Some(mark) = cell.first_child().and_then(|c| c.next_sibling()).and_then(|t| t.next_sibling()) {
        mark.set_visible(app.downloaded.borrow().contains_key(&id));
    }
}

/// Keeps a cell's `show_kept` current while it lives: a row already on
/// screen is not bound again when only this changes.
fn follow_kept(app: &Rc<App>, item: &gtk::ListItem) {
    let (a, w) = (app.clone(), item.downgrade());
    app.listen(move |c| {
        let Some(item) = w.upgrade() else { return false };
        if c == Change::Playable {
            show_kept(&a, &item);
        }
        true
    });
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

/// A header over a list: cover, title, what the list holds, and its
/// Download button when `pin` names it as a kind and a ref (plan 012).
pub fn header(
    app: &Rc<App>,
    song: Option<&SongRef>,
    title: &str,
    subtitle: &str,
    pin: Option<(&str, String)>,
) -> gtk::Box {
    let b = gtk::Box::builder().spacing(18).margin_top(18).margin_bottom(12).margin_start(18).margin_end(18).build();
    if song.is_some() {
        b.append(&cover(app, song, 140));
    }
    let text =
        gtk::Box::builder().orientation(gtk::Orientation::Vertical).valign(gtk::Align::Center).spacing(6).build();
    text.append(&gtk::Label::builder().label(title).xalign(0.0).wrap(true).css_classes(["title-2"]).build());
    text.append(&gtk::Label::builder().label(subtitle).xalign(0.0).wrap(true).css_classes(["dim-label"]).build());
    if let Some((kind, r#ref)) = pin {
        text.append(&pin_button(app, kind, r#ref, title));
    }
    b.append(&text);
    b
}

/// Downloads a whole album, folder, artist, genre, playlist or shelf; once
/// downloaded it says how far along it is, and a click asks to remove it.
pub fn pin_button(app: &Rc<App>, kind: &str, r#ref: String, name: &str) -> gtk::Widget {
    let button = gtk::Button::builder().halign(gtk::Align::Start).css_classes(["pill"]).margin_top(6).build();
    let show = {
        let (app, kind, r#ref, b) = (app.clone(), kind.to_string(), r#ref.clone(), button.downgrade());
        move || {
            let Some(b) = b.upgrade() else { return false };
            let d = app.downloads();
            let (icon, label) = if d.pinned(&kind, &r#ref) {
                let songs = d.songs(&kind, &r#ref);
                let have = app.downloaded.borrow();
                let done = songs.iter().filter(|s| have.contains_key(&s.id)).count();
                if done < songs.len() {
                    ("folder-download-symbolic", format!("{done}/{}", songs.len()))
                } else {
                    ("object-select-symbolic", "Downloaded".to_string())
                }
            } else {
                ("folder-download-symbolic", "Download".to_string())
            };
            b.set_child(Some(&adw::ButtonContent::builder().icon_name(icon).label(label).build()));
            b.set_tooltip_text(Some(if d.pinned(&kind, &r#ref) {
                "Remove the download"
            } else {
                "Keep on this computer"
            }));
            true
        }
    };
    show();
    app.listen(move |c| if matches!(c, Change::Downloads | Change::Playable) { show() } else { true });
    let (app, kind, name) = (app.clone(), kind.to_string(), name.to_string());
    button.connect_clicked(move |b| {
        let d = app.downloads().clone();
        if d.pinned(&kind, &r#ref) {
            let key = dhun_core::downloads::key(&kind, &r#ref);
            confirm_unpin(b.upcast_ref(), &name, move || d.unpin(&key));
        } else {
            d.pin(&kind, &r#ref, &name);
        }
    });
    button.upcast()
}

/// Asks before a download goes, as the phone does.
pub fn confirm_unpin(parent: &gtk::Widget, name: &str, remove: impl Fn() + 'static) {
    let dialog = adw::AlertDialog::builder()
        .heading("Remove Download?")
        .body(format!(
            "“{name}” stays in the library. Its songs are deleted from this computer, unless something else you downloaded has them too."
        ))
        .build();
    dialog.add_responses(&[("cancel", "Cancel"), ("remove", "Remove")]);
    dialog.set_response_appearance("remove", adw::ResponseAppearance::Destructive);
    dialog.connect_response(Some("remove"), move |_, _| remove());
    dialog.present(Some(parent));
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
    let list: Rc<Vec<SongRef>> = Rc::new(songs.to_vec());
    let selected = {
        let (sel, list) = (sel.clone(), list.clone());
        move || -> Vec<SongRef> {
            (0..list.len() as u32).filter(|&i| sel.is_selected(i)).map(|i| list[i as usize].clone()).collect()
        }
    };
    let menu = gio::Menu::new();
    menu.append(Some("Play Next"), Some("songs.play-next"));
    menu.append(Some("Add to Queue"), Some("songs.add-to-queue"));
    if matches!(play, Play::Queue(_)) {
        menu.append(Some("Remove from Queue"), Some("songs.remove"));
    }
    // Download or Remove Download, as the songs picked are (plan 012).
    let keep = gio::Menu::new();
    menu.append_section(None, &keep);
    // A right-click selects the row under it, unless it is in the selection already, and opens the menu.
    let menu_on = {
        let (sel, menu, app, selected) = (sel.clone(), menu.clone(), app.clone(), selected.clone());
        move |cell: &gtk::Widget, item: &gtk::ListItem| {
            let g = gtk::GestureClick::builder().button(3).build();
            let (sel, menu, item, cell2) = (sel.clone(), menu.clone(), item.downgrade(), cell.downgrade());
            let (app, keep, selected) = (app.clone(), keep.clone(), selected.clone());
            g.connect_pressed(move |_, _, x, y| {
                let (Some(item), Some(cell)) = (item.upgrade(), cell2.upgrade()) else { return };
                let pos = item.position();
                if !sel.is_selected(pos) {
                    sel.select_item(pos, true);
                }
                let picked = selected();
                let d = app.downloads();
                keep.remove_all();
                if matches!(picked.as_slice(), [s] if d.pinned(dl::SONG, &s.id.to_string())) {
                    keep.append(Some("Remove Download"), Some("songs.unpin"));
                } else if picked.iter().any(|s| !app.downloaded.borrow().contains_key(&s.id)) {
                    keep.append(Some("Download"), Some("songs.download"));
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
        let a = app.clone();
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
            follow_kept(&a, item);
        });
        let a = app.clone();
        factory.connect_bind(move |_, item| {
            let item = item.downcast_ref::<gtk::ListItem>().unwrap();
            let obj = item.item().and_downcast::<glib::BoxedAnyObject>().unwrap();
            let label = item.child().and_downcast::<gtk::Label>().unwrap();
            label.set_label(&text(&obj.borrow::<SongRef>()));
            show_kept(&a, item);
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
    let (m, a) = (menu_on.clone(), app.clone());
    factory.connect_setup(move |_, item| {
        let item = item.downcast_ref::<gtk::ListItem>().unwrap();
        let b = gtk::Box::builder().spacing(10).build();
        let pic =
            gtk::Picture::builder().content_fit(gtk::ContentFit::Cover).width_request(28).height_request(28).build();
        let frame = gtk::Frame::builder().child(&pic).css_classes(["cover"]).build();
        b.append(&frame);
        b.append(&gtk::Label::builder().xalign(0.0).ellipsize(gtk::pango::EllipsizeMode::End).build());
        let kept = gtk::Image::builder().icon_name("folder-download-symbolic").pixel_size(12).build();
        kept.add_css_class("dim-label");
        kept.set_tooltip_text(Some("Downloaded"));
        b.append(&kept);
        m(b.upcast_ref(), item);
        item.set_child(Some(&b));
        follow_kept(&a, item);
    });
    let a = app.clone();
    factory.connect_bind(move |_, item| {
        let item = item.downcast_ref::<gtk::ListItem>().unwrap();
        let obj = item.item().and_downcast::<glib::BoxedAnyObject>().unwrap();
        let song = obj.borrow::<SongRef>().clone();
        let b = item.child().and_downcast::<gtk::Box>().unwrap();
        let pic = b.first_child().and_downcast::<gtk::Frame>().unwrap().child().and_downcast::<gtk::Picture>().unwrap();
        set_cover(&a, &pic, Some(&song), 28, None);
        // By place from the cover: a menu opened on the cell is a child of it too, after these.
        let label = pic.parent().unwrap().next_sibling().and_downcast::<gtk::Label>().unwrap();
        label.set_label(&song.title);
        show_kept(&a, item);
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

    let (a, l, p) = (app.clone(), list.clone(), play.clone());
    view.connect_activate(move |_, pos| {
        let pb = a.playback();
        match &p {
            Play::List { name, source } => pb.play_list(name, source, &l, pos as usize),
            Play::Queue(id) => pb.play_queue_at(id, l[pos as usize].id),
        }
    });
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
    let a = app.clone();
    action(
        "download",
        Box::new(move |songs| {
            for s in songs {
                a.downloads().pin(dl::SONG, &s.id.to_string(), &s.title);
            }
        }),
    );
    let (a, v) = (app.clone(), view.downgrade());
    action(
        "unpin",
        Box::new(move |songs| {
            let (Some(s), Some(v)) = (songs.first(), v.upgrade()) else { return };
            let (d, key) = (a.downloads().clone(), dl::key(dl::SONG, &s.id.to_string()));
            confirm_unpin(v.upcast_ref(), &s.title, move || d.unpin(&key));
        }),
    );
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
