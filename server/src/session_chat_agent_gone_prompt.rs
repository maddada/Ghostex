//! Retires the approval or question card of an agent that left without answering it (killed,
//! crashed), once the detection funnel confirms it is gone (`agentGone`).

use std::collections::HashSet;
use std::sync::{LazyLock, Mutex};

use rusqlite::{Transaction, TransactionBehavior};
use serde_json::{json, Map, Value};

use crate::domain::DomainRepository;
use crate::server::AppState;
use crate::session_status::{compute_activity_update, TURN_COMPLETE_ATTENTION_SOURCE};
use crate::storage::open_gxserver_database;

/// Sessions with a retire running, `project:session`: probes that land meanwhile skip it.
static IN_FLIGHT: LazyLock<Mutex<HashSet<String>>> = LazyLock::new(Default::default);

/// A session's place in [`IN_FLIGHT`], given back when the retire ends, panics included.
struct InFlight(String);

impl Drop for InFlight {
    fn drop(&mut self) {
        IN_FLIGHT
            .lock()
            .unwrap_or_else(|error| error.into_inner())
            .remove(&self.0);
    }
}

/// Whether the session holds a stored approval or question card.
pub(crate) fn session_holds_prompt_card(session: &Value) -> bool {
    crate::agents::session_chat_prompt_setting(session).is_some()
}

/// Whether a session whose agent is missing from its live daemon needs the detection funnel's look
/// even though it does not read working: it holds a card nothing can answer, or queued messages
/// that wait on an agent that is gone (the queue sends them once the absence is confirmed,
/// `transcript_lifecycle_is_working` in session_chat_queue_runtime/scheduler.rs).
pub(crate) fn session_needs_gone_probe(state: &AppState, session: &Value) -> bool {
    if session_holds_prompt_card(session) {
        return true;
    }
    let (Some(project_id), Some(session_id)) = (
        session.get("projectId").and_then(Value::as_str),
        session.get("sessionId").and_then(Value::as_str),
    ) else {
        return false;
    };
    open_gxserver_database(&state.paths).is_ok_and(|db| {
        crate::session_chat_queue::session_has_pending_session_chat_queue(
            &db, project_id, session_id,
        )
    })
}

/// Runs [`retire_gone_agent_prompt`] off the probe's thread, at most once at a time per session,
/// for the absence confirmed at `generation`.
pub(crate) fn dispatch_gone_agent_prompt_retire(
    state: AppState,
    project_id: &str,
    session_id: &str,
    generation: u64,
) {
    let key = format!("{project_id}:{session_id}");
    if !IN_FLIGHT
        .lock()
        .unwrap_or_else(|error| error.into_inner())
        .insert(key.clone())
    {
        return;
    }
    let slot = InFlight(key);
    let project_id = project_id.to_string();
    let session_id = session_id.to_string();
    let run = move || {
        let _slot = slot;
        retire_gone_agent_prompt(&state, &project_id, &session_id, generation);
    };
    match tokio::runtime::Handle::try_current() {
        Ok(handle) => {
            handle.spawn_blocking(run);
        }
        Err(_) => run(),
    }
}

/// CDXC:SessionChat 2026-11-11 WHY:
/// An agent killed while its approval or question card stood left the card up for good: no hook reports the kill, and #551's gone-agent settle ends only a working claim, so the session stayed in attention with a live Allow/Deny that nothing could answer. A confirmed absence retires the stored card and, when the attention was the card's (not a finished turn's), ends it as idle. The write is guarded like the settle (session_chat_agent_gone_activity.rs): under SQLite's write lock and the gone record's lock at the generation the absence was confirmed at, so an agent started meanwhile keeps whatever its hooks store.
/// SEE-ALSO: server/src/session_chat_agent_gone_activity.rs, server/src/session_chat_start_failure.rs (`while_agent_gone`), server/src/server/session_self_heal_runtime.rs (probes a session nobody watches).
pub(crate) fn retire_gone_agent_prompt(
    state: &AppState,
    project_id: &str,
    session_id: &str,
    generation: u64,
) {
    let Ok(db) = open_gxserver_database(&state.paths) else {
        return;
    };
    let repository = DomainRepository::new(&db, state.metadata.server_id.as_str());
    let Ok(Some(session)) = repository.get_session(project_id, session_id) else {
        return;
    };
    if !session_holds_prompt_card(&session) {
        return;
    }
    // A fresh process read, not the shared snapshot: an agent back in the session keeps its card.
    let Ok(zmx_name) = crate::zmx::provider_zmx_session_name(&session) else {
        return;
    };
    let Ok(identities) = crate::zmx::read_zmx_session_process_identities(
        std::slice::from_ref(&zmx_name),
        &crate::resume_lookup::home_dir(),
    ) else {
        return;
    };
    if identities.contains_key(&zmx_name) || !db.is_autocommit() {
        return;
    }
    let Ok(transaction) = Transaction::new_unchecked(&db, TransactionBehavior::Immediate) else {
        return;
    };
    let written = crate::session_chat_start_failure::while_agent_gone(
        project_id,
        session_id,
        generation,
        || -> Option<Value> {
            let inner = DomainRepository::new(&transaction, state.metadata.server_id.as_str());
            let current = inner.get_session(project_id, session_id).ok()??;
            let update = retired_prompt_update(&current, project_id, session_id)?;
            let updated = inner.update_session(&update).ok()?;
            // Committed while the gone record's lock is held, as the settle does.
            transaction.commit().ok()?;
            Some(updated)
        },
    );
    let Some(Some(updated)) = written else {
        return;
    };
    crate::session_chat_interactive::emit_session_chat_prompt_state_frame(state, &updated);
    let _ = crate::server::schedule_presentation_session_delta(
        state,
        &db,
        &repository,
        project_id,
        session_id,
    );
}

/// The session update that drops the stored card, with the card's attention ended as idle.
/// `None` when no card is stored any more.
fn retired_prompt_update(
    session: &Value,
    project_id: &str,
    session_id: &str,
) -> Option<Map<String, Value>> {
    if !session_holds_prompt_card(session) {
        return None;
    }
    let mut runtime = session
        .get("runtimeSettings")
        .and_then(Value::as_object)
        .cloned()
        .unwrap_or_default();
    let stored = runtime
        .get("agentActivity")
        .and_then(Value::as_object)
        .cloned()?;
    let card_attention = stored.get("activity").and_then(Value::as_str) == Some("attention")
        && stored.get("attentionSource").and_then(Value::as_str)
            != Some(TURN_COMPLETE_ATTENTION_SOURCE);
    let mut update = Map::new();
    update.insert("projectId".to_string(), json!(project_id));
    update.insert("sessionId".to_string(), json!(session_id));
    let activity = if card_attention {
        let mut params = Map::new();
        params.insert("projectId".to_string(), json!(project_id));
        params.insert("sessionId".to_string(), json!(session_id));
        let next = compute_activity_update(session, &params, Some("acknowledge"));
        if let Some(last_active_at) = next.last_active_at {
            update.insert("lastActiveAt".to_string(), json!(last_active_at));
        }
        next.activity
    } else {
        Value::Object(stored)
    };
    let mut activity = activity;
    if let Some(object) = activity.as_object_mut() {
        object.remove("sessionChatPrompt");
    }
    runtime.insert("agentActivity".to_string(), activity);
    update.insert("runtimeSettings".to_string(), Value::Object(runtime));
    Some(update)
}
