//! New/Edit Task and New/Edit Project modals.

use std::rc::Rc;

use gtk::{glib, prelude::*};

use super::picker::{Item, Picker, Suggestion};
use super::widgets::{self, glyph, hbox, label, vbox, wrapping};
use super::{App, Sel, dialogs};
use crate::db::{Created, ProjectInput, TaskInput};
use crate::{dates, model::Task};

fn field(title: &str, w: &impl IsA<gtk::Widget>) -> gtk::Box {
    let b = vbox(6);
    b.append(&label(title, &["field-label"]));
    b.append(w);
    b.set_hexpand(true);
    b
}

/// Lets a tall form scroll inside the modal on small windows.
pub fn scrolling(app: &App, body: &impl IsA<gtk::Widget>) -> gtk::ScrolledWindow {
    let scroll = gtk::ScrolledWindow::new();
    scroll.set_hscrollbar_policy(gtk::PolicyType::Never);
    scroll.set_propagate_natural_height(true);
    scroll.set_max_content_height((app.window.height() - 200).max(240));
    scroll.set_child(Some(body));
    scroll
}

pub fn modal_header(app: &Rc<App>, g: &str, title: &str) -> gtk::Box {
    let head = hbox(10);
    head.add_css_class("modal-header");
    head.append(&glyph(g, &["modal-title-glyph"]));
    let t = label(title, &["modal-title"]);
    t.set_hexpand(true);
    head.append(&t);
    let close = widgets::icon_button(widgets::glyph::CLOSE, "Close (Esc)", &[]);
    let weak = Rc::downgrade(app);
    close.connect_clicked(move |_| {
        if let Some(app) = weak.upgrade() {
            app.pop_modal();
        }
    });
    head.append(&close);
    head
}

fn text_area(text: &str, placeholder_lines: i32) -> (gtk::ScrolledWindow, gtk::TextView) {
    let view = gtk::TextView::new();
    view.add_css_class("field");
    view.set_wrap_mode(gtk::WrapMode::WordChar);
    view.set_accepts_tab(false);
    view.buffer().set_text(text);
    let scroll = gtk::ScrolledWindow::new();
    scroll.set_hscrollbar_policy(gtk::PolicyType::Never);
    scroll.set_min_content_height(placeholder_lines * 22);
    scroll.set_max_content_height(260);
    scroll.set_propagate_natural_height(true);
    scroll.set_child(Some(&view));
    (scroll, view)
}

fn buffer_text(view: &gtk::TextView) -> String {
    let b = view.buffer();
    b.text(&b.start_iter(), &b.end_iter(), false).to_string()
}

fn people_picker(app: &Rc<App>) -> Rc<Picker> {
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
    let weak = Rc::downgrade(app);
    let create = Box::new(move |name: &str| {
        let app = weak.upgrade()?;
        match app.db.create_person(name) {
            Ok(created) => {
                let label = match &created {
                    Created::Existing(_, n) => n.clone(),
                    Created::New(_) => name.trim().to_string(),
                };
                app.refresh();
                Some(Item { id: created.id(), label, done: false })
            }
            Err(e) => {
                app.toast_error(&e);
                None
            }
        }
    });
    Picker::new("Search or add people…", false, 0, source, Some(create), None)
}

fn project_picker(app: &Rc<App>) -> Rc<Picker> {
    let weak = Rc::downgrade(app);
    let source = Box::new(move |q: &str, exclude: &[i64]| {
        let Some(app) = weak.upgrade() else { return vec![] };
        let q = q.to_lowercase();
        let snap = app.snap.borrow();
        let mut out: Vec<Suggestion> = snap
            .projects
            .iter()
            .filter(|p| !exclude.contains(&p.id) && p.name.to_lowercase().contains(&q))
            .map(|p| Suggestion { id: p.id, label: p.name.clone(), detail: if p.archived() { "archived".into() } else { String::new() } })
            .collect();
        // Active projects first.
        out.sort_by_key(|s| !s.detail.is_empty());
        out
    });
    let weak = Rc::downgrade(app);
    let create = Box::new(move |name: &str| {
        let app = weak.upgrade()?;
        match app.db.find_or_create_project(name) {
            Ok(created) => {
                let label = match &created {
                    Created::Existing(_, n) => n.clone(),
                    Created::New(_) => name.trim().to_string(),
                };
                app.refresh();
                Some(Item { id: created.id(), label, done: false })
            }
            Err(e) => {
                app.toast_error(&e);
                None
            }
        }
    });
    Picker::new("Select…", true, 0, source, Some(create), None)
}

fn task_picker(app: &Rc<App>, me: Option<i64>, placeholder: &str) -> Rc<Picker> {
    let weak = Rc::downgrade(app);
    let source = Box::new(move |q: &str, exclude: &[i64]| {
        let Some(app) = weak.upgrade() else { return vec![] };
        let mut exclude = exclude.to_vec();
        exclude.extend(me);
        let snap = app.snap.borrow();
        snap.search(q, &exclude)
            .into_iter()
            .filter_map(|id| snap.task(id))
            .map(|t| Suggestion { id: t.id, label: t.what.clone(), detail: snap.project_name(t.project_id).to_string() })
            .collect()
    });
    let weak = Rc::downgrade(app);
    let on_chip: Rc<dyn Fn(i64)> = Rc::new(move |id| {
        if let Some(app) = weak.upgrade() {
            open_task(&app, Some(id), None, None);
        }
    });
    Picker::new(placeholder, false, 1, source, None, Some(on_chip))
}

fn task_items(app: &App, ids: &[i64]) -> Vec<Item> {
    let snap = app.snap.borrow();
    ids.iter().filter_map(|id| snap.task(*id)).map(|t| Item { id: t.id, label: t.what.clone(), done: t.completed() }).collect()
}

/// `project` pre-selects a project, `parent` makes it a new sub-task.
pub fn open_task(app: &Rc<App>, id: Option<i64>, project: Option<i64>, parent: Option<i64>) {
    let existing: Option<Task> = id.and_then(|id| app.snap.borrow().task(id).cloned());
    if id.is_some() && existing.is_none() {
        return;
    }
    let parent = existing.as_ref().map_or(parent, |t| t.parent_id);
    let parent_task = parent.and_then(|p| app.snap.borrow().task(p).cloned());
    let title = match (&existing, parent.is_some()) {
        (Some(_), _) => "Edit Task",
        (None, true) => "New Sub-task",
        (None, false) => "New Task",
    };

    let card = vbox(0);
    card.set_width_request(app.theme.tokens.borrow().sp(720.0).min(app.window.width() as i64 - 48) as i32);
    card.append(&modal_header(app, if existing.is_some() { widgets::glyph::EDIT } else { widgets::glyph::PLUS }, title));

    let body = vbox(14);
    body.add_css_class("modal-body");
    let errors = wrapping("", &["form-errors"]);
    errors.set_visible(false);
    body.append(&errors);

    if let Some(p) = &parent_task {
        let l = label(&format!("Sub-task of “{}” · {}", p.what, app.snap.borrow().project_name(p.project_id)), &["hint"]);
        l.set_ellipsize(gtk::pango::EllipsizeMode::End);
        body.append(&l);
    }

    let what = gtk::Entry::new();
    what.set_placeholder_text(Some("Describe the task…"));
    body.append(&field("WHAT? *", &what));

    let grid = gtk::Grid::new();
    grid.set_column_spacing(14);
    grid.set_row_spacing(14);
    grid.set_column_homogeneous(true);

    let people = people_picker(app);
    grid.attach(&field("WHO?", &people.root), 0, 0, 1, 1);

    let when_box = vbox(4);
    let when_line = hbox(6);
    let when = gtk::Entry::new();
    when.set_placeholder_text(Some("today, fri, +3, oct 12…"));
    when.set_hexpand(true);
    when_line.append(&when);
    let calendar = gtk::Calendar::new();
    let cal_pop = gtk::Popover::new();
    cal_pop.set_child(Some(&calendar));
    let cal_button = gtk::MenuButton::new();
    cal_button.set_child(Some(&gtk::Label::new(Some(widgets::glyph::CALENDAR))));
    cal_button.add_css_class("btn");
    cal_button.set_valign(gtk::Align::Start);
    cal_button.set_tooltip_text(Some("Pick a date"));
    cal_button.set_popover(Some(&cal_pop));
    cal_button.set_focusable(false);
    when_line.append(&cal_button);
    when_box.append(&when_line);
    let when_hint = label("No date", &["field-hint"]);
    when_box.append(&when_hint);
    grid.attach(&field("WHEN?", &when_box), 1, 0, 1, 1);

    let project_picker = project_picker(app);
    if parent_task.is_none() {
        grid.attach(&field("PROJECT *", &project_picker.root), 0, 1, 1, 1);
    }

    let link_line = hbox(6);
    let link = gtk::Entry::new();
    link.set_placeholder_text(Some("https://…"));
    link.set_hexpand(true);
    link_line.append(&link);
    if existing.as_ref().is_some_and(|t| !t.link.is_empty()) {
        let open = widgets::icon_button(widgets::glyph::EXTERNAL, "Open link", &["btn"]);
        let weak = Rc::downgrade(app);
        let l = link.clone();
        open.connect_clicked(move |_| {
            if let Some(app) = weak.upgrade() {
                widgets::open_uri(&app.window, l.text().trim());
            }
        });
        link_line.append(&open);
    }
    grid.attach(&field("LINK", &link_line), if parent_task.is_none() { 1 } else { 0 }, 1, if parent_task.is_none() { 1 } else { 2 }, 1);

    let needs = task_picker(app, id, "Search tasks this one needs…");
    let enables = task_picker(app, id, "Search tasks this one unblocks…");
    grid.attach(&field("NEEDS", &needs.root), 0, 2, 1, 1);
    grid.attach(&field("ENABLES", &enables.root), 1, 2, 1, 1);
    body.append(&grid);

    let (notes_scroll, notes) = text_area(existing.as_ref().map(|t| t.notes.as_str()).unwrap_or(""), 4);
    body.append(&field("NOTES", &notes_scroll));
    card.append(&scrolling(app, &body));

    // ---- initial values
    let me = app.identity();
    {
        let snap = app.snap.borrow();
        match &existing {
            Some(t) => {
                what.set_text(&t.what);
                people.set(t.people.iter().filter_map(|p| snap.person(*p)).map(|p| Item { id: p.id, label: p.name.clone(), done: false }).collect());
                if let Some(d) = t.due_on {
                    when.set_text(&dates::medium(d));
                }
                link.set_text(&t.link);
                if let Some(p) = snap.project(t.project_id) {
                    project_picker.set(vec![Item { id: p.id, label: p.name.clone(), done: false }]);
                }
            }
            None => {
                // You, when you've said who you are; otherwise the web app's default.
                let default = me.and_then(|id| snap.person(id)).or_else(|| snap.people.iter().find(|p| p.name == "John Doe"));
                if let Some(p) = default {
                    people.set(vec![Item { id: p.id, label: p.name.clone(), done: false }]);
                }
                let preset = project.or(parent_task.as_ref().map(|t| t.project_id));
                if let Some(p) = preset.and_then(|p| snap.project(p)) {
                    project_picker.set(vec![Item { id: p.id, label: p.name.clone(), done: false }]);
                }
            }
        }
    }
    if let Some(t) = &existing {
        needs.set(task_items(app, &t.needs));
        enables.set(task_items(app, &t.enables));
    }

    // ---- live date feedback
    let today = app.snap.borrow().today;
    let update_hint = {
        let when = when.clone();
        let hint = when_hint.clone();
        move || {
            let text = when.text();
            if text.trim().is_empty() {
                hint.set_text("No date");
                hint.remove_css_class("bad");
                when.remove_css_class("invalid");
                return;
            }
            match dates::parse(&text, today) {
                Some(d) => {
                    hint.set_text(&dates::describe(d, today));
                    hint.remove_css_class("bad");
                    when.remove_css_class("invalid");
                }
                None => {
                    hint.set_text("Not a date I understand — try “fri”, “+3”, “oct 12”");
                    hint.add_css_class("bad");
                }
            }
        }
    };
    update_hint();
    let uh = update_hint.clone();
    when.connect_changed(move |_| uh());
    {
        let calendar = calendar.clone();
        let when = when.clone();
        cal_pop.connect_show(move |_| {
            let d = dates::parse(&when.text(), today).unwrap_or(today);
            use chrono::Datelike;
            if let Ok(dt) = glib::DateTime::from_local(d.year(), d.month() as i32, d.day() as i32, 0, 0, 0.0) {
                calendar.select_day(&dt);
            }
        });
    }
    {
        let when = when.clone();
        let pop = cal_pop.clone();
        calendar.connect_day_selected(move |c| {
            let dt = c.date();
            if let Some(d) = chrono::NaiveDate::from_ymd_opt(dt.year(), dt.month() as u32, dt.day_of_month() as u32) {
                when.set_text(&dates::medium(d));
            }
            pop.popdown();
        });
    }

    // ---- footer
    let footer = hbox(8);
    footer.add_css_class("modal-footer");
    let completed = existing.as_ref().is_some_and(|t| t.completed());
    if let Some(t) = &existing {
        let tid = t.id;
        let pending = app.st.borrow().pending.contains_key(&tid);
        let complete = widgets::button(
            Some(if completed { widgets::glyph::UNDO } else { widgets::glyph::CHECK }),
            if completed { "Reopen" } else if pending { "Undo complete" } else { "Complete" },
            None,
            &["btn"],
        );
        let weak = Rc::downgrade(app);
        complete.connect_clicked(move |_| {
            if let Some(app) = weak.upgrade() {
                app.pop_modal();
                app.select(Some(Sel::Task(tid)), true);
                app.toggle_complete(tid);
            }
        });
        footer.append(&complete);
        if !completed {
            let progress = widgets::button(
                Some(if t.in_progress { widgets::glyph::STOP } else { widgets::glyph::PLAY }),
                if t.in_progress { "Stop Progress" } else { "Start Progress" },
                None,
                &["btn"],
            );
            let weak = Rc::downgrade(app);
            let on = !t.in_progress;
            progress.connect_clicked(move |_| {
                if let Some(app) = weak.upgrade() {
                    app.pop_modal();
                    app.run(app.db.set_in_progress(tid, on));
                }
            });
            footer.append(&progress);
        }
        let delete = widgets::button(Some(widgets::glyph::TRASH), "Delete", None, &["btn", "danger"]);
        let weak = Rc::downgrade(app);
        delete.connect_clicked(move |_| {
            let Some(app) = weak.upgrade() else { return };
            let weak = Rc::downgrade(&app);
            dialogs::confirm(&app, "Delete task", "Delete this task?", "Delete", move || {
                if let Some(app) = weak.upgrade() {
                    app.close_all_modals();
                    app.st.borrow_mut().pending.remove(&tid);
                    if app.run(app.db.delete_task(tid)) {
                        app.toast("Task deleted.");
                    }
                }
            });
        });
        footer.append(&delete);
    }
    footer.append(&widgets::spacer());
    let cancel = widgets::button(None, "Cancel", Some("esc"), &["btn"]);
    let weak = Rc::downgrade(app);
    cancel.connect_clicked(move |_| {
        if let Some(app) = weak.upgrade() {
            app.pop_modal();
        }
    });
    footer.append(&cancel);
    let save_button = widgets::button(None, if existing.is_some() { "Save Changes" } else { "Add Task" }, Some("⇧↵"), &["btn", "primary"]);
    footer.append(&save_button);
    card.append(&footer);

    // ---- save
    let save: Rc<dyn Fn()> = {
        let weak = Rc::downgrade(app);
        let (what, when, link, notes, errors) = (what.clone(), when.clone(), link.clone(), notes.clone(), errors.clone());
        let (people, project_picker, needs, enables) = (people.clone(), project_picker.clone(), needs.clone(), enables.clone());
        Rc::new(move || {
            let Some(app) = weak.upgrade() else { return };
            let when_text = when.text();
            let due_on = if when_text.trim().is_empty() {
                None
            } else {
                let today = app.snap.borrow().today;
                match dates::parse(&when_text, today) {
                    Some(d) => Some(d),
                    None => {
                        when.add_css_class("invalid");
                        errors.set_text("When? isn't a date I understand");
                        errors.set_visible(true);
                        when.grab_focus();
                        return;
                    }
                }
            };
            let input = TaskInput {
                what: what.text().to_string(),
                project_id: project_picker.ids().first().copied(),
                parent_id: parent,
                due_on,
                link: link.text().to_string(),
                notes: buffer_text(&notes),
                people: people.ids(),
                needs: needs.ids(),
                enables: enables.ids(),
            };
            let result = match id {
                Some(id) => app.db.update_task(id, input).map(|_| id),
                None => app.db.create_task(input),
            };
            match result {
                Ok(saved) => {
                    app.pop_modal();
                    if let Some(p) = parent {
                        app.st.borrow_mut().expanded.insert(p);
                    }
                    app.refresh();
                    app.select(Some(Sel::Task(saved)), true);
                    if id.is_none() {
                        app.toast("Task added.");
                    }
                }
                Err(e) => {
                    errors.set_text(&e);
                    errors.set_visible(true);
                }
            }
        })
    };
    {
        let save = save.clone();
        save_button.connect_clicked(move |_| save());
    }
    for entry in [&what, &when, &link] {
        let save = save.clone();
        entry.connect_activate(move |_| save());
    }
    {
        // Enter in the project field saves once a project is chosen and the
        // suggestions are closed (the picker consumes Enter while they're open).
        let save = save.clone();
        project_picker.entry.connect_activate(move |_| save());
    }

    app.push_modal(card.upcast_ref(), false, Some(save), None);
    let what = what.clone();
    widgets::after_layout(&card, move || {
        what.grab_focus();
    });
}

pub fn open_project(app: &Rc<App>, id: Option<i64>) {
    let existing = id.and_then(|id| app.snap.borrow().project(id).cloned());
    if id.is_some() && existing.is_none() {
        return;
    }
    let card = vbox(0);
    card.set_width_request(app.theme.tokens.borrow().sp(560.0).min(app.window.width() as i64 - 48) as i32);
    card.append(&modal_header(
        app,
        if existing.is_some() { widgets::glyph::EDIT } else { widgets::glyph::FOLDER },
        if existing.is_some() { "Edit Project" } else { "New Project" },
    ));
    let body = vbox(14);
    body.add_css_class("modal-body");
    let errors = wrapping("", &["form-errors"]);
    errors.set_visible(false);
    body.append(&errors);
    let name = gtk::Entry::new();
    name.set_placeholder_text(Some("e.g. Website Redesign"));
    body.append(&field("PROJECT NAME *", &name));
    let link = gtk::Entry::new();
    link.set_placeholder_text(Some("https://…"));
    body.append(&field("LINK", &link));
    let (notes_scroll, notes) = text_area(existing.as_ref().map(|p| p.notes.as_str()).unwrap_or(""), 5);
    body.append(&field("NOTES", &notes_scroll));
    if let Some(p) = &existing {
        name.set_text(&p.name);
        link.set_text(&p.link);
    }
    card.append(&scrolling(app, &body));

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
    let submit = widgets::button(None, if existing.is_some() { "Update Project" } else { "Create Project" }, Some("⇧↵"), &["btn", "primary"]);
    footer.append(&submit);
    card.append(&footer);

    let save: Rc<dyn Fn()> = {
        let weak = Rc::downgrade(app);
        let (name, link, notes, errors) = (name.clone(), link.clone(), notes.clone(), errors.clone());
        Rc::new(move || {
            let Some(app) = weak.upgrade() else { return };
            let input = ProjectInput { name: name.text().to_string(), link: link.text().to_string(), notes: buffer_text(&notes) };
            let result = match id {
                Some(id) => app.db.update_project(id, &input).map(|_| id),
                None => app.db.create_project(&input),
            };
            match result {
                Ok(saved) => {
                    app.pop_modal();
                    app.refresh();
                    if id.is_none() {
                        app.select(Some(Sel::Project(saved)), true);
                        app.toast("Project created.");
                    }
                }
                Err(e) => {
                    errors.set_text(&e);
                    errors.set_visible(true);
                }
            }
        })
    };
    {
        let save = save.clone();
        submit.connect_clicked(move |_| save());
    }
    for entry in [&name, &link] {
        let save = save.clone();
        entry.connect_activate(move |_| save());
    }
    app.push_modal(card.upcast_ref(), false, Some(save), None);
    widgets::after_layout(&card, move || {
        name.grab_focus();
    });
}
