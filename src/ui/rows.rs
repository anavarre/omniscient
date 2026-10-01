//! Task rows and the section cards that hold them.

use std::rc::Rc;

use gtk::{gdk, glib, prelude::*};

use super::widgets::{self, ellipsized, glyph, hbox, label, vbox, wrapping};
use super::{App, COUNTDOWN, Sel, editor};
use crate::dates;
use crate::model::{Due, Task};

#[derive(Clone, Copy)]
pub struct RowOpts {
    pub show_project: bool,
}

pub fn columns(app: &App) -> (i32, i32) {
    let t = app.theme.tokens.borrow();
    if app.narrow() {
        (t.sp(52.0) as i32, t.sp(96.0) as i32)
    } else if app.compact() {
        (t.sp(150.0) as i32, t.sp(104.0) as i32)
    } else {
        (t.sp(180.0) as i32, t.sp(112.0) as i32)
    }
}

/// A bordered card with the "What? / Who? / When?" header and rows.
pub fn card(app: &Rc<App>, ids: &[i64], hidden: &[i64], key: &str, opts: RowOpts, faded: bool) -> gtk::Box {
    let card = vbox(0);
    card.add_css_class("card");
    if faded {
        card.add_css_class("faded");
    }
    let (who_w, when_w) = columns(app);
    let header = hbox(12);
    header.add_css_class("col-header");
    let what = label("WHAT?", &[]);
    what.set_hexpand(true);
    header.append(&what);
    let who = label("WHO?", &[]);
    who.set_width_request(who_w);
    header.append(&who);
    let when = label("WHEN?", &[]);
    when.set_width_request(when_w);
    header.append(&when);
    card.append(&header);

    let rows = vbox(0);
    rows.add_css_class("rows");
    card.append(&rows);
    let revealed = app.st.borrow().revealed.contains(key);
    for id in ids.iter().chain(if revealed { hidden.iter() } else { [].iter() }) {
        // The first screenfuls are built now; the rest stream in between frames.
        let mut st = app.st.borrow_mut();
        if st.budget > 0 {
            st.budget -= 1;
            drop(st);
            rows.append(&task_block(app, *id, opts));
        } else {
            st.deferred.push((rows.clone(), *id, opts));
        }
    }
    if !hidden.is_empty() && !revealed {
        let more = widgets::button(Some(widgets::glyph::CHEVRON_DOWN), &format!("Show {} more", hidden.len()), None, &["more"]);
        more.set_focusable(false);
        let weak = Rc::downgrade(app);
        let key = key.to_string();
        more.connect_clicked(move |_| {
            if let Some(app) = weak.upgrade() {
                app.st.borrow_mut().revealed.insert(key.clone());
                app.render();
            }
        });
        card.append(&more);
    }
    card
}

/// A top-level task row plus its (expanded) sub-tasks.
pub fn task_block(app: &Rc<App>, id: i64, opts: RowOpts) -> gtk::Box {
    let block = vbox(0);
    block.add_css_class("task-block");
    block.append(&task_row(app, id, opts, false));
    let (expanded, subtasks) = {
        let snap = app.snap.borrow();
        let subtasks = snap.task(id).map(|t| t.subtasks.clone()).unwrap_or_default();
        (app.st.borrow().expanded.contains(&id), subtasks)
    };
    if expanded {
        for sub in subtasks {
            block.append(&task_row(app, sub, RowOpts { show_project: false }, true));
        }
    }
    block
}

fn esc(s: &str) -> String {
    glib::markup_escape_text(s).to_string()
}

/// A wrapping line of clickable items: `task:ID`, `project:ID` or a URL.
/// One label instead of a widget per chip keeps long lists fast and lets
/// the line wrap naturally on narrow tiles.
fn link_line(app: &Rc<App>, markup: &str, classes: &[&str]) -> gtk::Label {
    let l = wrapping("", classes);
    l.set_markup(markup);
    l.add_css_class("linkline");
    let weak = Rc::downgrade(app);
    l.connect_activate_link(move |_, uri| {
        let Some(app) = weak.upgrade() else { return glib::Propagation::Stop };
        if let Some(id) = uri.strip_prefix("task:").and_then(|i| i.parse().ok()) {
            editor::open_task(&app, Some(id), None, None);
        } else if let Some(id) = uri.strip_prefix("project:").and_then(|i| i.parse().ok()) {
            app.go(super::Page::Project(id));
        } else {
            widgets::open_uri(&app.window, uri);
        }
        glib::Propagation::Stop
    });
    l
}

/// "Needs" / "Enables": a marker glyph, then each task as a link; done
/// ones struck through.
fn dependency_line(app: &Rc<App>, g: &str, tip: &str, ids: &[i64]) -> gtk::Label {
    let snap = app.snap.borrow();
    let items: Vec<String> = ids
        .iter()
        .filter_map(|d| snap.task(*d))
        .map(|t| {
            let title = esc(&t.what);
            let title = if t.completed() { format!("<s>{title}</s>") } else { title };
            format!("<a href=\"task:{}\" title=\"{}\">{title}</a>", t.id, esc(snap.project_name(t.project_id)))
        })
        .collect();
    let l = link_line(app, &format!("<span alpha=\"60%\">{g}</span>  {}", items.join("  ·  ")), &["deps"]);
    l.set_tooltip_text(Some(tip));
    l
}

fn who_column(app: &Rc<App>, task: &Task, width: i32) -> gtk::Box {
    let who = vbox(4);
    who.set_width_request(width);
    who.set_valign(gtk::Align::Start);
    let snap = app.snap.borrow();
    if app.narrow() {
        // Avatars only; names in the tooltip.
        let line = gtk::FlowBox::new();
        line.set_selection_mode(gtk::SelectionMode::None);
        line.set_column_spacing(3);
        line.set_row_spacing(3);
        line.set_max_children_per_line(2);
        line.set_can_target(true);
        if task.people.is_empty() {
            let a = widgets::avatar("", true);
            a.set_tooltip_text(Some("Unassigned"));
            line.insert(&a, -1);
        }
        for person in task.people.iter().filter_map(|p| snap.person(*p)) {
            let a = widgets::avatar(&person.name, false);
            a.set_tooltip_text(Some(&person.name));
            line.insert(&a, -1);
        }
        who.append(&line);
        return who;
    }
    if task.people.is_empty() {
        let line = hbox(8);
        line.append(&widgets::avatar("", true));
        line.append(&label("Unassigned", &["who-none"]));
        who.append(&line);
    }
    for person in task.people.iter().filter_map(|p| snap.person(*p)) {
        let line = hbox(8);
        line.append(&widgets::avatar(&person.name, false));
        line.append(&ellipsized(&person.name, &["who-name"]));
        who.append(&line);
    }
    who
}

fn when_column(app: &Rc<App>, task: &Task, width: i32) -> gtk::Box {
    let when = hbox(0);
    when.set_width_request(width);
    when.set_valign(gtk::Align::Start);
    let snap = app.snap.borrow();
    match task.due_on {
        Some(d) => {
            let badge = hbox(5);
            badge.add_css_class("date");
            badge.append(&glyph(widgets::glyph::CALENDAR, &[]));
            badge.append(&label(&dates::short(d, snap.today), &[]));
            match (task.completed(), snap.due(task)) {
                (true, _) => badge.add_css_class("done"),
                (false, Due::Overdue) => badge.add_css_class("overdue"),
                (false, Due::Today) => badge.add_css_class("today"),
                _ => {}
            }
            let tip = if task.due_on_auto_set { format!("{} · set automatically", dates::long(d)) } else { dates::long(d) };
            badge.set_tooltip_text(Some(&tip));
            when.append(&badge);
        }
        None => {
            when.append(&label("—", &["date-none"]));
        }
    }
    when
}

pub fn task_row(app: &Rc<App>, id: i64, opts: RowOpts, subtask: bool) -> gtk::Widget {
    let snap = app.snap.borrow();
    let Some(task) = snap.task(id).cloned() else { return vbox(0).upcast() };
    let st = app.st.borrow();
    let pending = st.pending.get(&id).copied();
    let message = st.task_msgs.get(&id).map(|(m, _, _)| m.clone());
    let expanded = st.expanded.contains(&id);
    drop(st);
    let conflicts: Vec<String> = snap.date_conflicts(&task).iter().map(|t| t.what.clone()).collect();
    let project_name = snap.project_name(task.project_id).to_string();
    drop(snap);
    let (who_w, when_w) = columns(app);
    let tokens = app.theme.tokens.borrow().clone();

    let row = vbox(4);
    row.add_css_class("task-row");
    if subtask {
        row.add_css_class("subtask");
    }
    if task.completed() {
        row.add_css_class("completed");
    }
    if pending.is_some() {
        row.add_css_class("pending");
    }

    let main = hbox(12);
    // [handle] [check] [chevron]  | title line, meta, notes, needs, enables
    let line = hbox(8);
    line.set_hexpand(true);
    if subtask {
        let handle = label(widgets::glyph::DRAG, &["handle"]);
        handle.set_tooltip_text(Some("Drag to reorder (or Shift+J / Shift+K)"));
        handle.set_valign(gtk::Align::Start);
        line.append(&handle);
        install_drag(app, &row, &handle, id);
    }

    let check = gtk::Button::with_label(widgets::glyph::CHECK);
    check.add_css_class("check");
    if task.completed() || pending.is_some() {
        check.add_css_class("done");
    }
    check.set_valign(gtk::Align::Start);
    check.set_focusable(false);
    check.set_tooltip_text(Some(if task.completed() {
        "Mark incomplete"
    } else if pending.is_some() {
        "Undo"
    } else {
        "Complete (x)"
    }));
    {
        let weak = Rc::downgrade(app);
        check.connect_clicked(move |_| {
            if let Some(app) = weak.upgrade() {
                app.toggle_complete(id);
            }
        });
    }
    line.append(&check);

    if !subtask {
        if task.subtasks.is_empty() {
            let pad = hbox(0);
            pad.set_width_request(tokens.sp(16.0) as i32);
            line.append(&pad);
        } else {
            let chev = gtk::Button::with_label(if expanded { widgets::glyph::CHEVRON_DOWN } else { widgets::glyph::CHEVRON_RIGHT });
            chev.add_css_class("chevron");
            chev.set_focusable(false);
            chev.set_valign(gtk::Align::Start);
            chev.set_tooltip_text(Some("Toggle sub-tasks (space)"));
            let weak = Rc::downgrade(app);
            chev.connect_clicked(move |_| {
                if let Some(app) = weak.upgrade() {
                    {
                        let mut st = app.st.borrow_mut();
                        if !st.expanded.remove(&id) {
                            st.expanded.insert(id);
                        }
                    }
                    app.render();
                }
            });
            line.append(&chev);
        }
    }

    let content = vbox(4);
    content.set_hexpand(true);
    let title_line = hbox(8);
    let title = wrapping(&task.what, &["title"]);
    title.set_tooltip_text(Some(&task.what));
    title.set_hexpand(true);
    title.set_valign(gtk::Align::Start);
    title_line.append(&title);
    if let Some(started) = pending {
        let ring = widgets::ring(&app.theme, started, COUNTDOWN, tokens.sp(14.0) as i32);
        ring.set_valign(gtk::Align::Start);
        ring.set_tooltip_text(Some("Completing… click the box or press x to undo"));
        app.st.borrow_mut().rings.push(ring.clone());
        title_line.append(&ring);
    }
    if !subtask && !task.completed() && pending.is_none() {
        let add = widgets::icon_button(widgets::glyph::PLUS, "Add sub-task", &["reveal"]);
        add.set_valign(gtk::Align::Start);
        let weak = Rc::downgrade(app);
        add.connect_clicked(move |_| {
            if let Some(app) = weak.upgrade() {
                editor::open_task(&app, None, None, Some(id));
            }
        });
        title_line.append(&add);
    }
    content.append(&title_line);
    line.append(&content);

    // One quiet line of facts under the title: progress, link, project.
    let mut facts: Vec<String> = vec![];
    if !task.subtasks.is_empty() {
        let snap = app.snap.borrow();
        let done = task.subtasks.iter().filter(|s| snap.task(**s).is_some_and(|t| t.completed())).count();
        facts.push(format!("{} {done}/{}", widgets::glyph::LIST, task.subtasks.len()));
    }
    if task.in_progress && !task.completed() {
        facts.push(format!("<span foreground=\"{}\">{} In progress</span>", tokens.yellow.to_hex(), widgets::glyph::HOURGLASS));
    }
    if !task.link.is_empty() {
        let host = task.link.split("://").nth(1).unwrap_or(&task.link).split('/').next().unwrap_or("").trim_start_matches("www.");
        facts.push(format!("<a href=\"{}\" title=\"{}\">{} {}</a>", esc(&task.link), esc(&task.link), widgets::glyph::LINK, esc(host)));
    }
    if opts.show_project {
        facts.push(format!("<a href=\"project:{}\" title=\"Open project\">{} {}</a>", task.project_id, widgets::glyph::FOLDER, esc(&project_name)));
    }
    if !facts.is_empty() {
        // Each fact stays on one line; only the gaps between facts may wrap.
        let facts: Vec<String> = facts.iter().map(|f| f.replace(' ', "\u{a0}").replace("<a\u{a0}", "<a ").replace("<span\u{a0}", "<span ").replace("\"\u{a0}title", "\" title")).collect();
        content.append(&link_line(app, &facts.join("    "), &["facts", "meta"]));
    }

    let notes_line = task.notes.lines().map(str::trim).filter(|l| !l.is_empty()).collect::<Vec<_>>().join("  ");
    if !notes_line.is_empty() {
        let notes = ellipsized(&notes_line, &["notes", "meta"]);
        content.append(&notes);
    }
    if !task.needs.is_empty() {
        content.append(&dependency_line(app, widgets::glyph::LINK, "Needs", &task.needs));
    }
    if !task.enables.is_empty() {
        content.append(&dependency_line(app, widgets::glyph::UNLOCK, "Enables", &task.enables));
    }
    main.append(&line);
    main.append(&who_column(app, &task, who_w));
    main.append(&when_column(app, &task, when_w));
    row.append(&main);

    if !conflicts.is_empty() {
        let text = if conflicts.len() == 1 {
            format!("{} This task is scheduled before a dependency that must complete first: {}.", "\u{f071}", conflicts[0])
        } else {
            format!("{} This task is scheduled before dependencies that must complete first: {}.", "\u{f071}", conflicts.join(", "))
        };
        content.append(&wrapping(&text, &["conflict"]));
    }
    if let Some(msg) = message {
        content.append(&wrapping(&msg, &["blocked"]));
    }

    let click = gtk::GestureClick::new();
    let weak = Rc::downgrade(app);
    click.connect_released(move |g, n, _, _| {
        let Some(app) = weak.upgrade() else { return };
        if n == 1 && g.current_button() == gdk::BUTTON_PRIMARY {
            app.select(Some(Sel::Task(id)), false);
            editor::open_task(&app, Some(id), None, None);
        }
    });
    row.add_controller(click);

    app.st.borrow_mut().rows.push((Sel::Task(id), row.clone().upcast()));
    row.upcast()
}

/// Drag a sub-task by its handle onto a sibling: drop on the upper half to
/// place it before, on the lower half to place it after.
fn install_drag(app: &Rc<App>, row: &gtk::Box, handle: &gtk::Label, id: i64) {
    let source = gtk::DragSource::new();
    source.set_actions(gdk::DragAction::MOVE);
    source.connect_prepare(move |_, _, _| Some(gdk::ContentProvider::for_value(&id.to_value())));
    let r = row.clone();
    source.connect_drag_begin(move |src, _| {
        r.add_css_class("dragging");
        let paintable = gtk::WidgetPaintable::new(Some(&r));
        src.set_icon(Some(&paintable), 12, 12);
    });
    let r = row.clone();
    source.connect_drag_end(move |_, _, _| r.remove_css_class("dragging"));
    handle.add_controller(source);

    let target = gtk::DropTarget::new(glib::Type::I64, gdk::DragAction::MOVE);
    let r = row.clone();
    target.connect_motion(move |_, _, y| {
        let upper = y < r.height() as f64 / 2.0;
        r.remove_css_class(if upper { "drop-after" } else { "drop-before" });
        r.add_css_class(if upper { "drop-before" } else { "drop-after" });
        gdk::DragAction::MOVE
    });
    let r = row.clone();
    target.connect_leave(move |_| {
        r.remove_css_class("drop-before");
        r.remove_css_class("drop-after");
    });
    let weak = Rc::downgrade(app);
    let r = row.clone();
    target.connect_drop(move |_, value, _, y| {
        r.remove_css_class("drop-before");
        r.remove_css_class("drop-after");
        let (Some(app), Ok(dragged)) = (weak.upgrade(), value.get::<i64>()) else { return false };
        let after = y >= r.height() as f64 / 2.0;
        let order = {
            let snap = app.snap.borrow();
            let parent = snap.task(id).and_then(|t| t.parent_id).and_then(|p| snap.task(p));
            parent.and_then(|p| crate::model::drop_order(&p.subtasks, dragged, id, after))
        };
        let Some(order) = order else { return false };
        app.run(app.db.reorder(&order));
        app.select(Some(Sel::Task(dragged)), false);
        true
    });
    row.add_controller(target);
}
