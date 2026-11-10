//! Ends the stored working claim of an agent that left without a Stop hook (killed, crashed), once
//! the detection funnel confirms it is gone (`agentGone`).

use std::collections::HashSet;
use std::sync::{LazyLock, Mutex};

use serde_json::Value;

use crate::agents::{stored_activity_is_working, GoneActivityGuard};
use crate::domain::DomainRepository;
use crate::server::AppState;
use crate::storage::open_gxserver_database;

/// Sessions with a reconcile running, `project:session`: probes that land meanwhile skip it.
static IN_FLIGHT: LazyLock<Mutex<HashSet<String>>> = LazyLock::new(Default::default);

/// Runs [`reconcile_gone_agent_activity`] off the probe's thread, at most once at a time per
/// session. Called by every detection that reads the agent gone.
pub(crate) fn dispatch_gone_agent_reconcile(state: AppState, project_id: &str, session_id: &str) {
    let key = format!("{project_id}:{session_id}");
    if !IN_FLIGHT
        .lock()
        .unwrap_or_else(|error| error.into_inner())
        .insert(key.clone())
    {
        return;
    }
    let project_id = project_id.to_string();
    let session_id = session_id.to_string();
    let run = move || {
        reconcile_gone_agent_activity(&state, &project_id, &session_id);
        IN_FLIGHT
            .lock()
            .unwrap_or_else(|error| error.into_inner())
            .remove(&key);
    };
    match tokio::runtime::Handle::try_current() {
        Ok(handle) => {
            handle.spawn_blocking(run);
        }
        Err(_) => run(),
    }
}

/// CDXC:SessionStatus 2026-11-10 WHY:
/// No hook reports a killed agent, so its hook-backed "working" stayed for good: the sidebar, `ghostex sessions`, coordinators and phones whose chat core predates `agentGone` kept showing it working. A confirmed absence (session_chat_start_failure.rs `agent_left_conversation`) ends the claim the Stop hook would have ended, as idle, not attention: nothing finished, so no sound and no done notification, and a session already in attention keeps it. The write is guarded on the stored state this read observed (`/api/settleGoneAgentActivity`, agents/gone_activity.rs) and applied again on every probe while the agent stays gone and the session reads working, because writers that read outside a transaction can bring an older working state back; there is no "settled" flag that such a write could outlive.
/// SEE-ALSO: server/src/session_chat_start_failure.rs, server/src/agents/gone_activity.rs, server/src/zmx/provider.rs (`forget_agent_gone` at the start boundary), server/src/server/session_self_heal_runtime.rs (probes a session nobody watches), packages/gx-chat-core/src/session/working.rs.
pub(crate) fn reconcile_gone_agent_activity(state: &AppState, project_id: &str, session_id: &str) {
    let Ok(db) = open_gxserver_database(&state.paths) else {
        return;
    };
    let repository = DomainRepository::new(&db, state.metadata.server_id.as_str());
    let Ok(Some(session)) = repository.get_session(project_id, session_id) else {
        return;
    };
    if !stored_activity_is_working(&session) {
        return;
    }
    // A fresh process read, not the shared snapshot the classifier uses: an agent back in the
    // session (typed at its shell, or started a moment ago) keeps its working state.
    let Ok(zmx_name) = crate::zmx::provider_zmx_session_name(&session) else {
        return;
    };
    let Ok(identities) = crate::zmx::read_zmx_session_process_identities(
        std::slice::from_ref(&zmx_name),
        &crate::resume_lookup::home_dir(),
    ) else {
        return;
    };
    if identities.contains_key(&zmx_name) {
        return;
    }
    let (Some(observed_activity), Some(observed_updated_at)) = (
        session
            .pointer("/runtimeSettings/agentActivity")
            .filter(|value| value.is_object())
            .cloned(),
        session
            .get("updatedAt")
            .and_then(Value::as_str)
            .map(str::to_string),
    ) else {
        return;
    };
    let guard = GoneActivityGuard {
        observed_activity,
        observed_updated_at,
    };
    let _ = crate::server::dispatch_agent_http_blocking(
        state,
        crate::agents::SETTLE_GONE_AGENT_ACTIVITY_ENDPOINT.to_string(),
        format!("agent-gone-{project_id}-{session_id}"),
        guard.into_params(project_id, session_id),
    );
}
