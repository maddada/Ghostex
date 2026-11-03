//! Stopping a session's turn and starting a coordinator by voice, and the confirm card a stop the
//! voice agent is unsure about puts on screen.
//!
//! CDXC:VoiceMode 2026-10-30 DECISION:
//! User (Q782): stopping a session by voice stops it right away, the same as the chat's Stop button (the turn ends, the session stays). Only when the voice agent isn't sure which session is meant does it ask first, with a confirm card (Stop / Cancel). Closing a session is a stronger action that would always ask first; the voice agent has no close tool. `stop_session` reuses `/api/interruptSessionChat`, the route the chat's Stop and `ghostex interrupt-session-chat` call.
//!
//! CDXC:VoiceMode 2026-10-30 DECISION:
//! User (Q783): "start a coordinator" by voice starts a coordinator right away, with what was said as its first message, like starting a normal agent by voice. `start_coordinator` sends the `/api/createAgentSession` request `ghostex coordinator create` sends (`coordinator_create_params`), so its agent, model and effort defaults are the CLI's.

use std::sync::Arc;
use std::time::Duration;

use serde_json::{json, Value};

use super::computers;
use super::focus::Purpose;
use super::sessions::{self, Match, SessionInfo, VoiceState};
use super::tools::{self, ToolMemory, ToolOutcome};
use crate::server::AppState;

/// Which session a stop names, and whether the voice agent may stop it without asking.
#[derive(Debug)]
pub(super) enum StopTarget {
    /// The user's words name this session plainly: stop it now.
    Sure(SessionInfo),
    /// The words fit these sessions only loosely (or several alike): ask first, with the card.
    Unsure(Vec<SessionInfo>),
}

/// The session `query` names for a stop. Sure: its ref or id, a name already resolved earlier in
/// the call, its exact title, or a title holding every word the user said ("the Discord one" for
/// "Discord triage"). A match that leans on the project or agent, misses some of the words, or
/// fits several sessions is unsure.
pub(super) fn stop_target(
    memory: &ToolMemory,
    all: &[SessionInfo],
    query: &str,
) -> Result<StopTarget, String> {
    if let Some(reference) = memory.recall(query) {
        if let Some(session) = all.iter().find(|session| session.reference() == reference) {
            return Ok(StopTarget::Sure(session.clone()));
        }
    }
    match sessions::match_session(all, query) {
        Match::One(session) => {
            let trimmed = query.trim();
            let exact = session.reference() == trimmed
                || session.session_id == trimmed
                || trimmed.ends_with(&format!(":{}", session.reference()))
                || session.title.trim().eq_ignore_ascii_case(trimmed);
            if exact || sessions::title_has_every_word(&session.title, query) {
                memory.remember(query, &session);
                Ok(StopTarget::Sure(session))
            } else {
                Ok(StopTarget::Unsure(vec![session]))
            }
        }
        Match::Ambiguous(candidates) => Ok(StopTarget::Unsure(candidates)),
        Match::None => Err(format!(
            "{} \"{query}\". Call list_sessions to see their titles.",
            tools::NO_MATCH
        )),
    }
}

pub(super) async fn stop_session(
    state: &Arc<AppState>,
    memory: &ToolMemory,
    args: &Value,
) -> Result<Value, String> {
    // A call about one session stops only that session.
    let target = if memory.scope().is_some() {
        StopTarget::Sure(tools::resolve_session(state, memory, args, Purpose::Send).await?)
    } else {
        let query = tools::arg_text(args, "session");
        if query.is_empty() {
            return Err("Say which session.".to_string());
        }
        let (all, unavailable) = sessions::list_everywhere_async(state).await?;
        stop_target(memory, &all, &query)
            .map_err(|error| computers::with_unreachable(error, &unavailable))?
    };
    let session = match target {
        StopTarget::Sure(session) => session,
        StopTarget::Unsure(candidates) => return Ok(unsure_result(&candidates)),
    };
    if session.state == VoiceState::Sleeping {
        return Ok(json!({
            "ok": true,
            "stopped": false,
            "session": session.brief(),
            "note": "It is asleep, so it isn't running anything to stop.",
        }));
    }
    computers::call(
        state,
        session.computer.as_ref(),
        "/api/interruptSessionChat",
        json!({"projectId": session.project_id, "sessionId": session.session_id}),
        Duration::from_secs(20),
    )
    .await?;
    let working = matches!(
        session.state,
        VoiceState::Working | VoiceState::WaitingOnQuestion | VoiceState::WaitingOnApproval
    );
    Ok(json!({
        "ok": true,
        "stopped": true,
        "session": session.brief(),
        "wasWorking": working,
        "note": if working {
            "Its turn was stopped, like pressing Stop in its chat. The session stays open and waits for the user's next prompt."
        } else {
            "It had already finished its turn; Stop was pressed anyway. The session stays open."
        },
    }))
}

/// What the model hears when a stop was not sure: nothing was stopped, the card is up.
pub(super) fn unsure_result(candidates: &[SessionInfo]) -> Value {
    let note = if candidates.len() == 1 {
        "Nothing was stopped: the user's words fit this session only loosely. A card on screen asks to stop it. Ask the user in a few words whether you should stop it (say its title). If they agree, call stop_session again with its ref; if not, say you left it running."
    } else {
        "Nothing was stopped: the user's words fit several sessions. A card on screen lists them. Ask which one to stop, naming them briefly, then call stop_session with that one's ref. Never guess."
    };
    json!({
        "ok": true,
        "stopped": false,
        "needsConfirmation": true,
        "candidates": candidates.iter().map(SessionInfo::to_tool_value).collect::<Vec<_>>(),
        "note": note,
    })
}

/// The tool row's label once a stop turned into a question.
pub(super) const UNSURE_LABEL: &str = "Checking which session to stop";

/// The `confirm` frame for a stop result that needs the user's yes, keyed by the tool call's id.
/// The client draws it as a card: `title`, one row per choice with a `confirmLabel` button, and a
/// Cancel button; it answers `{type: "confirm", id, session: <ref> | null}`.
pub(super) fn confirm_frame(id: &str, output: &Value) -> Option<Value> {
    if output.get("needsConfirmation").and_then(Value::as_bool) != Some(true) {
        return None;
    }
    let choices: Vec<Value> = output
        .get("candidates")
        .and_then(Value::as_array)?
        .iter()
        .filter_map(|candidate| {
            let reference = candidate.get("ref").and_then(Value::as_str)?;
            let field = |key: &str| candidate.get(key).and_then(Value::as_str).unwrap_or("");
            let detail = [field("project"), field("state")]
                .into_iter()
                .filter(|part| !part.is_empty())
                .collect::<Vec<_>>()
                .join(" · ");
            Some(json!({"session": reference, "title": field("title"), "detail": detail}))
        })
        .collect();
    let title = match choices.as_slice() {
        [] => return None,
        [only] => format!("Stop {}?", only["title"].as_str().unwrap_or("this session")),
        _ => "Stop which session?".to_string(),
    };
    Some(json!({
        "type": "confirm",
        "id": id,
        "action": "stopSession",
        "title": title,
        "choices": choices,
        "confirmLabel": "Stop",
        "cancelLabel": "Cancel",
    }))
}

/// The confirm card on screen, which a stop of one of its sessions or a button press closes.
#[derive(Clone, Debug)]
pub(super) struct OpenConfirm {
    pub(super) id: String,
    /// `(ref, title)` of each session the card offers.
    choices: Vec<(String, String)>,
}

impl OpenConfirm {
    pub(super) fn from_frame(frame: &Value) -> Option<Self> {
        Some(Self {
            id: frame.get("id")?.as_str()?.to_string(),
            choices: frame
                .get("choices")?
                .as_array()?
                .iter()
                .filter_map(|choice| {
                    Some((
                        choice.get("session")?.as_str()?.to_string(),
                        choice.get("title")?.as_str()?.to_string(),
                    ))
                })
                .collect(),
        })
    }

    /// The title of the card's session `reference`, when the card offers it.
    pub(super) fn title_of(&self, reference: &str) -> Option<&str> {
        self.choices
            .iter()
            .find(|(choice, _)| choice == reference)
            .map(|(_, title)| title.as_str())
    }
}

/// The ref a successful `stop_session` stopped.
pub(super) fn stopped_ref(output: &Value) -> Option<&str> {
    if output.get("stopped").and_then(Value::as_bool) != Some(true) {
        return None;
    }
    output.pointer("/session/ref").and_then(Value::as_str)
}

/// The `confirmDone` frame that takes the card `id` off screen.
pub(super) fn confirm_done_frame(id: &str, outcome: &str) -> Value {
    json!({"type": "confirmDone", "id": id, "outcome": outcome})
}

/// The prefix of the messages that tell the model what the user pressed on a card.
pub(super) const CARD_PREFIX: &str = "[Ghostex card]";

/// What the model is told after the user pressed Stop on the card and the stop ran.
pub(super) fn card_stop_message(title: &str, output: &Value) -> String {
    format!("{CARD_PREFIX} The user pressed Stop on the card for \"{title}\". stop_session result: {output}")
}

/// What the model is told after the user pressed Cancel on the card.
pub(super) fn card_cancel_message() -> String {
    format!("{CARD_PREFIX} The user pressed Cancel on the stop card: nothing was stopped, leave it running.")
}

/// The launcher a voice-started coordinator runs on: the one the user named (it must be an agent
/// a coordinator can run on), else their Default Prompt Agent when it can, else the first agent
/// that can, Claude first (`ghostex coordinator create`'s default).
pub(super) fn coordinator_agent(
    rows: &[Value],
    requested: &str,
    preferred: Option<&str>,
) -> Result<String, String> {
    let id_of = |row: &Value| {
        row.get("agentId")
            .and_then(Value::as_str)
            .map(str::to_string)
    };
    let name_of = |row: &Value| {
        row.get("name")
            .and_then(Value::as_str)
            .or_else(|| row.get("agentId").and_then(Value::as_str))
            .unwrap_or_default()
            .to_lowercase()
    };
    if !requested.is_empty() {
        let lower = requested.to_lowercase();
        let row = rows
            .iter()
            .find(|row| {
                id_of(row).is_some_and(|id| id.to_lowercase() == lower) || name_of(row) == lower
            })
            .or_else(|| {
                rows.iter().find(|row| {
                    id_of(row).is_some_and(|id| id.to_lowercase().contains(&lower))
                        || name_of(row).contains(&lower)
                })
            })
            .ok_or_else(|| {
                format!("No agent type matches \"{requested}\". Call list_agent_types.")
            })?;
        if crate::coordinators::coordinator_launcher_family(row).is_none() {
            return Err(format!(
                "A coordinator can't run on \"{requested}\"; it runs on {}. Ask the user which to use.",
                crate::coordinators::coordinator_agent_families_text()
            ));
        }
        return id_of(row).ok_or_else(|| "That agent has no id.".to_string());
    }
    if let Some(preferred) = preferred {
        if rows.iter().any(|row| {
            id_of(row).as_deref() == Some(preferred)
                && crate::coordinators::coordinator_launcher_family(row).is_some()
        }) {
            return Ok(preferred.to_string());
        }
    }
    crate::coordinators::default_coordinator_launcher(rows).ok_or_else(|| {
        format!(
            "No agent a coordinator can run on is set up ({}).",
            crate::coordinators::coordinator_agent_families_text()
        )
    })
}

pub(super) async fn start_coordinator(state: &Arc<AppState>, args: &Value) -> ToolOutcome {
    let project = match tools::project_named(state, &tools::arg_text(args, "project")).await {
        Ok(project) => project,
        Err(error) => return tools::failure(error),
    };
    let computer = project.computer.as_ref();
    let hud = match computers::call(
        state,
        computer,
        "/api/readSidebarHud",
        json!({}),
        Duration::from_secs(10),
    )
    .await
    {
        Ok(hud) => hud,
        Err(error) => return tools::failure(error),
    };
    let rows = hud
        .get("agents")
        .and_then(Value::as_array)
        .cloned()
        .unwrap_or_default();
    let settings = super::keys::read_settings(&state.paths);
    let preferred = settings.get("defaultPromptAgentId").and_then(Value::as_str);
    let agent_id = match coordinator_agent(&rows, &tools::arg_text(args, "agent"), preferred) {
        Ok(agent_id) => agent_id,
        Err(error) => return tools::failure(error),
    };
    let title = tools::arg_text(args, "title");
    let message = tools::arg_text(args, "message");
    let params =
        crate::coordinators::coordinator_create_params(crate::coordinators::CoordinatorLaunch {
            agent_rows: &rows,
            project_id: &project.project_id,
            agent_id: &agent_id,
            title: (!title.is_empty()).then_some(title),
            goal: String::new(),
            task: message.clone(),
            model: None,
            effort: None,
        });
    let created = match computers::call(
        state,
        computer,
        "/api/createAgentSession",
        params,
        Duration::from_secs(30),
    )
    .await
    {
        Ok(created) => created,
        Err(error) => return tools::failure(format!("Couldn't create the coordinator: {error}")),
    };
    let session = created.get("session").cloned().unwrap_or(Value::Null);
    let field = |key: &str| session.get(key).and_then(Value::as_str).map(str::to_string);
    let (Some(project_id), Some(session_id)) = (field("projectId"), field("sessionId")) else {
        return tools::failure("gxserver created no session.");
    };
    if let Some(computer) = computer {
        computers::forget(state, &computer.server_id);
    }
    if let Err(error) = computers::call(
        state,
        computer,
        "/api/startSessionProvider",
        json!({"projectId": project_id, "sessionId": session_id}),
        Duration::from_secs(60),
    )
    .await
    {
        return tools::failure(format!(
            "The coordinator was created but its agent did not start: {error}"
        ));
    }
    let agent = rows
        .iter()
        .find(|row| row.get("agentId").and_then(Value::as_str) == Some(agent_id.as_str()))
        .and_then(|row| row.get("name").and_then(Value::as_str))
        .unwrap_or(agent_id.as_str())
        .to_string();
    let mut output = json!({
        "ok": true,
        "started": true,
        "coordinator": true,
        "project": project.name,
        "agent": agent,
        "ref": tools::new_session_ref(computer, &project_id, &session_id),
        // gxserver queued the first message (the skill line, then this) before it answered.
        "firstMessageQueued": !message.is_empty(),
    });
    if let Some(computer) = computer {
        output["computer"] = json!(computer.name);
    }
    if let Some(error) = created
        .get("coordinatorRoleError")
        .and_then(Value::as_str)
        .filter(|error| !error.is_empty())
    {
        output["warning"] = json!(format!(
            "Its coordinator role could not be set up: {error}. Tell the user it started as a normal session."
        ));
    }
    ToolOutcome { output, ok: true }
}
