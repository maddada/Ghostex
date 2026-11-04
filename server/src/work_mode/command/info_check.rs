//! The hourly `info` check: a card pass queues one for each workspace whose command it reads and
//! whose accepted `info` is older than `INFO_RECHECK`, and a background worker checks the command
//! again as `ghostex issues source check` does (`recheck_work_source`), holding the store's writer
//! lock until `info` starts. An answer the check refuses (another id, another id format, a stored
//! value in its id, ID keys or credential names) changes nothing but what a check always writes,
//! and a field it newly marks secret has its short saved value removed, as an asked check does.
//!
//! A check that didn't move the command's `checkedAt` (it failed, or was refused) isn't tried
//! again for `INFO_RECHECK` at the generation it left, so a failing command isn't asked every
//! pass. A path or credential change checks the command at once on its own. The tries are kept in
//! memory, so a gxserver start forgets them: a failing command is asked once an hour, and once
//! after each start.

use std::collections::HashMap;
use std::sync::{Mutex, MutexGuard, OnceLock};
use std::time::Instant;

use serde_json::json;

use crate::logging::{GxserverLogInput, GxserverLogger, LogLevel};
use crate::paths::GxserverPaths;

use super::*;

/// The hourly check of one workspace's command.
pub(crate) struct InfoCheck {
    pub(crate) plan: JobPlan,
    pub(crate) paths: GxserverPaths,
}

impl std::fmt::Debug for InfoCheck {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("InfoCheck")
            .field("plan", &self.plan)
            .finish_non_exhaustive()
    }
}

/// When the hourly check last ran, by workspace and source, and the generation it left.
type Attempts = HashMap<(String, String), (u64, Instant)>;

fn attempts() -> MutexGuard<'static, Attempts> {
    static ATTEMPTS: OnceLock<Mutex<Attempts>> = OnceLock::new();
    ATTEMPTS
        .get_or_init(|| Mutex::new(HashMap::new()))
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
}

/// Whether the workspace's command, last checked at `checked_at`, is due for its hourly check at
/// `plan`'s generation: its accepted `info` is older than `INFO_RECHECK`, and no hourly check of
/// that source left that generation within it.
pub(crate) fn info_check_wanted(plan: &JobPlan, checked_at: &str) -> bool {
    let tried_lately = attempts()
        .get(&(plan.workspace_id.clone(), plan.source.as_str().to_string()))
        .is_some_and(|(generation, at)| {
            *generation == plan.generation && at.elapsed() < INFO_RECHECK
        });
    !tried_lately && info_check_due(checked_at, chrono::Utc::now())
}

/// Runs one hourly check and logs, once, what went wrong, without the command's own words.
pub(crate) fn run_info_check(job: &InfoCheck) {
    let _slot = slots().take(SlotUse::Background);
    let Ok(db) = crate::storage::open_gxserver_database(&job.paths) else {
        // Tried, as a check that failed: the card pass would otherwise queue it again every pass.
        record_try(&job.plan, job.plan.generation);
        return;
    };
    let repository = crate::domain::DomainRepository::new(&db, "");
    let references = || {
        referenced_source_ids(
            &repository,
            &crate::workspaces::read_sidebar_workspaces(&db)?,
        )
    };
    // One the setup changed under answers `None`, and leaves the new generation due.
    let Some(HourlyCheck {
        change,
        tried_generation,
    }) = recheck_work_source(&db, &job.paths, &job.plan, &references)
    else {
        return;
    };
    record_try(&job.plan, tried_generation);
    let logger = GxserverLogger::new(job.paths.clone());
    match &change.result {
        Ok(_) => {}
        Err(WorkSourceError::Failed(failure)) => log_command_failure(
            &logger,
            CommandVerb::Info,
            &failure.failure,
            failure.duration,
            &failure.workspace_id,
            failure.source_id.as_deref(),
        ),
        Err(_) => log_info_check_refused(
            &logger,
            &change.mask.shown(&job.plan.workspace_id),
            &change.mask.shown(job.plan.source.as_str()),
        ),
    }
}

/// Records an hourly check of `plan`'s workspace and source as tried now, leaving `generation`.
fn record_try(plan: &JobPlan, generation: u64) {
    attempts().insert(
        (plan.workspace_id.clone(), plan.source.as_str().to_string()),
        (generation, Instant::now()),
    );
}

/// One error line in fixed words when an hourly check was refused or couldn't be kept: the
/// refusal's own words can quote what the command answered.
fn log_info_check_refused(logger: &GxserverLogger, workspace_id: &str, source_id: &str) {
    let _ = logger.log(GxserverLogInput {
        level: LogLevel::Error,
        event: "work_source_check_refused".to_string(),
        server_id: None,
        request_id: None,
        client: None,
        duration_ms: None,
        error: Some(
            "Ghostex didn't keep a ticket command's hourly check; `ghostex issues source check` says why."
                .to_string(),
        ),
        details: Some(json!({ "verb": "info", "workspaceId": workspace_id, "sourceId": source_id })),
    });
}
