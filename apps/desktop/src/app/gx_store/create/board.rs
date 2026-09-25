//! The Project Board's conversation requests (native Kanban, native Automate and the CEF board
//! page): the board's state, a toast, associating the focused session with a card, Start work,
//! jumping to a linked conversation and unlinking it, performed in Rust
//! (`handleGpuiProjectBoardConversationRequest` in the old runtime).
//!
//! macOS `handleProjectBoardRequest` parity for the conversation half of the board bridge. Links
//! persist in the daemon's `projectBoardConfig.beadConversationLinks`, the same durable storage the
//! daemon's own `ghostex board start-work` and `associate` write. The calls are the runtime's
//! (`/api/listProjects` fresh on every request so link mutations from other clients are visible,
//! `/api/listPreviousSessions` and `/api/readAgentResumePlan` for a link with no live session,
//! `/api/updateProject` for a link write), not the daemon's board endpoints, whose Start work
//! builds its own prompt from `bd` and reuses a linked worker instead of starting the one the user
//! asked for.
//!
//! This file answers a request and builds the state; `board_links_host.rs` holds the actions that
//! write links (associate, Start work, jump, unlink).
//!
//! SEE-ALSO: packages/gx-core/src/session_create/board_links.rs and board_state.rs,
//! apps/desktop/src/app/status_pet.rs (`dispatch_gpui_project_board_conversation_request`).

use std::collections::HashMap;
use std::time::Duration;
use web_time::Instant;

use ghostex_gx_core::{
    BoardSession, BoardSessionFacts, LinkAvailability, MachineId, board_agent_options,
    board_reference, canonicalize_links_for_board, iso_string_from_ms, link_view,
    project_board_links, select_link_store_projects,
};
use serde_json::{Map, Value, json};

use super::super::gx_rpc;
use super::board_links_host as links;
use crate::GhostexGpuiApp;
use crate::shared_settings;

/// `GPUI_PROJECT_BOARD_RESTORABLE_LINK_CHECK_TTL_MS`: the board re-reads its state every 8 s.
const LINK_CHECK_TTL: Duration = Duration::from_secs(60);
/// `GPUI_PROJECT_BOARD_RESTORABLE_LINK_CHECK_CACHE_MAX`.
const LINK_CHECK_CACHE_MAX: usize = 512;
/// `GPUI_PROJECT_BOARD_LINK_AVAILABILITY_CONCURRENCY`.
const LINK_CHECK_CONCURRENCY: usize = 4;
/// `GPUI_PROJECT_BOARD_RESUMABLE_AGENT_IDS`.
///
/// CDXC:ProjectBoard 2026-08-07:
/// Resuming a bead's closed conversation runs through the daemon's fork plan, which only knows how
/// to continue Codex, Claude, and Pi conversations. gxserver stays the authority and rejects
/// anything else, so this set exists to keep the board from offering a Resume the daemon would
/// refuse.
const RESUMABLE_AGENT_IDS: [&str; 3] = ["claude", "codex", "pi"];
const DEFAULT_PROMPT_AGENT_ID: &str = "codex";
/// Link availability answers by `projectId:sessionId`.
pub(super) type LinkChecks = HashMap<String, (Instant, LinkAvailability)>;


/// One board request, bounded the way `normalizeGpuiProjectBoardConversationRequest` bounds it.
#[derive(Clone, Debug, Default)]
pub(super) struct BoardRequest {
    pub(super) action: String,
    pub(super) agent_id: Option<String>,
    pub(super) bead_display_id: Option<String>,
    pub(super) bead_id: Option<String>,
    pub(super) project_id: Option<String>,
    pub(super) project_path: Option<String>,
    pub(super) prompt: Option<String>,
    pub(super) request_id: String,
    pub(super) session_id: Option<String>,
    pub(super) start_location: Option<String>,
    pub(super) toast_description: Option<String>,
    pub(super) toast_level: Option<String>,
    pub(super) toast_title: Option<String>,
}

fn bounded(request: &Value, key: &str, max_chars: usize) -> Option<String> {
    let value = request.get(key)?.as_str()?.trim();
    (!value.is_empty() && value.chars().count() <= max_chars).then(|| value.to_string())
}

impl BoardRequest {
    fn read(request: &Value) -> Option<Self> {
        let action = request.get("action")?.as_str()?;
        if !matches!(
            action,
            "appendDebugLog"
                | "associateFocusedSession"
                | "getState"
                | "jumpToConversation"
                | "showToast"
                | "startWork"
                | "unlinkConversation"
        ) {
            return None;
        }
        Some(Self {
            action: action.to_string(),
            agent_id: bounded(request, "agentId", 256),
            bead_display_id: bounded(request, "beadDisplayId", 256),
            bead_id: bounded(request, "beadId", 512),
            project_id: bounded(request, "projectId", 512),
            project_path: bounded(request, "projectPath", 4096),
            prompt: bounded(request, "prompt", 60_000),
            request_id: bounded(request, "requestId", 256)?,
            session_id: bounded(request, "sessionId", 512),
            start_location: bounded(request, "startLocation", 32),
            toast_description: bounded(request, "toastDescription", 2_000),
            toast_level: bounded(request, "toastLevel", 16),
            toast_title: bounded(request, "toastTitle", 300),
        })
    }
}

/// What the board reads from this app at the moment of a request.
#[derive(Clone)]
pub(super) struct BoardContext {
    pub(super) facts: BoardSessionFacts,
    pub(super) agents: Vec<Value>,
    pub(super) hud: Option<Value>,
    pub(super) default_agent_id: String,
    pub(super) debugging_mode: bool,
    pub(super) diagnostic_logging: Value,
    pub(super) global_beads_directory: String,
    pub(super) link_checks: LinkChecks,
    /// Every session row, raw, for a link's `agentSessionId`, `agentSessionPath` and `zmxName`.
    pub(super) session_rows: HashMap<(String, String), Value>,
}

impl BoardContext {
    pub(super) fn now_iso() -> String {
        iso_string_from_ms(super::super::host::now_ms() as i64)
    }
}

/// A board failure: the sentence the board shows.
pub(super) type BoardResult<T> = Result<T, String>;

impl GhostexGpuiApp {
    /// A board conversation request from any board surface. Returns whether it was a request.
    pub(crate) fn gx_store_run_board_conversation_request(
        &mut self,
        request: &Value,
        cx: &mut gpui::Context<Self>,
    ) -> bool {
        let Some(request) = BoardRequest::read(request) else {
            return false;
        };
        self.gx_store.create.counters.board_requests += 1;
        if request.action == "showToast" {
            let level = match request.toast_level.as_deref() {
                Some(level @ ("error" | "info" | "success" | "warning")) => level,
                _ => "error",
            };
            let title = request
                .toast_title
                .clone()
                .unwrap_or_else(|| "Project Board update failed".to_string());
            self.gx_store_create_toast(level, &title, request.toast_description.as_deref(), cx);
            self.gx_store_board_respond(&request.request_id, Ok(None), cx);
            return true;
        }
        let context = self.gx_store_board_context();
        cx.spawn(async move |this, cx| {
            let result = match request.action.as_str() {
                "associateFocusedSession" => {
                    links::associate_focused(&this, cx, &request, &context).await
                }
                "startWork" => links::start_work(&this, cx, &request, &context).await,
                "jumpToConversation" => links::jump(&this, cx, &request, &context).await,
                "unlinkConversation" => links::unlink(&request, &context).await,
                // `getState`, and `appendDebugLog`, whose log line Rust wrote before the request
                // arrived (status_pet.rs), answer with the state.
                _ => Ok(()),
            };
            let answer = match result {
                Ok(()) => {
                    // The state is built from this app as it is NOW, after the action.
                    let context = match this.update(cx, |this, _| this.gx_store_board_context()) {
                        Ok(context) => context,
                        Err(_) => return,
                    };
                    let (state, checks) = board_state(&request, &context).await;
                    let _ = this.update(cx, |this, _| this.gx_store_note_board_link_checks(checks));
                    state.map(Some)
                }
                Err(error) => Err(error),
            };
            let _ = this.update(cx, |this, cx| {
                this.gx_store_board_respond(&request.request_id, answer, cx);
            });
        })
        .detach();
        true
    }

    /// `postGpuiProjectBoardConversationResponse`, through the same receiver the runtime's post
    /// reached.
    fn gx_store_board_respond(
        &mut self,
        request_id: &str,
        answer: BoardResult<Option<Value>>,
        cx: &mut gpui::Context<Self>,
    ) {
        let mut response = Map::new();
        match answer {
            Ok(payload) => {
                response.insert("ok".into(), json!(true));
                if let Some(payload) = payload {
                    response.insert("payload".into(), payload);
                }
            }
            Err(error) => {
                let error = match error.trim().is_empty() {
                    true => "Project board conversation action failed.".to_string(),
                    false => error,
                };
                response.insert("error".into(), json!(error));
                response.insert("ok".into(), json!(false));
            }
        }
        response.insert("requestId".into(), json!(request_id));
        self.receive_sidebar_project_board_conversation_response_payload(
            &json!({
                "response": response,
                "type": crate::GPUI_SIDEBAR_PROJECT_BOARD_CONVERSATION_RESPONSE_MESSAGE_TYPE,
                "version": crate::GPUI_SIDEBAR_PROJECT_BOARD_CONVERSATION_RESPONSE_MESSAGE_VERSION,
            }).to_string(),
            cx,
        );
    }

    /// Everything a request reads from this app, taken at once so the async half needs nothing else.
    pub(super) fn gx_store_board_context(&self) -> BoardContext {
        let focus = self.gx_store.core.focus();
        let active_project_id = focus
            .active_project
            .as_ref()
            .filter(|project| project.machine.is_local())
            .map(|project| project.project_id.clone());
        let focused_session_id = focus
            .focused_session
            .as_ref()
            .filter(|session| session.machine.is_local())
            .map(|session| session.session_id.clone());
        let mut facts = BoardSessionFacts {
            active_project_id,
            focused_session_id,
            ..BoardSessionFacts::default()
        };
        let mut session_rows = HashMap::new();
        if let Some(loaded) = self.gx_store.core.presentation().loaded(&MachineId::Local) {
            for project in loaded.projects() {
                facts
                    .project_titles
                    .insert(project.project_id.clone(), project.title.clone());
            }
            let mut sessions: Vec<_> = loaded
                .server_sessions()
                .filter(|session| {
                    matches!(
                        session.kind,
                        ghostex_gx_core::protocol::SessionKind::Agent
                            | ghostex_gx_core::protocol::SessionKind::Terminal
                    )
                })
                .collect();
            sessions.sort_by(|a, b| a.sort_key.cmp(&b.sort_key));
            for session in sessions {
                facts.sessions.push(BoardSession {
                    project_id: session.project_id.clone(),
                    session_id: session.session_id.clone(),
                    title: session.title.clone(),
                    agent_id: session.agent_name.clone().or_else(|| session.agent_id.clone()),
                    agent_session_id: session.agent_session_id.clone(),
                    sleeping: session.lifecycle_state
                        == ghostex_gx_core::protocol::LifecycleState::Sleeping,
                });
                session_rows.insert(
                    (session.project_id.clone(), session.session_id.clone()),
                    json!({
                        "agentId": session.agent_id,
                        "agentName": session.agent_name,
                        "agentSessionId": session.agent_session_id,
                        "agentSessionPath": session.agent_session_path,
                        "zmxName": session.zmx_name,
                    }),
                );
            }
        }
        let hud = self.gx_store.runtime_facts.hud.as_deref().cloned();
        let snapshot = shared_settings::shared_sidebar_settings_snapshot();
        let settings = snapshot.object();
        BoardContext {
            facts,
            agents: board_agent_options(hud.as_ref()),
            default_agent_id: hud
                .as_ref()
                .and_then(|hud| hud.get("settings"))
                .and_then(|settings| settings.get("defaultPromptAgentId"))
                .and_then(Value::as_str)
                .map(str::trim)
                .filter(|id| !id.is_empty())
                .unwrap_or(DEFAULT_PROMPT_AGENT_ID)
                .to_string(),
            hud,
            debugging_mode: snapshot.debugging_mode(),
            diagnostic_logging: settings
                .get("diagnosticLogging")
                .cloned()
                .unwrap_or_else(|| json!({})),
            global_beads_directory: settings
                .get("globalBeadsDirectory")
                .and_then(Value::as_str)
                .map(str::trim)
                .unwrap_or_default()
                .to_string(),
            link_checks: self.gx_store.create.board_link_checks.clone(),
            session_rows,
        }
    }

    fn gx_store_note_board_link_checks(&mut self, checks: Vec<(String, LinkAvailability)>) {
        let held = &mut self.gx_store.create.board_link_checks;
        for (key, availability) in checks {
            if held.len() >= LINK_CHECK_CACHE_MAX {
                held.clear();
            }
            held.insert(key, (Instant::now(), availability));
        }
    }
}

/// `listGpuiProjectBoardDomainProjects`: fresh, so other clients' link writes are visible.
pub(super) async fn list_projects() -> BoardResult<Vec<Value>> {
    let response = gx_rpc(None, "/api/listProjects", json!({}))
        .await
        .map_err(|error| error.message)?;
    Ok(response
        .get("projects")
        .and_then(Value::as_array)
        .cloned()
        .unwrap_or_default())
}

/// `selectGpuiProjectBoardDomainProject`: the project id, then the path, then the active project.
pub(super) fn select_board_project(
    request: &BoardRequest,
    projects: &[Value],
    context: &BoardContext,
) -> BoardResult<Value> {
    let id_of = |project: &Value| project.get("projectId").and_then(Value::as_str).map(str::to_string);
    if let Some(project_id) = &request.project_id
        && let Some(project) = projects.iter().find(|project| id_of(project).as_ref() == Some(project_id))
    {
        return Ok(project.clone());
    }
    let wanted = request
        .project_path
        .as_deref()
        .and_then(ghostex_gx_core::normalize_project_path);
    if let Some(wanted) = wanted
        && let Some(project) = projects.iter().find(|project| {
            project
                .get("path")
                .and_then(Value::as_str)
                .and_then(ghostex_gx_core::normalize_project_path)
                .is_some_and(|path| path == wanted)
        })
    {
        return Ok(project.clone());
    }
    if let Some(active) = &context.facts.active_project_id
        && let Some(project) = projects.iter().find(|project| id_of(project).as_ref() == Some(active))
    {
        return Ok(project.clone());
    }
    Err("Project not found.".to_string())
}

/// `resolveGpuiProjectBoardDomainScope`: the board project and the rows that mount its board.
pub(super) async fn resolve_scope(
    request: &BoardRequest,
    context: &BoardContext,
) -> BoardResult<(Value, Vec<Value>, Vec<Value>)> {
    let projects = list_projects().await?;
    let board = select_board_project(request, &projects, context)?;
    let store = select_link_store_projects(&board, &projects, &context.global_beads_directory);
    Ok((board, store, projects))
}

/// `createGpuiProjectBoardConversationState`, and the availability checks it made.
async fn board_state(
    request: &BoardRequest,
    context: &BoardContext,
) -> (BoardResult<Value>, Vec<(String, LinkAvailability)>) {
    let (board, store, projects) = match resolve_scope(request, context).await {
        Ok(scope) => scope,
        Err(error) => return (Err(error), Vec::new()),
    };
    let board_id = board
        .get("projectId")
        .and_then(Value::as_str)
        .unwrap_or_default()
        .to_string();
    let options = context.facts.session_options(&board, &store, &projects);
    let now = BoardContext::now_iso();
    let links: Vec<_> = store
        .iter()
        .flat_map(|project| project_board_links(project, &now))
        .filter(|link| link.status != "archived")
        .collect();
    let links = canonicalize_links_for_board(&links, &board_id);
    let mut checks = Vec::new();
    let mut views = Vec::with_capacity(links.len());
    for chunk in links.chunks(LINK_CHECK_CONCURRENCY) {
        let mut pending = Vec::new();
        for link in chunk {
            let session = options
                .iter()
                .find(|option| option.session_id == link.ghostex_session_id)
                .cloned()
                .or_else(|| {
                    context
                        .facts
                        .linked_session_option(&board_id, &link.ghostex_session_id)
                });
            let check = match session {
                Some(_) => None,
                None => Some(link_availability(&board_id, &link.ghostex_session_id, context)),
            };
            pending.push((link, session, check));
        }
        let answers = futures::future::join_all(pending.iter_mut().map(|(_, _, check)| async {
            match check.take() {
                Some(check) => Some(check.await),
                None => None,
            }
        }))
        .await;
        for ((link, session, _), answer) in pending.into_iter().zip(answers) {
            let availability = answer.map(|(key, availability, fresh)| {
                if fresh {
                    checks.push((key, availability.clone()));
                }
                availability
            });
            views.push(link_view(link, session.as_ref(), availability.as_ref()));
        }
    }
    let focused_terminal = options
        .iter()
        .find(|option| option.is_focused)
        .map(|option| option.session_id.clone());
    let mut state = Map::new();
    if context.facts.active_project_id.as_deref() == Some(board_id.as_str())
        && let Some(focused) = &context.facts.focused_session_id
    {
        state.insert("activeSessionId".into(), json!(focused));
    }
    state.insert("agents".into(), Value::Array(context.agents.clone()));
    state.insert("debuggingMode".into(), json!(context.debugging_mode));
    // The board page gates its debug breadcrumbs on the native.project.board scenario; Rust owns
    // the writer and also enforces the global Show debug UI controls gate.
    state.insert("diagnosticLogging".into(), context.diagnostic_logging.clone());
    state.insert("defaultAgentId".into(), json!(context.default_agent_id));
    if let Some(focused) = focused_terminal {
        state.insert("focusedTerminalSessionId".into(), json!(focused));
    }
    state.insert("links".into(), Value::Array(views));
    state.insert("projectId".into(), json!(board_id));
    state.insert(
        "sessions".into(),
        Value::Array(options.iter().map(|option| option.to_json()).collect()),
    );
    (Ok(Value::Object(state)), checks)
}

/// `checkGpuiProjectBoardLinkAvailability`: the cached answer inside its TTL, else the daemon's.
///
/// macOS resolves link restorability from its previous-sessions cache with a gxserver fallback;
/// GPUI keeps no such cache, so non-live links check the daemon directly behind a short TTL.
///
/// CDXC:ProjectBoard 2026-08-07:
/// Previous-session history only carries rows that closed with a trusted resume title, so a bead
/// worked by a since-closed agent session usually has no restorable row at all. The daemon can
/// still plan a resume from the session row's own agent identity, so ask it before calling the
/// link dead.
async fn link_availability(
    board_id: &str,
    ghostex_session_id: &str,
    context: &BoardContext,
) -> (String, LinkAvailability, bool) {
    let (project_id, session_id) = board_reference(ghostex_session_id, board_id);
    let key = format!("{project_id}:{session_id}");
    if let Some((checked_at, cached)) = context.link_checks.get(&key)
        && checked_at.elapsed() < LINK_CHECK_TTL
    {
        return (key, cached.clone(), false);
    }
    let previous = gx_rpc(
        None,
        "/api/listPreviousSessions",
        json!({
            "includeActive": false,
            "includePrevious": true,
            "limit": 20,
            "projectId": project_id,
            "query": session_id,
        }),
    )
    .await;
    let availability = match previous {
        // An unavailable history lookup renders the link as not restorable for this cycle; the
        // next TTL window re-checks.
        Err(_) => LinkAvailability::default(),
        Ok(response) => {
            let row = response
                .get("results")
                .and_then(Value::as_array)
                .and_then(|rows| {
                    rows.iter().find(|row| {
                        row.get("projectId").and_then(Value::as_str) == Some(project_id.as_str())
                            && row.get("sessionId").and_then(Value::as_str) == Some(session_id.as_str())
                            && row.get("lifecycleState").and_then(Value::as_str) != Some("running")
                    })
                })
                .cloned();
            match row {
                Some(row) => {
                    let title = ["displayTitle", "primaryTitle", "title"]
                        .iter()
                        .find_map(|key| row.get(*key).and_then(Value::as_str).map(str::to_string));
                    LinkAvailability {
                        restorable: true,
                        resumable: false,
                        title,
                    }
                }
                None => match link_resumable(&project_id, &session_id).await {
                    Some(resumable) => LinkAvailability {
                        restorable: false,
                        resumable,
                        title: None,
                    },
                    None => LinkAvailability::default(),
                },
            }
        }
    };
    (key, availability, true)
}

/// `checkGpuiProjectBoardLinkResumable`: `None` when the daemon's answer could not be read.
async fn link_resumable(project_id: &str, session_id: &str) -> Option<bool> {
    let response = gx_rpc(
        None,
        "/api/readAgentResumePlan",
        json!({ "projectId": project_id, "sessionId": session_id }),
    )
    .await
    .ok()?;
    let plan = response.get("plan");
    let command = plan
        .and_then(|plan| plan.get("primaryCommand"))
        .and_then(Value::as_str)
        .is_some_and(|command| !command.trim().is_empty());
    let agent = plan
        .and_then(|plan| plan.get("agentId"))
        .and_then(Value::as_str)
        .map(|agent| agent.trim().to_lowercase())
        .unwrap_or_default();
    Some(command && RESUMABLE_AGENT_IDS.contains(&agent.as_str()))
}
