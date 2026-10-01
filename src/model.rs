//! In-memory snapshot of the whole database plus the list rules (date
//! sections, grouping, blocking). A personal task list is small enough that
//! loading everything after each mutation costs a millisecond or two, which
//! keeps every view trivially consistent.

use std::collections::HashMap;

use chrono::{Duration, NaiveDate};

#[derive(Clone, Debug)]
pub struct Person {
    pub id: i64,
    pub name: String,
    pub open_tasks: usize,
}

#[derive(Clone, Debug)]
pub struct Project {
    pub id: i64,
    pub name: String,
    pub link: String,
    pub notes: String,
    pub archived_on: Option<NaiveDate>,
    pub open_tasks: usize,
}

impl Project {
    pub fn archived(&self) -> bool {
        self.archived_on.is_some()
    }
}

#[derive(Clone, Debug)]
pub struct Task {
    pub id: i64,
    pub project_id: i64,
    pub parent_id: Option<i64>,
    pub what: String,
    pub due_on: Option<NaiveDate>,
    pub due_on_auto_set: bool,
    pub link: String,
    pub notes: String,
    pub completed_at: Option<String>,
    pub in_progress: bool,
    pub position: i64,
    pub people: Vec<i64>,
    /// Tasks this one depends on (it is blocked by them).
    pub needs: Vec<i64>,
    /// Tasks that depend on this one.
    pub enables: Vec<i64>,
    /// Ordered by position, then id.
    pub subtasks: Vec<i64>,
}

impl Task {
    pub fn completed(&self) -> bool {
        self.completed_at.is_some()
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Scope {
    All,
    Mine(i64),
    Project(i64),
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Grouping {
    Date,
    Project,
    Person,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Due {
    Overdue,
    Today,
    Future,
    None,
}

pub struct Section {
    pub title: String,
    /// Project id when the section is a project group (heading links to it).
    pub project: Option<i64>,
    pub tasks: Vec<i64>,
    /// Tasks left out by the section cap; the UI offers to reveal them.
    pub hidden: Vec<i64>,
}

#[derive(Default)]
pub struct Snapshot {
    pub today: NaiveDate,
    pub people: Vec<Person>,
    pub projects: Vec<Project>,
    pub tasks: HashMap<i64, Task>,
}

pub const SECTION_CAP: usize = 10;

impl Snapshot {
    pub fn task(&self, id: i64) -> Option<&Task> {
        self.tasks.get(&id)
    }

    pub fn project(&self, id: i64) -> Option<&Project> {
        self.projects.iter().find(|p| p.id == id)
    }

    pub fn person(&self, id: i64) -> Option<&Person> {
        self.people.iter().find(|p| p.id == id)
    }

    pub fn project_name(&self, id: i64) -> &str {
        self.project(id).map(|p| p.name.as_str()).unwrap_or("")
    }

    pub fn due(&self, task: &Task) -> Due {
        match task.due_on {
            None => Due::None,
            Some(d) if d < self.today => Due::Overdue,
            Some(d) if d == self.today => Due::Today,
            Some(_) => Due::Future,
        }
    }

    fn incomplete(&self, ids: &[i64]) -> bool {
        ids.iter().any(|id| self.task(*id).is_some_and(|t| !t.completed()))
    }

    pub fn open_needs(&self, task: &Task) -> bool {
        self.incomplete(&task.needs)
    }

    pub fn open_subtasks(&self, task: &Task) -> bool {
        self.incomplete(&task.subtasks)
    }

    /// Why a task can't be completed yet, in the web app's words.
    pub fn blocked_reason(&self, task: &Task) -> Option<&'static str> {
        match (self.open_needs(task), self.open_subtasks(task)) {
            (true, true) => Some("Complete all sub-tasks and dependencies first!"),
            (true, false) => Some("All dependencies must be completed first!"),
            (false, true) => Some("Complete all sub-tasks first!"),
            (false, false) => None,
        }
    }

    /// Incomplete dependencies due on or after this task — it is scheduled
    /// before its blockers are done.
    pub fn date_conflicts(&self, task: &Task) -> Vec<&Task> {
        let Some(due) = task.due_on else { return vec![] };
        if task.completed() {
            return vec![];
        }
        task.needs
            .iter()
            .filter_map(|id| self.task(*id))
            .filter(|dep| !dep.completed() && dep.due_on.is_some_and(|d| d >= due))
            .collect()
    }

    fn by_due_date(&self, ids: &mut [i64]) {
        ids.sort_by_key(|id| {
            let t = &self.tasks[id];
            (t.due_on.is_none(), t.due_on, t.id)
        });
    }

    /// Incomplete top-level tasks in scope, by due date (undated last).
    pub fn open_top_level(&self, scope: Scope) -> Vec<i64> {
        let mut ids: Vec<i64> = self
            .tasks
            .values()
            .filter(|t| !t.completed() && t.parent_id.is_none())
            .filter(|t| match scope {
                Scope::All => true,
                Scope::Mine(person) => t.people.contains(&person),
                Scope::Project(project) => t.project_id == project,
            })
            .map(|t| t.id)
            .collect();
        self.by_due_date(&mut ids);
        ids
    }

    /// Overdue / Today / Tomorrow / Later. Mine shows every task due today;
    /// elsewhere Today, Tomorrow and Later each show the first ten.
    pub fn date_sections(&self, scope: Scope) -> Vec<Section> {
        let ids = self.open_top_level(scope);
        let tomorrow = self.today + Duration::days(1);
        let mut overdue = vec![];
        let mut today = vec![];
        let mut tom = vec![];
        let mut later = vec![];
        for id in ids {
            match self.tasks[&id].due_on {
                Some(d) if d < self.today => overdue.push(id),
                Some(d) if d == self.today => today.push(id),
                Some(d) if d == tomorrow => tom.push(id),
                _ => later.push(id),
            }
        }
        let mine = matches!(scope, Scope::Mine(_));
        let section = |title: &str, mut tasks: Vec<i64>, capped: bool| {
            let hidden = if capped && tasks.len() > SECTION_CAP { tasks.split_off(SECTION_CAP) } else { vec![] };
            Section { title: title.into(), project: None, tasks, hidden }
        };
        [
            section("Overdue", overdue, false),
            section("Today", today, !mine),
            section("Tomorrow", tom, !mine),
            section("Later", later, true),
        ]
        .into_iter()
        .filter(|s| !s.tasks.is_empty())
        .collect()
    }

    /// The tasks a scope shows in its date view — what the other groupings
    /// regroup in Mine mode, matching the web app.
    pub fn visible_tasks(&self, scope: Scope) -> Vec<i64> {
        match scope {
            Scope::Mine(_) => self.date_sections(scope).into_iter().flat_map(|s| s.tasks).collect(),
            _ => self.open_top_level(scope),
        }
    }

    pub fn grouped(&self, scope: Scope, grouping: Grouping) -> Vec<Section> {
        match grouping {
            Grouping::Date => self.date_sections(scope),
            Grouping::Project => {
                let ids = self.visible_tasks(scope);
                let mut out = vec![];
                for project in &self.projects {
                    let tasks: Vec<i64> = ids.iter().copied().filter(|id| self.tasks[id].project_id == project.id).collect();
                    if !tasks.is_empty() {
                        out.push(Section { title: project.name.clone(), project: Some(project.id), tasks, hidden: vec![] });
                    }
                }
                out
            }
            Grouping::Person => {
                let ids = self.visible_tasks(scope);
                let mut out = vec![];
                for person in &self.people {
                    let tasks: Vec<i64> = ids.iter().copied().filter(|id| self.tasks[id].people.contains(&person.id)).collect();
                    if !tasks.is_empty() {
                        out.push(Section { title: person.name.clone(), project: None, tasks, hidden: vec![] });
                    }
                }
                let unassigned: Vec<i64> = ids.iter().copied().filter(|id| self.tasks[id].people.is_empty()).collect();
                if !unassigned.is_empty() {
                    out.push(Section { title: "Unassigned".into(), project: None, tasks: unassigned, hidden: vec![] });
                }
                out
            }
        }
    }

    /// Completed top-level tasks, most recently completed first.
    pub fn completed(&self, scope: Scope) -> Vec<i64> {
        let mut tasks: Vec<&Task> = self
            .tasks
            .values()
            .filter(|t| t.completed() && t.parent_id.is_none())
            .filter(|t| match scope {
                Scope::Project(p) => t.project_id == p,
                _ => true,
            })
            .collect();
        tasks.sort_by(|a, b| b.completed_at.cmp(&a.completed_at).then(b.id.cmp(&a.id)));
        tasks.into_iter().map(|t| t.id).collect()
    }

    /// Incomplete tasks (sub-tasks included) whose title or notes contain the
    /// query, alphabetical, at most 20.
    pub fn search(&self, query: &str, exclude: &[i64]) -> Vec<i64> {
        let q = query.trim().to_lowercase();
        if q.is_empty() {
            return vec![];
        }
        let mut hits: Vec<&Task> = self
            .tasks
            .values()
            .filter(|t| !t.completed() && !exclude.contains(&t.id))
            .filter(|t| t.what.to_lowercase().contains(&q) || t.notes.to_lowercase().contains(&q))
            .collect();
        hits.sort_by(|a, b| {
            let a_title = a.what.to_lowercase();
            let b_title = b.what.to_lowercase();
            // Title matches first, then prefix matches, then alphabetical.
            (!a_title.contains(&q), !a_title.starts_with(&q), &a_title, a.id).cmp(&(
                !b_title.contains(&q),
                !b_title.starts_with(&q),
                &b_title,
                b.id,
            ))
        });
        hits.into_iter().take(20).map(|t| t.id).collect()
    }
}

pub fn task_count(n: usize) -> String {
    // The web app's wording: only two or more is plural.
    if n >= 2 { format!("{n} tasks") } else { format!("{n} task") }
}

/// Sibling order after dropping `dragged` on `target`'s upper half
/// (`after == false`) or lower half. `None` when the drop is a no-op.
pub fn drop_order(siblings: &[i64], dragged: i64, target: i64, after: bool) -> Option<Vec<i64>> {
    if dragged == target || !siblings.contains(&dragged) || !siblings.contains(&target) {
        return None;
    }
    let mut order: Vec<i64> = siblings.iter().copied().filter(|s| *s != dragged).collect();
    let at = order.iter().position(|s| *s == target)? + after as usize;
    order.insert(at, dragged);
    (order != siblings).then_some(order)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn dropping_subtasks() {
        let s = [4, 5, 6];
        assert_eq!(drop_order(&s, 6, 5, false), Some(vec![4, 6, 5]));
        assert_eq!(drop_order(&s, 6, 4, false), Some(vec![6, 4, 5]));
        assert_eq!(drop_order(&s, 4, 6, true), Some(vec![5, 6, 4]));
        assert_eq!(drop_order(&s, 4, 5, true), Some(vec![5, 4, 6]));
        assert_eq!(drop_order(&s, 4, 5, false), None); // already there
        assert_eq!(drop_order(&s, 5, 5, true), None);
        assert_eq!(drop_order(&s, 9, 5, true), None); // not a sibling
    }

    fn d(offset: i64) -> Option<NaiveDate> {
        Some(today() + Duration::days(offset))
    }

    fn today() -> NaiveDate {
        NaiveDate::from_ymd_opt(2026, 10, 1).unwrap()
    }

    fn task(id: i64, due: Option<NaiveDate>, people: &[i64]) -> Task {
        Task {
            id,
            project_id: 1,
            parent_id: None,
            what: format!("Task {id}"),
            due_on: due,
            due_on_auto_set: false,
            link: String::new(),
            notes: String::new(),
            completed_at: None,
            in_progress: false,
            position: 0,
            people: people.to_vec(),
            needs: vec![],
            enables: vec![],
            subtasks: vec![],
        }
    }

    fn snap(tasks: Vec<Task>) -> Snapshot {
        Snapshot {
            today: today(),
            people: vec![Person { id: 1, name: "Ann".into(), open_tasks: 0 }, Person { id: 2, name: "Bob".into(), open_tasks: 0 }],
            projects: vec![Project { id: 1, name: "Alpha".into(), link: String::new(), notes: String::new(), archived_on: None, open_tasks: 0 }],
            tasks: tasks.into_iter().map(|t| (t.id, t)).collect(),
        }
    }

    #[test]
    fn date_sections_split_and_cap() {
        let mut tasks = vec![task(1, d(-2), &[1]), task(2, d(0), &[1]), task(3, d(1), &[]), task(4, None, &[])];
        for i in 0..12 {
            tasks.push(task(100 + i, d(5 + i), &[1]));
        }
        let s = snap(tasks);
        let sections = s.date_sections(Scope::All);
        let titles: Vec<&str> = sections.iter().map(|s| s.title.as_str()).collect();
        assert_eq!(titles, ["Overdue", "Today", "Tomorrow", "Later"]);
        let later = &sections[3];
        assert_eq!(later.tasks.len(), 10);
        assert_eq!(later.hidden.len(), 3, "undated task sorts last and is capped away");
        assert_eq!(*later.hidden.last().unwrap(), 4);

        let mine = s.date_sections(Scope::Mine(1));
        assert_eq!(mine.iter().map(|s| s.title.as_str()).collect::<Vec<_>>(), ["Overdue", "Today", "Later"]);
    }

    #[test]
    fn blocking_and_conflicts() {
        let mut a = task(1, d(3), &[]);
        let mut b = task(2, d(5), &[]);
        a.needs = vec![2];
        b.enables = vec![1];
        let s = snap(vec![a, b]);
        assert_eq!(s.blocked_reason(s.task(1).unwrap()), Some("All dependencies must be completed first!"));
        assert_eq!(s.date_conflicts(s.task(1).unwrap()).len(), 1);
        assert_eq!(s.blocked_reason(s.task(2).unwrap()), None);
    }

    #[test]
    fn grouping_by_person_lists_unassigned_last() {
        let s = snap(vec![task(1, d(0), &[1, 2]), task(2, d(0), &[])]);
        let groups = s.grouped(Scope::All, Grouping::Person);
        let titles: Vec<&str> = groups.iter().map(|g| g.title.as_str()).collect();
        assert_eq!(titles, ["Ann", "Bob", "Unassigned"]);
    }

    #[test]
    fn counts() {
        assert_eq!(task_count(0), "0 task");
        assert_eq!(task_count(1), "1 task");
        assert_eq!(task_count(2), "2 tasks");
    }
}
