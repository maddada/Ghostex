/*
CDXC:SessionChatDetectedOptions 2026-09-24 DECISION:
User: Cursor chats show what Cursor hands its statusline command (context use,
output tokens, version, Max Mode, auto-run, worktree, folder) plus the branch,
lines changed and pull request Ghostex works out for the session's checkout, so
the status line and More details are more than the model and effort the chat
box already shows. The payload is the one the Ghostex statusline script stored
for this Cursor session id (`agent_hooks/statusline.rs`); the git values come
from the sidebar's per-checkout cache when it has the folder, else from the same
probe run here at most once per `GIT_TTL`.
*/

use std::{
    collections::HashMap,
    path::Path,
    sync::{Mutex, OnceLock},
    time::{Duration, Instant},
};

use serde_json::{json, Map, Value};

use crate::agent_hooks::statusline::{
    cursor_statusline_payload_path, read_statusline_payload_file,
};
use crate::domain::DomainRepository;
use crate::session_chat_options::{SessionChatContextUsage, SessionChatDetectedSelection};
use crate::session_git_status::{
    cached_session_git_status, effective_session_git_cwd, probe_git_status_with,
    run_git_probe_command, SessionGitStatus,
};

const GIT_TTL: Duration = Duration::from_secs(30);

/// Cursor's stored payload for this Ghostex session plus the checkout's git
/// state, as a detection layer: the context usage and the `cursorStatus` rows.
/// The git rows need no payload, so a chat shows them before Cursor has drawn
/// its first statusline.
pub fn read_cursor_statusline_selection(
    repository: &DomainRepository<'_>,
    hook_state_directory: &Path,
    project_id: &str,
    session_id: &str,
) -> Option<SessionChatDetectedSelection> {
    let session = repository.get_session(project_id, session_id).ok()??;
    let project = repository.get_project(project_id).ok().flatten();
    let git_cwd = effective_session_git_cwd(&session, project.as_ref());
    let git = git_cwd.as_deref().and_then(session_git_status);
    let empty = Map::new();
    let stored = cursor_statusline_payload_path(hook_state_directory, project_id, session_id)
        .and_then(|path| read_statusline_payload_file(&path));
    let payload = stored.as_ref().map_or(&empty, |stored| &stored.payload);
    let window = payload.get("context_window");
    let number = |key: &str| window.and_then(|window| window.get(key));
    let context_usage = SessionChatContextUsage {
        used_percentage: number("used_percentage")
            .and_then(Value::as_f64)
            .map(|value| value.round().clamp(0.0, 100.0) as u32),
        used_tokens: number("total_input_tokens").and_then(Value::as_u64),
        window_size: number("context_window_size").and_then(Value::as_u64),
    };
    let repo = git
        .as_ref()
        .and(git_cwd.as_deref())
        .and_then(|cwd| Path::new(cwd).file_name())
        .and_then(|name| name.to_str());
    let status = cursor_status_value(payload, git.as_ref(), repo);
    let context_usage = (!context_usage.is_empty()).then_some(context_usage);
    (context_usage.is_some() || status.is_some()).then(|| SessionChatDetectedSelection {
        context_usage,
        cursor_status: status,
        ..SessionChatDetectedSelection::default()
    })
}

/// The payload's values the chat can show, camelCase, each dropped when absent.
fn cursor_status_value(
    payload: &Map<String, Value>,
    git: Option<&SessionGitStatus>,
    repo: Option<&str>,
) -> Option<Value> {
    let text = |path: &[&str]| {
        let mut current = payload.get(*path.first()?)?;
        for key in &path[1..] {
            current = current.get(*key)?;
        }
        current
            .as_str()
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .map(|value| json!(value))
    };
    let mut status = Map::new();
    let mut put = |key: &str, value: Option<Value>| {
        if let Some(value) = value {
            status.insert(key.to_string(), value);
        }
    };
    put("version", text(&["version"]));
    put(
        "currentDir",
        text(&["workspace", "current_dir"]).or_else(|| text(&["cwd"])),
    );
    put("worktree", text(&["worktree", "name"]));
    put("outputStyle", text(&["output_style", "name"]));
    put(
        "totalOutputTokens",
        payload
            .get("context_window")
            .and_then(|window| window.get("total_output_tokens"))
            .and_then(Value::as_f64)
            .map(|tokens| json!(tokens)),
    );
    put(
        "autorun",
        payload
            .get("autorun")
            .and_then(Value::as_bool)
            .map(Value::Bool),
    );
    // Cursor only writes `max_mode` while it is on, so any payload says off without it.
    put(
        "maxMode",
        payload.contains_key("model").then(|| {
            Value::Bool(
                payload
                    .get("model")
                    .and_then(|model| model.get("max_mode"))
                    .and_then(Value::as_bool)
                    == Some(true),
            )
        }),
    );
    put("repo", repo.map(|repo| json!(repo)));
    if let Some(git) = git {
        put("branch", git.branch.clone().map(Value::String));
        put("linesAdded", Some(json!(git.additions)));
        put("linesRemoved", Some(json!(git.deletions)));
        if let Some(pull_request) = &git.pull_request {
            put("prNumber", Some(json!(pull_request.number)));
            put("prState", Some(json!(pull_request.state.as_wire())));
        }
    }
    (!status.is_empty()).then_some(Value::Object(status))
}

/// The checkout's git state: the sidebar's cache when it has the folder,
/// else a probe run here at most once per `GIT_TTL` per folder.
fn session_git_status(cwd: &str) -> Option<SessionGitStatus> {
    if let Some(status) = cached_session_git_status(cwd) {
        return Some(status);
    }
    static PROBES: OnceLock<Mutex<HashMap<String, (Instant, Option<SessionGitStatus>)>>> =
        OnceLock::new();
    let probes = PROBES.get_or_init(Default::default);
    if let Some((probed_at, status)) = probes.lock().ok()?.get(cwd) {
        if probed_at.elapsed() < GIT_TTL {
            return status.clone();
        }
    }
    let status = probe_git_status_with(&|args| run_git_probe_command(cwd, args)).map(|probe| {
        SessionGitStatus {
            branch: probe.branch,
            additions: probe.additions,
            deletions: probe.deletions,
            pull_request: None,
            updated_at: chrono::Utc::now().to_rfc3339(),
        }
    });
    probes
        .lock()
        .ok()?
        .insert(cwd.to_string(), (Instant::now(), status.clone()));
    status
}
