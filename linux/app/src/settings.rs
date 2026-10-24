//! Settings (plans 018, 023, 034), as the phone's and the Mac's: the look of
//! this computer, the Playback settings that follow the user, this
//! computer's downloads and cache, the account and family members, and How
//! it works. Also the theme and accent the window wears.

use crate::app::{App, Change};
use crate::widgets::confirm;
use adw::prelude::*;
use dhun_core::downloads::format_bytes;
use dhun_core::store;
use dhun_core::synced::{self, Bool, Int, Text};
use dhun_core::tempo::Tempo;
use gtk::{gdk, glib};
use std::cell::RefCell;
use std::rc::Rc;

/// The phone's eight accents, Dull Orange first, each with a lighter shade for dark.
pub const PALETTES: [(&str, u32, u32); 8] = [
    ("Dull Orange", 0xC27B45, 0xD9925C),
    ("Sage", 0x7E9C7A, 0x98B594),
    ("Slate Blue", 0x6A7FA8, 0x8C9FC6),
    ("Teal", 0x4E9A96, 0x6FB6B1),
    ("Dusty Rose", 0xB9707F, 0xD08D9B),
    ("Mauve", 0x9479A8, 0xB097C2),
    ("Olive", 0x8F9152, 0xADAF6F),
    ("Sand", 0xB59E73, 0xCBB68C),
];

const THEMES: [(&str, &str); 5] = [
    ("system", "Follow the system"),
    ("light", "Light"),
    ("dark", "Dark"),
    ("black", "Black"),
    ("time", "Day and night by the clock"),
];

thread_local! {
    static THEME_CSS: gtk::CssProvider = gtk::CssProvider::new();
}

/// Day and night by the clock: light from 07:00 to 19:00, as on the phone.
fn daytime() -> bool {
    glib::DateTime::now_local().is_ok_and(|t| (7..19).contains(&t.hour()))
}

fn hex(c: u32) -> String {
    format!("#{c:06x}")
}

/// `a` moved `f` of the way to `b`.
fn lerp(a: u32, b: u32, f: f64) -> String {
    let ch = |c: u32, s: u32| ((c >> s) & 0xff) as f64;
    let mix = |s| (ch(a, s) + (ch(b, s) - ch(a, s)) * f).round() as u32;
    hex(mix(16) << 16 | mix(8) << 8 | mix(0))
}

/// The theme and accent of this computer, put on the window.
pub fn apply_theme(app: &App) {
    let p = app.prefs.get();
    let sm = adw::StyleManager::default();
    sm.set_color_scheme(match p.theme.as_str() {
        "light" => adw::ColorScheme::ForceLight,
        "dark" | "black" => adw::ColorScheme::ForceDark,
        "time" if daytime() => adw::ColorScheme::ForceLight,
        "time" => adw::ColorScheme::ForceDark,
        _ => adw::ColorScheme::Default,
    });
    let (_, light, dark) = PALETTES[p.palette.min(PALETTES.len() - 1)];
    let mut css = if sm.is_dark() {
        format!(
            "@define-color accent_bg_color {0};\n@define-color accent_color {0};\n@define-color accent_fg_color #1a1410;\n",
            hex(dark)
        )
    } else {
        // Accent-coloured text a little darker than the accent, so it reads on white.
        format!(
            "@define-color accent_bg_color {};\n@define-color accent_color {};\n@define-color accent_fg_color #ffffff;\n",
            hex(light),
            lerp(light, 0x1d1b19, 0.25)
        )
    };
    if p.theme == "black" {
        css += "@define-color window_bg_color #000000;\n@define-color view_bg_color #000000;\n\
                @define-color headerbar_bg_color #000000;\n@define-color sidebar_bg_color #0a0a0a;\n\
                @define-color secondary_sidebar_bg_color #0a0a0a;\n@define-color card_bg_color #111111;\n\
                @define-color dialog_bg_color #111111;\n@define-color popover_bg_color #111111;\n";
    }
    for (i, (_, light, dark)) in PALETTES.iter().enumerate() {
        css += &format!(
            ".palette-{i} {{ background: {}; min-width: 24px; min-height: 24px; }}\n",
            hex(if sm.is_dark() { *dark } else { *light })
        );
    }
    THEME_CSS.with(|c| c.load_from_string(&css));
}

/// Puts the theme on the display, and keeps it in step with the system's
/// light or dark and, for day and night, with the clock.
pub fn start_theme(app: &Rc<App>) {
    if let Some(d) = gdk::Display::default() {
        THEME_CSS
            .with(|c| gtk::style_context_add_provider_for_display(&d, c, gtk::STYLE_PROVIDER_PRIORITY_APPLICATION + 1));
    }
    apply_theme(app);
    let a = app.clone();
    adw::StyleManager::default().connect_dark_notify(move |_| apply_theme(&a));
    let a = app.clone();
    glib::timeout_add_seconds_local(60, move || {
        if a.prefs.get().theme == "time" {
            apply_theme(&a);
        }
        glib::ControlFlow::Continue
    });
}

/// A value's label, with " (default)" on the default.
fn marked(label: String, default: bool) -> String {
    if default { format!("{label} (default)") } else { label }
}

/// A row choosing one of `labels`, `at` chosen; `set` hears the new choice.
fn choice(title: &str, labels: &[String], at: usize, set: impl Fn(usize) + 'static) -> adw::ComboRow {
    let model = gtk::StringList::new(&labels.iter().map(String::as_str).collect::<Vec<_>>());
    let row = adw::ComboRow::builder().title(title).model(&model).selected(at as u32).build();
    row.connect_selected_notify(move |r| set(r.selected() as usize));
    row
}

fn info_row(title: &str, value: &str) -> adw::ActionRow {
    let row = adw::ActionRow::builder().title(title).build();
    row.add_suffix(&gtk::Label::builder().label(value).css_classes(["dim-label"]).selectable(true).build());
    row
}

/// A page's last group: Reset to Defaults, after asking.
fn reset_group(title: &'static str, body: &'static str, reset: impl Fn() + 'static) -> adw::PreferencesGroup {
    let g = adw::PreferencesGroup::new();
    let b = gtk::Button::builder().label("Reset to Defaults…").halign(gtk::Align::Start).build();
    let reset = Rc::new(reset);
    b.connect_clicked(move |b| {
        let r = reset.clone();
        confirm(b, title, body, "Reset", move || r());
    });
    g.add(&b);
    g
}

pub fn open(app: &Rc<App>, parent: &impl IsA<gtk::Widget>) -> adw::PreferencesDialog {
    let d = adw::PreferencesDialog::builder().title("Settings").build();
    d.set_search_enabled(false);
    d.add(&appearance(app));
    d.add(&playback(app, &d));
    d.add(&storage(app));
    d.add(&account(app, &d));
    d.add(&guide_page());
    d.present(Some(parent));
    d
}

fn appearance(app: &Rc<App>) -> adw::PreferencesPage {
    let page = adw::PreferencesPage::builder()
        .title("Appearance")
        .name("appearance")
        .icon_name("applications-graphics-symbolic")
        .build();
    let g =
        adw::PreferencesGroup::builder().title("Appearance").description("This computer's own: not synced.").build();
    let p = app.prefs.get();
    let labels: Vec<String> = THEMES.iter().map(|(k, l)| marked(l.to_string(), *k == "system")).collect();
    let at = THEMES.iter().position(|(k, _)| *k == p.theme).unwrap_or(0);
    let a = app.clone();
    g.add(&choice("Theme", &labels, at, move |i| {
        a.prefs.update(|p| p.theme = THEMES[i].0.into());
        apply_theme(&a);
    }));
    let row = adw::ActionRow::builder().title("Accent colour").build();
    let dots = gtk::Box::builder().spacing(6).valign(gtk::Align::Center).build();
    let buttons: Rc<RefCell<Vec<gtk::Button>>> = Rc::default();
    for (i, (name, _, _)) in PALETTES.iter().enumerate() {
        let b = gtk::Button::builder()
            .css_classes(["circular", &format!("palette-{i}")])
            .tooltip_text(marked(name.to_string(), i == 0))
            .build();
        if i == p.palette {
            b.set_icon_name("object-select-symbolic");
        }
        let (a, all) = (app.clone(), buttons.clone());
        b.connect_clicked(move |b| {
            a.prefs.update(|p| p.palette = i);
            apply_theme(&a);
            for x in all.borrow().iter() {
                x.set_child(None::<&gtk::Widget>);
            }
            b.set_icon_name("object-select-symbolic");
        });
        dots.append(&b);
        buttons.borrow_mut().push(b);
    }
    row.add_suffix(&dots);
    g.add(&row);
    page.add(&g);
    let a = app.clone();
    page.add(&reset_group("Reset Appearance settings?", "The theme and accent colour of this computer.", move || {
        a.prefs.update(|p| {
            p.theme.clear();
            p.palette = 0;
        });
        apply_theme(&a);
    }));
    page
}

/// Synced settings: they follow the user to every device (plans 009, 010).
fn playback(app: &Rc<App>, dialog: &adw::PreferencesDialog) -> adw::PreferencesPage {
    let page = adw::PreferencesPage::builder()
        .title("Playback")
        .name("playback")
        .icon_name("media-playback-start-symbolic")
        .description("These follow you to all your devices.")
        .build();
    let settings = app.settings.borrow().clone();
    let set = {
        let app = app.clone();
        move |name: &'static str, v: serde_json::Value| {
            app.store(|db| store::setting(db, name, v));
            app.reload_queues();
        }
    };
    let int = |title: &str, key: &'static Int, label: fn(i64) -> String| {
        let labels: Vec<String> = key.2.iter().map(|&v| marked(label(v), v == key.1)).collect();
        let at = key.2.iter().position(|&v| v == key.get(&settings)).unwrap_or(0);
        let set = set.clone();
        choice(title, &labels, at, move |i| set(key.0, key.2[i].into()))
    };
    let text = |title: &str, key: &'static Text, label: fn(&str) -> &'static str| {
        let labels: Vec<String> = key.2.iter().map(|v| marked(label(v).into(), *v == key.1)).collect();
        let at = key.2.iter().position(|v| *v == key.get(&settings)).unwrap_or(0);
        let set = set.clone();
        choice(title, &labels, at, move |i| set(key.0, key.2[i].into()))
    };
    let switch = |title: &str, key: &'static Bool| {
        let row = adw::SwitchRow::builder().title(title).active(key.get(&settings)).build();
        let set = set.clone();
        row.connect_active_notify(move |r| set(key.0, r.is_active().into()));
        row
    };
    let with_info = |row: adw::ComboRow, section: &'static str| {
        let b = gtk::Button::builder()
            .icon_name("help-about-symbolic")
            .tooltip_text("How it works")
            .css_classes(["flat", "circular"])
            .valign(gtk::Align::Center)
            .build();
        let d = dialog.downgrade();
        b.connect_clicked(move |_| {
            if let Some(d) = d.upgrade() {
                d.push_subpage(&guide_subpage(section));
            }
        });
        row.add_prefix(&b);
        row
    };

    let buttons = adw::PreferencesGroup::builder().title("Buttons").build();
    let labels = [marked("Restart song first".into(), true), "Previous song".into()];
    let at = if synced::PREVIOUS_RESTART.get(&settings) { 0 } else { 1 };
    let s = set.clone();
    buttons.add(&choice("Previous", &labels, at, move |i| s(synced::PREVIOUS_RESTART.0, (i == 0).into())));
    page.add(&buttons);

    let long = adw::PreferencesGroup::builder().title("Long files").build();
    let row = int("Remember position", &synced::MIN_MINUTES, |m| format!("{m} min and longer"));
    row.set_subtitle("For audiobooks, podcasts and mixes.");
    long.add(&with_info(row, "Remember position"));
    long.add(&text("When played again", &synced::RESUME, |v| match v {
        "auto" => "Resume",
        "ask" => "Ask",
        _ => "Start over",
    }));
    let row = text("Previous button", &synced::PREVIOUS, |v| match v {
        "keep" => "Previous file",
        "seek" => "Seek back (headset)",
        "ignore" => "Ignore headset",
        _ => "Same as songs",
    });
    row.set_subtitle("On the screen, Previous always goes to the previous file.");
    long.add(&with_info(row, "Previous button"));
    let row = text("Next button", &synced::NEXT, |v| if v == "seek" { "Seek forward (headset)" } else { "Next file" });
    long.add(&with_info(row, "Next button"));
    long.add(&int("Seek back", &synced::SEEK_BACK, |s| format!("{s} s")));
    long.add(&int("Seek forward", &synced::SEEK_FORWARD, |s| format!("{s} s")));
    long.add(&int("Rewind on resume", &synced::REWIND, |s| if s == 0 { "Off".into() } else { format!("{s} s") }));
    let after = int("After pauses of", &synced::REWIND_AFTER, |m| format!("{m} min"));
    after.set_sensitive(synced::REWIND.get(&settings) != 0);
    long.add(&after);
    let row =
        int("Cache ahead", &synced::CACHE_AHEAD, |m| if m == 0 { "Whole file".into() } else { format!("{m} min") });
    row.set_subtitle("A long file is cached around where you are, not whole.");
    long.add(&with_info(row, "Song cache"));
    page.add(&long);

    let later = adw::PreferencesGroup::builder().title("Listen Later").build();
    let remove = switch("Remove when finished", &synced::AUTO_REMOVE);
    let pct = int("Finished at", &synced::FINISHED_PERCENT, |p| format!("{p}%"));
    pct.set_sensitive(synced::AUTO_REMOVE.get(&settings));
    let p = pct.downgrade();
    remove.connect_active_notify(move |r| {
        if let Some(p) = p.upgrade() {
            p.set_sensitive(r.is_active());
        }
    });
    later.add(&remove);
    later.add(&pct);
    page.add(&later);

    let speed = adw::PreferencesGroup::builder().title("Speed and pitch").build();
    let everyday = app.prefs.get().everyday();
    let row = adw::ActionRow::builder()
        .title("Everyday on this computer")
        .subtitle(everyday.label().unwrap_or_else(|| "Normal".into()))
        .build();
    let reset =
        gtk::Button::builder().label("Reset").valign(gtk::Align::Center).sensitive(everyday != Tempo::NORMAL).build();
    let (a, r) = (app.clone(), row.downgrade());
    reset.connect_clicked(move |b| {
        a.prefs.update(|p| {
            p.speed = 1.0;
            p.semitones = 0;
        });
        a.playback().set_tempo(a.playback().tempo(), a.playback().song_tempo().is_some());
        b.set_sensitive(false);
        if let Some(r) = r.upgrade() {
            r.set_subtitle("Normal");
        }
    });
    row.add_suffix(&reset);
    speed.add(&row);
    page.add(&speed);

    // Null, not today's default, so a later default reaches every device (plan 034).
    let (a, d) = (app.clone(), dialog.downgrade());
    page.add(&reset_group("Reset Playback settings?", "They follow you, so all your devices change.", move || {
        a.store(|db| synced::PLAYBACK.iter().try_for_each(|name| store::setting(db, name, serde_json::Value::Null)));
        a.reload_queues();
        if let Some(d) = d.upgrade() {
            d.close();
        }
    }));
    page
}

/// This computer's storage and network, so not synced (plans 012, 019).
fn storage(app: &Rc<App>) -> adw::PreferencesPage {
    let page = adw::PreferencesPage::builder()
        .title("Downloads")
        .name("downloads")
        .icon_name("folder-download-symbolic")
        .build();
    let p = app.prefs.get();
    let downloads = adw::PreferencesGroup::builder()
        .title("Downloads")
        .description("At the limit, downloads stop. Nothing is deleted.")
        .build();
    downloads.add(&info_row("Used", &format_bytes(app.downloads().status.borrow().used)));
    let sizes = [2, 5, 10, 20, 50, 0];
    let labels: Vec<String> =
        sizes.iter().map(|&g| marked(if g == 0 { "No limit".into() } else { format!("{g} GB") }, g == 10)).collect();
    let at = sizes.iter().position(|&g| g == p.download_limit_gb.unwrap_or(10)).unwrap_or(2);
    let a = app.clone();
    downloads.add(&choice("Storage limit", &labels, at, move |i| {
        a.prefs.update(|p| p.download_limit_gb = Some(sizes[i]));
        a.downloads().poke();
    }));
    let metered = adw::SwitchRow::builder()
        .title("Download on a metered network")
        .subtitle("A phone's hotspot, or a connection that counts its data.")
        .active(p.download_on_metered == Some(true))
        .build();
    let a = app.clone();
    metered.connect_active_notify(move |r| {
        a.prefs.update(|p| p.download_on_metered = Some(r.is_active()));
        a.downloads().poke();
    });
    downloads.add(&metered);
    page.add(&downloads);

    let cache = adw::PreferencesGroup::builder().title("Song cache").build();
    cache.add(&info_row("Used", &format_bytes(app.cache.used())));
    let sizes = [1, 3, 5, 10, 0];
    let labels: Vec<String> =
        sizes.iter().map(|&g| marked(if g == 0 { "Off".into() } else { format!("{g} GB") }, g == 3)).collect();
    let at = sizes.iter().position(|&g| g == p.cache_limit_gb.unwrap_or(3)).unwrap_or(1);
    let a = app.clone();
    cache.add(&choice("Storage limit", &labels, at, move |i| a.prefs.update(|p| p.cache_limit_gb = Some(sizes[i]))));
    page.add(&cache);

    let a = app.clone();
    page.add(&reset_group("Reset Downloads settings?", "Nothing downloaded is deleted.", move || {
        a.prefs.update(|p| {
            p.download_limit_gb = None;
            p.cache_limit_gb = None;
            p.download_on_metered = None;
        });
        a.downloads().poke();
    }));
    page
}

fn account(app: &Rc<App>, dialog: &adw::PreferencesDialog) -> adw::PreferencesPage {
    let page =
        adw::PreferencesPage::builder().title("Account").name("account").icon_name("avatar-default-symbolic").build();
    let p = app.prefs.get();
    let g = adw::PreferencesGroup::builder().title("Account").build();
    let who = if app.signed_in() {
        format!("{}{}", p.user, if p.admin { " (admin)" } else { "" })
    } else {
        "Not signed in".into()
    };
    g.add(&info_row("Signed in as", &who));
    g.add(&info_row("Server", &p.server));
    if !p.server_version.is_empty() {
        g.add(&info_row("Server version", &p.server_version));
    }
    let last = adw::ActionRow::builder().title("Last sync").build();
    let label = gtk::Label::builder()
        .css_classes(["dim-label"])
        .ellipsize(gtk::pango::EllipsizeMode::End)
        .max_width_chars(36)
        .build();
    last.add_suffix(&label);
    let update = {
        let app = app.clone();
        move || {
            let e = app.sync_error.borrow().clone();
            label.set_label(&match e {
                Some(e) => e,
                None if app.syncing.get() => "Syncing…".into(),
                None => "Up to date".into(),
            });
        }
    };
    update();
    let l = last.downgrade();
    app.listen(move |c| {
        if l.upgrade().is_none() {
            return false;
        }
        if c == Change::Status {
            update();
        }
        true
    });
    g.add(&last);
    let actions = gtk::Box::builder().spacing(8).margin_top(12).build();
    let sync = gtk::Button::with_label("Sync Now");
    sync.set_sensitive(app.signed_in());
    let a = app.clone();
    sync.connect_clicked(move |_| a.sync_soon(std::time::Duration::ZERO));
    actions.append(&sync);
    if p.admin {
        let members = gtk::Button::with_label("Family Members…");
        let (a, d) = (app.clone(), dialog.downgrade());
        members.connect_clicked(move |_| {
            if let Some(d) = d.upgrade() {
                d.push_subpage(&family(&a));
            }
        });
        actions.append(&members);
    }
    let out = gtk::Button::builder()
        .label("Sign Out…")
        .css_classes(["destructive-action"])
        .hexpand(true)
        .halign(gtk::Align::End)
        .build();
    out.set_sensitive(app.signed_in());
    let (a, d) = (app.clone(), dialog.downgrade());
    out.connect_clicked(move |b| {
        let (a, d) = (a.clone(), d.clone());
        confirm(
            b,
            "Sign out?",
            "This computer forgets your queues, downloads and changes not yet sent.",
            "Sign Out",
            move || {
                if let Some(d) = d.upgrade() {
                    d.close();
                }
                let a = a.clone();
                glib::spawn_future_local(async move { a.sign_out().await });
            },
        );
    });
    actions.append(&out);
    g.add(&actions);
    page.add(&g);

    let about = adw::PreferencesGroup::builder().title("About").build();
    about.add(&info_row("Dhun for Linux", env!("CARGO_PKG_VERSION")));
    let source = adw::ActionRow::builder().title("Source code (GPL-3.0)").activatable(true).build();
    source.add_suffix(&gtk::Image::from_icon_name("adw-external-link-symbolic"));
    source.connect_activated(|r| {
        let w = r.root().and_downcast::<gtk::Window>();
        gtk::UriLauncher::new("https://github.com/vivekg7/dhun").launch(
            w.as_ref(),
            gtk::gio::Cancellable::NONE,
            |_| {},
        );
    });
    about.add(&source);
    page.add(&about);
    page
}

/// Family members, for the admin (plan 023): list, add, reset a forgotten password.
fn family(app: &Rc<App>) -> adw::NavigationPage {
    let page = adw::PreferencesPage::new();
    let list = adw::PreferencesGroup::builder().title("Family members").build();
    let add = gtk::Button::builder().label("Add Member…").css_classes(["flat"]).build();
    list.set_header_suffix(Some(&add));
    page.add(&list);
    let rows: Rc<RefCell<Vec<gtk::Widget>>> = Rc::default();
    let load: Rc<dyn Fn()> = {
        let (app, list, rows) = (app.clone(), list.downgrade(), rows.clone());
        Rc::new(move || {
            let (api, list, rows, app) = (app.api.clone(), list.clone(), rows.clone(), app.clone());
            glib::spawn_future_local(async move {
                let got = gtk::gio::spawn_blocking(move || api.members()).await;
                let Some(list) = list.upgrade() else { return };
                for r in rows.borrow_mut().drain(..) {
                    list.remove(&r);
                }
                let members = match got {
                    Ok(Ok(m)) => m,
                    Ok(Err(e)) => {
                        let r = adw::ActionRow::builder().title(e.to_string()).css_classes(["error"]).build();
                        list.add(&r);
                        rows.borrow_mut().push(r.upcast());
                        return;
                    }
                    Err(_) => return,
                };
                for m in members {
                    let r = adw::ActionRow::builder()
                        .title(format!("{}{}", m.name, if m.admin { " (admin)" } else { "" }))
                        .subtitle(if m.admin { "Its password is changed on the NAS".into() } else { seen(&m) })
                        .build();
                    if !m.admin {
                        let b = gtk::Button::builder().label("Reset Password…").valign(gtk::Align::Center).build();
                        let (app, id, name) = (app.clone(), m.id, m.name.clone());
                        b.connect_clicked(move |b| {
                            let api = app.api.clone();
                            let save = move |_, pw: String| api.reset_password(id, &pw).map_err(|e| e.to_string());
                            member_form(b, &format!("New password for {name}"), false, save, || {});
                        });
                        r.add_suffix(&b);
                    }
                    list.add(&r);
                    rows.borrow_mut().push(r.upcast());
                }
            });
        })
    };
    load();
    let (app, l) = (app.clone(), load.clone());
    add.connect_clicked(move |b| {
        let (api, l) = (app.api.clone(), l.clone());
        let save = move |name: String, pw: String| api.add_member(&name, &pw).map_err(|e| e.to_string());
        member_form(b, "Add a family member", true, save, move || l());
    });
    let tv = adw::ToolbarView::new();
    tv.add_top_bar(&adw::HeaderBar::new());
    tv.set_content(Some(&page));
    adw::NavigationPage::builder().title("Family Members").child(&tv).build()
}

/// "Signed in on 2 devices · seen 3 hours ago", as on the phone.
fn seen(m: &dhun_core::api::Member) -> String {
    if m.devices <= 0 {
        return "No device signed in".into();
    }
    let on = format!("Signed in on {} device{}", m.devices, if m.devices == 1 { "" } else { "s" });
    let at = dhun_core::time::parse(&m.last_seen_at);
    if at <= 0 {
        return on;
    }
    let mins = (dhun_core::time::now_ms() - at) / 60_000;
    let ago = match mins {
        ..1 => "just now".into(),
        1..60 => format!("{mins} min ago"),
        60..1440 => format!("{} hours ago", mins / 60),
        _ => format!("{} days ago", mins / 1440),
    };
    format!("{on} · seen {ago}")
}

/// A name (when adding) and a password, shown as typed: the admin reads it
/// out to the member, and a typo in a hidden password would lock them out.
/// The server's rules for both come back as its own error, rather than
/// copied here where they could drift.
fn member_form(
    parent: &impl IsA<gtk::Widget>,
    heading: &str,
    ask_name: bool,
    save: impl Fn(String, String) -> Result<(), String> + Send + Sync + 'static,
    done: impl Fn() + 'static,
) {
    let fields = gtk::Box::builder().orientation(gtk::Orientation::Vertical).spacing(8).build();
    let name = gtk::Entry::builder().placeholder_text("Name").visible(ask_name).build();
    let password = gtk::Entry::builder().placeholder_text("Password").build();
    let error = gtk::Label::builder().wrap(true).css_classes(["error", "caption"]).visible(false).build();
    fields.append(&name);
    fields.append(&password);
    fields.append(&error);
    let dialog = adw::AlertDialog::builder().heading(heading).extra_child(&fields).build();
    dialog.add_responses(&[("cancel", "Cancel"), ("save", "Save")]);
    dialog.set_response_appearance("save", adw::ResponseAppearance::Suggested);
    dialog.set_response_enabled("save", false);
    dialog.set_close_response("cancel");
    let ready = {
        let (d, n, p) = (dialog.downgrade(), name.clone(), password.clone());
        move || {
            if let Some(d) = d.upgrade() {
                d.set_response_enabled("save", !p.text().is_empty() && (!ask_name || !n.text().trim().is_empty()));
            }
        }
    };
    let r = ready.clone();
    name.connect_changed(move |_| r());
    password.connect_changed(move |_| ready());
    let (save, done) = (std::sync::Arc::new(save), Rc::new(done));
    let parent = parent.clone().upcast::<gtk::Widget>();
    let p = parent.clone();
    dialog.connect_response(Some("save"), move |d, _| {
        let (n, pw, save, done) =
            (name.text().trim().to_string(), password.text().to_string(), save.clone(), done.clone());
        let (d, error, parent) = (d.clone(), error.clone(), p.clone());
        glib::spawn_future_local(async move {
            let r = gtk::gio::spawn_blocking(move || save(n, pw)).await.unwrap_or_else(|_| Err("Stopped".into()));
            match r {
                Ok(()) => done(),
                // Open again with the server's refusal under the fields, to be read and fixed.
                Err(e) => {
                    error.set_label(&e);
                    error.set_visible(true);
                    d.present(Some(&parent));
                }
            }
        });
    });
    dialog.present(Some(&parent));
}

/// How it works (docs/GUIDE.md, copied into `guide.md` by scripts/guide.sh):
/// headings, paragraphs, list items and bold, which is all the guide uses.
fn guide_blocks() -> Vec<(usize, String)> {
    // 0 for a paragraph, 9 for a list item, else the heading's level.
    let mut blocks: Vec<(usize, String)> = vec![];
    let mut open = false;
    for line in include_str!("guide.md").lines() {
        let l = line.trim();
        if l.is_empty() {
            open = false;
        } else if l.starts_with('#') {
            let level = l.chars().take_while(|&c| c == '#').count();
            blocks.push((level, l[level..].trim().into()));
            open = false;
        } else if let Some(item) = l.strip_prefix("- ") {
            blocks.push((9, item.into()));
            open = true;
        } else if let (true, Some(last)) = (open, blocks.last_mut()) {
            last.1 += " ";
            last.1 += l;
        } else {
            blocks.push((0, l.into()));
            open = true;
        }
    }
    blocks
}

/// `**bold**` as Pango markup, the rest escaped.
fn markup(text: &str) -> String {
    let mut out = String::new();
    for (i, part) in text.split("**").enumerate() {
        let part = glib::markup_escape_text(part);
        if i % 2 == 1 {
            out += &format!("<b>{part}</b>");
        } else {
            out += &part;
        }
    }
    out
}

/// The guide's text, and the heading labels by name, to scroll to one.
fn guide_text() -> (gtk::Box, Vec<(String, gtk::Label)>) {
    let b = gtk::Box::builder().orientation(gtk::Orientation::Vertical).spacing(8).build();
    let mut headings = vec![];
    for (kind, text) in guide_blocks() {
        // Selectable, but not focused: a focused label selects all of itself.
        let l = gtk::Label::builder().use_markup(true).xalign(0.0).wrap(true).selectable(true).can_focus(false).build();
        match kind {
            0 => l.set_markup(&markup(&text)),
            9 => l.set_markup(&format!("•  {}", markup(&text))),
            1 => {
                l.set_markup(&markup(&text));
                l.add_css_class("title-2");
            }
            2 => {
                l.set_markup(&markup(&text));
                l.add_css_class("title-4");
                l.set_margin_top(10);
            }
            _ => {
                l.set_markup(&markup(&text));
                l.add_css_class("heading");
                l.set_margin_top(4);
            }
        }
        if (1..9).contains(&kind) {
            headings.push((text.clone(), l.clone()));
        }
        b.append(&l);
    }
    (b, headings)
}

fn guide_page() -> adw::PreferencesPage {
    let page =
        adw::PreferencesPage::builder().title("How it works").name("guide").icon_name("help-about-symbolic").build();
    let g = adw::PreferencesGroup::new();
    g.add(&guide_text().0);
    page.add(&g);
    page
}

/// The guide at the `###` heading of that name, from a setting's ⓘ.
fn guide_subpage(section: &str) -> adw::NavigationPage {
    let (text, headings) = guide_text();
    text.set_margin_start(18);
    text.set_margin_end(18);
    text.set_margin_top(12);
    text.set_margin_bottom(18);
    let scroll =
        gtk::ScrolledWindow::builder().child(&text).hscrollbar_policy(gtk::PolicyType::Never).vexpand(true).build();
    if let Some((_, label)) = headings.into_iter().find(|(h, _)| h == section) {
        // Once laid out, or there is nothing to scroll to yet.
        let s = scroll.downgrade();
        label.connect_map(move |l| {
            let (Some(s), l) = (s.upgrade(), l.clone()) else { return };
            glib::idle_add_local_once(move || {
                if let Some(b) = l.compute_bounds(&s.child().unwrap()) {
                    s.vadjustment().set_value(b.y() as f64);
                }
            });
        });
    }
    let tv = adw::ToolbarView::new();
    tv.add_top_bar(&adw::HeaderBar::new());
    tv.set_content(Some(&scroll));
    adw::NavigationPage::builder().title("How it works").child(&tv).build()
}
