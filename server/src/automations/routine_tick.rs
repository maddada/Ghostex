//! A GBot's routine when it comes due: the pacing rules, its pre-check (precheck.rs) and, only when
//! the check saw something new, the run. Plain automations never come here.
//!
//! CDXC:GBots 2026-11-09 WHY:
//! Routines run unattended, so their cost is capped on the server: a routine's checks and runs come no closer than 15 minutes apart (checked when it is set), it starts at most 24 runs a day, a check that keeps failing backs off from 15 minutes, doubling, to 4 hours, and a run missed while Ghostex was closed is caught up once only when it is less than 12 hours late. Quiet checks fold into one "quiet" row in the run history, so the history shows runs and not every look.

use std::collections::HashSet;
use std::sync::{Arc, Mutex, OnceLock, Weak};

use rusqlite::{params, Connection, OptionalExtension};
use serde_json::Value;

use crate::domain::{DomainRepository, DomainStateError};
use crate::server::AppState;
use crate::storage::open_gxserver_database;

use super::*;

/// The most runs a routine starts in a day.
pub(crate) const ROUTINE_RUNS_PER_DAY: i64 = 24;
/// The shortest gap between a routine's checks or runs.
pub(crate) const ROUTINE_MIN_GAP_MS: i64 = 15 * 60 * 1000;
/// A missed run older than this is skipped rather than caught up.
const CATCH_UP_HOURS: i64 = 12;
/// A failing check's longest back-off.
const BACKOFF_MAX_MINUTES: i64 = 4 * 60;
const QUIET_PREFIX: &str = "Quiet:";

/// Routines whose check is running now, so a slow check is never started twice.
fn checking() -> &'static Mutex<HashSet<String>> {
    static CHECKING: OnceLock<Mutex<HashSet<String>>> = OnceLock::new();
    CHECKING.get_or_init(Default::default)
}

/// gxserver's state, attached once it exists (foreground.rs), for what a routine needs beyond the
/// database: opening a GBot's chat and calling a connector.
pub(super) type AppSlot = Arc<OnceLock<Weak<AppState>>>;

impl AutomationRuntime {
    pub(crate) fn attach_app_state(&self, state: &Arc<AppState>) {
        let _ = self.app.set(Arc::downgrade(state));
    }

    pub(super) fn app_state(&self) -> Option<Arc<AppState>> {
        self.app.get().and_then(Weak::upgrade)
    }

    /// A due routine (an owned automation, or one with a pre-check). Advances its next run first,
    /// so the next tick never picks it again while its check runs.
    pub(super) fn routine_due(
        &self,
        repository: &DomainRepository<'_>,
        db: &Connection,
        automation: &AutomationDefinitionRecord,
    ) -> Result<(), DomainStateError> {
        if checking()
            .lock()
            .map(|set| set.contains(&automation.id))
            .unwrap_or(false)
        {
            return Ok(());
        }
        let due_at = automation.next_run_at.clone().unwrap_or_default();
        update_automation_next_run_at(db, &automation.project_id, &automation.id)?;
        if let Some(bot_id) = automation.owner_bot_id.as_deref() {
            match crate::gbots::routine_gate(&self.paths, db, bot_id) {
                crate::gbots::RoutineGate::Run => {}
                crate::gbots::RoutineGate::Gone => {
                    // The GBot was removed: its routines go with it.
                    db.execute(
                        "DELETE FROM automations WHERE automationId = ?1",
                        [&automation.id],
                    )
                    .map_err(sql_error)?;
                    return Ok(());
                }
                crate::gbots::RoutineGate::Hold(reason) => {
                    return note_quiet(db, automation, &reason);
                }
            }
        }
        let late = chrono::DateTime::parse_from_rfc3339(&due_at)
            .map(|due| chrono::Utc::now().signed_duration_since(due).num_hours() >= CATCH_UP_HOURS)
            .unwrap_or(false);
        if late {
            let run = create_run_record(
                automation,
                "skipped",
                Some("Skipped: it came due while Ghostex was closed, more than 12 hours ago."),
            );
            return upsert_run(db, &run);
        }
        if runs_today(db, &automation.id)? >= ROUTINE_RUNS_PER_DAY {
            return note_quiet(
                db,
                automation,
                "it already ran 24 times today, the most a routine runs in a day",
            );
        }
        let Some(precheck) = automation.precheck.clone() else {
            let _ = queue_automation_run(self, repository, db, automation);
            return Ok(());
        };
        if let Ok(mut set) = checking().lock() {
            set.insert(automation.id.clone());
        }
        let runtime = self.clone();
        let automation = automation.clone();
        tokio::task::spawn_blocking(move || {
            let folder = automation
                .owner_bot_id
                .as_deref()
                .map(|bot_id| crate::gbots::routine_folder(&runtime.paths, bot_id))
                .unwrap_or_default();
            let app = runtime.app_state();
            let outcome = run_precheck(app.as_deref(), &precheck, &folder);
            let _ = runtime.settle_check(&automation, outcome);
            if let Ok(mut set) = checking().lock() {
                set.remove(&automation.id);
            }
        });
        Ok(())
    }

    fn settle_check(
        &self,
        automation: &AutomationDefinitionRecord,
        outcome: Result<String, String>,
    ) -> Result<(), DomainStateError> {
        let db = open_gxserver_database(&self.paths).map_err(internal_error)?;
        let repository = DomainRepository::new(&db, self.server_id.as_str());
        let (previous, streak): (Option<String>, i64) = db
            .query_row(
                "SELECT lastCheckFingerprint, failureStreak FROM automations WHERE automationId = ?1",
                [&automation.id],
                |row| Ok((row.get(0)?, row.get(1)?)),
            )
            .optional()
            .map_err(sql_error)?
            .unwrap_or((None, 0));
        let fingerprint = match outcome {
            Err(error) => {
                let streak = streak + 1;
                let minutes = (15_i64 << (streak - 1).min(5)).min(BACKOFF_MAX_MINUTES);
                let retry = chrono::Utc::now() + chrono::Duration::minutes(minutes);
                let retry_at = retry.to_rfc3339_opts(chrono::SecondsFormat::Millis, true);
                db.execute(
                    "UPDATE automations SET failureStreak = ?2, \
                     nextRunAt = CASE WHEN nextRunAt IS NULL OR nextRunAt < ?3 THEN ?3 ELSE nextRunAt END \
                     WHERE automationId = ?1 AND enabled = 1",
                    params![automation.id, streak, retry_at],
                )
                .map_err(sql_error)?;
                record_check(&db, &automation.id, "failed", &error, Some(&retry_at))?;
                let next: Option<String> = db
                    .query_row(
                        "SELECT nextRunAt FROM automations WHERE automationId = ?1",
                        [&automation.id],
                        |row| row.get(0),
                    )
                    .optional()
                    .map_err(sql_error)?
                    .flatten();
                let when = if next.as_deref() == Some(retry_at.as_str()) {
                    format!("It tries again in {minutes} minutes.")
                } else {
                    "It tries again at its next run.".to_string()
                };
                let run = create_run_record(
                    automation,
                    "skipped",
                    Some(&format!("The check failed: {error} {when}")),
                );
                return upsert_run(&db, &run);
            }
            Ok(fingerprint) => fingerprint,
        };
        db.execute(
            "UPDATE automations SET lastCheckFingerprint = ?2, failureStreak = 0 WHERE automationId = ?1",
            params![automation.id, fingerprint],
        )
        .map_err(sql_error)?;
        match previous {
            None => {
                record_check(&db, &automation.id, "baseline", "Started watching.", None)?;
                note_quiet(
                    &db,
                    automation,
                    "started watching; it runs when something changes",
                )
            }
            Some(previous) if previous == fingerprint => {
                record_check(&db, &automation.id, "quiet", "Nothing changed.", None)?;
                note_quiet(&db, automation, "nothing changed")
            }
            Some(_) => {
                record_check(&db, &automation.id, "changed", "Something changed.", None)?;
                db.execute(
                    "UPDATE gbots_routine_checks SET quietChecks = 0 WHERE automationId = ?1",
                    [&automation.id],
                )
                .map_err(sql_error)?;
                let fresh = read_automation_by_id(&db, &automation.id)?
                    .unwrap_or_else(|| automation.clone());
                queue_automation_run(self, &repository, &db, &fresh).map(|_| ())
            }
        }
    }

    /// A finished run of a routine: its result goes where the routine says (gbots/routine_runs.rs).
    pub(super) fn routine_run_settled(&self, project_id: &str, run_id: &str) {
        let Some(app) = self.app_state() else {
            return;
        };
        let Ok(db) = open_gxserver_database(&self.paths) else {
            return;
        };
        let Ok(run) = read_run(&db, project_id, run_id) else {
            return;
        };
        let Ok(Some(automation)) = read_automation_by_id(&db, &run.automation_id) else {
            return;
        };
        let Some(bot_id) = automation.owner_bot_id.as_deref() else {
            return;
        };
        crate::gbots::routine_run_settled(
            &app,
            &db,
            crate::gbots::SettledRoutineRun {
                bot_id,
                automation_id: &automation.id,
                name: &automation.name,
                deliver: automation.deliver.as_deref().unwrap_or("chat"),
                status: &run.status,
                summary: run
                    .findings_summary
                    .as_deref()
                    .or(run.error_message.as_deref())
                    .unwrap_or_default(),
                session: run.session_id.as_deref(),
            },
        );
    }
}

pub(super) fn read_automation_by_id(
    db: &Connection,
    automation_id: &str,
) -> Result<Option<AutomationDefinitionRecord>, DomainStateError> {
    let project_id: Option<String> = db
        .query_row(
            "SELECT projectId FROM automations WHERE automationId = ?1",
            [automation_id],
            |row| row.get(0),
        )
        .optional()
        .map_err(sql_error)?;
    let Some(project_id) = project_id else {
        return Ok(None);
    };
    let project = ProjectRecord {
        name: String::new(),
        path: String::new(),
        project_id,
    };
    read_automation(db, &project, automation_id).map(Some)
}

/// Runs a routine started (not skipped) in the last 24 hours.
fn runs_today(db: &Connection, automation_id: &str) -> Result<i64, DomainStateError> {
    let since = (chrono::Utc::now() - chrono::Duration::hours(24))
        .to_rfc3339_opts(chrono::SecondsFormat::Millis, true);
    db.query_row(
        "SELECT COUNT(*) FROM automation_runs WHERE automationId = ?1 AND status != 'skipped' AND createdAt >= ?2",
        params![automation_id, since],
        |row| row.get(0),
    )
    .map_err(sql_error)
}

fn record_check(
    db: &Connection,
    automation_id: &str,
    outcome: &str,
    note: &str,
    retry_at: Option<&str>,
) -> Result<(), DomainStateError> {
    db.execute(
        "INSERT INTO gbots_routine_checks (automationId, lastCheckAt, lastCheckOutcome, lastCheckNote, retryAt) \
         VALUES (?1, ?2, ?3, ?4, ?5) \
         ON CONFLICT(automationId) DO UPDATE SET lastCheckAt = excluded.lastCheckAt, \
         lastCheckOutcome = excluded.lastCheckOutcome, lastCheckNote = excluded.lastCheckNote, \
         retryAt = excluded.retryAt, \
         quietChecks = quietChecks + CASE WHEN excluded.lastCheckOutcome = 'quiet' THEN 1 ELSE 0 END",
        params![automation_id, now_iso(), outcome, note, retry_at],
    )
    .map_err(sql_error)?;
    Ok(())
}

/// Folds a tick that started nothing into the newest run row when that is a quiet one too, so the
/// history reads "Quiet: nothing changed (12 checks)" instead of twelve rows.
fn note_quiet(
    db: &Connection,
    automation: &AutomationDefinitionRecord,
    reason: &str,
) -> Result<(), DomainStateError> {
    let newest: Option<(String, String, i64)> = db
        .query_row(
            "SELECT runId, COALESCE(errorMessage, ''), \
             (SELECT quietChecks FROM gbots_routine_checks WHERE automationId = ?1) \
             FROM automation_runs WHERE automationId = ?1 \
             ORDER BY createdAt DESC, rowid DESC LIMIT 1",
            [&automation.id],
            |row| {
                Ok((
                    row.get(0)?,
                    row.get(1)?,
                    row.get::<_, Option<i64>>(2)?.unwrap_or(0),
                ))
            },
        )
        .optional()
        .map_err(sql_error)?;
    let checks = |count: i64| {
        if count > 1 {
            format!(" ({count} checks)")
        } else {
            String::new()
        }
    };
    if let Some((run_id, message, count)) = newest {
        if message.starts_with(QUIET_PREFIX) {
            db.execute(
                "UPDATE automation_runs SET errorMessage = ?2, completedAt = ?3, updatedAt = ?3 WHERE runId = ?1",
                params![run_id, format!("{QUIET_PREFIX} {reason}{}.", checks(count)), now_iso()],
            )
            .map_err(sql_error)?;
            return Ok(());
        }
    }
    let run = create_run_record(
        automation,
        "skipped",
        Some(&format!("{QUIET_PREFIX} {reason}.")),
    );
    upsert_run(db, &run)
}

/// The routine's check state, for the routine list (gbots/routines.rs).
pub(crate) fn routine_check_state(db: &Connection, automation_id: &str) -> Value {
    db.query_row(
        "SELECT lastCheckAt, lastCheckOutcome, lastCheckNote, quietChecks, retryAt \
         FROM gbots_routine_checks WHERE automationId = ?1",
        [automation_id],
        |row| {
            Ok(serde_json::json!({
                "lastCheckAt": row.get::<_, Option<String>>(0)?,
                "lastCheckOutcome": row.get::<_, Option<String>>(1)?,
                "lastCheckNote": row.get::<_, Option<String>>(2)?,
                "quietChecks": row.get::<_, i64>(3)?,
                "retryAt": row.get::<_, Option<String>>(4)?,
            }))
        },
    )
    .optional()
    .ok()
    .flatten()
    .unwrap_or(Value::Null)
}
