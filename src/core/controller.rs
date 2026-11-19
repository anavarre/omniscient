//! The app's brain, independent of Qt.
//!
//! QML talks to it through two calls: `act(name, args)` for anything that
//! changes state and `query(name, args)` for read-only lookups (editor
//! data, pickers, search). Both take and return JSON. After every `act`
//! the bridge asks for `rows()` (the visible list) and `state()` (page
//! chrome, sidebar, toasts) and pushes only what changed to QML.
//!
//! Completion and archiving keep the web app's 10-second undo window:
//! the row is struck through with a countdown ring, and the write happens
//! when the window closes (`tick`) or the app quits (`flush`).

use std::collections::{HashMap, HashSet};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use serde_json::{Value, json};

use crate::core::dates;
use crate::core::db::{self, Created, Db, ProjectInput, TaskInput};
use crate::core::location::{self, Location};
use crate::core::model::{self, Grouping, Scope, Section, Snapshot, Task};
use crate::core::rows::Row;

pub const UNDO_WINDOW: Duration = Duration::from_secs(10);
/// The web app dated unblocked dependents from a job 30 s after completion.
pub const AUTO_DATE_DELAY: Duration = Duration::from_secs(30);

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Page {
    Tasks,
    Projects,
    Project(i64),
}

pub type Outcome = Result<Value, String>;

pub struct Controller {
    db: Db,
    snap: Snapshot,
    page: Page,
    /// Tasks page grouping; the project page keeps its own (Date / Person).
    grouping: Grouping,
    project_grouping: Grouping,
    /// Mine/All inside a project; per session, independent of the global filter.
    project_mine: bool,
    identity: Option<i64>,
    mine: bool,
    /// Parents whose sub-tasks are showing.
    expanded: HashSet<i64>,
    /// Capped sections the user asked to see in full.
    revealed: HashSet<String>,
    show_completed: bool,
    show_archived: bool,
    /// Task id → when its completion commits (ms since epoch).
    completing: HashMap<i64, u64>,
    /// Project id → when its archive commits.
    archiving: HashMap<i64, u64>,
    /// Completed task id → when to date its unblocked dependents.
    auto_dating: HashMap<i64, u64>,
    toast: Option<(u64, String, bool)>,
    toast_seq: u64,
    /// Where the database lives and who decided; None disables moving it.
    location: Option<Location>,
    /// Overridable clock (tests).
    now: fn() -> u64,
}

pub fn wall_ms() -> u64 {
    SystemTime::now().duration_since(UNIX_EPOCH).map(|d| d.as_millis() as u64).unwrap_or(0)
}

fn arg_i64(args: &Value, key: &str) -> Option<i64> {
    args.get(key).and_then(Value::as_i64)
}

fn arg_str<'a>(args: &'a Value, key: &str) -> &'a str {
    args.get(key).and_then(Value::as_str).unwrap_or("")
}

fn arg_ids(args: &Value, key: &str) -> Vec<i64> {
    args.get(key).and_then(Value::as_array).map(|a| a.iter().filter_map(Value::as_i64).collect()).unwrap_or_default()
}

fn grouping_name(g: Grouping) -> &'static str {
    match g {
        Grouping::Date => "date",
        Grouping::Project => "project",
        Grouping::Person => "person",
    }
}

fn parse_grouping(s: &str) -> Option<Grouping> {
    match s {
        "date" => Some(Grouping::Date),
        "project" => Some(Grouping::Project),
        "person" => Some(Grouping::Person),
        _ => None,
    }
}

/// First non-empty line of Markdown notes, with the markup stripped.
pub fn notes_line(notes: &str) -> String {
    let line = notes.lines().map(str::trim).find(|l| !l.is_empty() && !l.starts_with("```")).unwrap_or("");
    let line = line.trim_start_matches(['#', '>', ' ']);
    let line = line.strip_prefix("- ").or_else(|| line.strip_prefix("* ")).unwrap_or(line);
    let mut out = String::with_capacity(line.len());
    let mut chars = line.chars().peekable();
    while let Some(c) = chars.next() {
        match c {
            '*' | '_' | '`' | '~' => {}
            // [text](url) → text
            ']' if chars.peek() == Some(&'(') => {
                for c in chars.by_ref() {
                    if c == ')' {
                        break;
                    }
                }
            }
            '[' => {}
            c => out.push(c),
        }
    }
    out
}

fn initials(name: &str) -> String {
    let mut words = name.split_whitespace();
    let first = words.next().and_then(|w| w.chars().next());
    let last = words.last().and_then(|w| w.chars().next());
    first.into_iter().chain(last).flat_map(char::to_uppercase).collect()
}

impl Controller {
    pub fn new(db: Db) -> Result<Controller, String> {
        let snap = db.snapshot()?;
        let identity = db.setting("identity").and_then(|v| v.parse().ok()).filter(|id| snap.person(*id).is_some());
        let mine = identity.is_some() && db.setting("filter").as_deref() != Some("all");
        let grouping = db.setting("grouping").as_deref().and_then(parse_grouping).unwrap_or(Grouping::Date);
        Ok(Controller {
            db,
            snap,
            page: Page::Tasks,
            grouping,
            project_grouping: Grouping::Date,
            project_mine: false,
            identity,
            mine,
            expanded: HashSet::new(),
            revealed: HashSet::new(),
            show_completed: false,
            show_archived: false,
            completing: HashMap::new(),
            archiving: HashMap::new(),
            auto_dating: HashMap::new(),
            toast: None,
            toast_seq: 0,
            location: None,
            now: wall_ms,
        })
    }

    #[cfg(test)]
    fn with_clock(mut self, now: fn() -> u64) -> Self {
        self.now = now;
        self
    }

    pub fn set_location(&mut self, location: Location) {
        self.location = Some(location);
    }

    /// Moves the database to a new file and makes that the saved location.
    ///
    /// Ordered so that every failure leaves the app exactly as it was: pending
    /// countdowns are committed, the data is copied and verified at the
    /// destination, the config is rewritten, and only then does the app
    /// switch over. The old file is left in place for the user to delete.
    fn move_db(&mut self, input: &str) -> Outcome {
        let loc = self.location.clone().ok_or("Moving the database isn't available here.")?;
        if !loc.source.movable() {
            return Err(format!(
                "This session was started with {}, which overrides the saved location. Restart without it to move the database.",
                if loc.source == location::Source::Flag { "--db" } else { "OMNISCIENT_DB" }
            ));
        }
        let dest = location::parse_destination(input)?;
        let same = |a: &std::path::Path, b: &std::path::Path| match (a.canonicalize(), b.parent().and_then(|p| p.canonicalize().ok())) {
            (Ok(a), Some(p)) => b.file_name().is_some_and(|n| a == p.join(n)),
            _ => a == b,
        };
        if same(&loc.path, &dest) {
            return Err("The database is already stored there.".into());
        }

        self.flush();
        let new_db = self.db.copy_to(&dest)?;
        if let Err(e) = location::write_config(&loc.config_file, &dest) {
            drop(new_db);
            db::remove_files(&dest);
            return Err(e);
        }
        if !location::hold_instance(&dest) {
            // Lost a race with another window on that very file: undo.
            drop(new_db);
            let _ = location::write_config(&loc.config_file, &loc.path);
            db::remove_files(&dest);
            return Err("Another Omniscient window is using that location.".into());
        }
        self.db = new_db;
        self.location = Some(Location { path: dest.clone(), source: location::Source::Config, ..loc.clone() });
        self.reload()?;
        self.say("Database moved.", false);
        Ok(json!({ "path": dest, "old": loc.path }))
    }

    pub fn snapshot(&self) -> &Snapshot {
        &self.snap
    }

    fn reload(&mut self) -> Result<(), String> {
        self.snap = self.db.snapshot()?;
        if self.identity.is_some_and(|id| self.snap.person(id).is_none()) {
            self.identity = None;
            self.mine = false;
            self.project_mine = false;
        }
        if let Page::Project(id) = self.page
            && self.snap.project(id).is_none()
        {
            self.page = Page::Projects;
        }
        // Countdowns for rows that vanished (deleted elsewhere, a parent
        // taking its sub-tasks with it) must not fire later.
        self.completing.retain(|id, _| self.snap.task(*id).is_some_and(|t| !t.completed()));
        self.archiving.retain(|id, _| self.snap.project(*id).is_some_and(|p| !p.archived()));
        self.expanded.retain(|id| self.snap.task(*id).is_some());
        Ok(())
    }

    fn say(&mut self, text: impl Into<String>, error: bool) {
        self.toast_seq += 1;
        self.toast = Some((self.toast_seq, text.into(), error));
    }

    fn scope(&self) -> Scope {
        match (self.page, self.identity) {
            (Page::Project(id), me) => Scope::Project(id, me.filter(|_| self.project_mine)),
            (_, Some(me)) if self.mine => Scope::Mine(me),
            _ => Scope::All,
        }
    }

    fn task_project(&self, id: i64) -> Option<i64> {
        self.snap.task(id).map(|t| t.project_id)
    }

    // ------------------------------------------------------------ actions

    pub fn act(&mut self, name: &str, args: &Value) -> Outcome {
        let before = self.db.data_changes();
        let out = self.dispatch(name, args);
        // Anything that wrote re-reads the whole database: one source of
        // truth, every view consistent. UI-only actions (navigation, starting
        // a countdown) skip it.
        if (name == "refresh" || self.db.data_changes() != before)
            && let Err(e) = self.reload()
        {
            self.say(e, true);
        }
        out
    }

    fn dispatch(&mut self, name: &str, args: &Value) -> Outcome {
        let id = arg_i64(args, "id");
        let need_id = || id.ok_or_else(|| format!("{name}: missing id"));
        match name {
            "go" => {
                self.page = match arg_str(args, "page") {
                    "projects" => Page::Projects,
                    "project" => Page::Project(need_id()?),
                    _ => Page::Tasks,
                };
                self.revealed.clear();
                Ok(json!({}))
            }
            "group" => {
                let g = parse_grouping(arg_str(args, "by")).ok_or("Unknown grouping")?;
                match self.page {
                    Page::Project(_) if g == Grouping::Project => {}
                    Page::Project(_) => self.project_grouping = g,
                    _ => {
                        self.grouping = g;
                        self.db.set_setting("grouping", Some(grouping_name(g)))?;
                    }
                }
                Ok(json!({}))
            }
            "filter" => {
                let mine = args.get("mine").and_then(Value::as_bool).unwrap_or(false);
                if mine && self.identity.is_none() {
                    return Err("Choose who you are in Collaborators (c) to see your tasks.".into());
                }
                if matches!(self.page, Page::Project(_)) {
                    self.project_mine = mine;
                    return Ok(json!({}));
                }
                self.mine = mine;
                self.db.set_setting("filter", Some(if mine { "mine" } else { "all" }))?;
                Ok(json!({}))
            }
            "identity" => {
                self.identity = id.filter(|id| self.snap.person(*id).is_some());
                self.mine = self.identity.is_some();
                self.db.set_setting("identity", self.identity.map(|i| i.to_string()).as_deref())?;
                self.db.set_setting("filter", Some(if self.mine { "mine" } else { "all" }))?;
                Ok(json!({}))
            }
            "zoom" => {
                let z = args.get("value").and_then(Value::as_f64).ok_or("missing value")?;
                let z = (z.clamp(0.8, 2.0) * 100.0).round() / 100.0;
                self.db.set_setting("zoom", Some(&z.to_string()))?;
                Ok(json!({ "value": z }))
            }
            "expand" => {
                let id = need_id()?;
                let open = args.get("open").and_then(Value::as_bool).unwrap_or(!self.expanded.contains(&id));
                if open { self.expanded.insert(id) } else { self.expanded.remove(&id) };
                Ok(json!({ "open": open }))
            }
            "reveal" => {
                self.revealed.insert(arg_str(args, "section").to_string());
                Ok(json!({}))
            }
            "toggleCompleted" => {
                self.show_completed = !self.show_completed;
                Ok(json!({ "open": self.show_completed }))
            }
            "toggleArchived" => {
                self.show_archived = !self.show_archived;
                Ok(json!({ "open": self.show_archived }))
            }
            "toggle" => self.toggle_complete(need_id()?),
            "progress" => {
                let id = need_id()?;
                let task = self.snap.task(id).ok_or("Task not found")?;
                self.db.set_in_progress(id, !task.in_progress)?;
                Ok(json!({ "inProgress": !task.in_progress }))
            }
            "archive" => self.toggle_archive(need_id()?),
            "unarchive" => {
                self.db.unarchive_project(need_id()?)?;
                Ok(json!({}))
            }
            "deleteProject" => {
                let id = need_id()?;
                self.db.delete_project(id)?;
                Ok(json!({}))
            }
            "deleteTask" => {
                let id = need_id()?;
                self.completing.remove(&id);
                self.db.delete_task(id)?;
                Ok(json!({}))
            }
            "reorder" => {
                // Sub-task drag and drop: siblings of one parent, new order.
                let ids = arg_ids(args, "ids");
                let parent = self.snap.task(*ids.first().ok_or("Nothing to reorder")?).and_then(|t| t.parent_id);
                if parent.is_none() || ids.iter().any(|i| self.snap.task(*i).and_then(|t| t.parent_id) != parent) {
                    return Err("Only sub-tasks of one task can be reordered.".into());
                }
                self.db.reorder(&ids)?;
                Ok(json!({}))
            }
            "moveDb" => self.move_db(arg_str(args, "path")),
            "saveTask" => self.save_task(id, args),
            "saveProject" => {
                let input = ProjectInput {
                    name: arg_str(args, "name").into(),
                    link: arg_str(args, "link").into(),
                    notes: arg_str(args, "notes").into(),
                };
                let id = match id {
                    Some(id) => {
                        self.db.update_project(id, &input)?;
                        id
                    }
                    None => self.db.create_project(&input)?,
                };
                Ok(json!({ "id": id }))
            }
            "createProject" => {
                let created = self.db.find_or_create_project(arg_str(args, "name"))?;
                let name = match &created {
                    Created::Existing(_, name) => name.clone(),
                    Created::New(_) => arg_str(args, "name").trim().to_string(),
                };
                Ok(json!({ "id": created.id(), "name": name, "existed": matches!(created, Created::Existing(..)) }))
            }
            "createPerson" => {
                let created = self.db.create_person(arg_str(args, "name"))?;
                let name = match &created {
                    Created::Existing(_, name) => name.clone(),
                    Created::New(_) => arg_str(args, "name").trim().to_string(),
                };
                Ok(json!({ "id": created.id(), "name": name, "existed": matches!(created, Created::Existing(..)) }))
            }
            "renamePerson" => {
                self.db.rename_person(need_id()?, arg_str(args, "name"))?;
                Ok(json!({}))
            }
            "deletePerson" => {
                self.db.delete_person(need_id()?)?;
                Ok(json!({}))
            }
            "refresh" => Ok(json!({})),
            "tick" => {
                self.tick();
                Ok(json!({}))
            }
            "flush" => {
                self.flush();
                Ok(json!({}))
            }
            other => Err(format!("Unknown action {other}")),
        }
    }

    fn toggle_complete(&mut self, id: i64) -> Outcome {
        let task = self.snap.task(id).ok_or("Task not found")?;
        if task.completed() {
            self.db.uncomplete_task(id)?;
            self.auto_dating.remove(&id);
            return Ok(json!({ "completed": false }));
        }
        if self.completing.remove(&id).is_some() {
            return Ok(json!({ "completed": false, "cancelled": true }));
        }
        if let Some(reason) = self.snap.blocked_reason(task) {
            return Err(reason.into());
        }
        self.completing.insert(id, (self.now)() + UNDO_WINDOW.as_millis() as u64);
        Ok(json!({ "pending": true }))
    }

    fn toggle_archive(&mut self, id: i64) -> Outcome {
        let project = self.snap.project(id).ok_or("Project not found")?;
        if self.archiving.remove(&id).is_some() {
            return Ok(json!({ "cancelled": true }));
        }
        if project.open_tasks > 0 {
            return Err("All tasks should be completed before a project can be archived.".into());
        }
        self.archiving.insert(id, (self.now)() + UNDO_WINDOW.as_millis() as u64);
        Ok(json!({ "pending": true }))
    }

    /// Commits countdowns whose window has closed. Cheap when idle.
    pub fn tick(&mut self) {
        let now = (self.now)();
        let due = |m: &HashMap<i64, u64>| -> Vec<i64> {
            let mut v: Vec<i64> = m.iter().filter(|(_, at)| **at <= now).map(|(id, _)| *id).collect();
            v.sort();
            v
        };
        for id in due(&self.completing) {
            self.completing.remove(&id);
            self.commit_completion(id, false);
        }
        for id in due(&self.archiving) {
            self.archiving.remove(&id);
            if let Err(e) = self.db.archive_project(id) {
                self.say(e, true);
            }
        }
        for id in due(&self.auto_dating) {
            self.auto_dating.remove(&id);
            if let Err(e) = self.db.auto_date_unblocked_dependents(id) {
                self.say(e, true);
            }
        }
    }

    fn commit_completion(&mut self, id: i64, immediate: bool) {
        match self.db.complete_task(id) {
            Ok(()) if immediate => {
                if let Err(e) = self.db.auto_date_unblocked_dependents(id) {
                    self.say(e, true);
                }
            }
            Ok(()) => {
                self.auto_dating.insert(id, (self.now)() + AUTO_DATE_DELAY.as_millis() as u64);
            }
            Err(e) => {
                let what = self.snap.task(id).map(|t| t.what.clone()).unwrap_or_default();
                self.say(format!("{what}: {e}"), true);
            }
        }
    }

    /// Quitting: commit every pending countdown now, as the web app did on
    /// navigation, including the dependents' auto-dating.
    pub fn flush(&mut self) {
        let mut tasks: Vec<i64> = self.completing.drain().map(|(id, _)| id).collect();
        tasks.sort();
        // Sub-tasks before parents, so a parent completing with its last
        // sub-task isn't blocked by it.
        tasks.sort_by_key(|id| self.snap.task(*id).is_none_or(|t| t.parent_id.is_none()));
        for id in tasks {
            self.commit_completion(id, true);
        }
        for (id, _) in self.auto_dating.drain().collect::<Vec<_>>() {
            let _ = self.db.auto_date_unblocked_dependents(id);
        }
        for (id, _) in self.archiving.drain().collect::<Vec<_>>() {
            let _ = self.db.archive_project(id);
        }
        let _ = self.reload();
    }

    /// When the next countdown needs `tick`, in ms from now.
    pub fn next_deadline(&self) -> Option<u64> {
        let now = (self.now)();
        self.completing.values().chain(self.archiving.values()).chain(self.auto_dating.values()).min().map(|at| at.saturating_sub(now))
    }

    fn save_task(&mut self, id: Option<i64>, args: &Value) -> Outcome {
        let due_text = arg_str(args, "due").trim();
        let due_on = if due_text.is_empty() {
            None
        } else {
            Some(dates::parse(due_text, self.snap.today).ok_or_else(|| format!("When? \"{due_text}\" isn't a date I understand."))?)
        };
        let input = TaskInput {
            what: arg_str(args, "what").into(),
            project_id: arg_i64(args, "projectId"),
            parent_id: arg_i64(args, "parentId"),
            due_on,
            link: arg_str(args, "link").into(),
            notes: arg_str(args, "notes").into(),
            people: arg_ids(args, "people"),
            needs: arg_ids(args, "needs"),
            enables: arg_ids(args, "enables"),
        };
        let parent = input.parent_id;
        let id = match id {
            Some(id) => {
                self.db.update_task(id, input)?;
                id
            }
            None => self.db.create_task(input)?,
        };
        if let Some(parent) = parent {
            self.expanded.insert(parent);
        }
        Ok(json!({ "id": id }))
    }

    // ------------------------------------------------------------ queries

    pub fn query(&self, name: &str, args: &Value) -> Outcome {
        let s = &self.snap;
        match name {
            "zoom" => Ok(json!({
                "value": self.db.setting("zoom").and_then(|v| v.parse::<f64>().ok()).filter(|z| (0.8..=2.0).contains(z)).unwrap_or(1.0)
            })),
            "location" => Ok(match &self.location {
                Some(l) => json!({
                    "path": l.path,
                    "source": l.source.name(),
                    "movable": l.source.movable(),
                    "configFile": l.config_file,
                }),
                None => json!({ "movable": false }),
            }),
            "task" => {
                let t = s.task(arg_i64(args, "id").ok_or("missing id")?).ok_or("Task not found")?;
                Ok(json!({
                    "id": t.id,
                    "what": t.what,
                    "projectId": t.project_id,
                    "projectName": s.project_name(t.project_id),
                    "parentId": t.parent_id,
                    "parentWhat": t.parent_id.and_then(|p| s.task(p)).map(|p| p.what.clone()),
                    "due": t.due_on.map(dates::iso).unwrap_or_default(),
                    // Always with the year: the editor re-parses this on save,
                    // and a bare "Sep 29" means the *next* Sep 29.
                    "dueText": t.due_on.map(dates::medium).unwrap_or_default(),
                    "link": t.link,
                    "notes": t.notes,
                    "completed": t.completed(),
                    "pending": self.completing.contains_key(&t.id),
                    "inProgress": t.in_progress,
                    "people": t.people.iter().filter_map(|p| s.person(*p)).map(|p| json!({"id": p.id, "name": p.name})).collect::<Vec<_>>(),
                    "needs": t.needs.iter().filter_map(|d| s.task(*d)).map(|d| self.task_ref(d)).collect::<Vec<_>>(),
                    "enables": t.enables.iter().filter_map(|d| s.task(*d)).map(|d| self.task_ref(d)).collect::<Vec<_>>(),
                    "subtasks": t.subtasks.len(),
                    "blocked": s.blocked_reason(t),
                }))
            }
            "newTask" => {
                // Defaults for the create form: the page's project, or the
                // parent's for a sub-task; assigned to me.
                let parent = arg_i64(args, "parentId").and_then(|p| s.task(p));
                let project = parent.map(|p| p.project_id).or(match self.page {
                    Page::Project(id) => Some(id),
                    _ => None,
                });
                // The web app assigned new tasks to "John Doe"; here it's
                // whoever you said you are, with the same fallback.
                let me = self.identity.and_then(|id| s.person(id)).or_else(|| s.people.iter().find(|p| p.name == "John Doe"));
                Ok(json!({
                    "projectId": project,
                    "projectName": project.map(|p| s.project_name(p)),
                    "parentId": parent.map(|p| p.id),
                    "parentWhat": parent.map(|p| p.what.clone()),
                    "people": me.map(|p| vec![json!({"id": p.id, "name": p.name})]).unwrap_or_default(),
                }))
            }
            "project" => {
                let p = s.project(arg_i64(args, "id").ok_or("missing id")?).ok_or("Project not found")?;
                Ok(json!({"id": p.id, "name": p.name, "link": p.link, "notes": p.notes, "archived": p.archived(), "open": p.open_tasks}))
            }
            "searchTasks" => {
                let exclude = arg_ids(args, "exclude");
                let hits = s.search(arg_str(args, "q"), &exclude);
                Ok(Value::Array(hits.iter().filter_map(|id| s.task(*id)).map(|t| self.task_ref(t)).collect()))
            }
            "people" => {
                let q = arg_str(args, "q").trim().to_lowercase();
                Ok(Value::Array(
                    s.people
                        .iter()
                        .filter(|p| q.is_empty() || p.name.to_lowercase().contains(&q))
                        .map(|p| json!({"id": p.id, "name": p.name, "open": p.open_tasks, "initials": initials(&p.name), "me": Some(p.id) == self.identity}))
                        .collect(),
                ))
            }
            "projects" => {
                let q = arg_str(args, "q").trim().to_lowercase();
                Ok(Value::Array(
                    s.projects
                        .iter()
                        .filter(|p| !p.archived() && (q.is_empty() || p.name.to_lowercase().contains(&q)))
                        .map(|p| json!({"id": p.id, "name": p.name, "open": p.open_tasks}))
                        .collect(),
                ))
            }
            "person" => {
                let id = arg_i64(args, "id").ok_or("missing id")?;
                let p = s.person(id).ok_or("Person not found")?;
                let open = s.open_top_level(Scope::Mine(id));
                Ok(json!({
                    "id": p.id, "name": p.name, "open": p.open_tasks,
                    "tasks": open.iter().filter_map(|t| s.task(*t)).map(|t| self.task_ref(t)).collect::<Vec<_>>(),
                }))
            }
            "parseDate" => {
                let text = arg_str(args, "text");
                if text.trim().is_empty() {
                    return Ok(json!({ "empty": true }));
                }
                Ok(match dates::parse(text, s.today) {
                    Some(d) => json!({ "iso": dates::iso(d), "text": dates::describe(d, s.today) }),
                    None => json!({ "invalid": true }),
                })
            }
            other => Err(format!("Unknown query {other}")),
        }
    }

    fn task_ref(&self, t: &Task) -> Value {
        json!({
            "id": t.id,
            "what": t.what,
            "done": t.completed(),
            "project": self.snap.project_name(t.project_id),
            "due": t.due_on.map(|d| dates::short(d, self.snap.today)),
        })
    }

    // ------------------------------------------------------------ state

    /// Page chrome, sidebar and transient feedback for QML.
    pub fn state(&self) -> Value {
        let s = &self.snap;
        let me = self.identity.and_then(|id| s.person(id));
        let active: Vec<Value> = s
            .projects
            .iter()
            .filter(|p| !p.archived())
            .map(|p| json!({"id": p.id, "name": p.name, "open": p.open_tasks}))
            .collect();
        let (page, title, subtitle, groupings, grouping) = match self.page {
            Page::Tasks => {
                let n = s.count_open_top_level(self.scope());
                let title = match (self.mine, me) {
                    (true, Some(p)) => format!("Hi, {}!", p.name),
                    _ => "All Tasks".to_string(),
                };
                ("tasks", title, model::task_count(n), vec!["date", "project", "person"], self.grouping)
            }
            Page::Projects => {
                let n = active.len();
                ("projects", "Projects".to_string(), format!("{n} active"), vec![], Grouping::Date)
            }
            Page::Project(id) => {
                let p = s.project(id);
                let n = s.count_open_top_level(self.scope());
                (
                    "project",
                    p.map(|p| p.name.clone()).unwrap_or_default(),
                    model::task_count(n),
                    vec!["date", "person"],
                    self.project_grouping,
                )
            }
        };
        let project = match self.page {
            Page::Project(id) => s.project(id).map(|p| {
                json!({"id": p.id, "name": p.name, "link": p.link, "notes": p.notes, "archived": p.archived(),
                       "archivedOn": p.archived_on.map(dates::day_month_year), "open": p.open_tasks})
            }),
            _ => None,
        };
        json!({
            "page": page,
            "projectId": match self.page { Page::Project(id) => Some(id), _ => None },
            "project": project,
            "title": title,
            "subtitle": subtitle,
            "groupings": groupings,
            "grouping": grouping_name(grouping),
            "mine": if matches!(self.page, Page::Project(_)) { self.project_mine } else { self.mine },
            "identity": me.map(|p| json!({"id": p.id, "name": p.name, "initials": initials(&p.name)})),
            "projects": active,
            "openTotal": s.count_open_top_level(Scope::All),
            "mineTotal": self.identity.map(|id| s.count_open_top_level(Scope::Mine(id))),
            "peopleCount": s.people.len(),
            "pending": self.completing.len() + self.archiving.len(),
            "nextDeadline": self.next_deadline(),
            "toast": self.toast.as_ref().map(|(seq, text, error)| json!({"seq": seq, "text": text, "error": error})),
            "today": dates::iso(s.today),
        })
    }

    // ------------------------------------------------------------ rows

    /// The visible list for the current page.
    pub fn rows(&self) -> Vec<Row> {
        let mut out = vec![];
        match self.page {
            Page::Tasks => self.task_rows(&mut out, self.scope(), self.grouping, true),
            Page::Project(_) => self.task_rows(&mut out, self.scope(), self.project_grouping, false),
            Page::Projects => self.project_rows(&mut out),
        }
        out
    }

    fn push(out: &mut Vec<Row>, key: String, value: Value) {
        out.push(Row { key, json: value.to_string() });
    }

    fn task_rows(&self, out: &mut Vec<Row>, scope: Scope, grouping: Grouping, cross_project: bool) {
        let s = &self.snap;
        let sections: Vec<Section> = s.grouped(scope, grouping);
        if sections.is_empty() {
            let (title, text) = match scope {
                Scope::Mine(_) => ("Nothing on your plate", "No open tasks are assigned to you. Press t to add one."),
                Scope::Project(_, Some(_)) => ("Nothing on your plate here", "No open tasks in this project are assigned to you."),
                Scope::Project(..) => ("No open tasks", "Press t to add the first one."),
                Scope::All if s.projects.iter().all(|p| p.archived()) => ("Welcome to Omniscient", "Create a project with P, then add tasks with t."),
                Scope::All => ("All clear", "Every task is done. Press t to add one."),
            };
            Self::push(out, "empty".into(), json!({"kind": "empty", "title": title, "text": text}));
        }
        for (si, section) in sections.iter().enumerate() {
            let skey = match (grouping, section.project) {
                (Grouping::Project, Some(p)) => format!("p{p}"),
                _ => format!("{}:{}", grouping_name(grouping), section.title),
            };
            let revealed = self.revealed.contains(&skey);
            let total = section.tasks.len() + section.hidden.len();
            Self::push(
                out,
                format!("s:{skey}"),
                json!({"kind": "section", "title": section.title, "count": total, "projectId": section.project, "first": si == 0,
                       "tone": match section.title.as_str() { "Overdue" => "overdue", "Today" => "today", _ => "" }}),
            );
            let shown = section.tasks.iter().chain(if revealed { section.hidden.iter() } else { [].iter() });
            for (i, id) in shown.enumerate() {
                self.push_task(out, &skey, *id, cross_project, i == 0);
            }
            if !revealed && !section.hidden.is_empty() {
                Self::push(out, format!("m:{skey}"), json!({"kind": "more", "section": skey, "count": section.hidden.len()}));
            }
        }
        let completed = s.completed(scope);
        if !completed.is_empty() {
            Self::push(out, "c".into(), json!({"kind": "completedToggle", "count": completed.len(), "open": self.show_completed}));
            if self.show_completed {
                for (i, id) in completed.iter().enumerate() {
                    self.push_task(out, "done", *id, cross_project, i == 0);
                }
            }
        }
    }

    fn push_task(&self, out: &mut Vec<Row>, skey: &str, id: i64, cross_project: bool, first: bool) {
        let Some(t) = self.snap.task(id) else { return };
        let expanded = self.expanded.contains(&id) && !t.subtasks.is_empty();
        out.push(Row { key: format!("t:{skey}:{id}"), json: self.task_json(t, 0, cross_project, expanded, first) });
        if expanded {
            for sid in &t.subtasks {
                if let Some(st) = self.snap.task(*sid) {
                    out.push(Row { key: format!("t:{skey}:{id}:{sid}"), json: self.task_json(st, 1, false, false, false) });
                }
            }
        }
    }

    fn task_json(&self, t: &Task, depth: u8, cross_project: bool, expanded: bool, first: bool) -> String {
        use crate::core::json::{Obj, array};
        let s = &self.snap;
        let pending = self.completing.get(&t.id).copied();
        let done = t.completed() || pending.is_some();
        let due = t.due_on.map(|d| {
            let tone = if done {
                "done"
            } else {
                match s.due(t) {
                    model::Due::Overdue => "overdue",
                    model::Due::Today => "today",
                    _ => "",
                }
            };
            Obj::with_capacity(64).str("text", &dates::short(d, s.today)).str("tone", tone).bool("auto", t.due_on_auto_set).finish()
        });
        let conflicts = s.date_conflicts(t);
        let conflict = (!conflicts.is_empty()).then(|| {
            let names: Vec<String> = conflicts.iter().map(|c| format!("\"{}\"", c.what)).collect();
            format!("Scheduled before {} {} done", names.join(", "), if names.len() == 1 { "is" } else { "are" })
        });
        let refs = |ids: &[i64]| {
            array(ids.iter().filter_map(|d| s.task(*d)).map(|d| Obj::with_capacity(96).int("id", d.id).str("what", &d.what).bool("done", d.completed()).finish()))
        };
        let open_subtasks = t.subtasks.iter().filter(|id| s.task(**id).is_some_and(|st| !st.completed())).count();
        let parent = t.parent_id.and_then(|p| s.task(p));
        Obj::with_capacity(768)
            .str("kind", "task")
            .int("id", t.id)
            .int("depth", depth as i64)
            .bool("first", first)
            .opt_int("parentId", t.parent_id)
            .str("what", &t.what)
            .bool("done", done)
            .bool("completed", t.completed())
            .opt_int("pendingUntil", pending.map(|p| p as i64))
            .bool("inProgress", t.in_progress && !done)
            .str("link", &t.link)
            .opt_str("project", cross_project.then(|| s.project_name(t.project_id)))
            .int("projectId", t.project_id)
            .str("notes", &notes_line(&t.notes))
            .bool("hasNotes", !t.notes.trim().is_empty())
            .raw("needs", &refs(&t.needs))
            .raw("enables", &refs(&t.enables))
            .raw(
                "people",
                &array(t.people.iter().filter_map(|p| s.person(*p)).map(|p| {
                    Obj::with_capacity(64).int("id", p.id).str("name", &p.name).str("initials", &initials(&p.name)).finish()
                })),
            )
            .raw("due", due.as_deref().unwrap_or("null"))
            .opt_str("conflict", conflict.as_deref())
            .opt_str("blocked", if done { None } else { s.blocked_reason(t) })
            .int("subtasks", t.subtasks.len() as i64)
            .int("openSubtasks", open_subtasks as i64)
            .bool("expanded", expanded)
            .raw("siblings", &parent.map(|p| array(p.subtasks.iter().map(|i| i.to_string()))).unwrap_or_else(|| "null".into()))
            .bool("locked", parent.is_some_and(|p| p.completed()))
            .finish()
    }

    fn project_rows(&self, out: &mut Vec<Row>) {
        let s = &self.snap;
        let (archived, active): (Vec<_>, Vec<_>) = s.projects.iter().partition(|p| p.archived());
        if active.is_empty() {
            Self::push(out, "empty".into(), json!({"kind": "empty", "title": "No projects yet", "text": "Press P to create one."}));
        }
        for (i, p) in active.iter().enumerate() {
            Self::push(
                out,
                format!("p:{}", p.id),
                json!({"kind": "project", "id": p.id, "name": p.name, "open": p.open_tasks, "link": p.link, "hasNotes": !p.notes.trim().is_empty(),
                       "notes": notes_line(&p.notes), "archived": false, "pendingUntil": self.archiving.get(&p.id), "first": i == 0}),
            );
        }
        if !archived.is_empty() {
            Self::push(out, "a".into(), json!({"kind": "archivedToggle", "count": archived.len(), "open": self.show_archived}));
            if self.show_archived {
                for (i, p) in archived.iter().enumerate() {
                    Self::push(
                        out,
                        format!("p:{}", p.id),
                        json!({"kind": "project", "id": p.id, "name": p.name, "open": p.open_tasks, "link": p.link, "hasNotes": !p.notes.trim().is_empty(),
                               "notes": notes_line(&p.notes), "archived": true, "archivedOn": p.archived_on.map(dates::day_month_year), "first": i == 0}),
                    );
                }
            }
        }
    }

    pub fn page(&self) -> Page {
        self.page
    }

    /// Identifies the page being shown: when it changes, the list is a new
    /// document (replace it) rather than an edit (animate the difference).
    pub fn view_key(&self) -> String {
        format!("{:?}", self.page)
    }

    pub fn task_project_of(&self, id: i64) -> Option<i64> {
        self.task_project(id)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::cell::Cell;

    thread_local! {
        static CLOCK: Cell<u64> = const { Cell::new(1_000_000) };
    }
    fn clock() -> u64 {
        CLOCK.with(Cell::get)
    }
    fn advance(ms: u64) {
        CLOCK.with(|c| c.set(c.get() + ms));
    }

    fn ctl() -> Controller {
        let db = Db::memory().unwrap();
        Controller::new(db).unwrap().with_clock(clock)
    }

    #[test]
    fn moving_the_database_keeps_data_and_pending_completions() {
        let dir = std::env::temp_dir().join(format!("omniscient-move-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        let old = dir.join("old.sqlite3");
        let config = dir.join("cfg/config.toml");
        let mut c = Controller::new(Db::open(&old).unwrap()).unwrap().with_clock(clock);
        let loc = Location { path: old.clone(), source: location::Source::Default, config_file: config.clone() };
        c.set_location(loc.clone());
        let project = ok(&mut c, "createProject", json!({ "name": "P" }))["id"].as_i64().unwrap();
        let task = ok(&mut c, "saveTask", json!({ "what": "Pending", "projectId": project, "due": "" }))["id"].as_i64().unwrap();
        ok(&mut c, "toggle", json!({ "id": task })); // counting down, not yet committed

        let new = dir.join("new/data.sqlite3");
        let out = ok(&mut c, "moveDb", json!({ "path": new.to_str().unwrap() }));
        assert_eq!(out["path"], json!(new));
        assert_eq!(location::read_config(&config), Some(new.clone()));
        assert_eq!(c.query("location", &json!({})).unwrap()["path"], json!(new));
        // The countdown was committed into the copy, and the app now writes there.
        assert!(c.snapshot().task(task).unwrap().completed());
        ok(&mut c, "saveProject", json!({ "name": "Q" }));
        drop(c);
        let reopened = Db::open(&new).unwrap().snapshot().unwrap();
        assert_eq!(reopened.projects.len(), 2);
        assert!(reopened.task(task).unwrap().completed());
        // The old file stays as it was, without the later write.
        assert_eq!(Db::open(&old).unwrap().snapshot().unwrap().projects.len(), 1);
        std::fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn moving_the_database_refuses_bad_requests_and_changes_nothing() {
        let dir = std::env::temp_dir().join(format!("omniscient-move-bad-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        let old = dir.join("old.sqlite3");
        let config = dir.join("config.toml");
        let mut c = Controller::new(Db::open(&old).unwrap()).unwrap().with_clock(clock);
        let err = |c: &mut Controller, p: &str| c.act("moveDb", &json!({ "path": p })).unwrap_err();
        assert!(err(&mut c, "/x/y").contains("isn't available"));

        let mut loc = Location { path: old.clone(), source: location::Source::Flag, config_file: config.clone() };
        c.set_location(loc.clone());
        assert!(err(&mut c, "/x/y").contains("--db"));
        loc.source = location::Source::Env;
        c.set_location(loc.clone());
        assert!(err(&mut c, "/x/y").contains("OMNISCIENT_DB"));

        loc.source = location::Source::Default;
        c.set_location(loc);
        assert!(err(&mut c, "").contains("Enter a path"));
        assert!(err(&mut c, "relative.db").contains("absolute"));
        assert!(err(&mut c, old.to_str().unwrap()).contains("already stored"));
        let taken = dir.join("taken.sqlite3");
        std::fs::write(&taken, b"precious").unwrap();
        assert!(err(&mut c, taken.to_str().unwrap()).contains("already exists"));
        assert_eq!(std::fs::read(&taken).unwrap(), b"precious");
        assert!(!config.exists(), "a failed move must not write the config");
        assert_eq!(c.query("location", &json!({})).unwrap()["path"], json!(old));
        std::fs::remove_dir_all(dir).unwrap();
    }

    fn ok(c: &mut Controller, name: &str, args: Value) -> Value {
        c.act(name, &args).unwrap_or_else(|e| panic!("{name}: {e}"))
    }

    fn kinds(c: &Controller) -> Vec<String> {
        c.rows()
            .iter()
            .map(|r| {
                let v: Value = serde_json::from_str(&r.json).unwrap();
                match v["kind"].as_str().unwrap() {
                    "section" => format!("[{}]", v["title"].as_str().unwrap()),
                    "task" => v["what"].as_str().unwrap().to_string(),
                    k => format!("<{k}>"),
                }
            })
            .collect()
    }

    fn setup() -> (Controller, i64) {
        let mut c = ctl();
        let p = ok(&mut c, "saveProject", json!({"name": "Alpha"}))["id"].as_i64().unwrap();
        (c, p)
    }

    fn task(c: &mut Controller, project: i64, what: &str, extra: Value) -> i64 {
        let mut args = json!({"what": what, "projectId": project});
        if let Value::Object(m) = extra {
            for (k, v) in m {
                args[k] = v;
            }
        }
        ok(c, "saveTask", args)["id"].as_i64().unwrap()
    }

    #[test]
    fn empty_database_welcomes() {
        let c = ctl();
        assert_eq!(kinds(&c), ["<empty>"]);
        assert_eq!(c.state()["title"], "All Tasks");
    }

    #[test]
    fn tasks_page_lists_date_sections() {
        let (mut c, p) = setup();
        task(&mut c, p, "Late", json!({"due": "-2"}));
        task(&mut c, p, "Now", json!({"due": "today"}));
        task(&mut c, p, "Someday", json!({}));
        assert_eq!(kinds(&c), ["[Overdue]", "Late", "[Today]", "Now", "[Later]", "Someday"]);
        assert_eq!(c.state()["subtitle"], "3 tasks");
    }

    #[test]
    fn completion_waits_for_the_undo_window() {
        let (mut c, p) = setup();
        let blocker = task(&mut c, p, "Blocker", json!({}));
        let blocked = task(&mut c, p, "Blocked", json!({"needs": [blocker]}));
        assert_eq!(c.act("toggle", &json!({"id": blocked})).unwrap_err(), "All dependencies must be completed first!");

        ok(&mut c, "toggle", json!({"id": blocker}));
        let row: Value = serde_json::from_str(&c.rows().iter().find(|r| r.json.contains("\"Blocker\"")).unwrap().json).unwrap();
        assert_eq!(row["done"], true, "struck through immediately");
        assert!(row["pendingUntil"].is_u64());
        assert!(!c.snapshot().task(blocker).unwrap().completed(), "not written yet");

        // Unticking inside the window cancels.
        ok(&mut c, "toggle", json!({"id": blocker}));
        advance(20_000);
        ok(&mut c, "tick", json!({}));
        assert!(!c.snapshot().task(blocker).unwrap().completed());

        ok(&mut c, "toggle", json!({"id": blocker}));
        advance(10_000);
        ok(&mut c, "tick", json!({}));
        assert!(c.snapshot().task(blocker).unwrap().completed());
        assert_eq!(c.snapshot().task(blocked).unwrap().due_on, None, "dependents are dated 30 s later");
        advance(30_000);
        ok(&mut c, "tick", json!({}));
        assert_eq!(c.snapshot().task(blocked).unwrap().due_on, Some(c.snapshot().today));
    }

    #[test]
    fn quitting_commits_countdowns_and_auto_dates() {
        let (mut c, p) = setup();
        let blocker = task(&mut c, p, "Blocker", json!({}));
        let blocked = task(&mut c, p, "Blocked", json!({"needs": [blocker]}));
        let parent = task(&mut c, p, "Parent", json!({}));
        let child = task(&mut c, p, "Child", json!({"parentId": parent}));
        ok(&mut c, "toggle", json!({"id": blocker}));
        ok(&mut c, "toggle", json!({"id": child}));
        assert_eq!(c.act("toggle", &json!({"id": parent})).unwrap_err(), "Complete all sub-tasks first!", "pending isn't done yet");
        ok(&mut c, "flush", json!({}));
        let s = c.snapshot();
        assert!(s.task(blocker).unwrap().completed());
        assert!(s.task(child).unwrap().completed());
        assert_eq!(s.task(blocked).unwrap().due_on, Some(s.today), "auto-dated on quit, not lost");
    }

    #[test]
    fn deleting_a_parent_cancels_its_subtask_countdowns() {
        let (mut c, p) = setup();
        let parent = task(&mut c, p, "Parent", json!({}));
        let child = task(&mut c, p, "Child", json!({"parentId": parent}));
        ok(&mut c, "toggle", json!({"id": child}));
        ok(&mut c, "deleteTask", json!({"id": parent}));
        assert_eq!(c.state()["pending"], 0);
        advance(10_000);
        ok(&mut c, "tick", json!({}));
        assert!(c.state()["toast"].is_null(), "no error about a vanished task");
    }

    #[test]
    fn subtasks_expand_under_their_parent() {
        let (mut c, p) = setup();
        let parent = task(&mut c, p, "Parent", json!({}));
        task(&mut c, p, "Child", json!({"parentId": parent}));
        assert_eq!(kinds(&c), ["[Later]", "Parent", "Child"], "adding a sub-task opens its parent");
        ok(&mut c, "expand", json!({"id": parent}));
        assert_eq!(kinds(&c), ["[Later]", "Parent"]);
    }

    #[test]
    fn mine_filter_needs_an_identity() {
        let (mut c, p) = setup();
        assert!(c.act("filter", &json!({"mine": true})).is_err());
        let ann = ok(&mut c, "createPerson", json!({"name": "Ann Lee"}))["id"].as_i64().unwrap();
        task(&mut c, p, "Hers", json!({"people": [ann]}));
        task(&mut c, p, "Nobody's", json!({}));
        ok(&mut c, "identity", json!({"id": ann}));
        assert_eq!(c.state()["title"], "Hi, Ann Lee!");
        assert_eq!(c.state()["identity"]["initials"], "AL");
        assert_eq!(kinds(&c), ["[Later]", "Hers"]);
        ok(&mut c, "filter", json!({"mine": false}));
        assert_eq!(kinds(&c).len(), 3);
    }

    #[test]
    fn mine_filter_works_inside_a_project_without_touching_the_global_one() {
        let (mut c, p) = setup();
        let ann = ok(&mut c, "createPerson", json!({"name": "Ann Lee"}))["id"].as_i64().unwrap();
        task(&mut c, p, "Hers", json!({"people": [ann]}));
        task(&mut c, p, "Nobody's", json!({}));
        ok(&mut c, "identity", json!({"id": ann}));
        ok(&mut c, "go", json!({"page": "project", "id": p}));
        assert_eq!(kinds(&c).len(), 3, "projects start on All");
        ok(&mut c, "filter", json!({"mine": true}));
        assert_eq!(kinds(&c), ["[Later]", "Hers"]);
        assert_eq!(c.state()["mine"], true);
        ok(&mut c, "go", json!({"page": "tasks"}));
        ok(&mut c, "filter", json!({"mine": false}));
        ok(&mut c, "go", json!({"page": "project", "id": p}));
        assert_eq!(kinds(&c), ["[Later]", "Hers"], "project choice is independent");
    }

    #[test]
    fn projects_page_and_archive_countdown() {
        let (mut c, p) = setup();
        let t = task(&mut c, p, "Only", json!({}));
        ok(&mut c, "go", json!({"page": "projects"}));
        assert_eq!(c.act("archive", &json!({"id": p})).unwrap_err(), "All tasks should be completed before a project can be archived.");
        ok(&mut c, "toggle", json!({"id": t}));
        ok(&mut c, "flush", json!({}));
        ok(&mut c, "archive", json!({"id": p}));
        advance(10_000);
        ok(&mut c, "tick", json!({}));
        assert_eq!(kinds(&c), ["<empty>", "<archivedToggle>"]);
        ok(&mut c, "toggleArchived", json!({}));
        assert_eq!(kinds(&c).len(), 3);
    }

    #[test]
    fn project_page_and_grouping() {
        let (mut c, p) = setup();
        let ann = ok(&mut c, "createPerson", json!({"name": "Ann"}))["id"].as_i64().unwrap();
        task(&mut c, p, "A", json!({"people": [ann]}));
        task(&mut c, p, "B", json!({}));
        ok(&mut c, "go", json!({"page": "project", "id": p}));
        assert_eq!(c.state()["title"], "Alpha");
        ok(&mut c, "group", json!({"by": "person"}));
        assert_eq!(kinds(&c), ["[Ann]", "A", "[Unassigned]", "B"]);
        ok(&mut c, "group", json!({"by": "project"}));
        assert_eq!(c.state()["grouping"], "person", "projects can't group by project");
    }

    #[test]
    fn typed_dates_and_bad_dates() {
        let (mut c, p) = setup();
        let e = c.act("saveTask", &json!({"what": "x", "projectId": p, "due": "someday maybe"})).unwrap_err();
        assert!(e.contains("isn't a date"), "{e}");
        let id = task(&mut c, p, "Tomorrow", json!({"due": "tom"}));
        assert_eq!(c.snapshot().task(id).unwrap().due_on, Some(c.snapshot().today + chrono::Duration::days(1)));
        let parsed = c.query("parseDate", &json!({"text": "tom"})).unwrap();
        assert!(parsed["text"].as_str().unwrap().starts_with("Tomorrow"));
    }

    #[test]
    fn one_edit_changes_one_row() {
        let (mut c, p) = setup();
        for i in 0..5 {
            task(&mut c, p, &format!("T{i}"), json!({}));
        }
        let before = c.rows();
        let t2 = c.snapshot().tasks.values().find(|t| t.what == "T2").unwrap().id;
        ok(&mut c, "progress", json!({"id": t2}));
        let ops = crate::core::rows::diff(&before, &c.rows());
        assert_eq!(ops.len(), 1, "{ops:?}");
        assert!(matches!(ops[0], crate::core::rows::Op::Change { .. }));
    }

    #[test]
    fn saving_the_editor_unchanged_keeps_past_and_future_dates() {
        let (mut c, p) = setup();
        for due in ["2026-09-29", "2026-10-01", "2027-03-05", "2020-01-01"] {
            let id = task(&mut c, p, due, json!({"due": due}));
            let t = c.query("task", &json!({"id": id})).unwrap();
            ok(&mut c, "saveTask", json!({"id": id, "what": due, "projectId": p, "due": t["dueText"]}));
            assert_eq!(c.snapshot().task(id).unwrap().due_on.map(dates::iso).as_deref(), Some(due), "shown as {}", t["dueText"]);
        }
    }

    #[test]
    fn markdown_first_line() {
        assert_eq!(notes_line("\n# **Big** plan\nmore"), "Big plan");
        assert_eq!(notes_line("- see [doc](https://x) now"), "see doc now");
    }
}
