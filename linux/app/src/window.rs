//! The window: sign-in, then the sidebar and the page stack, as the Mac's
//! `MainView` and `Sidebar`. Narrower than a sidebar and a page side by
//! side, it shows one at a time, as the phone does.

use crate::app::{App, Change};
use crate::pages::{Nav, Route};
use adw::prelude::*;
use dhun_core::prefs::server_url;
use gtk::{gio, glib};
use std::rc::Rc;
use std::time::Duration;

pub fn build(gapp: &adw::Application, app: &Rc<App>) -> adw::ApplicationWindow {
    let window = adw::ApplicationWindow::builder()
        .application(gapp)
        .title("Dhun")
        .name("main")
        .default_width(1100)
        .default_height(720)
        .build();
    let stack = gtk::Stack::builder().transition_type(gtk::StackTransitionType::Crossfade).build();
    let nav = Nav::new(app);
    let split = main_view(app, &nav);
    // The playing queue beside the page, and Now playing along the foot (plan 024's layout).
    let (side, pages) = crate::lyrics::side(app);
    let panel = adw::OverlaySplitView::builder()
        .sidebar_position(gtk::PackType::End)
        .show_sidebar(false)
        .content(&split)
        .sidebar(&side)
        .build();
    let main = adw::ToolbarView::builder().content(&panel).build();
    // A tighter Now playing bar below the first width; one pane at a time,
    // as on a phone, below the second. Only the last that matches applies.
    let medium = adw::Breakpoint::new(adw::BreakpointCondition::parse("max-width: 960sp").unwrap());
    let narrow = adw::Breakpoint::new(adw::BreakpointCondition::parse("max-width: 800sp").unwrap());
    narrow.add_setter(&split, "collapsed", Some(&true.to_value()));
    main.add_bottom_bar(&crate::nowplaying::bar(app, &panel, &pages, &medium, &narrow));
    stack.add_named(&main, Some("main"));
    stack.add_named(&sign_in(app), Some("sign-in"));
    let show = {
        let (stack, app) = (stack.clone(), app.clone());
        move || stack.set_visible_child_name(if app.signed_in() { "main" } else { "sign-in" })
    };
    show();
    app.listen(move |c| {
        if c == Change::Account {
            show();
        }
        true
    });
    // Notes for the user, as a queue removed to make room.
    let toasts = adw::ToastOverlay::new();
    toasts.set_child(Some(&stack));
    let t = toasts.downgrade();
    app.playback().notices.borrow_mut().push(Box::new(move |text| {
        if let Some(t) = t.upgrade() {
            t.add_toast(adw::Toast::new(text));
        }
    }));
    window.set_content(Some(&toasts));
    window.add_breakpoint(medium);
    window.add_breakpoint(narrow);
    // Ctrl+F searches.
    let find = gio::SimpleAction::new("find", None);
    let n = nav.clone();
    let s = split.clone();
    find.connect_activate(move |_, _| {
        s.set_show_content(false);
        n.search.grab_focus();
        n.show_recent();
    });
    window.add_action(&find);
    gapp.set_accels_for_action("win.find", &["<Control>f"]);
    let settings = gio::SimpleAction::new("settings", None);
    let (a, w) = (app.clone(), window.downgrade());
    settings.connect_activate(move |_, _| {
        if let Some(w) = w.upgrade() {
            crate::settings::open(&a, &w);
        }
    });
    window.add_action(&settings);
    gapp.set_accels_for_action("win.settings", &["<Control>comma"]);
    // A song's album, book or show, or its artist, from any song menu.
    for (name, artist) in [("go-album", false), ("go-artist", true)] {
        let go = gio::SimpleAction::new(name, Some(glib::VariantTy::INT64));
        let (a, n, s) = (app.clone(), nav.clone(), split.clone());
        go.connect_activate(move |_, v| {
            let c = a.catalog();
            let Some(song) = v.and_then(|v| v.get::<i64>()).and_then(|id| c.by_id.get(&id).cloned()) else { return };
            n.search.set_text("");
            if artist {
                n.open(Route::Artists);
                n.push(Route::Artist(song.artists.first().cloned().unwrap_or_else(|| song.display_artist().into())));
            } else if let Some(shelf) = c.shelf_of(&song) {
                n.open(if shelf.is_book() { Route::Books } else { Route::Shows });
                n.push(Route::Shelf(shelf.id.clone()));
            } else if let Some(album) = c.album_of.get(&song.id) {
                n.open(Route::Albums);
                n.push(Route::Album(album.id.clone()));
            }
            s.set_show_content(true);
        });
        window.add_action(&go);
    }
    keys(&window, app);
    // Syncs on becoming active, as on the phone and the Mac.
    let a = app.clone();
    window.connect_is_active_notify(move |w| {
        if w.is_active() {
            a.sync_soon(Duration::from_secs(2));
        }
    });
    nav.open(Route::Albums);
    if let Ok(spec) = std::env::var("DHUN_OPEN") {
        open_when_loaded(app, &nav, &split, &panel, spec);
    }
    window
}

/// For screenshots under test (`DHUN_SCREENSHOT`): pages to open once the
/// library has loaded, as `artists,artist:Name`, `album:Name`,
/// `folder:Path`, `books`, `search:words` (`search:` for the empty field), `play-album:Name`,
/// `play:<song id>`, `switch:<queue name>`, `list:N` (of `ListKind::ALL`), `playlist:Name`, `queue:Name`,
/// `select:N` and `do:<action>[=text|#number]` for a song list's menu, `type:text` into a dialog,
/// `tempo:<speed>/<semitones>`, `sleep:end` or `sleep:<minutes>`, `popup:<tooltip>` for a button, `settings:<page>`, `seek-later:<ms>`, `continue-later` for a hand-off, `panel` for the playing queue,
/// `sidebar` for a narrow window, `app:<action>` (`app:mini`), `close` for the window, or a click two seconds on: `activate:N` on a song row, `nav:N` on a sidebar row.
fn open_when_loaded(
    app: &Rc<App>,
    nav: &Rc<Nav>,
    split: &adw::NavigationSplitView,
    panel: &adw::OverlaySplitView,
    spec: String,
) {
    let (a, n, s, p) = (app.clone(), nav.clone(), split.clone(), panel.clone());
    app.listen(move |c| {
        if c != Change::Catalog {
            return true;
        }
        for (i, step) in spec.split(',').enumerate() {
            let (what, arg) = step.split_once(':').unwrap_or((step, ""));
            let route = match what {
                "queues" => Route::Queues,
                "downloads" => Route::Downloads,
                "artists" => Route::Artists,
                "genres" => Route::Genres,
                "folders" => Route::Folder(String::new()),
                "folder" => Route::Folder(arg.into()),
                "books" => Route::Books,
                "shows" => Route::Shows,
                "artist" => Route::Artist(arg.into()),
                "album" => match a.catalog().albums.iter().find(|x| x.name == arg) {
                    Some(x) => Route::Album(x.id.clone()),
                    None => continue,
                },
                "list" => match crate::lists::ListKind::ALL.get(arg.parse::<usize>().unwrap_or(99)) {
                    Some(k) => Route::List(*k),
                    None => continue,
                },
                "playlist" => match a.playlists.borrow().iter().find(|x| x.name == arg) {
                    Some(x) => Route::Playlist(x.id),
                    None => continue,
                },
                "queue" => match a.queues.borrow().iter().find(|x| x.name == arg) {
                    Some(x) => Route::Queue(x.id.clone()),
                    None => continue,
                },
                "select" => {
                    let (s, row) = (s.clone(), arg.parse::<u32>().unwrap_or(0));
                    glib::timeout_add_seconds_local_once(2, move || {
                        if let Some(m) = shown::<gtk::ColumnView>(s.upcast_ref()).and_then(|v| v.model()) {
                            m.select_item(row, false);
                        }
                    });
                    continue;
                }
                "do" => {
                    // An action of the song list on screen, as its menu would: `songs.mark=fav:1`, `songs.add-to-playlist#7`.
                    let (s, arg) = (s.clone(), arg.to_string());
                    glib::timeout_add_seconds_local_once(3, move || {
                        let target = if let Some((name, n)) = arg.split_once('#') {
                            (name.to_string(), Some(n.parse::<i64>().unwrap_or(0).to_variant()))
                        } else if let Some((name, t)) = arg.split_once('=') {
                            (name.to_string(), Some(t.to_variant()))
                        } else {
                            (arg.clone(), None)
                        };
                        if let Some(v) = shown::<gtk::ColumnView>(s.upcast_ref()) {
                            if let Err(e) = v.activate_action(&target.0, target.1.as_ref()) {
                                eprintln!("do: {e}");
                            }
                        }
                    });
                    continue;
                }
                "tempo" => {
                    // `tempo:1.5/-2` three seconds on, then how far the song moved in the four after.
                    let (a, arg) = (a.clone(), arg.to_string());
                    glib::timeout_add_seconds_local_once(3, move || {
                        let (speed, semi) = arg.split_once('/').unwrap_or((&arg, "0"));
                        let t = dhun_core::tempo::Tempo {
                            speed: speed.parse().unwrap_or(1.0),
                            semitones: semi.parse().unwrap_or(0),
                        };
                        a.playback().set_tempo(t, false);
                        let (a, from) = (a.clone(), a.playback().position_ms());
                        glib::timeout_add_seconds_local_once(5, move || {
                            let at = a.playback().position_ms();
                            glib::g_info!("Dhun", "tempo: {from} to {at} ms after 5 s");
                        });
                    });
                    continue;
                }
                "popup" => {
                    // Opens the menu button with this tooltip, or clicks the button, two seconds on.
                    let (s, tip) = (s.clone(), arg.to_string());
                    glib::timeout_add_seconds_local_once(2, move || {
                        let Some(root) = s.root() else { return };
                        let mut todo = vec![root.upcast::<gtk::Widget>()];
                        while let Some(x) = todo.pop() {
                            if x.tooltip_text().is_some_and(|t| t.starts_with(&tip)) {
                                if let Some(m) = x.downcast_ref::<gtk::MenuButton>() {
                                    return m.popup();
                                }
                                if let Some(b) = x.downcast_ref::<gtk::Button>() {
                                    return b.emit_clicked();
                                }
                            }
                            let mut c = x.first_child();
                            while let Some(n) = c {
                                c = n.next_sibling();
                                todo.push(n);
                            }
                        }
                    });
                    continue;
                }
                "settings" => {
                    // Settings, at the page of that title.
                    let (a, s, page) = (a.clone(), s.clone(), arg.to_string());
                    glib::timeout_add_seconds_local_once(2, move || {
                        crate::settings::open(&a, &s).set_visible_page_name(&page);
                    });
                    continue;
                }
                "sleep" => {
                    // `sleep:end` stops at the end of this song, `sleep:<minutes>` by the clock.
                    let pb = a.playback();
                    match arg.parse::<i64>() {
                        Ok(m) => pb.sleep_minutes(m, false),
                        Err(_) => pb.sleep_end_of_song(),
                    }
                    continue;
                }
                "type" => {
                    // Text into the dialog's field, then Return, four seconds on.
                    let (s, text) = (s.clone(), arg.to_string());
                    glib::timeout_add_seconds_local_once(4, move || {
                        let root = s.root().and_downcast::<gtk::Window>().and_then(|w| w.first_child());
                        let dialog = root.and_then(|w| shown::<adw::Dialog>(&w));
                        // Its field may not be mapped yet while the dialog opens.
                        let field = dialog.and_then(|d| {
                            let mut todo = vec![d.upcast::<gtk::Widget>()];
                            while let Some(w) = todo.pop() {
                                if let Ok(t) = w.clone().downcast::<gtk::Text>() {
                                    return Some(t);
                                }
                                let mut c = w.first_child();
                                while let Some(x) = c {
                                    c = x.next_sibling();
                                    todo.push(x);
                                }
                            }
                            None
                        });
                        match field {
                            Some(t) => {
                                t.set_text(&text);
                                t.emit_by_name::<()>("activate", &[]);
                            }
                            None => eprintln!("type: no dialog with a text field"),
                        }
                    });
                    continue;
                }
                "shelf" => match a.catalog().shelves.values().find(|x| x.title == arg) {
                    Some(x) => Route::Shelf(x.id.clone()),
                    None => continue,
                },
                "search" => {
                    // `search:` alone is the empty field's page.
                    if arg.is_empty() {
                        n.show_recent()
                    } else {
                        n.search.set_text(arg)
                    }
                    continue;
                }
                "continue-later" => {
                    // Takes the hand-off offered, once the sync has brought it.
                    let a = a.clone();
                    glib::timeout_add_seconds_local_once(4, move || {
                        if let Some(np) = a.playback().handoff() {
                            a.playback().continue_from(&np);
                        }
                    });
                    continue;
                }
                "seek-later" => {
                    // A seek three seconds on, once the song has opened.
                    let (a, ms) = (a.clone(), arg.parse().unwrap_or(0));
                    glib::timeout_add_seconds_local_once(3, move || a.playback().seek(ms));
                    continue;
                }
                "activate" => {
                    // A click on row N of the list on screen, once the page is built.
                    let (s, row) = (s.clone(), arg.parse::<u32>().unwrap_or(0));
                    glib::timeout_add_seconds_local_once(2, move || {
                        if let Some(v) = shown::<gtk::ColumnView>(s.upcast_ref()) {
                            v.emit_by_name::<()>("activate", &[&row]);
                        }
                    });
                    continue;
                }
                "nav" => {
                    // A click on row N of the sidebar, which refills itself as it opens the page.
                    let (s, row) = (s.clone(), arg.parse::<i32>().unwrap_or(0));
                    glib::timeout_add_seconds_local_once(2, move || {
                        if let Some(r) = shown::<gtk::ListBox>(s.upcast_ref()).and_then(|l| l.row_at_index(row)) {
                            r.activate();
                        }
                    });
                    continue;
                }
                "switch" => {
                    let id = a.queues.borrow().iter().find(|q| q.name == arg).map(|q| q.id.clone());
                    if let Some(id) = id {
                        a.playback().switch_to(&id);
                    }
                    continue;
                }
                "play" => {
                    if let Some(x) = arg.parse().ok().and_then(|id: i64| a.catalog().by_id.get(&id).cloned()) {
                        a.playback().play_list(&x.title, "", std::slice::from_ref(&x), 0);
                    }
                    continue;
                }
                "play-album" => {
                    if let Some(x) = a.catalog().albums.iter().find(|x| x.name == arg) {
                        a.playback().play_list(&x.name, &format!("album:{}", x.id), &x.songs, 0);
                    }
                    continue;
                }
                "panel" => {
                    p.set_show_sidebar(true);
                    continue;
                }
                "sidebar" => {
                    s.set_show_content(false);
                    continue;
                }
                "close" => {
                    if let Some(w) = s.root().and_downcast::<gtk::Window>() {
                        w.close();
                    }
                    continue;
                }
                // An app action, as the menu or the launcher would: `app:mini`, `app:shuffle`.
                "app" => {
                    if let Some(g) = gio::Application::default() {
                        g.activate_action(arg, None);
                    }
                    continue;
                }
                _ => continue,
            };
            if i == 0 { n.open(route) } else { n.push(route) }
        }
        false
    });
}

/// The first `T` on screen under `w`, for the `activate` and `nav` test steps.
fn shown<T: IsA<gtk::Widget>>(w: &gtk::Widget) -> Option<T> {
    if !w.is_mapped() {
        return None;
    }
    if let Some(v) = w.downcast_ref::<T>() {
        return Some(v.clone());
    }
    let mut c = w.first_child();
    while let Some(x) = c {
        if let Some(v) = shown(&x) {
            return Some(v);
        }
        c = x.next_sibling();
    }
    None
}

/// Space plays or pauses, Ctrl+← and Ctrl+→ go to the previous and next
/// song, as on the Mac: caught before the focused list or button, except
/// while typing, where Space is a space.
fn keys(window: &adw::ApplicationWindow, app: &Rc<App>) {
    let c = gtk::EventControllerKey::new();
    c.set_propagation_phase(gtk::PropagationPhase::Capture);
    let (a, w) = (app.clone(), window.downgrade());
    c.connect_key_pressed(move |_, key, _, mods| {
        // Typing, or a dialog open over the window: its focused button takes Space.
        let focus = w.upgrade().and_then(|w| gtk::prelude::GtkWindowExt::focus(&w));
        let typing = focus.is_some_and(|f| f.is::<gtk::Text>() || f.ancestor(adw::Dialog::static_type()).is_some());
        let ctrl = mods.contains(gtk::gdk::ModifierType::CONTROL_MASK);
        let pb = a.playback();
        match key {
            gtk::gdk::Key::space if !typing && !ctrl => pb.toggle(),
            gtk::gdk::Key::Left if ctrl && !typing => pb.previous(true),
            gtk::gdk::Key::Right if ctrl && !typing => pb.next(true),
            _ => return glib::Propagation::Proceed,
        }
        glib::Propagation::Stop
    });
    window.add_controller(c);
}

fn main_view(app: &Rc<App>, nav: &Rc<Nav>) -> adw::NavigationSplitView {
    let split = adw::NavigationSplitView::builder().min_sidebar_width(220.0).max_sidebar_width(280.0).build();
    let list = gtk::ListBox::builder().css_classes(["navigation-sidebar"]).build();
    let routes: Rc<std::cell::RefCell<Vec<Option<Route>>>> = Rc::default();
    let fill = {
        let (app, list, routes, nav) = (app.clone(), list.clone(), routes.clone(), nav.clone());
        move || {
            list.remove_all();
            let mut r = routes.borrow_mut();
            r.clear();
            let c = app.catalog();
            let mut item = |icon: &str, title: &str, route: Option<Route>| {
                let row = gtk::ListBoxRow::new();
                if let Some(route) = route {
                    let b = gtk::Box::builder().spacing(10).build();
                    b.append(&gtk::Image::from_icon_name(icon));
                    b.append(
                        &gtk::Label::builder()
                            .label(title)
                            .xalign(0.0)
                            .ellipsize(gtk::pango::EllipsizeMode::End)
                            .build(),
                    );
                    row.set_child(Some(&b));
                    if nav.current().as_ref() == Some(&route) {
                        list.select_row(Some(&row));
                    }
                    r.push(Some(route));
                } else {
                    row.set_child(Some(
                        &gtk::Label::builder()
                            .label(title)
                            .xalign(0.0)
                            .css_classes(["heading", "dim-label"])
                            .margin_top(12)
                            .build(),
                    ));
                    row.set_activatable(false);
                    row.set_selectable(false);
                    r.push(None);
                }
                list.append(&row);
            };
            item("view-list-symbolic", "Queues", Some(Route::Queues));
            item("", "Library", None);
            item("folder-symbolic", "Folders", Some(Route::Folder(String::new())));
            item("media-optical-symbolic", "Albums", Some(Route::Albums));
            item("avatar-default-symbolic", "Artists", Some(Route::Artists));
            item("view-grid-symbolic", "Genres", Some(Route::Genres));
            // Each only once its collection has something in it (plan 031).
            if !c.books.is_empty() {
                item("accessories-dictionary-symbolic", "Audiobooks", Some(Route::Books));
            }
            if !c.shows.is_empty() {
                item("audio-input-microphone-symbolic", "Podcasts", Some(Route::Shows));
            }
            item("", "Lists", None);
            for k in crate::lists::ListKind::ALL {
                item(k.icon(), k.title(), Some(Route::List(k)));
            }
            let playlists = app.playlists.borrow().clone();
            if !playlists.is_empty() {
                item("", "Playlists", None);
                for p in playlists {
                    item(
                        if p.shared { "user-home-symbolic" } else { "view-list-symbolic" },
                        &p.name,
                        Some(Route::Playlist(p.id)),
                    );
                }
            }
            item("", "", None);
            item("folder-download-symbolic", "Downloads", Some(Route::Downloads));
        }
    };
    fill();
    let refill = fill.clone();
    app.listen(move |c| {
        if matches!(c, Change::Catalog | Change::Playlists | Change::Account | Change::Section) {
            refill();
        }
        true
    });
    let (n, s) = (nav.clone(), split.clone());
    list.connect_row_activated(move |_, row| {
        // Taken out first: opening refills the sidebar, which needs `routes` mutably.
        let route = routes.borrow().get(row.index() as usize).cloned().flatten();
        if let Some(route) = route {
            n.search.set_text("");
            n.open(route);
            s.set_show_content(true);
        }
    });

    let top = adw::HeaderBar::new();
    top.pack_end(&menu());
    let new = gtk::Button::builder().icon_name("list-add-symbolic").tooltip_text("New Playlist").build();
    let (a, n, s) = (app.clone(), nav.clone(), split.clone());
    new.connect_clicked(move |b| {
        let (a, n, s) = (a.clone(), n.clone(), s.clone());
        crate::widgets::ask_name(b, "New Playlist", "", "Create", move |name| {
            let mut made = None;
            a.store(|db| dhun_core::store::create_playlist(db, &name, &[]).map(|p| made = Some(p.id)));
            a.reload_playlists();
            if let Some(id) = made {
                n.open(Route::Playlist(id));
                s.set_show_content(true);
            }
        });
    });
    top.pack_start(&new);
    let sidebar = gtk::Box::builder().orientation(gtk::Orientation::Vertical).build();
    let search = &nav.search;
    search.set_margin_start(8);
    search.set_margin_end(8);
    search.set_margin_bottom(6);
    let s = split.clone();
    search.connect_search_changed(move |e| {
        if !e.text().is_empty() {
            s.set_show_content(true);
        }
    });
    sidebar.append(search);
    sidebar.append(&crate::widgets::scrolled(&list));
    sidebar.append(&status(app));
    let tv = adw::ToolbarView::new();
    tv.add_top_bar(&top);
    tv.set_content(Some(&sidebar));
    split.set_sidebar(Some(&adw::NavigationPage::builder().title("Dhun").child(&tv).build()));
    split.set_content(Some(&adw::NavigationPage::builder().title("Library").child(&nav.view).build()));
    split
}

fn menu() -> gtk::MenuButton {
    let m = gio::Menu::new();
    m.append(Some("Mini Player"), Some("app.mini"));
    m.append(Some("Settings"), Some("win.settings"));
    m.append(Some("Sync Now"), Some("app.sync"));
    m.append(Some("Sign Out"), Some("app.sign-out"));
    m.append(Some("About Dhun"), Some("app.about"));
    m.append(Some("Quit"), Some("app.quit"));
    gtk::MenuButton::builder().icon_name("open-menu-symbolic").menu_model(&m).tooltip_text("Menu").build()
}

/// Offline, syncing, or an error, small at the foot of the sidebar.
fn status(app: &Rc<App>) -> gtk::Box {
    let b = gtk::Box::builder().spacing(6).margin_start(12).margin_end(12).margin_top(6).margin_bottom(8).build();
    let spinner = gtk::Spinner::new();
    let label = gtk::Label::builder()
        .xalign(0.0)
        .css_classes(["caption", "dim-label"])
        .ellipsize(gtk::pango::EllipsizeMode::End)
        .build();
    b.append(&spinner);
    b.append(&label);
    let update = {
        let (app, spinner, label) = (app.clone(), spinner.clone(), label.clone());
        move || {
            spinner.set_spinning(app.syncing.get());
            spinner.set_visible(app.syncing.get());
            let (text, tip) = if app.offline.get() {
                ("Offline".to_string(), "The server cannot be reached. Changes are kept and sent later.".to_string())
            } else if app.syncing.get() {
                ("Syncing…".into(), String::new())
            } else if let Some(e) = app.sync_error.borrow().clone() {
                ("Sync failed".into(), e)
            } else {
                (String::new(), String::new())
            };
            label.set_label(&text);
            label.set_tooltip_text(Some(&tip).filter(|t| !t.is_empty()).map(|t| t.as_str()));
        }
    };
    update();
    app.listen(move |c| {
        if c == Change::Status {
            update();
        }
        true
    });
    b
}

fn sign_in(app: &Rc<App>) -> gtk::Widget {
    let group = adw::PreferencesGroup::new();
    let server = adw::EntryRow::builder().title("Server").text(app.prefs.get().server).build();
    let user = adw::EntryRow::builder().title("Name").text(app.prefs.get().user).build();
    let password = adw::PasswordEntryRow::builder().title("Password").build();
    group.add(&server);
    group.add(&user);
    group.add(&password);
    let button = gtk::Button::builder()
        .label("Sign In")
        .css_classes(["suggested-action", "pill"])
        .halign(gtk::Align::Center)
        .margin_top(18)
        .build();
    let error = gtk::Label::builder().wrap(true).css_classes(["error"]).margin_top(12).build();
    let b = gtk::Box::builder().orientation(gtk::Orientation::Vertical).build();
    b.append(&group);
    b.append(&button);
    b.append(&error);
    let page = adw::StatusPage::builder()
        .title("Sign in to Dhun")
        .description("The address of the Dhun server on your NAS, as http://nas:8585 or just its name")
        .icon_name("folder-music-symbolic")
        .child(&adw::Clamp::builder().maximum_size(420).child(&b).build())
        .build();
    let go = {
        let (app, server, user, password, button, error) =
            (app.clone(), server.clone(), user.clone(), password.clone(), button.clone(), error.clone());
        move || {
            let (s, u, p) = (server_url(&server.text()), user.text().trim().to_string(), password.text().to_string());
            if s.is_empty() || u.is_empty() || p.is_empty() {
                error.set_label("The server, a name and the password are all needed");
                return;
            }
            button.set_sensitive(false);
            error.set_label("");
            let (app, button, error, password) = (app.clone(), button.clone(), error.clone(), password.clone());
            glib::spawn_future_local(async move {
                match app.sign_in(s, u, p).await {
                    Ok(()) => password.set_text(""),
                    Err(e) => error.set_label(&e),
                }
                button.set_sensitive(true);
            });
        }
    };
    let g = go.clone();
    button.connect_clicked(move |_| g());
    password.connect_entry_activated(move |_| go());
    let tv = adw::ToolbarView::new();
    tv.add_top_bar(&adw::HeaderBar::new());
    tv.set_content(Some(&page));
    tv.upcast()
}
