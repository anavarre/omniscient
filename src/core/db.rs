//! SQLite persistence and every write rule (validation messages, blocking,
//! auto-dating). Messages match the web app's wording.

use std::collections::HashMap;
use std::path::{Path, PathBuf};

use chrono::NaiveDate;
use rusqlite::{Connection, OptionalExtension, params};

use crate::core::dates;
use crate::core::model::{Person, Project, Snapshot, Task};

pub type Result<T> = std::result::Result<T, String>;

fn err(e: rusqlite::Error) -> String {
    e.to_string()
}

/// Schema history, applied in order against `PRAGMA user_version`. Never
/// edit a shipped entry; append a new one.
const MIGRATIONS: &[&str] = &["
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
"];

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
    /// The file behind `conn` (None for in-memory test databases).
    path: Option<PathBuf>,
    /// Rows changed by settings writes, which never affect the snapshot.
    setting_changes: std::cell::Cell<u64>,
}

pub fn default_path() -> PathBuf {
    crate::core::location::default_db()
}

fn sidecar(db: &Path, suffix: &str) -> PathBuf {
    let mut s = db.as_os_str().to_owned();
    s.push(suffix);
    PathBuf::from(s)
}

/// Deletes a database and its WAL/shm files (a half-made destination).
pub fn remove_files(db: &Path) {
    for p in [db.to_path_buf(), sidecar(db, "-wal"), sidecar(db, "-shm")] {
        let _ = std::fs::remove_file(p);
    }
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
        Self::init(conn, Some(path.to_path_buf()))
    }

    pub fn memory() -> Result<Db> {
        Self::init(Connection::open_in_memory().map_err(err)?, None)
    }

    pub fn path(&self) -> Option<&Path> {
        self.path.as_deref()
    }

    fn init(conn: Connection, path: Option<PathBuf>) -> Result<Db> {
        conn.execute_batch("PRAGMA journal_mode = WAL; PRAGMA foreign_keys = ON; PRAGMA synchronous = NORMAL;")
            .map_err(err)?;
        let db = Db { conn, path, setting_changes: std::cell::Cell::new(0) };
        db.migrate()?;
        Ok(db)
    }

    fn migrate(&self) -> Result<()> {
        let version: i64 = self.conn.query_row("PRAGMA user_version", [], |r| r.get(0)).map_err(err)?;
        let version = usize::try_from(version).unwrap_or(0);
        if version > MIGRATIONS.len() {
            return Err(format!("This database was written by a newer Omniscient (schema {version})."));
        }
        for (i, sql) in MIGRATIONS.iter().enumerate().skip(version) {
            let tx = self.conn.unchecked_transaction().map_err(err)?;
            tx.execute_batch(sql).map_err(err)?;
            tx.pragma_update(None, "user_version", (i + 1) as i64).map_err(err)?;
            tx.commit().map_err(err)?;
        }
        Ok(())
    }

    // ------------------------------------------------------------ reading

    pub fn snapshot(&self) -> Result<Snapshot> {
        let c = &self.conn;
        let mut tasks: HashMap<i64, Task> = HashMap::with_capacity(1024);
        let mut stmt = c
            .prepare_cached(
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
            .prepare_cached("SELECT id, name FROM people ORDER BY name COLLATE NOCASE, id")
            .map_err(err)?
            .query_map([], |r| Ok(Person { id: r.get(0)?, name: r.get(1)?, open_tasks: 0 }))
            .map_err(err)?
            .collect::<std::result::Result<_, _>>()
            .map_err(err)?;

        let mut assign = c.prepare_cached("SELECT task_id, person_id FROM task_assignments").map_err(err)?;
        for row in assign.query_map([], |r| Ok((r.get::<_, i64>(0)?, r.get::<_, i64>(1)?))).map_err(err)? {
            let (task, person) = row.map_err(err)?;
            if let Some(t) = tasks.get_mut(&task) {
                t.people.push(person);
            }
        }
        let mut deps = c.prepare_cached("SELECT task_id, depends_on_task_id FROM task_dependencies").map_err(err)?;
        let pairs: Vec<(i64, i64)> = deps
            .query_map([], |r| Ok((r.get(0)?, r.get(1)?)))
            .map_err(err)?
            .collect::<std::result::Result<_, _>>()
            .map_err(err)?;
        for &(task, dep) in &pairs {
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

        // Keep chips and assignee lists in a stable, readable order. Only
        // tasks that appear as chips need their titles folded.
        let names: HashMap<i64, String> = people.iter().map(|p| (p.id, p.name.to_lowercase())).collect();
        let titles: HashMap<i64, String> =
            pairs.iter().flat_map(|(a, b)| [*a, *b]).filter_map(|id| tasks.get(&id).map(|t| (id, t.what.to_lowercase()))).collect();
        let mut open_by_person: HashMap<i64, usize> = HashMap::new();
        for t in tasks.values_mut() {
            if t.people.len() > 1 {
                t.people.sort_by(|a, b| names.get(a).cmp(&names.get(b)));
            }
            if t.needs.len() > 1 {
                t.needs.sort_by(|a, b| titles.get(a).cmp(&titles.get(b)));
            }
            if t.enables.len() > 1 {
                t.enables.sort_by(|a, b| titles.get(a).cmp(&titles.get(b)));
            }
            if t.completed_at.is_none() {
                for p in &t.people {
                    *open_by_person.entry(*p).or_default() += 1;
                }
            }
        }
        for p in &mut people {
            p.open_tasks = open_by_person.get(&p.id).copied().unwrap_or(0);
        }

        let mut open_by_project: HashMap<i64, usize> = HashMap::new();
        for t in tasks.values().filter(|t| t.completed_at.is_none()) {
            *open_by_project.entry(t.project_id).or_default() += 1;
        }
        let projects: Vec<Project> = c
            .prepare_cached("SELECT id, name, link, notes, archived_at FROM projects ORDER BY name COLLATE NOCASE, id")
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
                p.open_tasks = open_by_project.get(&p.id).copied().unwrap_or(0);
                p
            })
            .collect();

        Ok(Snapshot { today: dates::today(), people, projects, tasks })
    }

    pub fn setting(&self, key: &str) -> Option<String> {
        self.conn.query_row("SELECT value FROM settings WHERE key = ?1", [key], |r| r.get(0)).optional().ok().flatten()
    }

    /// Rows changed so far by anything that the snapshot reflects: lets the
    /// controller skip re-reading after actions that wrote nothing.
    pub fn data_changes(&self) -> u64 {
        self.conn.total_changes() - self.setting_changes.get()
    }

    pub fn set_setting(&self, key: &str, value: Option<&str>) -> Result<()> {
        let before = self.conn.total_changes();
        let out = match value {
            Some(v) => self.conn.execute(
                "INSERT INTO settings (key, value) VALUES (?1, ?2) ON CONFLICT (key) DO UPDATE SET value = ?2",
                params![key, v],
            ),
            None => self.conn.execute("DELETE FROM settings WHERE key = ?1", [key]),
        }
        .map(|_| ())
        .map_err(err);
        self.setting_changes.set(self.setting_changes.get() + (self.conn.total_changes() - before));
        out
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

    fn open_children(&self, id: i64) -> Result<(bool, bool)> {
        self.conn
            .query_row(
                "SELECT EXISTS (SELECT 1 FROM task_dependencies d JOIN tasks t ON t.id = d.depends_on_task_id
                                WHERE d.task_id = ?1 AND t.completed_at IS NULL),
                        EXISTS (SELECT 1 FROM tasks WHERE parent_task_id = ?1 AND completed_at IS NULL)",
                [id],
                |r| Ok((r.get(0)?, r.get(1)?)),
            )
            .map_err(err)
    }

    pub fn complete_task(&self, id: i64) -> Result<()> {
        let (completed, due): (bool, Option<String>) = self
            .conn
            .query_row("SELECT completed_at IS NOT NULL, due_on FROM tasks WHERE id = ?1", [id], |r| Ok((r.get(0)?, r.get(1)?)))
            .optional()
            .map_err(err)?
            .ok_or("Task not found")?;
        if completed {
            return Ok(());
        }
        if let Some(reason) = crate::core::model::blocked_message(self.open_children(id)?) {
            return Err(reason.into());
        }
        let due = due.as_deref().and_then(dates::from_iso);
        let auto = due.is_none();
        let due = due.unwrap_or_else(dates::today);
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
        self.conn
            .execute(
                "UPDATE tasks SET due_on = ?2, due_on_auto_set = 1
                 WHERE id IN (SELECT task_id FROM task_dependencies WHERE depends_on_task_id = ?1)
                   AND EXISTS (SELECT 1 FROM tasks WHERE id = ?1 AND completed_at IS NOT NULL)
                   AND completed_at IS NULL AND due_on IS NULL
                   AND NOT EXISTS (SELECT 1 FROM task_dependencies d JOIN tasks b ON b.id = d.depends_on_task_id
                                   WHERE d.task_id = tasks.id AND b.completed_at IS NULL)",
                params![id, dates::iso(dates::today())],
            )
            .map_err(err)
    }

    pub fn reorder(&self, ids: &[i64]) -> Result<()> {
        let tx = self.conn.unchecked_transaction().map_err(err)?;
        for (pos, id) in ids.iter().enumerate() {
            tx.execute("UPDATE tasks SET position = ?1 WHERE id = ?2", params![pos as i64, id]).map_err(err)?;
        }
        tx.commit().map_err(err)
    }

    // ------------------------------------------------------------ relocation

    /// Copies the whole database to `dest` and returns it opened there. The
    /// original is never touched, so any failure leaves everything as it was.
    ///
    /// `VACUUM INTO` writes a consistent snapshot (WAL included) to a
    /// temporary file beside `dest`; that copy is verified (integrity, foreign
    /// keys, schema version, row count of every table) and only then linked
    /// into place without overwriting anything.
    pub fn copy_to(&self, dest: &Path) -> Result<Db> {
        let sidecars = ["-wal", "-shm", "-journal"].map(|s| sidecar(dest, s));
        if std::iter::once(dest).chain(sidecars.iter().map(PathBuf::as_path)).any(|p| p.symlink_metadata().is_ok()) {
            return Err(format!("{} already exists; choose a new file name.", dest.display()));
        }
        let dir = dest.parent().ok_or("Bad destination")?;
        std::fs::create_dir_all(dir).map_err(|e| format!("Cannot create {}: {e}", dir.display()))?;
        let name = dest.file_name().ok_or("Bad destination")?.to_string_lossy();
        let tmp = dir.join(format!(".{name}.moving-{}", std::process::id()));
        let _ = std::fs::remove_file(&tmp);

        let staged = self.stage_copy(&tmp);
        let linked = staged.and_then(|()| {
            // hard_link refuses to replace a file that appeared meanwhile.
            std::fs::hard_link(&tmp, dest).map_err(|e| format!("Cannot place {}: {e}", dest.display()))
        });
        let _ = std::fs::remove_file(&tmp);
        linked?;
        if let Ok(d) = std::fs::File::open(dir) {
            let _ = d.sync_all();
        }
        Db::open(dest).inspect_err(|_| remove_files(dest))
    }

    fn stage_copy(&self, tmp: &Path) -> Result<()> {
        let tmp_str = tmp.to_str().ok_or("The path must be valid UTF-8")?;
        self.conn.execute("VACUUM INTO ?1", [tmp_str]).map_err(|e| format!("Copy failed: {e}"))?;
        let check = Connection::open_with_flags(tmp, rusqlite::OpenFlags::SQLITE_OPEN_READ_ONLY).map_err(err)?;
        let one = |sql: &str| check.query_row(sql, [], |r| r.get::<_, String>(0)).map_err(err);
        if one("PRAGMA integrity_check")? != "ok" {
            return Err("The copy failed its integrity check; nothing was changed.".into());
        }
        if check.prepare("PRAGMA foreign_key_check").and_then(|mut s| s.exists([])).map_err(err)? {
            return Err("The copy failed its foreign-key check; nothing was changed.".into());
        }
        let version = |c: &Connection| c.query_row("PRAGMA user_version", [], |r| r.get::<_, i64>(0)).map_err(err);
        if version(&check)? != version(&self.conn)? {
            return Err("The copy has a different schema version; nothing was changed.".into());
        }
        let counts = |c: &Connection| -> Result<Vec<(String, i64)>> {
            let tables: Vec<String> = c
                .prepare("SELECT name FROM sqlite_master WHERE type = 'table' AND name NOT LIKE 'sqlite_%' ORDER BY name")
                .and_then(|mut s| s.query_map([], |r| r.get(0))?.collect())
                .map_err(err)?;
            tables
                .into_iter()
                .map(|t| {
                    let n = c.query_row(&format!("SELECT count(*) FROM \"{}\"", t.replace('"', "\"\"")), [], |r| r.get(0)).map_err(err)?;
                    Ok((t, n))
                })
                .collect()
        };
        if counts(&check)? != counts(&self.conn)? {
            return Err("The copy doesn't match the original; nothing was changed.".into());
        }
        drop(check);
        std::fs::File::open(tmp).and_then(|f| f.sync_all()).map_err(|e| format!("Cannot flush the copy: {e}"))
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
    /// become Markdown. Rows that can't be carried over (blank names, tasks
    /// whose project is gone) are skipped and reported; names that differ
    /// only by case are merged into the first one.
    pub fn import_rails(&self, path: &Path) -> Result<String> {
        if !self.is_empty() {
            return Err("The database already has data; import only works into an empty one.".into());
        }
        if !path.exists() {
            return Err(format!("{} does not exist", path.display()));
        }
        let src = Connection::open_with_flags(path, rusqlite::OpenFlags::SQLITE_OPEN_READ_ONLY).map_err(err)?;
        let has_rich_text: bool = src
            .query_row("SELECT EXISTS (SELECT 1 FROM sqlite_master WHERE name = 'action_text_rich_texts')", [], |r| r.get(0))
            .map_err(err)?;
        let notes = |record_type: &str| -> Result<HashMap<i64, String>> {
            let mut out = HashMap::new();
            if !has_rich_text {
                return Ok(out);
            }
            let mut stmt = src
                .prepare("SELECT record_id, body FROM action_text_rich_texts WHERE record_type = ?1 AND name = 'notes'")
                .map_err(err)?;
            let rows = stmt
                .query_map([record_type], |r| Ok((r.get::<_, i64>(0)?, r.get::<_, Option<String>>(1)?.unwrap_or_default())))
                .map_err(err)?;
            for row in rows {
                let (id, body) = row.map_err(err)?;
                out.insert(id, html_to_markdown(&body));
            }
            Ok(out)
        };
        let project_notes = notes("Project")?;
        let task_notes = notes("Task")?;
        let now = dates::now_stamp();
        let mut skipped: Vec<String> = vec![];

        let tx = self.conn.unchecked_transaction().map_err(err)?;
        // Source id → id kept here (differs only for merged duplicates).
        let mut people: HashMap<i64, i64> = HashMap::new();
        let mut projects: HashMap<i64, i64> = HashMap::new();
        let mut tasks: HashMap<i64, ()> = HashMap::new();
        {
            let mut by_name: HashMap<String, i64> = HashMap::new();
            let mut stmt = src.prepare("SELECT id, name, created_at FROM people ORDER BY id").map_err(err)?;
            let rows = stmt
                .query_map([], |r| Ok((r.get::<_, i64>(0)?, r.get::<_, Option<String>>(1)?, r.get::<_, Option<String>>(2)?)))
                .map_err(err)?;
            for row in rows {
                let (id, name, created) = row.map_err(err)?;
                let name = name.unwrap_or_default().trim().to_string();
                if name.is_empty() {
                    skipped.push(format!("person #{id} (no name)"));
                    continue;
                }
                if let Some(&first) = by_name.get(&name.to_lowercase()) {
                    people.insert(id, first);
                    continue;
                }
                let created = created.unwrap_or_else(|| now.clone());
                tx.execute("INSERT INTO people (id, name, created_at, updated_at) VALUES (?1, ?2, ?3, ?3)", params![id, name, created])
                    .map_err(err)?;
                by_name.insert(name.to_lowercase(), id);
                people.insert(id, id);
            }

            let mut by_name: HashMap<String, i64> = HashMap::new();
            let mut stmt = src.prepare("SELECT id, name, link, archived_at, created_at FROM projects ORDER BY id").map_err(err)?;
            let rows = stmt
                .query_map([], |r| {
                    Ok((
                        r.get::<_, i64>(0)?,
                        r.get::<_, Option<String>>(1)?,
                        r.get::<_, Option<String>>(2)?,
                        r.get::<_, Option<String>>(3)?,
                        r.get::<_, Option<String>>(4)?,
                    ))
                })
                .map_err(err)?;
            for row in rows {
                let (id, name, link, archived, created) = row.map_err(err)?;
                let name = name.unwrap_or_default().trim().to_string();
                if name.is_empty() {
                    skipped.push(format!("project #{id} (no name)"));
                    continue;
                }
                if let Some(&first) = by_name.get(&name.to_lowercase()) {
                    projects.insert(id, first);
                    continue;
                }
                let created = created.unwrap_or_else(|| now.clone());
                tx.execute(
                    "INSERT INTO projects (id, name, link, notes, archived_at, created_at, updated_at) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?6)",
                    params![id, name, link.unwrap_or_default().trim(), project_notes.get(&id).cloned().unwrap_or_default(), archived, created],
                )
                .map_err(err)?;
                by_name.insert(name.to_lowercase(), id);
                projects.insert(id, id);
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
                        r.get::<_, Option<i64>>(1)?,
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
                let Some(&project) = project.and_then(|p| projects.get(&p)) else {
                    skipped.push(format!("task #{id} (its project is missing)"));
                    continue;
                };
                if parent.is_some_and(|p| !tasks.contains_key(&p)) {
                    skipped.push(format!("task #{id} (its parent task is missing)"));
                    continue;
                }
                let what = what.unwrap_or_default().trim().to_string();
                if what.is_empty() {
                    skipped.push(format!("task #{id} (no title)"));
                    continue;
                }
                let created = created.unwrap_or_else(|| now.clone());
                tx.execute(
                    "INSERT INTO tasks (id, project_id, parent_task_id, what, due_on, due_on_auto_set, link, notes, completed_at,
                                        in_progress_at, position, created_at, updated_at)
                     VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?12)",
                    params![
                        id,
                        project,
                        parent,
                        what,
                        due.as_deref().and_then(dates::from_iso).map(dates::iso),
                        auto.unwrap_or(false),
                        link.unwrap_or_default().trim(),
                        task_notes.get(&id).cloned().unwrap_or_default(),
                        completed,
                        progress,
                        position.unwrap_or(0),
                        created
                    ],
                )
                .map_err(err)?;
                tasks.insert(id, ());
            }
            // A sub-task lives in its parent's project.
            tx.execute(
                "UPDATE tasks SET project_id = (SELECT p.project_id FROM tasks p WHERE p.id = tasks.parent_task_id)
                 WHERE parent_task_id IS NOT NULL",
                [],
            )
            .map_err(err)?;

            let mut stmt = src.prepare("SELECT task_id, person_id FROM task_assignments").map_err(err)?;
            for row in stmt.query_map([], |r| Ok((r.get::<_, i64>(0)?, r.get::<_, i64>(1)?))).map_err(err)? {
                let (task, person) = row.map_err(err)?;
                if let (true, Some(person)) = (tasks.contains_key(&task), people.get(&person)) {
                    tx.execute("INSERT OR IGNORE INTO task_assignments (task_id, person_id) VALUES (?1, ?2)", params![task, person])
                        .map_err(err)?;
                }
            }
            let mut stmt = src.prepare("SELECT task_id, depends_on_task_id FROM task_dependencies").map_err(err)?;
            for row in stmt.query_map([], |r| Ok((r.get::<_, i64>(0)?, r.get::<_, i64>(1)?))).map_err(err)? {
                let (task, dep) = row.map_err(err)?;
                if task != dep && tasks.contains_key(&task) && tasks.contains_key(&dep) {
                    tx.execute("INSERT OR IGNORE INTO task_dependencies (task_id, depends_on_task_id) VALUES (?1, ?2)", params![task, dep])
                        .map_err(err)?;
                }
            }
        }
        tx.commit().map_err(err)?;
        let distinct = |m: &HashMap<i64, i64>| m.iter().filter(|(k, v)| k == v).count();
        let mut summary =
            format!("Imported {} people, {} projects and {} tasks.", distinct(&people), distinct(&projects), tasks.len());
        if !skipped.is_empty() {
            summary.push_str(&format!("\nSkipped {}: {}", skipped.len(), skipped.join(", ")));
        }
        Ok(summary)
    }
}

/// ActionText/Lexxy HTML → Markdown: emphasis, links, lists, headings,
/// quotes and code survive; attachments become their caption.
pub fn html_to_markdown(html: &str) -> String {
    let mut out = String::new();
    // Open lists: Some(n) numbered, None bulleted.
    let mut lists: Vec<Option<usize>> = vec![];
    let mut links: Vec<String> = vec![];
    let mut pre = false;
    let newline = |out: &mut String| {
        if !out.is_empty() && !out.ends_with('\n') {
            out.push('\n');
        }
    };
    let blank_line = |out: &mut String| {
        if !out.is_empty() {
            if !out.ends_with('\n') {
                out.push('\n');
            }
            if !out.ends_with("\n\n") {
                out.push('\n');
            }
        }
    };
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
            let closing = tag.starts_with('/');
            let name: String =
                tag.trim_start_matches('/').chars().take_while(|c| c.is_ascii_alphanumeric() || *c == '-').collect::<String>().to_lowercase();
            let attr = |key: &str| -> Option<String> {
                let at = tag.find(&format!("{key}=\""))? + key.len() + 2;
                Some(tag[at..].split('"').next()?.to_string())
            };
            match (name.as_str(), closing) {
                ("br", _) => out.push('\n'),
                ("strong" | "b", _) => out.push_str("**"),
                ("em" | "i", _) => out.push('*'),
                ("s" | "del" | "strike", _) => out.push_str("~~"),
                ("code", _) if !pre => out.push('`'),
                ("a", false) => {
                    links.push(attr("href").unwrap_or_default());
                    out.push('[');
                }
                ("a", true) => {
                    let href = links.pop().unwrap_or_default();
                    out.push_str(&format!("]({href})"));
                }
                ("ul", false) => {
                    if lists.is_empty() { blank_line(&mut out) } else { newline(&mut out) }
                    lists.push(None);
                }
                ("ol", false) => {
                    if lists.is_empty() { blank_line(&mut out) } else { newline(&mut out) }
                    lists.push(Some(0));
                }
                ("ul" | "ol", true) => {
                    lists.pop();
                    if lists.is_empty() {
                        blank_line(&mut out);
                    }
                }
                ("li", false) => {
                    newline(&mut out);
                    let depth = lists.len().saturating_sub(1);
                    out.push_str(&"  ".repeat(depth));
                    match lists.last_mut() {
                        Some(Some(n)) => {
                            *n += 1;
                            out.push_str(&format!("{n}. "));
                        }
                        _ => out.push_str("- "),
                    }
                }
                ("h1" | "h2" | "h3" | "h4" | "h5" | "h6", false) => {
                    blank_line(&mut out);
                    let level = name[1..].parse::<usize>().unwrap_or(1).min(3);
                    out.push_str(&format!("{} ", "#".repeat(level)));
                }
                ("blockquote", false) => {
                    blank_line(&mut out);
                    out.push_str("> ");
                }
                ("pre", false) => {
                    blank_line(&mut out);
                    out.push_str("```\n");
                    pre = true;
                }
                ("pre", true) => {
                    newline(&mut out);
                    out.push_str("```");
                    pre = false;
                    blank_line(&mut out);
                }
                ("action-text-attachment", false) => {
                    if let Some(caption) = attr("caption").or_else(|| attr("filename")).filter(|c| !c.is_empty()) {
                        out.push_str(&format!("[{caption}]"));
                    }
                }
                ("p" | "div" | "h1" | "h2" | "h3" | "h4" | "h5" | "h6" | "blockquote", true) => blank_line(&mut out),
                ("li", true) => {}
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
        } else if c == '\n' && !pre {
            // Source newlines are layout, not content.
            if !out.ends_with([' ', '\n']) {
                out.push(' ');
            }
        } else {
            out.push(c);
        }
    }
    let mut text = out.lines().map(str::trim_end).collect::<Vec<_>>().join("\n");
    while text.contains("\n\n\n") {
        text = text.replace("\n\n\n", "\n\n");
    }
    text.replace("****", "").trim().to_string()
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
    fn html_notes_become_markdown() {
        let html = "<div class=\"trix-content\"><div>Call <strong>Bob</strong> &amp; Ann<br>later</div><ul><li>one</li><li>two</li></ul></div>";
        assert_eq!(html_to_markdown(html), "Call **Bob** & Ann\nlater\n\n- one\n- two");
        let html = "<p>See <a href=\"https://x.io\">the doc</a></p><ol><li>a</li><li>b<ul><li>c</li></ul></li></ol><h2>Next</h2><p><em>soon</em></p>";
        assert_eq!(html_to_markdown(html), "See [the doc](https://x.io)\n\n1. a\n2. b\n  - c\n\n## Next\n\n*soon*");
        assert_eq!(html_to_markdown("<action-text-attachment caption=\"plan.pdf\"></action-text-attachment>"), "[plan.pdf]");
    }

    // Behaviour specs carried over from the Rails app's model tests
    // (taskestra/test/models/*_test.rb).

    #[test]
    fn auto_dating_only_dates_fully_unblocked_undated_open_dependents() {
        let (db, project, _) = setup();
        let a = db.create_task(input(project, "A")).unwrap();
        let b = db.create_task(input(project, "B")).unwrap();
        let both = db.create_task(TaskInput { needs: vec![a, b], ..input(project, "Needs A and B") }).unwrap();
        let dated = db
            .create_task(TaskInput { needs: vec![a], due_on: NaiveDate::from_ymd_opt(2030, 1, 1), ..input(project, "Already dated") })
            .unwrap();
        let done = db.create_task(TaskInput { needs: vec![a], ..input(project, "Done dependent") }).unwrap();
        db.complete_task(a).unwrap();
        db.complete_task(done).unwrap();
        db.uncomplete_task(done).unwrap();
        db.complete_task(done).unwrap();
        let done_due = db.snapshot().unwrap().task(done).unwrap().due_on;

        assert_eq!(db.auto_date_unblocked_dependents(a).unwrap(), 0);
        let snap = db.snapshot().unwrap();
        assert_eq!(snap.task(both).unwrap().due_on, None, "B still blocks it");
        assert_eq!(snap.task(dated).unwrap().due_on, NaiveDate::from_ymd_opt(2030, 1, 1), "existing dates are kept");
        assert_eq!(snap.task(done).unwrap().due_on, done_due, "completed dependents are left alone");

        db.complete_task(b).unwrap();
        assert_eq!(db.auto_date_unblocked_dependents(b).unwrap(), 1, "the last blocker dates it");
        let t = db.snapshot().unwrap().task(both).cloned().unwrap();
        assert_eq!(t.due_on, Some(dates::today()));
        assert!(t.due_on_auto_set);
        assert_eq!(db.auto_date_unblocked_dependents(both).unwrap(), 0, "an open task dates nothing");
    }

    #[test]
    fn completion_keeps_an_existing_date_and_is_idempotent() {
        let (db, project, _) = setup();
        let due = NaiveDate::from_ymd_opt(2026, 1, 15);
        let id = db.create_task(TaskInput { due_on: due, ..input(project, "Dated") }).unwrap();
        db.complete_task(id).unwrap();
        let first = db.snapshot().unwrap().task(id).cloned().unwrap();
        assert_eq!(first.due_on, due);
        assert!(!first.due_on_auto_set);
        db.complete_task(id).unwrap();
        assert_eq!(db.snapshot().unwrap().task(id).unwrap().completed_at, first.completed_at);
        db.uncomplete_task(id).unwrap();
        assert_eq!(db.snapshot().unwrap().task(id).unwrap().due_on, due, "only auto dates are cleared");
    }

    #[test]
    fn subtask_dates_relative_to_the_parent() {
        let (db, project, _) = setup();
        let undated = db.create_task(input(project, "Undated parent")).unwrap();
        db.create_task(TaskInput { parent_id: Some(undated), due_on: NaiveDate::from_ymd_opt(2020, 1, 1), ..input(project, "Any") })
            .expect("a parent without a date allows any");
        let parent = db.create_task(TaskInput { due_on: NaiveDate::from_ymd_opt(2026, 5, 1), ..input(project, "Parent") }).unwrap();
        db.create_task(TaskInput { parent_id: Some(parent), due_on: NaiveDate::from_ymd_opt(2026, 5, 1), ..input(project, "Same day") })
            .expect("same day as the parent is fine");
        let child = db.create_task(TaskInput { parent_id: Some(parent), ..input(project, "No date") }).unwrap();
        // Completing a sub-task with no date gives it today, which may
        // precede its parent: still allowed.
        db.complete_task(child).unwrap();
        db.uncomplete_task(child).unwrap();
        let snap = db.snapshot().unwrap();
        assert_eq!(snap.task(parent).unwrap().subtasks.len(), 2);
        let positions: Vec<i64> = snap.task(parent).unwrap().subtasks.iter().map(|id| snap.task(*id).unwrap().position).collect();
        assert_eq!(positions, [0, 1], "sub-tasks are numbered in creation order");
        assert_eq!(snap.task(parent).unwrap().position, 0);
    }

    #[test]
    fn search_matches_titles_and_notes_case_insensitively() {
        let (db, project, _) = setup();
        let a = db.create_task(TaskInput { notes: "Call the **Vendor**".into(), ..input(project, "Budget") }).unwrap();
        let b = db.create_task(input(project, "vendor contract")).unwrap();
        db.create_task(input(project, "Unrelated")).unwrap();
        let snap = db.snapshot().unwrap();
        assert_eq!(snap.search("VENDOR", &[]), vec![b, a], "title hits first, then notes");
        assert!(snap.search("nothing like it", &[]).is_empty());
        assert_eq!(snap.search("vendor", &[b]), vec![a]);
    }

    #[test]
    fn names_and_links_are_trimmed_and_unique_ignoring_case() {
        let (db, project, ann) = setup();
        db.update_project(project, &ProjectInput { name: "  Alpha  ".into(), link: " https://a.io ".into(), notes: String::new() })
            .unwrap();
        let snap = db.snapshot().unwrap();
        assert_eq!(snap.project(project).unwrap().name, "Alpha");
        assert_eq!(snap.project(project).unwrap().link, "https://a.io");
        assert!(matches!(db.find_or_create_project(" ALPHA ").unwrap(), Created::Existing(id, _) if id == project));
        let bob = db.create_person("Bob").unwrap().id();
        assert_eq!(db.rename_person(bob, "ANN").unwrap_err(), "Name has already been taken");
        db.rename_person(ann, " Annie ").unwrap();
        assert_eq!(db.snapshot().unwrap().person(ann).unwrap().name, "Annie");
        assert_eq!(db.create_task(TaskInput { link: "https://has space.io".into(), ..input(project, "x") }).unwrap_err(),
            "Link must be a valid URL (starting with http:// or https://)");
    }

    #[test]
    fn migrations_are_versioned_and_reopen_cleanly() {
        let dir = std::env::temp_dir().join(format!("omniscient-test-{}", std::process::id()));
        let path = dir.join("db.sqlite3");
        {
            let db = Db::open(&path).unwrap();
            db.create_person("Ann").unwrap();
        }
        let db = Db::open(&path).unwrap();
        let v: i64 = db.conn.query_row("PRAGMA user_version", [], |r| r.get(0)).unwrap();
        assert_eq!(v as usize, MIGRATIONS.len());
        assert_eq!(db.snapshot().unwrap().people.len(), 1);
        db.conn.pragma_update(None, "user_version", 99i64).unwrap();
        drop(db);
        assert!(Db::open(&path).err().unwrap().contains("newer Omniscient"));
        std::fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn copy_to_is_verified_never_overwrites_and_cleans_up() {
        let dir = std::env::temp_dir().join(format!("omniscient-copy-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        let src = Db::open(&dir.join("a/src.sqlite3")).unwrap();
        let p = src.create_project(&ProjectInput { name: "P".into(), link: String::new(), notes: String::new() }).unwrap();
        src.create_person("Ann").unwrap();
        src.create_task(input(p, "one")).unwrap();
        src.set_setting("grouping", Some("project")).unwrap();

        let dest = dir.join("b/c/dest.sqlite3");
        let moved = src.copy_to(&dest).unwrap();
        let (a, b) = (src.snapshot().unwrap(), moved.snapshot().unwrap());
        assert_eq!((a.tasks.len(), a.people.len(), a.projects.len()), (b.tasks.len(), b.people.len(), b.projects.len()));
        assert_eq!(moved.setting("grouping").as_deref(), Some("project"));
        assert_eq!(moved.path(), Some(dest.as_path()));
        // Source untouched; no temp files left beside the destination.
        assert_eq!(src.snapshot().unwrap().tasks.len(), 1);
        let leftovers: Vec<_> = std::fs::read_dir(dest.parent().unwrap()).unwrap().map(|e| e.unwrap().file_name().to_string_lossy().into_owned())
            .filter(|n| n.contains("moving")).collect();
        assert!(leftovers.is_empty(), "{leftovers:?}");

        // An existing file (or stale WAL) is never replaced.
        assert!(src.copy_to(&dest).err().unwrap().contains("already exists"));
        std::fs::write(dir.join("stale.sqlite3-wal"), b"x").unwrap();
        assert!(src.copy_to(&dir.join("stale.sqlite3")).err().unwrap().contains("already exists"));
        // An unwritable destination fails without touching the source.
        assert!(src.copy_to(Path::new("/proc/nope/x.sqlite3")).is_err());
        assert_eq!(src.snapshot().unwrap().tasks.len(), 1);
        // The in-memory database can be saved to disk too.
        assert!(Db::memory().unwrap().copy_to(&dir.join("mem.sqlite3")).is_ok());
        std::fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn importing_the_rails_fixture() {
        let dir = std::env::temp_dir().join(format!("omniscient-import-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let rails = dir.join("rails.sqlite3");
        let src = Connection::open(&rails).unwrap();
        src.execute_batch(include_str!("../../tests/rails_fixture.sql")).unwrap();
        // Hazards the importer must survive.
        src.execute_batch(
            "INSERT INTO people (id, name, created_at, updated_at) VALUES (90, NULL, '2026-01-01', '2026-01-01');
             INSERT INTO tasks (id, project_id, what, created_at, updated_at, position) VALUES (900, 999, 'Orphan', '2026-01-01', '2026-01-01', 0);",
        )
        .unwrap();
        drop(src);
        let db = Db::memory().unwrap();
        let summary = db.import_rails(&rails).unwrap();
        assert!(summary.starts_with("Imported 4 people, 4 projects and 13 tasks."), "{summary}");
        assert!(summary.contains("person #90") && summary.contains("task #900"), "{summary}");
        let snap = db.snapshot().unwrap();
        assert_eq!(
            snap.project(1).unwrap().notes,
            "Goal: ship the **new homepage** before the launch.\nStakeholders: marketing & sales.",
            "notes keep their formatting as Markdown"
        );
        assert!(db.import_rails(&rails).is_err(), "only into an empty database");
        std::fs::remove_dir_all(dir).unwrap();
    }
}

#[cfg(test)]
mod perf {
    /// `OMNISCIENT_BENCH_DB=… cargo test --release bench_snapshot -- --ignored --nocapture`
    #[test]
    #[ignore]
    fn bench_snapshot() {
        let Some(path) = std::env::var_os("OMNISCIENT_BENCH_DB") else { return };
        let db = super::Db::open(std::path::Path::new(&path)).unwrap();
        let _ = db.snapshot().unwrap();
        let t = std::time::Instant::now();
        for _ in 0..20 {
            std::hint::black_box(db.snapshot().unwrap());
        }
        eprintln!("snapshot: {:?}", t.elapsed() / 20);
        let t = std::time::Instant::now();
        for _ in 0..20 {
            let mut stmt = db.conn.prepare_cached("SELECT id, project_id, parent_task_id, what, due_on, due_on_auto_set, link, notes, completed_at, in_progress_at, position FROM tasks").unwrap();
            let n = stmt.query_map([], |r| Ok((r.get::<_, i64>(0)?, r.get::<_, String>(3)?, r.get::<_, String>(7)?))).unwrap().count();
            std::hint::black_box(n);
        }
        eprintln!("tasks query only: {:?}", t.elapsed() / 20);
        let t = std::time::Instant::now();
        for i in 0..20 {
            db.set_in_progress(5, i % 2 == 0).unwrap();
        }
        eprintln!("one update: {:?}", t.elapsed() / 20);
    }
}
