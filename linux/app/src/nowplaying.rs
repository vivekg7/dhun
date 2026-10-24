//! The Now playing bar along the window's foot, the hand-off and resume
//! offers above it, and the panel with the playing queue, as on the Mac
//! (plans 017, 020, 024).

use crate::app::{App, Change};
use crate::widgets::{self, clock, cover, row};
use adw::prelude::*;
use dhun_core::{store, synced};
use std::cell::Cell;
use std::rc::Rc;

/// The bar and the offers over it.
pub fn bar(app: &Rc<App>, panel: &adw::OverlaySplitView, pages: &gtk::Stack) -> gtk::Box {
    let outer = gtk::Box::builder().orientation(gtk::Orientation::Vertical).build();
    outer.append(&handoff(app));
    outer.append(&resume_offer(app));
    // Close together: a long file's bar holds thirteen buttons.
    let b = gtk::Box::builder().spacing(4).margin_start(12).margin_end(12).margin_top(8).margin_bottom(8).build();
    let art = adw::Bin::new();
    let title =
        gtk::Label::builder().xalign(0.0).ellipsize(gtk::pango::EllipsizeMode::End).css_classes(["heading"]).build();
    let artist = gtk::Label::builder()
        .xalign(0.0)
        .ellipsize(gtk::pango::EllipsizeMode::End)
        .css_classes(["caption", "dim-label"])
        .build();
    let text = gtk::Box::builder()
        .orientation(gtk::Orientation::Vertical)
        .valign(gtk::Align::Center)
        .width_request(200)
        .margin_start(8)
        .margin_end(8)
        .build();
    text.append(&title);
    text.append(&artist);
    let button = |icon: &str, tip: &str| {
        gtk::Button::builder()
            .icon_name(icon)
            .tooltip_text(tip)
            .css_classes(["flat", "circular"])
            .valign(gtk::Align::Center)
            .build()
    };
    let prev = button("media-skip-backward-symbolic", "Previous (Ctrl+←)");
    let play = gtk::Button::builder()
        .icon_name("media-playback-start-symbolic")
        .tooltip_text("Play (Space)")
        .css_classes(["circular", "suggested-action"])
        .valign(gtk::Align::Center)
        .build();
    let next = button("media-skip-forward-symbolic", "Next (Ctrl+→)");
    let pos = gtk::Label::builder().css_classes(["caption", "numeric", "dim-label"]).width_chars(7).xalign(1.0).build();
    let scale = gtk::Scale::builder().hexpand(true).draw_value(false).valign(gtk::Align::Center).build();
    scale.set_range(0.0, 1.0);
    let dur = gtk::Label::builder().css_classes(["caption", "numeric", "dim-label"]).width_chars(7).xalign(0.0).build();
    let shuffle = gtk::ToggleButton::builder()
        .icon_name("media-playlist-shuffle-symbolic")
        .tooltip_text("Shuffle")
        .css_classes(["flat", "circular"])
        .valign(gtk::Align::Center)
        .build();
    let repeat = button("media-playlist-repeat-symbolic", "Repeat: off");
    // The panel beside the page, on the playing queue or the lyrics: a click
    // shows that page, or hides the panel when it already shows it.
    let side = |icon: &str, tip: &str, page: &'static str| {
        let b = gtk::ToggleButton::builder()
            .icon_name(icon)
            .tooltip_text(tip)
            .css_classes(["flat", "circular"])
            .valign(gtk::Align::Center)
            .build();
        let (pl, pg) = (panel.clone(), pages.clone());
        b.connect_clicked(move |_| {
            let showing = pl.shows_sidebar() && pg.visible_child_name().as_deref() == Some(page);
            pg.set_visible_child_name(page);
            pl.set_show_sidebar(!showing);
        });
        let sync = {
            let (b, panel, pages) = (b.downgrade(), panel.downgrade(), pages.downgrade());
            move || {
                let (Some(b), Some(panel), Some(pages)) = (b.upgrade(), panel.upgrade(), pages.upgrade()) else {
                    return;
                };
                b.set_active(panel.shows_sidebar() && pages.visible_child_name().as_deref() == Some(page));
            }
        };
        let s = sync.clone();
        panel.connect_show_sidebar_notify(move |_| s());
        pages.connect_visible_child_name_notify(move |_| sync());
        b
    };
    let queue = side("view-list-symbolic", "The playing queue", "queue");
    let lyrics = side("media-view-subtitles-symbolic", "Lyrics", "lyrics");
    // A long file steps back and on within itself (plan 009).
    let back = button("media-seek-backward-symbolic", "Back");
    let on = button("media-seek-forward-symbolic", "Forward");
    // A book is heard in order: in Shuffle's place, its chapters and notes (plan 031).
    let details = crate::controls::details_button(app);
    for w in [
        art.upcast_ref::<gtk::Widget>(),
        text.upcast_ref(),
        details.upcast_ref(),
        shuffle.upcast_ref(),
        prev.upcast_ref(),
        back.upcast_ref(),
        play.upcast_ref(),
        on.upcast_ref(),
        next.upcast_ref(),
        repeat.upcast_ref(),
    ] {
        b.append(w);
    }
    for w in [pos.upcast_ref::<gtk::Widget>(), scale.upcast_ref(), dur.upcast_ref()] {
        b.append(w);
    }
    // Favorites and Listen Later for the song playing, as the phone's Now playing has.
    let fav = button("non-starred-symbolic", "Add to Favorites");
    let later = button("alarm-symbolic", "Listen Later");
    let mark = button("bookmark-new-symbolic", "Add a Bookmark");
    let a = app.clone();
    mark.connect_clicked(move |_| crate::controls::add_bookmark(&a));
    for w in [
        fav.upcast_ref::<gtk::Widget>(),
        later.upcast_ref(),
        mark.upcast_ref(),
        crate::controls::speed_button(app).upcast_ref(),
        crate::controls::sleep_button(app).upcast_ref(),
        lyrics.upcast_ref(),
        queue.upcast_ref(),
    ] {
        b.append(w);
    }
    // A chapter back or on, from the right-click menu of Previous and Next:
    // a click goes by file, and a chapter is asked for by name, never by a
    // press that could be miscounted (plan 009).
    for (w, label, forward) in [(&prev, "Previous Chapter", false), (&next, "Next Chapter", true)] {
        let menu = gtk::gio::Menu::new();
        menu.append(Some(label), Some(if forward { "np.chapter-on" } else { "np.chapter-back" }));
        let pop = gtk::PopoverMenu::from_model(Some(&menu));
        pop.set_parent(w);
        let (p, pop2) = (app.playback().clone(), pop.clone());
        let g = gtk::GestureClick::builder().button(3).build();
        g.connect_pressed(move |_, _, _, _| {
            if p.current().is_some_and(|s| !s.chapters.is_empty()) {
                pop2.popup();
            }
        });
        w.add_controller(g);
    }
    let group = gtk::gio::SimpleActionGroup::new();
    for (name, forward) in [("chapter-on", true), ("chapter-back", false)] {
        let act = gtk::gio::SimpleAction::new(name, None);
        let p = app.playback().clone();
        act.connect_activate(move |_, _| if forward { p.chapter_on() } else { p.chapter_back() });
        group.add_action(&act);
    }
    b.insert_action_group("np", Some(&group));
    for (button, kind) in [(&fav, store::FAV), (&later, store::LATER)] {
        let (a, p) = (app.clone(), app.playback().clone());
        button.connect_clicked(move |_| {
            let Some(s) = p.current() else { return };
            let on = a.marks.borrow().has(kind, s.id);
            a.mark(kind, &[s.id], !on);
        });
    }
    // A double click on the song goes to its album, book or show.
    let twice = gtk::GestureClick::new();
    let p = app.playback().clone();
    twice.connect_pressed(move |g, n, _, _| {
        if let (2, Some(s)) = (n, p.current()) {
            let _ = g.widget().and_then(|w| w.activate_action("win.go-album", Some(&s.id.to_variant())).ok());
        }
    });
    text.add_controller(twice);
    outer.append(&b);

    let pb = app.playback().clone();
    let p = pb.clone();
    prev.connect_clicked(move |_| p.previous(true));
    let p = pb.clone();
    play.connect_clicked(move |_| p.toggle());
    let p = pb.clone();
    next.connect_clicked(move |_| p.next(true));
    let p = pb.clone();
    repeat.connect_clicked(move |_| p.cycle_repeat());
    let p = pb.clone();
    back.connect_clicked(move |_| p.seek_back());
    let p = pb.clone();
    on.connect_clicked(move |_| p.seek_forward());
    // Set from the player, the scale's moves are not the user's.
    let quiet = Rc::new(Cell::new(false));
    let (p, q) = (pb.clone(), quiet.clone());
    shuffle.connect_toggled(move |t| {
        if !q.get() && t.is_active() != p.shuffle() {
            p.toggle_shuffle();
        }
    });
    let p = pb.clone();
    scale.connect_change_value(move |_, _, v| {
        p.seek((v * 1000.0) as i64);
        gtk::glib::Propagation::Proceed
    });

    let update_position = {
        let (pb, scale, pos, dur, quiet) = (pb.clone(), scale.clone(), pos.clone(), dur.clone(), quiet.clone());
        move || {
            let d = pb.duration_ms();
            quiet.set(true);
            scale.set_range(0.0, (d.max(1) as f64) / 1000.0);
            scale.set_value(pb.position_ms() as f64 / 1000.0);
            quiet.set(false);
            pos.set_label(&clock(pb.position_ms()));
            dur.set_label(&if d > 0 { clock(d) } else { String::new() });
        }
    };
    let marks = {
        let (app, pb) = (app.clone(), pb.clone());
        move || {
            let m = app.marks.borrow();
            let id = pb.current().map_or(0, |s| s.id);
            let (f, l) = (m.has(store::FAV, id), m.has(store::LATER, id));
            fav.set_icon_name(if f { "starred-symbolic" } else { "non-starred-symbolic" });
            fav.set_tooltip_text(Some(if f { "Remove from Favorites" } else { "Add to Favorites" }));
            later.set_tooltip_text(Some(if l { "Remove from Listen Later" } else { "Listen Later" }));
            for (b, on) in [(&fav, f), (&later, l)] {
                if on { b.add_css_class("accent") } else { b.remove_css_class("accent") }
            }
        }
    };
    let marks = Rc::new(marks);
    // The line under the title: the chapter playing, else the bookmarks; who
    // and from what; the sleep timer.
    let line = {
        let (app, pb) = (app.clone(), pb.clone());
        Rc::new(move || artist.set_label(&subtitle(&app, &pb)))
    };
    let update = {
        let marks = marks.clone();
        let (app, pb, b, art, title, play, shuffle, repeat, quiet, up) = (
            app.clone(),
            pb.clone(),
            b.clone(),
            art,
            title,
            play.clone(),
            shuffle,
            repeat,
            quiet,
            update_position.clone(),
        );
        let (details, back, on, mark, line) = (details, back, on, mark, line.clone());
        move || {
            let cur = pb.current();
            b.set_sensitive(cur.is_some());
            art.set_child(Some(&cover(&app, cur.as_ref(), 48)));
            title.set_label(cur.as_ref().map_or("Nothing playing", |s| s.title.as_str()));
            line();
            let spoken = cur.as_ref().is_some_and(|s| s.spoken());
            let long = cur.as_ref().is_some_and(|s| pb.is_long(s));
            details.set_visible(spoken);
            shuffle.set_visible(!spoken);
            for w in [back.upcast_ref::<gtk::Widget>(), on.upcast_ref(), mark.upcast_ref()] {
                w.set_visible(long);
            }
            let settings = app.settings.borrow();
            back.set_tooltip_text(Some(&format!("Back {} seconds", synced::SEEK_BACK.get(&settings))));
            on.set_tooltip_text(Some(&format!("Forward {} seconds", synced::SEEK_FORWARD.get(&settings))));
            drop(settings);
            let playing = pb.is_playing();
            play.set_icon_name(if playing { "media-playback-pause-symbolic" } else { "media-playback-start-symbolic" });
            play.set_tooltip_text(Some(if playing { "Pause (Space)" } else { "Play (Space)" }));
            quiet.set(true);
            shuffle.set_active(pb.shuffle());
            quiet.set(false);
            let mode = pb.repeat_mode();
            repeat.set_icon_name(if mode == "song" {
                "media-playlist-repeat-song-symbolic"
            } else {
                "media-playlist-repeat-symbolic"
            });
            repeat.set_tooltip_text(Some(match mode.as_str() {
                "queue" => "Repeat: the queue",
                "song" => "Repeat: this song",
                _ => "Repeat: off",
            }));
            if mode == "off" {
                repeat.remove_css_class("accent")
            } else {
                repeat.add_css_class("accent")
            }
            marks();
            up();
        }
    };
    update();
    let line2 = line.clone();
    app.listen(move |c| {
        match c {
            Change::Playback | Change::Queues | Change::Thumbs => update(),
            Change::Position => {
                update_position();
                line2();
            }
            Change::Marks => {
                marks();
                line2();
            }
            _ => {}
        }
        true
    });
    outer
}

/// "Chapter 3 of 20 · Name · Author · Book", "Artist · Album · Sleep in 12 min".
fn subtitle(app: &App, pb: &crate::playback::Playback) -> String {
    let Some(s) = pb.current() else { return String::new() };
    if pb.waiting() {
        return "Waiting for the network…".into();
    }
    let marks = app.marks.borrow().bookmarks.iter().filter(|b| b.song == s.id).count();
    let mut parts = vec![crate::controls::chapter_label(&s, pb.position_ms()).unwrap_or_else(|| match marks {
        0 => String::new(),
        1 => "1 bookmark".into(),
        n => format!("{n} bookmarks"),
    })];
    match s.kind.as_str() {
        dhun_core::model::PODCAST => parts.extend([s.group_title.clone(), s.day()]),
        // A book of one file is often titled as the book: not twice.
        dhun_core::model::AUDIOBOOK => parts.extend([
            if s.group_title == s.title { String::new() } else { s.group_title.clone() },
            if s.artist.is_empty() { s.album_artist.clone() } else { s.artist.clone() },
        ]),
        _ => parts.extend([s.display_artist().to_string(), s.album.clone()]),
    }
    if let Some(l) = pb.sleep_label() {
        parts.push(format!("Sleep {l}"));
    }
    parts.retain(|p| !p.is_empty());
    parts.join(" · ")
}

/// "Continue from Pixel": what another device played last (plan 017).
fn handoff(app: &Rc<App>) -> gtk::Revealer {
    let label = gtk::Label::builder().xalign(0.0).hexpand(true).ellipsize(gtk::pango::EllipsizeMode::End).build();
    let go = gtk::Button::builder().label("Continue").css_classes(["pill", "suggested-action"]).build();
    let close = gtk::Button::builder()
        .icon_name("window-close-symbolic")
        .css_classes(["flat", "circular"])
        .tooltip_text("Dismiss")
        .build();
    let b = gtk::Box::builder().spacing(12).margin_start(12).margin_end(12).margin_top(6).margin_bottom(6).build();
    b.append(&gtk::Image::from_icon_name("phone-symbolic"));
    b.append(&label);
    b.append(&go);
    b.append(&close);
    let r = gtk::Revealer::builder().child(&b).css_classes(["toolbar"]).build();
    let pb = app.playback().clone();
    let p = pb.clone();
    go.connect_clicked(move |_| {
        if let Some(np) = p.handoff() {
            p.continue_from(&np);
        }
    });
    let p = pb.clone();
    close.connect_clicked(move |_| {
        if let Some(np) = p.handoff() {
            p.dismiss_handoff(&np);
        }
    });
    let update = {
        let (app, r) = (app.clone(), r.clone());
        move || match pb.handoff() {
            Some(np) => {
                let song = app.catalog().by_id.get(&np.song).map(|s| s.title.clone()).unwrap_or_default();
                let device = if np.device_name.is_empty() { "another device".into() } else { np.device_name.clone() };
                label.set_label(&format!("Continue from {device}: “{song}” at {}", clock(pb.place_of(&np))));
                r.set_reveal_child(true);
            }
            None => r.set_reveal_child(false),
        }
    };
    update();
    app.listen(move |c| {
        if matches!(c, Change::Handoff | Change::Playback | Change::Queues | Change::Catalog) {
            update();
        }
        true
    });
    r
}

/// "Resume at 12:34?" for a long file, when its place is asked about rather than gone back to (plan 009).
fn resume_offer(app: &Rc<App>) -> gtk::Revealer {
    let label = gtk::Label::builder().xalign(0.0).hexpand(true).build();
    let yes = gtk::Button::builder().label("Resume").css_classes(["pill", "suggested-action"]).build();
    let no = gtk::Button::builder().label("From Here").css_classes(["pill"]).build();
    let b = gtk::Box::builder().spacing(12).margin_start(12).margin_end(12).margin_top(6).margin_bottom(6).build();
    b.append(&label);
    b.append(&no);
    b.append(&yes);
    let r = gtk::Revealer::builder().child(&b).build();
    let pb = app.playback().clone();
    let p = pb.clone();
    yes.connect_clicked(move |_| p.answer_resume(true));
    let p = pb.clone();
    no.connect_clicked(move |_| p.answer_resume(false));
    let rr = r.clone();
    app.listen(move |c| {
        if c == Change::Playback {
            match pb.offer_resume() {
                Some((s, ms)) => {
                    label.set_label(&format!("Resume “{}” at {}?", s.title, clock(ms)));
                    rr.set_reveal_child(true);
                }
                None => rr.set_reveal_child(false),
            }
        }
        true
    });
    r
}

/// The playing queue, beside the page: its songs, the playing one marked.
pub fn queue_panel(app: &Rc<App>) -> gtk::Box {
    let b = gtk::Box::builder().orientation(gtk::Orientation::Vertical).width_request(300).build();
    let head = gtk::Box::builder().spacing(6).margin_start(12).margin_end(6).margin_top(10).margin_bottom(6).build();
    let name = gtk::Label::builder()
        .xalign(0.0)
        .hexpand(true)
        .ellipsize(gtk::pango::EllipsizeMode::End)
        .css_classes(["heading"])
        .build();
    head.append(&name);
    head.append(&picker(app));
    b.append(&head);
    let holder = adw::Bin::builder().vexpand(true).build();
    b.append(&holder);
    let pb = app.playback().clone();
    let update = {
        let (app, holder) = (app.clone(), holder.clone());
        move || {
            let q = pb.active();
            name.set_label(&q.as_ref().map_or("No queue".to_string(), |q| q.name.clone()));
            let songs = app.catalog().songs_of(&pb.items());
            let current = pb.current().map(|s| s.id);
            let left: i64 = current
                .and_then(|c| songs.iter().position(|s| s.id == c))
                .map_or(0, |i| songs[i..].iter().map(|s| s.duration_ms).sum::<i64>() - pb.position_ms());
            let (a, p) = (app.clone(), pb.clone());
            let list = widgets::list(
                &app,
                &songs.iter().cloned().enumerate().collect::<Vec<_>>(),
                move |(_, s)| {
                    let r = row(&cover(&a, Some(s), 32), &s.title, s.display_artist());
                    if Some(s.id) == current {
                        r.add_css_class("accent");
                        r.append(&gtk::Image::from_icon_name("audio-volume-high-symbolic"));
                    }
                    r.upcast()
                },
                move |(i, _)| p.play_at(*i),
            );
            let scroll = widgets::scrolled(&list);
            let col = gtk::Box::builder().orientation(gtk::Orientation::Vertical).build();
            let what = format!("{} · {} left", widgets::count(songs.len()), clock(left.max(0)));
            col.append(
                &gtk::Label::builder()
                    .label(what)
                    .xalign(0.0)
                    .margin_start(12)
                    .margin_bottom(6)
                    .css_classes(["caption", "dim-label"])
                    .build(),
            );
            col.append(&scroll);
            holder.set_child(Some(&col));
        }
    };
    update();
    app.listen(move |c| {
        if matches!(c, Change::Playback | Change::Queues | Change::Catalog) {
            update();
        }
        true
    });
    b
}

/// The queues, to switch between: numbered from 1 in the user's order, as in Musicolet (plan 020).
pub fn picker(app: &Rc<App>) -> gtk::MenuButton {
    let list =
        gtk::ListBox::builder().css_classes(["navigation-sidebar"]).selection_mode(gtk::SelectionMode::None).build();
    let pop = gtk::Popover::builder()
        .child(
            &gtk::ScrolledWindow::builder()
                .child(&list)
                .propagate_natural_height(true)
                .max_content_height(420)
                .min_content_width(260)
                .build(),
        )
        .build();
    let button = gtk::MenuButton::builder()
        .icon_name("view-more-symbolic")
        .tooltip_text("Switch queue")
        .popover(&pop)
        .css_classes(["flat"])
        .build();
    let pb = app.playback().clone();
    let ids: Rc<std::cell::RefCell<Vec<String>>> = Rc::default();
    let (p, i, popover) = (pb.clone(), ids.clone(), pop.clone());
    list.connect_row_activated(move |_, r| {
        let id = i.borrow().get(r.index() as usize).cloned();
        if let Some(id) = id {
            popover.popdown();
            p.switch_to(&id);
        }
    });
    pop.connect_show(move |_| {
        list.remove_all();
        let active = pb.active_id();
        let mut v = ids.borrow_mut();
        v.clear();
        for (n, q) in pb.ordered().into_iter().enumerate() {
            let l = gtk::Label::builder()
                .label(format!("{}. {}", n + 1, q.name))
                .xalign(0.0)
                .ellipsize(gtk::pango::EllipsizeMode::End)
                .build();
            if q.id == active {
                l.add_css_class("accent");
            }
            list.append(&l);
            v.push(q.id);
        }
    });
    button
}
