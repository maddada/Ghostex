//! CDXC:AgentProviders 2026-09-12 WHY:
//! Account switching can take several seconds without changing the transcript. Publish durable progress through the existing chat state stream so every open chat sees the actual restart stages, including failures and reconnects.
//! SEE-ALSO: session_chat_queue.rs (read/frame carriage), accounts/restart.rs, accounts/continuation.rs, packages/core-ui/accounts/account-switch-card.tsx.

use crate::{domain::DomainRepository, server::AppState, storage::open_gxserver_database};
use rusqlite::{params, Connection, OptionalExtension};
use serde_json::{json, Map, Value};

pub(crate) fn read(db: &Connection, project: &str, session: &str) -> Option<Value> {
    let text: Option<String> = db.query_row(
        "SELECT json_extract(runtimeSettingsJson, '$.accountSwitch') FROM sessions WHERE projectId = ?1 AND sessionId = ?2",
        params![project, session], |row| row.get(0),
    ).optional().ok().flatten().flatten();
    text.and_then(|text| serde_json::from_str(&text).ok())
}

pub(crate) fn set_phase(runtime: &mut Map<String, Value>, phase: &str, reason: Option<&str>) {
    let Some(progress) = runtime
        .get_mut("accountSwitch")
        .and_then(Value::as_object_mut)
    else {
        return;
    };
    if matches!(phase, "continuing" | "success") {
        progress.insert("accountReady".into(), json!(true));
    }
    progress.insert("phase".into(), json!(phase));
    progress.insert("updatedAt".into(), json!(chrono::Utc::now().to_rfc3339()));
    if let Some(reason) = reason {
        progress.insert("reason".into(), json!(reason));
    } else {
        progress.remove("reason");
    }
}

pub(crate) fn publish(state: &AppState, row: &Value) {
    crate::session_chat_queue_runtime::broadcast_session_chat_queue_state(
        state,
        row["projectId"].as_str().unwrap_or_default(),
        row["sessionId"].as_str().unwrap_or_default(),
    );
}

pub(crate) fn resuming(state: &AppState, project: &str, session: &str, attempt: &str) {
    let Ok(_gate) = state.accounts.mutations.lock() else {
        return;
    };
    let Ok(db) = open_gxserver_database(&state.paths) else {
        return;
    };
    let repo = DomainRepository::new(&db, &state.metadata.server_id);
    let Ok(Some(row)) = repo.get_session(project, session) else {
        return;
    };
    if row
        .pointer("/runtimeSettings/accountSwitchAttempt")
        .and_then(Value::as_str)
        != Some(attempt)
    {
        return;
    }
    let mut runtime = row["runtimeSettings"]
        .as_object()
        .cloned()
        .unwrap_or_default();
    set_phase(&mut runtime, "resuming", None);
    if super::endpoint::update_session(&repo, &row, runtime).is_ok() {
        publish(state, &row);
    }
}

/// A failure after the switch was accepted must leave a reviewable terminal state, never a permanently moving step.
pub(crate) fn fail(state: &AppState, repo: &DomainRepository<'_>, session: &Value, error: &str) {
    let Ok(Some(row)) = repo.get_session(
        session["projectId"].as_str().unwrap_or_default(),
        session["sessionId"].as_str().unwrap_or_default(),
    ) else {
        return;
    };
    let mut runtime = row["runtimeSettings"]
        .as_object()
        .cloned()
        .unwrap_or_default();
    set_phase(&mut runtime, "failed", Some(error));
    if super::endpoint::update_session(repo, &row, runtime).is_ok() {
        publish(state, &row);
    }
}
