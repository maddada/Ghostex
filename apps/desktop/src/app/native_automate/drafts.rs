//! The create/edit form's draft and the schedule wording, ported from the deleted
//! apps/desktop/views/project-board/automations-drafts.ts (in git history) so the native dialog
//! builds the same definition the React dialog saved.

use super::model::{AutomationDefinition, AutomationExecutionMode, AutomationSchedule};
use chrono::{DateTime, Datelike as _, Local, NaiveDateTime, TimeZone as _, Timelike as _, Utc};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum SchedulePreset {
    Every5m,
    Every15m,
    Every30m,
    Hourly,
    Every6h,
    Every12h,
    Daily,
    Weekdays,
    Weekly,
    Cron,
}

/// `AUTOMATION_SCHEDULE_PRESETS`, in menu order.
pub(crate) const SCHEDULE_PRESETS: [SchedulePreset; 10] = [
    SchedulePreset::Every5m,
    SchedulePreset::Every15m,
    SchedulePreset::Every30m,
    SchedulePreset::Hourly,
    SchedulePreset::Every6h,
    SchedulePreset::Every12h,
    SchedulePreset::Daily,
    SchedulePreset::Weekdays,
    SchedulePreset::Weekly,
    SchedulePreset::Cron,
];

impl SchedulePreset {
    pub(crate) fn label(self) -> &'static str {
        match self {
            Self::Every5m => "Every 5 minutes",
            Self::Every15m => "Every 15 minutes",
            Self::Every30m => "Every 30 minutes",
            Self::Hourly => "Hourly",
            Self::Every6h => "Every 6 hours",
            Self::Every12h => "Every 12 hours",
            Self::Daily => "Daily",
            Self::Weekdays => "Weekdays",
            Self::Weekly => "Weekly",
            Self::Cron => "Custom cron",
        }
    }

    /// `AUTOMATION_INTERVAL_MS_BY_PRESET`.
    pub(crate) fn interval_ms(self) -> Option<f64> {
        const MINUTE: f64 = 60.0 * 1000.0;
        match self {
            Self::Every5m => Some(5.0 * MINUTE),
            Self::Every15m => Some(15.0 * MINUTE),
            Self::Every30m => Some(30.0 * MINUTE),
            Self::Hourly => Some(60.0 * MINUTE),
            Self::Every6h => Some(6.0 * 60.0 * MINUTE),
            Self::Every12h => Some(12.0 * 60.0 * MINUTE),
            _ => None,
        }
    }

    pub(crate) fn uses_time(self) -> bool {
        matches!(self, Self::Daily | Self::Weekly | Self::Weekdays)
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum ScheduleMode {
    Repeat,
    Timer,
    Date,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum TimerUnit {
    Minutes,
    Hours,
    Days,
}

pub(crate) const TIMER_UNITS: [TimerUnit; 3] =
    [TimerUnit::Minutes, TimerUnit::Hours, TimerUnit::Days];

impl TimerUnit {
    pub(crate) fn label(self) -> &'static str {
        match self {
            Self::Minutes => "Minutes",
            Self::Hours => "Hours",
            Self::Days => "Days",
        }
    }

    fn ms(self) -> f64 {
        match self {
            Self::Minutes => 60.0 * 1000.0,
            Self::Hours => 60.0 * 60.0 * 1000.0,
            Self::Days => 24.0 * 60.0 * 60.0 * 1000.0,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum ExecutionKind {
    Worktree,
    Local,
    Thread,
}

/// `AUTOMATION_WEEKDAY_OPTIONS`, Sunday first (the index is the schedule's day number).
pub(crate) const WEEKDAYS: [&str; 7] = [
    "Sunday",
    "Monday",
    "Tuesday",
    "Wednesday",
    "Thursday",
    "Friday",
    "Saturday",
];

/// `AutomationDraft`. The text fields hold exactly what the user typed.
#[derive(Clone, Debug)]
pub(crate) struct AutomationDraft {
    pub(crate) id: Option<String>,
    pub(crate) created_at: Option<String>,
    pub(crate) agent_id: String,
    pub(crate) cron_expression: String,
    pub(crate) enabled: bool,
    pub(crate) execution_kind: ExecutionKind,
    pub(crate) expires_at: String,
    /// The model a run's new session starts on (the catalog's model value); empty keeps the
    /// agent's own.
    pub(crate) model: String,
    /// The effort a run's new session starts on; empty keeps the agent's own.
    pub(crate) effort: String,
    pub(crate) name: String,
    pub(crate) prompt: String,
    pub(crate) project_id: String,
    pub(crate) run_at: String,
    pub(crate) schedule_mode: ScheduleMode,
    pub(crate) schedule_preset: SchedulePreset,
    pub(crate) schedule_time: String,
    pub(crate) setup_command: String,
    pub(crate) timer_amount: String,
    pub(crate) timer_unit: TimerUnit,
    pub(crate) thread_agent_session_id: String,
    pub(crate) thread_session_id: String,
    pub(crate) weekly_day: usize,
}

impl AutomationDraft {
    /// `createAutomationDraft()` defaults.
    pub(crate) fn new(agent_id: String, project_id: String, can_use_worktrees: bool) -> Self {
        Self {
            id: None,
            created_at: None,
            agent_id,
            cron_expression: "*/15 * * * *".to_string(),
            enabled: true,
            execution_kind: if can_use_worktrees {
                ExecutionKind::Worktree
            } else {
                ExecutionKind::Local
            },
            expires_at: String::new(),
            model: String::new(),
            effort: String::new(),
            name: String::new(),
            prompt: String::new(),
            project_id,
            run_at: String::new(),
            schedule_mode: ScheduleMode::Repeat,
            schedule_preset: SchedulePreset::Every15m,
            schedule_time: "09:00".to_string(),
            setup_command: String::new(),
            timer_amount: "30".to_string(),
            timer_unit: TimerUnit::Minutes,
            thread_agent_session_id: String::new(),
            thread_session_id: String::new(),
            weekly_day: 1,
        }
    }

    /// `createAutomationDraftFromDefinition`.
    pub(crate) fn from_definition(definition: &AutomationDefinition, project_id: String) -> Self {
        let mut draft = Self::new(definition.agent_id.clone(), project_id, true);
        draft.id = Some(definition.id.clone());
        draft.created_at = definition.created_at.clone();
        draft.enabled = definition.enabled;
        draft.name = definition.name.clone();
        draft.prompt = definition.prompt.clone();
        draft.model = definition.model.clone().unwrap_or_default();
        draft.effort = definition.effort.clone().unwrap_or_default();
        draft.schedule_preset = schedule_preset_for(&definition.schedule);
        match &definition.schedule {
            AutomationSchedule::Once { run_at } => {
                draft.run_at = iso_to_local_input(run_at);
                draft.schedule_mode = ScheduleMode::Date;
            }
            AutomationSchedule::Weekly { days, time, .. } => {
                draft.schedule_time = time.clone();
                draft.weekly_day = days.first().copied().unwrap_or(1).min(6) as usize;
            }
            AutomationSchedule::Daily { time, .. } => draft.schedule_time = time.clone(),
            AutomationSchedule::Cron { expression, .. } => {
                draft.cron_expression = expression.clone();
            }
            AutomationSchedule::Interval { .. } => {}
        }
        match &definition.execution_mode {
            AutomationExecutionMode::Local => draft.execution_kind = ExecutionKind::Local,
            AutomationExecutionMode::Worktree { setup_command } => {
                draft.execution_kind = ExecutionKind::Worktree;
                draft.setup_command = setup_command.clone().unwrap_or_default();
            }
            AutomationExecutionMode::Thread {
                agent_session_id,
                expires_at,
                session_id,
            } => {
                draft.execution_kind = ExecutionKind::Thread;
                draft.thread_agent_session_id = agent_session_id.clone().unwrap_or_default();
                draft.thread_session_id = session_id.clone().unwrap_or_default();
                draft.expires_at = expires_at
                    .as_deref()
                    .map(iso_to_local_input)
                    .unwrap_or_default();
            }
        }
        draft
    }

    /// `createAutomationScheduleFromDraft`.
    pub(crate) fn schedule(&self) -> Option<AutomationSchedule> {
        match self.schedule_mode {
            ScheduleMode::Timer => {
                let amount: f64 = self.timer_amount.trim().parse().ok()?;
                let delay_ms = amount * self.timer_unit.ms();
                if !delay_ms.is_finite() || !(60_000.0..=365.0 * 86_400_000.0).contains(&delay_ms) {
                    return None;
                }
                let run_at = Utc::now() + chrono::Duration::milliseconds(delay_ms as i64);
                Some(AutomationSchedule::Once {
                    run_at: iso_string(run_at),
                })
            }
            ScheduleMode::Date => Some(AutomationSchedule::Once {
                run_at: local_input_to_iso(&self.run_at)?,
            }),
            ScheduleMode::Repeat => {
                let timezone = local_timezone_name();
                if let Some(every_ms) = self.schedule_preset.interval_ms() {
                    return Some(AutomationSchedule::Interval { every_ms });
                }
                let time = valid_time(&self.schedule_time);
                Some(match self.schedule_preset {
                    SchedulePreset::Cron => {
                        let expression = self.cron_expression.trim().to_string();
                        if expression.is_empty() {
                            return None;
                        }
                        AutomationSchedule::Cron {
                            expression,
                            timezone,
                        }
                    }
                    SchedulePreset::Weekly => AutomationSchedule::Weekly {
                        days: vec![self.weekly_day as u32],
                        time: time?,
                        timezone,
                    },
                    SchedulePreset::Weekdays => AutomationSchedule::Weekly {
                        days: vec![1, 2, 3, 4, 5],
                        time: time?,
                        timezone,
                    },
                    _ => AutomationSchedule::Daily {
                        time: time?,
                        timezone,
                    },
                })
            }
        }
    }

    fn execution_mode(&self) -> AutomationExecutionMode {
        let optional = |value: &str| Some(value.trim().to_string()).filter(|v| !v.is_empty());
        match self.execution_kind {
            ExecutionKind::Local => AutomationExecutionMode::Local,
            ExecutionKind::Thread => AutomationExecutionMode::Thread {
                agent_session_id: optional(&self.thread_agent_session_id),
                expires_at: local_input_to_iso(&self.expires_at),
                session_id: optional(&self.thread_session_id),
            },
            ExecutionKind::Worktree => AutomationExecutionMode::Worktree {
                setup_command: optional(&self.setup_command),
            },
        }
    }

    /// `createAutomationDefinitionFromDraft`: the `payloadJson` of `automationSave`, or `None`
    /// when a required field is missing (the caller shows the React page's message).
    pub(crate) fn to_definition_json(
        &self,
        fallback_agent_id: &str,
        project_id: &str,
    ) -> Option<serde_json::Value> {
        let name = self.name.trim();
        let prompt = self.prompt.trim();
        let agent_id = Some(self.agent_id.trim())
            .filter(|id| !id.is_empty())
            .unwrap_or(fallback_agent_id.trim());
        let schedule = self.schedule()?;
        if name.is_empty() || prompt.is_empty() || agent_id.is_empty() {
            return None;
        }
        let execution_mode = self.execution_mode();
        if let AutomationExecutionMode::Thread {
            agent_session_id: None,
            session_id: None,
            ..
        } = execution_mode
        {
            return None;
        }
        let now = iso_string(Utc::now());
        let mut definition = serde_json::json!({
            "agentId": agent_id,
            "createdAt": self.created_at.clone().unwrap_or_else(|| now.clone()),
            "enabled": self.enabled,
            "executionMode": execution_mode.to_json(),
            "id": self
                .id
                .clone()
                .unwrap_or_else(|| format!("automation-{}", uuid::Uuid::new_v4())),
            "name": name,
            "projectIds": [project_id],
            "prompt": prompt,
            "schedule": schedule.to_json(),
            "updatedAt": now,
        });
        // A thread run uses a session that already exists, which keeps its own model and effort.
        if self.execution_kind != ExecutionKind::Thread {
            for (key, value) in [("model", &self.model), ("effort", &self.effort)] {
                let value = value.trim();
                if !value.is_empty() {
                    definition[key] = serde_json::Value::String(value.to_string());
                }
            }
        }
        if self.enabled
            && let Some(next_run_at) = compute_next_run_at(&schedule)
        {
            definition["nextRunAt"] = serde_json::Value::String(next_run_at);
        }
        Some(definition)
    }
}

/// `resolveAutomationSchedulePreset`.
fn schedule_preset_for(schedule: &AutomationSchedule) -> SchedulePreset {
    match schedule {
        AutomationSchedule::Once { .. } => SchedulePreset::Every15m,
        AutomationSchedule::Interval { every_ms } => SCHEDULE_PRESETS
            .into_iter()
            .find(|preset| preset.interval_ms() == Some(*every_ms))
            .unwrap_or(SchedulePreset::Hourly),
        AutomationSchedule::Weekly { days, .. } if is_weekdays(days) => SchedulePreset::Weekdays,
        AutomationSchedule::Weekly { .. } => SchedulePreset::Weekly,
        AutomationSchedule::Daily { .. } => SchedulePreset::Daily,
        AutomationSchedule::Cron { .. } => SchedulePreset::Cron,
    }
}

fn is_weekdays(days: &[u32]) -> bool {
    days.len() == 5 && (1..=5).all(|day| days.contains(&day))
}

/// `describeAutomationSchedule`.
pub(crate) fn describe_schedule(schedule: &AutomationSchedule) -> String {
    match schedule {
        AutomationSchedule::Once { run_at } => format!(
            "Once on {}",
            parse_iso(run_at)
                .map(|at| at
                    .with_timezone(&Local)
                    .format("%-m/%-d/%Y, %-I:%M:%S %p")
                    .to_string())
                .unwrap_or_else(|| "Invalid Date".to_string())
        ),
        AutomationSchedule::Interval { every_ms } => {
            if let Some(preset) = SCHEDULE_PRESETS
                .into_iter()
                .find(|preset| preset.interval_ms() == Some(*every_ms))
            {
                return preset.label().to_string();
            }
            let hour_ms = 3_600_000.0;
            if every_ms % hour_ms == 0.0 {
                let hours = every_ms / hour_ms;
                if hours == 1.0 {
                    "Hourly".to_string()
                } else {
                    format!("Every {hours} hours")
                }
            } else {
                format!("Every {} minutes", (every_ms / 60_000.0).round())
            }
        }
        AutomationSchedule::Daily { time, .. } => format!("Daily at {time}"),
        AutomationSchedule::Weekly { days, time, .. } if is_weekdays(days) => {
            format!("Weekdays at {time}")
        }
        AutomationSchedule::Weekly { days, time, .. } => format!(
            "Weekly {} at {time}",
            WEEKDAYS
                .get(days.first().copied().unwrap_or(0) as usize)
                .copied()
                .unwrap_or("Weekly")
        ),
        AutomationSchedule::Cron { expression, .. } => expression.clone(),
    }
}

/// `describeAutomationMode`.
pub(crate) fn describe_mode(mode: &AutomationExecutionMode) -> &'static str {
    match mode {
        AutomationExecutionMode::Worktree { .. } => "Worktree",
        AutomationExecutionMode::Thread { .. } => "Thread",
        AutomationExecutionMode::Local => "Local checkout",
    }
}

/// `automationRunStatusLabel`.
pub(crate) fn run_status_label(status: &str) -> String {
    match status {
        "no_findings" => "No findings".to_string(),
        "needs_attention" => "Needs attention".to_string(),
        other => {
            let label = other.replace('_', " ");
            let mut chars = label.chars();
            match chars.next() {
                Some(first) => first.to_uppercase().chain(chars).collect(),
                None => String::new(),
            }
        }
    }
}

pub(crate) fn parse_iso(value: &str) -> Option<DateTime<Utc>> {
    DateTime::parse_from_rfc3339(value.trim())
        .ok()
        .map(|at| at.with_timezone(&Utc))
}

pub(crate) fn parse_iso_millis(value: &str) -> Option<i64> {
    parse_iso(value).map(|at| at.timestamp_millis())
}

/// `formatShortDate`: "Sep 23" in the local timezone, empty for a missing or bad date.
pub(crate) fn format_short_date(value: Option<&str>) -> String {
    value
        .and_then(parse_iso)
        .map(|at| at.with_timezone(&Local).format("%b %-d").to_string())
        .unwrap_or_default()
}

/// `Date.prototype.toISOString`.
fn iso_string(at: DateTime<Utc>) -> String {
    at.to_rfc3339_opts(chrono::SecondsFormat::Millis, true)
}

/// `toDatetimeLocalValue`, written with a space instead of the `T` a browser picker hides.
fn iso_to_local_input(value: &str) -> String {
    parse_iso(value)
        .map(|at| {
            at.with_timezone(&Local)
                .format("%Y-%m-%d %H:%M")
                .to_string()
        })
        .unwrap_or_default()
}

/// `datetimeLocalToIso`: accepts `YYYY-MM-DD HH:MM` or the `T` form, seconds optional.
pub(crate) fn local_input_to_iso(value: &str) -> Option<String> {
    let trimmed = value.trim();
    if trimmed.is_empty() {
        return None;
    }
    let naive = [
        "%Y-%m-%d %H:%M",
        "%Y-%m-%dT%H:%M",
        "%Y-%m-%d %H:%M:%S",
        "%Y-%m-%dT%H:%M:%S",
    ]
    .into_iter()
    .find_map(|format| NaiveDateTime::parse_from_str(trimmed, format).ok())?;
    Local
        .from_local_datetime(&naive)
        .earliest()
        .map(|at| iso_string(at.with_timezone(&Utc)))
}

/// A `HH:MM` wall time (`TIME_PATTERN`), padded when typed as `9:05`.
fn valid_time(value: &str) -> Option<String> {
    let (hour, minute) = value.trim().split_once(':')?;
    let hour: u32 = hour.trim().parse().ok()?;
    let minute: u32 = minute.trim().parse().ok()?;
    (hour <= 23 && minute <= 59).then(|| format!("{hour:02}:{minute:02}"))
}

/// The IANA name `Intl.DateTimeFormat().resolvedOptions().timeZone` reports, read from the
/// `/etc/localtime` link; `local` (the shared normalizer's own fallback) when it cannot be read.
fn local_timezone_name() -> String {
    if let Ok(zone) = std::env::var("TZ")
        && !zone.trim().is_empty()
        && zone.contains('/')
    {
        return zone.trim_start_matches(':').to_string();
    }
    std::fs::read_link("/etc/localtime")
        .ok()
        .and_then(|target| {
            let target = target.to_string_lossy().to_string();
            target
                .split_once("zoneinfo/")
                .map(|(_, zone)| zone.to_string())
        })
        .filter(|zone| !zone.is_empty())
        .unwrap_or_else(|| "local".to_string())
}

/// `computeNextRunAt` for the schedules the dialog creates, in local wall time. A cron shape the
/// simple matcher does not know returns `None`, and gxserver computes the next run itself.
fn compute_next_run_at(schedule: &AutomationSchedule) -> Option<String> {
    let now = Utc::now();
    match schedule {
        AutomationSchedule::Once { run_at } => {
            parse_iso(run_at).filter(|at| *at > now).map(iso_string)
        }
        AutomationSchedule::Interval { every_ms } => Some(iso_string(
            now + chrono::Duration::milliseconds(*every_ms as i64),
        )),
        AutomationSchedule::Daily { time, .. } => {
            let (hour, minute) = split_time(time)?;
            next_local_occurrence(hour, minute, &[0, 1, 2, 3, 4, 5, 6])
        }
        AutomationSchedule::Weekly { days, time, .. } => {
            let (hour, minute) = split_time(time)?;
            next_local_occurrence(hour, minute, days)
        }
        AutomationSchedule::Cron { expression, .. } => next_simple_cron(expression),
    }
}

fn split_time(time: &str) -> Option<(u32, u32)> {
    let (hour, minute) = time.split_once(':')?;
    Some((hour.parse().ok()?, minute.parse().ok()?))
}

fn next_local_occurrence(hour: u32, minute: u32, days: &[u32]) -> Option<String> {
    let now = Local::now();
    (0..=7).find_map(|offset| {
        let date = now.date_naive() + chrono::Duration::days(offset);
        if !days.contains(&date.weekday().num_days_from_sunday()) {
            return None;
        }
        let candidate = Local
            .from_local_datetime(&date.and_hms_opt(hour, minute, 0)?)
            .earliest()?;
        (candidate > now).then(|| iso_string(candidate.with_timezone(&Utc)))
    })
}

fn next_simple_cron(expression: &str) -> Option<String> {
    let parts: Vec<&str> = expression.split_whitespace().collect();
    if parts.len() != 5 {
        return None;
    }
    if let Some(step) = parts[0]
        .strip_prefix("*/")
        .and_then(|step| step.parse::<u32>().ok())
        .filter(|step| *step > 0 && *step < 60)
        && parts[1..].iter().all(|part| *part == "*")
    {
        let now = Local::now();
        let next_minute = (now.minute() / step + 1) * step;
        let base = now.with_second(0)?.with_nanosecond(0)?.with_minute(0)?;
        let next = base + chrono::Duration::minutes(i64::from(next_minute));
        return Some(iso_string(next.with_timezone(&Utc)));
    }
    let minute: u32 = parts[0].parse().ok()?;
    let hour: u32 = parts[1].parse().ok()?;
    if parts[2] != "*" || parts[3] != "*" || hour > 23 || minute > 59 {
        return None;
    }
    let days: Vec<u32> = if parts[4] == "*" {
        (0..7).collect()
    } else {
        parts[4]
            .split(',')
            .map(|day| day.parse::<u32>().ok().map(|day| day % 7))
            .collect::<Option<Vec<_>>>()?
    };
    next_local_occurrence(hour, minute, &days)
}
