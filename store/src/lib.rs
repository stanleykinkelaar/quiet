//! Storage. Everything the app remembers lives in one SQLite file.
//!
//! This module knows nothing about the UI. All day-dependent behaviour
//! (rollover, repeating tasks, reminders, "done today") takes the current
//! date or time as a parameter so it can be tested without a clock.

use std::path::Path;

use anyhow::{Context as _, Result};
use chrono::{DateTime, Datelike, Duration, Local, NaiveDate, NaiveDateTime, NaiveTime, TimeZone, Weekday};
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
    /// In Today since an earlier day and not yet kept or sent back.
    pub carried: bool,
    /// The reminder rang (or passed while the PC was off) and nobody has
    /// reacted yet.
    pub reminder_open: bool,
    /// The day this task is for. It comes into Today `lead_days` before.
    pub day: Option<NaiveDate>,
    pub lead_days: i64,
    /// Inbox only: looked at during a sort and kept in the Inbox on purpose.
    pub reviewed: bool,
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
    pub day: Option<NaiveDate>,
    pub lead_days: i64,
}

/// A reminder nobody reacted to rings once more, this much later.
pub const REMIND_AGAIN_AFTER: Duration = Duration::minutes(10);

/// Lead times the editor offers for a task's day, in days.
pub const LEAD_DAYS: &[i64] = &[0, 1, 2, 7];

pub struct Store {
    conn: Connection,
}

const TASK_COLUMNS: &str = "id, title, note, project_id, status, in_today, is_next_action, \
     repeat, reminder, created_at, completed_at, today_order, repeat_of, \
     carried, reminder_open, day, lead_days, reviewed";

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
"#, r#"
    ALTER TABLE tasks ADD COLUMN carried       INTEGER NOT NULL DEFAULT 0;
    ALTER TABLE tasks ADD COLUMN snooze_at     TEXT;
    ALTER TABLE tasks ADD COLUMN rings         INTEGER NOT NULL DEFAULT 0;
    ALTER TABLE tasks ADD COLUMN reminder_open INTEGER NOT NULL DEFAULT 0;
    ALTER TABLE tasks ADD COLUMN day           TEXT;
    ALTER TABLE tasks ADD COLUMN lead_days     INTEGER NOT NULL DEFAULT 0;
    ALTER TABLE tasks ADD COLUMN day_shown     INTEGER NOT NULL DEFAULT 0;
    ALTER TABLE tasks ADD COLUMN reviewed      INTEGER NOT NULL DEFAULT 0;
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
        carried: row.get(13)?,
        reminder_open: row.get(14)?,
        day: row.get::<_, Option<String>>(15)?.and_then(|d| NaiveDate::parse_from_str(&d, "%Y-%m-%d").ok()),
        lead_days: row.get(16)?,
        reviewed: row.get(17)?,
    })
}

fn read_project(row: &Row) -> rusqlite::Result<Project> {
    Ok(Project { id: row.get(0)?, name: row.get(1)?, color: row.get(2)?, archived: row.get(3)? })
}

fn day_key(day: NaiveDate) -> String {
    day.format("%Y-%m-%d").to_string()
}

/// Local date and time as stored for snoozes, sortable as text.
fn moment_key(at: NaiveDateTime) -> String {
    at.format("%Y-%m-%dT%H:%M:%S").to_string()
}

fn parse_moment(text: &str) -> Option<NaiveDateTime> {
    NaiveDateTime::parse_from_str(text, "%Y-%m-%dT%H:%M:%S").ok()
}

/// Reads a day the way people type it, relative to `today`: "today",
/// "tomorrow", a weekday ("fri", "friday", the next one, today included),
/// "+3" or "3d" or "in 3 days", "2w", "14 nov", "nov 14", "14-11", "14/11"
/// or "2026-11-14". A day and month without a year that has already
/// passed means next year.
pub fn parse_day(text: &str, today: NaiveDate) -> Option<NaiveDate> {
    let text = text.trim().to_lowercase();
    let text = text.strip_prefix("in ").unwrap_or(&text).trim();
    match text {
        "" => return None,
        "today" | "tod" => return Some(today),
        "tomorrow" | "tom" | "tmr" => return today.succ_opt(),
        _ => {}
    }
    if let Some(day) = weekday_from(text) {
        let ahead = (7 + day.num_days_from_monday() - today.weekday().num_days_from_monday()) % 7;
        return today.checked_add_days(chrono::Days::new(ahead.into()));
    }
    // Offsets: "+3", "3d", "3 days", "2w", "2 weeks".
    let offset = text.trim_start_matches('+');
    let digits: String = offset.chars().take_while(|c| c.is_ascii_digit()).collect();
    if !digits.is_empty() {
        let unit = offset[digits.len()..].trim();
        let n: u64 = digits.parse().ok()?;
        let days = match unit {
            "" if text.starts_with('+') => Some(n),
            "d" | "day" | "days" => Some(n),
            "w" | "wk" | "week" | "weeks" => Some(n * 7),
            _ => None,
        };
        if let Some(days) = days {
            return today.checked_add_days(chrono::Days::new(days));
        }
    }
    if let Ok(date) = NaiveDate::parse_from_str(text, "%Y-%m-%d") {
        return Some(date);
    }
    // Day and month, in either order, with any separator.
    let parts: Vec<&str> =
        text.split([' ', '-', '/', '.']).filter(|p| !p.is_empty()).collect();
    let (day, month, year) = match parts.as_slice() {
        [a, b] | [a, b, _] => {
            let year = parts.get(2).and_then(|y| y.parse::<i32>().ok());
            match (a.parse::<u32>(), b.parse::<u32>(), month_from(a), month_from(b)) {
                (Ok(d), Ok(m), _, _) => (d, m, year),
                (Ok(d), Err(_), _, Some(m)) => (d, m, year),
                (Err(_), Ok(d), Some(m), _) => (d, m, year),
                _ => return None,
            }
        }
        _ => return None,
    };
    if let Some(year) = year {
        let year = if year < 100 { 2000 + year } else { year };
        return NaiveDate::from_ymd_opt(year, month, day);
    }
    let this_year = NaiveDate::from_ymd_opt(today.year(), month, day)?;
    if this_year >= today { Some(this_year) } else { NaiveDate::from_ymd_opt(today.year() + 1, month, day) }
}

fn weekday_from(text: &str) -> Option<Weekday> {
    if text.len() < 3 {
        return None;
    }
    [Weekday::Mon, Weekday::Tue, Weekday::Wed, Weekday::Thu, Weekday::Fri, Weekday::Sat, Weekday::Sun]
        .into_iter()
        .find(|&d| weekday_name(d).to_lowercase().starts_with(text))
}

fn month_from(text: &str) -> Option<u32> {
    const MONTHS: [&str; 12] = [
        "january", "february", "march", "april", "may", "june", "july", "august", "september", "october",
        "november", "december",
    ];
    if text.len() < 3 {
        return None;
    }
    MONTHS.iter().position(|m| m.starts_with(text)).map(|i| i as u32 + 1)
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

/// How far ahead planning the day looks for tasks with a day.
pub const PLAN_DAYS_AHEAD: i64 = 7;

pub fn now() -> i64 {
    Local::now().timestamp()
}

/// Whether a task for `day` belongs in Today by `today`, given its lead time.
pub fn day_arrives(day: NaiveDate, lead_days: i64, today: NaiveDate) -> bool {
    day - Duration::days(lead_days.max(0)) <= today
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
            "WHERE status = 'open' AND in_today = 1 ORDER BY carried, today_order, id",
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

    /// Inbox tasks waiting for a decision: not in Today, oldest first, the
    /// ones never looked at before the ones kept on purpose.
    pub fn inbox_to_sort(&self) -> Result<Vec<Task>> {
        self.query_tasks(
            "WHERE status = 'open' AND project_id IS NULL AND in_today = 0
             ORDER BY reviewed, created_at, id",
            [],
        )
    }

    /// How many Inbox tasks nobody has looked at yet.
    pub fn inbox_unsorted_count(&self) -> Result<usize> {
        let count: i64 = self.conn.query_row(
            "SELECT COUNT(*) FROM tasks
             WHERE status = 'open' AND project_id IS NULL AND in_today = 0 AND reviewed = 0",
            [],
            |r| r.get(0),
        )?;
        Ok(count as usize)
    }

    /// Open tasks in the Inbox and in active projects: the Inbox first,
    /// then each project's tasks together, its next action first.
    pub fn inbox_with_projects(&self) -> Result<Vec<Task>> {
        self.query_tasks(
            "WHERE status = 'open'
               AND (project_id IS NULL OR project_id IN (SELECT id FROM projects WHERE archived = 0))
             ORDER BY project_id IS NOT NULL, project_id, is_next_action DESC, created_at, id",
            [],
        )
    }

    /// What planning the day offers, one at a time, in this order: tasks
    /// whose day is within the coming week (soonest first), one task from
    /// each active project, then the Inbox (never looked at first). A
    /// project offers its next action, or without one its oldest task that
    /// has no day and does not repeat. Only open tasks that are not in
    /// Today yet, each once.
    pub fn plan_candidates(&self, today: NaiveDate) -> Result<Vec<Task>> {
        let soon = day_key(today + Duration::days(PLAN_DAYS_AHEAD));
        let mut tasks = self.query_tasks(
            "WHERE status = 'open' AND in_today = 0 AND day IS NOT NULL AND day >= ?1 AND day <= ?2
             ORDER BY day, created_at, id",
            params![day_key(today), soon],
        )?;
        tasks.extend(self.query_tasks(
            "WHERE status = 'open' AND in_today = 0
               AND project_id IN (SELECT id FROM projects WHERE archived = 0)
               AND (is_next_action = 1 OR (
                   NOT EXISTS (SELECT 1 FROM tasks n WHERE n.project_id = tasks.project_id
                                 AND n.status = 'open' AND n.is_next_action = 1)
                   AND id = (SELECT f.id FROM tasks f WHERE f.project_id = tasks.project_id
                               AND f.status = 'open'
                               AND f.day IS NULL AND f.repeat = ''
                             ORDER BY f.created_at, f.id LIMIT 1)))
             ORDER BY created_at, id",
            [],
        )?);
        tasks.extend(self.inbox_to_sort()?);
        let mut seen = std::collections::HashSet::new();
        tasks.retain(|t| seen.insert(t.id));
        Ok(tasks)
    }

    /// Whether the day has been planned (the plan was opened) on `today`.
    pub fn planned_on(&self, today: NaiveDate) -> Result<bool> {
        Ok(self.setting("planned_day")?.is_some_and(|d| d == day_key(today)))
    }

    pub fn mark_planned(&self, today: NaiveDate) -> Result<()> {
        self.set_setting("planned_day", &day_key(today))
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
            "UPDATE tasks SET title = ?2, note = ?3, project_id = ?4, repeat = ?5,
                 is_next_action = CASE WHEN ?7 THEN 0 ELSE is_next_action END,
                 reminded_on = CASE WHEN reminder IS ?6 THEN reminded_on ELSE NULL END,
                 snooze_at = CASE WHEN reminder IS ?6 THEN snooze_at ELSE NULL END,
                 rings = CASE WHEN reminder IS ?6 THEN rings ELSE 0 END,
                 reminder_open = CASE WHEN reminder IS ?6 THEN reminder_open ELSE 0 END,
                 day_shown = CASE WHEN day IS ?8 AND lead_days = ?9 THEN day_shown ELSE 0 END,
                 reminder = ?6, day = ?8, lead_days = ?9
             WHERE id = ?1",
            params![
                id,
                edit.title.trim(),
                edit.note.as_deref().map(str::trim).filter(|n| !n.is_empty()),
                edit.project_id,
                edit.repeat.to_db(),
                edit.reminder.map(|t| t.format("%H:%M").to_string()),
                moved_project,
                edit.day.map(day_key),
                edit.lead_days.max(0),
            ],
        )?;
        tx.commit()?;
        // A repeating task belongs in Today on its days; a reminder only
        // rings for tasks in Today, so setting one brings the task in.
        let newly_repeating = before.repeat != edit.repeat && edit.repeat.matches(today);
        let new_reminder = edit.reminder.is_some() && before.reminder != edit.reminder;
        let day_arrived = (before.day != edit.day || before.lead_days != edit.lead_days)
            && edit.day.is_some_and(|day| day_arrives(day, edit.lead_days, today));
        if !before.is_done() && !before.in_today && (newly_repeating || new_reminder || day_arrived) {
            self.set_in_today(id, true)?;
        }
        if day_arrived {
            self.conn.execute("UPDATE tasks SET day_shown = 1 WHERE id = ?1", [id])?;
        }
        Ok(())
    }

    pub fn set_in_today(&self, id: TaskId, in_today: bool) -> Result<()> {
        let order = if in_today { self.next_today_order()? } else { 0 };
        self.conn.execute(
            "UPDATE tasks SET in_today = ?2, today_order = ?3, carried = 0,
                 snooze_at = CASE WHEN ?2 THEN snooze_at ELSE NULL END,
                 reminder_open = CASE WHEN ?2 THEN reminder_open ELSE 0 END
             WHERE id = ?1 AND in_today != ?2",
            params![id, in_today, order],
        )?;
        Ok(())
    }

    /// Keeps a task that came along from an earlier day in Today, at the
    /// end of today's list.
    pub fn keep_carried(&self, id: TaskId) -> Result<()> {
        let order = self.next_today_order()?;
        self.conn.execute(
            "UPDATE tasks SET carried = 0, today_order = ?2 WHERE id = ?1 AND carried = 1",
            params![id, order],
        )?;
        Ok(())
    }

    /// Keeps every task from earlier days, or sends them all back.
    pub fn settle_carried(&self, keep: bool) -> Result<()> {
        let ids: Vec<TaskId> = self.today()?.into_iter().filter(|t| t.carried).map(|t| t.id).collect();
        for id in ids {
            if keep { self.keep_carried(id)? } else { self.set_in_today(id, false)? }
        }
        Ok(())
    }

    /// Inbox sorting: moves a task into a project.
    pub fn move_to_project(&self, id: TaskId, project: ProjectId) -> Result<()> {
        self.conn.execute(
            "UPDATE tasks SET project_id = ?2, is_next_action = 0, reviewed = 1 WHERE id = ?1",
            params![id, project],
        )?;
        Ok(())
    }

    /// Inbox sorting: looked at, and left in the Inbox on purpose.
    pub fn mark_reviewed(&self, id: TaskId) -> Result<()> {
        self.conn.execute("UPDATE tasks SET reviewed = 1 WHERE id = ?1", [id])?;
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
                "UPDATE tasks SET status = 'done', completed_at = ?2, is_next_action = 0,
                     carried = 0, snooze_at = NULL, reminder_open = 0
                 WHERE id = ?1",
                params![id, at],
            )?;
        } else {
            let tx = self.conn.unchecked_transaction()?;
            tx.execute(
                "INSERT INTO tasks (title, note, project_id, status, in_today, created_at, completed_at, repeat_of)
                 SELECT title, note, project_id, 'done', 0, created_at, ?2, id FROM tasks WHERE id = ?1",
                params![id, at],
            )?;
            tx.execute(
                "UPDATE tasks SET in_today = 0, carried = 0, snooze_at = NULL, reminder_open = 0 WHERE id = ?1",
                [id],
            )?;
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
            &format!(
                "INSERT OR REPLACE INTO tasks ({TASK_COLUMNS})
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14, ?15, ?16, ?17, ?18)"
            ),
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
                task.carried,
                task.reminder_open,
                task.day.map(day_key),
                task.lead_days,
                task.reviewed,
            ],
        )?;
        Ok(())
    }

    // ---- the day --------------------------------------------------------

    /// Runs once per calendar day, on the first launch (or first check) of
    /// that day. Unfinished Today tasks stay, marked as carried from an
    /// earlier day so you can keep them or send them back. Repeating tasks
    /// for `today`, and tasks whose day has arrived, come into Today.
    /// Returns whether anything ran.
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
        tx.execute("UPDATE tasks SET snooze_at = NULL, rings = 0, reminder_open = 0", [])?;
        tx.execute("UPDATE tasks SET in_today = 0, today_order = 0, carried = 0 WHERE in_today = 1 AND status = 'done'", [])?;
        tx.execute("UPDATE tasks SET carried = 1 WHERE in_today = 1 AND status = 'open'", [])?;

        let mut arriving: Vec<TaskId> = Vec::new();
        {
            let mut stmt = tx.prepare(
                "SELECT id, repeat, day, lead_days, day_shown FROM tasks WHERE status = 'open'
                   AND (repeat != '' OR day IS NOT NULL) ORDER BY created_at, id",
            )?;
            let rows = stmt.query_map([], |r| {
                Ok((
                    r.get::<_, TaskId>(0)?,
                    r.get::<_, String>(1)?,
                    r.get::<_, Option<String>>(2)?,
                    r.get::<_, i64>(3)?,
                    r.get::<_, bool>(4)?,
                ))
            })?;
            for row in rows {
                let (id, rule, day, lead, shown) = row?;
                let repeats_today = Repeat::from_db(&rule).matches(today);
                let day_arrived = !shown
                    && day
                        .and_then(|d| NaiveDate::parse_from_str(&d, "%Y-%m-%d").ok())
                        .is_some_and(|d| day_arrives(d, lead, today));
                if repeats_today || day_arrived {
                    arriving.push(id);
                }
                if day_arrived {
                    tx.execute("UPDATE tasks SET day_shown = 1 WHERE id = ?1", [id])?;
                }
            }
        }
        for (order, id) in arriving.into_iter().enumerate() {
            tx.execute(
                "UPDATE tasks SET in_today = 1, carried = 0, today_order = ?2 WHERE id = ?1",
                params![id, order as i64 + 1],
            )?;
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
    /// has passed today and has not rung yet, and snoozed ones whose time
    /// has come. A reminder nobody reacts to rings once more after
    /// `REMIND_AGAIN_AFTER`. One more than `grace` late (the PC was asleep
    /// or off) does not ring at all; the task shows it was missed instead.
    pub fn take_due_reminders(&self, now: NaiveDateTime, grace: Duration) -> Result<Vec<Task>> {
        let today = now.date();
        let key = day_key(today);
        let candidates: Vec<(TaskId, Option<String>, Option<String>, i64)> = {
            let mut stmt = self.conn.prepare_cached(
                "SELECT id, reminded_on, snooze_at, rings FROM tasks
                 WHERE status = 'open' AND in_today = 1 AND reminder IS NOT NULL",
            )?;
            let rows = stmt.query_map([], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?)))?;
            rows.collect::<rusqlite::Result<_>>()?
        };
        let mut due = Vec::new();
        for (id, reminded_on, snooze_at, rings) in candidates {
            let Some(task) = self.task(id)? else { continue };
            let Some(at) = task.reminder else { continue };
            let first = reminded_on.as_deref() != Some(key.as_str()) && at <= now.time();
            let snoozed = snooze_at.as_deref().and_then(parse_moment).filter(|s| *s <= now);
            let ring_at = match (first, snoozed) {
                (true, _) => today.and_time(at),
                (false, Some(at)) => at,
                (false, None) => continue,
            };
            if now - ring_at > grace {
                self.conn.execute(
                    "UPDATE tasks SET reminded_on = ?2, snooze_at = NULL, reminder_open = 1 WHERE id = ?1",
                    params![id, key],
                )?;
                continue;
            }
            let rings = if first { 1 } else { rings + 1 };
            let again = (rings == 1).then(|| moment_key(now + REMIND_AGAIN_AFTER));
            self.conn.execute(
                "UPDATE tasks SET reminded_on = ?2, rings = ?3, snooze_at = ?4, reminder_open = 1 WHERE id = ?1",
                params![id, key, rings, again],
            )?;
            due.push(task);
        }
        Ok(due)
    }

    /// Rings the reminder again at `until`.
    pub fn snooze(&self, id: TaskId, until: NaiveDateTime) -> Result<()> {
        self.conn.execute(
            "UPDATE tasks SET snooze_at = ?2, reminder_open = 0 WHERE id = ?1",
            params![id, moment_key(until)],
        )?;
        Ok(())
    }

    /// The reminder was seen: no second ring, no missed mark.
    pub fn acknowledge_reminder(&self, id: TaskId) -> Result<()> {
        self.conn.execute("UPDATE tasks SET snooze_at = NULL, reminder_open = 0 WHERE id = ?1", [id])?;
        Ok(())
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
        TaskEdit {
            title: t.title,
            note: t.note,
            project_id: t.project_id,
            repeat: t.repeat,
            reminder: t.reminder,
            day: t.day,
            lead_days: t.lead_days,
        }
    }

    fn at(d: &str, t: &str) -> NaiveDateTime {
        day(d).and_time(time(t))
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
    fn unfinished_today_tasks_stay_marked_as_carried() {
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

        let fresh = s.add_task("fresh", None, false).unwrap();
        assert!(s.start_day(day(TUE)).unwrap());
        let today = s.today().unwrap();
        assert_eq!(today.len(), 2, "nothing is lost overnight");
        assert!(today.iter().all(|t| t.carried));

        // Something new for today sorts above what came along.
        s.set_in_today(fresh, true).unwrap();
        let today = s.today().unwrap();
        assert_eq!(today[0].id, fresh);
        assert!(!today[0].carried);

        s.keep_carried(inbox_task).unwrap();
        let kept = s.task(inbox_task).unwrap().unwrap();
        assert!(kept.in_today && !kept.carried);

        s.settle_carried(false).unwrap();
        assert!(!s.task(project_task).unwrap().unwrap().in_today, "sent back to its project");
        assert_eq!(s.project_tasks(p).unwrap()[0].id, project_task);
        assert_eq!(s.today().unwrap().len(), 2);

        // Done tasks leave Today overnight.
        s.complete(fresh, now()).unwrap();
        s.start_day(day(TUE) + Duration::days(1)).unwrap();
        assert!(!s.task(fresh).unwrap().unwrap().in_today);
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
        let today: Vec<_> = s.today().unwrap().into_iter().map(|t| (t.title, t.carried)).collect();
        // Mondays was not done, so it stays, marked as carried.
        assert_eq!(today, [("daily".into(), false), ("weekdays".into(), false), ("mondays".to_string(), true)]);

        s.settle_carried(false).unwrap();
        s.start_day(day(SAT)).unwrap();
        let today: Vec<_> = s.today().unwrap().into_iter().map(|t| (t.title, t.carried)).collect();
        assert_eq!(today, [("daily".to_string(), false), ("weekdays".into(), true)]);

        // Undoing the Monday completion removes the done copy.
        let copy = s.done_on(day(MON)).unwrap()[0].id;
        s.reopen(copy).unwrap();
        assert!(s.task(copy).unwrap().is_none());
        assert_eq!(s.done_count_on(day(MON)).unwrap(), 0);
    }

    #[test]
    fn reminders_ring_again_once_and_mark_missed_ones() {
        let s = Store::open_in_memory().unwrap();
        let a = s.add_task("call", None, false).unwrap();
        let mut e = edit(&s, a);
        e.reminder = Some(time("14:00"));
        s.update_task(a, &e, day(MON)).unwrap();
        assert!(s.task(a).unwrap().unwrap().in_today, "a reminder brings the task into Today");

        let grace = Duration::minutes(30);
        assert!(s.take_due_reminders(at(MON, "13:59"), grace).unwrap().is_empty());
        assert_eq!(s.take_due_reminders(at(MON, "14:00"), grace).unwrap().len(), 1);
        assert!(s.task(a).unwrap().unwrap().reminder_open);
        assert!(s.take_due_reminders(at(MON, "14:05"), grace).unwrap().is_empty());
        // Nobody reacted: one more ring, then quiet.
        assert_eq!(s.take_due_reminders(at(MON, "14:10"), grace).unwrap().len(), 1);
        assert!(s.take_due_reminders(at(MON, "14:30"), grace).unwrap().is_empty());
        assert!(s.task(a).unwrap().unwrap().reminder_open, "still marked until you react");
        s.acknowledge_reminder(a).unwrap();
        assert!(!s.task(a).unwrap().unwrap().reminder_open);

        // The PC was off at 14:00: no late ring, but the task shows it.
        s.start_day(day(TUE)).unwrap();
        assert!(s.task(a).unwrap().unwrap().in_today, "unfinished, so it stays");
        assert!(s.take_due_reminders(at(TUE, "16:00"), grace).unwrap().is_empty());
        assert!(s.task(a).unwrap().unwrap().reminder_open);
        assert!(s.take_due_reminders(at(TUE, "16:30"), grace).unwrap().is_empty());
    }

    #[test]
    fn snoozing_rings_later_and_completing_stops_it() {
        let s = Store::open_in_memory().unwrap();
        let a = s.add_task("stretch", None, true).unwrap();
        let mut e = edit(&s, a);
        e.reminder = Some(time("09:00"));
        s.update_task(a, &e, day(MON)).unwrap();
        let grace = Duration::minutes(30);
        assert_eq!(s.take_due_reminders(at(MON, "09:00"), grace).unwrap().len(), 1);

        s.snooze(a, at(MON, "10:00")).unwrap();
        assert!(!s.task(a).unwrap().unwrap().reminder_open);
        assert!(s.take_due_reminders(at(MON, "09:10"), grace).unwrap().is_empty(), "snooze replaces the second ring");
        assert_eq!(s.take_due_reminders(at(MON, "10:00"), grace).unwrap().len(), 1);
        assert!(s.take_due_reminders(at(MON, "10:10"), grace).unwrap().is_empty(), "a snoozed ring does not repeat");

        s.snooze(a, at(MON, "11:00")).unwrap();
        s.complete(a, now()).unwrap();
        assert!(s.take_due_reminders(at(MON, "11:00"), grace).unwrap().is_empty());
    }

    #[test]
    fn a_day_brings_the_task_into_today_once() {
        let s = Store::open_in_memory().unwrap();
        s.start_day(day(MON)).unwrap();
        let rent = s.add_task("pay rent", None, false).unwrap();
        let mut e = edit(&s, rent);
        e.day = Some(day(SAT));
        e.lead_days = 2;
        s.update_task(rent, &e, day(MON)).unwrap();
        assert!(!s.task(rent).unwrap().unwrap().in_today, "Saturday is still far away");

        s.start_day(day("2026-11-11")).unwrap();
        assert!(s.today().unwrap().is_empty());
        s.start_day(day("2026-11-12")).unwrap();
        assert_eq!(s.today().unwrap()[0].id, rent, "two days before Saturday");

        // Sent back on purpose: it does not keep coming back.
        s.set_in_today(rent, false).unwrap();
        s.start_day(day("2026-11-13")).unwrap();
        assert!(s.today().unwrap().is_empty());

        // A day that has already arrived brings the task in right away.
        let call = s.add_task("call mum", None, false).unwrap();
        let mut e = edit(&s, call);
        e.day = Some(day("2026-11-13"));
        s.update_task(call, &e, day("2026-11-13")).unwrap();
        assert!(s.task(call).unwrap().unwrap().in_today);
        assert_eq!(s.task(call).unwrap().unwrap().day, Some(day("2026-11-13")));
    }

    #[test]
    fn plan_candidates_come_in_order() {
        let s = Store::open_in_memory().unwrap();
        let today = day(MON);
        let project = s.add_project("Home", "#5B8DEF").unwrap();
        let archived = s.add_project("Old", "#5B8DEF").unwrap();
        s.set_project_archived(archived, true).unwrap();

        let inbox_seen = s.add_task("inbox seen", None, false).unwrap();
        s.mark_reviewed(inbox_seen).unwrap();
        s.add_task("inbox new", None, false).unwrap();
        s.add_task("already today", None, true).unwrap();
        let next = s.add_task("next action", Some(project), false).unwrap();
        s.set_next_action(next, true).unwrap();
        s.add_task("other project task", Some(project), false).unwrap();
        let old_next = s.add_task("archived next", Some(archived), false).unwrap();
        s.set_next_action(old_next, true).unwrap();

        let with_day = |title: &str, d: &str| {
            let id = s.add_task(title, Some(project), false).unwrap();
            let mut e = edit(&s, id);
            e.day = Some(day(d));
            s.update_task(id, &e, today).unwrap();
            id
        };
        with_day("saturday", SAT);
        with_day("tuesday", TUE);
        with_day("far away", "2026-12-01");
        with_day("yesterday", "2026-11-08");
        // A next action with a day shows once, with the days. (It also
        // takes over as Home's next action, so "next action" drops out.)
        let both = with_day("next with day", "2026-11-12");
        s.set_next_action(both, true).unwrap();

        let titles: Vec<_> = s.plan_candidates(today).unwrap().into_iter().map(|t| t.title).collect();
        assert_eq!(titles, ["tuesday", "next with day", "saturday", "inbox new", "inbox seen"]);
    }

    #[test]
    fn plan_offers_a_task_from_projects_without_a_next_action() {
        let s = Store::open_in_memory().unwrap();
        let today = day(MON);
        let work = s.add_project("Work", "#5B8DEF").unwrap();
        let later = s.add_task("for later", Some(work), false).unwrap();
        let mut e = edit(&s, later);
        e.day = Some(day("2026-12-01"));
        s.update_task(later, &e, today).unwrap();
        let daily = s.add_task("daily", Some(work), false).unwrap();
        let mut e = edit(&s, daily);
        e.repeat = Repeat::Daily;
        s.update_task(daily, &e, today).unwrap();
        s.set_in_today(daily, false).unwrap();
        s.add_task("oldest plain", Some(work), false).unwrap();
        s.add_task("newer plain", Some(work), false).unwrap();

        let titles = |s: &Store| -> Vec<String> {
            s.plan_candidates(today).unwrap().into_iter().map(|t| t.title).collect()
        };
        assert_eq!(titles(&s), ["oldest plain"], "no day, no repeat, oldest first");
        let oldest = s.project_tasks(work).unwrap().into_iter().find(|t| t.title == "oldest plain").unwrap();
        s.set_in_today(oldest.id, true).unwrap();
        assert!(titles(&s).is_empty(), "one per project: its oldest is already in Today");
        s.set_in_today(oldest.id, false).unwrap();

        let next = s.add_task("chosen", Some(work), false).unwrap();
        s.set_next_action(next, true).unwrap();
        assert_eq!(titles(&s), ["chosen"], "a next action wins");

        s.set_in_today(next, true).unwrap();
        assert!(titles(&s).is_empty(), "the project is already in Today");
    }

    #[test]
    fn inbox_with_projects_groups_by_project() {
        let s = Store::open_in_memory().unwrap();
        let home = s.add_project("Home", "#5B8DEF").unwrap();
        let old = s.add_project("Old", "#5B8DEF").unwrap();
        s.set_project_archived(old, true).unwrap();
        s.add_task("home a", Some(home), false).unwrap();
        s.add_task("loose", None, false).unwrap();
        let next = s.add_task("home next", Some(home), false).unwrap();
        s.set_next_action(next, true).unwrap();
        s.add_task("archived", Some(old), false).unwrap();
        let done = s.add_task("finished", None, false).unwrap();
        s.complete(done, now()).unwrap();

        let titles: Vec<_> = s.inbox_with_projects().unwrap().into_iter().map(|t| t.title).collect();
        assert_eq!(titles, ["loose", "home next", "home a"]);
    }

    #[test]
    fn planning_is_once_a_day() {
        let s = Store::open_in_memory().unwrap();
        assert!(!s.planned_on(day(MON)).unwrap());
        s.mark_planned(day(MON)).unwrap();
        assert!(s.planned_on(day(MON)).unwrap());
        assert!(!s.planned_on(day(TUE)).unwrap());
    }

    #[test]
    fn sorting_the_inbox() {
        let s = Store::open_in_memory().unwrap();
        let p = s.add_project("Home", "#5B8DEF").unwrap();
        let a = s.add_task("a", None, false).unwrap();
        let b = s.add_task("b", None, false).unwrap();
        let c = s.add_task("c", None, false).unwrap();
        s.add_task("already today", None, true).unwrap();
        assert_eq!(s.inbox_unsorted_count().unwrap(), 3);

        s.mark_reviewed(a).unwrap();
        s.move_to_project(b, p).unwrap();
        assert_eq!(s.inbox_unsorted_count().unwrap(), 1);
        let order: Vec<_> = s.inbox_to_sort().unwrap().into_iter().map(|t| t.id).collect();
        assert_eq!(order, [c, a], "unsorted first, kept ones after");
        assert_eq!(s.project_tasks(p).unwrap()[0].id, b);
    }

    #[test]
    fn days_people_type() {
        let today = day(TUE); // Tuesday 10 November 2026
        let d = |t: &str| parse_day(t, today).map(|d| d.format("%Y-%m-%d").to_string());
        assert_eq!(d("today").as_deref(), Some("2026-11-10"));
        assert_eq!(d("Tomorrow").as_deref(), Some("2026-11-11"));
        assert_eq!(d("fri").as_deref(), Some("2026-11-13"));
        assert_eq!(d("tuesday").as_deref(), Some("2026-11-10"));
        assert_eq!(d("mon").as_deref(), Some("2026-11-16"));
        assert_eq!(d("+3").as_deref(), Some("2026-11-13"));
        assert_eq!(d("in 3 days").as_deref(), Some("2026-11-13"));
        assert_eq!(d("2w").as_deref(), Some("2026-11-24"));
        assert_eq!(d("14 nov").as_deref(), Some("2026-11-14"));
        assert_eq!(d("Nov 14").as_deref(), Some("2026-11-14"));
        assert_eq!(d("14-11").as_deref(), Some("2026-11-14"));
        assert_eq!(d("3/1").as_deref(), Some("2027-01-03"), "passed this year, so next year");
        assert_eq!(d("2027-02-01").as_deref(), Some("2027-02-01"));
        assert_eq!(d("1 march 2028").as_deref(), Some("2028-03-01"));
        assert_eq!(d("31 feb"), None);
        assert_eq!(d("soon"), None);
        assert_eq!(d("3"), None);
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
