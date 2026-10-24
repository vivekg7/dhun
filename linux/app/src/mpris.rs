//! MPRIS (plan 039): the media keys, the desktop's media controls (COSMIC,
//! GNOME, KDE) and Bluetooth headset buttons, over GIO's D-Bus. The Mac's
//! `MediaKeys`. As there, Next and Previous from outside the window obey
//! the long-file settings (plan 009).

use crate::app::{App, Change};
use dhun_core::covers;
use gtk::gio;
use gtk::glib::{self, Variant};
use gtk::prelude::*;
use std::collections::HashMap;
use std::rc::Rc;

const PATH: &str = "/org/mpris/MediaPlayer2";
const ROOT: &str = "org.mpris.MediaPlayer2";
const PLAYER: &str = "org.mpris.MediaPlayer2.Player";

const XML: &str = r#"<node>
  <interface name="org.mpris.MediaPlayer2">
    <method name="Raise"/>
    <method name="Quit"/>
    <property name="CanQuit" type="b" access="read"/>
    <property name="CanRaise" type="b" access="read"/>
    <property name="HasTrackList" type="b" access="read"/>
    <property name="Identity" type="s" access="read"/>
    <property name="DesktopEntry" type="s" access="read"/>
    <property name="SupportedUriSchemes" type="as" access="read"/>
    <property name="SupportedMimeTypes" type="as" access="read"/>
  </interface>
  <interface name="org.mpris.MediaPlayer2.Player">
    <method name="Next"/>
    <method name="Previous"/>
    <method name="Pause"/>
    <method name="PlayPause"/>
    <method name="Stop"/>
    <method name="Play"/>
    <method name="Seek"><arg direction="in" name="Offset" type="x"/></method>
    <method name="SetPosition"><arg direction="in" name="TrackId" type="o"/><arg direction="in" name="Position" type="x"/></method>
    <method name="OpenUri"><arg direction="in" name="Uri" type="s"/></method>
    <signal name="Seeked"><arg name="Position" type="x"/></signal>
    <property name="PlaybackStatus" type="s" access="read"/>
    <property name="LoopStatus" type="s" access="read"/>
    <property name="Rate" type="d" access="read"/>
    <property name="Shuffle" type="b" access="read"/>
    <property name="Metadata" type="a{sv}" access="read"/>
    <property name="Volume" type="d" access="read"/>
    <property name="Position" type="x" access="read"/>
    <property name="MinimumRate" type="d" access="read"/>
    <property name="MaximumRate" type="d" access="read"/>
    <property name="CanGoNext" type="b" access="read"/>
    <property name="CanGoPrevious" type="b" access="read"/>
    <property name="CanPlay" type="b" access="read"/>
    <property name="CanPause" type="b" access="read"/>
    <property name="CanSeek" type="b" access="read"/>
    <property name="CanControl" type="b" access="read"/>
  </interface>
</node>"#;

/// Takes the bus name and serves the player while the app runs.
pub fn start(gapp: &adw::Application, app: &Rc<App>) {
    let (gapp, app) = (gapp.clone(), app.clone());
    let owner = gio::bus_own_name(
        gio::BusType::Session,
        "org.mpris.MediaPlayer2.dhun",
        gio::BusNameOwnerFlags::NONE,
        move |conn, _| serve(&conn, &gapp, &app),
        |_, _| {},
        |_, _| {},
    );
    // Held for the app's life: the name is given up only by `bus_unown_name`.
    let _ = owner;
}

fn serve(conn: &gio::DBusConnection, gapp: &adw::Application, app: &Rc<App>) {
    let node = gio::DBusNodeInfo::for_xml(XML).expect("MPRIS introspection");
    for iface in [ROOT, PLAYER] {
        let info = node.lookup_interface(iface).unwrap();
        let (g, a) = (gapp.clone(), app.clone());
        let a2 = app.clone();
        let reg = conn
            .register_object(PATH, &info)
            .method_call(move |_, _, _, _, method, params, invocation| {
                call(&g, &a, method, &params);
                invocation.return_value(None);
            })
            .property(move |_, _, _, iface, name| property(&a2, iface, name))
            .build();
        // Served until the connection closes; nothing unregisters it.
        let _ = reg;
    }
    // Every change the controls show, said on the bus.
    let (c, a) = (conn.clone(), app.clone());
    let last_position = std::cell::Cell::new(0i64);
    app.listen(move |ch| {
        match ch {
            Change::Playback | Change::Queues => {
                let names = [
                    "PlaybackStatus",
                    "Metadata",
                    "Shuffle",
                    "LoopStatus",
                    "CanGoNext",
                    "CanGoPrevious",
                    "CanPlay",
                    "CanPause",
                    "CanSeek",
                ];
                let changed: HashMap<String, Variant> =
                    names.iter().map(|n| (n.to_string(), property(&a, PLAYER, n))).collect();
                let _ = c.emit_signal(
                    None,
                    PATH,
                    "org.freedesktop.DBus.Properties",
                    "PropertiesChanged",
                    Some(&(PLAYER, changed, Vec::<String>::new()).to_variant()),
                );
            }
            Change::Position => {
                // A jump the clock alone would not make: a seek, here or elsewhere.
                let pos = a.playback().position_ms();
                if (pos - last_position.get()).abs() > 1500 {
                    let _ = c.emit_signal(None, PATH, PLAYER, "Seeked", Some(&(pos * 1000,).to_variant()));
                }
                last_position.set(pos);
            }
            _ => {}
        }
        true
    });
}

fn call(gapp: &adw::Application, app: &Rc<App>, method: &str, params: &Variant) {
    let pb = app.playback();
    match method {
        // The main window, even when hidden; not the mini window or a file's.
        "Raise" => gapp.activate(),
        "Quit" => gapp.quit(),
        "Next" => pb.next(false),
        "Previous" => pb.previous(false),
        "Pause" | "Stop" => pb.pause(),
        "Play" => pb.play(),
        "PlayPause" => pb.toggle(),
        "Seek" => {
            if let Some((us,)) = params.get::<(i64,)>() {
                pb.seek(pb.position_ms() + us / 1000);
            }
        }
        "SetPosition" => {
            if let Some(us) = params.child_value(1).get::<i64>() {
                pb.seek(us / 1000);
            }
        }
        _ => {}
    }
}

fn property(app: &Rc<App>, iface: &str, name: &str) -> Variant {
    let pb = app.playback();
    let cur = pb.current();
    match (iface, name) {
        (ROOT, "CanQuit") | (ROOT, "CanRaise") => true.to_variant(),
        (ROOT, "HasTrackList") => false.to_variant(),
        (ROOT, "Identity") => "Dhun".to_variant(),
        (ROOT, "DesktopEntry") => "io.github.vivekg7.dhun".to_variant(),
        (ROOT, _) => Vec::<String>::new().to_variant(),
        (_, "PlaybackStatus") => match (&cur, pb.is_playing()) {
            (None, _) => "Stopped",
            (_, true) => "Playing",
            _ => "Paused",
        }
        .to_variant(),
        (_, "LoopStatus") => match pb.repeat_mode().as_str() {
            "queue" => "Playlist",
            "song" => "Track",
            _ => "None",
        }
        .to_variant(),
        (_, "Rate" | "Volume" | "MinimumRate" | "MaximumRate") => 1.0f64.to_variant(),
        (_, "Shuffle") => pb.shuffle().to_variant(),
        (_, "Position") => (pb.position_ms() * 1000).to_variant(),
        (_, "Metadata") => metadata(app),
        (_, "CanControl") => true.to_variant(),
        (_, _) => cur.is_some().to_variant(),
    }
}

fn metadata(app: &Rc<App>) -> Variant {
    let mut m: HashMap<String, Variant> = HashMap::new();
    let Some(s) = app.playback().current() else { return m.to_variant() };
    let id = glib::variant::ObjectPath::try_from(format!("/io/github/vivekg7/dhun/song/{}", s.id)).unwrap();
    m.insert("mpris:trackid".into(), id.to_variant());
    m.insert("mpris:length".into(), (app.playback().duration_ms() * 1000).to_variant());
    m.insert("xesam:title".into(), s.title.to_variant());
    m.insert("xesam:artist".into(), vec![s.display_artist().to_string()].to_variant());
    m.insert("xesam:album".into(), s.album.to_variant());
    // The cover as a file: kept already, or fetched now and sent with the next change.
    let art = app.catalog().art_of(&s).clone();
    if art.has_art {
        match app.files.kept(&covers::key(&art), 256) {
            Some(f) => {
                m.insert("mpris:artUrl".into(), format!("file://{}", f.display()).to_variant());
            }
            None => {
                let a = app.clone();
                app.cover(&art, 256, move |_| a.changed(Change::Playback));
            }
        }
    }
    m.to_variant()
}
