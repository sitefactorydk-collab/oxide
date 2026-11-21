//! Scheduled tasks (Phase 15): a saved prompt that runs on its own, in a fresh chat each time.
//!
//! Schedules are either a five-field cron expression, read in the computer's local time, or
//! one moment ([`TaskSchedule::Once`]). [`LocalZone`] says how far local time is from UTC at
//! a given moment, so daylight saving time is handled without this crate reading the system
//! clock: the app crate supplies the real zone, tests a fixed or made-up one.

use serde::{Deserialize, Serialize};
use specta::Type;

use crate::Timestamp;
use crate::mcp::ChatTools;

/// Longest task name.
pub const MAX_TASK_NAME_CHARS: usize = 100;
/// Longest prompt.
pub const MAX_TASK_PROMPT_CHARS: usize = 20_000;
/// Most tasks Oxide keeps.
pub const MAX_TASKS: usize = 200;
/// Runs kept per task (older ones are dropped; their chats stay).
pub const MAX_RUNS_KEPT: usize = 100;
/// Characters of a run's reply kept as its output in the history.
pub const MAX_RUN_OUTPUT_CHARS: usize = 4_000;
/// A run more than this late (the computer slept, Oxide was closed) counts as missed.
pub const MISSED_AFTER_MS: i64 = 5 * 60_000;

/// When a task runs.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Type)]
#[serde(tag = "kind", rename_all = "camelCase")]
pub enum TaskSchedule {
    /// Five-field cron (`minute hour day-of-month month day-of-week`) in local time, or a
    /// macro (`@hourly`, `@daily`, `@weekly`, `@monthly`, `@yearly`).
    Cron { expr: String },
    /// One run at this moment.
    Once { at: Timestamp },
}

impl TaskSchedule {
    /// Checks the schedule; the error is a sentence for the user.
    pub fn validate(&self) -> Result<(), String> {
        match self {
            TaskSchedule::Cron { expr } => CronSpec::parse(expr).map(|_| ()),
            TaskSchedule::Once { at } if *at <= 0 => Err("Pick a date and time.".into()),
            TaskSchedule::Once { .. } => Ok(()),
        }
    }

    /// The first run strictly after `after` (Unix ms), or `None` if there's none.
    pub fn next_after(&self, after: Timestamp, zone: &dyn LocalZone) -> Option<Timestamp> {
        match self {
            TaskSchedule::Cron { expr } => CronSpec::parse(expr).ok()?.next_after(after, zone),
            TaskSchedule::Once { at } => (*at > after).then_some(*at),
        }
    }

    /// The next `n` runs after `after`.
    pub fn upcoming(&self, after: Timestamp, n: usize, zone: &dyn LocalZone) -> Vec<Timestamp> {
        let mut out = Vec::new();
        let mut t = after;
        while out.len() < n {
            match self.next_after(t, zone) {
                Some(next) => {
                    out.push(next);
                    t = next;
                }
                None => break,
            }
        }
        out
    }

    pub fn is_once(&self) -> bool {
        matches!(self, TaskSchedule::Once { .. })
    }
}

/// How runs handle tool calls that would ask for approval.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Hash, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub enum ApprovalMode {
    /// As in a chat in Manual: calls that ask wait for the user, and Oxide sends a
    /// notification.
    #[default]
    Ask,
    /// As a chat in Auto (Phase 23): runs on its own; an action the safety check stops
    /// pauses the run as "needs approval" and notifies.
    Auto,
    /// Every tool call runs without asking (saving a skill and computer use still ask).
    AllowAll,
}

impl ApprovalMode {
    /// The value stored in the database. `"auto"` meant "run everything without asking"
    /// before Phase 23, so that stays [`ApprovalMode::AllowAll`]'s.
    pub fn as_str(self) -> &'static str {
        match self {
            ApprovalMode::Ask => "ask",
            ApprovalMode::Auto => "autoCheck",
            ApprovalMode::AllowAll => "auto",
        }
    }

    pub fn parse(s: &str) -> Self {
        match s {
            "auto" => ApprovalMode::AllowAll,
            "autoCheck" => ApprovalMode::Auto,
            _ => ApprovalMode::Ask,
        }
    }
}

/// What happens to a run that was missed (Oxide closed, the computer asleep).
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Hash, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub enum MissedRunPolicy {
    /// Run once as soon as Oxide is back, however many were missed.
    #[default]
    RunOnce,
    /// Note it in the history and wait for the next time.
    Skip,
}

impl MissedRunPolicy {
    pub fn as_str(self) -> &'static str {
        match self {
            MissedRunPolicy::RunOnce => "runOnce",
            MissedRunPolicy::Skip => "skip",
        }
    }

    pub fn parse(s: &str) -> Self {
        match s {
            "skip" => MissedRunPolicy::Skip,
            _ => MissedRunPolicy::RunOnce,
        }
    }
}

/// What a task is made of (create and edit).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct TaskInput {
    pub name: String,
    pub prompt: String,
    pub schedule: TaskSchedule,
    /// `<providerId>/<model>`.
    pub model: String,
    #[serde(default)]
    pub project_id: Option<String>,
    /// Folders each run's chat gets (full paths).
    #[serde(default)]
    pub folders: Vec<String>,
    /// Tool switches each run's chat starts with.
    #[serde(default)]
    pub tools: ChatTools,
    #[serde(default)]
    pub thinking: bool,
    #[serde(default)]
    pub approval: ApprovalMode,
    #[serde(default)]
    pub missed: MissedRunPolicy,
    #[serde(default = "yes")]
    pub enabled: bool,
}

fn yes() -> bool {
    true
}

impl TaskInput {
    /// Trims the text fields and checks them; the error is a sentence for the user.
    pub fn clean(mut self) -> Result<Self, String> {
        self.name = self.name.trim().to_owned();
        self.prompt = self.prompt.trim().to_owned();
        self.model = self.model.trim().to_owned();
        if self.name.is_empty() {
            return Err("Give the task a name.".into());
        }
        if self.name.chars().count() > MAX_TASK_NAME_CHARS {
            return Err(format!(
                "The name is too long (at most {MAX_TASK_NAME_CHARS} characters)."
            ));
        }
        if self.prompt.is_empty() {
            return Err("Write what the task should do.".into());
        }
        if self.prompt.chars().count() > MAX_TASK_PROMPT_CHARS {
            return Err(format!(
                "The prompt is too long (at most {MAX_TASK_PROMPT_CHARS} characters)."
            ));
        }
        if self.model.is_empty() {
            return Err("Pick a model.".into());
        }
        if let TaskSchedule::Cron { expr } = &mut self.schedule {
            *expr = expr.split_whitespace().collect::<Vec<_>>().join(" ");
        }
        self.schedule.validate()?;
        self.project_id = self.project_id.filter(|p| !p.trim().is_empty());
        let mut folders: Vec<String> = Vec::new();
        for f in self.folders.drain(..) {
            let f = f.trim().to_owned();
            if !f.is_empty() && !folders.contains(&f) {
                folders.push(f);
            }
        }
        self.folders = folders;
        Ok(self)
    }
}

/// A saved task.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct ScheduledTask {
    pub id: String,
    pub name: String,
    pub prompt: String,
    pub schedule: TaskSchedule,
    pub model: String,
    pub project_id: Option<String>,
    pub folders: Vec<String>,
    pub tools: ChatTools,
    pub thinking: bool,
    pub approval: ApprovalMode,
    pub missed: MissedRunPolicy,
    /// Off: paused (a one-time task is switched off after its run).
    pub enabled: bool,
    /// When it runs next (`None`: paused, or nothing left to run).
    pub next_run_at: Option<Timestamp>,
    /// The latest run, if any.
    pub last_run: Option<TaskRun>,
    pub created_at: Timestamp,
    pub updated_at: Timestamp,
}

impl ScheduledTask {
    pub fn input(&self) -> TaskInput {
        TaskInput {
            name: self.name.clone(),
            prompt: self.prompt.clone(),
            schedule: self.schedule.clone(),
            model: self.model.clone(),
            project_id: self.project_id.clone(),
            folders: self.folders.clone(),
            tools: self.tools.clone(),
            thinking: self.thinking,
            approval: self.approval,
            missed: self.missed,
            enabled: self.enabled,
        }
    }
}

/// Where a run is.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub enum RunStatus {
    Running,
    /// A tool call waits for the user's approval.
    WaitingApproval,
    /// The reply finished.
    Done,
    /// The reply failed (or Oxide closed while it ran).
    Failed,
    /// Someone pressed Stop.
    Stopped,
    /// Missed while Oxide was closed or the computer slept, and the task skips missed runs.
    Missed,
    /// Not run: the previous run was still going.
    Skipped,
}

impl RunStatus {
    pub fn as_str(self) -> &'static str {
        match self {
            RunStatus::Running => "running",
            RunStatus::WaitingApproval => "waitingApproval",
            RunStatus::Done => "done",
            RunStatus::Failed => "failed",
            RunStatus::Stopped => "stopped",
            RunStatus::Missed => "missed",
            RunStatus::Skipped => "skipped",
        }
    }

    pub fn parse(s: &str) -> Self {
        match s {
            "running" => RunStatus::Running,
            "waitingApproval" => RunStatus::WaitingApproval,
            "done" => RunStatus::Done,
            "stopped" => RunStatus::Stopped,
            "missed" => RunStatus::Missed,
            "skipped" => RunStatus::Skipped,
            _ => RunStatus::Failed,
        }
    }

    /// Still going (running or waiting for the user).
    pub fn is_active(self) -> bool {
        matches!(self, RunStatus::Running | RunStatus::WaitingApproval)
    }
}

/// What started a run.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub enum RunTrigger {
    /// Its time came.
    Schedule,
    /// "Run now".
    Manual,
    /// A run that was missed, made up when Oxide was back.
    CatchUp,
}

impl RunTrigger {
    pub fn as_str(self) -> &'static str {
        match self {
            RunTrigger::Schedule => "schedule",
            RunTrigger::Manual => "manual",
            RunTrigger::CatchUp => "catchUp",
        }
    }

    pub fn parse(s: &str) -> Self {
        match s {
            "manual" => RunTrigger::Manual,
            "catchUp" => RunTrigger::CatchUp,
            _ => RunTrigger::Schedule,
        }
    }
}

/// One run of a task (its history).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct TaskRun {
    pub id: String,
    pub task_id: String,
    /// The chat the run wrote in (`None` if it never started, or the chat was deleted).
    pub conversation_id: Option<String>,
    pub trigger: RunTrigger,
    /// The scheduled moment (`None` for "Run now").
    pub scheduled_for: Option<Timestamp>,
    pub started_at: Timestamp,
    pub finished_at: Option<Timestamp>,
    pub status: RunStatus,
    /// The reply's text (shortened).
    pub output: Option<String>,
    /// Why it failed or wasn't run.
    pub error: Option<String>,
}

/// A task or one of its runs changed (`events.scheduleChanged`).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct ScheduleChange {
    pub task_id: String,
    /// The run that changed, if the change was a run's.
    pub run: Option<TaskRun>,
    /// The task was deleted.
    pub deleted: bool,
}

/// Local time's distance from UTC at a moment (daylight saving time included).
pub trait LocalZone: Send + Sync {
    /// Seconds east of UTC at the Unix-ms moment `utc_ms`.
    fn offset_seconds(&self, utc_ms: Timestamp) -> i32;
}

/// A zone that's always the same distance from UTC.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct FixedZone(pub i32);

impl LocalZone for FixedZone {
    fn offset_seconds(&self, _utc_ms: Timestamp) -> i32 {
        self.0
    }
}

const DAY: i64 = 86_400;

/// A parsed cron expression.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CronSpec {
    minutes: u64,
    hours: u32,
    /// Bits 1–31.
    days: u32,
    /// Bits 1–12.
    months: u16,
    /// Bits 0–6, Sunday = 0.
    weekdays: u8,
    /// The day-of-month field was `*` (or `?`).
    any_day: bool,
    /// The day-of-week field was `*` (or `?`).
    any_weekday: bool,
}

const MONTHS: [&str; 12] = [
    "jan", "feb", "mar", "apr", "may", "jun", "jul", "aug", "sep", "oct", "nov", "dec",
];
const WEEKDAYS: [&str; 7] = ["sun", "mon", "tue", "wed", "thu", "fri", "sat"];

impl CronSpec {
    /// Parses `minute hour day-of-month month day-of-week` (lists, ranges, steps, month and
    /// weekday names, 7 = Sunday) or a macro. The error is a sentence for the user.
    pub fn parse(expr: &str) -> Result<Self, String> {
        let expr = expr.trim();
        let expanded = match expr.to_ascii_lowercase().as_str() {
            "@yearly" | "@annually" => "0 0 1 1 *".to_owned(),
            "@monthly" => "0 0 1 * *".to_owned(),
            "@weekly" => "0 0 * * 0".to_owned(),
            "@daily" | "@midnight" => "0 0 * * *".to_owned(),
            "@hourly" => "0 * * * *".to_owned(),
            s if s.starts_with('@') => {
                return Err(format!("“{expr}” isn't a schedule Oxide knows."));
            }
            _ => expr.to_owned(),
        };
        let fields: Vec<&str> = expanded.split_whitespace().collect();
        if fields.len() != 5 {
            return Err(
                "A cron schedule has five parts: minute, hour, day of the month, month and day \
                 of the week (for example “0 9 * * 1-5”)."
                    .into(),
            );
        }
        let minutes = field(fields[0], 0, 59, &[], "minute")?;
        let hours = field(fields[1], 0, 23, &[], "hour")?;
        let days = field(fields[2], 1, 31, &[], "day of the month")?;
        let months = field(fields[3], 1, 12, &MONTHS, "month")?;
        let mut weekdays = field(fields[4], 0, 7, &WEEKDAYS, "day of the week")?;
        if weekdays & (1 << 7) != 0 {
            weekdays = (weekdays | 1) & 0x7f;
        }
        let star = |f: &str| f == "*" || f == "?";
        let spec = CronSpec {
            minutes,
            hours: u32::try_from(hours).unwrap_or(0),
            days: u32::try_from(days).unwrap_or(0),
            months: u16::try_from(months).unwrap_or(0),
            weekdays: u8::try_from(weekdays).unwrap_or(0),
            any_day: star(fields[2]),
            any_weekday: star(fields[4]),
        };
        // A date that never comes ("30 2" = 30 February).
        if spec.next_after(0, &FixedZone(0)).is_none() {
            return Err("This schedule never runs (that date doesn't exist).".into());
        }
        Ok(spec)
    }

    fn day_matches(&self, days_since_epoch: i64) -> bool {
        let (_, m, d) = civil_from_days(days_since_epoch);
        if (self.months >> m) & 1 == 0 {
            return false;
        }
        let wd = (days_since_epoch + 4).rem_euclid(7); // 1970-01-01 was a Thursday.
        let dom = (self.days >> d) & 1 != 0;
        let dow = (self.weekdays >> wd) & 1 != 0;
        match (self.any_day, self.any_weekday) {
            (true, true) => true,
            (true, false) => dow,
            (false, true) => dom,
            // Like cron: either one matching is enough when both are restricted.
            (false, false) => dom || dow,
        }
    }

    /// The first matching minute strictly after `after` (Unix ms), as Unix ms.
    pub fn next_after(&self, after: Timestamp, zone: &dyn LocalZone) -> Option<Timestamp> {
        let after_s = after.div_euclid(1000);
        let wall_after = after_s + i64::from(zone.offset_seconds(after));
        let start = (wall_after.div_euclid(60) + 1) * 60;
        let mut day = start.div_euclid(DAY);
        let mut from_minute = start.rem_euclid(DAY) / 60;
        // 28 years: every weekday/date combination (and 29 February) comes round.
        for _ in 0..(366 * 28) {
            if self.day_matches(day) {
                for h in (from_minute / 60)..24 {
                    if (self.hours >> h) & 1 == 0 {
                        continue;
                    }
                    let first = if h == from_minute / 60 {
                        from_minute % 60
                    } else {
                        0
                    };
                    for m in first..60 {
                        if (self.minutes >> m) & 1 == 0 {
                            continue;
                        }
                        let wall = day * DAY + h * 3600 + m * 60;
                        if let Some(t) = resolve(wall, zone, after) {
                            return Some(t);
                        }
                    }
                }
            }
            day += 1;
            from_minute = 0;
        }
        None
    }
}

/// The moment (Unix ms) local wall time `wall` (seconds) names, the earliest after `after`.
/// In a daylight-saving gap the time is moved forward by the gap.
fn resolve(wall: i64, zone: &dyn LocalZone, after: Timestamp) -> Option<Timestamp> {
    let before = i64::from(zone.offset_seconds((wall - DAY) * 1000));
    let later = i64::from(zone.offset_seconds((wall + DAY) * 1000));
    let mut valid: Vec<i64> = [before, later]
        .into_iter()
        .map(|o| wall - o)
        .filter(|&u| i64::from(zone.offset_seconds(u * 1000)) == wall - u)
        .collect();
    valid.sort_unstable();
    valid.dedup();
    if valid.is_empty() {
        // In the gap: what the clock says an hour (the gap) later.
        valid.push(wall - before);
    }
    valid.into_iter().map(|u| u * 1000).find(|&t| t > after)
}

fn field(text: &str, min: u32, max: u32, names: &[&str], what: &str) -> Result<u64, String> {
    let bad = || format!("The {what} part “{text}” isn't valid.");
    let value = |s: &str| -> Result<u32, String> {
        let lower = s.to_ascii_lowercase();
        if let Some(i) = names.iter().position(|n| *n == lower) {
            // Month names start at 1, weekday names at 0.
            return Ok(u32::try_from(i).unwrap_or(0) + min);
        }
        let n: u32 = s.parse().map_err(|_| bad())?;
        if n < min || n > max {
            return Err(format!(
                "The {what} must be between {min} and {max} (got {n})."
            ));
        }
        Ok(n)
    };
    let mut bits = 0u64;
    for item in text.split(',') {
        if item.is_empty() {
            return Err(bad());
        }
        let (range, step) = match item.split_once('/') {
            Some((r, s)) => {
                let s: u32 = s.parse().map_err(|_| bad())?;
                if s == 0 {
                    return Err(bad());
                }
                (r, s)
            }
            None => (item, 1),
        };
        let (lo, hi) = if range == "*" || range == "?" {
            (min, max)
        } else if let Some((a, b)) = range.split_once('-') {
            let (a, b) = (value(a)?, value(b)?);
            if a > b {
                return Err(bad());
            }
            (a, b)
        } else {
            let a = value(range)?;
            // "5/15": from 5 to the end, every 15.
            (a, if item.contains('/') { max } else { a })
        };
        let mut v = lo;
        while v <= hi {
            bits |= 1 << v;
            v += step;
        }
    }
    Ok(bits)
}

/// (year, month 1–12, day 1–31) of a day counted from 1970-01-01.
pub fn civil_from_days(z: i64) -> (i64, u32, u32) {
    let z = z + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z - era * 146_097;
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let y = yoe + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let m = if mp < 10 { mp + 3 } else { mp - 9 };
    let y = if m <= 2 { y + 1 } else { y };
    (
        y,
        u32::try_from(m).unwrap_or(1),
        u32::try_from(d).unwrap_or(1),
    )
}

/// Days from 1970-01-01 to a date.
pub fn days_from_civil(y: i64, m: u32, d: u32) -> i64 {
    let (m, d) = (i64::from(m), i64::from(d));
    let y = if m <= 2 { y - 1 } else { y };
    let era = y.div_euclid(400);
    let yoe = y - era * 400;
    let mp = if m > 2 { m - 3 } else { m + 9 };
    let doy = (153 * mp + 2) / 5 + d - 1;
    let doe = yoe * 365 + yoe / 4 - yoe / 100 + doy;
    era * 146_097 + doe - 719_468
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Unix ms of a UTC date and time.
    fn utc(y: i64, mo: u32, d: u32, h: i64, mi: i64) -> i64 {
        (days_from_civil(y, mo, d) * DAY + h * 3600 + mi * 60) * 1000
    }

    /// Copenhagen-like: UTC+1, UTC+2 from the last Sunday of March 01:00 UTC to the last
    /// Sunday of October 01:00 UTC (2026: 29 March, 25 October).
    struct Cph;
    impl LocalZone for Cph {
        fn offset_seconds(&self, t: i64) -> i32 {
            if t >= utc(2026, 3, 29, 1, 0) && t < utc(2026, 10, 25, 1, 0) {
                7200
            } else {
                3600
            }
        }
    }

    fn next(expr: &str, after: i64, zone: &dyn LocalZone) -> i64 {
        TaskSchedule::Cron { expr: expr.into() }
            .next_after(after, zone)
            .expect("has a next run")
    }

    #[test]
    fn civil_dates_round_trip() {
        for d in [-800_000, -1, 0, 1, 10_957, 20_000, 2_000_000] {
            let (y, m, dd) = civil_from_days(d);
            assert_eq!(days_from_civil(y, m, dd), d);
        }
        assert_eq!(civil_from_days(0), (1970, 1, 1));
        assert_eq!(days_from_civil(2026, 11, 13), 20_770);
    }

    #[test]
    fn daily_runs_at_the_local_time() {
        let z = FixedZone(3600);
        // 08:55 local (07:55 UTC) → 09:00 local the same day.
        let now = utc(2026, 11, 13, 7, 55);
        assert_eq!(next("0 9 * * *", now, &z), utc(2026, 11, 13, 8, 0));
        // Exactly at 09:00 local → tomorrow.
        assert_eq!(
            next("0 9 * * *", utc(2026, 11, 13, 8, 0), &z),
            utc(2026, 11, 14, 8, 0)
        );
        assert_eq!(next("@daily", now, &z), utc(2026, 11, 13, 23, 0));
    }

    #[test]
    fn fields_lists_ranges_steps_and_names() {
        let z = FixedZone(0);
        let fri = utc(2026, 11, 13, 10, 0); // Friday
        assert_eq!(next("30 9 * * mon-fri", fri, &z), utc(2026, 11, 16, 9, 30));
        assert_eq!(next("*/15 * * * *", fri, &z), utc(2026, 11, 13, 10, 15));
        assert_eq!(next("5/20 * * * *", fri, &z), utc(2026, 11, 13, 10, 5));
        assert_eq!(next("0 8,18 * * *", fri, &z), utc(2026, 11, 13, 18, 0));
        assert_eq!(next("0 0 1 jan *", fri, &z), utc(2027, 1, 1, 0, 0));
        assert_eq!(next("0 12 * * 7", fri, &z), utc(2026, 11, 15, 12, 0));
        assert_eq!(next("0 12 * * SUN", fri, &z), utc(2026, 11, 15, 12, 0));
        // Day of month OR day of week when both are given.
        assert_eq!(next("0 0 20 * mon", fri, &z), utc(2026, 11, 16, 0, 0));
        assert_eq!(next("0 0 29 2 *", fri, &z), utc(2028, 2, 29, 0, 0));
        assert_eq!(next("@monthly", fri, &z), utc(2026, 12, 1, 0, 0));
        assert_eq!(next("@hourly", fri, &z), utc(2026, 11, 13, 11, 0));
    }

    #[test]
    fn bad_expressions_say_why() {
        for bad in [
            "",
            "* * * *",
            "61 * * * *",
            "* 24 * * *",
            "0 0 0 * *",
            "*/0 * * * *",
            "a * * * *",
            "5-1 * * * *",
            "@sometimes",
            "1,,2 * * * *",
        ] {
            assert!(CronSpec::parse(bad).is_err(), "{bad:?} should be refused");
        }
        let never = CronSpec::parse("0 0 30 2 *").expect_err("never runs");
        assert!(never.contains("never"), "{never}");
    }

    #[test]
    fn daylight_saving_time_keeps_the_wall_clock() {
        // 09:00 every day across the March change: 08:00 UTC before, 07:00 UTC after.
        let sat = utc(2026, 3, 28, 9, 0);
        assert_eq!(next("0 9 * * *", sat, &Cph), utc(2026, 3, 29, 7, 0));
        // 02:30 doesn't exist on 29 March: it runs at 03:30 (01:30 UTC).
        let before = utc(2026, 3, 28, 23, 0);
        assert_eq!(next("30 2 * * *", before, &Cph), utc(2026, 3, 29, 1, 30));
        // 02:30 happens twice on 25 October: once only, the first time.
        let oct = utc(2026, 10, 24, 22, 0);
        let first = next("30 2 * * *", oct, &Cph);
        assert_eq!(first, utc(2026, 10, 25, 0, 30));
        assert_eq!(next("30 2 * * *", first, &Cph), utc(2026, 10, 26, 1, 30));
    }

    #[test]
    fn once_runs_only_in_the_future() {
        let s = TaskSchedule::Once { at: 5_000 };
        assert_eq!(s.next_after(4_999, &FixedZone(0)), Some(5_000));
        assert_eq!(s.next_after(5_000, &FixedZone(0)), None);
        assert_eq!(s.upcoming(0, 3, &FixedZone(0)), vec![5_000]);
        assert!(TaskSchedule::Once { at: 0 }.validate().is_err());
    }

    #[test]
    fn upcoming_lists_the_next_runs() {
        let s = TaskSchedule::Cron {
            expr: "0 9 * * 1-5".into(),
        };
        let fri = utc(2026, 11, 13, 10, 0);
        assert_eq!(
            s.upcoming(fri, 3, &FixedZone(0)),
            vec![
                utc(2026, 11, 16, 9, 0),
                utc(2026, 11, 17, 9, 0),
                utc(2026, 11, 18, 9, 0)
            ]
        );
    }

    #[test]
    fn inputs_are_cleaned_and_checked() {
        let input = TaskInput {
            name: "  Morning brief ".into(),
            prompt: " Summarize the news ".into(),
            schedule: TaskSchedule::Cron {
                expr: " 0  9 * * * ".into(),
            },
            model: "lmstudio/qwen".into(),
            project_id: Some(" ".into()),
            folders: vec!["C:\\a".into(), " C:\\a ".into(), String::new()],
            tools: ChatTools::default(),
            thinking: false,
            approval: ApprovalMode::Ask,
            missed: MissedRunPolicy::RunOnce,
            enabled: true,
        };
        let c = input.clone().clean().expect("valid");
        assert_eq!(c.name, "Morning brief");
        assert_eq!(c.prompt, "Summarize the news");
        assert_eq!(
            c.schedule,
            TaskSchedule::Cron {
                expr: "0 9 * * *".into()
            }
        );
        assert_eq!(c.project_id, None);
        assert_eq!(c.folders, vec!["C:\\a".to_owned()]);
        let mut empty = input.clone();
        empty.prompt = " ".into();
        assert!(empty.clean().is_err());
        let mut bad = input;
        bad.schedule = TaskSchedule::Cron {
            expr: "every day".into(),
        };
        assert!(bad.clean().is_err());
    }

    #[test]
    fn json_shapes() {
        let s = serde_json::to_value(TaskSchedule::Cron {
            expr: "0 9 * * *".into(),
        })
        .expect("json");
        assert_eq!(s, serde_json::json!({"kind": "cron", "expr": "0 9 * * *"}));
        let input: TaskInput = serde_json::from_value(serde_json::json!({
            "name": "x", "prompt": "y", "model": "a/b",
            "schedule": {"kind": "once", "at": 1}
        }))
        .expect("defaults fill the rest");
        assert!(input.enabled);
        assert_eq!(input.approval, ApprovalMode::Ask);
        assert_eq!(input.missed, MissedRunPolicy::RunOnce);
    }
}
