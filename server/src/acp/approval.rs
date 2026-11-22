//! Approval levels and client-side permission policy for ACP (Non-CLI) sessions.
//!
//! Four levels live on the chat box's existing Mode button:
//! - `ask`: ask before edits and commands
//! - `auto-edits`: auto-approve edit/delete/move inside the session folder and writable roots
//!   (symlink-resolved); ask for commands and anything else
//! - `auto`: use the agent's own advertised ACP automatic mode when available, else behave like `ask`
//! - `full-access`: auto-approve everything
//!
//! Read-only tools (`read`, `search`, `think`) are always auto-approved when the agent offers a
//! one-time allow option. Auto-approvals strictly choose `allow_once` (never `allow_always`) so
//! Ghostex never writes permanent rules into the agent's own allowlist.

use std::collections::VecDeque;
use std::fs;
use std::path::{Component, Path, PathBuf};

use serde::{Deserialize, Serialize};
use serde_json::{Map, Value};

/// Session `runtimeSettings` key storing the remembered ACP approval level for this session.
pub const SESSION_ACP_APPROVAL_LEVEL_KEY: &str = "acpApprovalLevel";

const MAX_SYMLINK_HOPS: usize = 40;

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
    if let Some(modes) = available_modes.as_array() {
        for wanted in ["default", "ask", "manual", "supervised"] {
            if let Some(id) = modes.iter().find_map(|item| {
                let id = item
                    .get("id")
                    .or_else(|| item.get("modeId"))
                    .and_then(Value::as_str)
                    .map(str::trim)
                    .filter(|s| !s.is_empty())?;
                id.eq_ignore_ascii_case(wanted).then(|| id.to_string())
            }) {
                return Some(id);
            }
        }
    }
    initial_mode
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .filter(|s| {
            let lower = s.to_ascii_lowercase();
            !matches!(lower.as_str(), "auto" | "auto_mode" | "auto-mode")
                && !lower.contains("bypass")
                && !lower.contains("yolo")
                && !lower.contains("full")
        })
        .map(str::to_string)
}

/// High-level category of an ACP `toolCall` for approval evaluation.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ToolCategory {
    /// `read`, `search`, `think`: never asked.
    ReadOnly,
    /// `edit`, `delete`, `move`: auto-approved in `Auto-edits` only when every resolved target path
    /// stays inside `cwd` or a configured writable root.
    FileMutation,
    /// `execute`, `fetch`, `switch_mode`, `other`, or omitted/unknown: always asked in `Ask` and
    /// `Auto-edits`.
    Other,
}

pub fn classify_tool_call(params: &Value) -> ToolCategory {
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

    let reason = if level == AcpApprovalLevel::FullAccess {
        Some(
            "The agent still asked for confirmation even though Full access is on. Review the details before allowing."
                .to_string(),
        )
    } else {
        None
    };
    PermissionDecision::AskUser { reason }
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

/// Resolve `raw_path` relative to `cwd` while following every symlink component, including
/// dangling symlinks whose final target does not exist yet. Fails closed (`None`) on symlink
/// loops, NUL bytes, or non-`NotFound` filesystem errors.
pub fn resolve_path_with_symlinks(raw_path: &Path, cwd: &Path) -> Option<PathBuf> {
    let text = raw_path.to_str()?;
    if text.trim().is_empty() || text.contains('\0') {
        return None;
    }
    let abs = if raw_path.is_absolute() {
        raw_path.to_path_buf()
    } else {
        cwd.join(raw_path)
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
                        let target_components = owned_components(&target);
                        if target.is_absolute() {
                            resolved.clear();
                        }
                        for target_comp in target_components.into_iter().rev() {
                            queue.push_front(target_comp);
                        }
                    }
                    Ok(_) => {
                        resolved = candidate;
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

/// Extract all target paths referenced by an ACP `edit`/`delete`/`move` tool call from both
/// `toolCall.locations` and `toolCall.rawInput` (plus `apply_patch`-style headers if present).
pub fn extract_mutation_paths(params: &Value) -> Vec<String> {
    let mut paths = Vec::new();
    let Some(tool_call) = params.get("toolCall") else {
        return paths;
    };
    if let Some(locations) = tool_call.get("locations").and_then(Value::as_array) {
        for loc in locations {
            if let Some(path) = loc
                .get("path")
                .and_then(Value::as_str)
                .map(str::trim)
                .filter(|s| !s.is_empty())
            {
                push_unique(&mut paths, path);
            }
        }
    }
    if let Some(raw_input) = tool_call.get("rawInput") {
        collect_raw_input_paths(raw_input, &mut paths);
    }
    paths
}

fn push_unique(paths: &mut Vec<String>, path: &str) {
    if !paths.iter().any(|existing| existing == path) {
        paths.push(path.to_string());
    }
}

const PATH_KEYS: &[&str] = &[
    "path",
    "Path",
    "file_path",
    "filePath",
    "FilePath",
    "target_path",
    "targetPath",
    "TargetFile",
    "target_file",
    "AbsolutePath",
    "absolutePath",
    "absolute_path",
    "source",
    "source_path",
    "sourcePath",
    "destination",
    "destination_path",
    "destinationPath",
    "dest",
    "from",
    "to",
    "old_path",
    "new_path",
    "oldPath",
    "newPath",
    "directory",
    "dir",
];

const PATH_LIST_KEYS: &[&str] = &["paths", "files", "file_paths", "filePaths", "targets", "edits"];

fn collect_raw_input_paths(value: &Value, paths: &mut Vec<String>) {
    let Some(obj) = value.as_object() else {
        return;
    };
    for key in PATH_KEYS {
        if let Some(path) = obj
            .get(*key)
            .and_then(Value::as_str)
            .map(str::trim)
            .filter(|s| !s.is_empty())
        {
            push_unique(paths, path);
        }
    }
    for key in PATH_LIST_KEYS {
        if let Some(items) = obj.get(*key).and_then(Value::as_array) {
            for item in items {
                if let Some(path) = item
                    .as_str()
                    .map(str::trim)
                    .filter(|s| !s.is_empty())
                {
                    push_unique(paths, path);
                } else if item.is_object() {
                    collect_raw_input_paths(item, paths);
                }
            }
        }
    }
    for patch_key in ["patch", "input", "diff"] {
        if let Some(patch) = obj.get(patch_key).and_then(Value::as_str) {
            for line in patch.lines() {
                for prefix in [
                    "*** Update File: ",
                    "*** Add File: ",
                    "*** Delete File: ",
                    "*** Move to: ",
                ] {
                    if let Some(rest) = line.strip_prefix(prefix) {
                        let trimmed = rest.trim();
                        if !trimmed.is_empty() {
                            push_unique(paths, trimmed);
                        }
                    }
                }
            }
        }
    }
}

/// True only when at least one mutation path is extracted AND every extracted path resolves
/// (with symlinks expanded) inside `writable_roots`. Fails closed when paths or roots are empty.
pub fn file_mutation_stays_inside_roots(
    params: &Value,
    writable_roots: &[PathBuf],
    cwd: &Path,
) -> bool {
    if writable_roots.is_empty() {
        return false;
    }
    let paths = extract_mutation_paths(params);
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
            .any(|root| resolved == *root || resolved.starts_with(root))
    })
}
