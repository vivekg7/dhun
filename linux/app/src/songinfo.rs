//! A song's tags, file and plays, and the playlists it is in (plan 041):
//! one dialog from the song menu, as the phone's `SongInfo`.

use crate::app::App;
use crate::widgets::clock;
use adw::prelude::*;
use dhun_core::catalog::SongRef;
use dhun_core::downloads::format_bytes;
use dhun_core::model::Playlist;

/// "Info" in the song menu. Rows with nothing to say are left out.
pub fn info(app: &App, parent: &gtk::Widget, s: &SongRef) {
    let plays = app.db.lock().unwrap().play_stats().unwrap_or_default().get(&s.id).map_or(0, |p| p.count);
    let rate = s.sample_rate;
    let file = [
        Some(s.format.clone()).filter(|f| !f.is_empty()),
        (s.bitrate > 0).then(|| format!("{} kbit/s", s.bitrate)),
        (rate > 0).then(|| {
            if rate % 1000 == 0 { format!("{} kHz", rate / 1000) } else { format!("{} kHz", rate as f64 / 1000.0) }
        }),
        (s.size > 0).then(|| format_bytes(s.size as u64)),
    ];
    let rows = [
        ("Artist", s.artist.clone()),
        ("Album", s.album.clone()),
        ("Album artist", s.album_artist.clone()),
        ("Composer", s.composer.clone()),
        ("Genre", s.genres.join(", ")),
        ("Year", if s.year > 0 { s.year.to_string() } else { String::new() }),
        ("Length", clock(s.duration_ms)),
        ("File", file.into_iter().flatten().collect::<Vec<_>>().join(" · ")),
        ("Folder", s.folder().to_string()),
        ("Plays", plays.to_string()),
        // Spread about evenly over the library: 50 is its median.
        ("Popularity", if s.score > 0 { format!("{} of 100", s.score) } else { String::new() }),
    ];
    let tags = boxed_list();
    for (name, value) in rows.into_iter().filter(|r| !r.1.is_empty()) {
        // No markup: a tag may hold an "&".
        let row = adw::ActionRow::builder()
            .title(name)
            .subtitle(value)
            .use_markup(false)
            .subtitle_selectable(true)
            .css_classes(["property"])
            .build();
        tags.append(&row);
    }
    let b = gtk::Box::builder().orientation(gtk::Orientation::Vertical).spacing(12).build();
    b.append(&tags);
    let dialog = adw::AlertDialog::builder().heading(&s.title).build();
    let lists = app.playlists_of(s.id);
    if !lists.is_empty() {
        b.append(&gtk::Label::builder().label("In Playlists").xalign(0.0).css_classes(["heading"]).build());
        b.append(&playlist_rows(&dialog, &lists));
    }
    present(dialog, &b, parent);
}

/// "In Playlists…" in the song menu, offered only when there is one.
pub fn in_playlists(app: &App, parent: &gtk::Widget, s: &SongRef) {
    let dialog = adw::AlertDialog::builder().heading("In Playlists").body(&s.title).build();
    let rows = playlist_rows(&dialog, &app.playlists_of(s.id));
    present(dialog, &rows, parent);
}

fn boxed_list() -> gtk::ListBox {
    gtk::ListBox::builder().selection_mode(gtk::SelectionMode::None).css_classes(["boxed-list"]).build()
}

/// Each a click away: the playlist opens, and the dialog closes.
fn playlist_rows(dialog: &adw::AlertDialog, lists: &[Playlist]) -> gtk::ListBox {
    let l = boxed_list();
    for p in lists {
        let row = adw::ActionRow::builder().title(&p.name).use_markup(false).activatable(true).build();
        row.add_prefix(&gtk::Image::from_icon_name("view-list-symbolic"));
        let (d, id) = (dialog.downgrade(), p.id);
        row.connect_activated(move |_| {
            let Some(d) = d.upgrade() else { return };
            // While the dialog is still in the window, which has the action.
            let _ = d.activate_action("win.open-playlist", Some(&id.to_variant()));
            d.force_close();
        });
        l.append(&row);
    }
    l
}

fn present(dialog: adw::AlertDialog, child: &impl IsA<gtk::Widget>, parent: &gtk::Widget) {
    let scroll = gtk::ScrolledWindow::builder()
        .child(child)
        .hscrollbar_policy(gtk::PolicyType::Never)
        .propagate_natural_height(true)
        .max_content_height(480)
        .build();
    dialog.set_extra_child(Some(&scroll));
    dialog.add_response("close", "Close");
    dialog.present(Some(parent));
}
