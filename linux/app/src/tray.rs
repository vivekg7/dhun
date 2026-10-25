//! The tray icon (plan 039): a StatusNotifierItem, which COSMIC and KDE
//! show (GNOME with its AppIndicator extension), and its menu over
//! `com.canonical.dbusmenu`, both on GIO's D-Bus. It is registered by the
//! connection's own name rather than a well-known one, as Chromium's is,
//! so the Flatpak needs only to talk to the watcher.

use crate::app::{App, Change};
use gtk::gio;
use gtk::glib::{self, Variant};
use gtk::prelude::*;
use std::cell::Cell;
use std::collections::HashMap;
use std::rc::Rc;

const ITEM: &str = "/StatusNotifierItem";
const MENU: &str = "/MenuBar";
const WATCHER: &str = "org.kde.StatusNotifierWatcher";

const XML: &str = r#"<node>
  <interface name="org.kde.StatusNotifierItem">
    <method name="Activate"><arg direction="in" type="i"/><arg direction="in" type="i"/></method>
    <method name="SecondaryActivate"><arg direction="in" type="i"/><arg direction="in" type="i"/></method>
    <method name="ContextMenu"><arg direction="in" type="i"/><arg direction="in" type="i"/></method>
    <method name="Scroll"><arg direction="in" type="i"/><arg direction="in" type="s"/></method>
    <signal name="NewTitle"/>
    <signal name="NewIcon"/>
    <signal name="NewToolTip"/>
    <signal name="NewStatus"><arg type="s"/></signal>
    <property name="Category" type="s" access="read"/>
    <property name="Id" type="s" access="read"/>
    <property name="Title" type="s" access="read"/>
    <property name="Status" type="s" access="read"/>
    <property name="WindowId" type="i" access="read"/>
    <property name="IconName" type="s" access="read"/>
    <property name="IconThemePath" type="s" access="read"/>
    <property name="OverlayIconName" type="s" access="read"/>
    <property name="AttentionIconName" type="s" access="read"/>
    <property name="ToolTip" type="(sa(iiay)ss)" access="read"/>
    <property name="ItemIsMenu" type="b" access="read"/>
    <property name="Menu" type="o" access="read"/>
  </interface>
  <interface name="com.canonical.dbusmenu">
    <method name="GetLayout">
      <arg direction="in" type="i"/><arg direction="in" type="i"/><arg direction="in" type="as"/>
      <arg direction="out" type="u"/><arg direction="out" type="(ia{sv}av)"/>
    </method>
    <method name="GetGroupProperties">
      <arg direction="in" type="ai"/><arg direction="in" type="as"/><arg direction="out" type="a(ia{sv})"/>
    </method>
    <method name="GetProperty">
      <arg direction="in" type="i"/><arg direction="in" type="s"/><arg direction="out" type="v"/>
    </method>
    <method name="Event">
      <arg direction="in" type="i"/><arg direction="in" type="s"/><arg direction="in" type="v"/><arg direction="in" type="u"/>
    </method>
    <method name="EventGroup"><arg direction="in" type="a(isvu)"/><arg direction="out" type="ai"/></method>
    <method name="AboutToShow"><arg direction="in" type="i"/><arg direction="out" type="b"/></method>
    <method name="AboutToShowGroup">
      <arg direction="in" type="ai"/><arg direction="out" type="ai"/><arg direction="out" type="ai"/>
    </method>
    <signal name="LayoutUpdated"><arg type="u"/><arg type="i"/></signal>
    <signal name="ItemsPropertiesUpdated"><arg type="a(ia{sv})"/><arg type="a(ias)"/></signal>
    <property name="Version" type="u" access="read"/>
    <property name="TextDirection" type="s" access="read"/>
    <property name="Status" type="s" access="read"/>
    <property name="IconThemePath" type="as" access="read"/>
  </interface>
</node>"#;

/// The menu's items, by their dbusmenu id; 0 is the root.
#[derive(Clone, Copy)]
enum Item {
    Song = 1,
    Play,
    Previous,
    Next,
    Mini,
    Show,
    Quit,
}

const ITEMS: [Option<Item>; 10] = [
    Some(Item::Song),
    None,
    Some(Item::Play),
    Some(Item::Previous),
    Some(Item::Next),
    None,
    Some(Item::Mini),
    Some(Item::Show),
    None,
    Some(Item::Quit),
];

/// Serves the icon and its menu, and registers it with the panel now and
/// whenever the panel starts again.
pub fn start(gapp: &adw::Application, app: &Rc<App>) {
    let Some(conn) = gapp.dbus_connection() else { return };
    let node = gio::DBusNodeInfo::for_xml(XML).expect("tray introspection");
    let (g, a, a2) = (gapp.clone(), app.clone(), app.clone());
    let _ = conn
        .register_object(ITEM, &node.lookup_interface("org.kde.StatusNotifierItem").unwrap())
        .method_call(move |_, _, _, _, method, _, invocation| {
            match method {
                "Activate" => g.activate(),
                "SecondaryActivate" => a.playback().toggle(),
                _ => {}
            }
            invocation.return_value(None);
        })
        .property(move |_, _, _, _, name| item_property(&a2, name))
        .build();
    let revision = Rc::new(Cell::new(1u32));
    let (g, a, r) = (gapp.clone(), app.clone(), revision.clone());
    let _ = conn
        .register_object(MENU, &node.lookup_interface("com.canonical.dbusmenu").unwrap())
        .method_call(move |_, _, _, _, method, params, invocation| {
            let reply = match method {
                "GetLayout" => Some((r.get(), layout(&a)).to_variant()),
                "GetGroupProperties" => {
                    let ids = params.child_value(0).get::<Vec<i32>>().unwrap_or_default();
                    let all: Vec<(i32, HashMap<String, Variant>)> = ITEMS
                        .iter()
                        .flatten()
                        .map(|&i| (i as i32, props(&a, Some(i))))
                        .filter(|(id, _)| ids.is_empty() || ids.contains(id))
                        .collect();
                    Some((all,).to_variant())
                }
                "GetProperty" => {
                    let (id, name) = params.get::<(i32, String)>().unwrap_or_default();
                    let v = props(&a, item(id)).remove(&name).unwrap_or_else(|| "".to_variant());
                    Some((v,).to_variant())
                }
                "Event" => {
                    let (id, event) = (params.child_value(0).get::<i32>(), params.child_value(1).get::<String>());
                    if event.as_deref() == Some("clicked") {
                        clicked(&g, &a, id.unwrap_or(0));
                    }
                    None
                }
                "EventGroup" => {
                    for e in params.child_value(0).iter() {
                        if e.child_value(1).get::<String>().as_deref() == Some("clicked") {
                            clicked(&g, &a, e.child_value(0).get::<i32>().unwrap_or(0));
                        }
                    }
                    Some((Vec::<i32>::new(),).to_variant())
                }
                "AboutToShow" => Some((false,).to_variant()),
                "AboutToShowGroup" => Some((Vec::<i32>::new(), Vec::<i32>::new()).to_variant()),
                _ => None,
            };
            invocation.return_value(reply.as_ref());
        })
        .property(|_, _, _, _, name| match name {
            "Version" => 3u32.to_variant(),
            "IconThemePath" => Vec::<String>::new().to_variant(),
            "TextDirection" => "ltr".to_variant(),
            _ => "normal".to_variant(),
        })
        .build();
    // The song and Play or Pause follow the player.
    let c = conn.clone();
    let shown = std::cell::RefCell::new(String::new());
    let a = app.clone();
    app.listen(move |ch| {
        if ch == Change::Playback {
            let now = format!("{}{}", song_line(&a), a.playback().is_playing());
            if now != *shown.borrow() {
                *shown.borrow_mut() = now;
                revision.set(revision.get() + 1);
                let _ = c.emit_signal(
                    None,
                    MENU,
                    "com.canonical.dbusmenu",
                    "LayoutUpdated",
                    Some(&(revision.get(), 0i32).to_variant()),
                );
                let _ = c.emit_signal(None, ITEM, "org.kde.StatusNotifierItem", "NewToolTip", None);
            }
        }
        true
    });
    // Watched for the app's life.
    let _ = gio::bus_watch_name_on_connection(
        &conn,
        WATCHER,
        gio::BusNameWatcherFlags::NONE,
        |conn, _, _| {
            conn.call(
                Some(WATCHER),
                "/StatusNotifierWatcher",
                WATCHER,
                "RegisterStatusNotifierItem",
                Some(&(ITEM,).to_variant()),
                None,
                gio::DBusCallFlags::NONE,
                -1,
                None::<&gio::Cancellable>,
                |r| {
                    if let Err(e) = r {
                        glib::g_info!("Dhun", "tray: {e}");
                    }
                },
            );
        },
        |_, _| {},
    );
}

fn item_property(app: &Rc<App>, name: &str) -> Variant {
    match name {
        "Category" => "ApplicationStatus".to_variant(),
        "Id" => "dhun".to_variant(),
        "Title" => "Dhun".to_variant(),
        "Status" => "Active".to_variant(),
        "WindowId" => 0i32.to_variant(),
        "IconName" => crate::ID.to_variant(),
        "ToolTip" => ("", Vec::<(i32, i32, Vec<u8>)>::new(), "Dhun", song_line(app)).to_variant(),
        "ItemIsMenu" => false.to_variant(),
        "Menu" => glib::variant::ObjectPath::try_from(MENU).unwrap().to_variant(),
        _ => "".to_variant(),
    }
}

/// "Title — Artist", or that nothing plays.
fn song_line(app: &App) -> String {
    match app.playback().current() {
        Some(s) => format!("{} — {}", s.title, s.display_artist()),
        None => "Nothing playing".into(),
    }
}

fn props(app: &App, item: Option<Item>) -> HashMap<String, Variant> {
    let mut m = HashMap::new();
    let Some(item) = item else {
        m.insert("children-display".into(), "submenu".to_variant());
        return m;
    };
    let playing = app.playback().is_playing();
    let label = match item {
        Item::Song => song_line(app),
        Item::Play => (if playing { "Pause" } else { "Play" }).into(),
        Item::Previous => "Previous".into(),
        Item::Next => "Next".into(),
        Item::Mini => "Mini Player".into(),
        Item::Show => "Show Dhun".into(),
        Item::Quit => "Quit".into(),
    };
    m.insert("label".into(), label.to_variant());
    let has_song = app.playback().current().is_some();
    let enabled = match item {
        Item::Song => false,
        Item::Play | Item::Previous | Item::Next => has_song,
        _ => true,
    };
    m.insert("enabled".into(), enabled.to_variant());
    m
}

/// The whole menu, whatever part was asked for: it is ten items.
fn layout(app: &App) -> (i32, HashMap<String, Variant>, Vec<Variant>) {
    let children: Vec<Variant> = ITEMS
        .iter()
        .enumerate()
        .map(|(i, item)| {
            let (id, p) = match item {
                Some(it) => (*it as i32, props(app, Some(*it))),
                None => (100 + i as i32, HashMap::from([("type".to_string(), "separator".to_variant())])),
            };
            (id, p, Vec::<Variant>::new()).to_variant()
        })
        .collect();
    (0, props(app, None), children)
}

fn item(id: i32) -> Option<Item> {
    ITEMS.iter().flatten().find(|&&i| i as i32 == id).copied()
}

fn clicked(gapp: &adw::Application, app: &Rc<App>, id: i32) {
    let pb = app.playback();
    match item(id) {
        Some(Item::Play) => pb.toggle(),
        Some(Item::Previous) => pb.previous(true),
        Some(Item::Next) => pb.next(true),
        Some(Item::Mini) => crate::mini::open(gapp, app),
        Some(Item::Show) => gapp.activate(),
        Some(Item::Quit) => gapp.quit(),
        _ => {}
    }
}
