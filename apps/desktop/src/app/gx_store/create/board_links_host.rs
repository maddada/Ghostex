//! The Project Board actions that write a conversation link: associate the focused session with a
//! card, Start work (in the board project or in a new worktree), jump to a linked conversation and
//! unlink it. Ported from the old runtime's `project-board.ts`; the state they answer with is
//! `board.rs`.
//!
//! SEE-ALSO: packages/gx-core/src/session_create/board_links.rs.

use std::time::Duration;

use ghostex_gx_core::git_menu::{worktree_slug_from_prompt, worktree_user_visible_error};
use ghostex_gx_core::{
    AgentRecordOptions, BeadConversationLink, ProjectKey, SessionKey, SidebarAgent,
    agent_record_params, agent_session_default_title, bead_conversation_link_id,
    bead_conversation_link_match_key, board_session_id, check_startup_prompt_receipt,
    created_session, first_prompt_title_runtime_settings, project_board_links,
    queue_startup_prompt_params, resolve_board_session_id, resolve_sidebar_agent,
    select_link_store_projects, start_provider_params,
};
use gpui::{AsyncApp, WeakEntity};
use serde_json::{Value, json};

use super::super::{gx_rpc, gx_rpc_with_timeout};
use super::board::{BoardContext, BoardRequest, BoardResult, list_projects, resolve_scope};
use crate::GhostexGpuiApp;

/// `/api/createProjectWorktree` cuts the checkout, registers it and runs its setup command.
const WORKTREE_CREATE_TIMEOUT: Duration = Duration::from_secs(90);

type App = WeakEntity<GhostexGpuiApp>;

/// A session a link is written for (`GpuiCreatedProjectAgentSessionRecord`).
#[derive(Clone, Debug, Default)]
struct LinkedSession {
    project_id: String,
    session_id: String,
    agent_session_id: Option<String>,
    agent_session_path: Option<String>,
    zmx_name: Option<String>,
}

fn project_id(project: &Value) -> String {
    project
        .get("projectId")
        .and_then(Value::as_str)
        .unwrap_or_default()
        .to_string()
}

fn stored_links(store: &[Value], now: &str) -> Vec<BeadConversationLink> {
    store
        .iter()
        .flat_map(|project| project_board_links(project, now))
        .collect()
}

fn links_json(links: &[BeadConversationLink]) -> Value {
    Value::Array(links.iter().map(BeadConversationLink::to_json).collect())
}

/// `writeGpuiProjectBoardConversationLinks`: `/api/updateProject` replaces `projectBoardConfig`
/// wholesale, so the row's other board settings travel with the links.
async fn write_links(project: &Value, links: &[BeadConversationLink]) -> BoardResult<()> {
    let mut config = project
        .get("projectBoardConfig")
        .and_then(Value::as_object)
        .cloned()
        .unwrap_or_default();
    config.insert("beadConversationLinks".into(), links_json(links));
    gx_rpc(
        None,
        "/api/updateProject",
        json!({ "projectBoardConfig": config, "projectId": project_id(project) }),
    )
    .await
    .map(|_| ())
    .map_err(|error| error.message)
}

/// `mutateGpuiProjectBoardConversationLinkStores`.
///
/// CDXC:ProjectBoard 2026-08-07:
/// The board reads links from every project row that mounts the same Beads board, so a link the
/// user acts on can be stored on a row other than the one whose board is open. Apply link mutations
/// to each row that actually holds a matching link; a row whose links come back unchanged is never
/// written.
async fn mutate_link_stores(
    board: &Value,
    store: &[Value],
    now: &str,
    mutate: impl Fn(Vec<BeadConversationLink>, &Value) -> Vec<BeadConversationLink>,
) -> BoardResult<()> {
    let projects = match store.is_empty() {
        true => std::slice::from_ref(board),
        false => store,
    };
    for project in projects {
        let current = project_board_links(project, now);
        let next = mutate(current.clone(), project);
        if links_json(&next) == links_json(&current) {
            continue;
        }
        write_links(project, &next).await?;
    }
    Ok(())
}

/// `reloadGpuiProjectBoardDomainScope`.
///
/// CDXC:ProjectBoard 2026-08-07:
/// Starting work can take minutes before the link is written (the worktree path registers a
/// project, runs setup, and refreshes presentation), and /api/updateProject replaces
/// projectBoardConfig wholesale. Re-read the row so the link write extends the current links
/// instead of persisting a snapshot taken before the session existed; otherwise a link that landed
/// during the gap is dropped and its card reads as never worked.
async fn reload_scope(board: &Value, store: &[Value], context: &BoardContext) -> (Value, Vec<Value>) {
    match list_projects().await {
        Ok(projects) => {
            let board_id = project_id(board);
            let latest = projects
                .iter()
                .find(|project| project_id(project) == board_id)
                .cloned()
                .unwrap_or_else(|| board.clone());
            let store = select_link_store_projects(&latest, &projects, &context.global_beads_directory);
            (latest, store)
        }
        Err(_) => {
            let store = match store.is_empty() {
                true => vec![board.clone()],
                false => store.to_vec(),
            };
            (board.clone(), store)
        }
    }
}

/// `upsertGpuiProjectBoardConversationLink`.
async fn upsert_link(
    this: &App,
    cx: &mut AsyncApp,
    board: &Value,
    store: &[Value],
    agent: Option<&SidebarAgent>,
    bead_display_id: Option<&str>,
    bead_id: &str,
    session: &LinkedSession,
    context: &BoardContext,
) -> BoardResult<()> {
    let now = BoardContext::now_iso();
    // The presentation row as it is now, after the create: `this.presentation` at this moment.
    let row = this
        .update(cx, |this, _| {
            this.gx_store_board_context()
                .session_rows
                .remove(&(session.project_id.clone(), session.session_id.clone()))
        })
        .ok()
        .flatten()
        .unwrap_or(Value::Null);
    let row_text = |key: &str| {
        row.get(key)
            .and_then(Value::as_str)
            .filter(|value| !value.is_empty())
            .map(str::to_string)
    };
    let (latest, store) = reload_scope(board, store, context).await;
    let latest_id = project_id(&latest);
    // A shared board is read across every row that mounts it, so update the row that already
    // holds this conversation instead of adding a second copy.
    let board_session = board_session_id(&session.project_id, &session.session_id, &latest_id);
    let bead_key = bead_conversation_link_match_key(bead_id);
    let stored = stored_links(&store, &now);
    let matches = |link: &BeadConversationLink| {
        bead_conversation_link_match_key(&link.bead_id) == bead_key
            && resolve_board_session_id(link, &latest_id, &stored) == board_session
    };
    let link_project = store
        .iter()
        .find(|project| project_board_links(project, &now).iter().any(matches))
        .cloned()
        .unwrap_or_else(|| latest.clone());
    let link_project_id = project_id(&link_project);
    let ghostex_session_id = board_session_id(&session.project_id, &session.session_id, &link_project_id);
    let next = BeadConversationLink {
        agent_id: agent
            .map(|agent| agent.agent_id.clone())
            .or_else(|| row_text("agentId")),
        agent_name: agent
            .map(|agent| agent.name.clone())
            .or_else(|| row_text("agentName")),
        agent_session_id: session
            .agent_session_id
            .clone()
            .or_else(|| row_text("agentSessionId")),
        agent_session_path: session
            .agent_session_path
            .clone()
            .or_else(|| row_text("agentSessionPath")),
        bead_display_id: bead_display_id.map(str::to_string),
        bead_id: bead_id.to_string(),
        created_at: now.clone(),
        id: bead_conversation_link_id(&link_project_id, bead_id, &ghostex_session_id),
        ghostex_session_id,
        project_id: link_project_id,
        session_persistence_name: session.zmx_name.clone().or_else(|| row_text("zmxName")),
        session_persistence_provider: Some("zmx".to_string()),
        session_project_id: Some(session.project_id.clone()),
        status: "active".to_string(),
        updated_at: now.clone(),
    };
    let mut links = project_board_links(&link_project, &now);
    match links.iter().position(matches) {
        // `{ ...link, ...nextLink, createdAt: link.createdAt }`: every field of the new record,
        // absent ones included, over the old one.
        Some(index) => {
            links[index] = BeadConversationLink {
                created_at: links[index].created_at.clone(),
                ..next
            };
        }
        None => links.push(next),
    }
    write_links(&link_project, &links).await
}

/// `associateGpuiProjectBoardFocusedSession`.
pub(super) async fn associate_focused(
    this: &App,
    cx: &mut AsyncApp,
    request: &BoardRequest,
    context: &BoardContext,
) -> BoardResult<()> {
    let bead_id = request
        .bead_id
        .as_deref()
        .ok_or("No bead id is available.")?;
    let (board, store, projects) = resolve_scope(request, context).await?;
    let focused = context
        .facts
        .session_options(&board, &store, &projects)
        .into_iter()
        .find(|option| option.is_focused)
        .ok_or("Focus an agent session before associating this bead.")?;
    let (project_id, session_id) =
        ghostex_gx_core::board_reference(&focused.session_id, &project_id(&board));
    let session = LinkedSession {
        project_id,
        session_id,
        ..LinkedSession::default()
    };
    upsert_link(
        this,
        cx,
        &board,
        &store,
        None,
        request.bead_display_id.as_deref(),
        bead_id,
        &session,
        context,
    )
    .await
}

/// `startGpuiProjectBoardWork`.
pub(super) async fn start_work(
    this: &App,
    cx: &mut AsyncApp,
    request: &BoardRequest,
    context: &BoardContext,
) -> BoardResult<()> {
    let bead_id = request
        .bead_id
        .as_deref()
        .ok_or("No bead id is available.")?;
    let prompt = request
        .prompt
        .as_deref()
        .ok_or("No bead prompt is available.")?;
    let (board, store, _) = resolve_scope(request, context).await?;
    // `resolveDefaultPromptAgent(request.agentId)`: the card's pick, else the Default Prompt Agent.
    let agent_id = request
        .agent_id
        .clone()
        .unwrap_or_else(|| context.default_agent_id.clone());
    let agent = resolve_sidebar_agent(context.hud.as_ref(), &agent_id)
        .filter(|agent| agent.launch_command().is_some())
        .ok_or("Choose a configured agent before starting work.")?;
    let session = if request.start_location.as_deref() == Some("newWorktree") {
        start_worktree_work(this, cx, &board, &agent, prompt, context).await?
    } else {
        // macOS `handleProjectBoardStartWork` current-project path: focus the board project, then
        // launch the agent with the bead prompt staged as the gxserver first user message (the
        // created session is focused by the create path itself).
        let board_id = project_id(&board);
        if context.facts.active_project_id.as_deref() != Some(board_id.as_str()) {
            focus_project(this, cx, &board_id);
        }
        create_agent_record(
            this,
            cx,
            &board_id,
            &agent,
            prompt,
            context,
            "Could not create an agent session for this bead.",
        )
        .await?
    };
    upsert_link(
        this,
        cx,
        &board,
        &store,
        Some(&agent),
        request.bead_display_id.as_deref(),
        bead_id,
        &session,
        context,
    )
    .await
}

/// `focusProjectId(projectId)`: the board activates the project the way a sidebar group click
/// does, through the store's `focusGroup` (focus_perform.rs; the runtime's until 2026-09-25).
fn focus_project(this: &App, cx: &mut AsyncApp, project_id: &str) {
    let group_id = ProjectKey::local(project_id).to_sidebar_group_id();
    let _ = this.update(cx, |this, cx| {
        this.dispatch_native_sidebar_command(json!({ "type": "focusGroup", "groupId": group_id }), cx);
    });
}

/// `startGpuiProjectBoardWorktreeWork`.
///
/// macOS board "New worktree" starts ride `createNativeWorktreeForAgentPrompt` with baseBranch HEAD
/// and a "Worktree started" toast; GPUI makes the same one gxserver call the Add Worktree dialog
/// makes (unique target, git worktree add, project registration, Beads hooks, setup command) and
/// starts the agent with the bead prompt in the new project, with the same toast lifecycle.
async fn start_worktree_work(
    this: &App,
    cx: &mut AsyncApp,
    board: &Value,
    agent: &SidebarAgent,
    prompt: &str,
    context: &BoardContext,
) -> BoardResult<LinkedSession> {
    let toast_id = worktree_toast_id();
    toast(this, cx, "info", "Creating worktree", None, &toast_id, true);
    let created = async {
        let response = gx_rpc_with_timeout(
            None,
            "/api/createProjectWorktree",
            json!({
                "baseRef": "HEAD",
                "nameHint": worktree_slug_from_prompt(prompt),
                "projectId": project_id(board),
            }),
            WORKTREE_CREATE_TIMEOUT,
        )
        .await
        .map_err(|error| error.message)?;
        let worktree_id = response
            .get("project")
            .map(project_id)
            .filter(|id| !id.is_empty())
            .ok_or("gxserver did not register the new checkout as a worktree project.")?;
        let session = create_agent_record(
            this,
            cx,
            &worktree_id,
            agent,
            prompt,
            context,
            "Could not create an agent session in the worktree.",
        )
        .await?;
        focus_project(this, cx, &worktree_id);
        BoardResult::Ok(session)
    }
    .await;
    match created {
        Ok(session) => {
            let _ = this.update(cx, |this, _| {
                if let Some(client) = this.gx_store.client.as_ref() {
                    client.request_resubscribe();
                }
            });
            toast(this, cx, "success", "Worktree started", None, &toast_id, false);
            Ok(session)
        }
        Err(error) => {
            let description = worktree_user_visible_error(&error);
            toast(
                this,
                cx,
                "error",
                "Could not create worktree",
                Some(&description),
                &toast_id,
                false,
            );
            Err(error)
        }
    }
}

/// `createGpuiWorktreeToastId`.
fn worktree_toast_id() -> String {
    static SEQUENCE: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
    let sequence = SEQUENCE.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
    format!("toast-gpui-worktree-{:x}-{sequence}", super::super::host::now_ms())
}

/// `postWorktreeToast`: a toast replaced in place by its id.
fn toast(
    this: &App,
    cx: &mut AsyncApp,
    level: &str,
    title: &str,
    description: Option<&str>,
    toast_id: &str,
    persistent: bool,
) {
    let mut request = json!({ "level": level, "title": title, "toastId": toast_id, "type": "toast" });
    if let Some(description) = description {
        request["description"] = json!(description);
    }
    if persistent {
        request["persistent"] = json!(true);
    }
    let _ = this.update(cx, |this, cx| this.receive_gpui_app_toast_bridge_message(&request, cx));
}

/// `createAgentSessionRecordForProject(project, agent, prompt, { errorMessage })`: create the
/// agent's session with the prompt as its first message, select it, start its provider and queue
/// the prompt, each step acknowledged.
async fn create_agent_record(
    this: &App,
    cx: &mut AsyncApp,
    project_id: &str,
    agent: &SidebarAgent,
    prompt: &str,
    context: &BoardContext,
    missing_session: &str,
) -> BoardResult<LinkedSession> {
    let title_settings = this
        .update(cx, |this, _| this.gx_store_title_generation_settings())
        .map_err(|_| "gxserver is unavailable.".to_string())?;
    let params = agent_record_params(
        agent,
        project_id,
        prompt,
        first_prompt_title_runtime_settings(&title_settings, context.hud.as_ref(), Some(prompt), None),
        &AgentRecordOptions::default(),
        &agent_session_default_title(Some(&agent.name)),
    );
    let response = gx_rpc(None, "/api/createAgentSession", params)
        .await
        .map_err(|error| error.message)?;
    let (_, session_id) = created_session(&response, Some(project_id)).ok_or(missing_session)?;
    let text = |pointer: &str| {
        response
            .pointer(pointer)
            .and_then(Value::as_str)
            .filter(|value| !value.trim().is_empty())
            .map(str::to_string)
    };
    // The record names the project it was asked for, as the old runtime's did.
    let session = LinkedSession {
        project_id: project_id.to_string(),
        session_id: session_id.clone(),
        agent_session_id: text("/session/agentSessionId")
            .or_else(|| text("/session/runtimeSettings/agentSessionId")),
        agent_session_path: text("/session/agentSessionPath")
            .or_else(|| text("/session/runtimeSettings/agentSessionPath")),
        zmx_name: text("/session/zmxName"),
    };
    let focused = SessionKey::local(project_id, &session_id);
    let _ = this.update(cx, |this, cx| {
        this.gx_store_focus_created_session(&focused.project_id, &focused.session_id, false, None, cx);
    });
    gx_rpc(
        None,
        "/api/startSessionProvider",
        start_provider_params(project_id, &session_id),
    )
    .await
    .map_err(|error| error.message)?;
    let receipt = gx_rpc(
        None,
        "/api/queueSessionChatPrompt",
        queue_startup_prompt_params(project_id, &session_id, prompt),
    )
    .await
    .map_err(|error| error.message)?;
    check_startup_prompt_receipt(&receipt)?;
    Ok(session)
}

/// `jumpToGpuiProjectBoardConversation`: gxserver resolves the conversation (its live session,
/// else a restore, else a resume) and the bead link follows a restored or resumed session before
/// it is focused.
pub(super) async fn jump(
    this: &App,
    cx: &mut AsyncApp,
    request: &BoardRequest,
    context: &BoardContext,
) -> BoardResult<()> {
    let session_id = request
        .session_id
        .as_deref()
        .ok_or("No linked conversation is selected.")?;
    let (board, store, _) = resolve_scope(request, context).await?;
    let board_id = project_id(&board);
    let (reference_project, reference_session) =
        ghostex_gx_core::board_reference(session_id, &board_id);
    let answer = gx_rpc(
        None,
        "/api/openConversation",
        json!({
            "projectId": reference_project,
            "restoreReason": "projectBoardJumpToConversationRestore",
            "resumeReason": "projectBoardResumeConversation",
            "sessionId": reference_session,
        }),
    )
    .await
    .map_err(|error| error.message)?;
    let text = |key: &str| {
        answer
            .get(key)
            .and_then(Value::as_str)
            .filter(|value| !value.trim().is_empty())
            .map(str::to_string)
    };
    let (Some(outcome), Some(opened_project), Some(opened_session)) =
        (text("outcome"), text("projectId"), text("sessionId"))
    else {
        return Err("The linked Ghostex session is no longer available.".to_string());
    };
    if outcome == "focus" {
        let sidebar_session_id = SessionKey::local(&opened_project, &opened_session).to_sidebar_session_id();
        let _ = this.update(cx, |this, cx| {
            this.dispatch_native_sidebar_ui(
                json!({ "type": "selectSession", "mode": "focus", "sessionId": sidebar_session_id }),
                cx,
            );
        });
        return Ok(());
    }
    let checked = format!("{reference_project}:{reference_session}");
    let _ = this.update(cx, |this, _| {
        this.gx_store.create.board_link_checks.remove(&checked);
    });
    replace_link_session(
        &board,
        &store,
        request.bead_id.as_deref(),
        session_id,
        &opened_project,
        &opened_session,
        text("zmxName"),
    )
    .await?;
    let _ = this.update(cx, |this, cx| {
        this.gx_store_focus_created_session(&opened_project, &opened_session, false, None, cx);
    });
    Ok(())
}

/// `replaceGpuiProjectBoardConversationLinkSession`: macOS `replaceProjectBoardConversationLinkSession`.
/// Every link on the old session id moves to the restored one (scoped to one bead when the jump
/// carried a bead id), collapsing any pre-existing duplicate link.
async fn replace_link_session(
    board: &Value,
    store: &[Value],
    bead_id: Option<&str>,
    old_session_id: &str,
    restored_project: &str,
    restored_session: &str,
    restored_persistence_name: Option<String>,
) -> BoardResult<()> {
    let now = BoardContext::now_iso();
    let board_id = project_id(board);
    let ghostex_session_id = board_session_id(restored_project, restored_session, &board_id);
    let stored = stored_links(store, &now);
    let bead_key = bead_id.map(bead_conversation_link_match_key);
    mutate_link_stores(board, store, &now, |links, store_project| {
        links
            .into_iter()
            .filter_map(|link| {
                let bead_matches = bead_key
                    .as_ref()
                    .is_none_or(|key| bead_conversation_link_match_key(&link.bead_id) == *key);
                let board_session = resolve_board_session_id(&link, &board_id, &stored);
                let is_target = board_session == old_session_id && bead_matches;
                if !is_target {
                    let duplicate =
                        bead_key.is_some() && bead_matches && board_session == ghostex_session_id;
                    return (!duplicate).then_some(link);
                }
                Some(BeadConversationLink {
                    id: bead_conversation_link_id(
                        &project_id(store_project),
                        &link.bead_id,
                        &ghostex_session_id,
                    ),
                    ghostex_session_id: ghostex_session_id.clone(),
                    // The stored provider name describes the session being replaced, so it is
                    // re-stated from the new session rather than left to describe a session this
                    // link no longer points at.
                    session_persistence_name: restored_persistence_name.clone(),
                    session_project_id: Some(restored_project.to_string()),
                    updated_at: now.clone(),
                    ..link
                })
            })
            .collect()
    })
    .await
}

/// `unlinkGpuiProjectBoardConversation`: the link is archived, not deleted.
pub(super) async fn unlink(request: &BoardRequest, context: &BoardContext) -> BoardResult<()> {
    let bead_id = request
        .bead_id
        .as_deref()
        .ok_or("No bead id is available.")?;
    let session_id = request
        .session_id
        .as_deref()
        .ok_or("No linked conversation is selected.")?;
    let (board, store, _) = resolve_scope(request, context).await?;
    let now = BoardContext::now_iso();
    let board_id = project_id(&board);
    let bead_key = bead_conversation_link_match_key(bead_id);
    let stored = stored_links(&store, &now);
    mutate_link_stores(&board, &store, &now, |links, _| {
        links
            .into_iter()
            .map(|link| {
                let target = bead_conversation_link_match_key(&link.bead_id) == bead_key
                    && resolve_board_session_id(&link, &board_id, &stored) == session_id;
                match target {
                    true => BeadConversationLink {
                        status: "archived".to_string(),
                        updated_at: now.clone(),
                        ..link
                    },
                    false => link,
                }
            })
            .collect()
    })
    .await
}
