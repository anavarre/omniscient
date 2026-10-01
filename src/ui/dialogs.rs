//! Confirm, keyboard shortcuts, the search palette and settings.

use std::cell::{Cell, RefCell};
use std::rc::Rc;

use gtk::{gdk, glib, prelude::*};

use super::editor::{self, modal_header};
use super::picker::{Item, Picker, Suggestion};
use super::widgets::{self, ellipsized, glyph, hbox, label, vbox, wrapping};
use super::{App, Page};
use crate::db::Created;
use crate::model::task_count;

pub fn confirm(app: &Rc<App>, title: &str, text: &str, action: &str, on_yes: impl Fn() + 'static) {
    let card = vbox(0);
    card.set_width_request(app.theme.tokens.borrow().sp(440.0) as i32);
    card.append(&modal_header(app, "\u{f071}", title));
    let body = vbox(0);
    body.add_css_class("modal-body");
    body.append(&wrapping(text, &[]));
    card.append(&body);
    let footer = hbox(8);
    footer.add_css_class("modal-footer");
    footer.append(&widgets::spacer());
    let cancel = widgets::button(None, "Cancel", Some("esc"), &["btn"]);
    let weak = Rc::downgrade(app);
    cancel.connect_clicked(move |_| {
        if let Some(app) = weak.upgrade() {
            app.pop_modal();
        }
    });
    footer.append(&cancel);
    let yes = widgets::button(None, action, Some("↵"), &["btn", "primary", "danger"]);
    footer.append(&yes);
    card.append(&footer);

    let weak = Rc::downgrade(app);
    let on_yes = Rc::new(on_yes);
    let go: Rc<dyn Fn()> = Rc::new(move || {
        if let Some(app) = weak.upgrade() {
            app.pop_modal();
        }
        on_yes();
    });
    let g = go.clone();
    yes.connect_clicked(move |_| g());
    app.push_modal(card.upcast_ref(), false, Some(go), None);
    widgets::after_layout(&card, move || {
        yes.grab_focus();
    });
}

pub fn shortcuts(app: &Rc<App>) {
    let groups: &[(&str, &[(&str, &str)])] = &[
        ("NAVIGATION", &[("h", "Go to tasks"), ("p", "Go to projects"), ("g g", "Scroll to top"), ("G", "Scroll to bottom"), ("f  /  ctrl k", "Search")]),
        (
            "GROUPING",
            &[("g d", "Group by date"), ("g p", "Group by project"), ("g c", "Group by collaborator"), ("g m", "Filter: my tasks"), ("g a", "Filter: all tasks")],
        ),
        (
            "ACTIONS",
            &[("t", "Add task"), ("P", "New project"), ("e", "Edit project"), ("a", "Archive project"), ("C", "Add collaborator"), ("s", "Identity & collaborators")],
        ),
        (
            "LIST",
            &[
                ("j  k", "Move selection"),
                ("enter", "Open selected"),
                ("x", "Complete / undo"),
                ("o", "Open selected link"),
                ("space  ←  →", "Toggle sub-tasks"),
                ("J  K", "Reorder sub-task"),
            ],
        ),
        ("GENERAL", &[("⇧ enter", "Save the open form"), ("esc", "Close / go back"), ("?", "Show shortcuts")]),
    ];
    let card = vbox(0);
    card.set_width_request(app.theme.tokens.borrow().sp(640.0).min(app.window.width() as i64 - 48) as i32);
    card.append(&modal_header(app, widgets::glyph::KEYBOARD, "Keyboard Shortcuts"));
    let body = gtk::Grid::new();
    body.add_css_class("modal-body");
    body.set_column_spacing(28);
    body.set_column_homogeneous(true);
    for (i, (title, keys)) in groups.iter().enumerate() {
        let col = vbox(6);
        col.append(&label(title, &["shortcut-group"]));
        for (k, what) in keys.iter() {
            let line = hbox(10);
            let name = label(what, &[]);
            name.set_hexpand(true);
            line.append(&name);
            for part in k.split("  ") {
                line.append(&label(part, &["kbd"]));
            }
            col.append(&line);
        }
        body.attach(&col, (i % 2) as i32, (i / 2) as i32, 1, 1);
    }
    card.append(&editor::scrolling(app, &body));
    app.push_modal(card.upcast_ref(), false, None, None);
}

enum Hit {
    Task(i64),
    Project(i64),
}

/// The search palette: tasks (title and notes) and projects, by keyboard.
pub fn search(app: &Rc<App>) {
    let card = vbox(0);
    card.set_width_request(app.theme.tokens.borrow().sp(620.0).min(app.window.width() as i64 - 48) as i32);
    let entry = gtk::Entry::new();
    entry.add_css_class("palette-entry");
    entry.set_placeholder_text(Some("Search for any task or project…"));
    entry.set_primary_icon_name(None);
    card.append(&entry);
    let results = vbox(0);
    let scroll = gtk::ScrolledWindow::new();
    scroll.set_hscrollbar_policy(gtk::PolicyType::Never);
    scroll.set_propagate_natural_height(true);
    scroll.set_max_content_height((app.window.height() as f64 * 0.5) as i32);
    scroll.set_child(Some(&results));
    card.append(&scroll);
    let footer = label("↑↓ move   ↵ open   esc close", &["palette-footer"]);
    card.append(&footer);

    let hits: Rc<RefCell<Vec<(Hit, gtk::Box)>>> = Rc::new(RefCell::new(vec![]));
    let cursor = Rc::new(Cell::new(0usize));

    let open = {
        let weak = Rc::downgrade(app);
        let hits = hits.clone();
        let cursor = cursor.clone();
        Rc::new(move |index: Option<usize>| {
            let Some(app) = weak.upgrade() else { return };
            let target = {
                let hits = hits.borrow();
                let i = index.unwrap_or(cursor.get());
                hits.get(i).map(|(h, _)| match h {
                    Hit::Task(id) => Hit::Task(*id),
                    Hit::Project(id) => Hit::Project(*id),
                })
            };
            let Some(target) = target else { return };
            app.pop_modal();
            match target {
                Hit::Task(id) => editor::open_task(&app, Some(id), None, None),
                Hit::Project(id) => app.go(Page::Project(id)),
            }
        })
    };

    let highlight = {
        let hits = hits.clone();
        let cursor = cursor.clone();
        let scroll = scroll.clone();
        Rc::new(move |i: usize| {
            let hits = hits.borrow();
            for (j, (_, w)) in hits.iter().enumerate() {
                if j == i { w.add_css_class("selected") } else { w.remove_css_class("selected") }
            }
            cursor.set(i);
            if let Some((_, w)) = hits.get(i)
                && let Some(b) = w.compute_bounds(&scroll.child().unwrap()) {
                    let adj = scroll.vadjustment();
                    let (top, bottom) = (b.y() as f64, (b.y() + b.height()) as f64);
                    if top < adj.value() {
                        adj.set_value(top);
                    } else if bottom > adj.value() + adj.page_size() {
                        adj.set_value(bottom - adj.page_size());
                    }
                }
        })
    };

    let update = {
        let weak = Rc::downgrade(app);
        let results = results.clone();
        let hits = hits.clone();
        let highlight = highlight.clone();
        let open = open.clone();
        move |q: &str| {
            let Some(app) = weak.upgrade() else { return };
            widgets::clear(&results);
            hits.borrow_mut().clear();
            let q = q.trim().to_lowercase();
            if q.is_empty() {
                results.append(&label("Type to search tasks by title or notes, and projects by name.", &["palette-empty"]));
                return;
            }
            let snap = app.snap.borrow();
            let mut found: Vec<(Hit, String, String, &str)> = snap
                .search(&q, &[])
                .into_iter()
                .filter_map(|id| snap.task(id))
                .map(|t| {
                    let detail = match t.parent_id.and_then(|p| snap.task(p)) {
                        Some(parent) => format!("{} › {}", snap.project_name(t.project_id), parent.what),
                        None => snap.project_name(t.project_id).to_string(),
                    };
                    (Hit::Task(t.id), t.what.clone(), detail, widgets::glyph::CHECK)
                })
                .collect();
            for p in snap.projects.iter().filter(|p| p.name.to_lowercase().contains(&q)) {
                let detail = if p.archived() { "archived project".to_string() } else { format!("project · {}", task_count(p.open_tasks)) };
                found.push((Hit::Project(p.id), p.name.clone(), detail, widgets::glyph::FOLDER));
            }
            if found.is_empty() {
                results.append(&label("No tasks found", &["palette-empty"]));
                return;
            }
            for (i, (hit, title, detail, g)) in found.into_iter().enumerate() {
                let row = hbox(12);
                row.add_css_class("palette-row");
                row.append(&glyph(g, &[]));
                let t = ellipsized(&title, &["title"]);
                t.set_hexpand(true);
                row.append(&t);
                row.append(&ellipsized(&detail, &["detail"]));
                let click = gtk::GestureClick::new();
                let open = open.clone();
                click.connect_released(move |_, _, _, _| open(Some(i)));
                row.add_controller(click);
                let motion = gtk::EventControllerMotion::new();
                let hl = highlight.clone();
                motion.connect_enter(move |_, _, _| hl(i));
                row.add_controller(motion);
                results.append(&row);
                hits.borrow_mut().push((hit, row));
            }
            highlight(0);
        }
    };
    update("");
    entry.connect_changed(move |e| update(&e.text()));
    {
        let open = open.clone();
        entry.connect_activate(move |_| open(None));
    }
    let keys = gtk::EventControllerKey::new();
    keys.set_propagation_phase(gtk::PropagationPhase::Capture);
    {
        let hits = hits.clone();
        let cursor = cursor.clone();
        keys.connect_key_pressed(move |_, key, _, state| {
            let n = hits.borrow().len();
            let ctrl = state.contains(gdk::ModifierType::CONTROL_MASK);
            let delta = match key {
                gdk::Key::Down => 1,
                gdk::Key::Up => -1,
                gdk::Key::j | gdk::Key::n if ctrl => 1,
                gdk::Key::k | gdk::Key::p if ctrl => -1,
                _ => return glib::Propagation::Proceed,
            };
            if n > 0 {
                highlight((cursor.get() as i32 + delta).clamp(0, n as i32 - 1) as usize);
            }
            glib::Propagation::Stop
        });
    }
    entry.add_controller(keys);

    app.push_modal(card.upcast_ref(), true, None, None);
    widgets::after_layout(&card, move || {
        entry.grab_focus();
    });
}

/// Identity ("Who are you?") and collaborators.
pub fn settings(app: &Rc<App>, add_collaborator: bool) {
    let card = vbox(0);
    card.set_width_request(app.theme.tokens.borrow().sp(520.0).min(app.window.width() as i64 - 48) as i32);
    card.append(&modal_header(app, widgets::glyph::COG, "Settings"));
    let body = vbox(10);
    body.add_css_class("modal-body");

    body.append(&label("WHO ARE YOU?", &["field-label"]));
    let weak = Rc::downgrade(app);
    let source = Box::new(move |q: &str, exclude: &[i64]| {
        let Some(app) = weak.upgrade() else { return vec![] };
        let q = q.to_lowercase();
        app.snap
            .borrow()
            .people
            .iter()
            .filter(|p| !exclude.contains(&p.id) && p.name.to_lowercase().contains(&q))
            .map(|p| Suggestion { id: p.id, label: p.name.clone(), detail: String::new() })
            .collect()
    });
    let identity = Picker::new("Select your name…", true, 0, source, None, None);
    let me = app.identity().and_then(|id| app.snap.borrow().person(id).cloned());
    if let Some(p) = me {
        identity.set(vec![Item { id: p.id, label: p.name, done: false }]);
    }
    body.append(&identity.root);
    body.append(&label("Your tasks show under “My tasks”, and new tasks are assigned to you.", &["field-hint"]));

    let head = hbox(8);
    head.set_margin_top(14);
    let t = label("COLLABORATORS", &["field-label"]);
    t.set_hexpand(true);
    t.set_valign(gtk::Align::Center);
    head.append(&t);
    let add = widgets::button(Some(widgets::glyph::PLUS), "Add", Some("C"), &["btn"]);
    head.append(&add);
    body.append(&head);

    let add_row = hbox(8);
    let add_entry = gtk::Entry::new();
    add_entry.set_placeholder_text(Some("Full name, e.g. Jane Smith"));
    add_entry.set_hexpand(true);
    add_row.append(&add_entry);
    let add_go = widgets::button(None, "Add Collaborator", None, &["btn", "primary"]);
    add_row.append(&add_go);
    add_row.set_visible(add_collaborator);
    body.append(&add_row);
    let add_error = label("", &["form-errors"]);
    add_error.set_visible(false);
    body.append(&add_error);

    let list = vbox(2);
    body.append(&list);
    card.append(&editor::scrolling(app, &body));

    let editing: Rc<Cell<Option<i64>>> = Rc::new(Cell::new(None));
    let errors: Rc<RefCell<std::collections::HashMap<i64, String>>> = Rc::default();
    let render: Rc<RefCell<Option<Box<dyn Fn()>>>> = Rc::new(RefCell::new(None));
    {
        let weak = Rc::downgrade(app);
        let list = list.downgrade();
        let editing = editing.clone();
        let errors = errors.clone();
        let render_ref = Rc::downgrade(&render);
        *render.borrow_mut() = Some(Box::new(move || {
            let (Some(app), Some(list)) = (weak.upgrade(), list.upgrade()) else { return };
            let rerender = {
                let render_ref = render_ref.clone();
                move || {
                    if let Some(r) = render_ref.upgrade()
                        && let Some(f) = r.borrow().as_ref() {
                            f();
                        }
                }
            };
            widgets::clear(&list);
            let people = app.snap.borrow().people.clone();
            let me = app.identity();
            if people.is_empty() {
                list.append(&label("No collaborators yet. Add one to get started.", &["hint"]));
            }
            for p in people {
                let pid = p.id;
                let row = vbox(2);
                row.add_css_class("person-row");
                if me == Some(pid) {
                    row.add_css_class("me");
                }
                let line = hbox(10);
                line.append(&widgets::avatar(&p.name, false));
                if editing.get() == Some(pid) {
                    let e = gtk::Entry::new();
                    e.set_text(&p.name);
                    e.set_hexpand(true);
                    line.append(&e);
                    let save = widgets::button(None, "Save", None, &["btn", "primary"]);
                    let cancel = widgets::button(None, "Cancel", None, &["btn"]);
                    line.append(&save);
                    line.append(&cancel);
                    let do_save = {
                        let weak = Rc::downgrade(&app);
                        let e = e.clone();
                        let editing = editing.clone();
                        let errors = errors.clone();
                        let rerender = rerender.clone();
                        Rc::new(move || {
                            let Some(app) = weak.upgrade() else { return };
                            match app.db.rename_person(pid, &e.text()) {
                                Ok(()) => {
                                    editing.set(None);
                                    errors.borrow_mut().remove(&pid);
                                    app.refresh();
                                }
                                Err(msg) => {
                                    errors.borrow_mut().insert(pid, msg);
                                }
                            }
                            rerender();
                        })
                    };
                    let cancel_edit = {
                        let editing = editing.clone();
                        let errors = errors.clone();
                        let rerender = rerender.clone();
                        Rc::new(move || {
                            editing.set(None);
                            errors.borrow_mut().remove(&pid);
                            rerender();
                        })
                    };
                    let s = do_save.clone();
                    save.connect_clicked(move |_| s());
                    let s = do_save.clone();
                    e.connect_activate(move |_| s());
                    let c = cancel_edit.clone();
                    cancel.connect_clicked(move |_| c());
                    // Esc cancels the rename before it can close the dialog.
                    let keys = gtk::EventControllerKey::new();
                    let c = cancel_edit.clone();
                    keys.connect_key_pressed(move |_, key, _, _| {
                        if key == gdk::Key::Escape {
                            c();
                            glib::Propagation::Stop
                        } else {
                            glib::Propagation::Proceed
                        }
                    });
                    e.add_controller(keys);
                    widgets::after_layout(&e, {
                        let e = e.clone();
                        move || {
                            e.grab_focus();
                        }
                    });
                } else {
                    let name = ellipsized(&p.name, &["name"]);
                    name.set_hexpand(true);
                    line.append(&name);
                    if me == Some(pid) {
                        line.append(&label("you", &["chip"]));
                    }
                    let count = label(&task_count(p.open_tasks), &["count"]);
                    if p.open_tasks == 0 {
                        count.add_css_class("zero");
                    }
                    line.append(&count);
                    let actions = hbox(2);
                    actions.add_css_class("actions");
                    let edit = widgets::button(Some(widgets::glyph::EDIT), "Edit", None, &["flat"]);
                    let remove = widgets::button(Some(widgets::glyph::TRASH), "Remove", None, &["flat", "danger"]);
                    let ed = editing.clone();
                    let rr = rerender.clone();
                    edit.connect_clicked(move |_| {
                        ed.set(Some(pid));
                        rr();
                    });
                    let weak = Rc::downgrade(&app);
                    let errs = errors.clone();
                    let rr = rerender.clone();
                    remove.connect_clicked(move |_| {
                        let Some(app) = weak.upgrade() else { return };
                        match app.db.delete_person(pid) {
                            Ok(()) => {
                                errs.borrow_mut().remove(&pid);
                                app.refresh();
                            }
                            Err(msg) => {
                                errs.borrow_mut().insert(pid, msg);
                            }
                        }
                        rr();
                    });
                    actions.append(&edit);
                    actions.append(&remove);
                    line.append(&actions);
                }
                row.append(&line);
                if let Some(msg) = errors.borrow().get(&pid) {
                    let l = label(msg, &["error"]);
                    l.add_css_class("blocked");
                    l.set_margin_start(30);
                    row.append(&l);
                }
                list.append(&row);
            }
        }));
    }
    let rerender = {
        let render = render.clone();
        move || {
            if let Some(f) = render.borrow().as_ref() {
                f();
            }
        }
    };
    rerender();

    {
        // Identity applies as soon as it changes.
        let weak = Rc::downgrade(app);
        let rr = rerender.clone();
        let identity_ref = Rc::downgrade(&identity);
        identity.connect_change(move || {
            let (Some(app), Some(identity)) = (weak.upgrade(), identity_ref.upgrade()) else { return };
            let chosen = identity.ids().first().copied();
            if chosen == app.identity() {
                return;
            }
            let _ = app.db.set_setting("identity", chosen.map(|id| id.to_string()).as_deref());
            let _ = app.db.set_setting("filter", Some(if chosen.is_some() { "mine" } else { "all" }));
            app.st.borrow_mut().mine = chosen.is_some();
            app.refresh();
            app.toast(if chosen.is_some() { "Identity updated." } else { "Identity cleared." });
            rr();
        });
    }

    let do_add: Rc<dyn Fn()> = {
        let weak = Rc::downgrade(app);
        let add_entry = add_entry.clone();
        let add_error = add_error.clone();
        let rr = rerender.clone();
        Rc::new(move || {
            let Some(app) = weak.upgrade() else { return };
            match app.db.create_person(&add_entry.text()) {
                Ok(Created::New(_)) => {
                    add_entry.set_text("");
                    add_error.set_visible(false);
                    app.refresh();
                    rr();
                }
                Ok(Created::Existing(_, name)) => {
                    add_error.set_text(&format!("\"{name}\" already exists as a collaborator."));
                    add_error.set_visible(true);
                }
                Err(e) => {
                    add_error.set_text(&e);
                    add_error.set_visible(true);
                }
            }
        })
    };
    {
        let d = do_add.clone();
        add_go.connect_clicked(move |_| d());
        let d = do_add.clone();
        add_entry.connect_activate(move |_| d());
        let add_row = add_row.clone();
        let add_entry = add_entry.clone();
        add.connect_clicked(move |_| {
            add_row.set_visible(true);
            add_entry.grab_focus();
        });
    }

    let submit: Option<Rc<dyn Fn()>> = if add_collaborator { Some(do_add.clone()) } else { None };
    app.push_modal(card.upcast_ref(), false, submit, None);
    if add_collaborator {
        widgets::after_layout(&card, move || {
            add_entry.grab_focus();
        });
    }
}
