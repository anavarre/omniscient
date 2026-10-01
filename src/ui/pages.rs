//! Sidebar and the three pages: Tasks, Projects, and a single Project.

use std::rc::Rc;

use gtk::{gdk, prelude::*};

use super::rows::{self, RowOpts};
use super::widgets::{self, ellipsized, glyph, hbox, label, vbox, wrapping};
use super::{App, COUNTDOWN, Page, Sel, dialogs, editor};
use crate::dates;
use crate::model::{Grouping, Scope, Section, task_count};

fn nav_button(app: &Rc<App>, g: &str, text: &str, count: Option<usize>, active: bool, go: impl Fn(&Rc<App>) + 'static) -> gtk::Button {
    let content = hbox(8);
    content.append(&glyph(g, &[]));
    let name = ellipsized(text, &[]);
    name.set_hexpand(true);
    // Sidebar width is fixed; long names ellipsize instead of widening it.
    name.set_max_width_chars(1);
    content.append(&name);
    if let Some(n) = count {
        content.append(&label(&n.to_string(), &["count"]));
    }
    let b = gtk::Button::new();
    b.set_child(Some(&content));
    b.add_css_class("nav");
    if active {
        b.add_css_class("active");
    }
    b.set_focusable(false);
    let weak = Rc::downgrade(app);
    b.connect_clicked(move |_| {
        if let Some(app) = weak.upgrade() {
            go(&app);
        }
    });
    b
}

pub fn sidebar(app: &Rc<App>, side: &gtk::Box) {
    widgets::clear(side);
    let snap = app.snap.borrow();
    let st = app.st.borrow();
    let me = app.identity();

    let brand = hbox(8);
    brand.add_css_class("brand");
    brand.append(&glyph(widgets::glyph::TASKS, &[]));
    brand.append(&label("Taskestra", &[]));
    side.append(&brand);

    let search = widgets::button(Some(widgets::glyph::SEARCH), "Search", Some("f"), &["nav"]);
    search.set_focusable(false);
    if let Some(content) = search.child()
        && let Some(l) = content.first_child().and_then(|g| g.next_sibling()) {
            l.set_hexpand(true);
            if let Some(l) = l.downcast_ref::<gtk::Label>() { l.set_xalign(0.0) }
        }
    let weak = Rc::downgrade(app);
    search.connect_clicked(move |_| {
        if let Some(app) = weak.upgrade() {
            dialogs::search(&app);
        }
    });
    side.append(&search);

    side.append(&label("TASKS", &["nav-heading"]));
    if let Some(me) = me {
        let n = snap.visible_tasks(Scope::Mine(me)).len();
        side.append(&nav_button(app, widgets::glyph::USER, "My tasks", Some(n), st.page == Page::Tasks && st.mine, |a| a.set_mine(true)));
    }
    let all = snap.open_top_level(Scope::All).len();
    side.append(&nav_button(app, widgets::glyph::LIST, "All tasks", Some(all), st.page == Page::Tasks && (!st.mine || me.is_none()), |a| {
        a.set_mine(false)
    }));
    let active_projects: Vec<_> = snap.projects.iter().filter(|p| !p.archived()).collect();
    side.append(&nav_button(app, widgets::glyph::FOLDER, "Projects", Some(active_projects.len()), st.page == Page::Projects, |a| {
        a.go(Page::Projects)
    }));

    if !active_projects.is_empty() {
        side.append(&label("PROJECTS", &["nav-heading"]));
        let list = vbox(2);
        for p in &active_projects {
            let id = p.id;
            list.append(&nav_button(app, "\u{f111}", &p.name, (p.open_tasks > 0).then_some(p.open_tasks), st.page == Page::Project(id), move |a| {
                a.go(Page::Project(id))
            }));
        }
        let scroll = gtk::ScrolledWindow::new();
        scroll.set_hscrollbar_policy(gtk::PolicyType::Never);
        scroll.set_propagate_natural_height(true);
        scroll.set_vexpand(true);
        scroll.set_child(Some(&list));
        side.append(&scroll);
        // The project bullets are small dots, not folder icons.
        let mut child = list.first_child();
        while let Some(c) = child {
            if let Some(g) = c.first_child().and_then(|b| b.first_child()) {
                g.add_css_class("dot");
                g.set_opacity(0.6);
                if let Some(l) = g.downcast_ref::<gtk::Label>() {
                    l.set_attributes(Some(&{
                        let a = gtk::pango::AttrList::new();
                        a.insert(gtk::pango::AttrFloat::new_scale(0.55));
                        a
                    }));
                }
            }
            child = c.next_sibling();
        }
    } else {
        side.append(&{
            let s = vbox(0);
            s.set_vexpand(true);
            s
        });
    }

    let footer = vbox(2);
    let who = match me.and_then(|id| snap.person(id)) {
        Some(p) => format!("You are {}", p.name),
        None => "Who are you?".to_string(),
    };
    let settings = widgets::button(Some(widgets::glyph::COG), &who, Some("s"), &["nav"]);
    settings.set_focusable(false);
    settings.set_tooltip_text(Some("Identity & collaborators"));
    if let Some(l) = settings.child().and_then(|c| c.first_child()).and_then(|g| g.next_sibling()) {
        l.set_hexpand(true);
        if let Some(l) = l.downcast_ref::<gtk::Label>() {
            l.set_xalign(0.0);
            l.set_ellipsize(gtk::pango::EllipsizeMode::End);
            l.set_max_width_chars(1);
        }
    }
    let weak = Rc::downgrade(app);
    settings.connect_clicked(move |_| {
        if let Some(app) = weak.upgrade() {
            dialogs::settings(&app, false);
        }
    });
    footer.append(&settings);
    let keys = widgets::button(Some(widgets::glyph::KEYBOARD), "Shortcuts", Some("?"), &["nav"]);
    keys.set_focusable(false);
    if let Some(l) = keys.child().and_then(|c| c.first_child()).and_then(|g| g.next_sibling()) {
        l.set_hexpand(true);
        if let Some(l) = l.downcast_ref::<gtk::Label>() {
            l.set_xalign(0.0);
        }
    }
    let weak = Rc::downgrade(app);
    keys.connect_clicked(move |_| {
        if let Some(app) = weak.upgrade() {
            dialogs::shortcuts(&app);
        }
    });
    footer.append(&keys);
    side.append(&footer);
}

pub fn page(app: &Rc<App>) -> gtk::Widget {
    let page = vbox(0);
    page.add_css_class("page");
    let clamp = vbox(0);
    clamp.set_halign(gtk::Align::Fill);
    clamp.set_hexpand(true);
    let current = app.st.borrow().page;
    match current {
        Page::Tasks => tasks_page(app, &page),
        Page::Projects => projects_page(app, &page),
        Page::Project(id) => project_page(app, &page, id),
    }
    clamp.append(&page);
    clamp.upcast()
}

fn segmented(app: &Rc<App>, options: &[(&str, &str, bool)], on: impl Fn(&Rc<App>, usize) + 'static) -> gtk::Box {
    let seg = hbox(0);
    seg.add_css_class("segmented");
    let on = Rc::new(on);
    for (i, (text, key, active)) in options.iter().enumerate() {
        let b = gtk::Button::with_label(text);
        if *active {
            b.add_css_class("active");
        }
        b.set_focusable(false);
        b.set_tooltip_text(Some(key));
        let weak = Rc::downgrade(app);
        let on = on.clone();
        b.connect_clicked(move |_| {
            if let Some(app) = weak.upgrade() {
                on(&app, i);
            }
        });
        seg.append(&b);
    }
    seg
}

fn add_task_button(app: &Rc<App>) -> gtk::Button {
    let b = widgets::button(Some(widgets::glyph::PLUS), "Add Task", Some("t"), &["btn", "primary"]);
    b.set_valign(gtk::Align::Center);
    b.set_focusable(false);
    let weak = Rc::downgrade(app);
    b.connect_clicked(move |_| {
        if let Some(app) = weak.upgrade() {
            app.new_task();
        }
    });
    b
}

fn controls(app: &Rc<App>, count: usize, project_page: bool) -> gtk::Box {
    let row = hbox(10);
    row.set_margin_top(18);
    let badge = hbox(6);
    badge.add_css_class("count-badge");
    badge.append(&glyph(widgets::glyph::LIST, &[]));
    badge.append(&label(&task_count(count), &[]));
    badge.set_valign(gtk::Align::Center);
    row.append(&badge);

    let me = app.identity();
    let (mine, grouping) = {
        let st = app.st.borrow();
        (st.mine && me.is_some(), st.grouping)
    };
    if !project_page && me.is_some() {
        row.append(&segmented(app, &[("Mine", "gm", mine), ("All", "ga", !mine)], |app, i| app.set_mine(i == 0)));
    }
    row.append(&widgets::spacer());
    if !app.compact() {
        row.append(&label("Group by", &["hint"]));
    }
    let mut options = vec![("Date", "gd", grouping == Grouping::Date)];
    if !project_page {
        options.push(("Project", "gp", grouping == Grouping::Project));
    }
    options.push((if app.compact() { "Who" } else { "Collaborator" }, "gc", grouping == Grouping::Person));
    row.append(&segmented(app, &options, move |app, i| {
        let g = match (project_page, i) {
            (_, 0) => Grouping::Date,
            (false, 1) => Grouping::Project,
            _ => Grouping::Person,
        };
        app.set_grouping(g);
    }));
    row
}

fn empty(g: &str, title: &str, text: &str, hint: Option<&str>) -> gtk::Box {
    let b = vbox(6);
    b.add_css_class("empty");
    b.add_css_class("card");
    b.set_margin_top(18);
    let gl = glyph(g, &[]);
    b.append(&gl);
    let t = label(title, &["title"]);
    t.set_xalign(0.5);
    b.append(&t);
    let x = wrapping(text, &["text"]);
    x.set_xalign(0.5);
    x.set_justify(gtk::Justification::Center);
    b.append(&x);
    if let Some(h) = hint {
        let h = label(h, &["hint"]);
        h.set_use_markup(true);
        h.set_xalign(0.5);
        b.append(&h);
    }
    b
}

fn section_heading(app: &Rc<App>, s: &Section) -> gtk::Box {
    let head = hbox(10);
    let title = label(&s.title, &["section-title"]);
    if let Some(project) = s.project {
        title.add_css_class("link");
        title.set_cursor_from_name(Some("pointer"));
        let click = gtk::GestureClick::new();
        let weak = Rc::downgrade(app);
        click.connect_released(move |_, _, _, _| {
            if let Some(app) = weak.upgrade() {
                app.go(Page::Project(project));
            }
        });
        title.add_controller(click);
    }
    head.append(&title);
    let n = s.tasks.len() + s.hidden.len();
    let count = label(&n.to_string(), &["section-count", "section-title"]);
    count.remove_css_class("section-title");
    count.set_valign(gtk::Align::End);
    count.set_margin_bottom(2);
    head.append(&count);
    head.set_margin_bottom(8);
    head
}

fn sections(app: &Rc<App>, page: &gtk::Box, list: Vec<Section>, scope_key: &str, show_project: bool) {
    let grouping = app.st.borrow().grouping;
    for s in &list {
        page.append(&section_heading(app, s));
        let opts = RowOpts { show_project: show_project && grouping != Grouping::Project };
        let key = format!("{scope_key}:{}", s.title);
        page.append(&rows::card(app, &s.tasks, &s.hidden, &key, opts, false));
    }
}

fn completed_section(app: &Rc<App>, page: &gtk::Box, ids: Vec<i64>, show_project: bool) {
    if ids.is_empty() {
        return;
    }
    let open = app.st.borrow().show_completed;
    let toggle = widgets::button(
        Some(if open { widgets::glyph::CHEVRON_DOWN } else { widgets::glyph::CHEVRON_RIGHT }),
        &format!("Completed ({})", ids.len()),
        None,
        &["disclosure"],
    );
    toggle.set_halign(gtk::Align::Start);
    toggle.set_focusable(false);
    let weak = Rc::downgrade(app);
    toggle.connect_clicked(move |_| {
        if let Some(app) = weak.upgrade() {
            let open = !app.st.borrow().show_completed;
            app.st.borrow_mut().show_completed = open;
            app.render();
        }
    });
    page.append(&toggle);
    if open {
        let card = rows::card(app, &ids, &[], "completed", RowOpts { show_project }, true);
        card.set_margin_top(8);
        page.append(&card);
    }
}

fn tasks_page(app: &Rc<App>, page: &gtk::Box) {
    let scope = app.scope();
    let mine = matches!(scope, Scope::Mine(_));
    let me = app.identity();
    let (name, list, count, completed) = {
        let snap = app.snap.borrow();
        let grouping = app.st.borrow().grouping;
        let name = me.and_then(|id| snap.person(id)).map(|p| p.name.clone()).unwrap_or_default();
        let count = snap.visible_tasks(scope).len();
        (name, snap.grouped(scope, grouping), count, snap.completed(Scope::All))
    };

    let head = hbox(12);
    let (title, subtitle) = if mine {
        (format!("Hi, {name}!"), format!("Here's what's next for you. {}", dates::long(app.snap.borrow().today)))
    } else {
        ("All Tasks".to_string(), "All tasks across every project".to_string())
    };
    let h = super::heading_box(&title, &subtitle);
    h.set_hexpand(true);
    head.append(&h);
    head.append(&add_task_button(app));
    page.append(&head);
    page.append(&controls(app, count, false));

    if list.is_empty() {
        let grouping = app.st.borrow().grouping;
        page.append(&match (mine, grouping) {
            (true, Grouping::Date) => empty(widgets::glyph::STAR, "All clear", "No overdue, today or upcoming tasks assigned to you.", None),
            (true, _) => empty(widgets::glyph::STAR, "All clear", "No tasks assigned to you.", None),
            (false, _) => empty(
                widgets::glyph::TASKS,
                "No tasks yet",
                "Tasks from all your projects will appear here.",
                Some("Hit <b>Add Task</b> (or press <b>t</b>) to get started."),
            ),
        });
    } else {
        sections(app, page, list, if mine { "mine" } else { "all" }, true);
    }
    if me.is_none() {
        let hint = label(
            "Set your identity with the cogwheel in the sidebar (or press <b>s</b>) to see <b>Mine</b> — tasks assigned to you.",
            &["hint"],
        );
        hint.set_use_markup(true);
        hint.set_wrap(true);
        hint.set_margin_top(14);
        page.append(&hint);
    }
    completed_section(app, page, completed, true);
}

fn project_page(app: &Rc<App>, page: &gtk::Box, id: i64) {
    let Some(project) = app.snap.borrow().project(id).cloned() else { return };
    let back = widgets::button(Some(widgets::glyph::BACK), "Projects", Some("esc"), &["flat"]);
    back.set_halign(gtk::Align::Start);
    back.set_focusable(false);
    back.set_margin_bottom(10);
    let weak = Rc::downgrade(app);
    back.connect_clicked(move |_| {
        if let Some(app) = weak.upgrade() {
            app.go(Page::Projects);
        }
    });
    page.append(&back);

    let head = hbox(12);
    let titles = vbox(4);
    titles.set_hexpand(true);
    let name_line = hbox(10);
    let title = wrapping(&project.name, &["page-title"]);
    name_line.append(&title);
    if let Some(d) = project.archived_on {
        let chip = label(&format!("Archived {}", dates::day_month_year(d)), &["chip"]);
        chip.set_valign(gtk::Align::Center);
        name_line.append(&chip);
    }
    titles.append(&name_line);
    if !project.link.is_empty() {
        let link = widgets::button(Some(widgets::glyph::EXTERNAL), &project.link, Some("o"), &["flat"]);
        link.add_css_class("page-meta");
        link.add_css_class("link");
        link.set_halign(gtk::Align::Start);
        link.set_focusable(false);
        let weak = Rc::downgrade(app);
        let uri = project.link.clone();
        link.connect_clicked(move |_| {
            if let Some(app) = weak.upgrade() {
                widgets::open_uri(&app.window, &uri);
            }
        });
        titles.append(&link);
    }
    head.append(&titles);
    let edit = widgets::button(Some(widgets::glyph::EDIT), "Edit", Some("e"), &["btn"]);
    edit.set_valign(gtk::Align::Center);
    edit.set_focusable(false);
    let weak = Rc::downgrade(app);
    edit.connect_clicked(move |_| {
        if let Some(app) = weak.upgrade() {
            editor::open_project(&app, Some(id));
        }
    });
    head.append(&edit);
    head.append(&add_task_button(app));
    page.append(&head);

    if !project.notes.trim().is_empty() {
        let notes = wrapping(project.notes.trim(), &["notes-block"]);
        notes.set_selectable(true);
        notes.set_margin_top(12);
        page.append(&notes);
    }

    let scope = Scope::Project(id);
    let (list, count, completed) = {
        let snap = app.snap.borrow();
        let grouping = app.st.borrow().grouping;
        (snap.grouped(scope, grouping), snap.open_top_level(scope).len(), snap.completed(scope))
    };
    page.append(&controls(app, count, true));
    if list.is_empty() {
        page.append(&empty(
            widgets::glyph::STAR,
            "All clear!",
            "No tasks in this project yet.",
            Some("Hit <b>Add Task</b> (or press <b>t</b>) to get started."),
        ));
    } else {
        sections(app, page, list, &format!("project{id}"), false);
    }
    completed_section(app, page, completed, false);
}

fn projects_page(app: &Rc<App>, page: &gtk::Box) {
    let head = hbox(12);
    let h = super::heading_box("Projects", "Manage and track all your work");
    h.set_hexpand(true);
    head.append(&h);
    let new = widgets::button(Some(widgets::glyph::PLUS), "New Project", Some("P"), &["btn", "primary"]);
    new.set_valign(gtk::Align::Center);
    new.set_focusable(false);
    let weak = Rc::downgrade(app);
    new.connect_clicked(move |_| {
        if let Some(app) = weak.upgrade() {
            editor::open_project(&app, None);
        }
    });
    head.append(&new);
    page.append(&head);

    let (active, archived) = {
        let snap = app.snap.borrow();
        let (a, b): (Vec<_>, Vec<_>) = snap.projects.iter().cloned().partition(|p| !p.archived());
        (a, b)
    };
    if active.is_empty() {
        page.append(&empty(widgets::glyph::FOLDER, "No projects yet", "Create your first project to start tracking tasks.", None));
    } else {
        let card = vbox(0);
        card.add_css_class("card");
        card.set_margin_top(18);
        for p in &active {
            card.append(&project_row(app, p, true));
        }
        page.append(&card);
    }

    if !archived.is_empty() {
        let open = app.st.borrow().show_archived;
        let toggle = widgets::button(
            Some(if open { widgets::glyph::CHEVRON_DOWN } else { widgets::glyph::CHEVRON_RIGHT }),
            &format!("Archived ({})", archived.len()),
            None,
            &["disclosure"],
        );
        toggle.set_halign(gtk::Align::Start);
        toggle.set_focusable(false);
        let weak = Rc::downgrade(app);
        toggle.connect_clicked(move |_| {
            if let Some(app) = weak.upgrade() {
                let open = !app.st.borrow().show_archived;
                app.st.borrow_mut().show_archived = open;
                app.render();
            }
        });
        page.append(&toggle);
        if open {
            let card = vbox(0);
            card.add_css_class("card");
            card.add_css_class("faded");
            card.set_margin_top(8);
            for p in &archived {
                card.append(&project_row(app, p, false));
            }
            page.append(&card);
        }
    }
}

fn project_row(app: &Rc<App>, p: &crate::model::Project, active: bool) -> gtk::Box {
    let id = p.id;
    let (pending, message) = {
        let st = app.st.borrow();
        (st.archiving.get(&id).copied(), st.project_msgs.get(&id).map(|m| m.0.clone()))
    };
    let row = vbox(4);
    row.add_css_class("project-row");
    if !active {
        row.add_css_class("archived");
    }
    if pending.is_some() {
        row.add_css_class("pending");
    }
    let line = hbox(12);
    let bullet = hbox(0);
    bullet.add_css_class("bullet");
    bullet.set_valign(gtk::Align::Center);
    line.append(&bullet);

    let info = vbox(2);
    info.set_hexpand(true);
    let name_line = hbox(8);
    name_line.append(&ellipsized(&p.name, &["name"]));
    if !p.notes.trim().is_empty() {
        let g = glyph(widgets::glyph::NOTE, &["count"]);
        g.set_tooltip_text(Some("Has notes"));
        name_line.append(&g);
    }
    if active {
        let count = label(&task_count(p.open_tasks), &["count"]);
        if p.open_tasks == 0 {
            count.add_css_class("zero");
        }
        name_line.append(&count);
    } else if let Some(d) = p.archived_on {
        name_line.append(&label(&format!("Archived {}", dates::day_month_year(d)), &["count"]));
    }
    info.append(&name_line);
    if !p.link.is_empty() {
        info.append(&ellipsized(&p.link, &["link"]));
    }
    line.append(&info);

    let actions = hbox(4);
    actions.add_css_class("actions");
    actions.set_valign(gtk::Align::Center);
    let action = |g: &str, text: &str, tip: &str, classes: &[&str], f: Box<dyn Fn(&Rc<App>)>| {
        let b = widgets::button(Some(g), text, None, classes);
        b.set_focusable(false);
        b.set_tooltip_text(Some(tip));
        let weak = Rc::downgrade(app);
        b.connect_clicked(move |_| {
            if let Some(app) = weak.upgrade() {
                f(&app);
            }
        });
        b
    };
    if active {
        actions.append(&action(widgets::glyph::EDIT, "Edit", "Edit project (e)", &["flat"], Box::new(move |a| editor::open_project(a, Some(id)))));
        if let Some(started) = pending {
            let ring = widgets::ring(&app.theme, started, COUNTDOWN, app.theme.tokens.borrow().sp(14.0) as i32);
            app.st.borrow_mut().rings.push(ring.clone());
            let cancel = gtk::Button::new();
            let c = hbox(6);
            c.append(&ring);
            c.append(&gtk::Label::new(Some("Cancel")));
            cancel.set_child(Some(&c));
            cancel.add_css_class("flat");
            cancel.set_focusable(false);
            cancel.set_tooltip_text(Some("Cancel archive"));
            let weak = Rc::downgrade(app);
            cancel.connect_clicked(move |_| {
                if let Some(app) = weak.upgrade() {
                    app.toggle_archive(id);
                }
            });
            actions.append(&cancel);
        } else {
            let tip = if p.open_tasks > 0 { "All tasks must be completed for a project to be archived." } else { "Archive project (a)" };
            let b = action(widgets::glyph::ARCHIVE, "Archive", tip, &["flat"], Box::new(move |a| a.toggle_archive(id)));
            if p.open_tasks > 0 {
                b.set_opacity(0.45);
                b.set_cursor_from_name(Some("not-allowed"));
            }
            actions.append(&b);
        }
    } else {
        actions.append(&action(widgets::glyph::UNDO, "Unarchive", "Unarchive project", &["flat"], Box::new(move |a| {
            let name = a.snap.borrow().project_name(id).to_string();
            if a.run(a.db.unarchive_project(id)) {
                a.toast(&format!("\"{name}\" has been unarchived."));
            }
        })));
    }
    let archived = !active;
    actions.append(&action(widgets::glyph::TRASH, "Delete", "Delete project", &["flat", "danger"], Box::new(move |a| {
        let name = a.snap.borrow().project_name(id).to_string();
        let text = if archived {
            format!("Permanently delete archived project \"{name}\" and all its tasks? This cannot be undone.")
        } else {
            format!("Permanently delete project \"{name}\" and all its tasks? This cannot be undone.")
        };
        let weak = Rc::downgrade(a);
        dialogs::confirm(a, "Delete project", &text, "Delete", move || {
            if let Some(app) = weak.upgrade()
                && app.run(app.db.delete_project(id)) {
                    app.toast("Project deleted.");
                }
        });
    })));
    line.append(&actions);
    row.append(&line);
    if let Some(m) = message {
        row.append(&wrapping(&m, &["error"]));
    }

    let click = gtk::GestureClick::new();
    let weak = Rc::downgrade(app);
    click.connect_released(move |g, n, _, _| {
        if n == 1 && g.current_button() == gdk::BUTTON_PRIMARY
            && let Some(app) = weak.upgrade() {
                app.go(Page::Project(id));
            }
    });
    row.add_controller(click);
    if active {
        app.st.borrow_mut().rows.push((Sel::Project(id), row.clone().upcast()));
    }
    row
}
