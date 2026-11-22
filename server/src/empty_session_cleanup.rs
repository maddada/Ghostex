//! New-session cleanup: when the user starts a new agent session in a project, that project's
//! other sessions that are still fully empty are closed. This file holds the marker, the "fully
//! empty" rule and the candidate list; `server/empty_session_cleanup_runtime.rs` reads each
//! candidate's input box and closes it.
//!
//! CDXC:Sessions 2026-10-09 DECISION:
//! User: "please disable the code that prevents having 2 empty sessions in the sidebar this code is buggy when the user has terminals as his preferred interface", then: "setting to enable this just for preferred chat view user (advanced). let's set disabled by default". The cleanup runs only when the advanced setting `closeEmptySessionsOnNew` is on (default off) and the new session's agent opens in Chat (its own Default view, else the Default Agent View, which means Chat when unset); with Terminal it never runs, even with the setting on. gxserver decides (`cleanup_applies`), so the desktop, the web build, the phone and the CLI all follow it, and clients may keep sending `replaceEmptySessions: true`. This supersedes the 2026-10-04 decision that closed a project's other fully empty sessions on every new-session action (the new-session hotkey, a project's agent button or menu, the New Thread picker, the phone's new session; the close is the ordinary quiet `/api/transitionSession` close with one log line). A client sends `replaceEmptySessions: true` on `/api/createAgentSession` only for those user actions.
//!
//! CDXC:Sessions 2026-10-06 WHY:
//! Only a session made by that same user action carries the marker, so a session an agent, the CLI, a coordinator, the board or an automation started (it may be waiting for its task) is never a candidate. The marker stays on the row for its whole life, so "never prompted" must be positively known, never inferred from a signal that is missing: issue #204 saw sessions the user had been working in (mostly Claude) closed by the next new session, because the 2026-10-04 rule counted a session as never prompted when it was still a draft OR had no `lastActiveAt`, and each of those survives a real prompt (a prompt typed into the terminal whose hook was rejected or never arrived keeps the draft marker; a prompt whose turn was never seen as working leaves `lastActiveAt` empty). A candidate now has to pass every never-used signal at once: still a draft, never active, never seen working, no first user message from a hook, and no agent transcript on disk (agents write none before the first prompt); a session that was not created as a draft is never a candidate, since gxserver cannot see what was typed into its terminal. It must also have no chat draft text (parked ones included), nothing queued or armed, no note or stash, not be pinned, parked, favorited, tagged, renamed, a coordinator or thread, or armed for Close After Done, be idle, in the same folder, not shown by any client (a pane, the focused session, a phone attached to it), and its agent input box must read as empty; a box that cannot be read counts as holding text. This supersedes the 2026-10-04 WHY and its "or an agent that has never been active" rule, and "a draft is never thrown away on its own" (CDXC:Drafts 2026-08-29 in agents/drafts.rs) for exactly these sessions.
//!
//! SEE-ALSO: server/src/server/empty_session_cleanup_runtime.rs, apps/desktop/src/app/gx_store/create/agent.rs, apps/desktop/src/app/helpers/agents_hub/workspace_agent_actions.rs, server/src/ghostex_cli/actions/create.rs (`--replace-empty-sessions`, the phone's new session).

use rusqlite::{params, Connection, OptionalExtension};
use serde_json::{json, Map, Value};

use crate::agents::session_is_draft;
use crate::domain::{DomainRepository, DomainStateError};

/// The `/api/createAgentSession` parameter a client sends for the user's new-session action.
pub(crate) const REPLACE_EMPTY_SESSIONS_PARAM: &str = "replaceEmptySessions";
/// The advanced Settings toggle (default off) that lets the cleanup run at all.
const CLOSE_EMPTY_SESSIONS_SETTING: &str = "closeEmptySessionsOnNew";
/// Server-owned: written at creation only when the create carried the parameter above.
const NEW_SESSION_MARKER_KEY: &str = "userNewSession";

pub(crate) fn requests_empty_session_cleanup(params: &Map<String, Value>) -> bool {
    params
        .get(REPLACE_EMPTY_SESSIONS_PARAM)
        .and_then(Value::as_bool)
        == Some(true)
}

/// The cleanup may run for this new session: the advanced setting is on and the session's agent
/// opens in Chat. An unreadable settings file means the setting is off.
pub(crate) fn cleanup_applies(paths: &crate::paths::GxserverPaths, new_session: &Value) -> bool {
    let Some(settings) = crate::session_lifecycle::read_sidebar_settings(paths) else {
        return false;
    };
    if settings
        .get(CLOSE_EMPTY_SESSIONS_SETTING)
        .and_then(Value::as_bool)
        != Some(true)
    {
        return false;
    }
    fn view(value: Option<&Value>) -> Option<&str> {
        value
            .and_then(Value::as_str)
            .filter(|value| matches!(*value, "chat" | "terminal"))
    }
    let overrides = settings.pointer("/preferredAgentInterfaceOverrides");
    let agent_override = crate::session_chat_composer::session_chat_composer_agent_id(new_session)
        .and_then(|agent_id| {
            view(overrides.and_then(|map| map.get(&agent_id))).map(str::to_string)
        });
    agent_override
        .as_deref()
        .or_else(|| view(settings.get("preferredAgentInterface")))
        .is_none_or(|view| view == "chat")
}

/// Arms the marker beside the draft marker; a client cannot set it through `runtimeSettings`.
pub(crate) fn apply_new_session_marker(
    params: &Map<String, Value>,
    runtime_settings: &mut Map<String, Value>,
) {
    if requests_empty_session_cleanup(params) {
        runtime_settings.insert(NEW_SESSION_MARKER_KEY.to_string(), json!(true));
    } else {
        runtime_settings.remove(NEW_SESSION_MARKER_KEY);
    }
}

/// A session the rule may close once its agent's input box reads as empty.
#[derive(Clone, Debug)]
pub(crate) struct EmptySessionCandidate {
    pub session_id: String,
    pub zmx_name: String,
    pub agent_id: String,
}

/// The new session's project siblings that are empty as far as gxserver's own state can tell.
pub(crate) fn empty_session_candidates(
    db: &Connection,
    repository: &DomainRepository<'_>,
    project_id: &str,
    new_session_id: &str,
) -> Result<Vec<EmptySessionCandidate>, DomainStateError> {
    let Some(new_session) = repository.get_session(project_id, new_session_id)? else {
        return Ok(Vec::new());
    };
    let folder = new_session.get("cwd").cloned();
    let mut candidates = Vec::new();
    for session in repository.list_sessions_excluding_stopped(Some(project_id))? {
        let Some(session_id) = session.get("sessionId").and_then(Value::as_str) else {
            continue;
        };
        if session_id == new_session_id
            || session.get("cwd").cloned() != folder
            || !row_is_empty(&session)
            // Shown in a pane, focused, or attached from the phone: clients hold what they show.
            || crate::session_keep_awake::is_held_awake(project_id, session_id)
        {
            continue;
        }
        let presentation = crate::presentation::build_presentation_session_delta(
            db, repository, project_id, session_id,
        )?;
        if !presentation
            .get("session")
            .is_some_and(presentation_is_empty)
            || holds_chat_text(db, project_id, session_id)
            || crate::session_chat_queue::session_has_pending_session_chat_queue(
                db, project_id, session_id,
            )
        {
            continue;
        }
        let (Ok(zmx_name), Some(agent_id)) = (
            crate::zmx::provider_zmx_session_name(&session),
            crate::session_chat_composer::session_chat_composer_agent_id(&session),
        ) else {
            continue;
        };
        candidates.push(EmptySessionCandidate {
            session_id: session_id.to_string(),
            zmx_name,
            agent_id,
        });
    }
    Ok(candidates)
}

/// The durable row: made by the new-session action, positively never used, untagged and never
/// renamed.
fn row_is_empty(session: &Value) -> bool {
    let runtime_settings = session.get("runtimeSettings").and_then(Value::as_object);
    let setting = |key: &str| runtime_settings.and_then(|settings| settings.get(key));
    setting(NEW_SESSION_MARKER_KEY).and_then(Value::as_bool) == Some(true)
        && session_is_draft(session)
        && !crate::zmx::session_has_ever_been_active(session)
        && !setting("agentActivity").is_some_and(activity_shows_use)
        && setting("firstUserMessage")
            .and_then(Value::as_str)
            .is_none_or(|message| message.trim().is_empty())
        && !setting("agentSessionPath")
            .and_then(Value::as_str)
            .is_some_and(transcript_may_exist)
        && session.get("kind").and_then(Value::as_str) == Some("agent")
        && !crate::presentation::session_tag_is_truthy(session)
        && setting("pendingAgentTitleRequestStatus").is_none()
        && !matches!(
            setting("titleSource").and_then(Value::as_str),
            Some("user" | "generated")
        )
}

/// The stored activity has seen the agent work or recorded meaningful activity.
fn activity_shows_use(activity: &Value) -> bool {
    activity.get("hasSeenWorking").and_then(Value::as_bool) == Some(true)
        || activity
            .get("lastMeaningfulActivityAt")
            .and_then(Value::as_str)
            .is_some_and(|at| !at.trim().is_empty())
}

/// The agent's transcript file exists, or this machine cannot tell that it does not.
fn transcript_may_exist(path: &str) -> bool {
    let path = path.trim();
    !path.is_empty()
        && std::fs::metadata(path).map_or_else(
            |error| error.kind() != std::io::ErrorKind::NotFound,
            |_| true,
        )
}

/// The sidebar row, with every overlay a client would show (draft dot, queue, delayed send, note,
/// stash, coordinator, Close After Done).
fn presentation_is_empty(session: &Value) -> bool {
    let flag = |key: &str| session.get(key).and_then(Value::as_bool) == Some(true);
    let count = |key: &str| session.get(key).and_then(Value::as_u64).unwrap_or(0);
    session.get("lifecycleState").and_then(Value::as_str) == Some("running")
        && session.get("activity").and_then(Value::as_str) == Some("idle")
        && ![
            "isPinned",
            "isParked",
            "isFavorite",
            "hasComposerDraft",
            "closeAfterDone",
        ]
        .into_iter()
        .any(flag)
        && [
            "queuedPromptCount",
            "queuedPromptFailedCount",
            "stashedPromptCount",
            "pendingQuestionCount",
        ]
        .into_iter()
        .all(|key| count(key) == 0)
        && [
            "coordinatorRole",
            "delayedSendDeadlineAt",
            "sendWhenAllProjectSessionsStopActive",
            "sendWhenAgentStopsActive",
            "sendWhenSpecificAgentFinishes",
            "sessionNote",
        ]
        .into_iter()
        .all(|key| session.get(key).is_none_or(Value::is_null))
}

/// Any synced chat draft with text, parked recovery text included; a failed read counts as text.
fn holds_chat_text(db: &Connection, project_id: &str, session_id: &str) -> bool {
    db.query_row(
        "SELECT 1 FROM session_chat_drafts WHERE projectId = ?1 AND sessionId = ?2 AND TRIM(content) <> '' LIMIT 1",
        params![project_id, session_id],
        |_| Ok(()),
    )
    .optional()
    .map_or(true, |row| row.is_some())
}
