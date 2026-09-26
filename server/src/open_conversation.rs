//! `/api/openConversation`: take the user to a conversation. The session that owns it now if one
//! is in the presentation, else the exact session named if it is, else the daemon brings it back:
//! a recorded terminal session is restored into a new session, and a conversation that can be
//! resumed is resumed into a new session. The client then focuses the session in the answer.
//!
//! CDXC:SavedPrompts 2026-09-25 WHY:
//! This was the desktop app runtime's `jumpToStashedPromptSession` and
//! `openGpuiConversationSessionReference` (shared with the Project Board's conversation links), run
//! in QuickJS against its own presentation copy. Every client needs "go to the session for this
//! prompt", so the resolution and the restore or resume are the daemon's, made through the same
//! endpoints and with the same parameters the runtime used; only the final focus is the client's.
//!
//! SEE-ALSO: server/src/server/open_conversation_http.rs,
//! apps/desktop/src/app/stashed_prompt_jump.rs (the desktop caller).

use serde_json::{json, Map, Value};

use crate::domain::DomainStateError;

/// Agents whose conversation the board can resume (`GPUI_PROJECT_BOARD_RESUMABLE_AGENT_IDS`).
const RESUMABLE_AGENT_IDS: [&str; 3] = ["claude", "codex", "pi"];

/// The default title of a restored terminal (`DEFAULT_TERMINAL_SESSION_TITLE`).
const DEFAULT_TERMINAL_SESSION_TITLE: &str = "Terminal Session";

/// What the caller asks for.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct OpenConversationRequest {
    /// The provider's conversation id, which outlives the gxserver ids.
    pub agent_session_id: Option<String>,
    pub project_id: Option<String>,
    pub session_id: Option<String>,
    /// Stamped on `/api/removeSession` when the session is restored.
    pub restore_reason: String,
    /// Stamped on `/api/forkSession` when the conversation is resumed.
    pub resume_reason: String,
}

impl OpenConversationRequest {
    pub fn read(params: &Map<String, Value>) -> Result<Self, DomainStateError> {
        let text = |key: &str| {
            params
                .get(key)
                .and_then(Value::as_str)
                .map(str::trim)
                .filter(|value| !value.is_empty())
                .map(str::to_string)
        };
        // gxserver ids never contain `:`; a combined sidebar id is refused rather than guessed at.
        let raw_id = |key: &str| text(key).filter(|value| !value.contains(':'));
        let request = Self {
            agent_session_id: text("agentSessionId"),
            project_id: raw_id("projectId"),
            session_id: raw_id("sessionId"),
            restore_reason: text("restoreReason")
                .unwrap_or_else(|| "openConversationRestore".to_string()),
            resume_reason: text("resumeReason")
                .unwrap_or_else(|| "openConversationResume".to_string()),
        };
        if request.agent_session_id.is_none()
            && (request.project_id.is_none() || request.session_id.is_none())
        {
            return Err(DomainStateError::bad_request(
                "openConversation needs agentSessionId or projectId and sessionId.",
            ));
        }
        Ok(request)
    }
}

/// Opens the conversation. `call` performs one of this daemon's own endpoints and answers its
/// `result`.
pub fn open_conversation(
    request: &OpenConversationRequest,
    call: &dyn Fn(&str, Value) -> Result<Value, DomainStateError>,
) -> Result<Value, DomainStateError> {
    let snapshot = call("/api/readPresentationSnapshot", json!({}))?;
    let sessions = snapshot
        .get("snapshot")
        .and_then(|snapshot| snapshot.get("sessions"))
        .map(|sessions| sessions.get("rows").unwrap_or(sessions))
        .and_then(Value::as_array)
        .cloned()
        .unwrap_or_default();
    if let Some((project_id, session_id)) = live_target(&sessions, request) {
        return Ok(json!({ "outcome": "focus", "projectId": project_id, "sessionId": session_id }));
    }
    let (Some(project_id), Some(session_id)) = (&request.project_id, &request.session_id) else {
        // Nothing is live and there is no session row to restore or resume from.
        return Err(DomainStateError::not_found(
            "The session for this conversation is gone.",
        ));
    };
    let previous = call(
        "/api/listPreviousSessions",
        json!({
            "includeActive": false,
            "includePrevious": true,
            "limit": 20,
            "projectId": project_id,
            "query": session_id,
        }),
    )?;
    let row = previous
        .get("results")
        .and_then(Value::as_array)
        .and_then(|results| {
            results.iter().find(|result| {
                result.get("projectId").and_then(Value::as_str) == Some(project_id.as_str())
                    && result.get("sessionId").and_then(Value::as_str) == Some(session_id.as_str())
                    && result.get("lifecycleState").and_then(Value::as_str) != Some("running")
            })
        })
        .cloned();
    /*
    A dead session whose row still carries a resumable agent conversation gets the conversation
    back through the resume path: restoring only the terminal hands the user an empty shell while
    the conversation the link is about stays dead. The terminal-restore path remains for plain
    terminal sessions, where the shell is all there is to bring back.
    */
    let resume = match &row {
        None => true,
        Some(_) => {
            let plan = call(
                "/api/readAgentResumePlan",
                json!({ "projectId": project_id, "sessionId": session_id }),
            )?;
            resumable(&plan)
        }
    };
    if resume {
        let fork = call(
            "/api/forkSession",
            json!({
                "projectId": project_id,
                "reason": request.resume_reason,
                "sessionId": session_id,
            }),
        )?;
        let session = fork.get("fork").and_then(|fork| fork.get("session"));
        let Some(resumed_id) = non_empty(session, "sessionId") else {
            return Err(DomainStateError::bad_request(
                "The linked conversation could not be resumed.",
            ));
        };
        let resumed_project = non_empty(session, "projectId").unwrap_or(project_id.clone());
        return Ok(outcome("resumed", resumed_project, resumed_id, session));
    }
    let row = row.expect("a row decides between restore and resume");
    let mut create = Map::new();
    create.insert("kind".into(), json!("terminal"));
    create.insert("lifecycleState".into(), json!("running"));
    create.insert("projectId".into(), json!(project_id));
    create.insert("restoredFromSessionId".into(), json!(session_id));
    if let Some(tag) = row
        .get("sessionTag")
        .filter(|tag| tag.as_str().is_some_and(|tag| !tag.is_empty()))
    {
        create.insert("sessionTag".into(), tag.clone());
    }
    if let Some(order) = row.get("sidebarOrder").filter(|order| !order.is_null()) {
        create.insert("sidebarOrder".into(), order.clone());
    }
    create.insert("surface".into(), json!("workspace"));
    create.insert("title".into(), json!(row_title(&row)));
    let created = call("/api/createSession", Value::Object(create))?;
    let session = created.get("session");
    let Some(restored_id) = non_empty(session, "sessionId") else {
        return Err(DomainStateError::bad_request(
            "The linked Ghostex session could not be restored.",
        ));
    };
    let restored_project = non_empty(session, "projectId").unwrap_or(project_id.clone());
    // The stopped history row goes, as the Previous Sessions restore removes it; a failure here
    // leaves an extra history row and changes nothing else.
    let _ = call(
        "/api/removeSession",
        json!({
            "projectId": project_id,
            "reason": request.restore_reason,
            "sessionId": session_id,
        }),
    );
    Ok(outcome("restored", restored_project, restored_id, session))
}

/// The live session the request means: whichever presentation session now carries the
/// conversation id (sleeping included; focusing wakes it), else the exact session named.
fn live_target(sessions: &[Value], request: &OpenConversationRequest) -> Option<(String, String)> {
    let ids = |session: &Value| {
        Some((
            session.get("projectId")?.as_str()?.to_string(),
            session.get("sessionId")?.as_str()?.to_string(),
        ))
    };
    if let Some(agent_session_id) = &request.agent_session_id {
        if let Some(owner) = sessions.iter().find(|session| {
            session
                .get("agentSessionId")
                .and_then(Value::as_str)
                .is_some_and(|id| id.trim() == agent_session_id)
        }) {
            return ids(owner);
        }
    }
    let (project_id, session_id) = (request.project_id.as_ref()?, request.session_id.as_ref()?);
    sessions
        .iter()
        .find(|session| {
            session.get("projectId").and_then(Value::as_str) == Some(project_id.as_str())
                && session.get("sessionId").and_then(Value::as_str) == Some(session_id.as_str())
        })
        .and_then(ids)
}

/// `/api/readAgentResumePlan` plans a primary command for an agent the board can resume.
fn resumable(result: &Value) -> bool {
    let plan = result.get("plan");
    non_empty(plan, "primaryCommand").is_some()
        && non_empty(plan, "agentId")
            .is_some_and(|agent| RESUMABLE_AGENT_IDS.contains(&agent.to_lowercase().as_str()))
}

/// `gpuiProjectBoardPreviousSessionRowTitle`.
fn row_title(row: &Value) -> String {
    ["displayTitle", "primaryTitle", "title"]
        .iter()
        .find_map(|key| {
            row.get(*key)
                .and_then(Value::as_str)
                .filter(|title| !title.is_empty())
        })
        .unwrap_or(DEFAULT_TERMINAL_SESSION_TITLE)
        .to_string()
}

fn non_empty(value: Option<&Value>, key: &str) -> Option<String> {
    value?
        .get(key)?
        .as_str()
        .map(str::trim)
        .filter(|text| !text.is_empty())
        .map(str::to_string)
}

fn outcome(kind: &str, project_id: String, session_id: String, session: Option<&Value>) -> Value {
    let mut answer = json!({ "outcome": kind, "projectId": project_id, "sessionId": session_id });
    if let Some(zmx_name) = non_empty(session, "zmxName") {
        answer["zmxName"] = Value::String(zmx_name);
    }
    answer
}
