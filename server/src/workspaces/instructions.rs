//! Per-workspace agent instructions backed by one Markdown file per workspace under Ghostex's
//! config directory (`<config_dir>/workspace-instructions/<workspace_id>.md`).
//!
//! Never writes or edits the user's own global (`~/.claude/CLAUDE.md`, `~/.agents/main.md`) or
//! repository (`CLAUDE.md`, `AGENTS.md`) files. Delivered automatically by gxserver via generated
//! files for Claude, Droid, Pi, OMP, and Copilot CLI, and via SessionStart hook context for
//! Codex, Antigravity, ZCode, Kiro, CodeBuddy, and Qoder. Never placed inline on a shell command
//! line.

use std::fs::{self, File};
use std::io::Read;
use std::path::{Path, PathBuf};

use crate::domain::DomainStateError;
use crate::paths::GxserverPaths;

/// Upper bound in UTF-8 bytes on workspace instructions (32 KiB). Bounded on both read and write
/// so externally edited or huge files cannot exhaust memory or blow hook/launch budgets.
pub const MAX_WORKSPACE_INSTRUCTIONS_BYTES: usize = 32_768;

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

/// Strips unsafe ASCII control characters (everything `< 0x20` except `\n` and `\t`, plus `0x7F`)
/// and normalizes CRLF/CR line endings to LF.
pub fn sanitize_workspace_instructions(raw: &str) -> String {
    let normalized = raw.replace("\r\n", "\n").replace('\r', "\n");
    normalized
        .chars()
        .filter(|&ch| ch == '\n' || ch == '\t' || (ch >= ' ' && ch != '\x7f'))
        .collect()
}

/// Bounded read of at most `MAX_WORKSPACE_INSTRUCTIONS_BYTES` bytes from `path`.
/// Returns `Ok(None)` when the file does not exist, or `Ok(Some((sanitized_text, truncated)))`.
fn read_bounded_instructions_file(path: &Path) -> std::io::Result<Option<(String, bool)>> {
    let file = match File::open(path) {
        Ok(file) => file,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(None),
        Err(error) => return Err(error),
    };
    let mut buf = Vec::new();
    file.take((MAX_WORKSPACE_INSTRUCTIONS_BYTES + 1) as u64)
        .read_to_end(&mut buf)?;
    let truncated = buf.len() > MAX_WORKSPACE_INSTRUCTIONS_BYTES;
    if truncated {
        buf.truncate(MAX_WORKSPACE_INSTRUCTIONS_BYTES);
        // Drop any partial trailing UTF-8 sequence split at the byte boundary.
        while !buf.is_empty() && std::str::from_utf8(&buf).is_err() {
            let valid_up_to = std::str::from_utf8(&buf)
                .err()
                .map(|err| err.valid_up_to())
                .unwrap_or(0);
            if buf.len() - valid_up_to <= 3 {
                buf.truncate(valid_up_to);
                break;
            } else {
                // Invalid UTF-8 earlier in the file; fall through to lossy decoding.
                break;
            }
        }
    }
    let decoded = String::from_utf8_lossy(&buf);
    let cleaned = sanitize_workspace_instructions(&decoded);
    Ok(Some((cleaned, truncated)))
}

/// Reads `workspace_id`'s instructions directly from disk (bounded to
/// `MAX_WORKSPACE_INSTRUCTIONS_BYTES`) so edits made outside Settings show up safely.
/// Returns `(content, file_path, exists, truncated)`.
pub fn read_workspace_instructions_file(
    paths: &GxserverPaths,
    workspace_id: &str,
) -> Result<(String, PathBuf, bool, bool), DomainStateError> {
    let path = workspace_instructions_file(paths, workspace_id);
    match read_bounded_instructions_file(&path) {
        Ok(Some((content, truncated))) => Ok((content, path, true, truncated)),
        Ok(None) => Ok((String::new(), path, false, false)),
        Err(error) => Err(DomainStateError {
            code: "internalError",
            message: format!("Could not read workspace instructions file: {error}"),
        }),
    }
}

/// Writes `content` to `workspace_id`'s instructions file (at most
/// `MAX_WORKSPACE_INSTRUCTIONS_BYTES` UTF-8 bytes).
///
/// When `content` is blank, removes the file if it exists so an empty workspace carries no stale
/// file.
pub fn write_workspace_instructions_file(
    paths: &GxserverPaths,
    workspace_id: &str,
    content: &str,
) -> Result<(String, PathBuf, bool), DomainStateError> {
    let sanitized = sanitize_workspace_instructions(content);
    if sanitized.len() > MAX_WORKSPACE_INSTRUCTIONS_BYTES {
        return Err(DomainStateError::bad_request(
            "Workspace instructions can be at most 32 KB.",
        ));
    }
    let path = workspace_instructions_file(paths, workspace_id);
    if sanitized.trim().is_empty() {
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
    fs::write(&path, &sanitized).map_err(|error| DomainStateError {
        code: "internalError",
        message: format!("Could not save workspace instructions file: {error}"),
    })?;
    Ok((sanitized, path, true))
}

/// Removes a deleted workspace's instructions file and any generated launch-note files under
/// `<root_dir>/agent-note/`.
pub fn delete_workspace_instructions_file(paths: &GxserverPaths, workspace_id: &str) {
    let path = workspace_instructions_file(paths, workspace_id);
    let _ = fs::remove_file(path);
    let stem = safe_workspace_file_stem(workspace_id);
    let note_dir = paths.root_dir.join("agent-note");
    for prefix in ["workspace", "coordinator-workspace", "thread-workspace"] {
        let _ = fs::remove_file(note_dir.join(format!("{prefix}-{stem}.md")));
        let _ = fs::remove_dir_all(note_dir.join(format!("{prefix}-{stem}-copilot")));
    }
}

/// Formats non-empty workspace instructions with an explicit precedence statement so every agent
/// combines them predictably with global rules and per-repo CLAUDE.md / AGENTS.md files.
pub fn format_workspace_instructions_note(workspace_name: &str, content: &str) -> Option<String> {
    let sanitized = sanitize_workspace_instructions(content);
    let trimmed = sanitized.trim();
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

/// Reads (bounded to `MAX_WORKSPACE_INSTRUCTIONS_BYTES`) and formats `workspace_id`'s instructions
/// for injection into an agent launch file or SessionStart hook. Returns `None` when the file is
/// missing, unreadable, or blank.
pub fn read_workspace_instructions_note(
    paths: &GxserverPaths,
    workspace_id: &str,
    workspace_name: &str,
) -> Option<String> {
    let path = workspace_instructions_file(paths, workspace_id);
    let (content, _truncated) = read_bounded_instructions_file(&path).ok()??;
    format_workspace_instructions_note(workspace_name, &content)
}
