//! The Now playing bar's popovers, as the Mac's: speed and pitch (plan
//! 016), the sleep timer (014), and a long file's bookmarks, chapters and
//! notes (031, 033).

use crate::app::{App, Change};
use crate::engine::Engine;
use crate::widgets::clock;
use adw::prelude::*;
use dhun_core::model::{self, Bookmark, Song};
use dhun_core::store;
use dhun_core::tempo::{self, Tempo};
use gtk::glib;
use std::cell::Cell;
use std::rc::Rc;

fn popover_box() -> gtk::Box {
    gtk::Box::builder()
        .orientation(gtk::Orientation::Vertical)
        .spacing(10)
        .margin_start(12)
        .margin_end(12)
        .margin_top(12)
        .margin_bottom(12)
        .build()
}

fn heading(text: &str) -> gtk::Label {
    gtk::Label::builder().label(text).xalign(0.0).css_classes(["heading"]).build()
}

fn caption(text: &str) -> gtk::Label {
    gtk::Label::builder()
        .label(text)
        .xalign(0.0)
        .wrap(true)
        .max_width_chars(36)
        .css_classes(["caption", "dim-label"])
        .build()
}

/// A button on the bar whose popover is built as it opens, for the song playing then.
fn popover_button(icon: &str, tip: &str, panel: impl Fn(&gtk::Popover) -> gtk::Widget + 'static) -> gtk::MenuButton {
    let pop = gtk::Popover::new();
    pop.connect_show(move |pop| pop.set_child(Some(&panel(pop))));
    gtk::MenuButton::builder()
        .icon_name(icon)
        .tooltip_text(tip)
        .popover(&pop)
        .css_classes(["flat", "circular"])
        .valign(gtk::Align::Center)
        .build()
}

/// Closes `pop` once a choice in it is made.
fn closer(pop: &gtk::Popover) -> impl Fn() + Clone + 'static {
    let pop = pop.downgrade();
    move || {
        if let Some(p) = pop.upgrade() {
            p.popdown();
        }
    }
}

/// Speed and pitch for the song playing, or as this computer's everyday one.
pub fn speed_button(app: &Rc<App>) -> gtk::MenuButton {
    let p = app.playback().clone();
    let button = popover_button("power-profile-performance-symbolic", "Speed and pitch", move |_| speed_panel(&p));
    let (b, p) = (button.clone(), app.playback().clone());
    let update = move || match p.tempo().label() {
        Some(l) => {
            b.set_label(&l);
            b.add_css_class("accent");
        }
        None => {
            b.set_icon_name("power-profile-performance-symbolic");
            b.remove_css_class("accent");
        }
    };
    update();
    app.listen(move |c| {
        if matches!(c, Change::Playback | Change::Queues) {
            update();
        }
        true
    });
    button
}

fn speed_panel(pb: &Rc<crate::playback::Playback>) -> gtk::Widget {
    let b = popover_box();
    b.set_width_request(300);
    let t = pb.tempo();
    let only = Rc::new(Cell::new(pb.song_tempo().is_some()));
    let title = heading(&format!("Speed {}×", tempo::format(t.speed)));
    b.append(&title);
    let scale = gtk::Scale::with_range(gtk::Orientation::Horizontal, Tempo::MIN_SPEED, Tempo::MAX_SPEED, 0.05);
    scale.set_value(t.speed);
    scale.set_draw_value(false);
    for v in [0.5, 1.0, 1.5, 2.0] {
        scale.add_mark(v, gtk::PositionType::Bottom, None);
    }
    b.append(&scale);
    let set = {
        let (pb, only) = (pb.clone(), only.clone());
        move |t: Tempo| pb.set_tempo(t, only.get())
    };
    // The speed under the pointer shows at once, and is applied, and synced, once it rests.
    let pending: Rc<Cell<Option<glib::SourceId>>> = Rc::default();
    let (s, p) = (set.clone(), pb.clone());
    scale.connect_value_changed(move |sc| {
        let v = (sc.value() * 20.0).round() / 20.0;
        title.set_label(&format!("Speed {}×", tempo::format(v)));
        if let Some(id) = pending.take() {
            id.remove();
        }
        let (s, p, pending2) = (s.clone(), p.clone(), pending.clone());
        pending.set(Some(glib::timeout_add_local_once(std::time::Duration::from_millis(350), move || {
            pending2.take();
            if p.tempo().speed != v {
                s(Tempo { speed: v, semitones: p.tempo().semitones });
            }
        })));
    });
    let presets = gtk::Box::builder().spacing(6).homogeneous(true).build();
    for v in [0.75, 1.0, 1.25, 1.5, 2.0] {
        let btn = gtk::Button::builder().label(format!("{}×", tempo::format(v))).css_classes(["pill", "small"]).build();
        let sc = scale.clone();
        btn.connect_clicked(move |_| sc.set_value(v));
        presets.append(&btn);
    }
    b.append(&presets);
    if Engine::can_pitch() {
        let row = gtk::Box::builder().spacing(8).build();
        row.append(&gtk::Label::builder().label("Pitch, in semitones").xalign(0.0).hexpand(true).build());
        let spin = gtk::SpinButton::with_range(-Tempo::MAX_SEMITONES as f64, Tempo::MAX_SEMITONES as f64, 1.0);
        spin.set_value(t.semitones as f64);
        let (s, p) = (set.clone(), pb.clone());
        spin.connect_value_changed(move |sp| {
            let semitones = sp.value() as i64;
            if p.tempo().semitones != semitones {
                s(Tempo { speed: p.tempo().speed, semitones });
            }
        });
        row.append(&spin);
        b.append(&row);
    }
    // Off, an episode or a book's file takes its collection's speed, which
    // follows the user (plan 031); a song takes this computer's.
    let kind = pb.current().map(|s| s.kind.clone()).unwrap_or_default();
    let (one, every) = match kind.as_str() {
        model::PODCAST => ("episode", "For every podcast, on all your devices."),
        model::AUDIOBOOK => ("file", "For every audiobook, on all your devices."),
        _ => ("song", "For every song on this computer without its own."),
    };
    let row = gtk::Box::builder().spacing(8).build();
    row.append(&gtk::Label::builder().label(format!("Only for this {one}")).xalign(0.0).hexpand(true).build());
    let switch =
        gtk::Switch::builder().active(only.get()).valign(gtk::Align::Center).sensitive(pb.current().is_some()).build();
    row.append(&switch);
    b.append(&row);
    let own = format!("This {one} keeps its own, on all your devices.");
    let note = caption(if only.get() { own.as_str() } else { every });
    b.append(&note);
    let (p, o, n) = (pb.clone(), only, note);
    switch.connect_active_notify(move |sw| {
        let on = sw.is_active();
        o.set(on);
        if on {
            p.set_tempo(p.tempo(), true);
            n.set_label(&own);
        } else {
            p.clear_song_tempo();
            n.set_label(every);
        }
    });
    let reset = gtk::Button::builder().label("Reset").halign(gtk::Align::Start).sensitive(t != Tempo::NORMAL).build();
    let pop = b.clone();
    reset.connect_clicked(move |_| {
        set(Tempo::NORMAL);
        if let Some(p) = pop.ancestor(gtk::Popover::static_type()).and_downcast::<gtk::Popover>() {
            p.popdown();
        }
    });
    b.append(&reset);
    b.upcast()
}

/// The sleep timer.
pub fn sleep_button(app: &Rc<App>) -> gtk::MenuButton {
    let a = app.clone();
    let button = popover_button("weather-clear-night-symbolic", "Sleep timer", move |pop| sleep_panel(&a, pop));
    let (b, p) = (button.clone(), app.playback().clone());
    let update = move || {
        let label = p.sleep_label();
        b.set_tooltip_text(Some(&label.as_ref().map_or("Sleep timer".into(), |l| format!("Sleep {l}"))));
        if label.is_some() { b.add_css_class("accent") } else { b.remove_css_class("accent") }
    };
    update();
    app.listen(move |c| {
        if c == Change::Playback {
            update();
        }
        true
    });
    button
}

fn sleep_panel(app: &Rc<App>, pop: &gtk::Popover) -> gtk::Widget {
    let pb = app.playback().clone();
    let b = popover_box();
    b.set_width_request(260);
    b.append(&heading(&pb.sleep_label().map_or("Sleep timer".into(), |l| format!("Stops {l}"))));
    let done = closer(pop);
    // Remembered on this computer, as on the phone (plan 034).
    let finish =
        gtk::CheckButton::builder().label("Let the song finish").active(app.prefs.get().sleep_finish_song).build();
    let a = app.clone();
    finish.connect_toggled(move |c| a.prefs.update(|p| p.sleep_finish_song = c.is_active()));
    let presets = gtk::Box::builder().spacing(6).homogeneous(true).build();
    for m in [15, 30, 45, 60] {
        let btn = gtk::Button::builder().label(format!("{m} min")).css_classes(["pill", "small"]).build();
        let (p, f, d) = (pb.clone(), finish.clone(), done.clone());
        btn.connect_clicked(move |_| {
            p.sleep_minutes(m, f.is_active());
            d();
        });
        presets.append(&btn);
    }
    b.append(&presets);
    let start_row = |label: &str, min: f64, max: f64, value: f64, go: Box<dyn Fn(i64)>| {
        let row = gtk::Box::builder().spacing(8).build();
        let spin = gtk::SpinButton::with_range(min, max, 1.0);
        spin.set_value(value);
        row.append(&spin);
        row.append(&gtk::Label::builder().label(label).xalign(0.0).hexpand(true).build());
        let start = gtk::Button::with_label("Start");
        let d = done.clone();
        start.connect_clicked(move |_| {
            go(spin.value() as i64);
            d();
        });
        row.append(&start);
        row
    };
    let (p, f) = (pb.clone(), finish.clone());
    b.append(&start_row("minutes", 1.0, 600.0, 20.0, Box::new(move |m| p.sleep_minutes(m, f.is_active()))));
    b.append(&finish);
    b.append(&gtk::Separator::new(gtk::Orientation::Horizontal));
    let item = |label: &str, go: Box<dyn Fn()>| {
        let btn = gtk::Button::builder().label(label).css_classes(["flat"]).build();
        btn.child().and_downcast::<gtk::Label>().inspect(|l| l.set_xalign(0.0));
        let d = done.clone();
        btn.connect_clicked(move |_| {
            go();
            d();
        });
        btn
    };
    let p = pb.clone();
    b.append(&item("At the end of this song", Box::new(move || p.sleep_end_of_song())));
    let p = pb.clone();
    b.append(&start_row("songs", 2.0, 100.0, 3.0, Box::new(move |n| p.sleep_songs(n as usize))));
    let p = pb.clone();
    b.append(&item("At the end of the queue", Box::new(move || p.sleep_end_of_queue())));
    if pb.sleep().is_some() {
        b.append(&gtk::Separator::new(gtk::Orientation::Horizontal));
        let p = pb.clone();
        let off = item("Turn Off", Box::new(move || p.sleep_cancel()));
        off.add_css_class("destructive-action");
        b.append(&off);
    }
    b.set_sensitive(pb.current().is_some());
    b.upcast()
}

/// Marks the place playing in a long file (plan 033), and says where.
pub fn add_bookmark(app: &Rc<App>) {
    let pb = app.playback();
    let Some(s) = pb.current() else { return };
    let ms = pb.position_ms();
    let b = Bookmark { id: store::uuid(), song: s.id, position_ms: ms, ..Default::default() };
    app.store(|db| store::bookmark(db, &b));
    app.reload_marks();
    pb.notice(&format!("Bookmark at {}", clock(ms)));
}

/// "Chapter 2 of 20 · Name" for the place in `s`; None without chapters.
pub fn chapter_label(s: &Song, pos: i64) -> Option<String> {
    if s.chapters.is_empty() {
        return None;
    }
    let i = s.chapters.iter().rposition(|c| c.start_ms <= pos).unwrap_or(0);
    let name = &s.chapters[i].title;
    let n = s.chapters.len();
    Some(if name.is_empty() {
        format!("Chapter {} of {n}", i + 1)
    } else {
        format!("Chapter {} of {n} · {name}", i + 1)
    })
}

/// A long file's bookmarks and chapters, each a click away, and its notes.
pub fn details_button(app: &Rc<App>) -> gtk::MenuButton {
    let a = app.clone();
    popover_button("view-list-bullet-symbolic", "Chapters and notes", move |pop| details_panel(&a, pop))
}

fn details_panel(app: &Rc<App>, pop: &gtk::Popover) -> gtk::Widget {
    let pb = app.playback().clone();
    let Some(song) = pb.current() else { return gtk::Label::new(None).upcast() };
    let b = popover_box();
    b.set_spacing(4);
    b.append(&gtk::Label::builder().label(&song.title).xalign(0.0).wrap(true).css_classes(["heading"]).build());
    let close = closer(pop);
    let place_row = |at: i64, text: &str, under: &str, current: bool| {
        let row = gtk::Box::builder().spacing(10).build();
        row.append(
            &gtk::Label::builder()
                .label(clock(at))
                .css_classes(["numeric", "dim-label"])
                .width_chars(8)
                .xalign(0.0)
                .build(),
        );
        let words = gtk::Box::builder().orientation(gtk::Orientation::Vertical).hexpand(true).build();
        let l = gtk::Label::builder().label(text).xalign(0.0).wrap(true).width_chars(30).max_width_chars(40).build();
        if current {
            l.add_css_class("accent");
            l.add_css_class("heading");
        }
        words.append(&l);
        if !under.is_empty() {
            words.append(&gtk::Label::builder().label(under).xalign(0.0).css_classes(["caption", "dim-label"]).build());
        }
        row.append(&words);
        let btn = gtk::Button::builder().child(&row).css_classes(["flat"]).hexpand(true).build();
        let (p, c) = (pb.clone(), close.clone());
        btn.connect_clicked(move |_| {
            p.seek(at);
            c();
        });
        btn
    };
    let chapter_name = |i: usize| {
        let t = &song.chapters[i].title;
        if t.is_empty() { format!("Chapter {}", i + 1) } else { t.clone() }
    };
    let marks: Vec<Bookmark> = app.marks.borrow().bookmarks.iter().filter(|b| b.song == song.id).cloned().collect();
    if !marks.is_empty() {
        b.append(&caption("Bookmarks"));
        for m in marks {
            let at = song.chapters.iter().rposition(|c| c.start_ms <= m.position_ms);
            let chapter = at.map(chapter_name).unwrap_or_default();
            let (text, under) = if m.note.is_empty() {
                (if chapter.is_empty() { "Bookmark".to_string() } else { chapter }, String::new())
            } else {
                (m.note.clone(), chapter)
            };
            let line = gtk::Box::builder().spacing(4).build();
            line.append(&place_row(m.position_ms, &text, &under, false));
            line.append(&bookmark_menu(app, &m, &close));
            b.append(&line);
        }
    }
    if !song.chapters.is_empty() {
        b.append(&caption("Chapters"));
        let now = song.chapters.iter().rposition(|c| c.start_ms <= pb.position_ms());
        for (i, c) in song.chapters.iter().enumerate() {
            b.append(&place_row(c.start_ms, &chapter_name(i), "", Some(i) == now));
        }
    }
    if !song.notes.is_empty() {
        b.append(&caption("Notes"));
        b.append(
            &gtk::Label::builder()
                .label(&song.notes)
                .xalign(0.0)
                .wrap(true)
                .selectable(true)
                .max_width_chars(48)
                .build(),
        );
    }
    gtk::ScrolledWindow::builder()
        .child(&b)
        .propagate_natural_height(true)
        .max_content_height(480)
        .min_content_width(380)
        .hscrollbar_policy(gtk::PolicyType::Never)
        .build()
        .upcast()
}

/// A bookmark's note written or changed, or the bookmark deleted.
fn bookmark_menu(app: &Rc<App>, m: &Bookmark, close: &(impl Fn() + Clone + 'static)) -> gtk::MenuButton {
    let pop = gtk::Popover::new();
    let b = gtk::Box::builder().orientation(gtk::Orientation::Vertical).build();
    let note = gtk::Button::builder()
        .label(if m.note.is_empty() { "Add a Note…" } else { "Edit the Note…" })
        .css_classes(["flat"])
        .build();
    let delete = gtk::Button::builder().label("Delete").css_classes(["flat", "error"]).build();
    b.append(&note);
    b.append(&delete);
    pop.set_child(Some(&b));
    let button = gtk::MenuButton::builder()
        .icon_name("view-more-symbolic")
        .popover(&pop)
        .css_classes(["flat", "circular"])
        .valign(gtk::Align::Center)
        .build();
    let (a, mark, c) = (app.clone(), m.clone(), close.clone());
    delete.connect_clicked(move |_| {
        let gone = Bookmark { deleted: true, note: String::new(), ..mark.clone() };
        a.store(|db| store::bookmark(db, &gone));
        a.reload_marks();
        c();
    });
    let (a, mark, c, w) = (app.clone(), m.clone(), close.clone(), button.downgrade());
    note.connect_clicked(move |_| {
        let Some(parent) = w.upgrade().and_then(|w| w.root()).and_downcast::<gtk::Window>() else { return };
        c();
        let (a, mark) = (a.clone(), mark.clone());
        let (heading, before) = (format!("Note at {}", clock(mark.position_ms)), mark.note.clone());
        crate::widgets::ask_text(&parent, &heading, &before, "Save", model::MAX_NOTE, move |text| {
            let changed = Bookmark { note: text, ..mark.clone() };
            a.store(|db| store::bookmark(db, &changed));
            a.reload_marks();
        });
    });
    button
}
