//! The "Custom flags ignored" card of a Native session: which of the agent's command flags it
//! still names, and its Dismiss, kept per agent.
//!
//! CDXC:NativeApprovals 2026-11-25 WHY:
//! GitHub #751: the card came back after Dismiss, in the same chat and in every other chat of the agent, and it kept naming `--dangerously-skip-permissions` although Native has honored that flag as Full permissions since 12.1.1. The card is read from the flags a session's launch saved (`nonCliIgnoredCommand`), and a wake never saves them again, so a chat started before 12.1.1 kept the old list for good; it now names only the flags Native still skips. Dismiss was one remembered card per chat on one device, which the next dismissed card replaced; it now tells gxserver, which keeps it per agent with the flags it named: every chat of that agent on every device loses the card, a new chat does not show it, and a command that skips other flags shows it again.
//!
//! CDXC:NativeApprovals 2026-11-25 DECISION:
//! User (#751 triage, "fix bugs and add dont show again"): the card also offers "Don't show again for this agent", a neutral button beside Dismiss. It is kept per agent in gxserver like Dismiss, but holds whatever flags the agent's command later skips: that agent never shows the card again, in any chat, on any device. Plain Dismiss keeps its rule (back when the skipped flags change).
//! SEE-ALSO: `spec::ignored_owner_command_tokens` (what a launch saves), session_chat_options/detection.rs `acp_terminal_notice` (the card), packages/gx-chat-core/src/questions/actions.rs `dismiss_notice` (the client's half).

use rusqlite::Connection;
use serde_json::{json, Map, Value};

use crate::domain::{DomainRepository, DomainStateError};
use crate::server::AppState;
use crate::storage::open_gxserver_database;

/// The card's Dismiss, as the chat sends it (`restartAgent` answer).
pub(crate) const ACTION_DISMISS: &str = "dismissIgnoredFlags";

/// The card's "Don't show again for this agent" (`restartAgent` answer).
pub(crate) const ACTION_NEVER_SHOW: &str = "neverShowIgnoredFlags";

/// gxserver metadata: `{ "<agentId>": "<skipped flags the dismissed card named>" | true }`, where
/// `true` is "Don't show again for this agent".
const METADATA_KEY: &str = "nativeIgnoredFlagsDismissed";

/// The skipped flags a session's saved list still names: a bypass flag Native now reads as Full
/// permissions (`spec::full_access_flag_width`) is left out. `None` when nothing is left.
pub(crate) fn still_ignored(family: &str, saved: &str) -> Option<String> {
    let tokens: Vec<&str> = saved.split_whitespace().collect();
    let mut kept = Vec::new();
    let mut i = 0;
    while i < tokens.len() {
        match super::spec::full_access_flag_width(family, tokens[i], tokens.get(i + 1).copied()) {
            0 => {
                kept.push(tokens[i]);
                i += 1;
            }
            width => i += width,
        }
    }
    (!kept.is_empty()).then(|| kept.join(" "))
}

fn read(db: &Connection) -> Map<String, Value> {
    crate::agents::settings::read_metadata_value(db, METADATA_KEY)
        .ok()
        .flatten()
        .and_then(|raw| serde_json::from_str::<Value>(&raw).ok())
        .and_then(|value| value.as_object().cloned())
        .unwrap_or_default()
}

/// Whether the card was dismissed for `agent_id` while it named exactly `ignored`, or turned off
/// for that agent.
pub(crate) fn dismissed(db: &Connection, agent_id: &str, ignored: &str) -> bool {
    match read(db).get(agent_id) {
        Some(Value::Bool(true)) => true,
        Some(Value::String(dismissed)) => dismissed == ignored,
        _ => false,
    }
}

/// The card's "Don't show again for this agent" button.
pub(crate) fn never_show_action() -> crate::session_chat_notice::SessionChatTerminalNoticeAction {
    crate::session_chat_notice::SessionChatTerminalNoticeAction {
        id: ACTION_NEVER_SHOW.to_string(),
        label: "Don't show again for this agent".to_string(),
        kind: crate::session_chat_notice::SessionChatTerminalNoticeActionKind::RestartAgent,
        send: None,
    }
}

/// The agent and the skipped flags a session's card names, when it has one.
fn card(session: &Value) -> Option<(String, String)> {
    let runtime = session.get("runtimeSettings")?.as_object()?;
    let saved = runtime
        .get(super::SESSION_NON_CLI_IGNORED_COMMAND_KEY)?
        .as_str()?;
    let agent_id = session.get("agentId")?.as_str()?.trim();
    let family = runtime
        .get(super::SESSION_ACP_FAMILY_KEY)
        .and_then(Value::as_str)
        .or(Some(agent_id))
        .and_then(super::canonical_native_family)?;
    Some((agent_id.to_string(), still_ignored(family, saved)?))
}

/// Dismiss on the card: kept for the session's agent with the flags it names, and the card leaves
/// every chat of that agent that names the same flags. With `never` ("Don't show again for this
/// agent") it is kept for the agent whatever it skips, and leaves every chat of that agent.
pub(crate) async fn answer_dismiss(
    state: &AppState,
    params: &Map<String, Value>,
    never: bool,
) -> Result<(), DomainStateError> {
    let field = |key: &str| {
        params
            .get(key)
            .and_then(Value::as_str)
            .map(str::to_string)
            .ok_or_else(|| DomainStateError::bad_request(format!("{key} is required.")))
    };
    let (project_id, session_id) = (field("projectId")?, field("sessionId")?);
    let state = state.clone();
    tokio::task::spawn_blocking(move || -> Result<(), DomainStateError> {
        let db = open_gxserver_database(&state.paths)
            .map_err(|error| DomainStateError::bad_request(error.to_string()))?;
        let repository = DomainRepository::new(&db, &state.metadata.server_id);
        let Some(dismissed_card) = repository
            .get_session(&project_id, &session_id)?
            .as_ref()
            .and_then(card)
        else {
            return Ok(());
        };
        let mut record = read(&db);
        // A Dismiss from a card still open on another device keeps "Don't show again".
        if record.get(&dismissed_card.0) != Some(&Value::Bool(true)) {
            let kept = if never {
                json!(true)
            } else {
                json!(dismissed_card.1)
            };
            record.insert(dismissed_card.0.clone(), kept);
        }
        db.execute(
            "INSERT INTO metadata (key, value, updatedAt) VALUES (?1, ?2, ?3)
             ON CONFLICT(key) DO UPDATE SET value = excluded.value, updatedAt = excluded.updatedAt",
            rusqlite::params![
                METADATA_KEY,
                Value::Object(record).to_string(),
                chrono::Utc::now().to_rfc3339()
            ],
        )
        .map_err(|error| DomainStateError::bad_request(error.to_string()))?;
        let detector = crate::session_chat_options::SessionChatOptionDetector::new(&state);
        for session in repository.list_sessions(None)? {
            let Some(other) = card(&session) else {
                continue;
            };
            if other.0 != dismissed_card.0 || (!never && other.1 != dismissed_card.1) {
                continue;
            }
            let mut runtime = session["runtimeSettings"]
                .as_object()
                .cloned()
                .unwrap_or_default();
            runtime.remove(super::SESSION_NON_CLI_IGNORED_COMMAND_KEY);
            crate::accounts::endpoint::update_session_members(
                &repository,
                &session,
                &runtime,
                &[super::SESSION_NON_CLI_IGNORED_COMMAND_KEY],
            )?;
            crate::accounts::endpoint::publish(&state, &repository, &session)?;
            detector.publish_screen_state(
                session["projectId"].as_str().unwrap_or_default(),
                session["sessionId"].as_str().unwrap_or_default(),
            );
        }
        Ok(())
    })
    .await
    .map_err(|_| DomainStateError::bad_request("Could not dismiss the card."))?
}
