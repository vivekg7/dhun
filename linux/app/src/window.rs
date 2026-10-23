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
        .default_width(1100)
        .default_height(720)
        .build();
    let stack = gtk::Stack::builder().transition_type(gtk::StackTransitionType::Crossfade).build();
    let nav = Nav::new(app);
    let split = main_view(app, &nav);
    // The playing queue beside the page, and Now playing along the foot (plan 024's layout).
    let panel = adw::OverlaySplitView::builder()
        .sidebar_position(gtk::PackType::End)
        .show_sidebar(false)
        .content(&split)
        .sidebar(&crate::nowplaying::queue_panel(app))
        .build();
    let main = adw::ToolbarView::builder().content(&panel).build();
    main.add_bottom_bar(&crate::nowplaying::bar(app, &panel));
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
    // One pane at a time below this width, as on a phone.
    let narrow = adw::Breakpoint::new(adw::BreakpointCondition::parse("max-width: 640sp").unwrap());
    narrow.add_setter(&split, "collapsed", Some(&true.to_value()));
    window.add_breakpoint(narrow);
    // Ctrl+F searches.
    let find = gio::SimpleAction::new("find", None);
    let n = nav.clone();
    let s = split.clone();
    find.connect_activate(move |_, _| {
        s.set_show_content(false);
        n.search.grab_focus();
    });
    window.add_action(&find);
    gapp.set_accels_for_action("win.find", &["<Control>f"]);
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
/// `folder:Path`, `books`, `search:words`, `play-album:Name`,
/// `switch:<queue name>`, `seek-later:<ms>`, `continue-later` for a hand-off, `panel` for the playing queue,
/// `sidebar` for a narrow window, or a click two seconds on: `activate:N` on a song row, `nav:N` on a sidebar row.
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
                "shelf" => match a.catalog().shelves.values().find(|x| x.title == arg) {
                    Some(x) => Route::Shelf(x.id.clone()),
                    None => continue,
                },
                "search" => {
                    n.search.set_text(arg);
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
        let typing =
            w.upgrade().and_then(|w| gtk::prelude::GtkWindowExt::focus(&w)).is_some_and(|f| f.is::<gtk::Text>());
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
    m.append(Some("Sync Now"), Some("app.sync"));
    m.append(Some("Sign Out"), Some("app.sign-out"));
    m.append(Some("About Dhun"), Some("app.about"));
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
