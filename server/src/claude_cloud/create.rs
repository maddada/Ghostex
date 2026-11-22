//! The cloud half of `/api/createAgentSession`: which project a cloud session belongs to, and the
//! record and account it starts with.

use std::path::{Path, PathBuf};

use rusqlite::Connection;
use serde_json::{json, Map, Value};

use super::record::{CloudRecord, CloudState, RECORD_KEY};
use super::{CLAUDE_CLOUD_AGENT_ID, CLAUDE_CLOUD_RUN_LOCATION};
use crate::cloud_runner::MAX_CLOUD_PROMPT_CHARS;
use crate::domain::{DomainRepository, DomainStateError};

/// Runtime keys an account launch writes. A cloud session keeps its account in its record and
/// runs no local agent, so none of them may wrap its terminal's command.
const ACCOUNT_RUNTIME_KEYS: &[&str] = &[
    "accountBaseCommand",
    "accountCommand",
    "accountId",
    "accountName",
    "accountProvider",
    "accountSlot",
];

fn text(params: &Map<String, Value>, key: &str) -> Option<String> {
    params
        .get(key)
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(str::to_string)
}

/// Whether a create asks for a Claude cloud session: the Claude (cloud) agent, or Claude cloud
/// picked as where it runs.
fn is_cloud_create(params: &Map<String, Value>) -> bool {
    text(params, "agentId").as_deref() == Some(CLAUDE_CLOUD_AGENT_ID)
        || text(params, "runLocation").as_deref() == Some(CLAUDE_CLOUD_RUN_LOCATION)
}

fn canonical(path: &str) -> PathBuf {
    std::fs::canonicalize(path).unwrap_or_else(|_| PathBuf::from(path))
}

/// The project whose folder is the main checkout of `project`'s repository: the parent of a
/// worktree project Ghostex made, or of any folder Git calls a linked worktree. `project` itself
/// when it is the main checkout or no project holds the main checkout.
///
/// CDXC:ClaudeCloud 2026-10-23 DECISION:
/// User: "we need to show the row for that cloud session in the main project folder of that repo on the machine". A cloud session works on the repository on GitHub, not on a checkout, so its row goes under the project of the repo's main checkout whichever project (a worktree project included) it was started from: New Thread, the CLI, the Work page or a migrated record.
fn main_checkout_project(
    repository: &DomainRepository<'_>,
    project: &Value,
) -> Result<Value, DomainStateError> {
    let projects = repository.list_projects()?;
    let usable = |candidate: &&Value| {
        candidate.get("isRecentProject").and_then(Value::as_bool) != Some(true)
            && candidate.get("path").and_then(Value::as_str).is_some()
    };
    if let Some(parent_id) = project
        .pointer("/worktree/parentProjectId")
        .and_then(Value::as_str)
    {
        if let Some(parent) = projects
            .iter()
            .filter(usable)
            .find(|candidate| candidate.get("projectId").and_then(Value::as_str) == Some(parent_id))
        {
            return Ok(parent.clone());
        }
    }
    let Some(path) = project.get("path").and_then(Value::as_str) else {
        return Ok(project.clone());
    };
    let Some(common_dir) = crate::session_git_status::run_git_probe_command(
        path,
        &["rev-parse", "--path-format=absolute", "--git-common-dir"],
    ) else {
        return Ok(project.clone());
    };
    let common_dir = PathBuf::from(common_dir.trim());
    if common_dir.file_name().and_then(|name| name.to_str()) != Some(".git") {
        return Ok(project.clone());
    }
    let Some(main) = common_dir.parent().map(Path::to_path_buf) else {
        return Ok(project.clone());
    };
    let main = canonical(&main.to_string_lossy());
    if canonical(path) == main {
        return Ok(project.clone());
    }
    Ok(projects
        .iter()
        .filter(usable)
        .find(|candidate| {
            candidate
                .get("path")
                .and_then(Value::as_str)
                .is_some_and(|candidate_path| canonical(candidate_path) == main)
        })
        .cloned()
        .unwrap_or_else(|| project.clone()))
}

/// A cloud create's params: the Claude (cloud) agent, in the main checkout's project, with no
/// working folder of its own. `None` for every other create.
pub(crate) fn cloud_create_params(
    repository: &DomainRepository<'_>,
    params: &Map<String, Value>,
) -> Result<Option<Map<String, Value>>, DomainStateError> {
    if !is_cloud_create(params) {
        return Ok(None);
    }
    let agent = text(params, "agentId");
    if agent
        .as_deref()
        .is_some_and(|agent| agent != CLAUDE_CLOUD_AGENT_ID && agent != "claude")
    {
        return Err(DomainStateError::bad_request(
            "Only Claude runs in Claude's cloud.",
        ));
    }
    let project = repository.resolve_create_session_project(params)?;
    let main = main_checkout_project(repository, &project)?;
    let mut params = params.clone();
    params.insert("agentId".to_string(), json!(CLAUDE_CLOUD_AGENT_ID));
    params.remove("runLocation");
    params.remove("cwd");
    params.remove("projectPath");
    if let Some(project_id) = main.get("projectId").cloned() {
        params.insert("projectId".to_string(), project_id);
    }
    Ok(Some(params))
}

/// Fills `runtimeSettings.claudeCloud` for a new cloud session: a draft whose task (the create's
/// first message, if any) starts it, on the Claude account the launch would use.
///
/// CDXC:ClaudeCloud 2026-10-23 WHY: the first message is kept in the record instead of being typed into the terminal, because the terminal runs Ghostex's cloud client, which starts the cloud session with it (`/api/claudeCloud` `start`); a long or multi-line task would not survive being typed as one line.
pub(crate) fn prepare_cloud_runtime(
    db: &Connection,
    project: &Value,
    runtime_settings: &mut Map<String, Value>,
) -> Result<(), DomainStateError> {
    let path = project
        .get("path")
        .and_then(Value::as_str)
        .ok_or_else(|| DomainStateError::bad_request("This project has no folder."))?;
    let repo = crate::work_mode::work_repo_of(path).ok_or_else(|| {
        DomainStateError::bad_request(
            "This project has no GitHub remote (origin), so Claude's cloud cannot get its code.",
        )
    })?;
    let task = runtime_settings
        .remove("firstUserMessage")
        .and_then(|value| value.as_str().map(str::trim).map(str::to_string))
        .filter(|task| !task.is_empty());
    if let Some(task) = &task {
        let length = task.chars().count();
        if length > MAX_CLOUD_PROMPT_CHARS {
            return Err(DomainStateError::bad_request(format!(
                "The task is {length} characters; a cloud session starts with at most {MAX_CLOUD_PROMPT_CHARS}. Shorten it and try again."
            )));
        }
    }
    let account = pick_claude_account(
        db,
        project,
        runtime_settings.get("accountId").and_then(Value::as_str),
    )?;
    for key in ACCOUNT_RUNTIME_KEYS {
        runtime_settings.remove(*key);
    }
    let mut record = CloudRecord::new(CloudState::Draft);
    record.repo = Some(repo);
    if let Some(task) = task {
        record.raw.insert("task".to_string(), json!(task));
    }
    for (key, value) in account {
        record.raw.insert(key, value);
    }
    record
        .raw
        .insert("createdAt".to_string(), json!(crate::domain::now_iso()));
    runtime_settings.insert(RECORD_KEY.to_string(), record.to_value());
    Ok(())
}

/// The Claude account a new cloud session runs on, as `accountId` and `accountName` record keys:
/// the one the create picked, else the account of the project's last cloud session (while it is
/// still saved), else the workspace account and the Account for new sessions rule, as the
/// sidebar's Claude button picks it. Empty when Claude runs on the current CLI login.
///
/// CDXC:ClaudeCloud 2026-10-23 SEE-ALSO: the user's decision on this order is on `project_last_cloud_account` in packages/gx-core/src/claude_cloud.rs, which the New Thread picker highlights the same way.
pub(crate) fn pick_claude_account(
    db: &Connection,
    project: &Value,
    requested: Option<&str>,
) -> Result<Map<String, Value>, DomainStateError> {
    // Resolved on a scratch map, so nothing wraps the session's own command.
    let mut scratch = Map::new();
    let last_cloud = requested
        .is_none()
        .then(|| project_last_cloud_account(db, project))
        .flatten();
    if let Some(account_id) = requested.map(str::to_string).or(last_cloud) {
        scratch.insert("accountId".to_string(), json!(account_id));
    }
    let workspace_account = crate::workspaces::project_claude_account_id(db, project)?;
    crate::accounts::launch::apply_new_session(
        db,
        "claude",
        Some("claude"),
        workspace_account.as_deref(),
        &mut scratch,
    )?;
    let mut account: Map<String, Value> = ["accountId", "accountName"]
        .into_iter()
        .filter_map(|key| Some((key.to_string(), scratch.get(key)?.clone())))
        .collect();
    // The email its claude.ai page must be signed in as (Q415's match by email).
    if let Some(account_id) = account
        .get("accountId")
        .and_then(Value::as_str)
        .map(str::to_string)
    {
        account.extend(super::accounts::account_facts(db, &account_id));
    }
    Ok(account)
}

/// The saved Claude account of the project's newest cloud session, while it is still saved and on.
fn project_last_cloud_account(db: &Connection, project: &Value) -> Option<String> {
    let project_id = project.get("projectId").and_then(Value::as_str)?;
    let account_id: String = db
        .query_row(
            "SELECT json_extract(runtimeSettingsJson, '$.claudeCloud.accountId') FROM sessions \
             WHERE projectId = ?1 AND json_extract(runtimeSettingsJson, '$.claudeCloud.accountId') IS NOT NULL \
             ORDER BY createdAt DESC LIMIT 1",
            rusqlite::params![project_id],
            |row| row.get(0),
        )
        .ok()?;
    let registry = crate::accounts::store::read(db).ok()?;
    registry
        .accounts
        .iter()
        .any(|account| {
            account.id == account_id
                && account.provider == crate::accounts::model::Provider::Claude
                && !account.user_disabled
        })
        .then_some(account_id)
}

/// Drops a client-supplied record from a create that is not a cloud create: only gxserver makes a
/// session a cloud session.
pub(crate) fn strip_client_record(runtime_settings: &mut Map<String, Value>) {
    runtime_settings.remove(RECORD_KEY);
}
