//! Dhun for Linux: docs/plans/039_linux_app.md. GTK4 and libadwaita for
//! the window; the logic is in `dhun-core`.

mod app;
mod pages;
mod search;
mod widgets;
mod window;

use adw::prelude::*;
use app::App;
use gtk::{gdk, gio, glib};
use std::path::PathBuf;
use std::rc::Rc;
use std::time::Duration;

const ID: &str = "io.github.vivekg7.dhun";

fn main() -> glib::ExitCode {
    let gapp = adw::Application::builder().application_id(ID).build();
    gapp.connect_startup(|_| {
        let css = gtk::CssProvider::new();
        css.load_from_string(include_str!("style.css"));
        gtk::style_context_add_provider_for_display(
            &gdk::Display::default().unwrap(),
            &css,
            gtk::STYLE_PROVIDER_PRIORITY_APPLICATION,
        );
    });
    let state: Rc<std::cell::OnceCell<Rc<App>>> = Rc::default();
    gapp.connect_activate(move |gapp| {
        if let Some(w) = gapp.active_window() {
            w.present();
            return;
        }
        let app = state.get_or_init(|| start(gapp)).clone();
        let window = window::build(gapp, &app);
        if let Some((w, h)) = std::env::var("DHUN_SIZE")
            .ok()
            .and_then(|s| Some(s.split_once('x')?).and_then(|(w, h)| Some((w.parse().ok()?, h.parse().ok()?))))
        {
            window.set_default_size(w, h);
        }
        window.present();
        glib::g_info!("Dhun", "window shown");
        if let Ok(path) = std::env::var("DHUN_SCREENSHOT") {
            screenshot(&window, PathBuf::from(path));
        }
    });
    gapp.run()
}

/// The app's folder: `DHUN_DATA` for a build under test, so it keeps its
/// own library and sign-in, as on the Mac; else the user's data folder
/// (inside the Flatpak, the app's own).
fn data_dir() -> PathBuf {
    std::env::var_os("DHUN_DATA").map(PathBuf::from).unwrap_or_else(|| glib::user_data_dir().join("dhun"))
}

fn start(gapp: &adw::Application) -> Rc<App> {
    let app = App::new(data_dir());
    let action = |name: &str, f: Box<dyn Fn()>| {
        let a = gio::SimpleAction::new(name, None);
        a.connect_activate(move |_, _| f());
        gapp.add_action(&a);
    };
    let a = app.clone();
    action("sync", Box::new(move || a.sync_soon(Duration::ZERO)));
    let a = app.clone();
    action(
        "sign-out",
        Box::new(move || {
            let a = a.clone();
            glib::spawn_future_local(async move { a.sign_out().await });
        }),
    );
    let g = gapp.clone();
    action("about", Box::new(move || about(&g)));
    let g = gapp.clone();
    action("quit", Box::new(move || g.quit()));
    gapp.set_accels_for_action("app.quit", &["<Control>q"]);
    // The kept copy at once, then what changed on the server.
    if app.signed_in() {
        app.reload_playlists();
        let a = app.clone();
        glib::spawn_future_local(async move {
            a.reload_songs().await;
            a.sync_soon(Duration::ZERO);
        });
    }
    // Back online: fetch what piled up meanwhile.
    let a = app.clone();
    gio::NetworkMonitor::default().connect_network_changed(move |_, up| {
        if up {
            a.sync_soon(Duration::from_secs(2));
        }
    });
    app
}

fn about(gapp: &adw::Application) {
    let about = adw::AboutDialog::builder()
        .application_name("Dhun")
        .application_icon("folder-music-symbolic")
        .version(env!("CARGO_PKG_VERSION"))
        .license_type(gtk::License::Gpl30)
        .website("https://github.com/vivekg7/dhun")
        .build();
    about.present(gapp.active_window().as_ref());
}

/// For checking the window without a person at the screen: draws it to a
/// PNG once the library has had time to load, then quits.
fn screenshot(window: &adw::ApplicationWindow, path: PathBuf) {
    let secs = std::env::var("DHUN_SCREENSHOT_AFTER").ok().and_then(|s| s.parse().ok()).unwrap_or(6);
    let w = window.clone();
    glib::timeout_add_seconds_local_once(secs, move || {
        // The window's content: a paintable of the window itself draws nothing,
        // and a paintable has an image only once the widget draws again.
        let paintable = gtk::WidgetPaintable::new(w.content().as_ref());
        w.content().unwrap().queue_draw();
        glib::timeout_add_local_once(Duration::from_millis(500), move || {
            let snapshot = gtk::Snapshot::new();
            // The window's own background, which its content leaves to it.
            let bg = if adw::StyleManager::default().is_dark() { "#222226" } else { "#fafafb" };
            let rect = gtk::graphene::Rect::new(0.0, 0.0, w.width() as f32, w.height() as f32);
            snapshot.append_color(&gdk::RGBA::parse(bg).unwrap(), &rect);
            paintable.snapshot(&snapshot, w.width() as f64, w.height() as f64);
            match (snapshot.to_node(), w.renderer()) {
                (Some(node), Some(renderer)) => {
                    if let Err(e) = renderer.render_texture(&node, None).save_to_png(&path) {
                        eprintln!("screenshot: {e}");
                    }
                }
                _ => eprintln!("screenshot: nothing drawn"),
            }
            w.application().unwrap().quit();
        });
    });
}
