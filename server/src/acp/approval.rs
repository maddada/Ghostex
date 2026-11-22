//! Approval levels and client-side permission policy for ACP (Non-CLI) sessions.
//!
//! Four levels live on the chat box's existing Mode button:
//! - `ask`: ask before edits and commands
//! - `auto-edits`: auto-approve edit/delete/move inside the session folder and writable roots
//!   (symlink-resolved); ask for commands and anything else
//! - `auto`: use the agent's own advertised ACP automatic mode when available, else behave like `ask`
//! - `full-access`: auto-approve everything
//!
//! Read-only tools (`read`, `search`, `think`) are auto-approved when the agent offers a one-time
//! allow option and `rawInput` does not carry a shell/command field. Auto-approvals strictly choose
//! `allow_once` (never `allow_always`) so Ghostex never writes permanent rules into the agent's own
//! allowlist.

use std::collections::VecDeque;
use std::fs;
use std::path::{Component, Path, PathBuf};

use serde::{Deserialize, Serialize};
use serde_json::{Map, Value};

/// Session `runtimeSettings` key storing the remembered ACP approval level for this session.
pub const SESSION_ACP_APPROVAL_LEVEL_KEY: &str = "acpApprovalLevel";

const MAX_SYMLINK_HOPS: usize = 40;
const MAX_RAW_INPUT_DEPTH: usize = 16;

/// The four approval levels offered on the chat box's Mode button for ACP sessions.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum AcpApprovalLevel {
    #[default]
    #[serde(rename = "ask")]
    Ask,
    #[serde(rename = "auto-edits")]
    AutoEdits,
    #[serde(rename = "auto")]
    Auto,
    #[serde(rename = "full-access")]
    FullAccess,
}

impl AcpApprovalLevel {
    pub const ALL: [Self; 4] = [Self::Ask, Self::AutoEdits, Self::Auto, Self::FullAccess];

    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Ask => "ask",
            Self::AutoEdits => "auto-edits",
            Self::Auto => "auto",
            Self::FullAccess => "full-access",
        }
    }

    pub const fn label(self) -> &'static str {
        match self {
            Self::Ask => "Ask",
            Self::AutoEdits => "Auto-edits",
            Self::Auto => "Auto",
            Self::FullAccess => "Full access",
        }
    }

    pub fn parse(raw: &str) -> Option<Self> {
        match raw.trim().to_ascii_lowercase().as_str() {
            "ask" | "default" | "manual" | "supervised" | "approval-required"
            | "ask-for-approval" => Some(Self::Ask),
            "auto-edits" | "accept-edits" | "auto-accept-edits" | "auto_edits" | "auto_edit"
            | "accept_edits" => Some(Self::AutoEdits),
            "auto" => Some(Self::Auto),
            "full-access" | "full_access" | "bypass" | "yolo" | "accept-all" | "accept_all" => {
                Some(Self::FullAccess)
            }
            _ => None,
        }
    }

    pub const fn from_accept_all_default(enabled: bool) -> Self {
        if enabled {
            Self::FullAccess
        } else {
            Self::Ask
        }
    }

    /// Whether switching this ACP session in-place to the interactive CLI terminal should pass the
    /// CLI's accept-all flag.
    pub const fn implies_cli_accept_all(self) -> bool {
        matches!(self, Self::FullAccess)
    }

    pub const fn next_in_cycle(self) -> Self {
        match self {
            Self::Ask => Self::AutoEdits,
            Self::AutoEdits => Self::Auto,
            Self::Auto => Self::FullAccess,
            Self::FullAccess => Self::Ask,
        }
    }

    /// Whether a non-owner caller (e.g. an agent session token or peer computer) may transition an
    /// ACP session from `current` to `target`. Raising permissions always requires an owner device;
    /// keeping the same level, lowering to `Ask`, or stepping down from `FullAccess` is allowed.
    pub const fn may_transition_without_owner(current: Self, target: Self) -> bool {
        matches!(
            (current, target),
            (Self::Ask, Self::Ask)
                | (Self::AutoEdits, Self::Ask | Self::AutoEdits)
                | (Self::Auto, Self::Ask | Self::Auto)
                | (Self::FullAccess, _)
        )
    }
}

fn read_trimmed_str<'a>(map: &'a Map<String, Value>, key: &str) -> Option<&'a str> {
    map.get(key)
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty())
}

/// Read the remembered per-session ACP approval level from `session.runtimeSettings`, if set.
pub fn stored_session_approval_level(session: &Value) -> Option<AcpApprovalLevel> {
    session
        .pointer(&format!("/runtimeSettings/{SESSION_ACP_APPROVAL_LEVEL_KEY}"))
        .and_then(Value::as_str)
        .and_then(AcpApprovalLevel::parse)
}

/// Resolve the starting approval level for an ACP session:
/// 1. Remembered per-session level (`runtimeSettings.acpApprovalLevel`), when present.
/// 2. Per-agent / per-project / per-launch `acceptAllMode` (`"enabled"` -> `FullAccess`,
///    `"disabled"` -> `Ask`, `"inherit"` or absent -> global `agentAcceptAllEnabled`).
/// 3. Default stays `Ask`.
pub fn resolve_starting_level(
    runtime_settings: &Map<String, Value>,
    agent_config: &Map<String, Value>,
    launch_settings: &Map<String, Value>,
    global_settings: &Map<String, Value>,
) -> AcpApprovalLevel {
    if let Some(remembered) =
        read_trimmed_str(runtime_settings, SESSION_ACP_APPROVAL_LEVEL_KEY)
            .and_then(AcpApprovalLevel::parse)
    {
        return remembered;
    }
    let accept_all_effective = match read_trimmed_str(agent_config, "acceptAllMode")
        .or_else(|| read_trimmed_str(launch_settings, "acceptAllMode"))
    {
        Some("enabled" | "acceptAll") => true,
        Some("disabled") => false,
        _ => global_settings
            .get("agentAcceptAllEnabled")
            .and_then(Value::as_bool)
            .unwrap_or(false),
    };
    AcpApprovalLevel::from_accept_all_default(accept_all_effective)
}

/// True when an ACP native mode id represents an automatic, edit-auto-approving, or bypass/yolo
/// mode that must not be chosen as the safe fallback when leaving `Auto`.
pub fn is_auto_or_bypass_mode_id(raw: &str) -> bool {
    let lower = raw.trim().to_ascii_lowercase();
    if lower.is_empty() {
        return false;
    }
    matches!(lower.as_str(), "auto" | "auto_mode" | "auto-mode")
        || lower.contains("bypass")
        || lower.contains("yolo")
        || lower.contains("full")
        || lower.contains("accept-all")
        || lower.contains("accept_all")
        || lower.contains("allow-all")
        || lower.contains("allow_all")
        || lower.contains("auto-approve")
        || lower.contains("auto_approve")
        || lower.contains("unrestricted")
        || lower.contains("accept-edit")
        || lower.contains("accept_edit")
        || lower.contains("auto-edit")
        || lower.contains("auto_edit")
}

/// Find the agent's advertised automatic ACP mode (e.g. `id: "auto"`), excluding edit-only or
/// bypass/yolo modes. Returns `None` when the agent advertises no automatic mode.
pub fn find_advertised_auto_mode_id(available_modes: &Value) -> Option<String> {
    let modes = available_modes.as_array()?;
    let mut fallback_label_match = None;
    for item in modes {
        let Some(id) = item
            .get("id")
            .or_else(|| item.get("modeId"))
            .and_then(Value::as_str)
            .map(str::trim)
            .filter(|s| !s.is_empty())
        else {
            continue;
        };
        let id_lower = id.to_ascii_lowercase();
        if matches!(id_lower.as_str(), "auto" | "auto_mode" | "auto-mode") {
            return Some(id.to_string());
        }
        let label_lower = item
            .get("name")
            .or_else(|| item.get("label"))
            .and_then(Value::as_str)
            .map(|s| s.trim().to_ascii_lowercase())
            .unwrap_or_default();
        if fallback_label_match.is_none()
            && label_lower == "auto"
            && !id_lower.contains("edit")
            && !id_lower.contains("bypass")
            && !id_lower.contains("yolo")
            && !id_lower.contains("full")
        {
            fallback_label_match = Some(id.to_string());
        }
    }
    fallback_label_match
}

/// Find the agent's default/ask ACP mode to use when leaving `Auto` (so `Ask`, `Auto-edits`, and
/// `Full access` keep the agent sending `session/request_permission` to `acp-host`).
pub fn find_advertised_default_mode_id(
    available_modes: &Value,
    initial_mode: Option<&str>,
) -> Option<String> {
    let mode_ids: Vec<String> = available_modes
        .as_array()
        .map(|modes| {
            modes
                .iter()
                .filter_map(|item| {
                    item.get("id")
                        .or_else(|| item.get("modeId"))
                        .and_then(Value::as_str)
                        .map(str::trim)
                        .filter(|s| !s.is_empty())
                        .map(str::to_string)
                })
                .collect()
        })
        .unwrap_or_default();

    for wanted in [
        "default",
        "ask",
        "manual",
        "supervised",
        "ask-for-approval",
        "approval-required",
        "normal",
        "standard",
        "code",
    ] {
        if let Some(id) = mode_ids.iter().find(|id| id.eq_ignore_ascii_case(wanted)) {
            return Some(id.clone());
        }
    }

    if let Some(initial) = initial_mode
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .filter(|s| !is_auto_or_bypass_mode_id(s))
    {
        return Some(initial.to_string());
    }

    // Fallback: any advertised mode that is not auto/bypass/edit-auto/plan.
    mode_ids.into_iter().find(|id| {
        let lower = id.to_ascii_lowercase();
        !is_auto_or_bypass_mode_id(id) && lower != "plan" && lower != "read-only" && lower != "readonly"
    })
}

/// High-level category of an ACP `toolCall` for approval evaluation.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ToolCategory {
    /// `read`, `search`, `think` (without shell/command fields): never asked.
    ReadOnly,
    /// `edit`, `delete`, `move` (without shell/command fields): auto-approved in `Auto-edits` only
    /// when every resolved target path stays strictly inside `cwd` or a configured writable root.
    FileMutation,
    /// `execute`, `fetch`, `switch_mode`, `other`, omitted/unknown, or any call carrying a command
    /// field in `rawInput`: always asked in `Ask` and `Auto-edits`.
    Other,
}

fn canonical_key(raw: &str) -> String {
    raw.chars()
        .filter(|ch| ch.is_ascii_alphanumeric())
        .map(|ch| ch.to_ascii_lowercase())
        .collect()
}

fn is_command_key(norm: &str) -> bool {
    matches!(
        norm,
        "command"
            | "commandline"
            | "cmd"
            | "cmdline"
            | "script"
            | "shell"
            | "shellcommand"
            | "argv"
            | "exec"
    )
}

fn raw_input_contains_command(value: &Value, depth: usize) -> bool {
    if depth > MAX_RAW_INPUT_DEPTH {
        // Fail closed on deeply nested inputs.
        return true;
    }
    match value {
        Value::String(text) => {
            let trimmed = text.trim();
            if trimmed.starts_with('{') || trimmed.starts_with('[') {
                match serde_json::from_str::<Value>(trimmed) {
                    Ok(parsed) => raw_input_contains_command(&parsed, depth + 1),
                    Err(_) => false,
                }
            } else {
                false
            }
        }
        Value::Array(items) => items
            .iter()
            .any(|item| raw_input_contains_command(item, depth + 1)),
        Value::Object(map) => {
            for (key, val) in map {
                let norm = canonical_key(key);
                if is_command_key(&norm) {
                    let non_empty = match val {
                        Value::Null => false,
                        Value::String(s) => !s.trim().is_empty(),
                        Value::Array(arr) => !arr.is_empty(),
                        _ => true,
                    };
                    if non_empty {
                        return true;
                    }
                }
                if raw_input_contains_command(val, depth + 1) {
                    return true;
                }
            }
            false
        }
        _ => false,
    }
}

pub fn classify_tool_call(params: &Value) -> ToolCategory {
    if let Some(raw_input) = params.pointer("/toolCall/rawInput") {
        if raw_input_contains_command(raw_input, 0) {
            return ToolCategory::Other;
        }
    }
    let kind = params
        .pointer("/toolCall/kind")
        .and_then(Value::as_str)
        .map(|s| s.trim().to_ascii_lowercase())
        .unwrap_or_default();
    match kind.as_str() {
        "read" | "search" | "think" => ToolCategory::ReadOnly,
        "edit" | "delete" | "move" => ToolCategory::FileMutation,
        _ => ToolCategory::Other,
    }
}

/// Permission option parsed from ACP `session/request_permission`.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct AcpPermissionOption {
    pub option_id: String,
    pub name: String,
    pub kind: String,
}

/// Outcome of evaluating a single ACP `session/request_permission` against the session's live
/// approval level.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum PermissionDecision {
    AutoApprove {
        option_id: String,
    },
    AskUser {
        /// Plain-language note shown on the approval card when an expected auto-approval could not
        /// run (for example, in Full access or when the agent only offered `allow_always`).
        reason: Option<String>,
    },
}

/// Select ONLY a one-time allow option (`kind == "allow_once"`). Never selects `allow_always`.
pub fn select_allow_once_option(options: &[AcpPermissionOption]) -> Option<String> {
    options
        .iter()
        .find(|opt| opt.kind == "allow_once" && !opt.option_id.trim().is_empty())
        .map(|opt| opt.option_id.clone())
}

fn no_allow_once_reason(options: &[AcpPermissionOption], level: AcpApprovalLevel) -> String {
    let has_allow_always = options.iter().any(|opt| opt.kind == "allow_always");
    if has_allow_always {
        if level == AcpApprovalLevel::FullAccess {
            "Full access didn't approve this automatically because the agent only offered a saved allow option. Ghostex asked you instead of saving it.".to_string()
        } else {
            "This agent only offered a saved allow option, so Ghostex asked you instead of saving it."
                .to_string()
        }
    } else if level == AcpApprovalLevel::FullAccess {
        "Full access didn't approve this automatically because the agent didn't offer a one-time allow option.".to_string()
    } else {
        "This agent didn't offer a one-time allow option, so Ghostex asked you to choose."
            .to_string()
    }
}

/// Evaluate one `session/request_permission` against `level`, `writable_roots`, and `cwd`.
pub fn evaluate_permission(
    level: AcpApprovalLevel,
    params: &Value,
    options: &[AcpPermissionOption],
    writable_roots: &[PathBuf],
    cwd: &Path,
) -> PermissionDecision {
    let category = classify_tool_call(params);
    let wants_auto_approve = match category {
        ToolCategory::ReadOnly => true,
        ToolCategory::FileMutation => match level {
            AcpApprovalLevel::FullAccess => true,
            AcpApprovalLevel::AutoEdits => {
                file_mutation_stays_inside_roots(params, writable_roots, cwd)
            }
            AcpApprovalLevel::Ask | AcpApprovalLevel::Auto => false,
        },
        ToolCategory::Other => matches!(level, AcpApprovalLevel::FullAccess),
    };

    if wants_auto_approve {
        if let Some(option_id) = select_allow_once_option(options) {
            return PermissionDecision::AutoApprove { option_id };
        }
        return PermissionDecision::AskUser {
            reason: Some(no_allow_once_reason(options, level)),
        };
    }

    PermissionDecision::AskUser { reason: None }
}

/// Canonicalize `cwd` and any additional writable roots, dropping duplicates and empty paths.
pub fn canonicalize_writable_roots(cwd: &Path, extra_roots: &[PathBuf]) -> Vec<PathBuf> {
    let mut roots = Vec::new();
    for raw in std::iter::once(cwd).chain(extra_roots.iter().map(PathBuf::as_path)) {
        if raw.as_os_str().is_empty() {
            continue;
        }
        let resolved = resolve_path_with_symlinks(raw, cwd).or_else(|| fs::canonicalize(raw).ok());
        if let Some(path) = resolved {
            if !roots.iter().any(|existing| existing == &path) {
                roots.push(path);
            }
        }
    }
    roots
}

enum OwnedComponent {
    Prefix(std::ffi::OsString),
    RootDir,
    CurDir,
    ParentDir,
    Normal(std::ffi::OsString),
}

fn owned_components(path: &Path) -> VecDeque<OwnedComponent> {
    path.components()
        .map(|comp| match comp {
            Component::Prefix(prefix) => OwnedComponent::Prefix(prefix.as_os_str().to_os_string()),
            Component::RootDir => OwnedComponent::RootDir,
            Component::CurDir => OwnedComponent::CurDir,
            Component::ParentDir => OwnedComponent::ParentDir,
            Component::Normal(part) => OwnedComponent::Normal(part.to_os_string()),
        })
        .collect()
}

fn percent_decode_strict(input: &str) -> Option<String> {
    let bytes = input.as_bytes();
    let mut out = Vec::with_capacity(bytes.len());
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == b'%' {
            if i + 2 >= bytes.len() {
                return None;
            }
            let hi = (bytes[i + 1] as char).to_digit(16)?;
            let lo = (bytes[i + 2] as char).to_digit(16)?;
            let byte = ((hi << 4) | lo) as u8;
            if byte == 0 {
                return None;
            }
            out.push(byte);
            i += 3;
        } else {
            out.push(bytes[i]);
            i += 1;
        }
    }
    String::from_utf8(out).ok()
}

/// Parse a `file://` URI strictly into a local filesystem path. Fails closed (`None`) for remote
/// authorities, query/fragment components, malformed percent-encoding, or non-absolute paths.
fn parse_file_uri(text: &str) -> Option<PathBuf> {
    if text.len() < 7 || !text[..7].eq_ignore_ascii_case("file://") {
        return None;
    }
    let rest = &text[7..];
    if rest.contains('?') || rest.contains('#') {
        return None;
    }
    let path_part = if let Some(after_slash) = rest.strip_prefix('/') {
        format!("/{after_slash}")
    } else if rest.len() >= 10 && rest[..10].eq_ignore_ascii_case("localhost/") {
        format!("/{}", &rest[10..])
    } else {
        // Non-empty non-localhost authority (e.g. file://host/share) or missing slash.
        return None;
    };
    let decoded = percent_decode_strict(&path_part)?;
    if decoded.is_empty() || decoded.contains('\0') {
        return None;
    }
    #[cfg(windows)]
    {
        let bytes = decoded.as_bytes();
        if bytes.len() >= 3
            && bytes[0] == b'/'
            && bytes[1].is_ascii_alphabetic()
            && bytes[2] == b':'
        {
            return Some(PathBuf::from(&decoded[1..]));
        }
    }
    Some(PathBuf::from(decoded))
}

fn is_windows_drive_relative(text: &str) -> bool {
    let bytes = text.as_bytes();
    bytes.len() >= 2
        && bytes[0].is_ascii_alphabetic()
        && bytes[1] == b':'
        && !matches!(bytes.get(2), Some(b'\\' | b'/'))
}

fn has_unsafe_path_syntax(text: &str) -> bool {
    text.trim().is_empty()
        || text.contains('\0')
        || text.starts_with('~')
        || text.contains('$')
        || text.contains("://")
        || is_windows_drive_relative(text)
}

/// Resolve `raw_path` relative to `cwd` while following every symlink component, including
/// dangling symlinks whose final target does not exist yet. Fails closed (`None`) on tilde paths,
/// shell/env syntax (`$`), non-`file://` URI schemes, malformed `file://` URIs, Windows
/// drive-relative paths (`C:foo`), symlink loops, NUL bytes, or non-`NotFound` filesystem errors.
pub fn resolve_path_with_symlinks(raw_path: &Path, cwd: &Path) -> Option<PathBuf> {
    let text = raw_path.to_str()?;
    let trimmed = text.trim();
    if trimmed.is_empty() || trimmed.contains('\0') {
        return None;
    }
    let parsed_path = if trimmed.len() >= 7 && trimmed[..7].eq_ignore_ascii_case("file://") {
        let uri_path = parse_file_uri(trimmed)?;
        let uri_text = uri_path.to_str()?;
        if has_unsafe_path_syntax(uri_text) {
            return None;
        }
        uri_path
    } else {
        if has_unsafe_path_syntax(trimmed) {
            return None;
        }
        PathBuf::from(trimmed)
    };

    // On Windows, a Prefix component without a RootDir is drive-relative (e.g. `C:foo`): fail closed.
    let mut has_prefix = false;
    let mut has_root = false;
    for comp in parsed_path.components() {
        match comp {
            Component::Prefix(_) => has_prefix = true,
            Component::RootDir => has_root = true,
            _ => {}
        }
    }
    if has_prefix && !has_root {
        return None;
    }

    let abs = if parsed_path.is_absolute() {
        parsed_path
    } else {
        cwd.join(parsed_path)
    };
    let mut queue = owned_components(&abs);
    let mut resolved = PathBuf::new();
    let mut hops = 0usize;

    while let Some(comp) = queue.pop_front() {
        match comp {
            OwnedComponent::Prefix(prefix) => {
                resolved.push(prefix);
            }
            OwnedComponent::RootDir => {
                resolved.push(Component::RootDir.as_os_str());
            }
            OwnedComponent::CurDir => {}
            OwnedComponent::ParentDir => {
                resolved.pop();
            }
            OwnedComponent::Normal(part) => {
                let candidate = resolved.join(&part);
                match fs::symlink_metadata(&candidate) {
                    Ok(meta) if meta.file_type().is_symlink() => {
                        hops += 1;
                        if hops > MAX_SYMLINK_HOPS {
                            return None;
                        }
                        let target = fs::read_link(&candidate).ok()?;
                        if target.as_os_str().is_empty()
                            || target.to_str().is_some_and(|s| s.contains('\0'))
                        {
                            return None;
                        }
                        let target_components = owned_components(&target);
                        if target.is_absolute() {
                            resolved.clear();
                        }
                        for target_comp in target_components.into_iter().rev() {
                            queue.push_front(target_comp);
                        }
                    }
                    Ok(_) => {
                        resolved = fs::canonicalize(&candidate).unwrap_or(candidate);
                    }
                    Err(err) if err.kind() == std::io::ErrorKind::NotFound => {
                        resolved = candidate;
                    }
                    Err(_) => {
                        return None;
                    }
                }
            }
        }
    }
    if resolved.as_os_str().is_empty() {
        return None;
    }
    Some(resolved)
}

fn push_unique(paths: &mut Vec<String>, path: &str) {
    if !paths.iter().any(|existing| existing == path) {
        paths.push(path.to_string());
    }
}

fn is_single_path_key(norm: &str) -> bool {
    matches!(
        norm,
        "path"
            | "filepath"
            | "targetpath"
            | "targetfile"
            | "target"
            | "absolutepath"
            | "source"
            | "sourcepath"
            | "src"
            | "srcpath"
            | "sourcefile"
            | "destination"
            | "destinationpath"
            | "destinationfile"
            | "dest"
            | "destpath"
            | "destfile"
            | "dst"
            | "dstpath"
            | "dstfile"
            | "from"
            | "frompath"
            | "fromfile"
            | "to"
            | "topath"
            | "tofile"
            | "oldpath"
            | "newpath"
            | "oldfile"
            | "newfile"
            | "oldfilepath"
            | "newfilepath"
            | "output"
            | "outputpath"
            | "outputfile"
            | "file"
            | "filename"
            | "uri"
            | "fileuri"
            | "targeturi"
            | "sourceuri"
            | "desturi"
            | "directory"
            | "dir"
            | "dirpath"
            | "folder"
            | "folderpath"
            | "cwd"
            | "workdir"
            | "workingdirectory"
            | "root"
            | "rootdir"
    )
}

fn is_path_list_or_map_key(norm: &str) -> bool {
    matches!(
        norm,
        "paths"
            | "files"
            | "filepaths"
            | "targets"
            | "edits"
            | "changes"
            | "operations"
            | "ops"
            | "moves"
            | "renames"
            | "deletes"
            | "deletions"
            | "patches"
            | "diffs"
            | "entries"
            | "items"
            | "uris"
    )
}

fn is_patch_key(norm: &str) -> bool {
    matches!(
        norm,
        "patch"
            | "patchtext"
            | "diff"
            | "difftext"
            | "unifieddiff"
            | "input"
            | "rawpatch"
    )
}

fn is_known_non_path_key(norm: &str) -> bool {
    matches!(
        norm,
        "content"
            | "contents"
            | "text"
            | "newtext"
            | "oldtext"
            | "newstring"
            | "oldstring"
            | "newstr"
            | "oldstr"
            | "replacement"
            | "replacementcontent"
            | "replacementchunks"
            | "targetcontent"
            | "codecontent"
            | "code"
            | "body"
            | "data"
            | "encoding"
            | "overwrite"
            | "append"
            | "create"
            | "createmissing"
            | "createdirs"
            | "recursive"
            | "force"
            | "replaceall"
            | "allowmultiple"
            | "emptyfile"
            | "startline"
            | "endline"
            | "line"
            | "column"
            | "offset"
            | "limit"
            | "instruction"
            | "description"
            | "summary"
            | "reason"
            | "explanation"
            | "complexity"
            | "isartifact"
            | "artifactmetadata"
            | "yieldms"
            | "timeout"
            | "mode"
            | "permissions"
            | "type"
            | "kind"
            | "id"
            | "name"
            | "title"
            | "language"
            | "format"
    )
}

fn looks_like_path_key_name(norm: &str) -> bool {
    norm.contains("path")
        || norm.contains("file")
        || norm.contains("dir")
        || norm.contains("folder")
        || norm.contains("dest")
        || norm.contains("dst")
        || norm.contains("target")
        || norm.contains("src")
        || norm.contains("source")
        || norm.contains("uri")
        || norm.contains("url")
        || norm.contains("patch")
        || norm.contains("diff")
        || norm.contains("move")
        || norm.contains("rename")
        || norm.contains("root")
}

fn looks_like_external_path_value(text: &str) -> bool {
    let trimmed = text.trim();
    trimmed.starts_with('/')
        || trimmed.starts_with('~')
        || trimmed.starts_with("../")
        || trimmed.starts_with("..\\")
        || trimmed.starts_with("\\\\")
        || trimmed.contains("://")
        || (trimmed.len() >= 2
            && trimmed.as_bytes()[0].is_ascii_alphabetic()
            && trimmed.as_bytes()[1] == b':')
}

fn clean_diff_path_token(raw: &str) -> Option<String> {
    let without_tab = raw.split('\t').next().unwrap_or(raw).trim();
    let unquoted = without_tab
        .strip_prefix('"')
        .and_then(|s| s.strip_suffix('"'))
        .unwrap_or(without_tab)
        .trim();
    if unquoted.is_empty() || unquoted == "/dev/null" {
        return None;
    }
    let stripped = unquoted
        .strip_prefix("a/")
        .or_else(|| unquoted.strip_prefix("b/"))
        .unwrap_or(unquoted)
        .trim();
    if stripped.is_empty() || stripped == "/dev/null" {
        return None;
    }
    Some(stripped.to_string())
}

/// Extract target paths from `apply_patch` headers (trimmed) and unified diff headers
/// (`diff --git`, `---`, `+++`, `rename from/to`, `copy from/to`). Returns `false` if the patch
/// text is non-empty and yields no paths (unparseable patch format -> fail closed).
fn collect_patch_paths(patch: &str, paths: &mut Vec<String>) -> bool {
    let trimmed_all = patch.trim();
    if trimmed_all.is_empty() {
        return false;
    }
    let mut found = 0usize;
    for raw_line in patch.lines() {
        let line = raw_line.trim_start();
        let mut matched_apply = false;
        for prefix in [
            "*** Update File:",
            "*** Add File:",
            "*** Delete File:",
            "*** Move to:",
        ] {
            if let Some(rest) = line.strip_prefix(prefix) {
                let target = rest.trim();
                if target.is_empty() {
                    return false;
                }
                push_unique(paths, target);
                found += 1;
                matched_apply = true;
                break;
            }
        }
        if matched_apply {
            continue;
        }
        if let Some(rest) = line.strip_prefix("diff --git ") {
            // `diff --git a/foo b/bar` (handle simple or quoted tokens; fail closed if malformed)
            let tokens: Vec<&str> = rest.split_whitespace().collect();
            if tokens.len() < 2 {
                return false;
            }
            for token in [tokens[0], tokens[tokens.len() - 1]] {
                if let Some(p) = clean_diff_path_token(token) {
                    push_unique(paths, &p);
                    found += 1;
                }
            }
            continue;
        }
        for prefix in ["--- ", "+++ ", "rename from ", "rename to ", "copy from ", "copy to "] {
            if let Some(rest) = line.strip_prefix(prefix) {
                if let Some(p) = clean_diff_path_token(rest) {
                    push_unique(paths, &p);
                    found += 1;
                }
                break;
            }
        }
    }
    found > 0
}

fn collect_raw_input_paths(value: &Value, paths: &mut Vec<String>, depth: usize) -> bool {
    if depth > MAX_RAW_INPUT_DEPTH {
        return false;
    }
    match value {
        Value::Null => true,
        Value::String(text) => {
            let trimmed = text.trim();
            if trimmed.is_empty() {
                return true;
            }
            if trimmed.starts_with('{') || trimmed.starts_with('[') {
                let Ok(parsed) = serde_json::from_str::<Value>(trimmed) else {
                    return false;
                };
                return collect_raw_input_paths(&parsed, paths, depth + 1);
            }
            // Plain string `rawInput`: only acceptable if it parses as a patch/diff.
            collect_patch_paths(trimmed, paths)
        }
        Value::Array(items) => {
            for item in items {
                match item {
                    Value::String(s) => {
                        let trimmed = s.trim();
                        if trimmed.is_empty() {
                            return false;
                        }
                        push_unique(paths, trimmed);
                    }
                    Value::Object(_) | Value::Array(_) => {
                        if !collect_raw_input_paths(item, paths, depth + 1) {
                            return false;
                        }
                    }
                    _ => return false,
                }
            }
            true
        }
        Value::Object(obj) => {
            for (key, val) in obj {
                let norm = canonical_key(key);
                if is_single_path_key(&norm) {
                    match val {
                        Value::Null => {}
                        Value::String(s) => {
                            let trimmed = s.trim();
                            if trimmed.is_empty() {
                                return false;
                            }
                            push_unique(paths, trimmed);
                        }
                        _ => return false,
                    }
                } else if is_path_list_or_map_key(&norm) {
                    match val {
                        Value::Null => {}
                        Value::Array(items) => {
                            for item in items {
                                match item {
                                    Value::String(s) => {
                                        let trimmed = s.trim();
                                        if trimmed.is_empty() {
                                            return false;
                                        }
                                        push_unique(paths, trimmed);
                                    }
                                    Value::Object(_) | Value::Array(_) => {
                                        if !collect_raw_input_paths(item, paths, depth + 1) {
                                            return false;
                                        }
                                    }
                                    _ => return false,
                                }
                            }
                        }
                        Value::Object(map) => {
                            // Path-keyed map such as `{ "changes": { "/repo/a": "...", "/outside/b": "..." } }`.
                            for (entry_key, entry_val) in map {
                                let trimmed_key = entry_key.trim();
                                if trimmed_key.is_empty() {
                                    return false;
                                }
                                push_unique(paths, trimmed_key);
                                if matches!(entry_val, Value::Object(_) | Value::Array(_))
                                    && !collect_raw_input_paths(entry_val, paths, depth + 1)
                                {
                                    return false;
                                }
                            }
                        }
                        _ => return false,
                    }
                } else if is_patch_key(&norm) {
                    match val {
                        Value::Null => {}
                        Value::String(s) => {
                            if !collect_patch_paths(s, paths) {
                                return false;
                            }
                        }
                        Value::Object(_) | Value::Array(_) => {
                            if !collect_raw_input_paths(val, paths, depth + 1) {
                                return false;
                            }
                        }
                        _ => return false,
                    }
                } else if is_known_non_path_key(&norm) {
                    // Still recurse if a known field carries structured nested objects/arrays
                    // (e.g. `replacementChunks`, structured `content` blocks).
                    if matches!(val, Value::Object(_) | Value::Array(_))
                        && !collect_raw_input_paths(val, paths, depth + 1)
                    {
                        return false;
                    }
                } else {
                    // Unrecognized key: fail closed if its name looks path/mutation-related, or if
                    // its scalar value looks like an external/filesystem path, or recurse if nested.
                    if looks_like_path_key_name(&norm) {
                        return false;
                    }
                    match val {
                        Value::Object(_) | Value::Array(_) => {
                            if !collect_raw_input_paths(val, paths, depth + 1) {
                                return false;
                            }
                        }
                        Value::String(s) if looks_like_external_path_value(s) => {
                            return false;
                        }
                        _ => {}
                    }
                }
            }
            true
        }
        _ => false,
    }
}

fn collect_content_paths(content: &Value, paths: &mut Vec<String>) -> bool {
    let items: Vec<&Value> = match content {
        Value::Null => return true,
        Value::Array(arr) => arr.iter().collect(),
        Value::Object(_) => vec![content],
        _ => return false,
    };
    for item in items {
        let Some(obj) = item.as_object() else {
            return false;
        };
        let is_diff = obj
            .get("type")
            .and_then(Value::as_str)
            .is_some_and(|t| t.eq_ignore_ascii_case("diff"));
        let mut item_paths = 0usize;
        for (key, val) in obj {
            let norm = canonical_key(key);
            if is_single_path_key(&norm) {
                match val {
                    Value::Null => {}
                    Value::String(s) => {
                        let trimmed = s.trim();
                        if trimmed.is_empty() {
                            return false;
                        }
                        push_unique(paths, trimmed);
                        item_paths += 1;
                    }
                    _ => return false,
                }
            } else if is_patch_key(&norm) {
                if let Some(s) = val.as_str() {
                    if !collect_patch_paths(s, paths) {
                        return false;
                    }
                    item_paths += 1;
                }
            }
        }
        if is_diff && item_paths == 0 {
            return false;
        }
    }
    true
}

/// Extract all target paths referenced by an ACP `edit`/`delete`/`move` tool call from
/// `toolCall.locations`, `toolCall.content`, and `toolCall.rawInput` (including unified diffs and
/// `apply_patch` headers). Fails closed (`None`) whenever any location, content diff, or structured
/// input cannot be fully accounted for.
pub fn extract_mutation_paths(params: &Value) -> Option<Vec<String>> {
    let tool_call = params.get("toolCall")?.as_object()?;
    let mut paths = Vec::new();

    if let Some(locations) = tool_call.get("locations") {
        if !locations.is_null() {
            let arr = locations.as_array()?;
            for loc in arr {
                let obj = loc.as_object()?;
                let path = obj
                    .get("path")
                    .and_then(Value::as_str)
                    .map(str::trim)
                    .filter(|s| !s.is_empty())?;
                push_unique(&mut paths, path);
                if let Some(uri) = obj.get("uri") {
                    if !uri.is_null() {
                        let uri_str = uri
                            .as_str()
                            .map(str::trim)
                            .filter(|s| !s.is_empty())?;
                        push_unique(&mut paths, uri_str);
                    }
                }
            }
        }
    }

    if let Some(content) = tool_call.get("content") {
        if !collect_content_paths(content, &mut paths) {
            return None;
        }
    }

    if let Some(raw_input) = tool_call.get("rawInput") {
        if !collect_raw_input_paths(raw_input, &mut paths, 0) {
            return None;
        }
    }

    if paths.is_empty() {
        return None;
    }
    Some(paths)
}

/// True only when path extraction accounts for every mutation target AND every extracted path
/// resolves (with symlinks expanded) strictly inside `writable_roots` (never the root directory
/// itself). Fails closed on any ambiguity, empty paths, or empty roots.
pub fn file_mutation_stays_inside_roots(
    params: &Value,
    writable_roots: &[PathBuf],
    cwd: &Path,
) -> bool {
    if writable_roots.is_empty() {
        return false;
    }
    let Some(paths) = extract_mutation_paths(params) else {
        return false;
    };
    if paths.is_empty() {
        return false;
    }
    let canonical_cwd = resolve_path_with_symlinks(cwd, cwd).unwrap_or_else(|| cwd.to_path_buf());
    paths.iter().all(|raw| {
        let Some(resolved) = resolve_path_with_symlinks(Path::new(raw), &canonical_cwd) else {
            return false;
        };
        writable_roots
            .iter()
            .any(|root| resolved != *root && resolved.starts_with(root))
    })
}
