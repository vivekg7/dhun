//! Lyrics (plan 015) in the panel beside the page, as the Mac's inspector:
//! synced lines follow the song and a click on one plays from it; plain
//! lyrics are just text. The panel's other page is the playing queue.

use crate::app::{App, Change};
use adw::prelude::*;
use dhun_core::lyrics::{self, Parsed};
use gtk::{gio, glib};
use std::cell::RefCell;
use std::rc::Rc;
use std::time::{Duration, Instant};

/// The panel: the playing queue and the lyrics, one at a time.
pub fn side(app: &Rc<App>) -> (gtk::Box, gtk::Stack) {
    let stack = gtk::Stack::builder().transition_type(gtk::StackTransitionType::Crossfade).vexpand(true).build();
    stack.add_titled(&crate::nowplaying::queue_panel(app), Some("queue"), "Queue");
    let lyrics_page = stack.add_titled(&panel(app), Some("lyrics"), "Lyrics");
    // An episode's transcript comes as its lyrics (plan 031).
    let p = app.playback().clone();
    let name = move || if p.current().is_some_and(|s| s.spoken()) { "Transcript" } else { "Lyrics" };
    lyrics_page.set_title(name());
    app.listen(move |c| {
        if c == Change::Playback {
            lyrics_page.set_title(name());
        }
        true
    });
    let switcher = gtk::StackSwitcher::builder().stack(&stack).halign(gtk::Align::Center).margin_top(8).build();
    let b = gtk::Box::builder().orientation(gtk::Orientation::Vertical).width_request(300).build();
    b.append(&switcher);
    b.append(&stack);
    (b, stack)
}

enum State {
    Loading,
    /// The server has none for this song.
    None,
    /// Not kept here, and the server cannot be reached.
    Offline,
    Shown(Parsed),
}

struct View {
    song: i64,
    state: State,
    /// The lines on screen, for following the song.
    labels: Vec<gtk::Label>,
    at: Option<usize>,
    /// A scroll by hand holds the lines where they are for a few seconds, as on the phone.
    held_until: Instant,
    /// The display kept awake while lyrics show and play, as the phone's screen is.
    awake: Option<u32>,
}

fn panel(app: &Rc<App>) -> gtk::Widget {
    let holder = adw::Bin::builder().vexpand(true).build();
    let view = Rc::new(RefCell::new(View {
        song: 0,
        state: State::None,
        labels: vec![],
        at: None,
        held_until: Instant::now(),
        awake: None,
    }));
    let show: Rc<dyn Fn()> = {
        let (app, holder, view) = (app.clone(), holder.downgrade(), view.clone());
        Rc::new(move || {
            let Some(holder) = holder.upgrade() else { return };
            let spoken = app.playback().current().is_some_and(|s| s.spoken());
            let status = |title: &str, icon: &str| {
                adw::StatusPage::builder().title(title).icon_name(icon).build().upcast::<gtk::Widget>()
            };
            let mut v = view.borrow_mut();
            v.labels.clear();
            v.at = None;
            let child = match &v.state {
                State::Loading => gtk::Spinner::builder()
                    .spinning(true)
                    .halign(gtk::Align::Center)
                    .valign(gtk::Align::Center)
                    .build()
                    .upcast(),
                State::None => {
                    status(if spoken { "No transcript" } else { "No lyrics" }, "media-view-subtitles-symbolic")
                }
                State::Offline => status("Lyrics need the server", "network-offline-symbolic"),
                State::Shown(l) => {
                    let lines = gtk::Box::builder()
                        .orientation(gtk::Orientation::Vertical)
                        .spacing(10)
                        .margin_start(16)
                        .margin_end(16)
                        .margin_top(16)
                        .margin_bottom(16)
                        .build();
                    let mut labels = vec![];
                    for line in &l.lines {
                        let label = gtk::Label::builder()
                            .label(if line.text.is_empty() { " " } else { &line.text })
                            .xalign(0.0)
                            .wrap(true)
                            .selectable(!l.synced)
                            .build();
                        if l.synced {
                            label.add_css_class("title-4");
                            label.add_css_class("dim-label");
                            let g = gtk::GestureClick::new();
                            let (p, ms) = (app.playback().clone(), line.ms);
                            g.connect_released(move |_, _, _, _| p.seek(ms));
                            label.add_controller(g);
                            label.set_cursor_from_name(Some("pointer"));
                        }
                        lines.append(&label);
                        labels.push(label);
                    }
                    v.labels = labels;
                    let scroll = crate::widgets::scrolled(&lines);
                    let held = gtk::EventControllerScroll::new(gtk::EventControllerScrollFlags::VERTICAL);
                    let vw = view.clone();
                    held.connect_scroll(move |_, _, _| {
                        vw.borrow_mut().held_until = Instant::now() + Duration::from_secs(4);
                        glib::Propagation::Proceed
                    });
                    scroll.add_controller(held);
                    scroll.upcast()
                }
            };
            drop(v);
            holder.set_child(Some(&child));
        })
    };
    // The line playing, marked, and scrolled to the middle unless held.
    let follow = {
        let (app, view) = (app.clone(), view.clone());
        move || {
            let mut v = view.borrow_mut();
            let State::Shown(l) = &v.state else { return };
            let at = l.at(app.playback().position_ms());
            if at == v.at {
                return;
            }
            for (i, on) in [(v.at, false), (at, true)] {
                if let Some(label) = i.and_then(|i| v.labels.get(i)) {
                    if on {
                        label.remove_css_class("dim-label");
                    } else {
                        label.add_css_class("dim-label");
                    }
                }
            }
            v.at = at;
            if Instant::now() < v.held_until {
                return;
            }
            let Some(label) = at.and_then(|i| v.labels.get(i)).cloned() else { return };
            drop(v);
            let Some(sw) = label.ancestor(gtk::ScrolledWindow::static_type()).and_downcast::<gtk::ScrolledWindow>()
            else {
                return;
            };
            if let Some(b) = label.compute_bounds(&sw.child().unwrap()) {
                let adj = sw.vadjustment();
                adj.set_value((b.y() as f64 + b.height() as f64 / 2.0 - adj.page_size() / 2.0).max(0.0));
            }
        }
    };
    let load = {
        let (app, view, show) = (app.clone(), view.clone(), show.clone());
        move || {
            let s = app.playback().current();
            let id = s.as_ref().map_or(0, |s| s.id);
            if view.borrow().song == id {
                return;
            }
            view.borrow_mut().song = id;
            let Some(s) = s.filter(|s| s.has_lyrics) else {
                view.borrow_mut().state = State::None;
                return show();
            };
            // The kept copy first, then the server's when it differs.
            let kept = app.db.lock().unwrap().lyrics(s.id).ok().flatten();
            view.borrow_mut().state = kept.as_deref().map_or(State::Loading, |t| State::Shown(lyrics::parse(t)));
            show();
            let (api, db, view, show) = (app.api.clone(), app.db.clone(), view.clone(), show.clone());
            glib::spawn_future_local(async move {
                let id = s.id;
                let got = gio::spawn_blocking(move || lyrics::fetch(&api, &db, id)).await;
                if view.borrow().song != id {
                    return;
                }
                let state = match got {
                    Ok(Ok(Some(text))) if Some(&text) != kept.as_ref() => State::Shown(lyrics::parse(&text)),
                    Ok(Ok(Some(_))) => return,
                    Ok(Ok(None)) => State::None,
                    // Unreachable, or an answer we cannot read: the kept copy, if any, stays up.
                    _ if kept.is_some() => return,
                    _ => State::Offline,
                };
                view.borrow_mut().state = state;
                show();
            });
        }
    };
    // Loaded only while the lyrics are on screen: the queue's page is the usual one.
    let awake = {
        let (app, view, holder) = (app.clone(), view.clone(), holder.clone());
        move || {
            let gapp = gio::Application::default().and_downcast::<gtk::Application>();
            let on = holder.is_mapped() && app.playback().is_playing();
            let mut v = view.borrow_mut();
            match (on, v.awake, gapp) {
                (true, None, Some(g)) => {
                    let w = holder.root().and_downcast::<gtk::Window>();
                    v.awake = Some(g.inhibit(w.as_ref(), gtk::ApplicationInhibitFlags::IDLE, Some("Showing lyrics")));
                }
                (false, Some(c), Some(g)) => {
                    // 0: the session could not be asked, so there is nothing to undo.
                    if c > 0 {
                        g.uninhibit(c);
                    }
                    v.awake = None;
                }
                _ => {}
            }
        }
    };
    let (l, a) = (load.clone(), awake.clone());
    holder.connect_map(move |_| {
        l();
        a();
    });
    let a = awake.clone();
    holder.connect_unmap(move |_| a());
    let h = holder.downgrade();
    app.listen(move |c| {
        let Some(h) = h.upgrade() else { return false };
        if !h.is_mapped() {
            return true;
        }
        match c {
            Change::Playback => {
                load();
                follow();
                awake();
            }
            Change::Position => follow(),
            _ => {}
        }
        true
    });
    holder.upcast()
}

/// Songs whose lyrics hold the words: the server's answer, or else, offline
/// or from a server without the endpoint, the lyrics kept here.
pub async fn search(app: &App, q: dhun_core::search::Query) -> Vec<lyrics::Hit> {
    let (api, db) = (app.api.clone(), app.db.clone());
    gio::spawn_blocking(move || match api.search_lyrics(&q.lyrics) {
        Ok(hits) => hits,
        // Kept lyrics are a few megabytes at most.
        Err(_) => lyrics::search_kept(&db.lock().unwrap().all_lyrics().unwrap_or_default(), &q),
    })
    .await
    .unwrap_or_default()
}
