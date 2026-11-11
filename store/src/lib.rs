//! Storage. Everything the app remembers lives in one SQLite file.
//!
//! This module knows nothing about the UI. All day-dependent behaviour
//! (rollover, repeating tasks, reminders, "done today") takes the current
//! date or time as a parameter so it can be tested without a clock.

use std::path::Path;

use anyhow::{Context as _, Result};
use chrono::{DateTime, Datelike, Local, NaiveDate, NaiveTime, TimeZone, Weekday};
use rusqlite::{Connection, OptionalExtension, Row, params};

pub type TaskId = i64;
pub type ProjectId = i64;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Status {
    Open,
    Done,
}

/// When a task comes back to Today on its own.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Repeat {
    Never,
    Daily,
    Weekdays,
    Weekly(Weekday),
}

impl Repeat {
    pub fn matches(self, day: NaiveDate) -> bool {
        match self {
            Repeat::Never => false,
            Repeat::Daily => true,
            Repeat::Weekdays => !matches!(day.weekday(), Weekday::Sat | Weekday::Sun),
            Repeat::Weekly(weekday) => day.weekday() == weekday,
        }
    }

    pub fn label(self) -> String {
        match self {
            Repeat::Never => "Does not repeat".into(),
            Repeat::Daily => "Every day".into(),
            Repeat::Weekdays => "Every weekday".into(),
            Repeat::Weekly(day) => format!("Every {}", weekday_name(day)),
        }
    }

    fn to_db(self) -> String {
        match self {
            Repeat::Never => String::new(),
            Repeat::Daily => "daily".into(),
            Repeat::Weekdays => "weekdays".into(),
            Repeat::Weekly(day) => format!("weekly:{}", day.number_from_monday()),
        }
    }

    fn from_db(value: &str) -> Self {
        match value {
            "daily" => Repeat::Daily,
            "weekdays" => Repeat::Weekdays,
            other => other
                .strip_prefix("weekly:")
                .and_then(|n| n.parse::<u8>().ok())
                .and_then(|n| Weekday::try_from(n.wrapping_sub(1)).ok())
                .map(Repeat::Weekly)
                .unwrap_or(Repeat::Never),
        }
    }
}

pub fn weekday_name(day: Weekday) -> &'static str {
    match day {
        Weekday::Mon => "Monday",
        Weekday::Tue => "Tuesday",
        Weekday::Wed => "Wednesday",
        Weekday::Thu => "Thursday",
        Weekday::Fri => "Friday",
        Weekday::Sat => "Saturday",
        Weekday::Sun => "Sunday",
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct Task {
    pub id: TaskId,
    pub title: String,
    pub note: Option<String>,
    pub project_id: Option<ProjectId>,
    pub status: Status,
    pub in_today: bool,
    pub is_next_action: bool,
    pub repeat: Repeat,
    pub reminder: Option<NaiveTime>,
    pub created_at: i64,
    pub completed_at: Option<i64>,
    /// Position within Today. Only meaningful while `in_today`.
    pub today_order: i64,
    /// For the done copy of a repeating task: the task it was made from.
    pub repeat_of: Option<TaskId>,
}

impl Task {
    pub fn is_done(&self) -> bool {
        self.status == Status::Done
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct Project {
    pub id: ProjectId,
    pub name: String,
    /// Hex colour such as "#5B8DEF".
    pub color: String,
    pub archived: bool,
}

/// The fields the edit dialog can change.
#[derive(Debug, Clone, PartialEq)]
pub struct TaskEdit {
    pub title: String,
    pub note: Option<String>,
    pub project_id: Option<ProjectId>,
    pub repeat: Repeat,
    pub reminder: Option<NaiveTime>,
}

pub struct Store {
    conn: Connection,
}

const TASK_COLUMNS: &str = "id, title, note, project_id, status, in_today, is_next_action, \
     repeat, reminder, created_at, completed_at, today_order, repeat_of";

const MIGRATIONS: &[&str] = &[r#"
    CREATE TABLE projects (
        id          INTEGER PRIMARY KEY,
        name        TEXT    NOT NULL,
        color       TEXT    NOT NULL,
        archived    INTEGER NOT NULL DEFAULT 0,
        created_at  INTEGER NOT NULL
    );
    CREATE TABLE tasks (
        id              INTEGER PRIMARY KEY,
        title           TEXT    NOT NULL,
        note            TEXT,
        project_id      INTEGER REFERENCES projects(id) ON DELETE SET NULL,
        status          TEXT    NOT NULL DEFAULT 'open' CHECK (status IN ('open', 'done')),
        in_today        INTEGER NOT NULL DEFAULT 0,
        today_order     INTEGER NOT NULL DEFAULT 0,
        is_next_action  INTEGER NOT NULL DEFAULT 0,
        repeat          TEXT    NOT NULL DEFAULT '',
        reminder        TEXT,
        reminded_on     TEXT,
        repeat_of       INTEGER,
        created_at      INTEGER NOT NULL,
        completed_at    INTEGER
    );
    CREATE INDEX tasks_open ON tasks (status, in_today);
    CREATE INDEX tasks_completed ON tasks (completed_at);
    CREATE TABLE settings (
        key   TEXT PRIMARY KEY,
        value TEXT NOT NULL
    );
"#];

fn read_task(row: &Row) -> rusqlite::Result<Task> {
    let status: String = row.get(4)?;
    let repeat: String = row.get(7)?;
    let reminder: Option<String> = row.get(8)?;
    Ok(Task {
        id: row.get(0)?,
        title: row.get(1)?,
        note: row.get(2)?,
        project_id: row.get(3)?,
        status: if status == "done" { Status::Done } else { Status::Open },
        in_today: row.get(5)?,
        is_next_action: row.get(6)?,
        repeat: Repeat::from_db(&repeat),
        reminder: reminder.and_then(|t| NaiveTime::parse_from_str(&t, "%H:%M").ok()),
        created_at: row.get(9)?,
        completed_at: row.get(10)?,
        today_order: row.get(11)?,
        repeat_of: row.get(12)?,
    })
}

fn read_project(row: &Row) -> rusqlite::Result<Project> {
    Ok(Project { id: row.get(0)?, name: row.get(1)?, color: row.get(2)?, archived: row.get(3)? })
}

fn day_key(day: NaiveDate) -> String {
    day.format("%Y-%m-%d").to_string()
}

/// Unix seconds of local midnight at the start of `day`.
pub fn day_start(day: NaiveDate) -> i64 {
    let midnight = day.and_hms_opt(0, 0, 0).expect("midnight exists");
    Local
        .from_local_datetime(&midnight)
        .earliest()
        .map(|t| t.timestamp())
        // A DST gap at midnight: fall back to treating it as UTC.
        .unwrap_or_else(|| midnight.and_utc().timestamp())
}

pub fn local_day(timestamp: i64) -> NaiveDate {
    DateTime::from_timestamp(timestamp, 0)
        .map(|t| t.with_timezone(&Local).date_naive())
        .unwrap_or_default()
}

/// Reads a time of day the way people type it: "9", "930", "9:30",
/// "14:00", "1400", "9.15", "2pm" or "2:30 PM".
pub fn parse_time(text: &str) -> Option<NaiveTime> {
    let lower = text.trim().to_lowercase().replace(' ', "").replace('.', ":");
    let (digits, suffix) = if let Some(rest) = lower.strip_suffix("pm") {
        (rest.to_string(), Some(true))
    } else if let Some(rest) = lower.strip_suffix("am") {
        (rest.to_string(), Some(false))
    } else {
        (lower, None)
    };
    let (h, m): (u32, u32) = match digits.split_once(':') {
        Some((h, m)) => (h.parse().ok()?, m.parse().ok()?),
        None => match digits.len() {
            1 | 2 => (digits.parse().ok()?, 0),
            3 | 4 => {
                let split = digits.len() - 2;
                (digits[..split].parse().ok()?, digits[split..].parse().ok()?)
            }
            _ => return None,
        },
    };
    let h = match suffix {
        None => h,
        Some(_) if !(1..=12).contains(&h) => return None,
        Some(true) => if h == 12 { 12 } else { h + 12 },
        Some(false) => if h == 12 { 0 } else { h },
    };
    NaiveTime::from_hms_opt(h, m, 0)
}

pub fn now() -> i64 {
    Local::now().timestamp()
}

impl Store {
    pub fn open(path: &Path) -> Result<Self> {
        if let Some(dir) = path.parent() {
            std::fs::create_dir_all(dir).with_context(|| format!("creating {}", dir.display()))?;
        }
        let conn = Connection::open(path).with_context(|| format!("opening {}", path.display()))?;
        Self::init(conn)
    }

    pub fn open_in_memory() -> Result<Self> {
        Self::init(Connection::open_in_memory()?)
    }

    fn init(conn: Connection) -> Result<Self> {
        conn.pragma_update(None, "foreign_keys", true)?;
        conn.pragma_update(None, "journal_mode", "WAL")?;
        let store = Self { conn };
        store.migrate()?;
        Ok(store)
    }

    fn migrate(&self) -> Result<()> {
        let version: i64 = self.conn.pragma_query_value(None, "user_version", |r| r.get(0))?;
        for (index, sql) in MIGRATIONS.iter().enumerate().skip(version as usize) {
            let tx = self.conn.unchecked_transaction()?;
            tx.execute_batch(sql)?;
            tx.pragma_update(None, "user_version", index as i64 + 1)?;
            tx.commit()?;
        }
        Ok(())
    }

    // ---- settings -------------------------------------------------------

    pub fn setting(&self, key: &str) -> Result<Option<String>> {
        Ok(self
            .conn
            .query_row("SELECT value FROM settings WHERE key = ?1", [key], |r| r.get(0))
            .optional()?)
    }

    pub fn set_setting(&self, key: &str, value: &str) -> Result<()> {
        self.conn.execute(
            "INSERT INTO settings (key, value) VALUES (?1, ?2)
             ON CONFLICT (key) DO UPDATE SET value = excluded.value",
            params![key, value],
        )?;
        Ok(())
    }

    // ---- reading tasks --------------------------------------------------

    fn query_tasks(&self, where_and_order: &str, args: impl rusqlite::Params) -> Result<Vec<Task>> {
        let sql = format!("SELECT {TASK_COLUMNS} FROM tasks {where_and_order}");
        let mut stmt = self.conn.prepare_cached(&sql)?;
        let rows = stmt.query_map(args, read_task)?;
        Ok(rows.collect::<rusqlite::Result<_>>()?)
    }

    pub fn task(&self, id: TaskId) -> Result<Option<Task>> {
        Ok(self.query_tasks("WHERE id = ?1", [id])?.into_iter().next())
    }

    /// Open tasks chosen for today, in the user's order.
    pub fn today(&self) -> Result<Vec<Task>> {
        self.query_tasks(
            "WHERE status = 'open' AND in_today = 1 ORDER BY today_order, id",
            [],
        )
    }

    /// Tasks completed on `day`, most recent first.
    pub fn done_on(&self, day: NaiveDate) -> Result<Vec<Task>> {
        let from = day_start(day);
        let to = day_start(day.succ_opt().unwrap_or(day));
        self.query_tasks(
            "WHERE status = 'done' AND completed_at >= ?1 AND completed_at < ?2
             ORDER BY completed_at DESC, id DESC",
            [from, to],
        )
    }

    pub fn done_count_on(&self, day: NaiveDate) -> Result<usize> {
        let from = day_start(day);
        let to = day_start(day.succ_opt().unwrap_or(day));
        let count: i64 = self.conn.query_row(
            "SELECT COUNT(*) FROM tasks WHERE status = 'done' AND completed_at >= ?1 AND completed_at < ?2",
            [from, to],
            |r| r.get(0),
        )?;
        Ok(count as usize)
    }

    /// Completed tasks grouped by local day, newest day first, covering at
    /// most `max_days` distinct days.
    pub fn done_by_day(&self, max_days: usize) -> Result<Vec<(NaiveDate, Vec<Task>)>> {
        let tasks =
            self.query_tasks("WHERE status = 'done' ORDER BY completed_at DESC, id DESC", [])?;
        let mut days: Vec<(NaiveDate, Vec<Task>)> = Vec::new();
        for task in tasks {
            let day = local_day(task.completed_at.unwrap_or(task.created_at));
            match days.last_mut() {
                Some((last, list)) if *last == day => list.push(task),
                _ => {
                    if days.len() == max_days {
                        break;
                    }
                    days.push((day, vec![task]));
                }
            }
        }
        Ok(days)
    }

    /// Open tasks without a project.
    pub fn inbox(&self) -> Result<Vec<Task>> {
        self.query_tasks(
            "WHERE status = 'open' AND project_id IS NULL ORDER BY created_at, id",
            [],
        )
    }

    /// Open tasks of a project, the next action first.
    pub fn project_tasks(&self, project: ProjectId) -> Result<Vec<Task>> {
        self.query_tasks(
            "WHERE status = 'open' AND project_id = ?1
             ORDER BY is_next_action DESC, created_at, id",
            [project],
        )
    }

    // ---- changing tasks -------------------------------------------------

    pub fn add_task(&self, title: &str, project: Option<ProjectId>, in_today: bool) -> Result<TaskId> {
        let order = if in_today { self.next_today_order()? } else { 0 };
        self.conn.execute(
            "INSERT INTO tasks (title, project_id, in_today, today_order, created_at)
             VALUES (?1, ?2, ?3, ?4, ?5)",
            params![title.trim(), project, in_today, order, now()],
        )?;
        Ok(self.conn.last_insert_rowid())
    }

    fn next_today_order(&self) -> Result<i64> {
        Ok(self.conn.query_row(
            "SELECT COALESCE(MAX(today_order), 0) + 1 FROM tasks WHERE in_today = 1",
            [],
            |r| r.get(0),
        )?)
    }

    pub fn update_task(&self, id: TaskId, edit: &TaskEdit, today: NaiveDate) -> Result<()> {
        let Some(before) = self.task(id)? else { return Ok(()) };
        let tx = self.conn.unchecked_transaction()?;
        let moved_project = before.project_id != edit.project_id;
        tx.execute(
            "UPDATE tasks SET title = ?2, note = ?3, project_id = ?4, repeat = ?5, reminder = ?6,
                 is_next_action = CASE WHEN ?7 THEN 0 ELSE is_next_action END,
                 reminded_on = CASE WHEN reminder IS ?6 THEN reminded_on ELSE NULL END
             WHERE id = ?1",
            params![
                id,
                edit.title.trim(),
                edit.note.as_deref().map(str::trim).filter(|n| !n.is_empty()),
                edit.project_id,
                edit.repeat.to_db(),
                edit.reminder.map(|t| t.format("%H:%M").to_string()),
                moved_project,
            ],
        )?;
        tx.commit()?;
        // A repeating task belongs in Today on its days; a reminder only
        // rings for tasks in Today, so setting one brings the task in.
        let newly_repeating = before.repeat != edit.repeat && edit.repeat.matches(today);
        let new_reminder = edit.reminder.is_some() && before.reminder != edit.reminder;
        if !before.is_done() && !before.in_today && (newly_repeating || new_reminder) {
            self.set_in_today(id, true)?;
        }
        Ok(())
    }

    pub fn set_in_today(&self, id: TaskId, in_today: bool) -> Result<()> {
        let order = if in_today { self.next_today_order()? } else { 0 };
        self.conn.execute(
            "UPDATE tasks SET in_today = ?2, today_order = ?3 WHERE id = ?1 AND in_today != ?2",
            params![id, in_today, order],
        )?;
        Ok(())
    }

    /// Moves a Today task to `index` within the open Today list.
    pub fn move_in_today(&self, id: TaskId, index: usize) -> Result<()> {
        let mut ids: Vec<TaskId> = self.today()?.into_iter().map(|t| t.id).collect();
        let Some(from) = ids.iter().position(|&t| t == id) else { return Ok(()) };
        let id = ids.remove(from);
        ids.insert(index.min(ids.len()), id);
        let tx = self.conn.unchecked_transaction()?;
        for (order, id) in ids.iter().enumerate() {
            tx.execute("UPDATE tasks SET today_order = ?2 WHERE id = ?1", params![id, order as i64 + 1])?;
        }
        tx.commit()?;
        Ok(())
    }

    /// Marks `id` as its project's next action, or clears it. A project has
    /// at most one next action.
    pub fn set_next_action(&self, id: TaskId, next: bool) -> Result<()> {
        let Some(task) = self.task(id)? else { return Ok(()) };
        let Some(project) = task.project_id else { return Ok(()) };
        let tx = self.conn.unchecked_transaction()?;
        tx.execute("UPDATE tasks SET is_next_action = 0 WHERE project_id = ?1", [project])?;
        if next {
            tx.execute("UPDATE tasks SET is_next_action = 1 WHERE id = ?1", [id])?;
        }
        tx.commit()?;
        Ok(())
    }

    /// Completes a task. A repeating task stays open for its next day: what
    /// gets marked done is a copy, so the Done view can list every day it
    /// was done.
    pub fn complete(&self, id: TaskId, at: i64) -> Result<()> {
        let Some(task) = self.task(id)? else { return Ok(()) };
        if task.is_done() {
            return Ok(());
        }
        if task.repeat == Repeat::Never {
            self.conn.execute(
                "UPDATE tasks SET status = 'done', completed_at = ?2, is_next_action = 0 WHERE id = ?1",
                params![id, at],
            )?;
        } else {
            let tx = self.conn.unchecked_transaction()?;
            tx.execute(
                "INSERT INTO tasks (title, note, project_id, status, in_today, created_at, completed_at, repeat_of)
                 SELECT title, note, project_id, 'done', 0, created_at, ?2, id FROM tasks WHERE id = ?1",
                params![id, at],
            )?;
            tx.execute("UPDATE tasks SET in_today = 0 WHERE id = ?1", [id])?;
            tx.commit()?;
        }
        Ok(())
    }

    /// Undoes `complete`.
    pub fn reopen(&self, id: TaskId) -> Result<()> {
        let Some(task) = self.task(id)? else { return Ok(()) };
        if !task.is_done() {
            return Ok(());
        }
        match task.repeat_of {
            Some(original) if self.task(original)?.is_some_and(|t| !t.is_done()) => {
                self.conn.execute("DELETE FROM tasks WHERE id = ?1", [id])?;
                self.set_in_today(original, true)?;
            }
            _ => {
                self.conn.execute(
                    "UPDATE tasks SET status = 'open', completed_at = NULL, repeat_of = NULL WHERE id = ?1",
                    [id],
                )?;
            }
        }
        Ok(())
    }

    /// Deletes a task and returns it so the caller can offer undo.
    pub fn delete_task(&self, id: TaskId) -> Result<Option<Task>> {
        let task = self.task(id)?;
        self.conn.execute("DELETE FROM tasks WHERE id = ?1", [id])?;
        Ok(task)
    }

    /// Puts back a task returned by `delete_task`.
    pub fn restore_task(&self, task: &Task) -> Result<()> {
        let project = match task.project_id {
            Some(p) if self.project(p)?.is_some() => Some(p),
            _ => None,
        };
        self.conn.execute(
            &format!("INSERT OR REPLACE INTO tasks ({TASK_COLUMNS}) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13)"),
            params![
                task.id,
                task.title,
                task.note,
                project,
                if task.is_done() { "done" } else { "open" },
                task.in_today,
                task.is_next_action && project.is_some(),
                task.repeat.to_db(),
                task.reminder.map(|t| t.format("%H:%M").to_string()),
                task.created_at,
                task.completed_at,
                task.today_order,
                task.repeat_of,
            ],
        )?;
        Ok(())
    }

    // ---- the day --------------------------------------------------------

    /// Runs once per calendar day, on the first launch (or first check) of
    /// that day. Unfinished Today tasks quietly go back where they came from
    /// (their project or the Inbox), and repeating tasks for `today` are put
    /// into Today. Returns whether anything ran.
    pub fn start_day(&self, today: NaiveDate) -> Result<bool> {
        let key = day_key(today);
        match self.setting("last_day")? {
            Some(last) if last == key => return Ok(false),
            // The very first launch has no "yesterday" to roll over from.
            None => {
                self.set_setting("last_day", &key)?;
                return Ok(false);
            }
            Some(_) => {}
        }
        let tx = self.conn.unchecked_transaction()?;
        tx.execute("UPDATE tasks SET in_today = 0, today_order = 0 WHERE in_today = 1", [])?;
        let repeating: Vec<(TaskId, String)> = {
            let mut stmt = tx.prepare(
                "SELECT id, repeat FROM tasks WHERE status = 'open' AND repeat != '' ORDER BY created_at, id",
            )?;
            let rows = stmt.query_map([], |r| Ok((r.get(0)?, r.get(1)?)))?;
            rows.collect::<rusqlite::Result<_>>()?
        };
        let mut order = 0;
        for (id, rule) in repeating {
            if Repeat::from_db(&rule).matches(today) {
                order += 1;
                tx.execute(
                    "UPDATE tasks SET in_today = 1, today_order = ?2 WHERE id = ?1",
                    params![id, order],
                )?;
            }
        }
        tx.execute(
            "INSERT INTO settings (key, value) VALUES ('last_day', ?1)
             ON CONFLICT (key) DO UPDATE SET value = excluded.value",
            [&key],
        )?;
        tx.commit()?;
        Ok(true)
    }

    /// Reminders that should ring now: open Today tasks whose reminder time
    /// has passed today and has not rung yet. Reminders more than `grace`
    /// late (for example because the PC was off) are dropped silently
    /// rather than shown late. Every returned or dropped reminder is marked
    /// as handled for `today`.
    pub fn take_due_reminders(
        &self,
        today: NaiveDate,
        now: NaiveTime,
        grace: chrono::Duration,
    ) -> Result<Vec<Task>> {
        let key = day_key(today);
        let candidates = self.query_tasks(
            "WHERE status = 'open' AND in_today = 1 AND reminder IS NOT NULL
               AND (reminded_on IS NULL OR reminded_on != ?1)",
            [&key],
        )?;
        let mut due = Vec::new();
        for task in candidates {
            let Some(at) = task.reminder else { continue };
            if at > now {
                continue;
            }
            self.conn.execute(
                "UPDATE tasks SET reminded_on = ?2 WHERE id = ?1",
                params![task.id, key],
            )?;
            if now - at <= grace {
                due.push(task);
            }
        }
        Ok(due)
    }

    // ---- projects -------------------------------------------------------

    pub fn projects(&self, include_archived: bool) -> Result<Vec<Project>> {
        let sql = if include_archived {
            "SELECT id, name, color, archived FROM projects ORDER BY archived, name COLLATE NOCASE"
        } else {
            "SELECT id, name, color, archived FROM projects WHERE archived = 0 ORDER BY name COLLATE NOCASE"
        };
        let mut stmt = self.conn.prepare_cached(sql)?;
        let rows = stmt.query_map([], read_project)?;
        Ok(rows.collect::<rusqlite::Result<_>>()?)
    }

    pub fn project(&self, id: ProjectId) -> Result<Option<Project>> {
        Ok(self
            .conn
            .query_row("SELECT id, name, color, archived FROM projects WHERE id = ?1", [id], read_project)
            .optional()?)
    }

    pub fn add_project(&self, name: &str, color: &str) -> Result<ProjectId> {
        self.conn.execute(
            "INSERT INTO projects (name, color, created_at) VALUES (?1, ?2, ?3)",
            params![name.trim(), color, now()],
        )?;
        Ok(self.conn.last_insert_rowid())
    }

    pub fn update_project(&self, id: ProjectId, name: &str, color: &str) -> Result<()> {
        self.conn.execute(
            "UPDATE projects SET name = ?2, color = ?3 WHERE id = ?1",
            params![id, name.trim(), color],
        )?;
        Ok(())
    }

    /// Archiving hides a project and takes its tasks out of Today. Nothing
    /// is deleted, and unarchiving brings the project back.
    pub fn set_project_archived(&self, id: ProjectId, archived: bool) -> Result<()> {
        let tx = self.conn.unchecked_transaction()?;
        tx.execute("UPDATE projects SET archived = ?2 WHERE id = ?1", params![id, archived])?;
        if archived {
            tx.execute(
                "UPDATE tasks SET in_today = 0 WHERE project_id = ?1 AND status = 'open'",
                [id],
            )?;
        }
        tx.commit()?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn day(s: &str) -> NaiveDate {
        NaiveDate::parse_from_str(s, "%Y-%m-%d").unwrap()
    }

    fn time(s: &str) -> NaiveTime {
        NaiveTime::parse_from_str(s, "%H:%M").unwrap()
    }

    fn edit(store: &Store, id: TaskId) -> TaskEdit {
        let t = store.task(id).unwrap().unwrap();
        TaskEdit { title: t.title, note: t.note, project_id: t.project_id, repeat: t.repeat, reminder: t.reminder }
    }

    // 2026-11-09 is a Monday.
    const MON: &str = "2026-11-09";
    const TUE: &str = "2026-11-10";
    const SAT: &str = "2026-11-14";

    #[test]
    fn add_complete_reopen_delete() {
        let s = Store::open_in_memory().unwrap();
        let a = s.add_task("  Write report ", None, true).unwrap();
        assert_eq!(s.task(a).unwrap().unwrap().title, "Write report");
        assert_eq!(s.today().unwrap().len(), 1);
        assert_eq!(s.inbox().unwrap().len(), 1);

        s.complete(a, now()).unwrap();
        assert!(s.today().unwrap().is_empty());
        assert_eq!(s.done_count_on(Local::now().date_naive()).unwrap(), 1);

        s.reopen(a).unwrap();
        assert_eq!(s.today().unwrap().len(), 1);

        let deleted = s.delete_task(a).unwrap().unwrap();
        assert!(s.task(a).unwrap().is_none());
        s.restore_task(&deleted).unwrap();
        assert_eq!(s.task(a).unwrap().unwrap(), deleted);
    }

    #[test]
    fn today_order_and_move() {
        let s = Store::open_in_memory().unwrap();
        let ids: Vec<_> = ["a", "b", "c"].iter().map(|t| s.add_task(t, None, true).unwrap()).collect();
        s.move_in_today(ids[2], 0).unwrap();
        let titles: Vec<_> = s.today().unwrap().into_iter().map(|t| t.title).collect();
        assert_eq!(titles, ["c", "a", "b"]);
        s.move_in_today(ids[2], 99).unwrap();
        let titles: Vec<_> = s.today().unwrap().into_iter().map(|t| t.title).collect();
        assert_eq!(titles, ["a", "b", "c"]);
        // Toggling out and back in puts the task at the end.
        s.set_in_today(ids[0], false).unwrap();
        s.set_in_today(ids[0], true).unwrap();
        let titles: Vec<_> = s.today().unwrap().into_iter().map(|t| t.title).collect();
        assert_eq!(titles, ["b", "c", "a"]);
    }

    #[test]
    fn one_next_action_per_project() {
        let s = Store::open_in_memory().unwrap();
        let p = s.add_project("Website", "#5B8DEF").unwrap();
        let q = s.add_project("Taxes", "#E07A5F").unwrap();
        let a = s.add_task("a", Some(p), false).unwrap();
        let b = s.add_task("b", Some(p), false).unwrap();
        let c = s.add_task("c", Some(q), false).unwrap();
        s.set_next_action(a, true).unwrap();
        s.set_next_action(c, true).unwrap();
        s.set_next_action(b, true).unwrap();
        let tasks = s.project_tasks(p).unwrap();
        assert_eq!(tasks[0].id, b);
        assert!(tasks[0].is_next_action && !tasks[1].is_next_action);
        assert!(s.task(c).unwrap().unwrap().is_next_action, "other project untouched");
        // Moving a task to another project drops its next-action mark.
        let mut e = edit(&s, b);
        e.project_id = Some(q);
        s.update_task(b, &e, day(MON)).unwrap();
        assert!(!s.task(b).unwrap().unwrap().is_next_action);
    }

    #[test]
    fn rollover_returns_unfinished_tasks_quietly() {
        let s = Store::open_in_memory().unwrap();
        let p = s.add_project("Website", "#5B8DEF").unwrap();
        let early = s.add_task("before first run", None, true).unwrap();
        assert!(!s.start_day(day(MON)).unwrap(), "first launch only records the day");
        assert!(s.task(early).unwrap().unwrap().in_today, "first launch keeps Today as it is");
        s.delete_task(early).unwrap();
        let inbox_task = s.add_task("inbox", None, true).unwrap();
        let project_task = s.add_task("project", Some(p), true).unwrap();
        assert!(!s.start_day(day(MON)).unwrap(), "runs once per day");
        assert_eq!(s.today().unwrap().len(), 2);

        assert!(s.start_day(day(TUE)).unwrap());
        assert!(s.today().unwrap().is_empty());
        assert_eq!(s.inbox().unwrap()[0].id, inbox_task);
        assert_eq!(s.project_tasks(p).unwrap()[0].id, project_task);
    }

    #[test]
    fn repeating_tasks_come_back_on_their_days() {
        let s = Store::open_in_memory().unwrap();
        s.start_day(day(SAT) - chrono::Duration::days(7)).unwrap();
        let daily = s.add_task("daily", None, false).unwrap();
        let weekdays = s.add_task("weekdays", None, false).unwrap();
        let mondays = s.add_task("mondays", None, false).unwrap();
        for (id, repeat) in [(daily, Repeat::Daily), (weekdays, Repeat::Weekdays), (mondays, Repeat::Weekly(Weekday::Mon))] {
            let mut e = edit(&s, id);
            e.repeat = repeat;
            s.update_task(id, &e, day(SAT)).unwrap();
        }
        // Setting the rule on Saturday pulls in only the daily task.
        let today: Vec<_> = s.today().unwrap().into_iter().map(|t| t.title).collect();
        assert_eq!(today, ["daily"]);

        s.start_day(day(MON)).unwrap();
        assert_eq!(s.today().unwrap().len(), 3);

        // Completing a repeating task leaves it open for next time.
        s.complete(daily, day_start(day(MON)) + 3600).unwrap();
        assert_eq!(s.today().unwrap().len(), 2);
        assert_eq!(s.done_count_on(day(MON)).unwrap(), 1);
        assert!(!s.task(daily).unwrap().unwrap().is_done());

        s.start_day(day(TUE)).unwrap();
        let today: Vec<_> = s.today().unwrap().into_iter().map(|t| t.title).collect();
        assert_eq!(today, ["daily", "weekdays"]);

        s.start_day(day(SAT)).unwrap();
        let today: Vec<_> = s.today().unwrap().into_iter().map(|t| t.title).collect();
        assert_eq!(today, ["daily"]);

        // Undoing the Monday completion removes the done copy.
        let copy = s.done_on(day(MON)).unwrap()[0].id;
        s.reopen(copy).unwrap();
        assert!(s.task(copy).unwrap().is_none());
        assert_eq!(s.done_count_on(day(MON)).unwrap(), 0);
    }

    #[test]
    fn reminders_ring_once_and_never_late() {
        let s = Store::open_in_memory().unwrap();
        let a = s.add_task("call", None, false).unwrap();
        let mut e = edit(&s, a);
        e.reminder = Some(time("14:00"));
        s.update_task(a, &e, day(MON)).unwrap();
        assert!(s.task(a).unwrap().unwrap().in_today, "a reminder brings the task into Today");

        let grace = chrono::Duration::minutes(30);
        assert!(s.take_due_reminders(day(MON), time("13:59"), grace).unwrap().is_empty());
        assert_eq!(s.take_due_reminders(day(MON), time("14:00"), grace).unwrap().len(), 1);
        assert!(s.take_due_reminders(day(MON), time("14:05"), grace).unwrap().is_empty());

        // Next day it is back in Today only if it repeats; here it does not,
        // so put it back by hand and start the PC late.
        s.start_day(day(TUE)).unwrap();
        s.set_in_today(a, true).unwrap();
        assert!(s.take_due_reminders(day(TUE), time("16:00"), grace).unwrap().is_empty());
        assert!(s.take_due_reminders(day(TUE), time("16:01"), grace).unwrap().is_empty());
    }

    #[test]
    fn done_by_day_groups() {
        let s = Store::open_in_memory().unwrap();
        for (title, d, h) in [("a", MON, 9), ("b", MON, 10), ("c", TUE, 9)] {
            let id = s.add_task(title, None, false).unwrap();
            s.complete(id, day_start(day(d)) + h * 3600).unwrap();
        }
        let days = s.done_by_day(10).unwrap();
        assert_eq!(days.len(), 2);
        assert_eq!(days[0].0, day(TUE));
        assert_eq!(days[1].1.iter().map(|t| t.title.as_str()).collect::<Vec<_>>(), ["b", "a"]);
        assert_eq!(s.done_by_day(1).unwrap().len(), 1);
    }

    #[test]
    fn archiving_takes_tasks_out_of_today() {
        let s = Store::open_in_memory().unwrap();
        let p = s.add_project("Old", "#888888").unwrap();
        s.add_task("x", Some(p), true).unwrap();
        s.set_project_archived(p, true).unwrap();
        assert!(s.today().unwrap().is_empty());
        assert!(s.projects(false).unwrap().is_empty());
        assert_eq!(s.projects(true).unwrap().len(), 1);
    }

    #[test]
    fn reminder_times() {
        let t = |s: &str| parse_time(s).map(|t| t.format("%H:%M").to_string());
        assert_eq!(t("9").as_deref(), Some("09:00"));
        assert_eq!(t("930").as_deref(), Some("09:30"));
        assert_eq!(t("14:00").as_deref(), Some("14:00"));
        assert_eq!(t("1400").as_deref(), Some("14:00"));
        assert_eq!(t("2pm").as_deref(), Some("14:00"));
        assert_eq!(t("2:30 PM").as_deref(), Some("14:30"));
        assert_eq!(t("12am").as_deref(), Some("00:00"));
        assert_eq!(t("12pm").as_deref(), Some("12:00"));
        assert_eq!(t("9.15").as_deref(), Some("09:15"));
        assert_eq!(t("25:00"), None);
        assert_eq!(t("13pm"), None);
        assert_eq!(t("soon"), None);
    }

    #[test]
    fn repeat_round_trips() {
        for r in [Repeat::Never, Repeat::Daily, Repeat::Weekdays, Repeat::Weekly(Weekday::Sun), Repeat::Weekly(Weekday::Mon)] {
            assert_eq!(Repeat::from_db(&r.to_db()), r);
        }
    }

    #[test]
    fn settings_and_reopen_file() {
        let dir = std::env::temp_dir().join(format!("quiet-test-{}", std::process::id()));
        let path = dir.join("quiet.db");
        {
            let s = Store::open(&path).unwrap();
            s.set_setting("hotkey", "Ctrl+Alt+Space").unwrap();
            s.add_task("persisted", None, false).unwrap();
        }
        let s = Store::open(&path).unwrap();
        assert_eq!(s.setting("hotkey").unwrap().as_deref(), Some("Ctrl+Alt+Space"));
        assert_eq!(s.inbox().unwrap().len(), 1);
        drop(s);
        std::fs::remove_dir_all(dir).ok();
    }
}
