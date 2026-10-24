//! A file opened from the file manager (plan 021), as the Mac's file
//! player: a small window of its own, through a second player. It makes no
//! queue and logs no listen; it stops when the window closes. Inside the
//! Flatpak the file arrives through the document portal, so the app needs
//! no access to the user's files.

use crate::widgets::clock;
use adw::prelude::*;
use gst::prelude::*;
use gstreamer as gst;
use gtk::{gdk, gio, glib};
use std::cell::Cell;
use std::rc::Rc;
use std::time::Duration;

/// Plays `file` in a window of its own. One file at a time, as on the phone:
/// a new one replaces the one playing rather than playing over it.
pub fn open(gapp: &adw::Application, file: &gio::File) {
    for w in gapp.windows().into_iter().filter(|w| w.widget_name() == "file") {
        w.close();
    }
    let name = file.basename().map_or_else(String::new, |p| p.to_string_lossy().into_owned());
    let window = adw::ApplicationWindow::builder()
        .application(gapp)
        .title(&name)
        .name("file")
        .default_width(400)
        .resizable(false)
        .build();
    let art = gtk::Image::builder().pixel_size(96).icon_name("audio-x-generic-symbolic").css_classes(["cover"]).build();
    let label = |text: &str, css: &[&str]| {
        gtk::Label::builder()
            .label(text)
            .xalign(0.0)
            .wrap(true)
            .lines(2)
            .ellipsize(gtk::pango::EllipsizeMode::End)
            .css_classes(css.to_vec())
            .build()
    };
    // Its name until the tags say better.
    let stem = name.rsplit_once('.').map_or(name.as_str(), |(s, _)| s);
    let title = label(stem, &["heading"]);
    let artist = label("", &["caption", "dim-label"]);
    let play = gtk::Button::builder()
        .icon_name("media-playback-pause-symbolic")
        .tooltip_text("Pause (Space)")
        .css_classes(["circular", "suggested-action"])
        .valign(gtk::Align::Center)
        .build();
    let scale = gtk::Scale::builder().hexpand(true).draw_value(false).build();
    scale.set_range(0.0, 1.0);
    let time = label("", &["caption", "numeric", "dim-label"]);
    let row = gtk::Box::builder().spacing(6).build();
    row.append(&play);
    row.append(&scale);
    let text = gtk::Box::builder().orientation(gtk::Orientation::Vertical).spacing(4).hexpand(true).build();
    for w in [title.upcast_ref::<gtk::Widget>(), artist.upcast_ref(), row.upcast_ref(), time.upcast_ref()] {
        text.append(w);
    }
    let b = gtk::Box::builder().spacing(14).margin_start(16).margin_end(16).margin_top(8).margin_bottom(16).build();
    b.append(&art);
    b.append(&text);
    let view = adw::ToolbarView::builder().content(&b).build();
    view.add_top_bar(&adw::HeaderBar::new());
    window.set_content(Some(&view));

    let player = gst::ElementFactory::make("playbin3").build().expect("GStreamer's playbin3");
    if let Ok(desc) = std::env::var("DHUN_AUDIO_SINK") {
        if let Ok(sink) = gst::parse::bin_from_description(&desc, true) {
            player.set_property("audio-sink", sink);
        }
    }
    player.set_property("uri", file.uri().as_str());
    let _ = player.set_state(gst::State::Playing);
    let playing = Rc::new(Cell::new(true));
    // Ended: Play starts it again.
    let ended = Rc::new(Cell::new(false));
    let show_playing = {
        let (play, playing) = (play.clone(), playing.clone());
        move |on: bool| {
            playing.set(on);
            play.set_icon_name(if on { "media-playback-pause-symbolic" } else { "media-playback-start-symbolic" });
            play.set_tooltip_text(Some(if on { "Pause (Space)" } else { "Play (Space)" }));
        }
    };
    let show_playing = Rc::new(show_playing);
    let toggle = {
        let (p, playing, ended, show) = (player.clone(), playing.clone(), ended.clone(), show_playing.clone());
        move || {
            if ended.replace(false) {
                let _ = p.seek_simple(gst::SeekFlags::FLUSH, gst::ClockTime::ZERO);
            }
            let on = !playing.get();
            let _ = p.set_state(if on { gst::State::Playing } else { gst::State::Paused });
            show(on);
        }
    };
    let toggle = Rc::new(toggle);
    let t = toggle.clone();
    play.connect_clicked(move |_| t());
    let keys = gtk::EventControllerKey::new();
    let t = toggle.clone();
    keys.connect_key_pressed(move |_, key, _, _| {
        if key == gdk::Key::space {
            t();
            return glib::Propagation::Stop;
        }
        glib::Propagation::Proceed
    });
    window.add_controller(keys);
    let (p, e) = (player.clone(), ended.clone());
    scale.connect_change_value(move |_, _, v| {
        e.set(false);
        let ms = (v.max(0.0) * 1000.0) as u64;
        let _ = p.seek_simple(gst::SeekFlags::FLUSH | gst::SeekFlags::ACCURATE, gst::ClockTime::from_mseconds(ms));
        glib::Propagation::Proceed
    });
    let bus = player.bus().unwrap();
    let (a, t, ar, show, e) = (art.clone(), title.clone(), artist.clone(), show_playing.clone(), ended.clone());
    let watch = bus
        .add_watch_local(move |_, msg| {
            match msg.view() {
                gst::MessageView::Tag(tag) => {
                    let tags = tag.tags();
                    if let Some(v) = tags.get::<gst::tags::Title>().filter(|v| !v.get().is_empty()) {
                        t.set_label(v.get());
                    }
                    if let Some(v) = tags.get::<gst::tags::Artist>() {
                        ar.set_label(v.get());
                    }
                    let image = tags.get::<gst::tags::Image>().or_else(|| tags.get::<gst::tags::PreviewImage>());
                    let bytes =
                        image.and_then(|i| i.get().buffer_owned()).and_then(|b| b.into_mapped_buffer_readable().ok());
                    if let Some(tex) =
                        bytes.and_then(|b| gdk::Texture::from_bytes(&glib::Bytes::from(b.as_slice())).ok())
                    {
                        a.set_paintable(Some(&tex));
                    }
                }
                gst::MessageView::Eos(_) => {
                    e.set(true);
                    show(false);
                }
                // A playlist arrives as audio too: said in the artist's place.
                gst::MessageView::Error(_) => {
                    ar.set_label("Cannot play this file");
                    show(false);
                }
                _ => {}
            }
            glib::ControlFlow::Continue
        })
        .expect("a bus watch");
    let (p, s) = (player.clone(), scale.clone());
    let tick = glib::timeout_add_local(Duration::from_millis(250), move || {
        let pos = p.query_position::<gst::ClockTime>().map_or(0, |c| c.mseconds() as i64);
        let dur = p.query_duration::<gst::ClockTime>().map_or(0, |c| c.mseconds() as i64);
        s.set_range(0.0, dur.max(1) as f64 / 1000.0);
        s.set_value(pos as f64 / 1000.0);
        time.set_label(&format!("{} / {}", clock(pos), clock(dur)));
        glib::ControlFlow::Continue
    });
    // Closing stops it, whatever closed it.
    let held = std::cell::RefCell::new(Some((watch, tick)));
    window.connect_close_request(move |_| {
        let _ = player.set_state(gst::State::Null);
        if let Some((_watch, tick)) = held.take() {
            tick.remove();
        }
        glib::Propagation::Proceed
    });
    window.present();
}
