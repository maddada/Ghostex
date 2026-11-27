//! `/api/readSessionWhere`: where one agent session runs, for the where pill at the left of the
//! chat box's pill strip (packages/gx-chat-core/src/extras/where_pill.rs).
//!
//! Read on the machine that runs the session, so every client sees the same facts: the project
//! (the parent's name for a worktree), the folder, this machine or the box the session runs in
//! (a Cloud Box or an agentbox), the worktree, the branch with its lines changed against the
//! default branch, its pull request, how far it is ahead of and behind its upstream, how many files
//! have uncommitted changes, the agent's account and the terminal backend that keeps the session.
//! The machine's own name (`location.computer`, its host name, the one phones and other computers
//! pair with) comes with it; a client that saved its own name for that machine shows that instead.
//!
//! Unlike `/api/readSessionDetails` it does not depend on the Session details extension, and it is
//! cheap: the branch and lines changed come from the Session details panel's own cache
//! (`session_details::read_changes`), the rest from a short cache of three git reads.
//!
//! CDXC:SessionChat 2026-11-17 DECISION:
//! User, from the where pill mockups (Q3781 to Q3785): a two-part pill at the left end of the pill strip always shows the project, the machine and the branch; hovering shows a card with the project, machine, Cloud Box, branch, worktree, pull request, counts, folder, agent and terminal; a click opens the same details with Copy branch and Open folder. Ahead and behind and the changed-file count are new per session reads, made here.
//! SEE-ALSO: packages/gx-chat-core/src/extras/where_pill.rs (what the pill shows), apps/desktop/src/app/native_chat/where_pill.rs (the drawing).

use std::collections::HashMap;
use std::path::Path;
use std::sync::{Mutex, OnceLock};
use std::time::{Duration, Instant};

use axum::http::StatusCode;
use serde_json::{json, Map, Value};

use crate::domain::{read_domain_rpc_params, DomainRepository, DomainStateError};
use crate::protocol::rpc_success;
use crate::server::{domain_error_response, routed_json, AppState, RoutedResponse};
use crate::storage::open_gxserver_database;

/// How long the ahead, behind and changed-file counts are reused for one folder.
const COUNTS_FRESH_FOR: Duration = Duration::from_secs(20);

fn failure(code: &'static str, message: impl Into<String>) -> DomainStateError {
    DomainStateError {
        code,
        message: message.into(),
    }
}

fn text_param(params: &Map<String, Value>, key: &str) -> String {
    params
        .get(key)
        .and_then(Value::as_str)
        .map(str::trim)
        .unwrap_or_default()
        .to_string()
}

fn text(value: &Value, pointer: &str) -> Option<String> {
    value
        .pointer(pointer)
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|text| !text.is_empty())
        .map(str::to_string)
}

/// `/api/readSessionWhere {projectId, sessionId}`.
pub(crate) async fn handle_read_session_where_http(
    state: &AppState,
    endpoint_path: String,
    request_id: String,
    body: &Value,
) -> RoutedResponse {
    let outcome = async {
        let params = read_domain_rpc_params(body)?;
        let (project_id, session_id) = (
            text_param(&params, "projectId"),
            text_param(&params, "sessionId"),
        );
        if project_id.is_empty() || session_id.is_empty() {
            return Err(failure(
                "invalidParams",
                "readSessionWhere requires projectId and sessionId.",
            ));
        }
        let state = state.clone();
        tokio::task::spawn_blocking(move || read_where(&state, &project_id, &session_id))
            .await
            .map_err(|error| failure("internalError", error.to_string()))?
    }
    .await;
    match outcome {
        Ok(result) => routed_json(
            Some(endpoint_path),
            StatusCode::OK,
            rpc_success(request_id, result),
        ),
        Err(error) => domain_error_response(endpoint_path, request_id, error),
    }
}

fn read_where(
    state: &AppState,
    project_id: &str,
    session_id: &str,
) -> Result<Value, DomainStateError> {
    let db = open_gxserver_database(&state.paths).map_err(|error| {
        failure(
            "internalError",
            format!("SQLite gxserver state error: {error}"),
        )
    })?;
    let repository = DomainRepository::new(&db, state.metadata.server_id.as_str());
    let session = repository
        .get_session(project_id, session_id)?
        .ok_or_else(|| failure("notFound", "The session no longer exists."))?;
    let project = repository.get_project(project_id)?;
    let parent = project
        .as_ref()
        .and_then(|project| text(project, "/worktree/parentProjectId"))
        .and_then(|parent_id| repository.get_project(&parent_id).ok().flatten());
    drop(db);

    let agent = crate::session_chat_follower::session_chat_display_agent_for_session(&session);
    let folder = crate::session_git_status::effective_session_git_cwd(&session, project.as_ref());
    let environment = crate::session_details::read_environment(
        &session,
        project.as_ref(),
        folder.as_deref(),
        &state.paths.home_dir,
        agent.as_deref(),
    );
    let worktree = environment["worktree"] == Value::Bool(true);
    // Main Hub's sessions are named Ghostex AI, its name since 2026-11-27 (session_details/environment.rs).
    let main_hub = environment["mainHub"] == Value::Bool(true);
    let project_name = if main_hub {
        Some("Ghostex AI".to_string())
    } else {
        project.as_ref().and_then(|project| text(project, "/name"))
    };
    let parent_name = parent.as_ref().and_then(|parent| text(parent, "/name"));
    let git = folder.as_deref().and_then(|folder| {
        let mut git = crate::session_details::read_changes(folder)?;
        let counts = read_counts(folder);
        if let (Some(git), Some(counts)) = (git.as_object_mut(), counts.as_object()) {
            git.extend(counts.clone());
        }
        Some(git)
    });
    // CDXC:SessionChat 2026-11-18 DECISION:
    // User: "i have email hidden but ur showing it here pls fix" (the where pill's Agent row showed
    // the account's email under Settings > Accounts > Hide account emails). With it on, the account
    // and its email leave this machine masked the way every other account surface masks them, so
    // the raw email never reaches the desktop card or the phone, which has no such setting.
    let hide_emails = crate::session_chat_notice::account_emails_hidden();
    Ok(json!({
        // A worktree shows under its parent's name; its own folder is the worktree's name.
        "project": parent_name.clone().or(project_name),
        "mainHub": main_hub,
        "folder": folder,
        "folderLabel": environment["folderLabel"],
        "location": location(&session),
        "worktree": worktree.then(|| json!({
            "name": folder
                .as_deref()
                .and_then(|folder| Path::new(folder).file_name())
                .map(|name| name.to_string_lossy().to_string()),
            "parentProject": parent_name,
        })),
        "git": git,
        "agent": agent,
        "account": account_text(&environment["account"], hide_emails),
        "accountEmail": account_text(&environment["accountEmail"], hide_emails),
        "terminal": terminal_backend(&session),
    }))
}

/// An account text field, masked by `mask_account_text` when Hide account emails is on.
fn account_text(value: &Value, hide_emails: bool) -> Value {
    match value.as_str() {
        Some(text) if hide_emails => json!(ghostex_gx_core::mask_account_text(text)),
        _ => value.clone(),
    }
}

/// `{kind, label, computer}`: `cloudBox` with its provider for a session on a Cloud Box's own
/// gxserver, `box` with its provider for an agentbox session, else `local`, the machine that
/// answered. `computer` is that machine's own name in every case.
fn location(session: &Value) -> Value {
    let computer = crate::connected_computers::this_computer_name();
    use ghostex_gx_protocol::cloud_box::{SessionCloudBoxRuntime, CLOUD_BOX_RUNTIME_SETTINGS_KEY};
    let cloud_box = session
        .get("runtimeSettings")
        .and_then(|settings| settings.get(CLOUD_BOX_RUNTIME_SETTINGS_KEY))
        .filter(|record| record.get("pending").and_then(Value::as_bool) != Some(true))
        .and_then(|record| serde_json::from_value::<SessionCloudBoxRuntime>(record.clone()).ok())
        .filter(|runtime| !runtime.box_id.is_empty());
    if let Some(runtime) = cloud_box {
        return json!({
            "kind": "cloudBox",
            "label": runtime.provider.label(),
            "computer": computer,
        });
    }
    match crate::agentbox::session_agentbox(session) {
        Some(agentbox) => json!({
            "kind": "box",
            "label": crate::agentbox::provider_label(&agentbox.provider),
            "computer": computer,
        }),
        None => json!({"kind": "local", "computer": computer}),
    }
}

/// The program that keeps the session's terminal (`zmx`, `tmux`, `zellij`, `wmx`), or `None` when
/// none does.
fn terminal_backend(session: &Value) -> Option<String> {
    crate::server::read_session_persistence_provider(session)
        .map(|provider| provider.trim().to_ascii_lowercase())
        .filter(|provider| !provider.is_empty() && provider != "off")
}

#[derive(Clone)]
struct Counts {
    at: Instant,
    value: Value,
}

fn counts_cache() -> &'static Mutex<HashMap<String, Counts>> {
    static CACHE: OnceLock<Mutex<HashMap<String, Counts>>> = OnceLock::new();
    CACHE.get_or_init(Mutex::default)
}

/// `{upstream, ahead, behind, changedFiles}` for the checkout at `cwd`: commits ahead of and behind
/// its upstream (`null` without one), and the files `git status` lists. Blocking; reused for
/// [`COUNTS_FRESH_FOR`].
fn read_counts(cwd: &str) -> Value {
    let now = Instant::now();
    if let Some(cached) = counts_cache()
        .lock()
        .ok()
        .and_then(|cache| cache.get(cwd).cloned())
        .filter(|cached| now.duration_since(cached.at) < COUNTS_FRESH_FOR)
    {
        return cached.value;
    }
    let git = |args: &[&str]| crate::session_git_status::run_git_probe_command(cwd, args);
    let upstream = git(&[
        "rev-parse",
        "--abbrev-ref",
        "--symbolic-full-name",
        "@{upstream}",
    ])
    .map(|upstream| upstream.trim().to_string())
    .filter(|upstream| !upstream.is_empty());
    let (ahead, behind) = upstream
        .as_ref()
        .and_then(|_| git(&["rev-list", "--left-right", "--count", "HEAD...@{upstream}"]))
        .and_then(|counts| {
            let mut parts = counts.split_whitespace().map(|n| n.parse::<u64>().ok());
            Some((parts.next()??, parts.next()??))
        })
        .map_or((Value::Null, Value::Null), |(ahead, behind)| {
            (json!(ahead), json!(behind))
        });
    let changed_files = git(&[
        "status",
        "--porcelain=v1",
        "--untracked-files=normal",
        "--ignore-submodules=dirty",
    ])
    .map_or(Value::Null, |status| {
        json!(status
            .lines()
            .filter(|line| !line.trim().is_empty())
            .count())
    });
    let value = json!({
        "upstream": upstream,
        "ahead": ahead,
        "behind": behind,
        "changedFiles": changed_files,
    });
    if let Ok(mut cache) = counts_cache().lock() {
        if cache.len() > 64 {
            cache.clear();
        }
        cache.insert(
            cwd.to_string(),
            Counts {
                at: now,
                value: value.clone(),
            },
        );
    }
    value
}
