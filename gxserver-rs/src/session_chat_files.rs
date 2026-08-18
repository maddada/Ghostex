use std::{
    collections::HashSet,
    fs,
    path::{Path, PathBuf},
};

use serde_json::{json, Value};

use crate::session_git_status::run_git_probe_command;

/*
CDXC:SessionChatFileMentions 2026-08-18:
The chat composer's "@" picker lists the session project's files. Like chat
skills, the walk happens on the session's machine and the client never submits
a scan path: it names a session, gxserver resolves that session's project, and
answers with project-relative paths the agent can resolve against its own
working directory.

Git owns the list when the project is a repository, so .gitignore decides what
is offered instead of a hand-maintained deny list. Non-repository projects fall
back to a bounded walk with the same ignore names the skill discovery uses.
*/

/** Upper bound on returned paths; the client filters the list in the picker. */
const SESSION_CHAT_FILE_LIMIT: usize = 12_000;
/** Directory depth for the non-git fallback walk. */
const SESSION_CHAT_FILE_MAX_DEPTH: usize = 12;

pub fn read_session_chat_files(project_path: Option<&Path>) -> Value {
    let root = project_path.filter(|path| path.is_absolute() && path.is_dir());
    let (files, truncated) = match root {
        Some(root) => collect_project_files(root),
        None => (Vec::new(), false),
    };

    json!({
        "files": files,
        "generatedAt": chrono::Utc::now().to_rfc3339(),
        "rootPath": root.map(|path| path.to_string_lossy().to_string()).unwrap_or_default(),
        "truncated": truncated,
    })
}

fn collect_project_files(root: &Path) -> (Vec<String>, bool) {
    if let Some(listed) = git_tracked_files(root) {
        return listed;
    }
    walk_project_files(root)
}

/// Tracked plus untracked-but-not-ignored paths, exactly what an agent can open.
fn git_tracked_files(root: &Path) -> Option<(Vec<String>, bool)> {
    if !root.join(".git").exists() {
        return None;
    }
    let output = run_git_probe_command(
        &root.to_string_lossy(),
        &["ls-files", "--cached", "--others", "--exclude-standard"],
    )?;
    let mut seen = HashSet::new();
    let mut files = Vec::new();
    let mut truncated = false;
    for line in output.lines() {
        let path = line.trim();
        if path.is_empty() {
            continue;
        }
        if files.len() >= SESSION_CHAT_FILE_LIMIT {
            truncated = true;
            break;
        }
        if seen.insert(path.to_string()) {
            files.push(path.to_string());
        }
    }
    files.sort();
    Some((files, truncated))
}

fn walk_project_files(root: &Path) -> (Vec<String>, bool) {
    let mut files = Vec::new();
    let mut truncated = false;
    let mut pending = vec![(root.to_path_buf(), 0usize)];
    while let Some((directory, depth)) = pending.pop() {
        let Ok(entries) = fs::read_dir(&directory) else {
            continue;
        };
        for entry in entries.filter_map(Result::ok) {
            let name = entry.file_name();
            let name = name.to_string_lossy().to_string();
            if is_ignored_entry(&name) {
                continue;
            }
            let Ok(file_type) = entry.file_type() else {
                continue;
            };
            if file_type.is_dir() {
                if depth < SESSION_CHAT_FILE_MAX_DEPTH {
                    pending.push((entry.path(), depth + 1));
                }
                continue;
            }
            if !file_type.is_file() {
                continue;
            }
            if files.len() >= SESSION_CHAT_FILE_LIMIT {
                truncated = true;
                break;
            }
            if let Some(relative) = relative_path(root, &entry.path()) {
                files.push(relative);
            }
        }
        if truncated {
            break;
        }
    }
    files.sort();
    (files, truncated)
}

/// Always forward-slash separated so a Windows project reads like the mention.
fn relative_path(root: &Path, path: &Path) -> Option<String> {
    let relative: PathBuf = path.strip_prefix(root).ok()?.to_path_buf();
    let joined = relative
        .components()
        .map(|component| component.as_os_str().to_string_lossy().to_string())
        .collect::<Vec<_>>()
        .join("/");
    (!joined.is_empty()).then_some(joined)
}

fn is_ignored_entry(name: &str) -> bool {
    name.starts_with('.')
        || matches!(
            name,
            "node_modules"
                | "vendor"
                | "dist"
                | "build"
                | "out"
                | "target"
                | "coverage"
                | "__pycache__"
                | "tmp"
                | "DerivedData"
        )
}
