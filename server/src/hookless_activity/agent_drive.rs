//! Codex's and OpenCode's hook-free observers driving their sessions' state on a computer where
//! that agent's Ghostex hook is not installed. Each agent is decided on its own, from its own hook
//! status: where its hook is installed nothing here runs and the hook keeps driving, as before.
//!
//! CDXC:AgentHooks 2026-11-07 FIX:
//! Issue #475 asked that every agent's turns be tracked without hooks where hooks are missing, not only Claude's. Codex's observer tails the rollout Ghostex already binds to the session (the live-process scan reads the rollout Codex holds open, so this needs no hook), and OpenCode's reads OpenCode's own service events for the conversation id Ghostex chose at launch; both already ran beside the hooks for the side-by-side comparison. Where the agent's hook is missing, their events now go through `/api/ingestAgentHookEvent` like the hook's would, marked as the observer's so the comparison skips them. They see less than the hooks: Codex's rollout shows no approval waiting on the user (codex.rs), and OpenCode's events carry no question or approval card, so the session turns "waiting on you" without its card. Screen-based tracking for the other agents was left out: the title rules and the "Not moving" watch already read their screens, and nothing there tells a finished turn from a quiet one reliably enough to end a turn by.
//! SEE-ALSO: server/src/hookless_activity/drive.rs (Claude's driver and the hook status cache), server/src/hookless_activity/codex.rs, server/src/hookless_activity/opencode.rs.

use std::collections::HashMap;
use std::sync::{LazyLock, Mutex};
use std::time::Instant;

use serde_json::{json, Map, Value};

use super::comparison::ActivityEvent;
use super::drive::{agent_hook_installed, OBSERVER_MARK};
use crate::logging::{DiagnosticLogScenario, GxserverLogInput, LogLevel};
use crate::paths::GxserverPaths;
use crate::server::AppState;

type HookCache = LazyLock<Mutex<Option<(Instant, Option<bool>)>>>;
static CODEX_HOOK: HookCache = LazyLock::new(Default::default);
static OPENCODE_HOOK: HookCache = LazyLock::new(Default::default);

/// Whether Codex's observer drives Codex sessions here: its hook is not installed.
pub(super) fn codex_drive_enabled(paths: &GxserverPaths) -> bool {
    !cfg!(test) && agent_hook_installed(paths, "codex", &CODEX_HOOK) == Some(false)
}

/// Whether OpenCode's observer drives OpenCode sessions here: its plugin is not installed.
pub(super) fn opencode_drive_enabled(paths: &GxserverPaths) -> bool {
    !cfg!(test) && agent_hook_installed(paths, "opencode", &OPENCODE_HOOK) == Some(false)
}

/// The events of an open turn: a baseline that ends on one finds the session mid-turn.
fn opens_turn(event_name: &str) -> bool {
    matches!(
        event_name,
        "UserPromptSubmit" | "PreToolUse" | "PostToolUse" | "SessionBusy"
    )
}

/// The hook params an observer event sends, or `None` for one that drives nothing (Codex's
/// `Compacted`, which no hook sends, and a baseline, which `baseline_events` sums up).
pub(super) fn hook_params(event: &ActivityEvent) -> Option<Map<String, Value>> {
    if event.baseline || !event.event_signal || event.event_name == "Compacted" {
        return None;
    }
    let mut params = Map::new();
    params.insert("projectId".to_string(), json!(event.project_id));
    params.insert("sessionId".to_string(), json!(event.session_id));
    params.insert("agentName".to_string(), json!(event.agent));
    params.insert("eventName".to_string(), json!(event.event_name));
    params.insert("rawEventName".to_string(), json!(event.event_name));
    params.insert(OBSERVER_MARK.to_string(), json!(true));
    // The status the agent's own hook posts with the event.
    if let Some(status) = crate::agent_hooks::event_mapping::activity_for_hook_event(
        event.agent,
        &event.event_name,
        &Value::Object(event.payload.clone()),
    ) {
        params.insert("status".to_string(), json!(status));
    }
    if let Some(tool) = event.payload.get("toolName") {
        params.insert("toolName".to_string(), tool.clone());
    }
    Some(params)
}

/// A baseline is where a session already stood when the observer first looked: only a session
/// found mid-turn is told so (working); a finished turn's old end never becomes news.
pub(super) fn baseline_events(events: &[ActivityEvent]) -> Vec<ActivityEvent> {
    let mut last: HashMap<(String, String), &ActivityEvent> = HashMap::new();
    for event in events.iter().filter(|event| event.baseline) {
        last.insert((event.project_id.clone(), event.session_id.clone()), event);
    }
    last.into_values()
        .filter(|event| opens_turn(&event.event_name))
        .map(|event| {
            let mut busy = event.clone();
            busy.event_name = "UserPromptSubmit".to_string();
            busy.baseline = false;
            busy
        })
        .collect()
}

/// Sends a driving agent's observer events through the hook path.
pub(super) fn drive(state: &AppState, events: &[ActivityEvent]) {
    for event in baseline_events(events).iter().chain(events) {
        let Some(params) = hook_params(event) else {
            continue;
        };
        let response = crate::server::dispatch_agent_http_blocking(
            state,
            "/api/ingestAgentHookEvent".to_string(),
            format!("hook-free-{}", uuid::Uuid::new_v4()),
            params,
        );
        let _ = state.logger.log_routine(
            DiagnosticLogScenario::HooklessActivityComparison,
            GxserverLogInput {
                level: LogLevel::Debug,
                event: "hooklessActivity.drive".to_string(),
                server_id: None,
                request_id: None,
                client: None,
                duration_ms: None,
                error: None,
                details: Some(json!({
                    "agent": event.agent,
                    "projectId": event.project_id,
                    "sessionId": event.session_id,
                    "event": event.event_name,
                    "status": response.response.status().as_u16(),
                })),
            },
        );
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn event(agent: &'static str, name: &str) -> ActivityEvent {
        ActivityEvent::new("P1", "G1", agent, name, Some("thread".to_string()))
    }

    #[test]
    fn an_observer_event_posts_what_the_agents_hook_would() {
        let start = hook_params(&event("codex", "UserPromptSubmit")).expect("params");
        assert_eq!(start["agentName"], "codex");
        assert_eq!(start["eventName"], "UserPromptSubmit");
        assert_eq!(start["status"], "working");
        assert_eq!(start[OBSERVER_MARK], true);
        // A finished Codex turn rings like its Stop hook; an interrupt only ends it.
        assert_eq!(
            hook_params(&event("codex", "Stop")).expect("stop")["status"],
            "attention"
        );
        assert_eq!(
            hook_params(&event("codex", "Interrupt")).expect("interrupt")["status"],
            "idle"
        );
        let tool = hook_params(&event("codex", "PreToolUse").with("toolName", json!("shell")))
            .expect("tool");
        assert_eq!(tool["toolName"], "shell");
        assert_eq!(tool["status"], "working");
        assert!(hook_params(&event("codex", "Compacted")).is_none());
        assert!(hook_params(&event("codex", "Stop").baseline()).is_none());
        // OpenCode's run events.
        assert_eq!(
            hook_params(&event("opencode", "SessionBusy")).expect("busy")["status"],
            "working"
        );
        assert_eq!(
            hook_params(&event("opencode", "PermissionRequest")).expect("ask")["status"],
            "attention"
        );
    }

    /// A Codex session whose hook never fires: its rollout alone takes it to working and, at
    /// `task_complete`, to a finished turn, through the same hook path.
    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn a_codex_rollout_drives_its_session_without_its_hook() {
        let temp = tempfile::tempdir().expect("tempdir");
        let paths = crate::paths::get_gxserver_paths(Some(temp.path().to_path_buf()));
        std::fs::create_dir_all(paths.config_file.parent().expect("config dir")).expect("dir");
        let state = crate::server::tests::helpers::test_app_state(paths.clone());
        let rollout = temp
            .path()
            .join("rollout-2026-11-07T10-00-00-0199a213-81c0-7800-8aa1-bbab2a035a53.jsonl");
        std::fs::write(
            &rollout,
            concat!(
                "{\"type\":\"session_meta\",\"payload\":{\"id\":\"0199a213-81c0-7800-8aa1-bbab2a035a53\"}}\n",
                "{\"type\":\"event_msg\",\"payload\":{\"type\":\"task_started\"}}\n",
                "{\"type\":\"response_item\",\"payload\":{\"type\":\"function_call\",\"name\":\"shell\",\"call_id\":\"c1\"}}\n",
            ),
        )
        .expect("rollout");
        let db = crate::storage::open_gxserver_database(&paths).expect("db");
        let repository = crate::domain::DomainRepository::new(&db, &state.metadata.server_id);
        let project = repository
            .create_project(
                json!({ "name": "Codex", "path": temp.path() })
                    .as_object()
                    .expect("params"),
            )
            .expect("project");
        let project_id = project["projectId"].as_str().expect("id").to_string();
        let session = repository
            .create_session(
                json!({
                    "agentId": "codex",
                    "kind": "agent",
                    "projectId": project_id,
                    "runtimeSettings": {
                        "agentName": "codex",
                        "agentSessionId": "0199a213-81c0-7800-8aa1-bbab2a035a53",
                        "agentSessionPath": rollout,
                    },
                    "title": "Codex thread",
                })
                .as_object()
                .expect("params"),
                true,
            )
            .expect("session");
        let session_id = session["sessionId"].as_str().expect("id").to_string();
        db.execute(
            "UPDATE sessions SET lifecycleState = 'running' WHERE sessionId = ?1",
            [&session_id],
        )
        .expect("running");
        let activity = || {
            let db = crate::storage::open_gxserver_database(&paths).expect("db");
            crate::domain::DomainRepository::new(&db, &state.metadata.server_id)
                .get_session(&project_id, &session_id)
                .expect("read")
                .expect("session")
                .pointer("/runtimeSettings/agentActivity/activity")
                .and_then(Value::as_str)
                .map(str::to_string)
        };
        let sessions = || {
            let db = crate::storage::open_gxserver_database(&paths).expect("db");
            crate::domain::DomainRepository::new(&db, &state.metadata.server_id)
                .list_sessions_with_lifecycle_state("running")
                .expect("sessions")
        };
        drop(db);
        let observer =
            std::sync::Arc::new(Mutex::new(super::super::codex::CodexObserver::default()));
        // As the poll loop does, on the blocking pool.
        let pass = || {
            let (state, observer, sessions) = (state.clone(), observer.clone(), sessions());
            tokio::task::spawn_blocking(move || {
                let events = observer.lock().unwrap().poll(&sessions);
                drive(&state, &events);
            })
        };
        // A rollout this new is read from its start: the turn and its tool call are news.
        pass().await.expect("pass");
        assert_eq!(activity().as_deref(), Some("working"));
        let mut grown = std::fs::read_to_string(&rollout).expect("read");
        grown.push_str("{\"type\":\"response_item\",\"payload\":{\"type\":\"function_call_output\",\"call_id\":\"c1\"}}\n");
        grown.push_str("{\"type\":\"event_msg\",\"payload\":{\"type\":\"task_complete\"}}\n");
        std::fs::write(&rollout, grown).expect("grow");
        pass().await.expect("pass");
        assert_eq!(activity().as_deref(), Some("attention"));
    }

    /// An OpenCode session whose plugin is missing: OpenCode's own run events drive it.
    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn opencode_run_events_drive_its_session_without_its_plugin() {
        let temp = tempfile::tempdir().expect("tempdir");
        let paths = crate::paths::get_gxserver_paths(Some(temp.path().to_path_buf()));
        std::fs::create_dir_all(paths.config_file.parent().expect("config dir")).expect("dir");
        let state = crate::server::tests::helpers::test_app_state(paths.clone());
        let db = crate::storage::open_gxserver_database(&paths).expect("db");
        let repository = crate::domain::DomainRepository::new(&db, &state.metadata.server_id);
        let project = repository
            .create_project(
                json!({ "name": "OpenCode", "path": temp.path() })
                    .as_object()
                    .expect("params"),
            )
            .expect("project");
        let project_id = project["projectId"].as_str().expect("id").to_string();
        let session = repository
            .create_session(
                json!({
                    "agentId": "opencode",
                    "kind": "agent",
                    "projectId": project_id,
                    "runtimeSettings": { "agentName": "opencode", "agentSessionId": "ses_drive475" },
                    "title": "OpenCode thread",
                })
                .as_object()
                .expect("params"),
                true,
            )
            .expect("session");
        let session_id = session["sessionId"].as_str().expect("id").to_string();
        db.execute(
            "UPDATE sessions SET lifecycleState = 'running' WHERE sessionId = ?1",
            [&session_id],
        )
        .expect("running");
        drop(db);
        super::super::opencode::bind_for_test("ses_drive475", &project_id, &session_id);
        let activity = || {
            let db = crate::storage::open_gxserver_database(&paths).expect("db");
            crate::domain::DomainRepository::new(&db, &state.metadata.server_id)
                .get_session(&project_id, &session_id)
                .expect("read")
                .expect("session")
                .pointer("/runtimeSettings/agentActivity/activity")
                .and_then(Value::as_str)
                .map(str::to_string)
        };
        let pass = |kind: &'static str| {
            let state = state.clone();
            tokio::task::spawn_blocking(move || {
                super::super::opencode::observe_stream_event(
                    kind,
                    &json!({ "sessionID": "ses_drive475" }),
                );
                let events: Vec<_> = super::super::opencode::drain()
                    .into_iter()
                    .filter(|event| event.agent_session_id.as_deref() == Some("ses_drive475"))
                    .collect();
                drive(&state, &events);
            })
        };
        pass("session.execution.started").await.expect("pass");
        assert_eq!(activity().as_deref(), Some("working"));
        pass("session.execution.succeeded").await.expect("pass");
        // A finished run settles as the plugin's Stop does (the same notify mapping).
        assert_eq!(activity().as_deref(), Some("idle"));
    }

    #[test]
    fn a_baseline_only_says_a_session_is_mid_turn() {
        let mut other = event("codex", "UserPromptSubmit").baseline();
        other.session_id = "G2".to_string();
        let events = vec![
            event("codex", "UserPromptSubmit").baseline(),
            event("codex", "PreToolUse").baseline(),
            event("codex", "Stop").baseline(),
            other,
            event("codex", "PreToolUse").baseline(),
        ];
        let mut busy = baseline_events(&events);
        busy.sort_by(|a, b| a.session_id.cmp(&b.session_id));
        // G1 ended on a tool call, G2 on a prompt: both are mid-turn.
        assert_eq!(busy.len(), 2);
        assert!(busy.iter().all(|event| !event.baseline));
        assert_eq!(hook_params(&busy[0]).expect("params")["status"], "working");
        // A session whose baseline ends on its turn's end is left as it is.
        assert!(baseline_events(&[
            event("codex", "UserPromptSubmit").baseline(),
            event("codex", "Stop").baseline(),
        ])
        .is_empty());
    }
}
