//! Pieces every page uses: covers, rows, the song list, the filter box.

use crate::app::{App, Change};
use adw::prelude::*;
use dhun_core::catalog::SongRef;
use dhun_core::downloads as dl;
use dhun_core::model::Playlist;
use dhun_core::store;
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
        None => {
            placeholder(&target);
            // The thumbnails are still on their way: it is shown when they land.
            let (a, s, weak, still) = (app.clone(), song.clone(), target.downgrade(), still.clone());
            // A recycled row that moved on stops waiting at the next change.
            app.listen(move |c| {
                let Some(w) = weak.upgrade().filter(|_| still.as_ref().is_none_or(|f| f())) else { return false };
                if c != Change::Thumbs {
                    return true;
                }
                if let Some(t) = a.thumbnail(&s) {
                    put(&w, &t);
                }
                false
            });
        }
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
    // Play and Shuffle, Download, a page's own: `buttons` finds the row.
    let row = gtk::Box::builder().spacing(8).margin_top(6).name("buttons").build();
    if let Some((kind, r#ref)) = pin {
        row.append(&pin_button(app, kind, r#ref, title));
    }
    text.append(&row);
    b.append(&text);
    b
}

/// The row of buttons under a header's title, anywhere in `top`.
pub fn buttons(top: &impl IsA<gtk::Widget>) -> Option<gtk::Box> {
    let mut todo = vec![top.clone().upcast::<gtk::Widget>()];
    while let Some(w) = todo.pop() {
        if w.widget_name() == "buttons" {
            return w.downcast().ok();
        }
        let mut c = w.first_child();
        while let Some(x) = c {
            c = x.next_sibling();
            todo.push(x);
        }
    }
    None
}

/// Downloads a whole album, folder, artist, genre, playlist or shelf; once
/// downloaded it says how far along it is, and a click asks to remove it.
pub fn pin_button(app: &Rc<App>, kind: &str, r#ref: String, name: &str) -> gtk::Widget {
    let button = gtk::Button::builder().halign(gtk::Align::Start).css_classes(["pill"]).build();
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
    let body = format!(
        "“{name}” stays in the library. Its songs are deleted from this computer, unless something else you downloaded has them too."
    );
    confirm(parent, "Remove Download?", &body, "Remove", remove);
}

/// Asks before something goes; `action` names the red button.
pub fn confirm(parent: &impl IsA<gtk::Widget>, heading: &str, body: &str, action: &str, done: impl Fn() + 'static) {
    let dialog = adw::AlertDialog::builder().heading(heading).body(body).build();
    dialog.add_responses(&[("cancel", "Cancel"), ("go", action)]);
    dialog.set_response_appearance("go", adw::ResponseAppearance::Destructive);
    dialog.connect_response(Some("go"), move |_, _| done());
    dialog.present(Some(parent));
}

/// Asks for a name: a new queue or playlist, or a new name for one. The
/// two lists' names cannot name a queue or a playlist, as on the phone.
pub fn ask_name(
    parent: &impl IsA<gtk::Widget>,
    heading: &str,
    initial: &str,
    action: &str,
    done: impl Fn(String) + 'static,
) {
    let entry = gtk::Entry::builder().text(initial).build();
    let dialog = adw::AlertDialog::builder().heading(heading).extra_child(&entry).build();
    dialog.add_responses(&[("cancel", "Cancel"), ("ok", action)]);
    dialog.set_default_response(Some("ok"));
    dialog.set_close_response("cancel");
    dialog.set_response_appearance("ok", adw::ResponseAppearance::Suggested);
    dialog.set_response_enabled("ok", !initial.trim().is_empty());
    let d = dialog.downgrade();
    entry.connect_changed(move |e| {
        let Some(d) = d.upgrade() else { return };
        let name = e.text().trim().to_lowercase();
        let reserved = ["favorites", "listen later"].contains(&name.as_str());
        d.set_body(&if reserved {
            format!("“{}” is the name of one of your lists.", e.text().trim())
        } else {
            String::new()
        });
        d.set_response_enabled("ok", !name.is_empty() && !reserved);
    });
    // Return names it: the field's own, since a field in a dialog does not reach the default button.
    let d = dialog.downgrade();
    entry.connect_activate(move |_| {
        if let Some(d) = d.upgrade().filter(|d| d.is_response_enabled("ok")) {
            d.emit_by_name_with_details::<()>("response", glib::Quark::from_str("ok"), &[&"ok"]);
            d.force_close();
        }
    });
    dialog.connect_response(Some("ok"), move |_, _| done(entry.text().trim().to_string()));
    dialog.present(Some(parent));
}

/// Asks for a line of text, empty allowed, at most `max` characters: a bookmark's note.
pub fn ask_text(
    parent: &impl IsA<gtk::Widget>,
    heading: &str,
    initial: &str,
    action: &str,
    max: usize,
    done: impl Fn(String) + 'static,
) {
    let entry =
        gtk::Entry::builder().text(initial).max_length(max as i32).placeholder_text("What happens here").build();
    let dialog = adw::AlertDialog::builder().heading(heading).extra_child(&entry).build();
    dialog.add_responses(&[("cancel", "Cancel"), ("ok", action)]);
    dialog.set_close_response("cancel");
    dialog.set_response_appearance("ok", adw::ResponseAppearance::Suggested);
    let d = dialog.downgrade();
    entry.connect_activate(move |_| {
        if let Some(d) = d.upgrade() {
            d.emit_by_name_with_details::<()>("response", glib::Quark::from_str("ok"), &[&"ok"]);
            d.force_close();
        }
    });
    dialog.connect_response(Some("ok"), move |_, _| done(entry.text().trim().to_string()));
    dialog.present(Some(parent));
}

/// A list of things in a `GtkListView`, which builds rows only for what is
/// on screen: the artists are thousands, the songs tens of thousands.
pub fn list<T: Clone + 'static>(
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
    view.connect_activate(move |_, pos| {
        if let Some(obj) = store.item(pos).and_downcast::<glib::BoxedAnyObject>() {
            activate(&obj.borrow::<T>());
        }
    });
    view
}

pub fn scrolled(child: &impl IsA<gtk::Widget>) -> gtk::ScrolledWindow {
    gtk::ScrolledWindow::builder().child(child).vexpand(true).hscrollbar_policy(gtk::PolicyType::Never).build()
}

/// The columns of a song list. An album shows track numbers and leaves the
/// album out; elsewhere the album shows. A book's or a show's files have
/// their own (plan 031).
#[derive(Clone, Default)]
pub struct Columns {
    pub track: bool,
    pub album: bool,
    pub shelf: Option<Rc<ShelfColumns>>,
}

/// A show's dates, or a book's parts ("2 of 39"); and how much of each was heard.
pub struct ShelfColumns {
    pub date: bool,
    pub part: std::collections::HashMap<i64, String>,
    pub heard: dhun_core::spoken::Heard,
}

/// What a song list's menu item does with the songs picked and its target.
type SongAction = Box<dyn Fn(Vec<SongRef>, Option<&glib::Variant>)>;

/// What activating a song in a list does.
#[derive(Clone)]
pub enum Play {
    /// Plays the list as a queue named after it (AGENTS.md: never into the current queue).
    List { name: String, source: String },
    /// The list is queue `id`: plays it from that song.
    Queue(String),
    /// A book's files or a show's episodes, by the shelf's id (plan 031).
    Shelf(String),
}

/// What a list's menu can take out of it, and whether its rows are dragged
/// into another order. A queue's are, from `Play::Queue`.
#[derive(Clone, Default)]
pub enum Edit {
    #[default]
    None,
    /// Favorites or Listen Later, by its mark and its name.
    Mark(&'static str, &'static str),
    /// A playlist of the user's own; each row's entry in it, by place, since
    /// a song can be in it twice.
    Playlist { id: i64, entries: Rc<Vec<usize>> },
}

/// Songs in a `GtkColumnView`: title, artist, album, time. Rows are built
/// only for what is on screen. Activating a row plays; its menu is the
/// phone's song menu (plan 011).
pub fn song_list(app: &Rc<App>, songs: &[SongRef], cols: Columns, play: Play, edit: Edit) -> gtk::ColumnView {
    let mut store = gio::ListStore::new::<glib::BoxedAnyObject>();
    store.extend(songs.iter().map(|s| glib::BoxedAnyObject::new(s.clone())));
    let sel = gtk::MultiSelection::new(Some(store.clone()));
    let view = gtk::ColumnView::new(Some(sel.clone()));
    view.set_show_column_separators(false);
    view.set_reorderable(false);
    view.add_css_class("data-table");
    let list: Rc<Vec<SongRef>> = Rc::new(songs.to_vec());
    let rows = {
        let (sel, n) = (sel.clone(), list.len() as u32);
        move || -> Vec<usize> { (0..n).filter(|&i| sel.is_selected(i)).map(|i| i as usize).collect() }
    };
    let selected = {
        let (rows, list) = (rows.clone(), list.clone());
        move || -> Vec<SongRef> { rows().into_iter().map(|i| list[i].clone()).collect() }
    };
    // Built again at each right-click, for the songs picked.
    let menu = gio::Menu::new();
    // A right-click selects the row under it, unless it is in the selection already, and opens the menu.
    let menu_on = {
        let (sel, menu, app, selected, play, edit) =
            (sel.clone(), menu.clone(), app.clone(), selected.clone(), play.clone(), edit.clone());
        move |cell: &gtk::Widget, item: &gtk::ListItem| {
            let g = gtk::GestureClick::builder().button(3).build();
            let (sel, menu, item, cell2) = (sel.clone(), menu.clone(), item.downgrade(), cell.downgrade());
            let (app, selected, play, edit) = (app.clone(), selected.clone(), play.clone(), edit.clone());
            g.connect_pressed(move |_, _, x, y| {
                let (Some(item), Some(cell)) = (item.upgrade(), cell2.upgrade()) else { return };
                let pos = item.position();
                if !sel.is_selected(pos) {
                    sel.select_item(pos, true);
                }
                song_menu(&app, &menu, &selected(), &play, &edit);
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
    // A queue's rows and a playlist's are dragged into another order: row `from` goes where row `to` is.
    let moving: Option<Rc<dyn Fn(usize, usize)>> = match (&play, &edit) {
        (Play::Queue(id), _) => {
            let (a, id, l) = (app.clone(), id.clone(), list.clone());
            Some(Rc::new(move |from, to| a.playback().move_in(&id, l[from].id, l[to].id)))
        }
        (_, Edit::Playlist { id, entries }) => {
            let (a, id, entries) = (app.clone(), *id, entries.clone());
            Some(Rc::new(move |from, to| {
                let Some(p) = a.playlists.borrow().iter().find(|p| p.id == id).cloned() else { return };
                a.store(|db| store::move_in_playlist(db, &p, entries[from], entries[to]));
                a.reload_playlists();
            }))
        }
        _ => None,
    };
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
        if let Some(mv) = &moving {
            let it = item.downgrade();
            drag_rows(&b, move || it.upgrade().map(|i| i.position() as usize), mv.clone());
        }
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
        // A recycled row may show another song when the cover arrives.
        let weak = item.downgrade();
        let still: Rc<dyn Fn() -> bool> = Rc::new(move || {
            weak.upgrade().and_then(|i| i.item()).is_some_and(|o| o == obj.clone().upcast::<glib::Object>())
        });
        set_cover(&a, &pic, Some(&song), 28, Some(still));
        // By place from the cover: a menu opened on the cell is a child of it too, after these.
        let label = pic.parent().unwrap().next_sibling().and_downcast::<gtk::Label>().unwrap();
        label.set_label(&song.title);
        show_kept(&a, item);
    });
    let title = gtk::ColumnViewColumn::new(Some("Title"), Some(factory));
    title.set_expand(true);
    title.set_resizable(true);
    view.append_column(&title);
    match cols.shelf.clone() {
        None => view.append_column(&column("Artist", true, Box::new(|s| s.display_artist().to_string()), false, false)),
        Some(sc) => {
            if sc.date {
                view.append_column(&column("Date", false, Box::new(|s| s.day()), true, true));
            }
            if !sc.part.is_empty() {
                let x = sc.clone();
                let part = Box::new(move |s: &SongRef| x.part.get(&s.id).cloned().unwrap_or_default());
                view.append_column(&column("Part", false, part, true, true));
            }
            view.append_column(&column("Heard", false, Box::new(move |s| sc.heard.state(s)), true, false));
        }
    }
    if cols.album {
        view.append_column(&column("Album", true, Box::new(|s| s.album.clone()), true, false));
    }
    view.append_column(&column("Time", false, Box::new(|s| clock(s.duration_ms)), true, true));

    let (a, l, p) = (app.clone(), list.clone(), play.clone());
    view.connect_activate(move |_, pos| {
        let pb = a.playback();
        match &p {
            Play::List { name, source } => pb.play_list(name, source, &l, pos as usize),
            Play::Queue(id) => pb.play_queue_at(id, l[pos as usize].id),
            Play::Shelf(id) => {
                if let Some(shelf) = a.catalog().shelves.get(id).cloned() {
                    crate::pages::play_shelf(&a, &shelf, Some(&l[pos as usize]));
                }
            }
        }
    });
    let group = gio::SimpleActionGroup::new();
    let action = |name: &str, arg: Option<&glib::VariantTy>, f: SongAction| {
        let a = gio::SimpleAction::new(name, arg);
        let selected = selected.clone();
        a.connect_activate(move |_, v| f(selected(), v));
        group.add_action(&a);
    };
    let a = app.clone();
    action("play-next", None, Box::new(move |s, _| a.playback().play_next(&s)));
    let a = app.clone();
    action("add-to-queue", None, Box::new(move |s, _| a.playback().add_to_queue(&s)));
    let a = app.clone();
    action(
        "add-to",
        Some(glib::VariantTy::STRING),
        Box::new(move |s, v| a.playback().add_to(&v.and_then(|v| v.get::<String>()).unwrap_or_default(), &s)),
    );
    let (a, v) = (app.clone(), view.downgrade());
    action(
        "new-queue",
        None,
        Box::new(move |s, _| {
            let Some(v) = v.upgrade() else { return };
            let name = if let [one] = s.as_slice() { one.title.clone() } else { String::new() };
            let a = a.clone();
            ask_name(&v, "New Queue", &name, "Create", move |n| a.playback().new_queue(&n, &s));
        }),
    );
    let a = app.clone();
    action(
        "add-to-playlist",
        Some(glib::VariantTy::INT64),
        Box::new(move |s, v| {
            let found = a.playlists.borrow().iter().find(|p| Some(p.id) == v.and_then(|v| v.get())).cloned();
            if let Some(p) = found {
                add_to_playlist(&a, &p, &s);
            }
        }),
    );
    let (a, v) = (app.clone(), view.downgrade());
    action(
        "new-playlist",
        None,
        Box::new(move |s, _| {
            let Some(v) = v.upgrade() else { return };
            let a = a.clone();
            ask_name(&v, "New Playlist", "", "Create", move |n| {
                let ids = music(&s);
                a.store(|db| store::create_playlist(db, &n, &ids).map(|_| ()));
                a.reload_playlists();
            });
        }),
    );
    // "fav:1": into Favorites; "later:0": out of Listen Later.
    let a = app.clone();
    action(
        "mark",
        Some(glib::VariantTy::STRING),
        Box::new(move |s, v| {
            let t = v.and_then(|v| v.get::<String>()).unwrap_or_default();
            let Some((kind, on)) = t.split_once(':') else { return };
            let on = on == "1";
            let ids: Vec<i64> = s.iter().filter(|s| kind != store::PLAYED || s.spoken()).map(|s| s.id).collect();
            a.mark(kind, &ids, on);
            // Either way an episode starts from the beginning next time.
            if kind == store::PLAYED {
                a.store(|db| ids.iter().try_for_each(|&i| store::resume(db, i, None)));
            }
        }),
    );
    let a = app.clone();
    action(
        "download",
        None,
        Box::new(move |songs, _| {
            for s in songs {
                a.downloads().pin(dl::SONG, &s.id.to_string(), &s.title);
            }
        }),
    );
    let (a, v) = (app.clone(), view.downgrade());
    action(
        "unpin",
        None,
        Box::new(move |songs, _| {
            let (Some(s), Some(v)) = (songs.first(), v.upgrade()) else { return };
            let (d, key) = (a.downloads().clone(), dl::key(dl::SONG, &s.id.to_string()));
            confirm_unpin(v.upcast_ref(), &s.title, move || d.unpin(&key));
        }),
    );
    let a = app.clone();
    action(
        "stop-after",
        None,
        Box::new(move |s, _| {
            let (pb, Some(s)) = (a.playback(), s.first()) else { return };
            match pb.sleep() {
                Some(crate::playback::Sleep::AfterSong(x, _)) if x == s.id => pb.sleep_cancel(),
                _ => pb.sleep_after_song(s.id, &s.title),
            }
        }),
    );
    let (a, p, e, r) = (app.clone(), play.clone(), edit.clone(), rows.clone());
    action(
        "remove",
        None,
        Box::new(move |s, _| match (&p, &e) {
            (Play::Queue(id), _) => a.playback().remove_from(id, &s.iter().map(|s| s.id).collect()),
            (_, Edit::Mark(kind, _)) => a.mark(kind, &s.iter().map(|s| s.id).collect::<Vec<_>>(), false),
            (_, Edit::Playlist { id, entries }) => {
                let Some(mut p) = a.playlists.borrow().iter().find(|p| p.id == *id).cloned() else { return };
                // From the end, so the places still point at the right entries.
                let mut at: Vec<usize> = r().into_iter().map(|i| entries[i]).collect();
                at.sort_unstable_by(|x, y| y.cmp(x));
                a.store(|db| {
                    for i in at {
                        p = store::remove_from_playlist(db, &p, i)?;
                    }
                    Ok(())
                });
                a.reload_playlists();
            }
            _ => {}
        }),
    );
    view.insert_action_group("songs", Some(&group));
    view
}

/// A row dragged onto another: the row's place, which `at` tells, travels as text.
pub fn drag_rows(
    cell: &impl IsA<gtk::Widget>,
    at: impl Fn() -> Option<usize> + 'static,
    moved: Rc<dyn Fn(usize, usize)>,
) {
    let at = Rc::new(at);
    let source = gtk::DragSource::builder().actions(gtk::gdk::DragAction::MOVE).build();
    let a = at.clone();
    source.connect_prepare(move |_, _, _| {
        Some(gtk::gdk::ContentProvider::for_value(&format!("dhun-row:{}", a()?).to_value()))
    });
    cell.add_controller(source);
    let target = gtk::DropTarget::new(glib::Type::STRING, gtk::gdk::DragAction::MOVE);
    target.connect_drop(move |_, v, _, _| {
        let (Some(to), Ok(text)) = (at(), v.get::<String>()) else { return false };
        let Some(from) = text.strip_prefix("dhun-row:").and_then(|n| n.parse::<usize>().ok()) else { return false };
        // After the drop has finished: the list is built again under it.
        let moved = moved.clone();
        glib::idle_add_local_once(move || moved(from, to));
        true
    });
    cell.add_controller(target);
}

/// The phone's song menu for the songs picked (plan 011): play next, add to
/// the playing queue or another, add to a playlist, the two lists, played,
/// download, take out of this list, go to the album or artist.
fn song_menu(app: &App, menu: &gio::Menu, songs: &[SongRef], play: &Play, edit: &Edit) {
    menu.remove_all();
    let item = |m: &gio::Menu, label: &str, action: &str, target: Option<glib::Variant>| {
        let i = gio::MenuItem::new(Some(label), None);
        i.set_action_and_target_value(Some(action), target.as_ref());
        m.append_item(&i);
    };
    let first = gio::Menu::new();
    item(&first, "Play Next", "songs.play-next", None);
    item(&first, "Add to Playing Queue", "songs.add-to-queue", None);
    let queues = gio::Menu::new();
    let pb = app.playback();
    for q in pb.ordered() {
        item(&queues, &q.name, "songs.add-to", Some(q.id.to_variant()));
    }
    let new = gio::Menu::new();
    item(&new, "New Queue…", "songs.new-queue", None);
    queues.append_section(None, &new);
    first.append_submenu(Some("Add to Queue"), &queues);
    // Playlists hold music only (plan 031).
    if songs.iter().any(|s| !s.spoken()) {
        let lists = gio::Menu::new();
        for p in app.playlists.borrow().iter().filter(|p| !p.shared) {
            item(&lists, &p.name, "songs.add-to-playlist", Some(p.id.to_variant()));
        }
        let new = gio::Menu::new();
        item(&new, "New Playlist…", "songs.new-playlist", None);
        lists.append_section(None, &new);
        first.append_submenu(Some("Add to Playlist"), &lists);
    }
    menu.append_section(None, &first);

    let marks = gio::Menu::new();
    {
        let m = app.marks.borrow();
        let all = |set: &[i64]| songs.iter().all(|s| set.contains(&s.id));
        let (fav, later) = (all(&m.favs), all(&m.later));
        let toggle = |kind: &str, on: bool| Some(format!("{kind}:{}", if on { 0 } else { 1 }).to_variant());
        item(
            &marks,
            if fav { "Remove from Favorites" } else { "Add to Favorites" },
            "songs.mark",
            toggle(store::FAV, fav),
        );
        let label = if later { "Remove from Listen Later" } else { "Listen Later" };
        item(&marks, label, "songs.mark", toggle(store::LATER, later));
        let spoken: Vec<&SongRef> = songs.iter().filter(|s| s.spoken()).collect();
        if !spoken.is_empty() {
            let played = spoken.iter().all(|s| m.played.contains(&s.id));
            let label = if played { "Mark as Not Played" } else { "Mark as Played" };
            item(&marks, label, "songs.mark", toggle(store::PLAYED, played));
        }
    }
    menu.append_section(None, &marks);

    let this = gio::Menu::new();
    // Musicolet's "Stop after this song", for a song in the playing queue (plan 020).
    if let (Play::Queue(id), [s]) = (play, songs) {
        if *id == pb.active_id() {
            let on = matches!(pb.sleep(), Some(crate::playback::Sleep::AfterSong(x, _)) if x == s.id);
            item(
                &this,
                if on { "Don’t Stop After This Song" } else { "Stop After This Song" },
                "songs.stop-after",
                None,
            );
        }
    }
    match (play, edit) {
        (Play::Queue(_), _) => item(&this, "Remove from Queue", "songs.remove", None),
        (_, Edit::Mark(_, name)) => item(&this, &format!("Remove from {name}"), "songs.remove", None),
        (_, Edit::Playlist { .. }) => item(&this, "Remove from Playlist", "songs.remove", None),
        _ => {}
    }
    // Download or Remove Download, as the songs picked are (plan 012).
    let d = app.downloads();
    if matches!(songs, [s] if d.pinned(dl::SONG, &s.id.to_string())) {
        item(&this, "Remove Download", "songs.unpin", None);
    } else if songs.iter().any(|s| !app.downloaded.borrow().contains_key(&s.id)) {
        item(&this, "Download", "songs.download", None);
    }
    menu.append_section(None, &this);

    if let [s] = songs {
        let go = gio::Menu::new();
        let id = Some(s.id.to_variant());
        if let Some(shelf) = app.catalog().shelf_of(s) {
            item(&go, if shelf.is_book() { "Go to Book" } else { "Go to Show" }, "win.go-album", id);
        } else {
            if !s.album.is_empty() {
                item(&go, "Go to Album", "win.go-album", id.clone());
            }
            item(&go, "Go to Artist", "win.go-artist", id);
        }
        menu.append_section(None, &go);
    }
}

/// Playlists hold music only: each is an `.m3u8` in Music, which cannot
/// name a file in Podcasts or Audiobooks, and the server refuses them (plan 031).
fn music(songs: &[SongRef]) -> Vec<i64> {
    songs.iter().filter(|s| !s.spoken()).map(|s| s.id).collect()
}

/// Adds the songs and says what happened, as the phone does: songs already there are skipped.
fn add_to_playlist(app: &Rc<App>, p: &Playlist, songs: &[SongRef]) {
    let ids = music(songs);
    let mut skipped = 0;
    app.store(|db| store::add_to_playlist(db, p, &ids).map(|n| skipped = n));
    app.reload_playlists();
    let adding = dhun_core::queue::distinct(ids).len() - skipped;
    let spoken = songs.iter().filter(|s| s.spoken()).count();
    let name = &p.name;
    let mut text = match (adding, skipped) {
        (0, 1) => format!("Already in “{name}”"),
        (0, n) => format!("All {n} already in “{name}”"),
        (a, 0) => format!("Added {} to “{name}”", count(a)),
        (a, n) => format!("Added {a} to “{name}” · {n} already there"),
    };
    if spoken > 0 {
        text += &format!(" · {spoken} podcast or audiobook files left out");
    }
    app.playback().notice(&text);
}

/// Play and Shuffle on a page's header, which play `songs` as a new queue;
/// only when there is something to play, as on the phone.
pub fn play_buttons(app: &Rc<App>, top: &impl IsA<gtk::Widget>, name: &str, source: &str, songs: &[SongRef]) {
    let Some(row) = buttons(top).filter(|_| !songs.is_empty()) else { return };
    let all = gtk::Button::builder().label("Play").css_classes(["pill", "suggested-action"]).build();
    let shuffle = gtk::Button::builder()
        .icon_name("media-playlist-shuffle-symbolic")
        .tooltip_text("Shuffle")
        .css_classes(["circular"])
        .valign(gtk::Align::Center)
        .build();
    let (a, n, src, list) = (app.clone(), name.to_string(), source.to_string(), songs.to_vec());
    all.connect_clicked(move |_| a.playback().play_list(&n, &src, &list, 0));
    let (a, n, src, list) = (app.clone(), name.to_string(), source.to_string(), songs.to_vec());
    shuffle.connect_clicked(move |_| a.playback().play_list(&n, &src, &dhun_core::queue::sorted(0, &list), 0));
    row.prepend(&shuffle);
    row.prepend(&all);
}

/// A song list that filters itself when it is long (plan 011), as search does (plan 029).
pub fn songs_page(
    app: &Rc<App>,
    top: Option<gtk::Widget>,
    songs: Vec<SongRef>,
    cols: Columns,
    play: Play,
    edit: Edit,
) -> gtk::Box {
    let b = gtk::Box::builder().orientation(gtk::Orientation::Vertical).build();
    if let Some(top) = top {
        if let Play::List { name, source } = &play {
            play_buttons(app, &top, name, source, &songs);
        }
        b.append(&top);
    }
    let holder = adw::Bin::new();
    holder.set_vexpand(true);
    holder.set_child(Some(&scrolled(&song_list(app, &songs, cols.clone(), play.clone(), edit.clone()))));
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
            h.set_child(Some(&scrolled(&song_list(&app, &shown, cols.clone(), play.clone(), edit.clone()))));
        });
        b.append(&entry);
    }
    b.append(&holder);
    b
}
