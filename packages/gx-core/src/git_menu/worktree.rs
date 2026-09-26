//! The worktree dialogs' pure parts: the drafts Delete Worktree and Rename Worktree open with, and
//! the words their failures show. Ported from the old runtime's `helpers/worktrees.ts` and
//! `worktrees.ts`.

use serde_json::{json, Map, Value};

/// `normalizeGpuiWorktreeDeleteBranchName`: the checked-out branch, else the one the worktree
/// row names; a detached head is no branch.
pub fn worktree_branch_name(current: Option<&str>, fallback: Option<&str>) -> Option<String> {
    [current, fallback]
        .into_iter()
        .flatten()
        .find_map(|candidate| {
            let branch = candidate.trim();
            (!branch.is_empty() && branch != "HEAD" && branch != "detached")
                .then(|| branch.to_string())
        })
}

/// `resolveGpuiWorktreeDeleteBranchMetadata`, given whether `origin/<branch>` exists.
pub fn worktree_branch_metadata(
    branch: Option<&str>,
    remote_branch_exists: bool,
) -> Map<String, Value> {
    let remote_name = "origin";
    let mut metadata = Map::new();
    match branch {
        None => {
            metadata.insert("branch".into(), Value::Null);
            metadata.insert("canDeleteLocalBranch".into(), json!(false));
            metadata.insert(
                "remoteBranchDisabledReason".into(),
                json!("No local branch is checked out for this worktree."),
            );
            metadata.insert("remoteBranchExists".into(), json!(false));
            metadata.insert("remoteName".into(), json!(remote_name));
        }
        Some(branch) => {
            metadata.insert("branch".into(), json!(branch));
            metadata.insert("canDeleteLocalBranch".into(), json!(true));
            metadata.insert("localBranchName".into(), json!(branch));
            if !remote_branch_exists {
                metadata.insert(
                    "remoteBranchDisabledReason".into(),
                    json!(format!("No {remote_name}/{branch} remote branch exists.")),
                );
            }
            metadata.insert("remoteBranchExists".into(), json!(remote_branch_exists));
            metadata.insert("remoteBranchName".into(), json!(branch));
            metadata.insert("remoteName".into(), json!(remote_name));
        }
    }
    metadata
}

/// `hasGpuiGitShortStatusChanges`: `git status --short --branch` lists a change.
pub fn short_status_has_changes(stdout: &str) -> bool {
    stdout.split('\n').any(|line| {
        let trimmed = line.trim();
        !trimmed.is_empty() && !trimmed.starts_with("##")
    })
}

/// `gpuiWorktreeFolderSuffix`: the folder's own name with the `<ParentFolder>-` prefix stripped,
/// because that prefix is re-applied on submit.
pub fn worktree_folder_suffix(folder_name: &str, parent_folder_name: &str) -> String {
    let prefix = format!("{parent_folder_name}-");
    if !parent_folder_name.is_empty() {
        if let Some(rest) = folder_name.strip_prefix(&prefix) {
            return rest.to_string();
        }
    }
    folder_name.to_string()
}

/// `isGpuiManagedWorktreeBranch`.
///
/// CDXC:Worktrees 2026-08-09-18:40:
/// The branch checkbox defaults on only for a branch gxserver minted or manages (`ghostex/<8hex>`
/// or `ghostex/<slug>`, mirroring `is_worktree_temp_branch` and `is_managed_worktree_branch` in
/// `server/src/worktree_sessions.rs`). A branch the user named is theirs and stays put unless they
/// say otherwise.
pub fn is_managed_worktree_branch(branch: Option<&str>) -> bool {
    let Some(slug) = branch.and_then(|branch| branch.strip_prefix("ghostex/")) else {
        return false;
    };
    !slug.is_empty()
        && slug != "automation"
        && !slug.starts_with("automation/")
        && slug
            .chars()
            .all(|ch| ch.is_ascii_lowercase() || ch.is_ascii_digit() || ch == '-')
}

/// `gpuiProjectNameFromPath`: the last path segment.
pub fn project_name_from_path(path: &str) -> String {
    let normalized = if looks_like_windows_root(path) {
        path.replace('\\', "/")
    } else {
        path.to_string()
    };
    normalized
        .split('/')
        .filter(|segment| !segment.is_empty())
        .next_back()
        .unwrap_or("Project")
        .to_string()
}

fn looks_like_windows_root(path: &str) -> bool {
    let bytes = path.as_bytes();
    (bytes.len() >= 3
        && bytes[0].is_ascii_alphabetic()
        && bytes[1] == b':'
        && (bytes[2] == b'\\' || bytes[2] == b'/'))
        || path.starts_with("\\\\")
        || path.starts_with("//")
}

/// `normalizeGpuiProjectPath`: trimmed, without trailing slashes.
pub fn normalize_project_path(value: Option<&str>) -> Option<String> {
    let trimmed = value?.trim().trim_end_matches('/');
    (!trimmed.is_empty()).then(|| trimmed.to_string())
}

/// `gpuiWorktreeSlugFromPrompt`: the name hint gxserver derives the branch and folder from.
pub fn worktree_slug_from_prompt(prompt: &str) -> String {
    let lowered: String = prompt
        .trim()
        .to_lowercase()
        .chars()
        .filter(|ch| !matches!(ch, '`' | '\'' | '"'))
        .collect();
    let mut dashed = String::new();
    let mut last_dash = false;
    for ch in lowered.chars() {
        if ch.is_ascii_lowercase() || ch.is_ascii_digit() {
            dashed.push(ch);
            last_dash = false;
        } else if !last_dash {
            dashed.push('-');
            last_dash = true;
        }
    }
    let words: Vec<&str> = dashed
        .trim_matches('-')
        .split('-')
        .filter(|word| !word.is_empty())
        .take(6)
        .collect();
    let joined = words.join("-");
    let base = if joined.is_empty() {
        "worktree".to_string()
    } else {
        joined
    };
    let clipped: String = base.chars().take(48).collect();
    let clipped = clipped.trim_end_matches('-').to_string();
    if clipped.is_empty() {
        "worktree".to_string()
    } else {
        clipped
    }
}

/// `gpuiWorktreeUserVisibleErrorMessage`: a short single-line message that names no path, else the
/// generic sentence.
pub fn worktree_user_visible_error(message: &str) -> String {
    let message = message.trim();
    if !message.is_empty()
        && !message.contains('/')
        && !message.contains('\\')
        && !message.contains('\n')
        && message.chars().count() <= 160
    {
        return message.to_string();
    }
    "The gxserver worktree operation failed.".to_string()
}

/// `gpuiWorktreeRenameUserVisibleErrorMessage`.
///
/// CDXC:Worktrees 2026-08-09-18:40:
/// gxserver's rename errors are bounded strings by contract (never git stderr), so this keeps a
/// slash (`Branch "feat/x" already exists.` is exactly the sentence the user needs) and guards the
/// shape instead. A daemon older than the feature cannot route the endpoint and names the path
/// back, which always means the daemon is older than the app.
pub fn worktree_rename_user_visible_error(message: &str) -> String {
    let message = message.trim();
    if message.contains("/api/renameWorktreeProject") {
        return "This Ghostex build's background service is out of date. Quit Ghostex fully, reopen it, and try again.".to_string();
    }
    if !message.is_empty()
        && !message.contains('\\')
        && !message.contains('\n')
        && message.chars().count() <= 200
    {
        return message.to_string();
    }
    "The gxserver worktree rename failed.".to_string()
}

/// `gpuiWorktreeListErrorText`: what the Add Worktree dialog says when its listing failed.
pub fn worktree_list_error_text(message: &str, folder: Option<&str>) -> String {
    let message = message.trim();
    if message.is_empty() {
        return "Could not load gxserver worktrees.".to_string();
    }
    if message.starts_with("Git could not read ") {
        return message.to_string();
    }
    let where_ = normalize_project_path(folder).unwrap_or_else(|| "the project folder".to_string());
    format!("Could not load worktrees for {where_}: {message}")
}

/// The toast a gxserver worktree-delete warning shows (`postGxserverWorktreeDeleteWarnings`).
pub fn worktree_delete_warning_title(kind: &str) -> Option<&'static str> {
    match kind {
        "localBranchDeleteFailed" | "localBranchNotResolved" => {
            Some("Worktree removed, but local branch cleanup needs attention")
        }
        "remoteBranchDeleteFailed" | "remoteBranchNotResolved" => {
            Some("Worktree removed, but remote branch cleanup needs attention")
        }
        "pruneFailed" => Some("Worktree removed, but stale metadata cleanup needs attention"),
        _ => None,
    }
}

/// `normalizeGpuiWorktreeBaseBranches`: the base branch choices, first occurrence of each name.
pub fn normalize_worktree_base_branches(branches: Option<&Value>) -> Value {
    let mut seen: Vec<String> = Vec::new();
    let mut out = Vec::new();
    for branch in branches.and_then(Value::as_array).into_iter().flatten() {
        let Some(name) = branch
            .get("name")
            .and_then(Value::as_str)
            .map(str::trim)
            .filter(|name| !name.is_empty())
        else {
            continue;
        };
        if seen.iter().any(|known| known == name) {
            continue;
        }
        seen.push(name.to_string());
        out.push(json!({
            "current": branch.get("current").and_then(Value::as_bool) == Some(true),
            "name": name,
            "remote": branch.get("remote").and_then(Value::as_bool) == Some(true),
        }));
    }
    Value::Array(out)
}

/// `normalizeGpuiExistingWorktreeOptions`: the existing checkouts the Open Existing tab offers.
pub fn normalize_existing_worktree_options(worktrees: Option<&Value>) -> Vec<Value> {
    let text = |entry: &Value, key: &str| {
        entry
            .get(key)
            .and_then(Value::as_str)
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .map(str::to_string)
    };
    worktrees
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .filter(|entry| entry.is_object())
        .filter_map(|entry| {
            let path = normalize_project_path(entry.get("path").and_then(Value::as_str))?;
            let name = text(entry, "name").unwrap_or_else(|| project_name_from_path(&path));
            let worktree_key = text(entry, "worktreeKey")?;
            Some(json!({
                "branch": text(entry, "branch").unwrap_or_default(),
                "isCurrentProject": entry.get("isCurrentProject").and_then(Value::as_bool) == Some(true),
                "isRegistered": entry.get("isRegistered").and_then(Value::as_bool) == Some(true),
                "name": name,
                "path": path,
                "worktreeKey": worktree_key,
            }))
        })
        .collect()
}
