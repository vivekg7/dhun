//! The mini window (plan 039), the Mac's floating mini player and the
//! phone's pill: the cover, the song and the controls, small. A Wayland
//! window cannot keep itself above the others, so the owner pins it with
//! the desktop's own "Always on top"; with no title bar, any part of it
//! moves it, and a right-click there opens that window menu.

use crate::app::{App, Change};
use crate::widgets::cover;
use adw::prelude::*;
use std::rc::Rc;

/// Shows the mini window, the one already open if there is one.
pub fn open(gapp: &adw::Application, app: &Rc<App>) {
    if let Some(w) = gapp.windows().into_iter().find(|w| w.widget_name() == "mini") {
        w.present();
        return;
    }
    let window =
        gtk::Window::builder().application(gapp).title("Dhun").name("mini").resizable(false).decorated(false).build();
    let b = gtk::Box::builder().spacing(10).margin_start(10).margin_end(6).margin_top(10).margin_bottom(10).build();
    let art = adw::Bin::new();
    let text = gtk::Box::builder().orientation(gtk::Orientation::Vertical).width_request(190).spacing(2).build();
    let label = |css: &[&str]| {
        gtk::Label::builder()
            .xalign(0.0)
            .max_width_chars(24)
            .ellipsize(gtk::pango::EllipsizeMode::End)
            .css_classes(css.to_vec())
            .build()
    };
    let (title, artist) = (label(&["heading"]), label(&["caption", "dim-label"]));
    let button = |icon: &str, tip: &str| {
        gtk::Button::builder().icon_name(icon).tooltip_text(tip).css_classes(["flat", "circular"]).build()
    };
    let (prev, play, next) = (
        button("media-skip-backward-symbolic", "Previous"),
        button("media-playback-start-symbolic", "Play"),
        button("media-skip-forward-symbolic", "Next"),
    );
    let controls = gtk::Box::builder().spacing(2).build();
    for w in [&prev, &play, &next] {
        controls.append(w);
    }
    text.append(&title);
    text.append(&artist);
    text.append(&controls);
    let close = button("window-close-symbolic", "Close");
    close.set_valign(gtk::Align::Start);
    let w = window.downgrade();
    close.connect_clicked(move |_| {
        if let Some(w) = w.upgrade() {
            w.close();
        }
    });
    b.append(&art);
    b.append(&text);
    b.append(&close);
    let pb = app.playback().clone();
    let p = pb.clone();
    prev.connect_clicked(move |_| p.previous(true));
    let p = pb.clone();
    play.connect_clicked(move |_| p.toggle());
    let p = pb.clone();
    next.connect_clicked(move |_| p.next(true));
    // A double click on the song shows the main window.
    let twice = gtk::GestureClick::new();
    let g = gapp.clone();
    twice.connect_pressed(move |_, n, _, _| {
        if n == 2 {
            g.activate();
        }
    });
    text.add_controller(twice);
    let update = {
        let (app, art) = (app.clone(), art.downgrade());
        move || -> bool {
            let Some(art) = art.upgrade() else { return false };
            let cur = pb.current();
            art.set_child(Some(&cover(&app, cur.as_ref(), 56)));
            title.set_label(cur.as_ref().map_or("Nothing playing", |s| s.title.as_str()));
            artist.set_label(cur.as_ref().map_or("", |s| s.display_artist()));
            let playing = pb.is_playing();
            play.set_icon_name(if playing { "media-playback-pause-symbolic" } else { "media-playback-start-symbolic" });
            play.set_tooltip_text(Some(if playing { "Pause" } else { "Play" }));
            for w in [&prev, &next] {
                w.set_sensitive(cur.is_some());
            }
            true
        }
    };
    update();
    app.listen(move |c| match c {
        Change::Playback | Change::Thumbs => update(),
        _ => true,
    });
    let handle = gtk::WindowHandle::builder().child(&b).build();
    window.set_child(Some(&handle));
    window.present();
}
