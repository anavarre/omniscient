//! Window, state and the interaction model: navigation, keyboard, selection,
//! modals, toasts and the 10-second countdowns.

mod dialogs;
mod editor;
mod pages;
mod picker;
mod rows;
pub mod widgets;

use std::cell::RefCell;
use std::collections::{HashMap, HashSet};
use std::rc::Rc;
use std::time::{Duration, Instant};

use gtk::{gdk, glib, prelude::*};

use crate::db::Db;
use crate::model::{Grouping, Scope, Snapshot};
use crate::theme::Theme;
use widgets::{glyph, hbox, vbox};

/// Seconds a completion or an archive waits before it is committed.
pub const COUNTDOWN: f64 = 10.0;
/// Rows built synchronously per render — comfortably more than a screenful.
const ROWS_UP_FRONT: usize = 40;
const ROWS_PER_CHUNK: usize = 40;
/// Seconds an inline error stays under its row.
const MESSAGE_SECS: f64 = 10.0;
const ARCHIVE_ERROR_SECS: f64 = 5.0;
const AUTO_DATE_DELAY: Duration = Duration::from_secs(30);

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Page {
    Tasks,
    Projects,
    Project(i64),
}

#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
pub enum Sel {
    Task(i64),
    Project(i64),
}

pub struct State {
    pub page: Page,
    pub mine: bool,
    pub grouping: Grouping,
    pub expanded: HashSet<i64>,
    pub show_completed: bool,
    pub show_archived: bool,
    /// Section keys whose capped tasks were revealed.
    pub revealed: HashSet<String>,
    pub selected: Option<Sel>,
    last_index: usize,
    /// Selectable rows in display order, rebuilt on every render.
    pub rows: Vec<(Sel, gtk::Widget)>,
    pub rings: Vec<gtk::DrawingArea>,
    pub pending: HashMap<i64, Instant>,
    pub archiving: HashMap<i64, Instant>,
    pub task_msgs: HashMap<i64, (String, Instant, f64)>,
    pub project_msgs: HashMap<i64, (String, Instant, f64)>,
    g_prefix: bool,
    /// Rows still to build for the current render (streamed in idle time).
    pub budget: usize,
    pub deferred: Vec<(gtk::Box, i64, rows::RowOpts)>,
    generation: u64,
    /// Current window width (logical px) and the user's sidebar toggle.
    width: i32,
    sidebar: Option<bool>,
}

pub struct Modal {
    layer: gtk::Widget,
    submit: Option<Rc<dyn Fn()>>,
    on_close: Option<Box<dyn Fn()>>,
    focus_back: Option<gtk::Widget>,
}

pub struct App {
    pub db: Db,
    pub snap: RefCell<Snapshot>,
    pub theme: Rc<Theme>,
    pub window: gtk::ApplicationWindow,
    overlay: gtk::Overlay,
    root: gtk::Box,
    sidebar: gtk::Box,
    scroller: gtk::ScrolledWindow,
    /// Holds the current page (and, briefly, the previous one being dismantled).
    pages: gtk::Box,
    pub st: RefCell<State>,
    modals: RefCell<Vec<Modal>>,
    toast: RefCell<Option<(gtk::Widget, glib::SourceId)>>,
    ticker: RefCell<Option<glib::SourceId>>,
    scroll_anim: RefCell<Option<gtk::TickCallbackId>>,
}

impl App {
    pub fn new(gapp: &gtk::Application, db: Db, theme: Rc<Theme>) -> Rc<App> {
        let window = gtk::ApplicationWindow::builder()
            .application(gapp)
            .title("Taskestra")
            .default_width(1180)
            .default_height(820)
            .build();
        window.add_css_class("taskestra");
        // Hyprland tiles and decorates; client-side decorations would only add
        // a second title bar.
        window.set_decorated(false);

        let sidebar = vbox(2);
        sidebar.add_css_class("sidebar");
        sidebar.set_width_request(theme.tokens.borrow().sp(230.0) as i32);

        let scroller = gtk::ScrolledWindow::new();
        scroller.set_hscrollbar_policy(gtk::PolicyType::Never);
        scroller.set_hexpand(true);
        scroller.set_vexpand(true);
        let pages_box = vbox(0);
        scroller.set_child(Some(&pages_box));

        // The sidebar keeps a fixed width: a non-propagating scrolled window
        // reports its minimum as natural, so long names ellipsize instead.
        let side_scroll = gtk::ScrolledWindow::new();
        side_scroll.set_hscrollbar_policy(gtk::PolicyType::External);
        side_scroll.set_vscrollbar_policy(gtk::PolicyType::Never);
        side_scroll.set_min_content_width(theme.tokens.borrow().sp(230.0) as i32);
        // Children that hexpand would otherwise make the sidebar share extra width.
        side_scroll.set_hexpand(false);
        side_scroll.set_child(Some(&sidebar));
        let root = hbox(0);
        root.append(&side_scroll);
        root.append(&scroller);

        let overlay = gtk::Overlay::new();
        overlay.set_child(Some(&root));
        window.set_child(Some(&overlay));

        let identity = db.setting("identity").and_then(|v| v.parse::<i64>().ok());
        let mine = db.setting("filter").as_deref() != Some("all");
        let snap = db.snapshot().unwrap_or_default();
        let identity_exists = identity.is_some_and(|id| snap.person(id).is_some());

        let app = Rc::new(App {
            db,
            snap: RefCell::new(snap),
            theme,
            window,
            overlay,
            root,
            sidebar,
            scroller,
            pages: pages_box,
            st: RefCell::new(State {
                page: Page::Tasks,
                mine: mine && identity_exists,
                grouping: Grouping::Date,
                expanded: HashSet::new(),
                show_completed: false,
                show_archived: false,
                revealed: HashSet::new(),
                selected: None,
                last_index: 0,
                rows: vec![],
                rings: vec![],
                pending: HashMap::new(),
                archiving: HashMap::new(),
                task_msgs: HashMap::new(),
                project_msgs: HashMap::new(),
                g_prefix: false,
                budget: 0,
                deferred: vec![],
                generation: 0,
                width: 1180,
                sidebar: None,
            }),
            modals: RefCell::new(vec![]),
            toast: RefCell::new(None),
            ticker: RefCell::new(None),
            scroll_anim: RefCell::new(None),
        });

        app.install_keys();
        app.install_click_outside();
        app.install_size_sensor();
        {
            let weak = Rc::downgrade(&app);
            app.theme.on_change(move || {
                if let Some(app) = weak.upgrade() {
                    let w = app.theme.tokens.borrow().sp(230.0) as i32;
                    app.sidebar.set_width_request(w);
                    if let Some(scroll) = app.sidebar.parent().and_then(|v| v.parent()).and_downcast::<gtk::ScrolledWindow>() {
                        scroll.set_min_content_width(w);
                    }
                    app.render();
                }
            });
        }
        {
            // Leaving during a countdown commits it, as the web app did on unload.
            let weak = Rc::downgrade(&app);
            app.window.connect_close_request(move |_| {
                if let Some(app) = weak.upgrade() {
                    app.flush_countdowns();
                }
                glib::Propagation::Proceed
            });
        }
        // Logout/shutdown sends SIGTERM: commit pending countdowns first.
        for signal in [libc_sigterm(), libc_sigint()] {
            let weak = Rc::downgrade(&app);
            glib_unix::unix_signal_add_local(signal, move || {
                if let Some(app) = weak.upgrade() {
                    app.flush_countdowns();
                    app.window.application().inspect(|a| a.quit());
                }
                glib::ControlFlow::Break
            });
        }
        {
            // Roll "Today" over at midnight (and catch up after suspend).
            let weak = Rc::downgrade(&app);
            glib::timeout_add_seconds_local(60, move || {
                let Some(app) = weak.upgrade() else { return glib::ControlFlow::Break };
                if app.snap.borrow().today != crate::dates::today() {
                    app.refresh();
                }
                glib::ControlFlow::Continue
            });
        }
        app.render();
        app
    }

    // ------------------------------------------------------------ responsive layout

    /// Narrow tiles (half or third of a screen) drop the sidebar by default.
    pub fn compact(&self) -> bool {
        self.st.borrow().width < self.theme.tokens.borrow().sp(1100.0) as i32
    }

    /// Very narrow tiles also shrink the Who column to avatars.
    pub fn narrow(&self) -> bool {
        self.st.borrow().width < self.theme.tokens.borrow().sp(760.0) as i32
    }

    fn sidebar_visible(&self) -> bool {
        self.st.borrow().sidebar.unwrap_or(!self.compact())
    }

    fn toggle_sidebar(self: &Rc<Self>) {
        let visible = !self.sidebar_visible();
        self.st.borrow_mut().sidebar = Some(visible);
        self.render();
    }

    /// Re-lays out when the window crosses a breakpoint (Hyprland resizes
    /// tiles freely, so this is the only reliable signal).
    fn install_size_sensor(self: &Rc<Self>) {
        let sensor = gtk::DrawingArea::new();
        sensor.set_can_target(false);
        sensor.set_hexpand(true);
        sensor.set_vexpand(true);
        let weak = Rc::downgrade(self);
        sensor.connect_resize(move |_, w, _| {
            let Some(app) = weak.upgrade() else { return };
            let before = (app.compact(), app.narrow());
            app.st.borrow_mut().width = w;
            if before != (app.compact(), app.narrow()) {
                let app = app.clone();
                glib::idle_add_local_once(move || app.render());
            }
        });
        self.overlay.add_overlay(&sensor);
    }

    // ------------------------------------------------------------ data

    pub fn identity(&self) -> Option<i64> {
        let id = self.db.setting("identity")?.parse::<i64>().ok()?;
        self.snap.borrow().person(id).map(|p| p.id)
    }

    pub fn scope(&self) -> Scope {
        let st = self.st.borrow();
        match (st.page, self.identity()) {
            (Page::Project(id), _) => Scope::Project(id),
            (_, Some(me)) if st.mine => Scope::Mine(me),
            _ => Scope::All,
        }
    }

    /// Reloads the snapshot and redraws everything.
    pub fn refresh(self: &Rc<Self>) {
        let started = Instant::now();
        let snapshot = self.db.snapshot();
        if std::env::var_os("TASKESTRA_DEBUG_TIMING").is_some() {
            eprintln!("snapshot: {:?}", started.elapsed());
        }
        match snapshot {
            Ok(s) => *self.snap.borrow_mut() = s,
            Err(e) => self.toast_error(&e),
        }
        self.render();
    }

    /// Runs a write; on success refreshes, on failure shows the message.
    pub fn run(self: &Rc<Self>, result: crate::db::Result<()>) -> bool {
        match result {
            Ok(()) => {
                self.refresh();
                true
            }
            Err(e) => {
                self.toast_error(&e);
                false
            }
        }
    }

    // ------------------------------------------------------------ rendering

    pub fn render(self: &Rc<Self>) {
        let started = Instant::now();
        let page = self.st.borrow().page;
        if let Page::Project(id) = page
            && self.snap.borrow().project(id).is_none() {
                self.st.borrow_mut().page = Page::Projects;
            }
        let scroll = self.scroller.vadjustment().value();
        {
            let mut st = self.st.borrow_mut();
            st.rows.clear();
            st.rings.clear();
            st.deferred.clear();
            st.budget = ROWS_UP_FRONT;
            st.generation += 1;
        }
        if let Some(scroll) = self.sidebar.parent().and_then(|v| v.parent()) {
            scroll.set_visible(self.sidebar_visible());
        }
        pages::sidebar(self, &self.sidebar);
        let content = pages::page(self);
        // Unparenting a big page is slow (unrealize, CSS, a11y). Hide the old
        // one now — cheap — and dismantle it in idle chunks after the new
        // page has painted.
        let old: Vec<gtk::Widget> = {
            let mut v = vec![];
            let mut c = self.pages.first_child();
            while let Some(w) = c {
                c = w.next_sibling();
                // Pages already being dismantled are not old pages again.
                if !w.has_css_class("dismantling") {
                    v.push(w);
                }
            }
            v
        };
        for w in &old {
            w.set_visible(false);
            w.add_css_class("dismantling");
        }
        self.pages.append(&content);
        for w in old {
            self.dismantle(w);
        }
        self.window.set_title(Some(&self.title()));

        let streaming = !self.st.borrow().deferred.is_empty();
        if streaming {
            self.stream_rows();
        } else {
            self.restore_selection();
        }

        let adj = self.scroller.vadjustment();
        widgets::after_layout(&self.scroller, move || adj.set_value(scroll.min(adj.upper() - adj.page_size()).max(0.0)));
        self.ensure_ticker();
        if std::env::var_os("TASKESTRA_DEBUG_TIMING").is_some() {
            eprintln!("render: {:?} ({} rows)", started.elapsed(), self.st.borrow().rows.len());
        }
    }

    fn dismantle(self: &Rc<Self>, page: gtk::Widget) {
        let mut blocks = vec![];
        let mut stack = vec![page.clone()];
        while let Some(w) = stack.pop() {
            if w.has_css_class("task-block") {
                blocks.push(w);
                continue;
            }
            let mut c = w.first_child();
            while let Some(child) = c {
                c = child.next_sibling();
                stack.push(child);
            }
        }
        let pages = self.pages.clone();
        glib::idle_add_local_full(glib::Priority::LOW, move || {
            for _ in 0..ROWS_PER_CHUNK {
                match blocks.pop() {
                    Some(b) => {
                        if let Some(parent) = b.parent().and_downcast::<gtk::Box>() {
                            parent.remove(&b);
                        }
                    }
                    None => {
                        pages.remove(&page);
                        return glib::ControlFlow::Break;
                    }
                }
            }
            glib::ControlFlow::Continue
        });
    }

    /// Keeps the selection on the same item, or its old position.
    fn restore_selection(&self) {
        let mut st = self.st.borrow_mut();
        let index = st.selected.and_then(|s| st.rows.iter().position(|(r, _)| *r == s));
        let index = match index {
            Some(i) => Some(i),
            None if st.rows.is_empty() => None,
            None => Some(st.last_index.min(st.rows.len() - 1)),
        };
        st.selected = index.map(|i| st.rows[i].0);
        if let Some(i) = index {
            st.last_index = i;
            st.rows[i].1.add_css_class("selected");
        }
    }

    /// Builds deferred rows a chunk at a time at idle priority, so frames
    /// keep flowing while a long list fills in.
    fn stream_rows(self: &Rc<Self>) {
        let generation = self.st.borrow().generation;
        let weak = Rc::downgrade(self);
        glib::idle_add_local_full(glib::Priority::DEFAULT_IDLE, move || {
            let Some(app) = weak.upgrade() else { return glib::ControlFlow::Break };
            if app.st.borrow().generation != generation {
                return glib::ControlFlow::Break;
            }
            let chunk: Vec<_> = {
                let mut st = app.st.borrow_mut();
                let n = st.deferred.len().min(ROWS_PER_CHUNK);
                st.deferred.drain(..n).collect()
            };
            for (container, id, opts) in chunk {
                container.append(&rows::task_block(&app, id, opts));
            }
            if !app.st.borrow().deferred.is_empty() {
                return glib::ControlFlow::Continue;
            }
            app.order_rows();
            app.restore_selection();
            glib::ControlFlow::Break
        });
    }

    /// Streamed rows register out of display order; re-sort by tree order.
    fn order_rows(&self) {
        let Some(root) = self.pages.last_child() else { return };
        let mut by_widget: HashMap<gtk::Widget, Sel> = self.st.borrow_mut().rows.drain(..).map(|(s, w)| (w, s)).collect();
        let mut ordered = Vec::with_capacity(by_widget.len());
        let mut stack = vec![root];
        while let Some(w) = stack.pop() {
            if let Some(sel) = by_widget.remove(&w) {
                ordered.push((sel, w.clone()));
            }
            let mut children = vec![];
            let mut c = w.first_child();
            while let Some(child) = c {
                c = child.next_sibling();
                children.push(child);
            }
            stack.extend(children.into_iter().rev());
        }
        self.st.borrow_mut().rows = ordered;
    }

    fn title(&self) -> String {
        let snap = self.snap.borrow();
        match self.st.borrow().page {
            Page::Tasks => "Tasks · Taskestra".into(),
            Page::Projects => "Projects · Taskestra".into(),
            Page::Project(id) => format!("{} · Taskestra", snap.project_name(id)),
        }
    }

    pub fn go(self: &Rc<Self>, page: Page) {
        {
            let mut st = self.st.borrow_mut();
            if st.page != page {
                st.page = page;
                st.selected = None;
                st.last_index = 0;
                st.show_completed = false;
                st.revealed.clear();
                if !matches!(page, Page::Tasks) && st.grouping == Grouping::Project {
                    st.grouping = Grouping::Date;
                }
            }
        }
        self.render();
        self.scroller.vadjustment().set_value(0.0);
    }

    pub fn set_mine(self: &Rc<Self>, mine: bool) {
        if mine && self.identity().is_none() {
            return;
        }
        let _ = self.db.set_setting("filter", Some(if mine { "mine" } else { "all" }));
        {
            let mut st = self.st.borrow_mut();
            st.mine = mine;
            st.selected = None;
            st.last_index = 0;
        }
        self.go(Page::Tasks);
    }

    pub fn set_grouping(self: &Rc<Self>, g: Grouping) {
        let page = self.st.borrow().page;
        if g == Grouping::Project && !matches!(page, Page::Tasks) {
            return;
        }
        if matches!(page, Page::Projects) {
            return;
        }
        self.st.borrow_mut().grouping = g;
        self.render();
    }

    // ------------------------------------------------------------ selection

    pub fn select(self: &Rc<Self>, sel: Option<Sel>, scroll: bool) {
        let mut st = self.st.borrow_mut();
        for (_, w) in &st.rows {
            w.remove_css_class("selected");
        }
        st.selected = sel;
        let found = sel.and_then(|s| st.rows.iter().position(|(r, _)| *r == s));
        if let Some(i) = found {
            st.last_index = i;
            let w = st.rows[i].1.clone();
            w.add_css_class("selected");
            drop(st);
            if scroll {
                self.scroll_into_view(&w);
            }
        }
    }

    fn move_selection(self: &Rc<Self>, delta: i32) {
        let (next, len) = {
            let st = self.st.borrow();
            let len = st.rows.len();
            if len == 0 {
                return;
            }
            let current = st.selected.and_then(|s| st.rows.iter().position(|(r, _)| *r == s));
            let next = match current {
                None => 0,
                Some(i) => (i as i32 + delta).clamp(0, len as i32 - 1) as usize,
            };
            (st.rows[next].0, len)
        };
        let _ = len;
        self.select(Some(next), true);
    }

    fn scroll_into_view(self: &Rc<Self>, w: &gtk::Widget) {
        let Some(bounds) = w.compute_bounds(&self.pages) else { return };
        let adj = self.scroller.vadjustment();
        let margin = self.theme.tokens.borrow().sp(80.0) as f64;
        let (top, bottom) = (bounds.y() as f64, (bounds.y() + bounds.height()) as f64);
        let (value, page) = (adj.value(), adj.page_size());
        let target = if top - margin < value {
            (top - margin).max(0.0)
        } else if bottom + margin > value + page {
            (bottom + margin - page).min(adj.upper() - page)
        } else {
            return;
        };
        self.smooth_scroll(target);
    }

    fn smooth_scroll(self: &Rc<Self>, target: f64) {
        if let Some(id) = self.scroll_anim.borrow_mut().take() {
            id.remove();
        }
        let adj = self.scroller.vadjustment();
        let from = adj.value();
        let start = Instant::now();
        let weak = Rc::downgrade(self);
        let id = self.scroller.add_tick_callback(move |_, _| {
            let t = (start.elapsed().as_secs_f64() / 0.14).min(1.0);
            let eased = 1.0 - (1.0 - t).powi(3);
            adj.set_value(from + (target - from) * eased);
            if t >= 1.0 {
                if let Some(app) = weak.upgrade() {
                    app.scroll_anim.borrow_mut().take();
                }
                glib::ControlFlow::Break
            } else {
                glib::ControlFlow::Continue
            }
        });
        *self.scroll_anim.borrow_mut() = Some(id);
    }

    fn activate_selected(self: &Rc<Self>) {
        let selected = self.st.borrow().selected;
        match selected {
            Some(Sel::Task(id)) => editor::open_task(self, Some(id), None, None),
            Some(Sel::Project(id)) => self.go(Page::Project(id)),
            None => {}
        }
    }

    fn open_selected_link(self: &Rc<Self>) {
        let snap = self.snap.borrow();
        let link = match self.st.borrow().selected {
            Some(Sel::Task(id)) => snap.task(id).map(|t| t.link.clone()),
            Some(Sel::Project(id)) => snap.project(id).map(|p| p.link.clone()),
            None => None,
        };
        drop(snap);
        if let Some(link) = link {
            widgets::open_uri(&self.window, &link);
        }
    }

    fn toggle_selected_expanded(self: &Rc<Self>, open: Option<bool>) {
        let Some(Sel::Task(id)) = self.st.borrow().selected else { return };
        let has_subtasks = self.snap.borrow().task(id).is_some_and(|t| !t.subtasks.is_empty());
        if !has_subtasks {
            return;
        }
        let mut st = self.st.borrow_mut();
        let now_open = st.expanded.contains(&id);
        let want = open.unwrap_or(!now_open);
        if want == now_open {
            return;
        }
        if want { st.expanded.insert(id) } else { st.expanded.remove(&id) };
        drop(st);
        self.render();
    }

    /// Shift+J / Shift+K: move the selected sub-task among its siblings.
    fn move_subtask(self: &Rc<Self>, delta: i32) {
        let Some(Sel::Task(id)) = self.st.borrow().selected else { return };
        let siblings = {
            let snap = self.snap.borrow();
            let Some(parent) = snap.task(id).and_then(|t| t.parent_id) else { return };
            snap.task(parent).map(|p| p.subtasks.clone()).unwrap_or_default()
        };
        let Some(i) = siblings.iter().position(|s| *s == id) else { return };
        let j = i as i32 + delta;
        if j < 0 || j >= siblings.len() as i32 {
            return;
        }
        let mut order = siblings;
        order.swap(i, j as usize);
        if self.run(self.db.reorder(&order)) {
            self.select(Some(Sel::Task(id)), true);
        }
    }

    // ------------------------------------------------------------ completion countdown

    /// Checkbox / `x`: start, cancel, or (for completed tasks) reopen.
    pub fn toggle_complete(self: &Rc<Self>, id: i64) {
        let (completed, blocked) = {
            let snap = self.snap.borrow();
            let Some(t) = snap.task(id) else { return };
            (t.completed(), snap.blocked_reason(t))
        };
        if completed {
            self.run(self.db.uncomplete_task(id));
            return;
        }
        let mut st = self.st.borrow_mut();
        if st.pending.remove(&id).is_none() {
            match blocked {
                Some(reason) => {
                    st.task_msgs.insert(id, (reason.into(), Instant::now(), MESSAGE_SECS));
                }
                None => {
                    st.task_msgs.remove(&id);
                    st.pending.insert(id, Instant::now());
                }
            }
        }
        drop(st);
        self.render();
    }

    fn commit_completion(self: &Rc<Self>, id: i64) {
        self.st.borrow_mut().pending.remove(&id);
        match self.db.complete_task(id) {
            Ok(()) => {
                let weak = Rc::downgrade(self);
                glib::timeout_add_local_once(AUTO_DATE_DELAY, move || {
                    let Some(app) = weak.upgrade() else { return };
                    if app.db.auto_date_unblocked_dependents(id).unwrap_or(0) > 0 && app.modals.borrow().is_empty() {
                        app.refresh();
                    }
                });
            }
            Err(e) => {
                self.st.borrow_mut().task_msgs.insert(id, (e, Instant::now(), MESSAGE_SECS));
            }
        }
        self.refresh();
    }

    pub fn toggle_archive(self: &Rc<Self>, id: i64) {
        let open = self.snap.borrow().project(id).map(|p| p.open_tasks).unwrap_or(0);
        let mut st = self.st.borrow_mut();
        if st.archiving.remove(&id).is_none() {
            if open > 0 {
                st.project_msgs.insert(
                    id,
                    ("All tasks should be completed before a project can be archived.".into(), Instant::now(), ARCHIVE_ERROR_SECS),
                );
            } else {
                st.archiving.insert(id, Instant::now());
            }
        }
        drop(st);
        self.render();
    }

    fn commit_archive(self: &Rc<Self>, id: i64) {
        self.st.borrow_mut().archiving.remove(&id);
        if let Err(e) = self.db.archive_project(id) {
            self.st.borrow_mut().project_msgs.insert(id, (e, Instant::now(), ARCHIVE_ERROR_SECS));
        } else {
            let name = self.snap.borrow().project_name(id).to_string();
            self.toast(&format!("\"{name}\" has been archived."));
        }
        self.refresh();
    }

    fn flush_countdowns(self: &Rc<Self>) {
        let (tasks, projects): (Vec<i64>, Vec<i64>) = {
            let st = self.st.borrow();
            (st.pending.keys().copied().collect(), st.archiving.keys().copied().collect())
        };
        for id in tasks {
            let _ = self.db.complete_task(id);
        }
        for id in projects {
            let _ = self.db.archive_project(id);
        }
    }

    /// One 30 fps ticker drives every ring and expires countdowns/messages.
    fn ensure_ticker(self: &Rc<Self>) {
        let busy = {
            let st = self.st.borrow();
            !(st.pending.is_empty() && st.archiving.is_empty() && st.task_msgs.is_empty() && st.project_msgs.is_empty())
        };
        if !busy || self.ticker.borrow().is_some() {
            return;
        }
        let weak = Rc::downgrade(self);
        let id = glib::timeout_add_local(Duration::from_millis(33), move || {
            let Some(app) = weak.upgrade() else { return glib::ControlFlow::Break };
            let mut done_tasks = vec![];
            let mut done_projects = vec![];
            let expired;
            {
                let mut st = app.st.borrow_mut();
                for ring in &st.rings {
                    ring.queue_draw();
                }
                done_tasks.extend(st.pending.iter().filter(|(_, t)| t.elapsed().as_secs_f64() >= COUNTDOWN).map(|(id, _)| *id));
                done_projects.extend(st.archiving.iter().filter(|(_, t)| t.elapsed().as_secs_f64() >= COUNTDOWN).map(|(id, _)| *id));
                let before = st.task_msgs.len() + st.project_msgs.len();
                st.task_msgs.retain(|_, (_, t, secs)| t.elapsed().as_secs_f64() < *secs);
                st.project_msgs.retain(|_, (_, t, secs)| t.elapsed().as_secs_f64() < *secs);
                expired = before != st.task_msgs.len() + st.project_msgs.len();
            }
            for id in &done_tasks {
                app.commit_completion(*id);
            }
            for id in &done_projects {
                app.commit_archive(*id);
            }
            if expired && done_tasks.is_empty() && done_projects.is_empty() {
                app.render();
            }
            let st = app.st.borrow();
            if st.pending.is_empty() && st.archiving.is_empty() && st.task_msgs.is_empty() && st.project_msgs.is_empty() {
                app.ticker.borrow_mut().take();
                glib::ControlFlow::Break
            } else {
                glib::ControlFlow::Continue
            }
        });
        *self.ticker.borrow_mut() = Some(id);
    }

    // ------------------------------------------------------------ modals

    pub fn modal_open(&self) -> bool {
        !self.modals.borrow().is_empty()
    }

    /// Shows `card` centered over a scrim. `submit` runs on Shift+Enter.
    pub fn push_modal(self: &Rc<Self>, card: &gtk::Widget, top: bool, submit: Option<Rc<dyn Fn()>>, on_close: Option<Box<dyn Fn()>>) {
        let scrim = vbox(0);
        scrim.add_css_class("scrim");
        scrim.set_hexpand(true);
        scrim.set_vexpand(true);
        card.add_css_class("modal");
        card.set_halign(gtk::Align::Center);
        if top {
            card.set_valign(gtk::Align::Start);
            card.set_margin_top((self.window.height() as f64 * 0.14) as i32);
        } else {
            card.set_valign(gtk::Align::Center);
            card.set_margin_top(24);
            card.set_margin_bottom(24);
        }
        card.set_margin_start(24);
        card.set_margin_end(24);
        scrim.append(card);

        let click = gtk::GestureClick::new();
        let weak = Rc::downgrade(self);
        let scrim_w = scrim.clone();
        click.connect_released(move |g, _, x, y| {
            if scrim_w.pick(x, y, gtk::PickFlags::DEFAULT).as_ref() == Some(scrim_w.upcast_ref()) {
                g.set_state(gtk::EventSequenceState::Claimed);
                if let Some(app) = weak.upgrade() {
                    app.pop_modal();
                }
            }
        });
        scrim.add_controller(click);

        let focus_back = gtk::prelude::GtkWindowExt::focus(&self.window);
        self.overlay.add_overlay(&scrim);
        self.root.add_css_class("blocked");
        self.root.set_sensitive(false);
        for m in self.modals.borrow().iter() {
            m.layer.set_sensitive(false);
        }
        self.modals.borrow_mut().push(Modal { layer: scrim.upcast(), submit, on_close, focus_back });
    }

    pub fn pop_modal(self: &Rc<Self>) {
        let Some(m) = self.modals.borrow_mut().pop() else { return };
        self.overlay.remove_overlay(&m.layer);
        if let Some(f) = &m.on_close {
            f();
        }
        // Copy out before touching widgets: sensitivity changes move focus,
        // and focus handlers may call back into the app.
        let top = self.modals.borrow().last().map(|m| m.layer.clone());
        if let Some(layer) = top {
            layer.set_sensitive(true);
        } else {
            self.root.set_sensitive(true);
            self.root.remove_css_class("blocked");
        }
        if let Some(w) = m.focus_back.filter(|w| w.is_sensitive() && w.root().is_some()) {
            w.grab_focus();
        } else if !self.modal_open() {
            gtk::prelude::GtkWindowExt::set_focus(&self.window, None::<&gtk::Widget>);
        }
    }

    pub fn close_all_modals(self: &Rc<Self>) {
        while self.modal_open() {
            self.pop_modal();
        }
    }

    fn submit_top_modal(&self) -> bool {
        let submit = self.modals.borrow().last().and_then(|m| m.submit.clone());
        match submit {
            Some(f) => {
                f();
                true
            }
            None => false,
        }
    }

    // ------------------------------------------------------------ toasts

    pub fn toast(self: &Rc<Self>, text: &str) {
        self.show_toast(text, false);
    }

    pub fn toast_error(self: &Rc<Self>, text: &str) {
        self.show_toast(text, true);
    }

    fn show_toast(self: &Rc<Self>, text: &str, error: bool) {
        if let Some((w, id)) = self.toast.borrow_mut().take() {
            id.remove();
            self.overlay.remove_overlay(&w);
        }
        let row = hbox(8);
        row.add_css_class("toast");
        if error {
            row.add_css_class("error");
        }
        row.append(&widgets::glyph(if error { "\u{f071}" } else { glyph::CHECK }, &[]));
        row.append(&widgets::wrapping(text, &[]));
        row.set_halign(gtk::Align::Center);
        row.set_valign(gtk::Align::End);
        row.set_can_target(false);
        self.overlay.add_overlay(&row);
        let weak = Rc::downgrade(self);
        let w: gtk::Widget = row.upcast();
        let id = glib::timeout_add_local_once(Duration::from_millis(if error { 5000 } else { 3000 }), move || {
            if let Some(app) = weak.upgrade()
                && let Some((w, _)) = app.toast.borrow_mut().take() {
                    app.overlay.remove_overlay(&w);
                }
        });
        *self.toast.borrow_mut() = Some((w, id));
    }

    // ------------------------------------------------------------ input

    /// Clicking empty space outside the lists clears the selection.
    fn install_click_outside(self: &Rc<Self>) {
        let click = gtk::GestureClick::new();
        click.set_propagation_phase(gtk::PropagationPhase::Capture);
        let weak = Rc::downgrade(self);
        click.connect_pressed(move |_, _, x, y| {
            let Some(app) = weak.upgrade() else { return };
            if app.modal_open() {
                return;
            }
            let Some(target) = app.window.pick(x, y, gtk::PickFlags::DEFAULT) else { return };
            let inside_row = app.st.borrow().rows.iter().any(|(_, w)| target == *w || target.is_ancestor(w));
            if !inside_row {
                app.select(None, false);
            }
        });
        self.window.add_controller(click);
    }

    fn install_keys(self: &Rc<Self>) {
        // Capture: shortcuts must win over whatever button has focus.
        let capture = gtk::EventControllerKey::new();
        capture.set_propagation_phase(gtk::PropagationPhase::Capture);
        let weak = Rc::downgrade(self);
        capture.connect_key_pressed(move |_, key, _, state| {
            let Some(app) = weak.upgrade() else { return glib::Propagation::Proceed };
            app.on_key(key, state)
        });
        self.window.add_controller(capture);

        // Bubble: Escape reaches us only if the focused widget didn't use it
        // (open pickers, inline edits).
        let bubble = gtk::EventControllerKey::new();
        let weak = Rc::downgrade(self);
        bubble.connect_key_pressed(move |_, key, _, _| {
            let Some(app) = weak.upgrade() else { return glib::Propagation::Proceed };
            if key != gdk::Key::Escape {
                return glib::Propagation::Proceed;
            }
            if app.modal_open() {
                app.pop_modal();
            } else if matches!(app.st.borrow().page, Page::Project(_)) {
                app.go(Page::Projects);
            } else {
                gtk::prelude::GtkWindowExt::set_focus(&app.window, None::<&gtk::Widget>);
            }
            glib::Propagation::Stop
        });
        self.window.add_controller(bubble);
    }

    fn editing(&self) -> bool {
        gtk::prelude::GtkWindowExt::focus(&self.window).is_some_and(|w| w.is::<gtk::Text>() || w.is::<gtk::TextView>())
    }

    fn on_key(self: &Rc<Self>, key: gdk::Key, state: gdk::ModifierType) -> glib::Propagation {
        use gdk::Key;
        let stop = glib::Propagation::Stop;
        let go_on = glib::Propagation::Proceed;
        let ctrl = state.contains(gdk::ModifierType::CONTROL_MASK);
        let shift = state.contains(gdk::ModifierType::SHIFT_MASK);
        if state.intersects(gdk::ModifierType::ALT_MASK | gdk::ModifierType::SUPER_MASK) {
            return go_on;
        }

        if self.modal_open() {
            if shift && matches!(key, Key::Return | Key::KP_Enter) && self.submit_top_modal() {
                return stop;
            }
            return go_on;
        }
        if ctrl {
            return match key {
                Key::k | Key::f => {
                    dialogs::search(self);
                    stop
                }
                Key::n => {
                    self.new_task();
                    stop
                }
                _ => go_on,
            };
        }
        if self.editing() {
            return go_on;
        }

        if std::mem::take(&mut self.st.borrow_mut().g_prefix) {
            match key {
                Key::g => self.smooth_scroll(0.0),
                Key::d => self.set_grouping(Grouping::Date),
                Key::p => self.set_grouping(Grouping::Project),
                Key::c => self.set_grouping(Grouping::Person),
                Key::m => self.set_mine(true),
                Key::a => self.set_mine(false),
                _ => {}
            }
            return stop;
        }

        match key {
            Key::g => self.st.borrow_mut().g_prefix = true,
            Key::b => self.toggle_sidebar(),
            Key::G => {
                let adj = self.scroller.vadjustment();
                self.smooth_scroll(adj.upper() - adj.page_size());
            }
            Key::h => self.go(Page::Tasks),
            Key::p => self.go(Page::Projects),
            Key::P => {
                self.go(Page::Projects);
                editor::open_project(self, None);
            }
            Key::C => dialogs::settings(self, true),
            Key::s => dialogs::settings(self, false),
            Key::f | Key::slash => dialogs::search(self),
            Key::t | Key::n => self.new_task(),
            Key::e => self.edit_context_project(),
            Key::question => dialogs::shortcuts(self),
            Key::j | Key::Down => self.move_selection(1),
            Key::k | Key::Up => self.move_selection(-1),
            Key::J => self.move_subtask(1),
            Key::K => self.move_subtask(-1),
            Key::Right | Key::l => self.toggle_selected_expanded(Some(true)),
            Key::Left => self.toggle_selected_expanded(Some(false)),
            Key::Return | Key::KP_Enter => self.activate_selected(),
            Key::space => self.toggle_selected_expanded(None),
            Key::x => {
                let selected = self.st.borrow().selected;
                if let Some(Sel::Task(id)) = selected {
                    self.toggle_complete(id);
                }
            }
            Key::o => self.open_selected_link(),
            Key::a => {
                let (page, selected) = {
                    let st = self.st.borrow();
                    (st.page, st.selected)
                };
                if let (Page::Projects, Some(Sel::Project(id))) = (page, selected) {
                    self.toggle_archive(id);
                }
            }
            Key::Home => self.smooth_scroll(0.0),
            Key::End => {
                let adj = self.scroller.vadjustment();
                self.smooth_scroll(adj.upper() - adj.page_size());
            }
            _ => return go_on,
        }
        stop
    }

    pub fn new_task(self: &Rc<Self>) {
        let page = self.st.borrow().page;
        match page {
            Page::Project(id) => editor::open_task(self, None, Some(id), None),
            Page::Tasks => editor::open_task(self, None, None, None),
            Page::Projects => {
                let app = self.clone();
                glib::idle_add_local_once(move || {
                    app.go(Page::Tasks);
                    editor::open_task(&app, None, None, None);
                });
            }
        }
    }

    fn edit_context_project(self: &Rc<Self>) {
        let st = self.st.borrow();
        let target = match (st.page, st.selected) {
            (Page::Project(id), _) => Some(id),
            (Page::Projects, Some(Sel::Project(id))) => Some(id),
            _ => None,
        };
        drop(st);
        if let Some(id) = target {
            editor::open_project(self, Some(id));
        }
    }
}

const fn libc_sigterm() -> i32 {
    15
}

const fn libc_sigint() -> i32 {
    2
}

pub fn build(gapp: &gtk::Application, db: Db) {
    let display = gdk::Display::default().expect("no display");
    let theme = Theme::install(&display);
    let app = App::new(gapp, db, theme);
    app.window.present();
    // Dev aid for screenshots/tests: replay key names ("t", "question",
    // "S-Return", "Escape"…), 250 ms apart.
    if let Ok(keys) = std::env::var("TASKESTRA_DEBUG_KEYS") {
        let app = app.clone();
        let keys: Vec<String> = keys.split_whitespace().map(String::from).collect();
        glib::timeout_add_local_once(Duration::from_millis(400), move || replay(app, keys, 0));
    }
    // Keep the App alive as long as the window.
    let keep = RefCell::new(Some(app.clone()));
    app.window.connect_destroy(move |_| {
        keep.borrow_mut().take();
    });
}

fn replay(app: Rc<App>, keys: Vec<String>, i: usize) {
    let Some(name) = keys.get(i) else { return };
    if let Some(n) = name.strip_prefix("focus:").and_then(|n| n.parse::<usize>().ok()) {
        // Focus the Nth text entry of the top modal (DFS order).
        fn entries(w: &gtk::Widget, out: &mut Vec<gtk::Entry>) {
            if let Some(e) = w.downcast_ref::<gtk::Entry>() {
                out.push(e.clone());
                return;
            }
            let mut c = w.first_child();
            while let Some(child) = c {
                entries(&child, out);
                c = child.next_sibling();
            }
        }
        let layer = app.modals.borrow().last().map(|m| m.layer.clone());
        if let Some(layer) = layer {
            let mut found = vec![];
            entries(&layer, &mut found);
            if let Some(e) = found.get(n) {
                e.grab_focus();
            }
        }
    } else if let Some(text) = name.strip_prefix("type:") {
        if let Some(w) = gtk::prelude::GtkWindowExt::focus(&app.window)
            && let Some(e) = w.ancestor(gtk::Entry::static_type()).and_downcast::<gtk::Entry>() {
                e.set_text(&text.replace('_', " "));
                e.set_position(-1);
            }
    } else {
        let (shift, key) = match name.strip_prefix("S-") {
            Some(k) => (true, k),
            None => (false, name.as_str()),
        };
        if let Some(key) = gdk::Key::from_name(key) {
            let state = if shift { gdk::ModifierType::SHIFT_MASK } else { gdk::ModifierType::empty() };
            if app.on_key(key, state) == glib::Propagation::Proceed {
                // Approximate what the focused widget would do with the key.
                let focus = gtk::prelude::GtkWindowExt::focus(&app.window);
                match key {
                    gdk::Key::Escape => app.pop_modal(),
                    gdk::Key::Tab => {
                        app.window.child_focus(gtk::DirectionType::TabForward);
                    }
                    gdk::Key::Return => {
                        if let Some(e) = focus.and_then(|w| w.ancestor(gtk::Entry::static_type())) {
                            e.activate();
                        }
                    }
                    _ => {}
                }
            }
        }
    }
    glib::timeout_add_local_once(Duration::from_millis(250), move || replay(app, keys, i + 1));
}

pub fn heading_box(title: &str, subtitle: &str) -> gtk::Box {
    let b = vbox(2);
    b.append(&widgets::wrapping(title, &["page-title"]));
    if !subtitle.is_empty() {
        b.append(&widgets::wrapping(subtitle, &["page-subtitle"]));
    }
    b
}
