//! Showing a session in the app: the `show_session` tool, and the follow every other tool that
//! works with one session does. Both reuse `/api/focusSession`, the route `ghostex focus` calls,
//! which asks the open desktop app to select the session in the sidebar and show it in the work
//! area, switching project when it lives in another one. A session whose project is parked in
//! Recent Projects has no sidebar row to select, so its project is restored first, the way a
//! Recent Projects click restores it (`/api/restoreRecentProject`).
//!
//! CDXC:VoiceMode 2026-10-20 DECISION:
//! User: "the voice assistant currently cannot switch which session is shown in the sidebar. Add that capability: it should be able to show a different session, including one in another project, and by default it should switch the user to whichever session it is talking about when the user is not actively typing, for example when reading a question from a session." `show_session` (the user asked to see it) always switches. The follow sends `unlessTyping`, and the desktop leaves the user where they are while they typed in the last few seconds or an IME composition is open (`USER_TYPING_WINDOW` in apps/desktop/src/app/voice_agent/typing.rs); the result tells the model so it can say it left the user where they are.

use std::sync::Arc;
use std::time::Duration;

use serde_json::{json, Value};

use super::loopback;
use super::sessions::SessionInfo;
use crate::server::AppState;

/// The tools that work with one session and show it afterwards (`start_agent` shows the new one).
pub(super) const FOLLOWING_TOOLS: &[&str] = &[
    "read_session",
    "get_pending_prompt",
    "answer_prompt",
    "draft_prompt",
    "send_prompt",
    "send_draft",
    "start_agent",
    "stop_session",
    "start_coordinator",
];

/// The session a successful tool call showed, for the client's tool event: `{projectId,
/// sessionId, title}`, plus `serverId` and `computer` for a session on another computer (the
/// conversation sends that one as `computerSession`, which only the desktop reads). Named whether or
/// not the desktop switched, since a client the call runs on (the phone's "Open <session>" chip)
/// shows it on its own.
///
/// CDXC:VoiceMode 2026-10-20 DECISION:
/// User, on what showing a session means on the phone: "Show an 'Open <session>' chip on the voice screen" (the latest session the agent showed, replacing the previous one, gone when the call ends; tapping it opens that session's chat and the call keeps running; the phone never navigates on its own). And a call from the phone still switches the desktop: "Yes, unless someone is typing there", which is what `follow` does for every client, since the desktop answers the focus whoever holds the call.
pub(super) fn shown_session(name: &str, output: &Value) -> Option<Value> {
    if name != "show_session" && !FOLLOWING_TOOLS.contains(&name) {
        return None;
    }
    let reference = output
        .pointer("/session/ref")
        .or_else(|| output.get("ref"))
        .and_then(Value::as_str)?;
    let (server_id, project_id, session_id) = split_reference(reference)?;
    // A new agent's result has no title yet: name it by its agent and project.
    let title = output
        .pointer("/session/title")
        .and_then(Value::as_str)
        .map(str::to_string)
        .or_else(|| {
            let agent = output.get("agent").and_then(Value::as_str)?;
            let project = output.get("project").and_then(Value::as_str)?;
            Some(format!("{agent} in {project}"))
        })?;
    let mut session = json!({"projectId": project_id, "sessionId": session_id, "title": title});
    if let Some(server_id) = server_id {
        session["serverId"] = json!(server_id);
        let computer = output
            .pointer("/session/computer")
            .or_else(|| output.get("computer"))
            .and_then(Value::as_str);
        if let Some(computer) = computer {
            session["computer"] = json!(computer);
        }
    }
    Some(session)
}

/// A tool's session ref: `projectId:sessionId` on this computer, `serverId:projectId:sessionId`
/// on another (`SessionInfo::reference`).
fn split_reference(reference: &str) -> Option<(Option<&str>, &str, &str)> {
    let mut parts = reference.split(':');
    let split = match (parts.next(), parts.next(), parts.next(), parts.next()) {
        (Some(project), Some(session), None, None) => Some((None, project, session)),
        (Some(server), Some(project), Some(session), None) => {
            Some((Some(server), project, session))
        }
        _ => None,
    };
    split.filter(|(_, project, session)| !project.is_empty() && !session.is_empty())
}

/// `show_session`: the user asked to see it, so it switches even while they type.
pub(super) async fn show_session(
    state: &Arc<AppState>,
    session: &SessionInfo,
) -> Result<Value, String> {
    let shown = async {
        restore_if_parked(state, &session.project_id).await?;
        focus(
            state,
            &session.project_id,
            &session.session_id,
            false,
            Duration::from_secs(15),
        )
        .await
    };
    shown
        .await
        .map_err(|error| format!("Couldn't show {}: {error}", session.title))?;
    Ok(json!({"ok": true, "shown": true, "session": session.brief()}))
}

/// Shows the session a following tool's successful output names (`session.ref`, or `ref` for a
/// new agent) unless the user is typing, and adds what happened to the output as `shownInApp`
/// (and `notShown` when it stayed). A failed show never fails the tool that already worked.
pub(super) async fn follow(state: &Arc<AppState>, output: &mut Value) {
    let reference = output
        .pointer("/session/ref")
        .or_else(|| output.get("ref"))
        .and_then(Value::as_str)
        .unwrap_or_default()
        .to_string();
    // A session on another computer is shown by the client from the tool frame
    // (`computerSession`): this computer's `/api/focusSession` selects only its own sessions.
    let Some((None, project_id, session_id)) = split_reference(&reference) else {
        return;
    };
    // Short: an app that does not answer must not hold up the reply the user waits to hear.
    let mut outcome = focus(state, project_id, session_id, true, Duration::from_secs(4)).await;
    // A parked project is restored only once the app said the user is not typing (that first
    // focus had no row to select), so typing never changes their sidebar either.
    let typing = |outcome: &Result<Value, String>| {
        outcome
            .as_ref()
            .is_ok_and(|result| result.get("reason").and_then(Value::as_str) == Some("userTyping"))
    };
    if outcome.is_ok() && !typing(&outcome) && is_parked(state, project_id).await {
        outcome = match restore_if_parked(state, project_id).await {
            Ok(()) => focus(state, project_id, session_id, false, Duration::from_secs(4)).await,
            Err(error) => Err(error),
        };
    }
    let Some(fields) = output.as_object_mut() else {
        return;
    };
    match outcome {
        Ok(_) if typing(&outcome) => {
            fields.insert("shownInApp".to_string(), json!(false));
            fields.insert(
                "notShown".to_string(),
                json!("The user is typing in Ghostex, so the app stayed where they are."),
            );
        }
        Ok(_) => {
            fields.insert("shownInApp".to_string(), json!(true));
        }
        Err(_) => {
            fields.insert("shownInApp".to_string(), json!(false));
        }
    }
}

/// Whether the project is parked in Recent Projects (closed from the sidebar).
async fn is_parked(state: &Arc<AppState>, project_id: &str) -> bool {
    let state = state.clone();
    let project_id = project_id.to_string();
    tokio::task::spawn_blocking(move || {
        let db = crate::storage::open_gxserver_database(&state.paths).ok()?;
        let repository =
            crate::domain::DomainRepository::new(&db, state.metadata.server_id.as_str());
        let project = repository.get_project(&project_id).ok()??;
        project.get("isRecentProject").and_then(Value::as_bool)
    })
    .await
    .ok()
    .flatten()
        == Some(true)
}

async fn restore_if_parked(state: &Arc<AppState>, project_id: &str) -> Result<(), String> {
    if !is_parked(state, project_id).await {
        return Ok(());
    }
    loopback::call(
        state,
        "/api/restoreRecentProject",
        json!({"projectId": project_id}),
        Duration::from_secs(10),
    )
    .await
    .map(|_| ())
}

async fn focus(
    state: &Arc<AppState>,
    project_id: &str,
    session_id: &str,
    unless_typing: bool,
    timeout: Duration,
) -> Result<Value, String> {
    let mut params = json!({"projectId": project_id, "sessionId": session_id});
    if unless_typing {
        params["unlessTyping"] = json!(true);
    }
    loopback::call(state, "/api/focusSession", params, timeout).await
}
