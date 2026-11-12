//! The filters over the search page's results (plan 041), the phone's chips
//! in GTK: Year, Artist, Genre, Popular and Sort, always in view, each
//! naming what it is set to. Year, Artist and Genre open a dialog to set it.

use crate::pages::Nav;
use adw::prelude::*;
use dhun_core::catalog::Group;
use dhun_core::search::{Filters, Query, Sort};
use gtk::{gio, glib};
use std::cell::RefCell;
use std::cmp::Reverse;
use std::collections::HashSet;
use std::ops::RangeInclusive;
use std::rc::Rc;
use std::sync::Arc;

/// Something picked: the results follow.
fn change(nav: &Nav, f: impl FnOnce(&mut Filters)) {
    f(&mut nav.filters.borrow_mut());
    nav.refill_search();
}

/// A filter that is set stands out, as a picked chip does on the phone.
fn accent(w: &impl IsA<gtk::Widget>, on: bool) {
    if on { w.add_css_class("suggested-action") } else { w.remove_css_class("suggested-action") }
}

/// A filter's value once set, its name until then.
fn show(button: &impl IsA<gtk::Widget>, label: &gtk::Label, value: Option<String>, name: &str) {
    label.set_label(value.as_deref().unwrap_or(name));
    accent(button, value.is_some());
}

/// The row of filters, and what shows it as the filters are now: the page calls it at each fill.
pub fn bar(nav: &Rc<Nav>) -> (gtk::Widget, Rc<dyn Fn()>) {
    // Wrapping onto a second line in a narrow window, where in one line the
    // names would shrink to "…" and not say what is set; packed at the start
    // in a wide one, rather than spread over its width.
    let row = gtk::FlowBox::builder()
        .selection_mode(gtk::SelectionMode::None)
        .halign(gtk::Align::Start)
        .max_children_per_line(5)
        .column_spacing(6)
        .row_spacing(6)
        .margin_start(12)
        .margin_end(12)
        .margin_top(6)
        .margin_bottom(6)
        .build();
    // Its buttons take the focus, not the box's own cells around them.
    let add = |w: &gtk::Widget| {
        row.append(w);
        if let Some(cell) = w.parent() {
            cell.set_focusable(false);
        }
    };
    let chip = |name: &str, open: fn(&Rc<Nav>, &gtk::Widget)| {
        let label =
            gtk::Label::builder().label(name).ellipsize(gtk::pango::EllipsizeMode::End).max_width_chars(24).build();
        let inner = gtk::Box::builder().spacing(4).build();
        inner.append(&label);
        inner.append(&gtk::Image::from_icon_name("pan-down-symbolic"));
        let b = gtk::Button::builder().child(&inner).build();
        let n = Rc::downgrade(nav);
        b.connect_clicked(move |b| {
            if let Some(n) = n.upgrade() {
                open(&n, b.upcast_ref());
            }
        });
        add(b.upcast_ref());
        (b, label)
    };
    let year = chip("Year", year_dialog);
    let artist = chip("Artist", |n, b| pick_dialog(n, b, true));
    let genre = chip("Genre", |n, b| pick_dialog(n, b, false));
    let popular = gtk::ToggleButton::builder().label("Popular").tooltip_text("Only the better-known songs").build();
    let n = Rc::downgrade(nav);
    popular.connect_toggled(move |t| {
        let on = t.is_active();
        // Set from the filters too, when there is nothing to change.
        if let Some(n) = n.upgrade().filter(|n| n.filters.borrow().popular != on) {
            change(&n, |f| f.popular = on);
        }
    });
    add(popular.upcast_ref());
    let sorts = gio::Menu::new();
    for (i, s) in Sort::ALL.iter().enumerate() {
        let item = gio::MenuItem::new(Some(s.label()), None);
        item.set_action_and_target_value(Some("filters.sort"), Some(&(i as i32).to_variant()));
        sorts.append_item(&item);
    }
    let sort = gtk::MenuButton::builder().label("Sort").menu_model(&sorts).always_show_arrow(true).build();
    add(sort.upcast_ref());
    // A radio menu: the action's state is the sort picked.
    let pick = gio::SimpleAction::new_stateful("sort", Some(glib::VariantTy::INT32), &0i32.to_variant());
    let n = Rc::downgrade(nav);
    pick.connect_activate(move |_, v| {
        let (Some(n), Some(i)) = (n.upgrade(), v.and_then(|v| v.get::<i32>())) else { return };
        change(&n, |f| f.sort = Sort::ALL[i as usize]);
    });
    let group = gio::SimpleActionGroup::new();
    group.add_action(&pick);
    row.insert_action_group("filters", Some(&group));
    let n = Rc::downgrade(nav);
    let refresh = Rc::new(move || {
        let Some(f) = n.upgrade().map(|n| n.filters.borrow().clone()) else { return };
        show(&year.0, &year.1, f.years.as_ref().map(Filters::years_label), "Year");
        show(&artist.0, &artist.1, Filters::names(&f.artists), "Artist");
        show(&genre.0, &genre.1, Filters::names(&f.genres), "Genre");
        popular.set_active(f.popular);
        accent(&popular, f.popular);
        let sorted = f.sort != Sort::Best;
        sort.set_label(if sorted { f.sort.label() } else { "Sort" });
        accent(&sort, sorted);
        pick.set_state(&(f.sort as i32).to_variant());
    });
    refresh();
    (row.upcast(), refresh)
}

/// Decades a click away, from this one down to the 1950s, or any range typed in.
fn year_dialog(nav: &Rc<Nav>, parent: &gtk::Widget) {
    let now = glib::DateTime::now_local().map_or(2026, |d| d.year() as i64);
    let years = nav.filters.borrow().years.clone();
    let dialog = adw::AlertDialog::builder().heading("Year").build();
    let list = gtk::ListBox::builder().selection_mode(gtk::SelectionMode::None).css_classes(["boxed-list"]).build();
    let mut ranges: Vec<RangeInclusive<i64>> = (1950..=now / 10 * 10).rev().step_by(10).map(|d| d..=d + 9).collect();
    ranges.push(1..=1949);
    for r in ranges {
        let row = adw::ActionRow::builder().title(Filters::years_label(&r)).activatable(true).build();
        if years.as_ref() == Some(&r) {
            row.add_suffix(&gtk::Image::from_icon_name("object-select-symbolic"));
        }
        let (d, n) = (dialog.downgrade(), nav.clone());
        row.connect_activated(move |_| {
            if let Some(d) = d.upgrade() {
                d.force_close();
            }
            change(&n, |f| f.years = Some(r.clone()));
        });
        list.append(&row);
    }
    let field = |name: &str, at: Option<i64>| {
        gtk::Entry::builder()
            .placeholder_text(name)
            .text(at.map_or(String::new(), |y| y.to_string()))
            .input_purpose(gtk::InputPurpose::Digits)
            .max_length(4)
            .width_chars(6)
            .hexpand(true)
            .build()
    };
    let from = field("From", years.as_ref().map(|r| *r.start()));
    let to = field("To", years.as_ref().map(|r| *r.end()));
    let typed = gtk::Box::builder().spacing(12).build();
    typed.append(&from);
    typed.append(&to);
    let b = gtk::Box::builder().orientation(gtk::Orientation::Vertical).spacing(12).build();
    b.append(
        &gtk::ScrolledWindow::builder()
            .child(&list)
            .hscrollbar_policy(gtk::PolicyType::Never)
            .propagate_natural_height(true)
            .max_content_height(280)
            .build(),
    );
    b.append(&typed);
    dialog.set_extra_child(Some(&b));
    dialog.add_responses(&[("any", "Any year"), ("apply", "Apply")]);
    dialog.set_response_appearance("apply", adw::ResponseAppearance::Suggested);
    let n = nav.clone();
    dialog.connect_response(Some("any"), move |_, _| change(&n, |f| f.years = None));
    let n = nav.clone();
    let (f, t) = (from.clone(), to.clone());
    dialog.connect_response(Some("apply"), move |_, _| {
        let year = |e: &gtk::Entry| e.text().chars().filter(char::is_ascii_digit).collect::<String>().parse().ok();
        let (a, b) = (year(&f), year(&t));
        // One end left empty is open: from 1, up to this year.
        let years = (a.is_some() || b.is_some()).then(|| {
            let (a, b) = (a.unwrap_or(1), b.unwrap_or(now));
            a.min(b)..=a.max(b)
        });
        change(&n, |f| f.years = years);
    });
    // Return applies: a field in a dialog does not reach its default response.
    for e in [&from, &to] {
        let d = dialog.downgrade();
        e.connect_activate(move |_| {
            if let Some(d) = d.upgrade() {
                d.emit_by_name_with_details::<()>("response", glib::Quark::from_str("apply"), &[&"apply"]);
                d.force_close();
            }
        });
    }
    dialog.present(Some(parent));
}

/// Several artists or genres, picked from the library's list and searched as
/// the Artists and Genres pages search it: the ones picked first, then the
/// biggest, since the names people look for are the big ones.
fn pick_dialog(nav: &Rc<Nav>, parent: &gtk::Widget, artists: bool) {
    let picked = {
        let f = nav.filters.borrow();
        if artists { f.artists.clone() } else { f.genres.clone() }
    };
    let chosen = Rc::new(RefCell::new(picked.clone()));
    let entry = gtk::SearchEntry::builder().placeholder_text("Search").build();
    // A fixed height: the list builds rows only for what is on screen, the artists are thousands.
    let holder =
        gtk::ScrolledWindow::builder().hscrollbar_policy(gtk::PolicyType::Never).min_content_height(360).build();
    let fill = {
        let (c, chosen, holder) = (nav.app.catalog(), chosen.clone(), holder.clone());
        move |text: &str| {
            // The index is built when first opened, not with the page.
            let index = if artists { c.artist_index() } else { c.genre_index() };
            let lower: HashSet<String> = picked.iter().map(|p| p.to_lowercase()).collect();
            let first = picked.iter().filter_map(|p| index.items.iter().find(|g| g.id() == p.to_lowercase()));
            let mut rest: Vec<&Arc<Group>> =
                index.filter(&Query::new(text)).into_iter().map(|i| &index.items[i]).collect();
            rest.retain(|g| !lower.contains(&g.id()));
            rest.sort_by_key(|g| Reverse(g.songs.len()));
            let items: Vec<Arc<Group>> = first.chain(rest).cloned().collect();
            let chosen = chosen.clone();
            let view = crate::widgets::list(
                &items,
                move |g: &Arc<Group>| {
                    let id = g.id();
                    let line = gtk::Box::builder().spacing(12).build();
                    line.append(
                        &gtk::Label::builder()
                            .label(&g.name)
                            .xalign(0.0)
                            .hexpand(true)
                            .ellipsize(gtk::pango::EllipsizeMode::End)
                            .build(),
                    );
                    line.append(
                        &gtk::Label::builder()
                            .label(g.songs.len().to_string())
                            .css_classes(["dim-label", "numeric", "caption"])
                            .build(),
                    );
                    let on = chosen.borrow().iter().any(|n| n.to_lowercase() == id);
                    let check = gtk::CheckButton::builder().child(&line).active(on).build();
                    let (chosen, name) = (chosen.clone(), g.name.clone());
                    check.connect_toggled(move |b| {
                        let mut c = chosen.borrow_mut();
                        c.retain(|n| n.to_lowercase() != id);
                        if b.is_active() {
                            c.push(name.clone());
                        }
                    });
                    check.upcast()
                },
                |_| {},
            );
            holder.set_child(Some(&view));
        }
    };
    fill("");
    entry.connect_search_changed(move |e| fill(&e.text()));
    let b = gtk::Box::builder().orientation(gtk::Orientation::Vertical).spacing(12).build();
    b.append(&entry);
    b.append(&holder);
    let dialog =
        adw::AlertDialog::builder().heading(if artists { "Artists" } else { "Genres" }).extra_child(&b).build();
    dialog.add_responses(&[("clear", "Clear"), ("apply", "Apply")]);
    dialog.set_response_appearance("apply", adw::ResponseAppearance::Suggested);
    let set = move |nav: &Nav, names: Vec<String>| {
        change(nav, |f| if artists { f.artists = names } else { f.genres = names })
    };
    let n = nav.clone();
    dialog.connect_response(Some("clear"), move |_, _| set(&n, vec![]));
    let n = nav.clone();
    dialog.connect_response(Some("apply"), move |_, _| set(&n, chosen.borrow().clone()));
    dialog.present(Some(parent));
}
