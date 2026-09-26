//! What a workspace terminal reports about itself (a bell, a changed title, Escape on the
//! "Generating title" overlay), turned into the gxserver call the old app runtime used to make.
//!
//! Web-ready: this file uses only `gx_rpc` and `gx-core` types, so step 4 of the runtime port can
//! symlink it into `apps/gpui-web`. The desktop half (which terminal rang, the Terminal settings
//! gate, the overlay flag of the workspace tab) stays in
//! `terminal_sync/workspace_terminal_dispatch.rs`.
//!
//! CDXC:Notifications 2026-09-25 WHY:
//! The three events used to be posted into the QuickJS runtime (`terminal-lifecycle-queue.ts`) only
//! for it to make one gxserver call each. The calls, their parameters and their silent failure are
//! the runtime's, one for one; only the hop through JavaScript is gone.
//!
//! SEE-ALSO: docs/2026-09-25/app-runtime-port/LEDGER.md rows C030, C031, C033, L018, R035, R036,
//! R038.

use std::collections::HashMap;

use ghostex_gx_core::protocol::{PresentationSession, SessionKind};
use serde_json::{Map, Value, json};

use crate::app::gx_store::gx_rpc;

/// How long a title has to stay put before it is sent: a title burst (a spinner, a path, then the
/// real title) ends in one call. The runtime's `GPUI_SIDEBAR_WORKSPACE_TERMINAL_TITLE_SETTLE_MS`.
#[cfg_attr(not(target_os = "windows"), allow(dead_code))]
pub(crate) const TERMINAL_TITLE_SETTLE_MS: u64 = 1_500;

/// The per-session title settle the old runtime kept in `workspaceTerminalTitleSettleTimeouts`.
#[derive(Default)]
pub(crate) struct TerminalTitleSettle {
    /// `(project, session)` to the newest raw title and the generation of the timer that owns it.
    pending: HashMap<(String, String), (u64, String)>,
    next_generation: u64,
}

impl TerminalTitleSettle {
    /// Records an observation and returns the generation its settle timer must present to
    /// [`Self::take_settled`]. A newer observation of the same session replaces it, which is the
    /// runtime's `clearTimeout` of the older timer.
    #[cfg_attr(not(target_os = "windows"), allow(dead_code))]
    pub(crate) fn observe(&mut self, project_id: &str, session_id: &str, raw_title: &str) -> u64 {
        self.next_generation += 1;
        let generation = self.next_generation;
        self.pending.insert(
            (project_id.to_string(), session_id.to_string()),
            (generation, raw_title.to_string()),
        );
        generation
    }

    /// The settled raw title, when no newer observation arrived since `generation`.
    #[cfg_attr(not(target_os = "windows"), allow(dead_code))]
    pub(crate) fn take_settled(
        &mut self,
        project_id: &str,
        session_id: &str,
        generation: u64,
    ) -> Option<String> {
        let key = (project_id.to_string(), session_id.to_string());
        if self.pending.get(&key)?.0 != generation {
            return None;
        }
        self.pending.remove(&key).map(|(_, raw_title)| raw_title)
    }
}

/// A local gxserver id the runtime's normalizers accept (`P0...`, `G...` without a scope).
/// Anything else (a remote-scoped or combined id) was dropped by the runtime and is dropped here.
pub(crate) fn local_workspace_ids_allowed(project_id: &str, session_id: &str) -> bool {
    let project = project_id.as_bytes();
    let project_ok = project.len() >= 2
        && project.len() <= 32
        && project[0] == b'P'
        && project[1].is_ascii_digit()
        && project[2..]
            .iter()
            .all(|byte| byte.is_ascii_lowercase() || byte.is_ascii_digit());
    // `gpuiStatusPetActivationSessionIdAllowed` plus "no scope": at most 256 UTF-16 units, no
    // slash, no backslash, no control character, no colon (the remote and combined forms).
    let session_ok = !session_id.trim().is_empty()
        && session_id.encode_utf16().count() <= 256
        && !session_id.contains(['/', '\\', ':'])
        && !session_id.chars().any(|ch| ch.is_ascii_control());
    project_ok && session_ok
}

/// `/api/updateAgentActivity` for a rung terminal. The caller has already applied the Terminal
/// setting "Show a notification on terminal bell". Best effort, like the runtime and macOS: a
/// failure is not reported anywhere.
pub(crate) async fn report_terminal_bell(
    project_id: String,
    session_id: String,
    agent_name: Option<String>,
) {
    let mut params = Map::new();
    if let Some(agent_name) = agent_name.filter(|name| !name.trim().is_empty()) {
        params.insert("agentName".into(), Value::String(agent_name));
    }
    params.insert("event".into(), json!("bell"));
    params.insert("projectId".into(), Value::String(project_id));
    params.insert("sessionId".into(), Value::String(session_id));
    let _ = gx_rpc(None, "/api/updateAgentActivity", Value::Object(params)).await;
}

/// `/api/cancelFirstPromptAutoTitle` for Escape on the "Generating title" overlay.
///
/// CDXC:SessionTitles 2026-07-26:
/// Escape inside the blocking "Generating title" pane overlay cancels the gxserver-owned first-prompt title job, matching the managed macOS pane. The overlay lifts at once (the caller clears the tab's flag first); a rejected cancel is recovered by the next presentation delta, which republishes the generating flag and brings the overlay back if the job is still alive.
pub(crate) async fn cancel_first_prompt_title(project_id: String, session_id: String) {
    let _ = gx_rpc(
        None,
        "/api/cancelFirstPromptAutoTitle",
        json!({ "projectId": project_id, "reason": "escape", "sessionId": session_id }),
    )
    .await;
}

/// The parameters of `/api/ingestTerminalTitleEvent` for a settled title, or `None` when the
/// runtime would not have called: no visible title, no terminal or agent row for the session, or
/// the same visible title the row already stores (whitespace runs compared as one space).
#[cfg_attr(not(target_os = "windows"), allow(dead_code))]
pub(crate) fn terminal_title_ingest_params(
    session: &PresentationSession,
    raw_title: &str,
    visible_title: impl Fn(&str) -> Option<String>,
) -> Option<Value> {
    let visible = visible_title(raw_title)?.trim().to_string();
    if visible.is_empty() {
        return None;
    }
    if !matches!(session.kind, SessionKind::Terminal | SessionKind::Agent) {
        return None;
    }
    let collapse = |value: &str| value.split_whitespace().collect::<Vec<_>>().join(" ");
    if let Some(stored) = session
        .terminal_title
        .as_deref()
        .and_then(&visible_title)
        .map(|title| title.trim().to_string())
        .filter(|title| !title.is_empty())
        && collapse(&stored) == collapse(&visible)
    {
        return None;
    }
    let mut params = Map::new();
    // `session.agentName ?? session.agentId`, sent when either is truthy.
    let agent_name = session
        .agent_name
        .clone()
        .or_else(|| session.agent_id.clone());
    if session
        .agent_name
        .as_deref()
        .is_some_and(|name| !name.is_empty())
        || session.agent_id.as_deref().is_some_and(|id| !id.is_empty())
    {
        if let Some(agent_name) = agent_name {
            params.insert("agentName".into(), Value::String(agent_name));
        }
    }
    params.insert(
        "projectId".into(),
        Value::String(session.project_id.clone()),
    );
    params.insert("rawTitle".into(), Value::String(raw_title.to_string()));
    params.insert(
        "sessionId".into(),
        Value::String(session.session_id.clone()),
    );
    if let Some(provider) = session.session_persistence_provider.as_ref() {
        params.insert(
            "sessionPersistenceProvider".into(),
            Value::String(provider.as_str().to_string()),
        );
    }
    Some(Value::Object(params))
}

/// `/api/ingestTerminalTitleEvent` with parameters from [`terminal_title_ingest_params`]. A later
/// observation or the presentation's recovery retries; nothing local is invented on failure.
#[cfg_attr(not(target_os = "windows"), allow(dead_code))]
pub(crate) async fn ingest_terminal_title(params: Value) {
    let _ = gx_rpc(None, "/api/ingestTerminalTitleEvent", params).await;
}
