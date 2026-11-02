//! `ghostex box prepare-session [--request <file>]`: get the checkout up to date, put the session
//! on its own branch (Q1005: from the latest base branch, not the snapshot's), copy the ticked
//! secret files in, pick the session's login, and start the agent session on the box's gxserver
//! with the first message.

use std::path::{Path, PathBuf};

use ghostex_gx_protocol::cloud_box::{
    box_files, BoxPrepareSessionRequest, BoxPrepareSessionResult, SessionCloudBoxRuntime,
    CLOUD_BOX_RUNTIME_SETTINGS_KEY,
};
use serde_json::{json, Map, Value};

use super::git::{default_branch, folder_name, git, has_origin, is_repository};
use super::{logins, state};
use crate::ghostex_cli::{args::Flags, rpc};

/// How long prepare waits for the agent to report its own session id after starting.
const AGENT_SESSION_ID_WAIT: std::time::Duration = std::time::Duration::from_secs(15);

fn read_request(path: Option<&str>) -> Result<BoxPrepareSessionRequest, String> {
    let ticket = state::require_ticket()?;
    let path = path.map(PathBuf::from).unwrap_or_else(|| {
        state::private_dir(&ticket.provider).join(box_files::PREPARE_SESSION_REQUEST)
    });
    let request: BoxPrepareSessionRequest = serde_json::from_slice(
        &std::fs::read(&path).map_err(|error| format!("Can't read {}: {error}", path.display()))?,
    )
    .map_err(|error| {
        format!(
            "{} is not a prepare-session request: {error}",
            path.display()
        )
    })?;
    if request.box_id != ticket.box_id {
        return Err(format!(
            "That request is for {}, but this is {}.",
            request.box_id, ticket.box_id
        ));
    }
    if request.checkout_path.trim().is_empty() || !request.checkout_path.starts_with('/') {
        return Err("The request needs an absolute checkoutPath.".to_string());
    }
    Ok(request)
}

/// Clones the repository when the box has no checkout yet (a blank box), else fetches.
fn ensure_checkout(request: &BoxPrepareSessionRequest) -> Result<PathBuf, String> {
    let checkout = PathBuf::from(&request.checkout_path);
    if !checkout.exists() {
        let url = request.repo_url.as_deref().ok_or_else(|| {
            format!(
                "{} is not on this box and the request names no repository to clone.",
                checkout.display()
            )
        })?;
        if let Some(parent) = checkout.parent() {
            std::fs::create_dir_all(parent)
                .map_err(|error| format!("Can't make {}: {error}", parent.display()))?;
        }
        git(
            checkout.parent().unwrap_or(Path::new("/")),
            &["clone", url, &checkout.to_string_lossy()],
        )?;
    } else if !is_repository(&checkout) {
        return Err(format!("{} is not a git checkout.", checkout.display()));
    } else if has_origin(&checkout) {
        git(&checkout, &["fetch", "--prune", "origin"])?;
    }
    Ok(checkout)
}

/// `origin/<base>` when the remote has it, else the local `<base>`.
fn start_point(checkout: &Path, base: &str) -> String {
    let remote = format!("origin/{base}");
    if git(checkout, &["rev-parse", "--verify", "--quiet", &remote]).is_ok() {
        remote
    } else {
        base.to_string()
    }
}

fn branch_exists(checkout: &Path, branch: &str) -> bool {
    git(
        checkout,
        &[
            "rev-parse",
            "--verify",
            "--quiet",
            &format!("refs/heads/{branch}"),
        ],
    )
    .is_ok()
}

/// Uncommitted changes, leaving out the secret files box mode copied in.
fn has_changes(checkout: &Path, placed: &state::Placed) -> bool {
    git(checkout, &["status", "--porcelain"])
        .unwrap_or_default()
        .lines()
        .filter_map(|line| line.get(3..))
        .any(|path| {
            let absolute = checkout.join(path.trim_matches('"'));
            !placed
                .secret_files
                .iter()
                .any(|file| Path::new(&file.path) == absolute)
        })
}

/// Puts the session's folder on its branch and returns the folder.
fn prepare_branch(
    checkout: &Path,
    request: &BoxPrepareSessionRequest,
    placed: &state::Placed,
) -> Result<PathBuf, String> {
    let base = request
        .base_branch
        .clone()
        .filter(|base| !base.trim().is_empty())
        .or_else(|| default_branch(checkout))
        .ok_or_else(|| "Can't tell which branch to start from.".to_string())?;
    let from = start_point(checkout, &base);
    let branch = request
        .branch
        .as_deref()
        .filter(|name| !name.trim().is_empty());
    if request.worktree {
        let branch = branch.ok_or("A worktree on the box needs a branch name.")?;
        let parent = checkout.parent().unwrap_or(Path::new("/"));
        let name = checkout
            .file_name()
            .map(|name| name.to_string_lossy().into_owned())
            .unwrap_or_else(|| "project".to_string());
        let folder = parent
            .join(format!("{name}-worktrees"))
            .join(folder_name(branch));
        if is_repository(&folder) {
            return Ok(folder);
        }
        let folder_text = folder.to_string_lossy().into_owned();
        if branch_exists(checkout, branch) {
            git(checkout, &["worktree", "add", &folder_text, branch])?;
        } else {
            git(
                checkout,
                &["worktree", "add", "-b", branch, &folder_text, &from],
            )?;
        }
        return Ok(folder);
    }
    let current = git(checkout, &["rev-parse", "--abbrev-ref", "HEAD"]).unwrap_or_default();
    let target = branch.unwrap_or(base.as_str());
    if current == target {
        if branch.is_none() && from != base {
            // This folder on the base branch: bring it up to date when that is a fast-forward.
            let _ = git(checkout, &["merge", "--ff-only", &from]);
        }
        return Ok(checkout.to_path_buf());
    }
    if has_changes(checkout, placed) {
        return Err(format!(
            "The box's checkout has uncommitted changes on {current}; start this session in a new worktree instead."
        ));
    }
    if branch_exists(checkout, target) {
        git(checkout, &["checkout", target])?;
    } else {
        git(checkout, &["checkout", "-b", target, &from])?;
    }
    Ok(checkout.to_path_buf())
}

/// A project-relative path that stays inside the project.
fn inside(cwd: &Path, relative: &str) -> Option<PathBuf> {
    let relative = Path::new(relative);
    let safe = !relative.is_absolute()
        && relative
            .components()
            .all(|part| matches!(part, std::path::Component::Normal(_)));
    safe.then(|| cwd.join(relative))
}

/// Copies the ticked secret files into the session's folder and records them.
pub(crate) fn place_secret_files(
    cwd: &Path,
    secrets: &ghostex_gx_protocol::cloud_box::BoxSecretsFile,
    placed: &mut state::Placed,
) -> Result<Vec<String>, String> {
    let mut written = Vec::new();
    for file in &secrets.files {
        let target = inside(cwd, &file.path)
            .ok_or_else(|| format!("\"{}\" is not a path inside the project.", file.path))?;
        state::write_private(&target, file.contents.as_bytes())?;
        let entry = state::PlacedSecretFile {
            path: target.to_string_lossy().into_owned(),
            source: file.path.clone(),
        };
        if !placed.secret_files.contains(&entry) {
            placed.secret_files.push(entry);
        }
        written.push(target.to_string_lossy().into_owned());
    }
    Ok(written)
}

fn text<'a>(value: &'a Value, pointer: &str) -> Option<&'a str> {
    value
        .pointer(pointer)
        .and_then(Value::as_str)
        .filter(|text| !text.is_empty())
}

/// The agent session id the box's gxserver learned for the session, waiting a little for the
/// agent's first hook.
fn wait_for_agent_session_id(project_id: &str, session_id: &str, flags: &Flags) -> Option<String> {
    let started = std::time::Instant::now();
    loop {
        if let Ok(snapshot) =
            rpc::call_gxserver_rpc("/api/readPresentationSnapshot", &json!({}), flags)
        {
            let found = snapshot
                .pointer("/snapshot/sessions")
                .and_then(Value::as_array)
                .and_then(|sessions| {
                    sessions.iter().find(|session| {
                        text(session, "/projectId") == Some(project_id)
                            && text(session, "/sessionId") == Some(session_id)
                    })
                })
                .and_then(|session| text(session, "/agentSessionId").map(str::to_string));
            if found.is_some() {
                return found;
            }
        }
        if started.elapsed() >= AGENT_SESSION_ID_WAIT {
            return None;
        }
        std::thread::sleep(std::time::Duration::from_millis(500));
    }
}

pub(crate) fn run_prepare_session(
    request_path: Option<&str>,
) -> Result<BoxPrepareSessionResult, String> {
    let request = read_request(request_path)?;
    let ticket = state::require_ticket()?;
    let mut placed = state::read_placed();
    let checkout = ensure_checkout(&request)?;
    let cwd = prepare_branch(&checkout, &request, &placed)?;
    let secrets = logins::read_secrets(&ticket.provider);
    place_secret_files(&cwd, &secrets, &mut placed)?;
    let cwd_text = cwd.to_string_lossy().into_owned();
    if !placed.checkouts.contains(&cwd_text) {
        placed.checkouts.push(cwd_text.clone());
    }
    state::write_placed(&placed)?;

    let account = logins::activate(
        &ticket.provider,
        &request.agent_id,
        request.account_id.as_deref(),
    )?;
    let runs_claude = request.agent_id == "claude"
        || account
            .as_ref()
            .is_some_and(|account| account.kind == logins::CLAUDE_BOX_TOKEN);
    if runs_claude {
        logins::prepare_claude_folder(&cwd)?;
    }

    let branch = git(&cwd, &["rev-parse", "--abbrev-ref", "HEAD"])
        .ok()
        .filter(|name| name != "HEAD");
    let flags = Flags::default();
    let project =
        rpc::call_gxserver_rpc("/api/addProjectPath", &json!({ "path": cwd_text }), &flags)
            .map_err(|error| format!("The box's gxserver did not add the project: {error}"))?;
    let project_id = text(&project, "/project/projectId")
        .ok_or("The box's gxserver added the project without an id.")?
        .to_string();
    let cloud_box = SessionCloudBoxRuntime {
        box_id: ticket.box_id.clone(),
        provider: ticket.provider.clone(),
        home_project_id: request
            .home_project_id
            .clone()
            .or(ticket.home_project_id.clone()),
        home_machine_id: request
            .home_machine_id
            .clone()
            .or(ticket.home_machine_id.clone()),
        home_session_id: request.home_session_id.clone(),
        snapshot_id: request.snapshot_id.clone(),
        branch: branch.clone(),
        agent_session_id: None,
    };
    let mut runtime = Map::new();
    runtime.insert(
        CLOUD_BOX_RUNTIME_SETTINGS_KEY.to_string(),
        serde_json::to_value(&cloud_box).unwrap_or_default(),
    );
    if let Some(message) = request
        .first_message
        .as_deref()
        .filter(|message| !message.trim().is_empty())
    {
        runtime.insert("firstUserMessage".to_string(), json!(message));
    }
    let mut params = json!({
        "agentId": request.agent_id,
        "projectId": project_id,
        "runtimeSettings": Value::Object(runtime),
    });
    if let Some(title) = request
        .title
        .as_deref()
        .filter(|title| !title.trim().is_empty())
    {
        params["title"] = json!(title);
    }
    let created = rpc::call_gxserver_rpc("/api/createAgentSession", &params, &flags)
        .map_err(|error| format!("The box's gxserver did not create the session: {error}"))?;
    let session_id = text(&created, "/session/sessionId")
        .ok_or("The box's gxserver created no session.")?
        .to_string();
    rpc::call_gxserver_rpc(
        "/api/startSessionProvider",
        &json!({ "projectId": project_id, "sessionId": session_id }),
        &flags,
    )
    .map_err(|error| format!("The session was created but did not start: {error}"))?;
    // `firstUserMessage` only names the session; the message itself goes through the session's
    // chat queue as its startup send, which holds it until the agent's input is up (the way
    // `ghostex board start-work` delivers a bead's prompt).
    if let Some(message) = request
        .first_message
        .as_deref()
        .filter(|message| !message.trim().is_empty())
    {
        rpc::call_gxserver_rpc(
            "/api/queueSessionChatPrompt",
            &json!({
                "projectId": project_id,
                "sessionId": session_id,
                "text": message,
                "startupSend": true,
            }),
            &flags,
        )
        .map_err(|error| {
            format!("The session started but its first message was not queued: {error}")
        })?;
    }
    let agent_session_id = text(&created, "/session/runtimeSettings/agentSessionId")
        .map(str::to_string)
        .or_else(|| wait_for_agent_session_id(&project_id, &session_id, &flags));
    Ok(BoxPrepareSessionResult {
        project_id,
        session_id,
        cwd: cwd_text,
        branch,
        agent_session_id,
    })
}
