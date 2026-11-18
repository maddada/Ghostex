use serde_json::Value;
use std::time::Duration;
use tokio::sync::broadcast;

use crate::{
    domain::{DomainRepository, DomainStateError},
    paths::GxserverPaths,
    storage::open_gxserver_database,
};

use super::*;

#[derive(Clone)]
pub struct AutomationRuntime {
    pub(super) auth_token_file: String,
    pub(super) base_url: String,
    pub(super) paths: GxserverPaths,
    pub(super) server_id: String,
    /// gxserver's state once it exists (routine_tick.rs), for GBot routines.
    pub(super) app: AppSlot,
}

#[derive(Clone)]
pub(super) struct AutomationDefinitionRecord {
    pub(super) agent_id: String,
    pub(super) created_at: String,
    pub(super) enabled: bool,
    pub(super) execution_mode: Value,
    pub(super) id: String,
    pub(super) name: String,
    pub(super) next_run_at: Option<String>,
    pub(super) project_id: String,
    pub(super) prompt: String,
    pub(super) schedule: Value,
    pub(super) updated_at: String,
    /// The GBot whose routine this is (server/src/gbots/routines.rs): each run is a chat of it.
    pub(super) owner_bot_id: Option<String>,
    /// The check that runs as plain code before a run (precheck.rs).
    pub(super) precheck: Option<Value>,
    /// Where a routine's result goes: `chat` (its own chat) or `chief`.
    pub(super) deliver: Option<String>,
    /// The model a run's new session starts on (`ghostex create-agent --model`); `None` keeps the
    /// agent's own (GitHub #662).
    pub(super) model: Option<String>,
    /// The effort a run's new session starts on (`--effort`); `None` keeps the agent's own.
    pub(super) effort: Option<String>,
}

#[derive(Clone)]
pub(super) struct AutomationRunRecord {
    pub(super) automation_id: String,
    pub(super) completed_at: Option<String>,
    pub(super) created_at: String,
    pub(super) error_message: Option<String>,
    pub(super) findings_summary: Option<String>,
    pub(super) id: String,
    pub(super) is_archived: bool,
    pub(super) is_unread: bool,
    pub(super) project_id: String,
    pub(super) session_id: Option<String>,
    pub(super) status: String,
    pub(super) updated_at: String,
    pub(super) worktree: Value,
}

pub(super) struct AutomationLaunch {
    /*
    CDXC:Automations 2026-07-30:
    Launches that create a fresh agent session own an undelivered prompt.
    `createAgentSession` only records the prompt as `runtimeSettings.firstUserMessage`
    metadata, and the agent launch plan's `startupText` carries the agent command
    alone, so nothing types the prompt into the session. Carry the pending prompt
    out of the launch so the run watcher delivers it.
    */
    pub(super) pending_prompt: Option<PendingPrompt>,
    pub(super) session_id: String,
    pub(super) session_project_id: String,
    pub(super) worktree: Option<Value>,
}

/// The prompt a run's watcher still has to deliver, and how it waits.
#[derive(Clone)]
pub(super) struct PendingPrompt {
    pub(super) text: String,
    /// A new session's first message: the chat queue holds it until the agent's input box is up.
    /// A thread run's prompt (`false`) waits like a queued chat message until the agent is free.
    pub(super) startup_send: bool,
}

const AUTOMATION_SCHEDULER_TICK_SECONDS: u64 = 30;
const AUTOMATION_RUN_POLL_SECONDS: u64 = 5;
const AUTOMATION_RUN_POLL_LIMIT: usize = 720;
/// How often a run's watcher looks whether the chat queue has delivered its prompt.
const AUTOMATION_PROMPT_POLL_SECONDS: u64 = 2;
/// How long a new session's first prompt may wait for the agent's input box before the run ends.
const AUTOMATION_STARTUP_PROMPT_LIMIT_SECONDS: u64 = 10 * 60;
/// How long a thread run's prompt may wait for a busy agent before the run ends.
const AUTOMATION_THREAD_PROMPT_LIMIT_SECONDS: u64 = 30 * 60;
/// How long the agent must sit idle after its turn, with no result, before the run ends.
const AUTOMATION_TURN_END_GRACE_SECONDS: u64 = 3 * 60;
pub(super) const AUTOMATION_RESULT_PREFIX: &str = "AUTOMATION_RESULT:";
pub(super) const AUTOMATION_MAX_COUNT: usize = 500;
pub(super) const AUTOMATION_MAX_RUN_COUNT: usize = 5_000;

impl AutomationRuntime {
    pub fn new(paths: GxserverPaths, server_id: impl Into<String>, base_url: String) -> Self {
        Self {
            auth_token_file: paths.auth_token_file.to_string_lossy().to_string(),
            base_url,
            paths,
            server_id: server_id.into(),
            app: AppSlot::default(),
        }
    }

    pub fn start(&self, mut shutdown_rx: broadcast::Receiver<()>) {
        /*
        CDXC:Automations 2026-06-29-15:55:
        Automations are daemon-owned work now, not macOS renderer timers. Keep the scheduler loop in the gxserver automation module so CLI, macOS, GPUI, and remote clients share one durable SQLite source of truth and one runner while Ghostex is open.
        */
        let runtime = self.clone();
        tokio::spawn(async move {
            let mut interval = crate::ticks::steady_interval(Duration::from_secs(
                AUTOMATION_SCHEDULER_TICK_SECONDS,
            ));
            loop {
                tokio::select! {
                    _ = shutdown_rx.recv() => break,
                    _ = interval.tick() => {
                        let _ = runtime.run_scheduler_tick();
                    }
                }
            }
        });
    }

    fn run_scheduler_tick(&self) -> Result<(), DomainStateError> {
        let db = open_gxserver_database(&self.paths).map_err(internal_error)?;
        let repository = DomainRepository::new(&db, self.server_id.as_str());
        recover_running_automation_runs(self, &repository, &db)?;
        let due = read_due_automations(&db)?;
        for automation in due {
            if automation.owner_bot_id.is_some() || automation.precheck.is_some() {
                let _ = self.routine_due(&repository, &db, &automation);
                continue;
            }
            if has_active_run(&db, &automation.id)? {
                upsert_run(&db, &skipped_run_record(&db, &automation)?)?;
                update_automation_next_run_at(&db, &automation.project_id, &automation.id)?;
                continue;
            }
            let _ = queue_automation_run(self, &repository, &db, &automation);
            update_automation_next_run_at(&db, &automation.project_id, &automation.id)?;
        }
        Ok(())
    }

    /// Starts the watcher that delivers a run's prompt and reads its result. A run that already
    /// has a watcher in this gxserver gets no second one.
    pub(super) fn spawn_run_watcher(
        &self,
        project_id: String,
        run_id: String,
        session_project_id: String,
        session_id: String,
        pending_prompt: Option<PendingPrompt>,
    ) {
        let Some(watch) = RunWatch::claim(&run_id) else {
            return;
        };
        let runtime = self.clone();
        tokio::spawn(async move {
            let _watch = watch;
            let _ = runtime
                .watch_automation_run(
                    project_id,
                    run_id,
                    session_project_id,
                    session_id,
                    pending_prompt,
                )
                .await;
        });
    }

    async fn watch_automation_run(
        &self,
        project_id: String,
        run_id: String,
        session_project_id: String,
        session_id: String,
        pending_prompt: Option<PendingPrompt>,
    ) -> Result<(), DomainStateError> {
        let run_created_at = {
            let db = open_gxserver_database(&self.paths).map_err(internal_error)?;
            read_run(&db, &project_id, &run_id)
                .ok()
                .map(|run| run.created_at)
        };
        if let Some(prompt) = pending_prompt {
            if let Err(message) = self
                .deliver_automation_prompt(
                    &project_id,
                    &run_id,
                    &session_project_id,
                    &session_id,
                    &prompt,
                )
                .await
            {
                let db = open_gxserver_database(&self.paths).map_err(internal_error)?;
                if is_run_active(&db, &project_id, &run_id)? {
                    fail_run(&db, &project_id, &run_id, &message, "failed")?;
                    self.routine_run_settled(&project_id, &run_id);
                }
                return Ok(());
            }
        }
        let since_ms = chrono::Utc::now().timestamp_millis();
        let mut turn = TurnWatch::default();
        for _ in 0..AUTOMATION_RUN_POLL_LIMIT {
            tokio::time::sleep(Duration::from_secs(AUTOMATION_RUN_POLL_SECONDS)).await;
            let db = open_gxserver_database(&self.paths).map_err(internal_error)?;
            let repository = DomainRepository::new(&db, self.server_id.as_str());
            if !is_run_active(&db, &project_id, &run_id)? {
                return Ok(());
            }
            if let Some((status, summary)) = read_automation_result_from_session(
                &repository,
                &session_project_id,
                &session_id,
                run_created_at.as_deref(),
            )? {
                complete_run(&db, &project_id, &run_id, &status, summary.as_deref())?;
                self.routine_run_settled(&project_id, &run_id);
                return Ok(());
            }
            let session = repository.get_session(&session_project_id, &session_id)?;
            if session.is_some_and(|session| turn.ended(&session, since_ms)) {
                fail_run(
                    &db,
                    &project_id,
                    &run_id,
                    "The agent finished its turn without reporting an AUTOMATION_RESULT line.",
                    "needs_attention",
                )?;
                self.routine_run_settled(&project_id, &run_id);
                return Ok(());
            }
        }
        let db = open_gxserver_database(&self.paths).map_err(internal_error)?;
        fail_run(
            &db,
            &project_id,
            &run_id,
            "Automation did not report an AUTOMATION_RESULT marker before the watcher timeout.",
            "needs_attention",
        )?;
        self.routine_run_settled(&project_id, &run_id);
        Ok(())
    }

    /*
    CDXC:Automations 2026-11-18 WHY:
    GitHub #656 and a Discord report on 11.9.0: a run's first prompt was typed through
    `/api/sendSessionMessage`, which queues its write and answers before it happens. When Claude
    Code was still drawing its input box the write was refused ("The claude input box is not on
    screen yet"), nothing tried again, and the run stayed `running` at an empty prompt while every
    later run was skipped. The prompt now goes through the session's chat queue, the way a new
    chat's first message and `ghostex board start-work` deliver theirs: a new session's prompt is a
    startup send the queue holds until the input box is up (waking, redrawing and retrying as it
    does for any held message), and a thread run's prompt waits like a queued chat message until
    the agent is free. The watcher follows that row: Not sent ends the run as failed with the
    queue's reason, and a row still waiting after the limit is taken back and ends the run, so a
    prompt that never arrives can no longer hold the automation.
    */
    async fn deliver_automation_prompt(
        &self,
        project_id: &str,
        run_id: &str,
        session_project_id: &str,
        session_id: &str,
        prompt: &PendingPrompt,
    ) -> Result<(), String> {
        let prompt_id = {
            let db = open_gxserver_database(&self.paths).map_err(|error| error.to_string())?;
            crate::session_chat_queue::enqueue_session_chat_prompt(
                &db,
                session_project_id,
                session_id,
                &prompt.text,
                prompt.startup_send,
            )
            .map_err(|error| {
                format!(
                    "The automation's prompt could not be queued: {}",
                    error.message
                )
            })?
            .id
        };
        self.publish_chat_queue(session_project_id, session_id);
        let limit_seconds = if prompt.startup_send {
            AUTOMATION_STARTUP_PROMPT_LIMIT_SECONDS
        } else {
            AUTOMATION_THREAD_PROMPT_LIMIT_SECONDS
        };
        let started = std::time::Instant::now();
        loop {
            tokio::time::sleep(Duration::from_secs(AUTOMATION_PROMPT_POLL_SECONDS)).await;
            let Ok(db) = open_gxserver_database(&self.paths) else {
                continue;
            };
            let run_active = is_run_active(&db, project_id, run_id).unwrap_or(true);
            let row = crate::session_chat_queue::read_session_chat_queue_snapshot_with(
                &db,
                session_project_id,
                session_id,
            )
            .queue
            .into_iter()
            .find(|row| row.id == prompt_id);
            drop(db);
            // Gone from the queue: the agent took it.
            let Some(row) = row else {
                return Ok(());
            };
            if row.state == crate::session_chat_queue::SESSION_CHAT_QUEUE_STATE_FAILED {
                return Err(format!(
                    "The automation's prompt was not sent to the agent: {}",
                    row.error_message
                        .as_deref()
                        .unwrap_or("the chat queue gave no reason.")
                ));
            }
            let timed_out = started.elapsed().as_secs() >= limit_seconds;
            if (timed_out || !run_active)
                && row.state == crate::session_chat_queue::SESSION_CHAT_QUEUE_STATE_QUEUED
                && crate::session_chat_queue::drop_queued_prompt(
                    &self.paths,
                    session_project_id,
                    session_id,
                    &prompt_id,
                )
            {
                self.publish_chat_queue(session_project_id, session_id);
                let waiting = row
                    .wait_reason
                    .map(|reason| format!(" ({})", reason.trim_end_matches('.')))
                    .unwrap_or_default();
                return Err(format!(
                    "The automation's prompt waited {} minutes and never reached the agent{waiting}.",
                    limit_seconds / 60
                ));
            }
        }
    }

    /// Tells the session's open chats that its queue changed, and wakes the queue for it.
    fn publish_chat_queue(&self, session_project_id: &str, session_id: &str) {
        match self.app_state() {
            Some(state) => crate::session_chat_queue_runtime::broadcast_session_chat_queue_state(
                &state,
                session_project_id,
                session_id,
            ),
            None => crate::session_chat_queue_runtime::kick_session_chat_queue(),
        }
    }
}

/// The runs that have a watcher in this gxserver, so the scheduler's recovery pass neither
/// starts a second one nor judges a run its watcher is still delivering.
static WATCHED_RUNS: std::sync::LazyLock<std::sync::Mutex<std::collections::HashSet<String>>> =
    std::sync::LazyLock::new(Default::default);

/// One run's claim on [`WATCHED_RUNS`], released when its watcher ends.
pub(super) struct RunWatch(String);

impl RunWatch {
    fn claim(run_id: &str) -> Option<Self> {
        let mut watched = WATCHED_RUNS.lock().ok()?;
        watched
            .insert(run_id.to_string())
            .then(|| Self(run_id.to_string()))
    }
}

impl Drop for RunWatch {
    fn drop(&mut self) {
        if let Ok(mut watched) = WATCHED_RUNS.lock() {
            watched.remove(&self.0);
        }
    }
}

pub(super) fn is_run_watched(run_id: &str) -> bool {
    WATCHED_RUNS
        .lock()
        .is_ok_and(|watched| watched.contains(run_id))
}

/*
CDXC:Automations 2026-11-18 WHY:
GitHub #656: a thread run into a session that stays open stayed `running` until the watcher's
hour ran out whenever the agent answered without the AUTOMATION_RESULT line, and every scheduled
run meanwhile was skipped. Once the agent has worked on the prompt (seen working, or a reply
written since it was delivered) and then sat idle for `AUTOMATION_TURN_END_GRACE_SECONDS`, the
run ends as needing attention. The grace leaves time for the reply to reach the transcript, and
an agent waiting on a question or approval is not idle, so it is left alone.
*/
#[derive(Default)]
struct TurnWatch {
    saw_working: bool,
    idle_since: Option<std::time::Instant>,
}

impl TurnWatch {
    fn ended(&mut self, session: &Value, since_ms: i64) -> bool {
        let now = chrono::Utc::now().to_rfc3339();
        match crate::session_lifecycle::session_activity(session, &now).as_str() {
            "working" => {
                self.saw_working = true;
                self.idle_since = None;
                false
            }
            "idle" => {
                let replied = crate::notification_feed::body::last_assistant_message(session)
                    .and_then(|(_, timestamp)| timestamp)
                    .is_some_and(|timestamp| timestamp >= since_ms);
                if !self.saw_working && !replied {
                    return false;
                }
                let idle_since = *self.idle_since.get_or_insert_with(std::time::Instant::now);
                idle_since.elapsed().as_secs() >= AUTOMATION_TURN_END_GRACE_SECONDS
            }
            _ => {
                self.idle_since = None;
                false
            }
        }
    }
}
