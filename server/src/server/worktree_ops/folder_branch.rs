use super::*;

/*
CDXC:Worktrees 2026-11-16 DECISION:
User (Q3520 to Q3526, from Brewed Bytes on Discord: "Select any branch from folder/machine
selector"): the new chat bar's last part for This folder is a branch menu, not only a label. It lists
Recent (the branches this folder was on lately, from git's own checkout history), Local and Remote
(remote branches with no local copy yet), with a search that also offers to create the typed name
from the current branch. A pick checks the branch out in the project's own folder right away (Q3521),
so the bar always says the branch the folder is really on. Before it does:
- another session of the project that is working right now blocks the switch, because its files would
  change mid-turn; the bar offers a new worktree from that branch instead (Q3523);
- uncommitted changes ask first: switch and take them along (git carries them, or refuses on a clash
  with its own reason), start a new worktree from that branch, or cancel (Q3522).
Idle sessions of the project are not asked: they find the new branch on their next turn.
SEE-ALSO: packages/gx-chat-core/src/menus/draft_worktree.rs (`folder_branch_menu`, the prompt),
apps/desktop/src/app/native_chat/draft_project_picker.rs, apps/mobile/app/src/chat/native/composer/WorktreeTray.tsx.
*/

/// How many branches the Recent section lists, the current one included.
const RECENT_LIMIT: usize = 5;
/// How far back the checkout history is read.
const REFLOG_DEPTH: &str = "400";

/// "2h ago", "yesterday", "3d ago" for a time `seconds` before now.
fn ago(seconds: i64) -> String {
    let minutes = seconds / 60;
    let hours = minutes / 60;
    let days = hours / 24;
    if minutes < 1 {
        "just now".to_string()
    } else if hours < 1 {
        format!("{minutes}m ago")
    } else if days < 1 {
        format!("{hours}h ago")
    } else if days < 2 {
        "yesterday".to_string()
    } else {
        format!("{days}d ago")
    }
}

/// The branches named by `git reflog`'s checkout lines (`HEAD@{<unix>}\tcheckout: moving from A to
/// B`), newest first, each with when the folder last moved onto it (the source of the newest move
/// counts at that move's time too, so the branch just left is listed).
pub(crate) fn parse_checkout_history(stdout: &str) -> Vec<(String, i64)> {
    let mut seen: Vec<(String, i64)> = Vec::new();
    let mut push = |name: &str, at: i64| {
        let name = name.trim();
        if !name.is_empty() && !seen.iter().any(|(known, _)| known == name) {
            seen.push((name.to_string(), at));
        }
    };
    for line in stdout.lines() {
        let Some((selector, subject)) = line.split_once('\t') else {
            continue;
        };
        let Some(moving) = subject.strip_prefix("checkout: moving from ") else {
            continue;
        };
        let Some((from, to)) = moving.rsplit_once(" to ") else {
            continue;
        };
        let at = selector
            .rsplit_once('{')
            .and_then(|(_, rest)| rest.strip_suffix('}'))
            .and_then(|stamp| stamp.parse::<i64>().ok())
            .unwrap_or(0);
        push(to, at);
        push(from, at);
    }
    seen
}

/// `[{name, ago}]`: the local branches this folder was on lately, newest first, the current one
/// included (its `ago` is empty). Read for the tray's Recent section.
pub(crate) fn recent_branches(path: &str, branches: &Value, current: Option<&str>) -> Value {
    let local: Vec<&str> = branches
        .as_array()
        .into_iter()
        .flatten()
        .filter(|branch| branch.get("remote").and_then(Value::as_bool) != Some(true))
        .filter_map(|branch| branch.get("name").and_then(Value::as_str))
        .collect();
    let history = crate::worktree_sessions::run_worktree_git(
        path,
        &[
            "reflog",
            "show",
            "--date=unix",
            "--format=%gd%x09%gs",
            "-n",
            REFLOG_DEPTH,
            "HEAD",
        ],
        crate::worktree_sessions::WORKTREE_GIT_COMMAND_TIMEOUT,
    )
    .unwrap_or_default();
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|elapsed| elapsed.as_secs() as i64)
        .unwrap_or(0);
    let mut rows = Vec::new();
    if let Some(current) = current.filter(|current| local.contains(current)) {
        rows.push(json!({ "name": current, "ago": "" }));
    }
    for (name, at) in parse_checkout_history(&history) {
        if rows.len() >= RECENT_LIMIT {
            break;
        }
        if Some(name.as_str()) == current || !local.contains(&name.as_str()) {
            continue;
        }
        let when = if at > 0 { ago(now - at) } else { String::new() };
        rows.push(json!({ "name": name, "ago": when }));
    }
    Value::Array(rows)
}

/// Uncommitted changes to tracked files in `path` (new untracked files ride along harmlessly and
/// are not counted).
fn changed_files(path: &str) -> usize {
    crate::worktree_sessions::run_worktree_git(
        path,
        &["status", "--porcelain", "--untracked-files=no"],
        crate::worktree_sessions::WORKTREE_GIT_COMMAND_TIMEOUT,
    )
    .map(|stdout| {
        stdout
            .lines()
            .filter(|line| !line.trim().is_empty())
            .count()
    })
    .unwrap_or(0)
}

/// The title of another session of the project that is working right now, if any.
fn working_session(state: &AppState, project_id: &str, session_id: &str) -> Option<String> {
    let db = open_gxserver_database(&state.paths).ok()?;
    let repository = DomainRepository::new(&db, state.metadata.server_id.as_str());
    let sessions = repository.list_sessions(Some(project_id)).ok()?;
    let now = now_iso();
    sessions
        .iter()
        .filter(|session| session.get("sessionId").and_then(Value::as_str) != Some(session_id))
        .find(|session| crate::presentation::presentation_activity(session, &now) == "working")
        .map(|session| {
            session
                .get("title")
                .and_then(Value::as_str)
                .map(str::trim)
                .filter(|title| !title.is_empty())
                .unwrap_or("Another session")
                .to_string()
        })
}

/// What `folderBranch` asks: `{name, remote?, create?, force?}`.
struct FolderSwitch {
    name: String,
    remote: bool,
    create: bool,
    force: bool,
}

impl FolderSwitch {
    fn read(value: &Value) -> std::result::Result<Self, DomainStateError> {
        let flag = |key: &str| value.get(key).and_then(Value::as_bool) == Some(true);
        let name = normalize_project_worktree_git_ref(value.get("name"), "folderBranch.name")?;
        Ok(Self {
            name,
            remote: flag("remote"),
            create: flag("create"),
            force: flag("force"),
        })
    }

    /// The local branch a pick lands on: a remote `origin/x` checks out as `x`, which git makes
    /// track it.
    fn local_name(&self) -> String {
        if self.remote {
            self.name
                .split_once('/')
                .map_or(self.name.clone(), |(_, rest)| rest.to_string())
        } else {
            self.name.clone()
        }
    }
}

/// `folderBranch`: switches the project's own folder to a branch, or says why not yet. The answer
/// is the tray's full answer with `folderSwitch`: `{state: "done", branch}`, `{state: "blocked",
/// branch, session}`, `{state: "confirm", branch, changes}` or `{state: "failed", branch, message}`.
pub(crate) async fn switch_folder_branch(
    state: &AppState,
    project: &Value,
    session: &Value,
    request: &Value,
) -> std::result::Result<Value, ProjectWorktreeOperationError> {
    let switch = FolderSwitch::read(request)?;
    let project_id = value_text(project, "projectId")?;
    let session_id = value_text(session, "sessionId")?;
    let mut context_params = Map::new();
    context_params.insert("projectId".to_string(), json!(project_id));
    let context = resolve_project_worktree_operation_context(state, &context_params)?;
    let path = context.source_path.clone();
    let outcome = if let Some(title) = working_session(state, &project_id, &session_id) {
        json!({ "state": "blocked", "branch": switch.name, "session": title })
    } else {
        let changes = if switch.create || switch.force {
            0
        } else {
            let path = path.clone();
            tokio::task::spawn_blocking(move || changed_files(&path))
                .await
                .unwrap_or(0)
        };
        if changes > 0 {
            json!({ "state": "confirm", "branch": switch.name, "changes": changes })
        } else {
            let mut params = Map::new();
            params.insert("branch".to_string(), json!(switch.local_name()));
            let action = if switch.create {
                "checkoutNewBranch"
            } else {
                "checkout"
            };
            let result = run_project_git_action(&context.projects, action, &path, params).await?;
            if exit_code(&result) == 0 {
                json!({ "state": "done", "branch": switch.local_name() })
            } else {
                json!({
                    "state": "failed",
                    "branch": switch.name,
                    "message": operation_failure_message(&result, "Could not switch the branch."),
                })
            }
        }
    };
    let mut answer = super::read_tray(state, project, session).await?;
    if let Some(object) = answer.as_object_mut() {
        object.insert("folderSwitch".to_string(), outcome);
    }
    Ok(answer)
}
