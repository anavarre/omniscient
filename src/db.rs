//! SQLite persistence and every write rule (validation messages, blocking,
//! auto-dating). Messages match the web app's wording.

use std::collections::HashMap;
use std::path::{Path, PathBuf};

use chrono::NaiveDate;
use rusqlite::{Connection, OptionalExtension, params};

use crate::dates;
use crate::model::{Person, Project, Snapshot, Task};

pub type Result<T> = std::result::Result<T, String>;

fn err(e: rusqlite::Error) -> String {
    e.to_string()
}

const SCHEMA: &str = "
CREATE TABLE IF NOT EXISTS people (
  id INTEGER PRIMARY KEY,
  name TEXT NOT NULL,
  created_at TEXT NOT NULL,
  updated_at TEXT NOT NULL
);
CREATE UNIQUE INDEX IF NOT EXISTS people_name ON people (lower(name));
CREATE TABLE IF NOT EXISTS projects (
  id INTEGER PRIMARY KEY,
  name TEXT NOT NULL,
  link TEXT NOT NULL DEFAULT '',
  notes TEXT NOT NULL DEFAULT '',
  archived_at TEXT,
  created_at TEXT NOT NULL,
  updated_at TEXT NOT NULL
);
CREATE UNIQUE INDEX IF NOT EXISTS projects_name ON projects (lower(name));
CREATE TABLE IF NOT EXISTS tasks (
  id INTEGER PRIMARY KEY,
  project_id INTEGER NOT NULL REFERENCES projects (id) ON DELETE CASCADE,
  parent_task_id INTEGER REFERENCES tasks (id) ON DELETE CASCADE,
  what TEXT NOT NULL,
  due_on TEXT,
  due_on_auto_set INTEGER NOT NULL DEFAULT 0,
  link TEXT NOT NULL DEFAULT '',
  notes TEXT NOT NULL DEFAULT '',
  completed_at TEXT,
  in_progress_at TEXT,
  position INTEGER NOT NULL DEFAULT 0,
  created_at TEXT NOT NULL,
  updated_at TEXT NOT NULL
);
CREATE INDEX IF NOT EXISTS tasks_project ON tasks (project_id);
CREATE INDEX IF NOT EXISTS tasks_parent ON tasks (parent_task_id);
CREATE TABLE IF NOT EXISTS task_assignments (
  task_id INTEGER NOT NULL REFERENCES tasks (id) ON DELETE CASCADE,
  person_id INTEGER NOT NULL REFERENCES people (id) ON DELETE CASCADE,
  PRIMARY KEY (task_id, person_id)
);
CREATE TABLE IF NOT EXISTS task_dependencies (
  task_id INTEGER NOT NULL REFERENCES tasks (id) ON DELETE CASCADE,
  depends_on_task_id INTEGER NOT NULL REFERENCES tasks (id) ON DELETE CASCADE,
  PRIMARY KEY (task_id, depends_on_task_id),
  CHECK (task_id <> depends_on_task_id)
);
CREATE TABLE IF NOT EXISTS settings (
  key TEXT PRIMARY KEY,
  value TEXT NOT NULL
);
";

#[derive(Clone, Debug, Default)]
pub struct TaskInput {
    pub what: String,
    pub project_id: Option<i64>,
    pub parent_id: Option<i64>,
    pub due_on: Option<NaiveDate>,
    pub link: String,
    pub notes: String,
    pub people: Vec<i64>,
    pub needs: Vec<i64>,
    pub enables: Vec<i64>,
}

#[derive(Clone, Debug, Default)]
pub struct ProjectInput {
    pub name: String,
    pub link: String,
    pub notes: String,
}

/// Result of creating a named record: a duplicate name (ignoring case)
/// resolves to the existing record, as the web app's pickers did.
pub enum Created {
    New(i64),
    Existing(i64, String),
}

impl Created {
    pub fn id(&self) -> i64 {
        match self {
            Created::New(id) | Created::Existing(id, _) => *id,
        }
    }
}

pub struct Db {
    conn: Connection,
}

pub fn default_path() -> PathBuf {
    let base = std::env::var_os("XDG_DATA_HOME")
        .map(PathBuf::from)
        .filter(|p| p.is_absolute())
        .unwrap_or_else(|| PathBuf::from(std::env::var_os("HOME").unwrap_or_default()).join(".local/share"));
    base.join("taskestra").join("taskestra.sqlite3")
}

fn valid_link(link: &str) -> bool {
    let rest = link.strip_prefix("https://").or_else(|| link.strip_prefix("http://"));
    rest.is_some_and(|r| !r.is_empty() && !r.chars().any(char::is_whitespace))
}

fn check_link(link: &str) -> Result<()> {
    if link.is_empty() || valid_link(link) {
        Ok(())
    } else {
        Err("Link must be a valid URL (starting with http:// or https://)".into())
    }
}

impl Db {
    pub fn open(path: &Path) -> Result<Db> {
        if let Some(dir) = path.parent() {
            std::fs::create_dir_all(dir).map_err(|e| format!("Cannot create {}: {e}", dir.display()))?;
        }
        let conn = Connection::open(path).map_err(err)?;
        Self::init(conn)
    }

    #[cfg(test)]
    pub fn memory() -> Result<Db> {
        Self::init(Connection::open_in_memory().map_err(err)?)
    }

    fn init(conn: Connection) -> Result<Db> {
        conn.execute_batch("PRAGMA journal_mode = WAL; PRAGMA foreign_keys = ON; PRAGMA synchronous = NORMAL;")
            .map_err(err)?;
        conn.execute_batch(SCHEMA).map_err(err)?;
        Ok(Db { conn })
    }

    // ------------------------------------------------------------ reading

    pub fn snapshot(&self) -> Result<Snapshot> {
        let c = &self.conn;
        let mut tasks: HashMap<i64, Task> = HashMap::new();
        let mut stmt = c
            .prepare(
                "SELECT id, project_id, parent_task_id, what, due_on, due_on_auto_set, link, notes,
                        completed_at, in_progress_at, position FROM tasks",
            )
            .map_err(err)?;
        let rows = stmt
            .query_map([], |r| {
                Ok(Task {
                    id: r.get(0)?,
                    project_id: r.get(1)?,
                    parent_id: r.get(2)?,
                    what: r.get(3)?,
                    due_on: r.get::<_, Option<String>>(4)?.as_deref().and_then(dates::from_iso),
                    due_on_auto_set: r.get(5)?,
                    link: r.get(6)?,
                    notes: r.get(7)?,
                    completed_at: r.get(8)?,
                    in_progress: r.get::<_, Option<String>>(9)?.is_some(),
                    position: r.get(10)?,
                    people: vec![],
                    needs: vec![],
                    enables: vec![],
                    subtasks: vec![],
                })
            })
            .map_err(err)?;
        for t in rows {
            let t = t.map_err(err)?;
            tasks.insert(t.id, t);
        }

        let mut people: Vec<Person> = c
            .prepare("SELECT id, name FROM people ORDER BY name COLLATE NOCASE, id")
            .map_err(err)?
            .query_map([], |r| Ok(Person { id: r.get(0)?, name: r.get(1)?, open_tasks: 0 }))
            .map_err(err)?
            .collect::<std::result::Result<_, _>>()
            .map_err(err)?;

        let mut assign = c.prepare("SELECT task_id, person_id FROM task_assignments").map_err(err)?;
        for row in assign.query_map([], |r| Ok((r.get::<_, i64>(0)?, r.get::<_, i64>(1)?))).map_err(err)? {
            let (task, person) = row.map_err(err)?;
            if let Some(t) = tasks.get_mut(&task) {
                t.people.push(person);
            }
        }
        let mut deps = c.prepare("SELECT task_id, depends_on_task_id FROM task_dependencies").map_err(err)?;
        let pairs: Vec<(i64, i64)> = deps
            .query_map([], |r| Ok((r.get(0)?, r.get(1)?)))
            .map_err(err)?
            .collect::<std::result::Result<_, _>>()
            .map_err(err)?;
        for (task, dep) in pairs {
            if !(tasks.contains_key(&task) && tasks.contains_key(&dep)) {
                continue;
            }
            tasks.get_mut(&task).unwrap().needs.push(dep);
            tasks.get_mut(&dep).unwrap().enables.push(task);
        }

        let mut children: Vec<(i64, i64, i64)> =
            tasks.values().filter_map(|t| t.parent_id.map(|p| (p, t.position, t.id))).collect();
        children.sort();
        for (parent, _, child) in children {
            if let Some(p) = tasks.get_mut(&parent) {
                p.subtasks.push(child);
            }
        }

        // Keep chips and assignee lists in a stable, readable order.
        let names: HashMap<i64, String> = people.iter().map(|p| (p.id, p.name.to_lowercase())).collect();
        let titles: HashMap<i64, String> = tasks.values().map(|t| (t.id, t.what.to_lowercase())).collect();
        for t in tasks.values_mut() {
            t.people.sort_by(|a, b| names.get(a).cmp(&names.get(b)));
            t.needs.sort_by(|a, b| titles.get(a).cmp(&titles.get(b)));
            t.enables.sort_by(|a, b| titles.get(a).cmp(&titles.get(b)));
        }

        for p in &mut people {
            p.open_tasks = tasks.values().filter(|t| !t.completed() && t.people.contains(&p.id)).count();
        }

        let projects: Vec<Project> = c
            .prepare("SELECT id, name, link, notes, archived_at FROM projects ORDER BY name COLLATE NOCASE, id")
            .map_err(err)?
            .query_map([], |r| {
                Ok(Project {
                    id: r.get(0)?,
                    name: r.get(1)?,
                    link: r.get(2)?,
                    notes: r.get(3)?,
                    archived_on: r.get::<_, Option<String>>(4)?.as_deref().and_then(dates::from_iso),
                    open_tasks: 0,
                })
            })
            .map_err(err)?
            .collect::<std::result::Result<Vec<_>, _>>()
            .map_err(err)?
            .into_iter()
            .map(|mut p| {
                p.open_tasks = tasks.values().filter(|t| !t.completed() && t.project_id == p.id).count();
                p
            })
            .collect();

        Ok(Snapshot { today: dates::today(), people, projects, tasks })
    }

    pub fn setting(&self, key: &str) -> Option<String> {
        self.conn.query_row("SELECT value FROM settings WHERE key = ?1", [key], |r| r.get(0)).optional().ok().flatten()
    }

    pub fn set_setting(&self, key: &str, value: Option<&str>) -> Result<()> {
        match value {
            Some(v) => self.conn.execute(
                "INSERT INTO settings (key, value) VALUES (?1, ?2) ON CONFLICT (key) DO UPDATE SET value = ?2",
                params![key, v],
            ),
            None => self.conn.execute("DELETE FROM settings WHERE key = ?1", [key]),
        }
        .map(|_| ())
        .map_err(err)
    }

    fn open_task_count(&self, sql: &str, id: i64) -> Result<i64> {
        self.conn.query_row(sql, [id], |r| r.get(0)).map_err(err)
    }

    // ------------------------------------------------------------ people

    pub fn create_person(&self, name: &str) -> Result<Created> {
        let name = name.trim();
        if name.is_empty() {
            return Err("Name can't be blank".into());
        }
        let existing: Option<(i64, String)> = self
            .conn
            .query_row("SELECT id, name FROM people WHERE lower(name) = lower(?1)", [name], |r| Ok((r.get(0)?, r.get(1)?)))
            .optional()
            .map_err(err)?;
        if let Some((id, name)) = existing {
            return Ok(Created::Existing(id, name));
        }
        let now = dates::now_stamp();
        self.conn
            .execute("INSERT INTO people (name, created_at, updated_at) VALUES (?1, ?2, ?2)", params![name, now])
            .map_err(err)?;
        Ok(Created::New(self.conn.last_insert_rowid()))
    }

    pub fn rename_person(&self, id: i64, name: &str) -> Result<()> {
        let name = name.trim();
        if name.is_empty() {
            return Err("Name can't be blank".into());
        }
        let taken: bool = self
            .conn
            .query_row("SELECT EXISTS (SELECT 1 FROM people WHERE lower(name) = lower(?1) AND id <> ?2)", params![name, id], |r| r.get(0))
            .map_err(err)?;
        if taken {
            return Err("Name has already been taken".into());
        }
        self.conn
            .execute("UPDATE people SET name = ?1, updated_at = ?2 WHERE id = ?3", params![name, dates::now_stamp(), id])
            .map(|_| ())
            .map_err(err)
    }

    pub fn delete_person(&self, id: i64) -> Result<()> {
        let open = self.open_task_count(
            "SELECT count(*) FROM task_assignments a JOIN tasks t ON t.id = a.task_id
             WHERE a.person_id = ?1 AND t.completed_at IS NULL",
            id,
        )?;
        if open > 0 {
            return Err("Unassign all tasks first!".into());
        }
        if self.setting("identity").as_deref() == Some(&id.to_string()) {
            self.set_setting("identity", None)?;
        }
        self.conn.execute("DELETE FROM people WHERE id = ?1", [id]).map(|_| ()).map_err(err)
    }

    // ------------------------------------------------------------ projects

    fn validate_project(&self, input: &ProjectInput, id: Option<i64>) -> Result<(String, String)> {
        let name = input.name.trim().to_string();
        let link = input.link.trim().to_string();
        if name.is_empty() {
            return Err("Name can't be blank".into());
        }
        let existing: Option<String> = self
            .conn
            .query_row(
                "SELECT name FROM projects WHERE lower(name) = lower(?1) AND id <> ?2",
                params![name, id.unwrap_or(-1)],
                |r| r.get(0),
            )
            .optional()
            .map_err(err)?;
        if let Some(existing) = existing {
            return Err(format!("\"{existing}\" already exists as a project."));
        }
        check_link(&link)?;
        Ok((name, link))
    }

    pub fn create_project(&self, input: &ProjectInput) -> Result<i64> {
        let (name, link) = self.validate_project(input, None)?;
        let now = dates::now_stamp();
        self.conn
            .execute(
                "INSERT INTO projects (name, link, notes, created_at, updated_at) VALUES (?1, ?2, ?3, ?4, ?4)",
                params![name, link, input.notes.trim_end(), now],
            )
            .map_err(err)?;
        Ok(self.conn.last_insert_rowid())
    }

    /// Quick-add from the task form: a duplicate name selects the existing project.
    pub fn find_or_create_project(&self, name: &str) -> Result<Created> {
        let existing: Option<(i64, String)> = self
            .conn
            .query_row("SELECT id, name FROM projects WHERE lower(name) = lower(?1)", [name.trim()], |r| Ok((r.get(0)?, r.get(1)?)))
            .optional()
            .map_err(err)?;
        match existing {
            Some((id, name)) => Ok(Created::Existing(id, name)),
            None => self.create_project(&ProjectInput { name: name.into(), ..Default::default() }).map(Created::New),
        }
    }

    pub fn update_project(&self, id: i64, input: &ProjectInput) -> Result<()> {
        let (name, link) = self.validate_project(input, Some(id))?;
        self.conn
            .execute(
                "UPDATE projects SET name = ?1, link = ?2, notes = ?3, updated_at = ?4 WHERE id = ?5",
                params![name, link, input.notes.trim_end(), dates::now_stamp(), id],
            )
            .map(|_| ())
            .map_err(err)
    }

    fn project_open_tasks(&self, id: i64) -> Result<i64> {
        self.open_task_count("SELECT count(*) FROM tasks WHERE project_id = ?1 AND completed_at IS NULL", id)
    }

    pub fn archive_project(&self, id: i64) -> Result<()> {
        if self.project_open_tasks(id)? > 0 {
            return Err("All tasks should be completed before a project can be archived.".into());
        }
        self.conn
            .execute("UPDATE projects SET archived_at = ?1, updated_at = ?1 WHERE id = ?2", params![dates::now_stamp(), id])
            .map(|_| ())
            .map_err(err)
    }

    pub fn unarchive_project(&self, id: i64) -> Result<()> {
        self.conn
            .execute("UPDATE projects SET archived_at = NULL, updated_at = ?1 WHERE id = ?2", params![dates::now_stamp(), id])
            .map(|_| ())
            .map_err(err)
    }

    pub fn delete_project(&self, id: i64) -> Result<()> {
        if self.project_open_tasks(id)? > 0 {
            let name: String = self.conn.query_row("SELECT name FROM projects WHERE id = ?1", [id], |r| r.get(0)).map_err(err)?;
            return Err(format!(
                "\"{name}\" cannot be deleted because it has open tasks. Complete or delete all tasks first."
            ));
        }
        self.conn.execute("DELETE FROM projects WHERE id = ?1", [id]).map(|_| ()).map_err(err)
    }

    // ------------------------------------------------------------ tasks

    fn parent_of(&self, id: i64) -> Result<Option<(Option<i64>, i64, Option<NaiveDate>)>> {
        self.conn
            .query_row("SELECT parent_task_id, project_id, due_on FROM tasks WHERE id = ?1", [id], |r| {
                Ok((r.get(0)?, r.get(1)?, r.get::<_, Option<String>>(2)?.as_deref().and_then(dates::from_iso)))
            })
            .optional()
            .map_err(err)
    }

    /// Validates and normalises input; returns the project id to store.
    fn validate_task(&self, input: &mut TaskInput, id: Option<i64>) -> Result<i64> {
        input.what = input.what.trim().to_string();
        input.link = input.link.trim().to_string();
        let mut errors = vec![];
        if input.what.is_empty() {
            errors.push("What can't be blank".to_string());
        }
        let mut project = input.project_id;
        if let Some(parent) = input.parent_id {
            match self.parent_of(parent)? {
                None => errors.push("Parent task not found".into()),
                Some((grandparent, parent_project, parent_due)) => {
                    if grandparent.is_some() {
                        errors.push("Sub-tasks cannot have sub-tasks".into());
                    }
                    // A sub-task always lives in its parent's project.
                    project = Some(parent_project);
                    if let (Some(due), Some(parent_due)) = (input.due_on, parent_due)
                        && due < parent_due {
                            errors.push(format!(
                                "Due on cannot be before the parent task's due date ({})",
                                dates::medium(parent_due)
                            ));
                        }
                }
            }
        }
        if project.is_none() {
            errors.push("Project must be selected".into());
        }
        if let Err(e) = check_link(&input.link) {
            errors.push(e);
        }
        if let Some(id) = id {
            input.needs.retain(|t| *t != id);
            input.enables.retain(|t| *t != id);
        }
        input.people.dedup();
        if errors.is_empty() { Ok(project.unwrap()) } else { Err(errors.join("\n")) }
    }

    fn write_links(&self, id: i64, input: &TaskInput) -> Result<()> {
        let c = &self.conn;
        c.execute("DELETE FROM task_assignments WHERE task_id = ?1", [id]).map_err(err)?;
        c.execute("DELETE FROM task_dependencies WHERE task_id = ?1 OR depends_on_task_id = ?1", [id]).map_err(err)?;
        for p in &input.people {
            c.execute("INSERT OR IGNORE INTO task_assignments (task_id, person_id) VALUES (?1, ?2)", params![id, p])
                .map_err(err)?;
        }
        for dep in &input.needs {
            c.execute("INSERT OR IGNORE INTO task_dependencies (task_id, depends_on_task_id) VALUES (?1, ?2)", params![id, dep])
                .map_err(err)?;
        }
        for dependent in &input.enables {
            c.execute(
                "INSERT OR IGNORE INTO task_dependencies (task_id, depends_on_task_id) VALUES (?1, ?2)",
                params![dependent, id],
            )
            .map_err(err)?;
        }
        Ok(())
    }

    pub fn create_task(&self, mut input: TaskInput) -> Result<i64> {
        let project = self.validate_task(&mut input, None)?;
        let tx = self.conn.unchecked_transaction().map_err(err)?;
        let position: i64 = match input.parent_id {
            Some(parent) => tx
                .query_row("SELECT coalesce(max(position), -1) + 1 FROM tasks WHERE parent_task_id = ?1", [parent], |r| r.get(0))
                .map_err(err)?,
            None => 0,
        };
        let now = dates::now_stamp();
        tx.execute(
            "INSERT INTO tasks (project_id, parent_task_id, what, due_on, link, notes, position, created_at, updated_at)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?8)",
            params![project, input.parent_id, input.what, input.due_on.map(dates::iso), input.link, input.notes.trim_end(), position, now],
        )
        .map_err(err)?;
        let id = tx.last_insert_rowid();
        self.write_links(id, &input)?;
        tx.commit().map_err(err)?;
        Ok(id)
    }

    pub fn update_task(&self, id: i64, mut input: TaskInput) -> Result<()> {
        let (current_parent, _, current_due) = self.parent_of(id)?.ok_or("Task not found")?;
        input.parent_id = current_parent;
        let project = self.validate_task(&mut input, Some(id))?;
        let tx = self.conn.unchecked_transaction().map_err(err)?;
        // Editing the date by hand means it is no longer an automatic one.
        let date_changed = current_due != input.due_on;
        tx.execute(
            "UPDATE tasks SET project_id = ?1, what = ?2, due_on = ?3, link = ?4, notes = ?5, updated_at = ?6,
                    due_on_auto_set = CASE WHEN ?7 THEN 0 ELSE due_on_auto_set END
             WHERE id = ?8",
            params![project, input.what, input.due_on.map(dates::iso), input.link, input.notes.trim_end(), dates::now_stamp(), date_changed, id],
        )
        .map_err(err)?;
        // Sub-tasks follow their parent to a new project.
        tx.execute("UPDATE tasks SET project_id = ?1 WHERE parent_task_id = ?2", params![project, id]).map_err(err)?;
        self.write_links(id, &input)?;
        tx.commit().map_err(err)
    }

    pub fn delete_task(&self, id: i64) -> Result<()> {
        self.conn.execute("DELETE FROM tasks WHERE id = ?1", [id]).map(|_| ()).map_err(err)
    }

    pub fn complete_task(&self, id: i64) -> Result<()> {
        let snap = self.snapshot()?;
        let task = snap.task(id).ok_or("Task not found")?;
        if task.completed() {
            return Ok(());
        }
        if let Some(reason) = snap.blocked_reason(task) {
            return Err(reason.into());
        }
        let auto = task.due_on.is_none();
        let due = task.due_on.unwrap_or(snap.today);
        self.conn
            .execute(
                "UPDATE tasks SET completed_at = ?1, due_on = ?2, due_on_auto_set = ?3, updated_at = ?1 WHERE id = ?4",
                params![dates::now_stamp(), dates::iso(due), auto, id],
            )
            .map(|_| ())
            .map_err(err)
    }

    pub fn uncomplete_task(&self, id: i64) -> Result<()> {
        self.conn
            .execute(
                "UPDATE tasks SET completed_at = NULL,
                        due_on = CASE WHEN due_on_auto_set THEN NULL ELSE due_on END,
                        due_on_auto_set = 0, updated_at = ?1
                 WHERE id = ?2",
                params![dates::now_stamp(), id],
            )
            .map(|_| ())
            .map_err(err)
    }

    pub fn set_in_progress(&self, id: i64, on: bool) -> Result<()> {
        let stamp = on.then(dates::now_stamp);
        self.conn
            .execute("UPDATE tasks SET in_progress_at = ?1 WHERE id = ?2", params![stamp, id])
            .map(|_| ())
            .map_err(err)
    }

    /// After a task completes, undated dependents with no remaining open
    /// dependencies get today's date so they surface without searching.
    pub fn auto_date_unblocked_dependents(&self, id: i64) -> Result<usize> {
        let snap = self.snapshot()?;
        let Some(task) = snap.task(id) else { return Ok(0) };
        if !task.completed() {
            return Ok(0);
        }
        let mut n = 0;
        for dep in task.enables.iter().filter_map(|d| snap.task(*d)) {
            if !dep.completed() && dep.due_on.is_none() && !snap.open_needs(dep) {
                self.conn
                    .execute("UPDATE tasks SET due_on = ?1, due_on_auto_set = 1 WHERE id = ?2", params![dates::iso(snap.today), dep.id])
                    .map_err(err)?;
                n += 1;
            }
        }
        Ok(n)
    }

    pub fn reorder(&self, ids: &[i64]) -> Result<()> {
        let tx = self.conn.unchecked_transaction().map_err(err)?;
        for (pos, id) in ids.iter().enumerate() {
            tx.execute("UPDATE tasks SET position = ?1 WHERE id = ?2", params![pos as i64, id]).map_err(err)?;
        }
        tx.commit().map_err(err)
    }

    // ------------------------------------------------------------ import

    pub fn is_empty(&self) -> bool {
        let n: i64 = self
            .conn
            .query_row("SELECT (SELECT count(*) FROM tasks) + (SELECT count(*) FROM projects) + (SELECT count(*) FROM people)", [], |r| r.get(0))
            .unwrap_or(0);
        n == 0
    }

    /// Copies a Taskestra (Rails) database in, keeping ids. ActionText notes
    /// become plain text.
    pub fn import_rails(&self, path: &Path) -> Result<String> {
        if !self.is_empty() {
            return Err("The Taskestra database already has data; import only works into an empty one.".into());
        }
        if !path.exists() {
            return Err(format!("{} does not exist", path.display()));
        }
        let src = Connection::open_with_flags(path, rusqlite::OpenFlags::SQLITE_OPEN_READ_ONLY).map_err(err)?;
        let notes = |record_type: &str| -> Result<HashMap<i64, String>> {
            let mut stmt = src
                .prepare("SELECT record_id, body FROM action_text_rich_texts WHERE record_type = ?1 AND name = 'notes'")
                .map_err(err)?;
            let rows = stmt
                .query_map([record_type], |r| Ok((r.get::<_, i64>(0)?, r.get::<_, Option<String>>(1)?.unwrap_or_default())))
                .map_err(err)?;
            let mut out = HashMap::new();
            for row in rows {
                let (id, body) = row.map_err(err)?;
                out.insert(id, html_to_text(&body));
            }
            Ok(out)
        };
        let project_notes = notes("Project")?;
        let task_notes = notes("Task")?;
        let now = dates::now_stamp();

        let tx = self.conn.unchecked_transaction().map_err(err)?;
        let mut counts = (0, 0, 0);
        {
            let mut stmt = src.prepare("SELECT id, name, created_at FROM people").map_err(err)?;
            let rows = stmt.query_map([], |r| Ok((r.get::<_, i64>(0)?, r.get::<_, String>(1)?, r.get::<_, Option<String>>(2)?))).map_err(err)?;
            for row in rows {
                let (id, name, created) = row.map_err(err)?;
                let created = created.unwrap_or_else(|| now.clone());
                tx.execute("INSERT INTO people (id, name, created_at, updated_at) VALUES (?1, ?2, ?3, ?3)", params![id, name.trim(), created])
                    .map_err(err)?;
                counts.0 += 1;
            }
            let mut stmt = src.prepare("SELECT id, name, link, archived_at, created_at FROM projects").map_err(err)?;
            let rows = stmt
                .query_map([], |r| {
                    Ok((r.get::<_, i64>(0)?, r.get::<_, String>(1)?, r.get::<_, Option<String>>(2)?, r.get::<_, Option<String>>(3)?, r.get::<_, Option<String>>(4)?))
                })
                .map_err(err)?;
            for row in rows {
                let (id, name, link, archived, created) = row.map_err(err)?;
                let created = created.unwrap_or_else(|| now.clone());
                tx.execute(
                    "INSERT INTO projects (id, name, link, notes, archived_at, created_at, updated_at) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?6)",
                    params![id, name.trim(), link.unwrap_or_default(), project_notes.get(&id).cloned().unwrap_or_default(), archived, created],
                )
                .map_err(err)?;
                counts.1 += 1;
            }
            // Parents before children so the self-reference holds.
            let mut stmt = src
                .prepare(
                    "SELECT id, project_id, parent_task_id, what, due_on, due_on_auto_set, link, completed_at, in_progress_at, position, created_at
                     FROM tasks ORDER BY parent_task_id IS NOT NULL, id",
                )
                .map_err(err)?;
            let rows = stmt
                .query_map([], |r| {
                    Ok((
                        r.get::<_, i64>(0)?,
                        r.get::<_, i64>(1)?,
                        r.get::<_, Option<i64>>(2)?,
                        r.get::<_, Option<String>>(3)?,
                        r.get::<_, Option<String>>(4)?,
                        r.get::<_, Option<bool>>(5)?,
                        r.get::<_, Option<String>>(6)?,
                        r.get::<_, Option<String>>(7)?,
                        r.get::<_, Option<String>>(8)?,
                        r.get::<_, Option<i64>>(9)?,
                        r.get::<_, Option<String>>(10)?,
                    ))
                })
                .map_err(err)?;
            for row in rows {
                let (id, project, parent, what, due, auto, link, completed, progress, position, created) = row.map_err(err)?;
                let created = created.unwrap_or_else(|| now.clone());
                tx.execute(
                    "INSERT INTO tasks (id, project_id, parent_task_id, what, due_on, due_on_auto_set, link, notes, completed_at,
                                        in_progress_at, position, created_at, updated_at)
                     VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?12)",
                    params![
                        id,
                        project,
                        parent,
                        what.unwrap_or_default().trim(),
                        due.as_deref().and_then(dates::from_iso).map(dates::iso),
                        auto.unwrap_or(false),
                        link.unwrap_or_default(),
                        task_notes.get(&id).cloned().unwrap_or_default(),
                        completed,
                        progress,
                        position.unwrap_or(0),
                        created
                    ],
                )
                .map_err(err)?;
                counts.2 += 1;
            }
            for (table, cols) in [("task_assignments", "task_id, person_id"), ("task_dependencies", "task_id, depends_on_task_id")] {
                let mut stmt = src.prepare(&format!("SELECT {cols} FROM {table}")).map_err(err)?;
                let rows = stmt.query_map([], |r| Ok((r.get::<_, i64>(0)?, r.get::<_, i64>(1)?))).map_err(err)?;
                for row in rows {
                    let (a, b) = row.map_err(err)?;
                    tx.execute(&format!("INSERT OR IGNORE INTO {table} ({cols}) VALUES (?1, ?2)"), params![a, b]).map_err(err)?;
                }
            }
        }
        tx.commit().map_err(err)?;
        Ok(format!("Imported {} people, {} projects and {} tasks.", counts.0, counts.1, counts.2))
    }
}

/// ActionText HTML → readable plain text (lists become bullets).
pub fn html_to_text(html: &str) -> String {
    let mut out = String::new();
    let mut chars = html.chars().peekable();
    while let Some(c) = chars.next() {
        if c == '<' {
            let mut tag = String::new();
            for t in chars.by_ref() {
                if t == '>' {
                    break;
                }
                tag.push(t);
            }
            let name: String = tag.trim_start_matches('/').chars().take_while(|c| c.is_ascii_alphanumeric()).collect::<String>().to_lowercase();
            let closing = tag.starts_with('/');
            match (name.as_str(), closing) {
                ("br", _) => out.push('\n'),
                ("li", false) => {
                    if !out.is_empty() && !out.ends_with('\n') {
                        out.push('\n');
                    }
                    out.push_str("• ");
                }
                ("p" | "div" | "h1" | "h2" | "h3" | "h4" | "blockquote" | "pre" | "ul" | "ol" | "li", true)
                    if !out.ends_with('\n') => {
                        out.push('\n');
                    }
                _ => {}
            }
        } else if c == '&' {
            let mut entity = String::new();
            while let Some(&n) = chars.peek() {
                if n == ';' || entity.len() > 8 {
                    break;
                }
                entity.push(n);
                chars.next();
            }
            if chars.peek() == Some(&';') {
                chars.next();
            }
            out.push_str(match entity.as_str() {
                "amp" => "&",
                "lt" => "<",
                "gt" => ">",
                "quot" => "\"",
                "#39" | "apos" => "'",
                "nbsp" => " ",
                _ => "",
            });
        } else {
            out.push(c);
        }
    }
    let mut text = out.lines().map(str::trim_end).collect::<Vec<_>>().join("\n");
    while text.contains("\n\n\n") {
        text = text.replace("\n\n\n", "\n\n");
    }
    text.trim().to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn setup() -> (Db, i64, i64) {
        let db = Db::memory().unwrap();
        let project = db.create_project(&ProjectInput { name: "Alpha".into(), ..Default::default() }).unwrap();
        let ann = db.create_person("Ann").unwrap().id();
        (db, project, ann)
    }

    fn input(db_project: i64, what: &str) -> TaskInput {
        TaskInput { what: what.into(), project_id: Some(db_project), ..Default::default() }
    }

    #[test]
    fn validation_messages() {
        let (db, project, _) = setup();
        let e = db.create_task(TaskInput { what: "  ".into(), ..Default::default() }).unwrap_err();
        assert_eq!(e, "What can't be blank\nProject must be selected");
        let e = db.create_task(TaskInput { link: "ftp://x".into(), ..input(project, "x") }).unwrap_err();
        assert_eq!(e, "Link must be a valid URL (starting with http:// or https://)");
        assert_eq!(
            db.create_project(&ProjectInput { name: "alpha".into(), ..Default::default() }).unwrap_err(),
            "\"Alpha\" already exists as a project."
        );
        assert!(matches!(db.create_person(" ann ").unwrap(), Created::Existing(_, _)));
    }

    #[test]
    fn subtasks_rules() {
        let (db, project, _) = setup();
        let due = NaiveDate::from_ymd_opt(2026, 3, 5);
        let parent = db.create_task(TaskInput { due_on: due, ..input(project, "Parent") }).unwrap();
        let child = db.create_task(TaskInput { parent_id: Some(parent), ..input(project, "Child") }).unwrap();
        let second = db.create_task(TaskInput { parent_id: Some(parent), ..input(project, "Second") }).unwrap();
        let e = db.create_task(TaskInput { parent_id: Some(child), ..input(project, "Grandchild") }).unwrap_err();
        assert_eq!(e, "Sub-tasks cannot have sub-tasks");
        let e = db
            .create_task(TaskInput { parent_id: Some(parent), due_on: NaiveDate::from_ymd_opt(2026, 3, 4), ..input(project, "Early") })
            .unwrap_err();
        assert_eq!(e, "Due on cannot be before the parent task's due date (Mar 5, 2026)");

        let snap = db.snapshot().unwrap();
        assert_eq!(snap.task(parent).unwrap().subtasks, vec![child, second]);
        assert_eq!(db.complete_task(parent).unwrap_err(), "Complete all sub-tasks first!");

        db.reorder(&[second, child]).unwrap();
        assert_eq!(db.snapshot().unwrap().task(parent).unwrap().subtasks, vec![second, child]);
        db.delete_task(parent).unwrap();
        assert!(db.snapshot().unwrap().tasks.is_empty(), "sub-tasks go with their parent");
    }

    #[test]
    fn completion_auto_dates() {
        let (db, project, _) = setup();
        let blocker = db.create_task(input(project, "Blocker")).unwrap();
        let blocked = db.create_task(TaskInput { needs: vec![blocker], ..input(project, "Blocked") }).unwrap();
        assert_eq!(db.complete_task(blocked).unwrap_err(), "All dependencies must be completed first!");

        db.complete_task(blocker).unwrap();
        let snap = db.snapshot().unwrap();
        let t = snap.task(blocker).unwrap();
        assert_eq!(t.due_on, Some(snap.today));
        assert!(t.due_on_auto_set);

        assert_eq!(db.auto_date_unblocked_dependents(blocker).unwrap(), 1);
        assert_eq!(db.snapshot().unwrap().task(blocked).unwrap().due_on, Some(snap.today));

        db.uncomplete_task(blocker).unwrap();
        assert_eq!(db.snapshot().unwrap().task(blocker).unwrap().due_on, None, "auto date is cleared on reopen");
    }

    #[test]
    fn manual_date_edit_clears_auto_flag() {
        let (db, project, _) = setup();
        let id = db.create_task(input(project, "A")).unwrap();
        db.complete_task(id).unwrap();
        db.uncomplete_task(id).unwrap();
        db.complete_task(id).unwrap();
        db.update_task(id, TaskInput { due_on: NaiveDate::from_ymd_opt(2030, 1, 1), ..input(project, "A") }).unwrap();
        db.uncomplete_task(id).unwrap();
        assert_eq!(db.snapshot().unwrap().task(id).unwrap().due_on, NaiveDate::from_ymd_opt(2030, 1, 1));
    }

    #[test]
    fn archive_delete_and_people_guards() {
        let (db, project, ann) = setup();
        let id = db.create_task(TaskInput { people: vec![ann], ..input(project, "A") }).unwrap();
        assert_eq!(db.archive_project(project).unwrap_err(), "All tasks should be completed before a project can be archived.");
        assert_eq!(
            db.delete_project(project).unwrap_err(),
            "\"Alpha\" cannot be deleted because it has open tasks. Complete or delete all tasks first."
        );
        assert_eq!(db.delete_person(ann).unwrap_err(), "Unassign all tasks first!");
        db.complete_task(id).unwrap();
        db.archive_project(project).unwrap();
        assert!(db.snapshot().unwrap().project(project).unwrap().archived());
        db.delete_person(ann).unwrap();
        db.delete_project(project).unwrap();
        assert!(db.is_empty());
    }

    #[test]
    fn html_notes_become_text() {
        let html = "<div class=\"trix-content\"><div>Call <strong>Bob</strong> &amp; Ann<br>later</div><ul><li>one</li><li>two</li></ul></div>";
        assert_eq!(html_to_text(html), "Call Bob & Ann\nlater\n• one\n• two");
    }
}
