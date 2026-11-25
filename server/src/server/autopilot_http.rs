//! `/api/autopilot`: Settings > Autopilot, the chat pills, the sidebar's project menu and
//! `ghostex autopilot` read and change Autopilot here.
//!
//! - `op: "read"` (default): the choices, the projects and the sessions set one by one, the
//!   summary since the switch went on, and the newest log lines (`limit`, 200 by default).
//! - `op: "set"`: `patch` holds the choices to change (`enabled`, `scope`, `tier`, `limits`,
//!   `answerQuestions`, `answerAfterMinutes`, `answerApprovals`, `readOnlyLeavePlan`,
//!   `useLimitResets`, `supervisor`, `supervisorPick`, `supervisorFallbacks`, `sweepSeconds`,
//!   `checkupMinutes`). `supervisorPick` is `{agent, model, effort}` (effort empty for a model
//!   with none); `supervisor` is the old `opusMedium` / `solHigh` (`custom` for any other pick).
//!   Only the user's own computer or phone may turn `useLimitResets` on or change
//!   `supervisorPick` / `supervisorFallbacks`; an agent's call is refused.
//! - `op: "supervisorMenu"` / `op: "supervisorPick"`: the Supervisor model and When it's at its
//!   limit rows' pickers (autopilot_supervisor_menu.rs).
//! - `op: "setProject"`: `projectId` and `on`; a worktree's main project is changed instead.
//! - `op: "setSession"`: `projectId`, `sessionId` and `on` (`toggle: true` flips it).
//! - `op: "decide"`: the supervisor's decision for one session (`ghostex autopilot decide`):
//!   `projectId`, `sessionId`, and `answers` (one pick per open question, in order), `message`
//!   (sent to it as the user's), `reload: true` (reload it on its own conversation) or `leave` (a
//!   note: left for the user). Ghostex applies it and logs
//!   it; the supervisor never answers a prompt or types into a session itself.
//!
//! Every change republishes the projects (the sidebar's Advanced > Autopilot row) and the open
//! chats (their pill).

use super::*;

use crate::autopilot::{self, AutopilotConfig};

pub(crate) const AUTOPILOT_ENDPOINTS: &[&str] = &["/api/autopilot"];

pub(crate) async fn handle_autopilot_http(
    state: &Arc<AppState>,
    caller: &crate::security::Principal,
    endpoint_path: String,
    request_id: String,
    body: &Value,
) -> RoutedResponse {
    let params = match read_domain_rpc_params(body) {
        Ok(params) => params,
        Err(error) => {
            return routed_json(
                Some(endpoint_path),
                StatusCode::BAD_REQUEST,
                rpc_error("badRequest", error.message, Some(request_id)),
            );
        }
    };
    let result = if params.get("op").and_then(Value::as_str) == Some("decide") {
        decide(state, &params).await
    } else {
        handle(state, caller, &params)
    };
    match result {
        Ok(result) => routed_json(
            Some(endpoint_path),
            StatusCode::OK,
            rpc_success(request_id, result),
        ),
        Err(error) => routed_json(
            Some(endpoint_path),
            if error.code == "badRequest" {
                StatusCode::BAD_REQUEST
            } else if error.code == "forbidden" {
                StatusCode::FORBIDDEN
            } else {
                StatusCode::INTERNAL_SERVER_ERROR
            },
            rpc_error(error.code, error.message, Some(request_id)),
        ),
    }
}

fn text(params: &Map<String, Value>, key: &str) -> Option<String> {
    params
        .get(key)
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(str::to_string)
}

fn saved(result: std::io::Result<AutopilotConfig>) -> Result<AutopilotConfig, DomainStateError> {
    result.map_err(|error| DomainStateError {
        code: "internalError",
        message: format!("Ghostex could not save Autopilot's settings: {error}"),
    })
}

/// CDXC:Autopilot 2026-11-21 DECISION:
/// "Use a limit reset when an account hits its limit" spends the user's banked limit resets, so only the user turns it on: the desktop app's owner session, the token file their own terminal uses, or a paired phone. An agent's session token, another computer, an extension or a Collab guest is refused and the refusal is kept in the security log. Turning it off is open to everyone. As on every owner route, an agent that reads the token file itself instead of using its session token is the one way around this.
const LIMIT_RESETS_ONLY_YOU: &str = "Only the user can turn on \"Use a limit reset when an account hits its limit\", in Settings > Autopilot or with ghostex autopilot limit-resets on in their own terminal outside Ghostex. Tell the user; never turn it on for them.";

/// The supervisor runs without asking, so only the user picks the agent it runs on.
const SUPERVISOR_ONLY_YOU: &str = "Only the user can change the agent, model or effort Autopilot's supervisor runs on, in Settings > Autopilot. Tell the user; never change it for them.";

/// While "Use a limit reset when an account hits its limit" is on, turning Autopilot on arms it.
const AUTOPILOT_ON_ONLY_YOU: &str = "While \"Use a limit reset when an account hits its limit\" is on, only the user can turn Autopilot on, in Settings > Autopilot or with ghostex autopilot on in their own terminal outside Ghostex. Tell the user; never turn it on for them.";

/// CDXC:Security 2026-11-21 DECISION:
/// Fable's security review (findings 4 and 5, proven live): `env -u GHOSTEX_SESSION_TOKEN ghostex autopilot limit-resets on` inside a session reached gxserver through the token file and was taken as the user, and with the switch already on an agent's own token could turn Autopilot on, which arms the spending again. The user is the desktop app's owner session, a paired phone, or the token file from a terminal that names no Ghostex session (the CLI sends the session it runs in, `token_file_names_session`); and while the switch is on, turning Autopilot on is the user's too. What stays open is the same-user limit Ghostex can't close: an agent that sends the token file without the session header, or edits autopilot.json.
fn is_user(caller: &crate::security::Principal, token_file_names_session: bool) -> bool {
    caller.is_owner_device()
        && !(matches!(caller, crate::security::Principal::LocalAgent) && token_file_names_session)
}

/// Why this patch is refused to a caller that is not the user, or `None`: it turns on "Use a limit
/// reset when an account hits its limit", changes the supervisor's agent, model or effort, or turns
/// Autopilot on while that switch is on (or with it).
fn refuses_limit_resets(
    before: &AutopilotConfig,
    patch: &Value,
    caller: &crate::security::Principal,
    token_file_names_session: bool,
) -> Option<(&'static str, &'static str)> {
    if is_user(caller, token_file_names_session) {
        return None;
    }
    let resets_on = patch["useLimitResets"] == true;
    if resets_on && !before.use_limit_resets {
        return Some((
            "Turn on Autopilot's \"Use a limit reset when an account hits its limit\"",
            LIMIT_RESETS_ONLY_YOU,
        ));
    }
    // CDXC:Autopilot 2026-11-26 DECISION:
    // The supervisor runs without asking (#765), so which agent, model and effort it runs on is the
    // user's to pick: an agent may still pick between the two old choices (`supervisor`), as before.
    let changes = |key: &str| {
        patch.get(key).is_some_and(|value| {
            serde_json::to_value(before)
                .ok()
                .and_then(|before| before.get(key).cloned())
                != Some(value.clone())
        })
    };
    if changes("supervisorPick") || changes("supervisorFallbacks") {
        return Some((
            "Change the agent, model or effort Autopilot's supervisor runs on",
            SUPERVISOR_ONLY_YOU,
        ));
    }
    if patch["enabled"] == true && !before.enabled && (before.use_limit_resets || resets_on) {
        return Some((
            "Turn on Autopilot while \"Use a limit reset when an account hits its limit\" is on",
            AUTOPILOT_ON_ONLY_YOU,
        ));
    }
    None
}

fn handle(
    state: &AppState,
    caller: &crate::security::Principal,
    params: &Map<String, Value>,
) -> Result<Value, DomainStateError> {
    let op = text(params, "op").unwrap_or_else(|| "read".to_string());
    let db = open_gxserver_database(&state.paths).map_err(|error| DomainStateError {
        code: "internalError",
        message: error.to_string(),
    })?;
    let repository = DomainRepository::new(&db, state.metadata.server_id.as_str());
    let before = autopilot::read_config(&state.paths);
    let after = match op.as_str() {
        "read" => None,
        "supervisorMenu" => {
            return Ok(super::autopilot_supervisor_menu::menu(
                &repository,
                &before,
                params,
            ))
        }
        "supervisorPick" => {
            if !is_user(caller, crate::audit::token_file_names_session()) {
                crate::audit::record(
                    crate::audit::Action::new(
                        "autopilot.widen",
                        "Change the agent, model or effort Autopilot's supervisor runs on",
                    ),
                    crate::audit::Actor::current(),
                    crate::audit::Outcome::Refused(SUPERVISOR_ONLY_YOU.to_string()),
                    None,
                );
                return Err(DomainStateError {
                    code: "forbidden",
                    message: SUPERVISOR_ONLY_YOU.to_string(),
                });
            }
            let (slot, pick) =
                super::autopilot_supervisor_menu::pick(&repository, &before, params)?;
            Some(saved(autopilot::update_config(&state.paths, |config| {
                super::autopilot_supervisor_menu::apply(config, slot, pick)
            }))?)
        }
        "set" => {
            let patch = params
                .get("patch")
                .cloned()
                .ok_or_else(|| DomainStateError::bad_request("patch is required."))?;
            if let Some((summary, message)) = refuses_limit_resets(
                &before,
                &patch,
                caller,
                crate::audit::token_file_names_session(),
            ) {
                crate::audit::record(
                    crate::audit::Action::new("autopilot.widen", summary),
                    crate::audit::Actor::current(),
                    crate::audit::Outcome::Refused(message.to_string()),
                    None,
                );
                return Err(DomainStateError {
                    code: "forbidden",
                    message: message.to_string(),
                });
            }
            Some(saved(autopilot::update_config(&state.paths, |config| {
                autopilot::apply_patch(config, &patch)
            }))?)
        }
        "setProject" => {
            let project_id = text(params, "projectId")
                .ok_or_else(|| DomainStateError::bad_request("projectId is required."))?;
            let on = params
                .get("on")
                .and_then(Value::as_bool)
                .ok_or_else(|| DomainStateError::bad_request("on is required (true or false)."))?;
            let project = repository
                .get_project(&project_id)?
                .ok_or_else(|| DomainStateError::not_found("Project not found."))?;
            let main = autopilot::main_project_of(&project).unwrap_or(project_id);
            Some(saved(autopilot::update_config(&state.paths, |config| {
                config.set_project(&main, on)
            }))?)
        }
        "setSession" => {
            let project_id = text(params, "projectId")
                .ok_or_else(|| DomainStateError::bad_request("projectId is required."))?;
            let session_id = text(params, "sessionId")
                .ok_or_else(|| DomainStateError::bad_request("sessionId is required."))?;
            let main = repository
                .get_project(&project_id)?
                .as_ref()
                .and_then(autopilot::main_project_of);
            let toggle = params.get("toggle").and_then(Value::as_bool) == Some(true);
            let on = match params.get("on").and_then(Value::as_bool) {
                Some(on) => on,
                None if toggle => !before.covers(&project_id, main.as_deref(), &session_id),
                None => {
                    return Err(DomainStateError::bad_request(
                        "on is required (true or false), or toggle: true.",
                    ))
                }
            };
            Some(saved(autopilot::update_config(&state.paths, |config| {
                config.set_session(&project_id, main.as_deref(), &session_id, on)
            }))?)
        }
        other => {
            return Err(DomainStateError::bad_request(format!(
                "Unknown op \"{other}\": use read, set, setProject, setSession, supervisorMenu or supervisorPick."
            )))
        }
    };
    if let Some(after) = &after {
        if *after != before {
            if let Some(summary) = autopilot_widening(&before, after) {
                crate::audit::record(
                    crate::audit::Action::new("autopilot.widen", summary),
                    crate::audit::Actor::current(),
                    crate::audit::Outcome::Done,
                    None,
                );
            }
            publish_change(state, &db, &repository, &before, after);
        }
    }
    let config = after.unwrap_or(before);
    let limit = params
        .get("limit")
        .and_then(Value::as_u64)
        .unwrap_or(200)
        .min(2_000) as usize;
    Ok(read_view(state, &repository, &config, limit))
}

/// The security log's sentence for a change that lets Autopilot do more (turned on, answers
/// approvals, a wider tier, a hard limit it may now cross, more projects or sessions); `None` when
/// the change only narrows it.
fn autopilot_widening(before: &AutopilotConfig, after: &AutopilotConfig) -> Option<String> {
    let mut changes = Vec::new();
    if after.enabled && !before.enabled {
        changes.push("turned on Autopilot".to_string());
    }
    // Turning Autopilot on with approvals already on (their default) widens it to them too.
    if after.answer_approvals && (!before.answer_approvals || (after.enabled && !before.enabled)) {
        changes.push("Autopilot may answer approvals".to_string());
    }
    if after.tier != before.tier {
        changes.push(format!("Autopilot's level set to {:?}", after.tier));
    }
    if after.use_limit_resets && !before.use_limit_resets {
        changes.push(
            "Autopilot may use a Claude limit reset when an account hits its limit".to_string(),
        );
    }
    if after.read_only_leave_plan && !before.read_only_leave_plan {
        changes.push("Autopilot may take read-only threads out of plan mode".to_string());
    }
    let limits = |config: &AutopilotConfig| {
        serde_json::to_value(&config.limits)
            .ok()
            .and_then(|value| value.as_object().cloned())
            .unwrap_or_default()
    };
    let old_limits = limits(before);
    for (limit, allowed) in limits(after) {
        if allowed == Value::Bool(true) && old_limits.get(&limit) != Some(&Value::Bool(true)) {
            changes.push(format!("Autopilot may now {limit}"));
        }
    }
    let added = after
        .projects
        .keys()
        .chain(after.sessions.keys())
        .filter(|key| !before.projects.contains_key(*key) && !before.sessions.contains_key(*key))
        .count();
    if added > 0 {
        changes.push(format!(
            "Autopilot covers {}",
            ghostex_gx_protocol::plural(
                added,
                "more project or session",
                "more projects or sessions"
            )
        ));
    }
    let mut summary = changes.join(", ");
    let first = summary.chars().next()?.to_uppercase().to_string();
    summary.replace_range(..1, &first);
    Some(summary)
}

/// What Settings > Autopilot and `ghostex autopilot status` show.
fn read_view(
    state: &AppState,
    repository: &DomainRepository<'_>,
    config: &AutopilotConfig,
    limit: usize,
) -> Value {
    let projects = repository.list_projects().unwrap_or_default();
    let name_of = |project_id: &str| {
        projects
            .iter()
            .find(|project| project["projectId"].as_str() == Some(project_id))
            .and_then(|project| project["name"].as_str())
            .unwrap_or_default()
            .to_string()
    };
    let project_rows: Vec<Value> = projects
        .iter()
        .filter(|project| autopilot::main_project_of(project).is_none())
        .filter(|project| !autopilot::is_supervisor_project(project))
        .filter(|project| project["isRecentProject"].as_bool() != Some(true))
        .filter_map(|project| {
            let project_id = project["projectId"].as_str()?;
            Some(json!({
                "projectId": project_id,
                "name": project["name"],
                "path": project["path"],
                "in": config.project_in(project_id, None),
            }))
        })
        .collect();
    let session_rows: Vec<Value> = config
        .sessions
        .iter()
        .filter_map(|(key, membership)| {
            let (project_id, session_id) = key.split_once(':')?;
            let session = repository.get_session(project_id, session_id).ok()??;
            Some(json!({
                "projectId": project_id,
                "sessionId": session_id,
                "title": session["title"],
                "project": name_of(project_id),
                "on": *membership == autopilot::Membership::On,
            }))
        })
        .collect();
    let since_ms = config
        .enabled_at
        .as_deref()
        .and_then(|at| chrono::DateTime::parse_from_rfc3339(at).ok())
        .map(|at| at.timestamp_millis())
        .unwrap_or_else(|| chrono::Utc::now().timestamp_millis() - 24 * 60 * 60 * 1000);
    let mut config_value = serde_json::to_value(config).unwrap_or_default();
    if let Some(object) = config_value.as_object_mut() {
        object.remove("projects");
        object.remove("sessions");
    }
    json!({
        "config": config_value,
        "projects": project_rows,
        "sessions": session_rows,
        "summary": autopilot::summary(&state.paths, since_ms),
        "log": autopilot::read_log(&state.paths, limit),
        "watching": super::autopilot_runtime::autopilot_runtime_watching(),
        "supervisor": supervisor_view(state),
        // The Supervisor model and When it's at its limit rows' buttons (logo, model, effort).
        "supervisorLabels": super::autopilot_supervisor_menu::labels(config),
    })
}

/// The supervisor's session and wakes, with `label` naming the agent and model it runs on for the
/// status line ("Antigravity, Orchestrator (Neon)").
fn supervisor_view(state: &AppState) -> Value {
    let mut view = super::autopilot_judge::supervisor_view(&state.paths);
    if let Some(record) = autopilot::read_supervisor_record(&state.paths) {
        let label = super::autopilot_supervisor_menu::pick_label(&autopilot::SupervisorPick::new(
            &record.agent,
            &record.model,
            "",
        ));
        let agent = label["agentName"].as_str().unwrap_or(&record.agent);
        view["label"] = json!(
            match label["model"].as_str().filter(|model| !model.is_empty()) {
                Some(model) if !record.model.is_empty() => format!("{agent}, {model}"),
                _ => agent.to_string(),
            }
        );
    }
    view
}

/// Republishes what shows Autopilot: every project row (the sidebar menu's row comes and goes
/// with the switch and the scope) and every running session's open chat (its pill).
fn publish_change(
    state: &AppState,
    db: &rusqlite::Connection,
    repository: &DomainRepository<'_>,
    before: &AutopilotConfig,
    after: &AutopilotConfig,
) {
    let projects_changed = before.enabled != after.enabled
        || before.scope != after.scope
        || before.projects != after.projects;
    if projects_changed {
        for project in repository.list_projects().unwrap_or_default() {
            if let Some(project_id) = project["projectId"].as_str() {
                let _ = schedule_presentation_project_delta(
                    state,
                    db,
                    repository,
                    project_id,
                    "projectUpdated",
                );
            }
        }
    }
    republish_open_chats(state, repository);
}

/// Re-sends every running session's open chat, so its Autopilot pill follows.
pub(super) fn republish_open_chats(state: &AppState, repository: &DomainRepository<'_>) {
    if let Ok(sessions) = repository.list_sessions_with_lifecycle_state("running") {
        for session in sessions {
            if let (Some(project_id), Some(session_id)) =
                (session["projectId"].as_str(), session["sessionId"].as_str())
            {
                super::coordinator_runtime::republish_session_chat(state, project_id, session_id);
            }
        }
    }
}

/// Applies the supervisor's decision for one session and logs it.
async fn decide(
    state: &Arc<AppState>,
    params: &Map<String, Value>,
) -> Result<Value, DomainStateError> {
    let project_id = text(params, "projectId")
        .ok_or_else(|| DomainStateError::bad_request("projectId is required."))?;
    let session_id = text(params, "sessionId")
        .ok_or_else(|| DomainStateError::bad_request("sessionId is required."))?;
    let key = (project_id.clone(), session_id.clone());
    let answers: Vec<String> = params
        .get("answers")
        .and_then(Value::as_array)
        .map(|answers| {
            answers
                .iter()
                .filter_map(Value::as_str)
                .map(str::trim)
                .filter(|answer| !answer.is_empty())
                .map(str::to_string)
                .collect()
        })
        .unwrap_or_default();
    let message = text(params, "message");
    let leave = text(params, "leave");
    let reload = params.get("reload").and_then(Value::as_bool) == Some(true);
    let (title, project, bot) = {
        let db = open_gxserver_database(&state.paths).map_err(|error| DomainStateError {
            code: "internalError",
            message: error.to_string(),
        })?;
        let repository = DomainRepository::new(&db, state.metadata.server_id.as_str());
        let session = repository
            .get_session(&project_id, &session_id)?
            .ok_or_else(|| DomainStateError::not_found("Session not found."))?;
        let project = repository.get_project(&project_id)?;
        let bot = project
            .as_ref()
            .is_some_and(crate::bot_projects::is_bot_project);
        let project = project
            .and_then(|project| project["name"].as_str().map(str::to_string))
            .unwrap_or_default();
        (
            session["title"].as_str().unwrap_or_default().to_string(),
            project,
            bot,
        )
    };
    // A bot's conversation is a chat, not a task: Autopilot types nothing into it (CDXC:Bots
    // 2026-11-08, bot_projects.rs).
    if bot && (message.is_some() || !answers.is_empty() || reload) {
        return Err(DomainStateError {
            code: "invalidState",
            message: "That is a bot's conversation: Autopilot never sends it anything.".to_string(),
        });
    }
    let failed = |message: String| DomainStateError {
        code: "invalidState",
        message,
    };
    let did = if !answers.is_empty() {
        super::autopilot_runtime::answer_with_picks(state, &key, &answers)
            .await
            .map_err(failed)?
    } else if let Some(message) = message {
        super::autopilot_runtime::send_text(
            state,
            &key,
            &format!("Ghostex: [Autopilot] {message}"),
        )
        .await
        .map_err(failed)?;
        format!("Told it: \"{message}\"")
    } else if reload {
        super::autopilot_runtime::reload(state, &key)
            .await
            .map_err(failed)?
    } else if let Some(note) = leave.clone() {
        format!("Left it for you: {note}")
    } else {
        return Err(DomainStateError::bad_request(
            "Give --answer (once per question), --message, --reload or --leave.",
        ));
    };
    autopilot::append_log(
        &state.paths,
        &autopilot::LogEntry {
            at: chrono::Utc::now().to_rfc3339_opts(chrono::SecondsFormat::Millis, true),
            project_id,
            session_id,
            title,
            project,
            saw: "The supervisor looked at it.".to_string(),
            did: did.clone(),
            rule: "supervisor".to_string(),
            outcome: if leave.is_some() && answers.is_empty() {
                autopilot::Outcome::Parked
            } else {
                autopilot::Outcome::Done
            },
        },
    );
    super::autopilot_judge::forget_session(&key);
    Ok(json!({ "did": did }))
}
