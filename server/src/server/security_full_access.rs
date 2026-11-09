//! Starting an agent with approvals off (GitHub #495 follow-up): a create call that starts an
//! agent with approvals off when the owner's own setting doesn't asks the owner every time, in
//! any project.
//!
//! | Route | Asks when |
//! | --- | --- |
//! | `/api/createSession`, `/api/createAgentSession`, `/api/createWorktreeSession`, `/api/createWorkspaceTerminal`, `/api/forkSession` | `launchSettings.acceptAllMode: "enabled"` against the owner's setting |
//!
//! Agents (coordinators and Chief too) and the token file (the web build, the CLI in the user's
//! own terminal) ask; the owner's devices never do.
//!
//! CDXC:Security 2026-11-08 DECISION:
//! User, on 11.1.2's "ask once per project" question: "let's please stop limiting agents from talking to other projects this is dumb". Agents message, queue prompts to and start sessions in any project without asking, as before 11.1.1; the reach question, its remembered approvals (security.json's `approvedProjectReach` is read and ignored) and its sweep are gone. Only the full-access question stays.

use serde_json::{json, Map, Value};

use crate::domain::DomainRepository;
use crate::security::{ActionRequest, Principal, SensitiveAction};

use super::AppState;

const CREATE_ROUTES: &[&str] = &[
    "/api/createSession",
    "/api/createAgentSession",
    "/api/createWorktreeSession",
    "/api/createWorkspaceTerminal",
    "/api/forkSession",
];

/// Whether `endpoint_path` is one this file may ask for.
pub(crate) fn is_full_access_route(endpoint_path: &str) -> bool {
    CREATE_ROUTES.contains(&endpoint_path)
}

/// Where a create starts: a known project, or a folder that would become a new one.
enum Reach {
    Project { id: String, name: String },
    Folder(String),
}

/// The approval this create needs, if any: only when it forces full access against the owner's
/// setting, and only for agents and the token file.
pub(crate) fn full_access_action(
    state: &AppState,
    caller: &Principal,
    params: &Map<String, Value>,
) -> Option<ActionRequest> {
    if !matches!(caller, Principal::Session { .. } | Principal::LocalAgent) {
        return None;
    }
    let db = crate::storage::open_gxserver_database(&state.paths).ok()?;
    let repository = DomainRepository::new(&db, state.metadata.server_id.as_str());
    let reach = create_reach(&repository, params)?;
    if !forces_full_access(&db, &repository, &reach, params) {
        return None;
    }
    let (target, place) = describe(&reach);
    Some(
        ActionRequest::new(
            SensitiveAction::ForceFullAccess,
            format!("Start an agent with approvals off in {place}"),
        )
        .target(target)
        .params(Value::Object(params.clone()))
        .detail("It runs every command without asking you first."),
    )
}

/*
CDXC:Security 2026-11-09 WHY:
Security review of the GBot API (finding 9): `/api/updateSession` can set a session's `launchSettings.acceptAllMode: "enabled"`, and a resume reads it, so an agent could turn a session's approvals off without the question a create asks. An update that turns it on asks exactly when a create would (against the owner's own setting, for agents and the token file); turning it off or leaving it as it is never asks.
*/
/// The approval an `/api/updateSession` needs: when it turns a session's approvals off
/// (`launchSettings.acceptAllMode: "enabled"` where it isn't yet) against the owner's setting.
pub(crate) fn update_full_access_action(
    state: &AppState,
    caller: &Principal,
    params: &Map<String, Value>,
) -> Option<ActionRequest> {
    if !matches!(caller, Principal::Session { .. } | Principal::LocalAgent) {
        return None;
    }
    let requested = params
        .get("launchSettings")
        .and_then(|settings| settings.get("acceptAllMode"))
        .and_then(Value::as_str)
        .is_some_and(|mode| mode.trim().eq_ignore_ascii_case("enabled"));
    if !requested {
        return None;
    }
    let project_id = text(params, "projectId")?;
    let session_id = text(params, "sessionId")?;
    let db = crate::storage::open_gxserver_database(&state.paths).ok()?;
    let repository = DomainRepository::new(&db, state.metadata.server_id.as_str());
    let session = repository
        .get_session(project_id, session_id)
        .ok()
        .flatten()?;
    let already = session
        .pointer("/launchSettings/acceptAllMode")
        .and_then(Value::as_str)
        .is_some_and(|mode| mode.trim().eq_ignore_ascii_case("enabled"));
    if already {
        return None;
    }
    let project = repository.get_project(project_id).ok().flatten()?;
    let reach = Reach::Project {
        id: project_id.to_string(),
        name: project_name(&project, project_id),
    };
    let mut asked = params.clone();
    if let Some(agent_id) = text_value(&session, "agentId") {
        asked
            .entry("agentId".to_string())
            .or_insert_with(|| json!(agent_id));
    }
    if !forces_full_access(&db, &repository, &reach, &asked) {
        return None;
    }
    let (target, place) = describe(&reach);
    Some(
        ActionRequest::new(
            SensitiveAction::ForceFullAccess,
            format!("Let a session in {place} run with approvals off"),
        )
        .target(target)
        .params(Value::Object(params.clone()))
        .detail("Once it restarts, it runs every command without asking you first."),
    )
}

/// The request's target and how the card names the place.
fn describe(reach: &Reach) -> (String, String) {
    match reach {
        Reach::Project { id, name } => (id.clone(), name.clone()),
        Reach::Folder(path) => (format!("folder:{path}"), format!("the folder {path}")),
    }
}

/// The project a create names: `projectId`, else `projectPath` or `cwd` (an existing project's
/// folder, or a folder the create would add as a project).
fn create_reach(repository: &DomainRepository, params: &Map<String, Value>) -> Option<Reach> {
    if let Some(project_id) = text(params, "projectId") {
        if let Some(project) = repository.get_project(project_id).ok().flatten() {
            return Some(Reach::Project {
                id: project_id.to_string(),
                name: project_name(&project, project_id),
            });
        }
    }
    let (field, value) = ["projectPath", "cwd"].iter().find_map(|field| {
        params
            .get(*field)
            .filter(|value| !value.is_null())
            .map(|value| (*field, value))
    })?;
    let path = crate::domain::normalize_existing_directory_path(Some(value), field).ok()?;
    Some(
        match repository.find_project_by_path(&path).ok().flatten() {
            Some(project) => {
                let id = text_value(&project, "projectId")?.to_string();
                let name = project_name(&project, &id);
                Reach::Project { id, name }
            }
            None => Reach::Folder(path),
        },
    )
}

/*
CDXC:Security 2026-11-10 DECISION:
User (Q2104, "drop"): starting an agent with approvals off no longer asks; it is written to the security log as allowed (security/policy.rs). This supersedes Q1863's "forcing full access always asks". A create call forces it when it sends `launchSettings.acceptAllMode: "enabled"` and the owner's own setting would not start that agent with approvals off: neither "Agent approvals: accept all" nor the agent's own launcher setting is on. A call that only repeats the owner's setting changes nothing, so it doesn't ask (the page's option A: "unless it matches your own setting").
*/
fn forces_full_access(
    db: &rusqlite::Connection,
    repository: &DomainRepository,
    reach: &Reach,
    params: &Map<String, Value>,
) -> bool {
    let requested = params
        .get("launchSettings")
        .and_then(|settings| settings.get("acceptAllMode"))
        .and_then(Value::as_str)
        .map(|mode| mode.trim().eq_ignore_ascii_case("enabled"))
        .unwrap_or(false);
    if !requested {
        return false;
    }
    let global = crate::agents::settings::read_agent_settings(db)
        .ok()
        .and_then(|settings| {
            settings
                .get("agentAcceptAllEnabled")
                .and_then(Value::as_bool)
        })
        .unwrap_or(false);
    if global {
        return false;
    }
    let project = match reach {
        Reach::Project { id, .. } => repository.get_project(id).ok().flatten(),
        Reach::Folder(_) => None,
    }
    .unwrap_or_else(|| json!({}));
    let agent_id =
        text(params, "agentId").unwrap_or(crate::agents::settings::DEFAULT_PROMPT_AGENT_ID);
    let own = crate::agents::owner_agent_accept_all_mode(db, &project, agent_id);
    !own.is_some_and(|mode| mode.trim().eq_ignore_ascii_case("enabled"))
}

fn project_name(project: &Value, fallback: &str) -> String {
    text_value(project, "name")
        .map(str::to_string)
        .unwrap_or_else(|| fallback.to_string())
}

fn text_value<'a>(value: &'a Value, key: &str) -> Option<&'a str> {
    value
        .get(key)
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|text| !text.is_empty())
}

fn text<'a>(params: &'a Map<String, Value>, key: &str) -> Option<&'a str> {
    params
        .get(key)
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty())
}
