//! Other projects' sessions (GitHub #495 follow-up, Q1863 "ask once per project"): an agent's
//! first message to, or new session in, a project other than its own asks the owner, and the
//! approval lets every agent of the caller's project reach that project from then on (ask once
//! per project pair, `SecurityState::project_reach_approved`), until either project is removed. A create call that starts an agent with approvals
//! off when the owner's own setting doesn't asks every time, in any project.
//!
//! | Route | Asks when |
//! | --- | --- |
//! | `/api/sendSessionChatMessage`, `/api/queueSessionChatPrompt`, `/api/sendSessionChatQueuedPrompt`, `/api/updateSessionChatQueuedPrompt`, `/api/scheduleDelayedSend` | the session is in another project and isn't one of the caller's threads |
//! | `/api/createSession`, `/api/createAgentSession`, `/api/createWorktreeSession`, `/api/createWorkspaceTerminal`, `/api/forkSession` | the project (or a folder that is no project yet) is another one, and the caller isn't a coordinator (which starts its threads this way); `launchSettings.acceptAllMode: "enabled"` against the owner's setting asks always |
//!
//! A project's worktrees count as the project (a worktree registered as its own project names
//! its parent in `worktree.parentProjectId`). The owner's devices never ask; Chief skips the
//! reach question (`Principal::skips_approval`), not the full-access one. The token file (the web
//! build, the CLI in the user's own terminal) has no project of its own and only gets the
//! full-access question.

use serde_json::{json, Map, Value};

use crate::domain::DomainRepository;
use crate::security::{ActionRequest, Principal, SensitiveAction, SessionRole};

use super::AppState;

const MESSAGE_ROUTES: &[&str] = &[
    "/api/sendSessionChatMessage",
    "/api/queueSessionChatPrompt",
    "/api/sendSessionChatQueuedPrompt",
    "/api/updateSessionChatQueuedPrompt",
    "/api/scheduleDelayedSend",
];

const CREATE_ROUTES: &[&str] = &[
    "/api/createSession",
    "/api/createAgentSession",
    "/api/createWorktreeSession",
    "/api/createWorkspaceTerminal",
    "/api/forkSession",
];

/// Whether `endpoint_path` is one this file may ask for.
pub(crate) fn is_cross_project_route(endpoint_path: &str) -> bool {
    MESSAGE_ROUTES.contains(&endpoint_path) || CREATE_ROUTES.contains(&endpoint_path)
}

/// The key a caller's remembered approvals are kept under: its project's root (so an agent in a
/// worktree registered as its own project shares its project's approvals); `None` for callers
/// this never asks (the owner's devices, and callers with no project of their own to compare
/// against that keep their own allow-lists: connected computers, guests, extensions).
fn caller_key(repository: &DomainRepository, caller: &Principal) -> Option<String> {
    match caller {
        Principal::Session { project_id, .. } => Some(project_root(repository, project_id)),
        Principal::LocalAgent => Some("localAgent".to_string()),
        _ => None,
    }
}

/// Where a call reaches: a known project, or a folder that would become a new one.
enum Reach {
    Project { id: String, name: String },
    Folder(String),
}

/// The approval this call needs, if any.
pub(crate) fn cross_project_action(
    state: &AppState,
    caller: &Principal,
    endpoint_path: &str,
    params: &Map<String, Value>,
) -> Option<ActionRequest> {
    let db = crate::storage::open_gxserver_database(&state.paths).ok()?;
    let repository = DomainRepository::new(&db, state.metadata.server_id.as_str());
    let key = caller_key(&repository, caller)?;
    let create = CREATE_ROUTES.contains(&endpoint_path);
    let reach = if create {
        create_reach(&repository, params)?
    } else {
        let project_id = text(params, "projectId")?;
        let project = repository.get_project(project_id).ok().flatten()?;
        Reach::Project {
            id: project_id.to_string(),
            name: project_name(&project, project_id),
        }
    };
    if create && forces_full_access(&db, &repository, &reach, params) {
        let (target, place) = describe(&reach);
        return Some(
            ActionRequest::new(
                SensitiveAction::ForceFullAccess,
                format!("Start an agent with approvals off in {place}"),
            )
            .target(target)
            .params(Value::Object(params.clone()))
            .detail("It runs every command without asking you first."),
        );
    }
    // CDXC:Security 2026-11-08 WHY: the token file has no project of its own, and its callers are
    // the GPUI web build and the ghostex CLI in the user's own terminal, which would otherwise ask
    // once for every project the user touches. Agents carry their session's own token, which is
    // what this question is about; the token file still asks before forcing full access.
    if matches!(caller, Principal::LocalAgent) || is_own_reach(&repository, caller, &reach) {
        return None;
    }
    if !create && is_callers_thread(state, caller, params) {
        return None;
    }
    // A coordinator starts its threads in other projects with a create, then links them; its
    // threads never ask (user decision).
    if create
        && matches!(
            caller,
            Principal::Session {
                role: SessionRole::Coordinator | SessionRole::Head,
                ..
            }
        )
    {
        return None;
    }
    let (target, place) = describe(&reach);
    if state.security.project_reach_approved(&key, &target) {
        return None;
    }
    let from = project_label(&repository, &key);
    Some(
        ActionRequest::new(
            SensitiveAction::ReachOtherProject,
            format!("Let agents in {from} message and start sessions in {place}"),
        )
        .params(reach_params(&key, &target))
        .target(target)
        .detail(format!(
            "Approved once, every agent in {from} can do this from now on, until either project is removed."
        ))
        .shared(),
    )
}

/*
CDXC:Security 2026-11-08 WHY:
A user on 11.1.1 (one card per agent and project since 11.1.2): "I came back to 3 popups for approval of agents talking to each other. I approved the first, and the rest did not disappear." A reach request carried the whole call, message text included, so one agent sending three messages to another project made three cards, and an approval was only remembered when the agent came back with it, so the other cards stayed until they lapsed. The request now names only the project reached (`reach_params`): one card per agent and project, which a retry with any message can spend. Approving or denying it answers the agent's other waiting requests for that project at once (`settle_reach_decision`), and an approval is remembered the moment it is given.
CDXC:Security 2026-11-08 DECISION:
User, after 11.1.2: one approval lets every agent in a project reach the other project (ask once per project pair). The memory is keyed on the caller's root project (`caller_key`), the request names both projects (`reach_params`) and is `.shared()`: one waiting card per pair, whichever of A's agents asks, spendable by any of them, and never by an agent of another project (the digest binds it to A).
*/
/// What a reach request is about: the caller's project and the project reached, never the
/// message or the agent, so every call from any agent of one project to another is the same
/// request.
fn reach_params(from: &str, target: &str) -> Value {
    json!({ "from": from, "reach": target })
}

/// How the card names the caller's project.
fn project_label(repository: &DomainRepository, project_id: &str) -> String {
    match repository.get_project(project_id).ok().flatten() {
        Some(project) => project_name(&project, project_id),
        None => project_id.to_string(),
    }
}

/// The owner just approved or denied `request` (`/api/approveApprovalRequest`,
/// `/api/denyApprovalRequest`): for a reach, other waiting requests for that project pair get the
/// same answer (returned, so their cards close), and an approval is remembered now, so every
/// agent of the caller's project passes whatever it sends.
pub(crate) fn settle_reach_decision(
    state: &AppState,
    request: &ghostex_gx_protocol::approval::ApprovalRequest,
    now_ms: u64,
) -> Vec<ghostex_gx_protocol::approval::ApprovalRequest> {
    if request.action != SensitiveAction::ReachOtherProject {
        return Vec::new();
    }
    let settled = state
        .security
        .approvals
        .settle_same_target(&request.id, now_ms);
    if request.status == ghostex_gx_protocol::approval::ApprovalStatus::Approved {
        if let Some(requester) = state.security.approvals.requester_of(&request.id) {
            remember_approved(state, &requester, request);
            // The agent's next call no longer spends the approval (it is remembered), so the
            // approval is logged here rather than in the gate.
            crate::audit::record(
                crate::audit::Action::from_approval(request),
                crate::audit::Actor::from(&requester),
                crate::audit::Outcome::Approved,
                Some(crate::audit::ApprovalRef::from(request)),
            );
        }
    }
    settled
}

/// After the owner approved `request` for `caller`, remember that its project may reach the
/// project it reached.
pub(crate) fn remember_approved(
    state: &AppState,
    caller: &Principal,
    approval: &ghostex_gx_protocol::approval::ApprovalRequest,
) {
    if !matches!(
        approval.action,
        SensitiveAction::ReachOtherProject | SensitiveAction::ForceFullAccess
    ) {
        return;
    }
    // Only an agent session is ever asked about reach; the token file's full-access approvals
    // aren't kept.
    if !matches!(caller, Principal::Session { .. }) {
        return;
    }
    let Some(target) = approval.target.as_deref() else {
        return;
    };
    let Ok(db) = crate::storage::open_gxserver_database(&state.paths) else {
        return;
    };
    let repository = DomainRepository::new(&db, state.metadata.server_id.as_str());
    let Some(key) = caller_key(&repository, caller) else {
        return;
    };
    state.security.approve_project_reach(&key, target);
}

/// The target a reach is remembered under and how the card names it.
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

/// Whether the reach is the caller's own project, or a worktree of it, or the project it is a
/// worktree of.
fn is_own_reach(repository: &DomainRepository, caller: &Principal, reach: &Reach) -> bool {
    let Principal::Session { project_id, .. } = caller else {
        return false;
    };
    let Reach::Project { id, .. } = reach else {
        return false;
    };
    id == project_id || project_root(repository, id) == project_root(repository, project_id)
}

/// A project's root: the project a worktree registered as its own project belongs to, else itself.
fn project_root(repository: &DomainRepository, project_id: &str) -> String {
    repository
        .get_project(project_id)
        .ok()
        .flatten()
        .and_then(|project| {
            project
                .pointer("/worktree/parentProjectId")
                .and_then(Value::as_str)
                .map(str::trim)
                .filter(|parent| !parent.is_empty())
                .map(str::to_string)
        })
        .unwrap_or_else(|| project_id.to_string())
}

/// Whether the message goes to one of the calling coordinator's threads.
fn is_callers_thread(state: &AppState, caller: &Principal, params: &Map<String, Value>) -> bool {
    let Principal::Session {
        project_id: own_project,
        session_id: own_session,
        role: SessionRole::Coordinator | SessionRole::Head,
    } = caller
    else {
        return false;
    };
    let (Some(project_id), Some(session_id)) =
        (text(params, "projectId"), text(params, "sessionId"))
    else {
        return false;
    };
    super::security_gates::is_own_thread(state, own_project, own_session, project_id, session_id)
}

/*
CDXC:Security 2026-11-08 DECISION:
User (#495 follow-up, Q1863): "forcing full access always asks". A create call forces it when it sends `launchSettings.acceptAllMode: "enabled"` and the owner's own setting would not start that agent with approvals off: neither "Agent approvals: accept all" nor the agent's own launcher setting is on. A call that only repeats the owner's setting changes nothing, so it doesn't ask (the page's option A: "unless it matches your own setting").
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
