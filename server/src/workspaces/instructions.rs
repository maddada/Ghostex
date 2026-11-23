//! Per-workspace agent instructions backed by one Markdown file per workspace under Ghostex's
//! config directory (`<config_dir>/workspace-instructions/<workspace_id>.md`).
//!
//! Never writes or edits the user's own global (`~/.claude/CLAUDE.md`, `~/.agents/main.md`) or
//! repository (`CLAUDE.md`, `AGENTS.md`) files. Delivered automatically by gxserver on every
//! session start and resume (launch flags for Claude/Codex/Droid/Pi/Grok/OMP/Copilot and
//! SessionStart hook context for Antigravity/ZCode/Kiro/CodeBuddy/Qoder).

use std::fs;
use std::path::PathBuf;

use crate::domain::DomainStateError;
use crate::paths::GxserverPaths;

/// Upper bound on workspace instructions so launch-flag agents (including Codex's `-c` arg)
/// stay well within shell command limits.
pub const MAX_WORKSPACE_INSTRUCTIONS_CHARS: usize = 32_768;

/// Folder holding one `.md` file per workspace.
pub fn workspace_instructions_dir(paths: &GxserverPaths) -> PathBuf {
    paths.app_config_dir.join("workspace-instructions")
}

/// Safe filename stem for a workspace id: only ASCII letters, digits, `-`, `_`.
pub fn safe_workspace_file_stem(workspace_id: &str) -> String {
    let trimmed = workspace_id.trim();
    let cleaned: String = trimmed
        .chars()
        .map(|ch| {
            if ch.is_ascii_alphanumeric() || matches!(ch, '-' | '_') {
                ch
            } else {
                '_'
            }
        })
        .collect();
    if cleaned.is_empty() || cleaned.chars().all(|ch| ch == '_') {
        "workspace".to_string()
    } else {
        cleaned
    }
}

/// The Markdown file backing `workspace_id`'s agent instructions.
pub fn workspace_instructions_file(paths: &GxserverPaths, workspace_id: &str) -> PathBuf {
    workspace_instructions_dir(paths).join(format!("{}.md", safe_workspace_file_stem(workspace_id)))
}

/// Reads `workspace_id`'s instructions directly from disk so edits made outside Settings show up.
/// Returns `(content, file_path, exists)`.
pub fn read_workspace_instructions_file(
    paths: &GxserverPaths,
    workspace_id: &str,
) -> Result<(String, PathBuf, bool), DomainStateError> {
    let path = workspace_instructions_file(paths, workspace_id);
    match fs::read_to_string(&path) {
        Ok(content) => Ok((content, path, true)),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
            Ok((String::new(), path, false))
        }
        Err(error) => Err(DomainStateError {
            code: "internalError",
            message: format!("Could not read workspace instructions file: {error}"),
        }),
    }
}

/// Writes `content` to `workspace_id`'s instructions file.
///
/// When `content` is blank and `create_file` is false, removes the file if it exists so an empty
/// workspace carries no stale file; when `create_file` is true (the "Open file" action), ensures
/// the file exists on disk so an external editor can open it.
pub fn write_workspace_instructions_file(
    paths: &GxserverPaths,
    workspace_id: &str,
    content: &str,
    create_file: bool,
) -> Result<(String, PathBuf, bool), DomainStateError> {
    if content.chars().count() > MAX_WORKSPACE_INSTRUCTIONS_CHARS {
        return Err(DomainStateError::bad_request(format!(
            "Workspace instructions can be at most {MAX_WORKSPACE_INSTRUCTIONS_CHARS} characters."
        )));
    }
    let path = workspace_instructions_file(paths, workspace_id);
    if content.trim().is_empty() && !create_file {
        match fs::remove_file(&path) {
            Ok(()) => {}
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
            Err(error) => {
                return Err(DomainStateError {
                    code: "internalError",
                    message: format!("Could not clear workspace instructions file: {error}"),
                });
            }
        }
        return Ok((String::new(), path, false));
    }
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).map_err(|error| DomainStateError {
            code: "internalError",
            message: format!("Could not prepare workspace instructions folder: {error}"),
        })?;
    }
    if create_file && content.is_empty() && path.is_file() {
        let existing = fs::read_to_string(&path).unwrap_or_default();
        return Ok((existing, path, true));
    }
    fs::write(&path, content).map_err(|error| DomainStateError {
        code: "internalError",
        message: format!("Could not save workspace instructions file: {error}"),
    })?;
    Ok((content.to_string(), path, true))
}

/// Removes a deleted workspace's instructions file, if one exists.
pub fn delete_workspace_instructions_file(paths: &GxserverPaths, workspace_id: &str) {
    let path = workspace_instructions_file(paths, workspace_id);
    let _ = fs::remove_file(path);
}

/// Formats non-empty workspace instructions with an explicit precedence statement so every agent
/// combines them predictably with global rules and per-repo CLAUDE.md / AGENTS.md files.
pub fn format_workspace_instructions_note(workspace_name: &str, content: &str) -> Option<String> {
    let trimmed = content.trim();
    if trimmed.is_empty() {
        return None;
    }
    let label = if workspace_name.trim().is_empty() {
        "Workspace"
    } else {
        workspace_name.trim()
    };
    Some(format!(
        "Workspace instructions ({label}):\n\
         These instructions come from Ghostex for every project in the {label} workspace.\n\
         Instruction order: Global rules -> Workspace instructions ({label}) -> Project files (CLAUDE.md / AGENTS.md).\n\
         Workspace instructions add to global rules and override global rules when they disagree; project files in the repository still govern repo-specific rules.\n\n\
         {trimmed}"
    ))
}

/// Reads and formats `workspace_id`'s instructions for injection into an agent launch or
/// SessionStart hook. Returns `None` when the file is missing, unreadable, or blank.
pub fn read_workspace_instructions_note(
    paths: &GxserverPaths,
    workspace_id: &str,
    workspace_name: &str,
) -> Option<String> {
    let path = workspace_instructions_file(paths, workspace_id);
    let content = fs::read_to_string(path).ok()?;
    let content = if content.chars().count() > MAX_WORKSPACE_INSTRUCTIONS_CHARS {
        content
            .chars()
            .take(MAX_WORKSPACE_INSTRUCTIONS_CHARS)
            .collect::<String>()
    } else {
        content
    };
    format_workspace_instructions_note(workspace_name, &content)
}
