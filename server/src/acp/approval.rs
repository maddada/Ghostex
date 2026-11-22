//! Approval levels and client-side permission policy for ACP (Non-CLI) sessions.
//!
//! Four levels live on the chat box's existing Mode button:
//! - `ask`: ask before edits and commands
//! - `auto-edits`: auto-approve edit/delete/move strictly inside the session folder and writable
//!   roots (symlink-resolved) using an explicit allowlist of understood tool shapes; ask for
//!   commands and anything else
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

use crate::domain::DomainStateError;

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

/// True when an `acceptAllMode` string explicitly turns accept-all on.
pub fn is_accept_all_enabled_value(raw: Option<&str>) -> bool {
    raw.map(str::trim)
        .is_some_and(|v| matches!(v, "enabled" | "acceptAll"))
}

/// True when an `acceptAllMode` string explicitly turns accept-all off.
pub fn is_accept_all_disabled_value(raw: Option<&str>) -> bool {
    raw.map(str::trim).is_some_and(|v| v == "disabled")
}

/// True if changing `acceptAllMode` from `stored` to `incoming` enables accept-all or removes a
/// `"disabled"` override while global `agentAcceptAllEnabled` is `true`.
pub fn accept_all_mode_change_raises(
    stored: Option<&str>,
    incoming: Option<&str>,
    global_accept_all_enabled: bool,
) -> bool {
    if is_accept_all_enabled_value(incoming) && !is_accept_all_enabled_value(stored) {
        return true;
    }
    if global_accept_all_enabled
        && is_accept_all_disabled_value(stored)
        && !is_accept_all_disabled_value(incoming)
    {
        return true;
    }
    false
}

/// Read the global `agentAcceptAllEnabled` boolean from shared settings or SQLite `agents.settings.v1`
/// (`true` if either enables it, `false` if both are off/absent).
pub fn global_accept_all_enabled_in_shared_settings(
    paths: &crate::paths::GxserverPaths,
) -> bool {
    let shared_enabled = crate::shared_settings_file::read_shared_settings(paths)
        .and_then(|v| v.pointer("/agentAcceptAllEnabled").and_then(Value::as_bool))
        .unwrap_or(false);
    if shared_enabled {
        return true;
    }
    crate::storage::open_gxserver_database(paths)
        .ok()
        .and_then(|db| crate::agents::read_agent_settings(&db).ok())
        .and_then(|settings| {
            settings
                .get("agentAcceptAllEnabled")
                .and_then(Value::as_bool)
        })
        .unwrap_or(false)
}

/// True when a shell command or startup text contains flags that bypass approvals or launch ACP
/// above `Ask` (`--dangerously-skip-permissions`, `--yolo`, `--accept-all`, `--approval-level`
/// above `ask`, `danger-full-access`, `bypassPermissions`, `--full-auto`, `--ask-for-approval never`).
pub fn command_has_accept_all_flag(cmd: &str) -> bool {
    let lower = cmd.to_ascii_lowercase();
    let tokens: Vec<String> = lower
        .split_whitespace()
        .map(|tok| {
            tok.trim_matches(|c: char| matches!(c, '\'' | '"' | ';' | '&' | '|' | '(' | ')' | '\r' | '\n'))
                .to_string()
        })
        .filter(|tok| !tok.is_empty())
        .collect();
    for (idx, tok) in tokens.iter().enumerate() {
        let next = tokens.get(idx + 1).map(String::as_str);
        if matches!(
            tok.as_str(),
            "--dangerously-skip-permissions"
                | "--dangerously-bypass-approvals-and-sandbox"
                | "--yolo"
                | "--accept-all"
                | "--full-auto"
                | "--allow-all"
                | "--bypass-permissions"
        ) {
            return true;
        }
        if tok.contains("danger-full-access")
            || tok.contains("bypasspermissions")
            || tok.contains("dangerously-skip-permissions")
            || tok.contains("dangerously-bypass-approvals-and-sandbox")
        {
            return true;
        }
        if let Some(val) = tok.strip_prefix("--approval-level=") {
            if AcpApprovalLevel::parse(val) != Some(AcpApprovalLevel::Ask) {
                return true;
            }
        } else if tok == "--approval-level" {
            if next.and_then(AcpApprovalLevel::parse) != Some(AcpApprovalLevel::Ask) {
                return true;
            }
        }
        if let Some(val) = tok
            .strip_prefix("--permission-mode=")
            .or_else(|| tok.strip_prefix("--approval-mode="))
            .or_else(|| tok.strip_prefix("--ask-for-approval="))
        {
            if matches!(
                val,
                "bypasspermissions" | "bypass" | "yolo" | "auto" | "never" | "full-auto"
            ) {
                return true;
            }
        } else if matches!(
            tok.as_str(),
            "--permission-mode" | "--approval-mode" | "--ask-for-approval" | "-a"
        ) {
            if matches!(
                next,
                Some("bypasspermissions" | "bypass" | "yolo" | "auto" | "never" | "full-auto")
            ) {
                return true;
            }
        }
    }
    false
}

fn value_has_non_empty_text(val: Option<&Value>) -> bool {
    val.and_then(Value::as_str)
        .is_some_and(|s| !s.trim().is_empty())
}

/// Guard and normalize session creation parameters across `/api/createSession`,
/// `/api/createAgentSession`, and `/api/createWorktreeSession`:
/// - Non-user-device callers (`Principal::Session`, guests, extensions) trying to force
///   `launchSettings.acceptAllMode` (`"enabled"` / `"acceptAll"`), raise
///   `runtimeSettings.acpApprovalLevel` above `Ask`, or pass accept-all flags in `agentCommand`
///   are explicitly refused (`403 forbidden`).
/// - On `/api/createSession` (`strip_raw_launch_plan == true`), non-user-device callers are also
///   refused when supplying raw `launchSettings.agentLaunchPlan` or `launchSettings.startupText`
///   so they cannot smuggle `--approval-level full-access` or `--dangerously-skip-permissions`.
/// - For all callers, client-supplied `runtimeSettings.acpApprovalLevel` is stripped so project,
///   agent, launch, and global `acceptAllMode` defaults always govern the starting level.
pub fn guard_create_session_params(
    params: &Map<String, Value>,
    is_user_device: bool,
    strip_raw_launch_plan: bool,
) -> Result<Map<String, Value>, DomainStateError> {
    let mut cleaned = params.clone();
    if !is_user_device {
        if let Some(raw_level) = cleaned
            .get("runtimeSettings")
            .and_then(Value::as_object)
            .and_then(|rt| rt.get(SESSION_ACP_APPROVAL_LEVEL_KEY))
            .and_then(Value::as_str)
            .map(str::trim)
            .filter(|s| !s.is_empty())
        {
            if AcpApprovalLevel::parse(raw_level) != Some(AcpApprovalLevel::Ask) {
                return Err(DomainStateError {
                    code: "forbidden",
                    message: "Only your own devices can start a session above Ask approval."
                        .to_string(),
                });
            }
        }
        if let Some(cmd) = cleaned.get("agentCommand").and_then(Value::as_str) {
            if command_has_accept_all_flag(cmd) {
                return Err(DomainStateError {
                    code: "forbidden",
                    message:
                        "Only your own devices can pass accept-all flags in an agent command."
                            .to_string(),
                });
            }
        }
        if let Some(launch) = cleaned.get("launchSettings").and_then(Value::as_object) {
            let incoming_accept = launch.get("acceptAllMode").and_then(Value::as_str);
            if is_accept_all_enabled_value(incoming_accept) {
                return Err(DomainStateError {
                    code: "forbidden",
                    message: "Only your own devices can start a session with accept-all enabled."
                        .to_string(),
                });
            }
            if let Some(cmd) = launch.get("agentCommand").and_then(Value::as_str) {
                if command_has_accept_all_flag(cmd) {
                    return Err(DomainStateError {
                        code: "forbidden",
                        message:
                            "Only your own devices can pass accept-all flags in an agent command."
                                .to_string(),
                    });
                }
            }
            let has_raw_startup = value_has_non_empty_text(launch.get("startupText"));
            let has_raw_plan =
                strip_raw_launch_plan && launch.get("agentLaunchPlan").is_some_and(|v| !v.is_null());
            if has_raw_plan || has_raw_startup {
                return Err(DomainStateError {
                    code: "forbidden",
                    message: "Only your own devices can supply a raw launch plan or startup command."
                        .to_string(),
                });
            }
        }
    }
    if let Some(runtime) = cleaned
        .get_mut("runtimeSettings")
        .and_then(Value::as_object_mut)
    {
        runtime.remove(SESSION_ACP_APPROVAL_LEVEL_KEY);
    }
    if !is_user_device {
        if let Some(launch) = cleaned
            .get_mut("launchSettings")
            .and_then(Value::as_object_mut)
        {
            if strip_raw_launch_plan {
                launch.remove("agentLaunchPlan");
            }
            launch.remove("startupText");
        }
    }
    Ok(cleaned)
}

/// Enforce the single-source-of-truth rule on `/api/updateSession`:
/// - Non-user-device callers trying to raise `runtimeSettings.acpApprovalLevel`, wipe stored
///   `acpApprovalLevel` via `"runtimeSettings": null`, enable `launchSettings.acceptAllMode`,
///   remove a `"disabled"` override (including via `"launchSettings": null`) while global
///   `agentAcceptAllEnabled` is `true`, or inject/modify `agentLaunchPlan`, `startupText`, or
///   accept-all `agentCommand` flags are explicitly refused (`403 forbidden`).
/// - For ALL callers, `runtimeSettings.acpApprovalLevel` is preserved from the stored session so
///   only `acp::channel::set_approval_level` can change a session's approval level.
pub fn guard_update_session_params(
    current_session: Option<&Value>,
    params: &Map<String, Value>,
    is_user_device: bool,
    global_accept_all_enabled: bool,
) -> Result<Map<String, Value>, DomainStateError> {
    let mut cleaned = params.clone();
    let stored_runtime = current_session
        .and_then(|s| s.get("runtimeSettings"))
        .and_then(Value::as_object);
    let stored_launch = current_session
        .and_then(|s| s.get("launchSettings"))
        .and_then(Value::as_object);
    let stored_level = current_session
        .and_then(stored_session_approval_level)
        .unwrap_or(AcpApprovalLevel::Ask);

    if cleaned.contains_key("runtimeSettings") {
        let is_object = cleaned
            .get("runtimeSettings")
            .is_some_and(Value::is_object);
        if !is_object {
            // Passing `"runtimeSettings": null` (or a non-object) would overwrite the stored
            // runtime object with `{}` in `merge_session_update`, wiping `acpApprovalLevel`.
            if !is_user_device
                && stored_runtime
                    .and_then(|m| m.get(SESSION_ACP_APPROVAL_LEVEL_KEY))
                    .is_some()
            {
                return Err(DomainStateError {
                    code: "forbidden",
                    message: "Only your own devices can clear or raise a session's approval level."
                        .to_string(),
                });
            }
            cleaned.insert("runtimeSettings".to_string(), Value::Object(Map::new()));
        }
        if let Some(incoming_runtime) = cleaned
            .get_mut("runtimeSettings")
            .and_then(Value::as_object_mut)
        {
            if !is_user_device {
                if let Some(raw_requested) = incoming_runtime
                    .get(SESSION_ACP_APPROVAL_LEVEL_KEY)
                    .and_then(Value::as_str)
                    .map(str::trim)
                    .filter(|s| !s.is_empty())
                {
                    let requested = AcpApprovalLevel::parse(raw_requested);
                    let allowed = requested.is_some_and(|target| {
                        AcpApprovalLevel::may_transition_without_owner(stored_level, target)
                    });
                    if !allowed {
                        return Err(DomainStateError {
                            code: "forbidden",
                            message: "Only your own devices can raise a session's approval level."
                                .to_string(),
                        });
                    }
                }
            }
            // Always keep the stored `acpApprovalLevel` so `/api/updateSession` cannot diverge
            // from `acp::channel::set_approval_level`.
            match stored_runtime.and_then(|m| m.get(SESSION_ACP_APPROVAL_LEVEL_KEY)) {
                Some(val) => {
                    incoming_runtime
                        .insert(SESSION_ACP_APPROVAL_LEVEL_KEY.to_string(), val.clone());
                }
                None => {
                    incoming_runtime.remove(SESSION_ACP_APPROVAL_LEVEL_KEY);
                }
            }
        }
    }

    if !is_user_device && cleaned.contains_key("launchSettings") {
        let incoming_launch = cleaned.get("launchSettings").and_then(Value::as_object);
        let stored_accept = stored_launch
            .and_then(|m| m.get("acceptAllMode"))
            .and_then(Value::as_str);
        let incoming_accept = incoming_launch
            .and_then(|m| m.get("acceptAllMode"))
            .and_then(Value::as_str);
        if accept_all_mode_change_raises(
            stored_accept,
            incoming_accept,
            global_accept_all_enabled,
        ) {
            return Err(DomainStateError {
                code: "forbidden",
                message: "Only your own devices can enable accept-all on a session.".to_string(),
            });
        }
        if let Some(launch) = incoming_launch {
            let stored_plan = stored_launch.and_then(|m| m.get("agentLaunchPlan"));
            if let Some(incoming_plan) = launch.get("agentLaunchPlan").filter(|v| !v.is_null()) {
                if stored_plan != Some(incoming_plan) {
                    return Err(DomainStateError {
                        code: "forbidden",
                        message: "Only your own devices can modify a session's launch plan."
                            .to_string(),
                    });
                }
            }
            let stored_startup = stored_launch
                .and_then(|m| m.get("startupText"))
                .and_then(Value::as_str);
            if let Some(incoming_startup) = launch
                .get("startupText")
                .and_then(Value::as_str)
                .filter(|s| !s.trim().is_empty())
            {
                if stored_startup != Some(incoming_startup) {
                    return Err(DomainStateError {
                        code: "forbidden",
                        message: "Only your own devices can set a session's startup command."
                            .to_string(),
                    });
                }
            }
            let stored_cmd = stored_launch
                .and_then(|m| m.get("agentCommand"))
                .and_then(Value::as_str);
            if let Some(incoming_cmd) = launch.get("agentCommand").and_then(Value::as_str) {
                if stored_cmd != Some(incoming_cmd) && command_has_accept_all_flag(incoming_cmd) {
                    return Err(DomainStateError {
                        code: "forbidden",
                        message:
                            "Only your own devices can pass accept-all flags in an agent command."
                                .to_string(),
                    });
                }
            }
        }
    }

    Ok(cleaned)
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
        !is_auto_or_bypass_mode_id(id)
            && lower != "plan"
            && lower != "read-only"
            && lower != "readonly"
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

/// Keys that hold raw file content / replacement text / patch text and must NEVER be JSON-parsed
/// or searched for command keys (otherwise editing `mcp.json` or `tasks.json` containing
/// `"command"` would falsely downgrade to a shell command).
fn is_file_body_key(norm: &str) -> bool {
    matches!(
        norm,
        "content"
            | "contents"
            | "filecontent"
            | "newfilecontent"
            | "sourcecode"
            | "newsource"
            | "text"
            | "newtext"
            | "oldtext"
            | "newcontent"
            | "oldcontent"
            | "newstring"
            | "oldstring"
            | "newstr"
            | "oldstr"
            | "replacement"
            | "replacementcontent"
            | "targetcontent"
            | "codecontent"
            | "code"
            | "body"
            | "data"
            | "snippet"
            | "oldsnippet"
            | "newsnippet"
            | "inserttext"
            | "search"
            | "replace"
            | "before"
            | "after"
            | "patch"
            | "patchtext"
            | "diff"
            | "difftext"
            | "unifieddiff"
            | "rawpatch"
    )
}

fn raw_input_contains_command(value: &Value, depth: usize) -> bool {
    if depth > MAX_RAW_INPUT_DEPTH {
        return true;
    }
    match value {
        // Only JSON-parse a top-level stringified `rawInput` (depth == 0), NEVER nested file-body
        // strings such as `Content: "{\"command\": ...}"` when editing JSON config files.
        Value::String(text) if depth == 0 => {
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
                if is_file_body_key(&norm) && val.is_string() {
                    continue;
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

/// Char-safe ASCII case-insensitive prefix strip. Never panics on multi-byte UTF-8 boundaries.
fn strip_ascii_prefix_ignore_case<'a>(input: &'a str, ascii_prefix: &str) -> Option<&'a str> {
    let prefix_bytes = ascii_prefix.as_bytes();
    if input
        .as_bytes()
        .get(..prefix_bytes.len())
        .is_some_and(|head| head.eq_ignore_ascii_case(prefix_bytes))
    {
        input.get(prefix_bytes.len()..)
    } else {
        None
    }
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
    let rest = strip_ascii_prefix_ignore_case(text, "file://")?;
    if rest.contains('?') || rest.contains('#') {
        return None;
    }
    let path_part = if let Some(after_slash) = rest.strip_prefix('/') {
        format!("/{after_slash}")
    } else if let Some(after_host) = strip_ascii_prefix_ignore_case(rest, "localhost/") {
        format!("/{after_host}")
    } else {
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

/// Matches any RFC 3986 URI scheme prefix of length >= 2 (e.g. `file:/etc/passwd`, `http://...`,
/// `vscode:...`), without colliding with single-letter Windows drive prefixes (`C:\...`).
fn has_uri_scheme_prefix(text: &str) -> bool {
    let Some((scheme, _)) = text.split_once(':') else {
        return false;
    };
    if scheme.len() < 2 {
        return false;
    }
    let mut chars = scheme.chars();
    let Some(first) = chars.next() else {
        return false;
    };
    first.is_ascii_alphabetic()
        && chars.all(|ch| ch.is_ascii_alphanumeric() || matches!(ch, '+' | '-' | '.'))
}

/// Matches Windows `%ENVVAR%` expansion syntax (e.g. `%USERPROFILE%\.ssh`).
fn has_windows_percent_env(text: &str) -> bool {
    let bytes = text.as_bytes();
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == b'%' {
            let start = i + 1;
            let mut end = start;
            while end < bytes.len()
                && (bytes[end].is_ascii_alphanumeric() || bytes[end] == b'_')
            {
                end += 1;
            }
            if end > start && bytes.get(end) == Some(&b'%') {
                return true;
            }
        }
        i += 1;
    }
    false
}

/// Matches shell variable/command substitution (`${...}`, `$(...)`, a leading `$IDENT` path
/// segment like `$HOME/.bashrc` or `$home/.bashrc`, or a mid-path all-uppercase `$ENV_VAR`
/// segment) while allowing legitimate filenames and framework dynamic route segments containing
/// `$` (`app/routes/users.$id.tsx`, `app/routes/$id/page.tsx`, `src/routes/$types.d.ts`,
/// `store$.ts`).
fn has_shell_var_syntax(text: &str) -> bool {
    if text.contains("${") || text.contains("$(") {
        return true;
    }
    for (idx, segment) in text.split(['/', '\\']).enumerate() {
        if let Some(ident) = segment.strip_prefix('$') {
            let valid_ident = !ident.is_empty()
                && ident
                    .chars()
                    .next()
                    .is_some_and(|ch| ch.is_ascii_alphabetic() || ch == '_')
                && ident
                    .chars()
                    .all(|ch| ch.is_ascii_alphanumeric() || ch == '_');
            if valid_ident {
                let all_upper_env = ident.chars().any(|ch| ch.is_ascii_uppercase())
                    && !ident.chars().any(|ch| ch.is_ascii_lowercase());
                if idx == 0 || all_upper_env {
                    return true;
                }
            }
        }
    }
    false
}

fn has_unsafe_path_syntax(text: &str) -> bool {
    text.trim().is_empty()
        || text.contains('\0')
        || text.starts_with('~')
        || text.contains("://")
        || has_uri_scheme_prefix(text)
        || is_windows_drive_relative(text)
        || has_windows_percent_env(text)
        || has_shell_var_syntax(text)
}

/// Resolve `raw_path` relative to `cwd` while following every symlink component, including
/// dangling symlinks whose final target does not exist yet. Fails closed (`None`) on tilde paths,
/// shell/env variable expansion (`$HOME`, `${VAR}`, `$(...)`, `%USERPROFILE%`), URI schemes other
/// than valid local `file://` URIs, Windows drive-relative paths (`C:foo`), symlink loops, NUL
/// bytes, or non-`NotFound` filesystem errors.
pub fn resolve_path_with_symlinks(raw_path: &Path, cwd: &Path) -> Option<PathBuf> {
    let text = raw_path.to_str()?;
    let trimmed = text.trim();
    if trimmed.is_empty() || trimmed.contains('\0') {
        return None;
    }
    let parsed_path = if strip_ascii_prefix_ignore_case(trimmed, "file://").is_some() {
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

    // On Windows, a Prefix component without a RootDir is drive-relative (`C:foo`): fail closed.
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

/// Execution-context directory keys (`cwd`, `workdir`, `root`): validated to stay inside (or equal
/// to) `writable_roots`, but NOT treated as mutation targets themselves (so `cwd: "<project_root>"`
/// does not fail strict `resolved != root`).
fn is_context_dir_key(norm: &str) -> bool {
    matches!(norm, "cwd" | "workdir" | "workingdirectory" | "root" | "rootdir")
}

/// Directory keys that act as execution context on `kind == "edit"` when another target path is
/// present, but stay strict mutation targets on `delete`/`move` (or when alone).
fn is_edit_context_dir_key(norm: &str) -> bool {
    matches!(norm, "directory" | "dir" | "dirpath" | "folder" | "folderpath")
}

/// Allowlisted single-target path keys for file mutation tools.
fn is_single_path_key(norm: &str) -> bool {
    matches!(
        norm,
        "path"
            | "filepath"
            | "notebookpath"
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
            | "location"
    )
}

/// True when `"target"` in `obj` is the old-text snippet of an edit tool (e.g. Antigravity CLI's
/// `edit_file({ Path, Target, Replacement })`) rather than a destination path.
fn target_key_is_edit_snippet(obj: &Map<String, Value>, is_edit_kind: bool) -> bool {
    if !is_edit_kind {
        return false;
    }
    let mut has_explicit_path = false;
    let mut has_replacement_body = false;
    let mut has_source_key = false;
    for key in obj.keys() {
        let norm = canonical_key(key);
        if matches!(
            norm.as_str(),
            "path"
                | "filepath"
                | "notebookpath"
                | "targetfile"
                | "targetpath"
                | "absolutepath"
                | "file"
                | "filename"
                | "uri"
                | "fileuri"
        ) {
            has_explicit_path = true;
        }
        if matches!(
            norm.as_str(),
            "replacement"
                | "replacementcontent"
                | "newstring"
                | "newtext"
                | "newcontent"
                | "newstr"
                | "replace"
                | "after"
                | "newsnippet"
        ) {
            has_replacement_body = true;
        }
        if matches!(
            norm.as_str(),
            "source" | "sourcepath" | "src" | "srcpath" | "sourcefile" | "from" | "frompath"
                | "oldpath" | "oldfile"
        ) {
            has_source_key = true;
        }
    }
    has_explicit_path && has_replacement_body && !has_source_key
}

/// True when `obj` (or already-collected locations) provides an explicit file target path besides
/// `directory`/`dir`/`folder`.
fn has_explicit_file_target(obj: &Map<String, Value>, has_location_paths: bool) -> bool {
    if has_location_paths {
        return true;
    }
    obj.keys().any(|key| {
        let norm = canonical_key(key);
        is_single_path_key(&norm) && !is_edit_context_dir_key(&norm) && norm != "target"
    })
}

/// Allowlisted path-list, edit-chunk, or path-keyed-map keys.
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
            | "replacementchunks"
            | "chunks"
            | "hunks"
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

/// Allowlisted non-path keys (file bodies, replacement text, edit metadata, tool flags).
fn is_known_non_path_key(norm: &str) -> bool {
    is_file_body_key(norm)
        || matches!(
            norm,
            "encoding"
                | "overwrite"
                | "append"
                | "create"
                | "createfile"
                | "isnewfile"
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
                | "targetline"
                | "targetposition"
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
                | "showdiff"
                | "type"
                | "kind"
                | "id"
                | "name"
                | "title"
                | "language"
                | "format"
                | "mimetype"
                | "waitforprevious"
                | "resource"
                | "cellid"
                | "cellnumber"
                | "celltype"
                | "editmode"
                | "expectedreplacements"
                | "occurrence"
        )
}

/// Clean a single unquoted diff path token (stripping trailing `\t` timestamp and `a/` / `b/`).
/// Fails closed (`Err(())`) on quoted paths (`"`), backslash escapes (`\`), or ASCII spaces
/// (which could hide space-delimited timestamps such as `+++ b/evil_link 2026-11-22 15:00:00`).
fn parse_diff_path_token(raw: &str) -> Result<Option<String>, ()> {
    let without_tab = raw.split('\t').next().unwrap_or(raw).trim();
    if without_tab.is_empty() {
        return Err(());
    }
    if without_tab.contains('"')
        || without_tab.contains('\\')
        || without_tab.chars().any(char::is_whitespace)
    {
        return Err(());
    }
    if without_tab == "/dev/null" {
        return Ok(None);
    }
    let stripped = without_tab
        .strip_prefix("a/")
        .or_else(|| without_tab.strip_prefix("b/"))
        .unwrap_or(without_tab)
        .trim();
    if stripped.is_empty() {
        return Err(());
    }
    if stripped == "/dev/null" {
        return Ok(None);
    }
    Ok(Some(stripped.to_string()))
}

/// Parse unified diff hunk header `@@ -a[,b] +c[,d] @@` into `(old_lines, new_lines)`.
fn parse_unified_hunk_counts(line: &str) -> Option<(usize, usize)> {
    let rest = line.strip_prefix("@@ -")?;
    let (ranges, _) = rest.split_once(" @@")?;
    let (old_part, new_part) = ranges.split_once(" +")?;
    let parse_count = |part: &str| -> Option<usize> {
        match part.split_once(',') {
            Some((_, count)) => count.trim().parse::<usize>().ok(),
            None => Some(1),
        }
    };
    Some((parse_count(old_part)?, parse_count(new_part)?))
}

/// Strip `prefix` followed by at least one ASCII space or tab (`--- `, `---\t`, `+++ `, `+++\t`).
fn strip_diff_marker_ws<'a>(line: &'a str, marker: &str) -> Option<&'a str> {
    let rest = line.strip_prefix(marker)?;
    let first = rest.as_bytes().first().copied()?;
    if first == b' ' || first == b'\t' {
        Some(rest.trim_start_matches([' ', '\t']))
    } else {
        None
    }
}

/// Hunk-aware extractor for `apply_patch` headers and unified diff headers.
/// - Ignores hunk body lines so code/comments inside diffs (like SQL `-- ` or strings mentioning
///   `*** Update File:`) are never mistaken for file headers.
/// - Accepts both spaces and tabs after `---`/`+++`.
/// - Fails closed (`false`) on quoted/escaped diff headers, malformed `diff --git` lines, or any
///   non-empty patch yielding zero paths.
fn collect_patch_paths(patch: &str, paths: &mut Vec<String>) -> bool {
    if patch.trim().is_empty() {
        return false;
    }
    let mut found = 0usize;
    let mut in_apply_patch = false;
    let mut in_apply_hunk = false;
    let mut hunk_old_rem: usize = 0;
    let mut hunk_new_rem: usize = 0;
    let mut prev_hunk_line_was_triple_minus = false;

    for raw_line in patch.lines() {
        // Inside a unified diff hunk: consume exact expected old/new lines without scanning body.
        // Fail closed if an inflated hunk hides a `--- ` + `+++ ` file header pair or terminates
        // early at a non-hunk header or EOF with remaining lines.
        if hunk_old_rem > 0 || hunk_new_rem > 0 {
            if prev_hunk_line_was_triple_minus && strip_diff_marker_ws(raw_line, "+++").is_some() {
                return false;
            }
            if raw_line.starts_with('-') {
                if hunk_old_rem == 0 {
                    return false;
                }
                prev_hunk_line_was_triple_minus = strip_diff_marker_ws(raw_line, "---").is_some();
                hunk_old_rem -= 1;
                continue;
            } else if raw_line.starts_with('+') {
                if hunk_new_rem == 0 {
                    return false;
                }
                prev_hunk_line_was_triple_minus = false;
                hunk_new_rem -= 1;
                continue;
            } else if raw_line.starts_with(' ') || raw_line.is_empty() {
                if hunk_old_rem == 0 || hunk_new_rem == 0 {
                    return false;
                }
                prev_hunk_line_was_triple_minus = false;
                hunk_old_rem -= 1;
                hunk_new_rem -= 1;
                continue;
            } else if raw_line.starts_with("\\ No newline") {
                prev_hunk_line_was_triple_minus = false;
                continue;
            }
            // Non-hunk line while counts are still non-zero: inflated/malformed hunk -> fail closed.
            return false;
        }
        prev_hunk_line_was_triple_minus = false;

        let trimmed = raw_line.trim_start();
        if (!in_apply_patch && trimmed == "*** Begin Patch") || raw_line == "*** Begin Patch" {
            in_apply_patch = true;
            in_apply_hunk = false;
            continue;
        }
        // Only match `*** End Patch` at column 0 when inside an `apply_patch` hunk so an indented
        // context line `" *** End Patch"` never exits hunk tracking prematurely.
        if (in_apply_hunk && raw_line == "*** End Patch")
            || (!in_apply_hunk && trimmed == "*** End Patch")
        {
            in_apply_patch = false;
            in_apply_hunk = false;
            continue;
        }

        // `apply_patch` file header lines start at column 0 (or after Begin Patch, never inside a
        // `+`/`-`/` ` hunk body line).
        if !in_apply_hunk || raw_line.starts_with("*** ") {
            let mut matched_apply = false;
            for prefix in [
                "*** Update File:",
                "*** Add File:",
                "*** Delete File:",
                "*** Move to:",
            ] {
                let candidate = if in_apply_patch { raw_line } else { trimmed };
                if let Some(rest) = candidate.strip_prefix(prefix) {
                    let target = rest.trim();
                    if target.is_empty() || target.contains('"') {
                        return false;
                    }
                    push_unique(paths, target);
                    found += 1;
                    in_apply_patch = true;
                    in_apply_hunk = false;
                    matched_apply = true;
                    break;
                }
            }
            if matched_apply {
                continue;
            }
        }

        if in_apply_patch {
            if raw_line.starts_with("@@") {
                in_apply_hunk = true;
            }
            continue;
        }

        // Unified diff hunk header.
        if let Some((old_c, new_c)) = parse_unified_hunk_counts(raw_line) {
            hunk_old_rem = old_c;
            hunk_new_rem = new_c;
            continue;
        }

        // Unified diff `diff --git a/foo b/bar` (strictly 2 unquoted tokens; fail closed otherwise).
        if let Some(rest) = raw_line.strip_prefix("diff --git ") {
            if rest.contains('"') || rest.contains('\\') {
                return false;
            }
            let tokens: Vec<&str> = rest.split_whitespace().collect();
            if tokens.len() != 2 {
                return false;
            }
            for token in tokens {
                match parse_diff_path_token(token) {
                    Ok(Some(p)) => {
                        push_unique(paths, &p);
                        found += 1;
                    }
                    Ok(None) => {}
                    Err(()) => return false,
                }
            }
            continue;
        }

        // Unified diff `---` / `+++` headers (space OR tab after marker).
        if let Some(rest) =
            strip_diff_marker_ws(raw_line, "---").or_else(|| strip_diff_marker_ws(raw_line, "+++"))
        {
            match parse_diff_path_token(rest) {
                Ok(Some(p)) => {
                    push_unique(paths, &p);
                    found += 1;
                }
                Ok(None) => {}
                Err(()) => return false,
            }
            continue;
        }

        // Git rename/copy headers.
        for prefix in ["rename from ", "rename to ", "copy from ", "copy to "] {
            if let Some(rest) = raw_line.strip_prefix(prefix) {
                match parse_diff_path_token(rest) {
                    Ok(Some(p)) => {
                        push_unique(paths, &p);
                        found += 1;
                    }
                    Ok(None) => {}
                    Err(()) => return false,
                }
                break;
            }
        }
    }

    if hunk_old_rem > 0 || hunk_new_rem > 0 {
        return false;
    }

    found > 0
}

#[derive(Default)]
struct ExtractedTargets {
    context_dirs: Vec<String>,
    location_paths: Vec<String>,
    raw_input_paths: Vec<String>,
}

fn collect_allowlisted_value(
    value: &Value,
    is_edit_kind: bool,
    is_patch_tool: bool,
    in_raw_input: bool,
    out: &mut ExtractedTargets,
    depth: usize,
) -> bool {
    if depth > MAX_RAW_INPUT_DEPTH {
        return false;
    }
    match value {
        Value::Null => true,
        Value::String(text) if depth == 0 && in_raw_input => {
            let trimmed = text.trim();
            if trimmed.is_empty() {
                return true;
            }
            if trimmed.starts_with('{') || trimmed.starts_with('[') {
                let Ok(parsed) = serde_json::from_str::<Value>(trimmed) else {
                    return false;
                };
                return collect_allowlisted_value(
                    &parsed,
                    is_edit_kind,
                    is_patch_tool,
                    in_raw_input,
                    out,
                    depth + 1,
                );
            }
            collect_patch_paths(trimmed, &mut out.raw_input_paths)
        }
        Value::Array(items) => {
            for item in items {
                match item {
                    Value::String(s) => {
                        let trimmed = s.trim();
                        if trimmed.is_empty() {
                            return false;
                        }
                        push_unique(&mut out.raw_input_paths, trimmed);
                    }
                    Value::Object(_) | Value::Array(_) => {
                        if !collect_allowlisted_value(
                            item,
                            is_edit_kind,
                            is_patch_tool,
                            in_raw_input,
                            out,
                            depth + 1,
                        ) {
                            return false;
                        }
                    }
                    _ => return false,
                }
            }
            true
        }
        Value::Object(obj) => {
            let is_diff_block = obj
                .get("type")
                .and_then(Value::as_str)
                .is_some_and(|t| t.eq_ignore_ascii_case("diff"));
            let before_count = out.raw_input_paths.len();
            let target_is_snippet = target_key_is_edit_snippet(obj, is_edit_kind);
            let dir_is_context =
                is_edit_kind && has_explicit_file_target(obj, !out.location_paths.is_empty());

            for (key, val) in obj {
                let norm = canonical_key(key);
                if is_context_dir_key(&norm) || (dir_is_context && is_edit_context_dir_key(&norm)) {
                    match val {
                        Value::Null => {}
                        Value::String(s) => {
                            let trimmed = s.trim();
                            if trimmed.is_empty() {
                                return false;
                            }
                            push_unique(&mut out.context_dirs, trimmed);
                        }
                        _ => return false,
                    }
                } else if target_is_snippet && norm == "target" {
                    // Antigravity CLI `edit_file({ Path, Target, Replacement })`: `Target` is the
                    // old code snippet to replace, not a filesystem path.
                    if !matches!(val, Value::Null | Value::String(_)) {
                        return false;
                    }
                } else if is_single_path_key(&norm) {
                    match val {
                        Value::Null => {}
                        Value::String(s) => {
                            let trimmed = s.trim();
                            if trimmed.is_empty() {
                                return false;
                            }
                            push_unique(&mut out.raw_input_paths, trimmed);
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
                                        push_unique(&mut out.raw_input_paths, trimmed);
                                    }
                                    Value::Object(_) | Value::Array(_) => {
                                        if !collect_allowlisted_value(
                                            item,
                                            is_edit_kind,
                                            is_patch_tool,
                                            in_raw_input,
                                            out,
                                            depth + 1,
                                        ) {
                                            return false;
                                        }
                                    }
                                    _ => return false,
                                }
                            }
                        }
                        Value::Object(map) => {
                            for (entry_key, entry_val) in map {
                                let trimmed_key = entry_key.trim();
                                if trimmed_key.is_empty() {
                                    return false;
                                }
                                push_unique(&mut out.raw_input_paths, trimmed_key);
                                match entry_val {
                                    Value::Null => {}
                                    Value::String(s) => match norm.as_str() {
                                        "moves" | "renames" => {
                                            let dest = s.trim();
                                            if dest.is_empty() {
                                                return false;
                                            }
                                            push_unique(&mut out.raw_input_paths, dest);
                                        }
                                        "patches" | "diffs" => {
                                            if !collect_patch_paths(s, &mut out.raw_input_paths) {
                                                return false;
                                            }
                                        }
                                        "changes" | "edits" | "files" => {}
                                        _ => return false,
                                    },
                                    Value::Object(_) | Value::Array(_) => {
                                        if !collect_allowlisted_value(
                                            entry_val,
                                            is_edit_kind,
                                            is_patch_tool,
                                            in_raw_input,
                                            out,
                                            depth + 1,
                                        ) {
                                            return false;
                                        }
                                    }
                                    _ => return false,
                                }
                            }
                        }
                        _ => return false,
                    }
                } else if is_patch_key(&norm) {
                    match val {
                        Value::Null => {}
                        Value::String(s) => {
                            if !collect_patch_paths(s, &mut out.raw_input_paths) {
                                return false;
                            }
                        }
                        Value::Object(_) | Value::Array(_) => {
                            if !collect_allowlisted_value(
                                val,
                                is_edit_kind,
                                true,
                                in_raw_input,
                                out,
                                depth + 1,
                            ) {
                                return false;
                            }
                        }
                        _ => return false,
                    }
                } else if is_known_non_path_key(&norm) {
                    match val {
                        Value::String(s)
                            if in_raw_input
                                && is_patch_tool
                                && matches!(norm.as_str(), "content" | "text" | "body" | "data") =>
                        {
                            if !collect_patch_paths(s, &mut out.raw_input_paths) {
                                return false;
                            }
                        }
                        Value::Object(_) | Value::Array(_) => {
                            if !collect_allowlisted_value(
                                val,
                                is_edit_kind,
                                is_patch_tool,
                                in_raw_input,
                                out,
                                depth + 1,
                            ) {
                                return false;
                            }
                        }
                        _ => {}
                    }
                } else {
                    // Allowlist rule: non-string scalar metadata (bool/number/null) on unrecognized
                    // keys cannot carry a path; ANY unrecognized string, object, or array fails
                    // closed and shows the approval card.
                    match val {
                        Value::Null | Value::Bool(_) | Value::Number(_) => {}
                        Value::String(_) | Value::Array(_) | Value::Object(_) => return false,
                    }
                }
            }

            if is_diff_block && out.raw_input_paths.len() == before_count {
                return false;
            }
            true
        }
        _ => false,
    }
}

fn extract_mutation_targets(params: &Value) -> Option<ExtractedTargets> {
    let tool_call = params.get("toolCall")?.as_object()?;
    let kind_lower = tool_call
        .get("kind")
        .and_then(Value::as_str)
        .map(|s| s.trim().to_ascii_lowercase())
        .unwrap_or_default();
    let is_edit_kind = kind_lower == "edit";
    let tool_name_lower = tool_call
        .get("name")
        .or_else(|| tool_call.get("title"))
        .and_then(Value::as_str)
        .map(|s| s.to_ascii_lowercase())
        .unwrap_or_default();
    let is_patch_tool = tool_name_lower.contains("patch") || tool_name_lower.contains("diff");

    let mut out = ExtractedTargets::default();

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
                push_unique(&mut out.location_paths, path);
                if let Some(uri) = obj.get("uri") {
                    if !uri.is_null() {
                        let uri_str = uri
                            .as_str()
                            .map(str::trim)
                            .filter(|s| !s.is_empty())?;
                        push_unique(&mut out.location_paths, uri_str);
                    }
                }
            }
        }
    }

    if let Some(content) = tool_call.get("content") {
        if !content.is_null() {
            match content {
                Value::Array(_) | Value::Object(_) => {
                    if !collect_allowlisted_value(
                        content,
                        is_edit_kind,
                        is_patch_tool,
                        false,
                        &mut out,
                        1,
                    ) {
                        return None;
                    }
                }
                _ => return None,
            }
        }
    }

    if let Some(raw_input) = tool_call.get("rawInput") {
        if !raw_input.is_null()
            && !collect_allowlisted_value(
                raw_input,
                is_edit_kind,
                is_patch_tool,
                true,
                &mut out,
                0,
            )
        {
            return None;
        }
    }

    if out.location_paths.is_empty() && out.raw_input_paths.is_empty() {
        return None;
    }
    Some(out)
}

fn path_strictly_inside_roots(raw: &str, base: &Path, writable_roots: &[PathBuf]) -> bool {
    let Some(resolved) = resolve_path_with_symlinks(Path::new(raw), base) else {
        return false;
    };
    writable_roots
        .iter()
        .any(|root| resolved != *root && resolved.starts_with(root))
}

/// True only when path extraction accounts for every mutation target using allowlisted shapes,
/// any execution-context directory (`cwd`/`workdir`/`root`) stays inside or equal to
/// `writable_roots`, every `toolCall.locations` path resolves strictly inside `writable_roots`
/// against session `cwd` (never masked by `rawInput.cwd`), AND every `rawInput`/`content` path
/// resolves strictly inside `writable_roots` against BOTH session `cwd` and any `rawInput` context
/// directory.
pub fn file_mutation_stays_inside_roots(
    params: &Value,
    writable_roots: &[PathBuf],
    cwd: &Path,
) -> bool {
    if writable_roots.is_empty() {
        return false;
    }
    let Some(targets) = extract_mutation_targets(params) else {
        return false;
    };
    let canonical_cwd = resolve_path_with_symlinks(cwd, cwd).unwrap_or_else(|| cwd.to_path_buf());

    // Validate every execution-context directory (`cwd`, `workdir`, `root`, or edit `directory`):
    // allowed to equal a writable root; multiple differing context dirs fail closed.
    let mut resolved_ctx: Option<PathBuf> = None;
    for raw_ctx in &targets.context_dirs {
        let Some(resolved) = resolve_path_with_symlinks(Path::new(raw_ctx), &canonical_cwd) else {
            return false;
        };
        if !writable_roots
            .iter()
            .any(|root| resolved == *root || resolved.starts_with(root))
        {
            return false;
        }
        if let Some(prev) = &resolved_ctx {
            if prev != &resolved {
                return false;
            }
        } else {
            resolved_ctx = Some(resolved);
        }
    }

    // `toolCall.locations` are always resolved strictly against session `cwd`.
    if !targets
        .location_paths
        .iter()
        .all(|raw| path_strictly_inside_roots(raw, &canonical_cwd, writable_roots))
    {
        return false;
    }

    // `rawInput` / `content` paths must stay strictly inside roots against `canonical_cwd` AND
    // (when present) against `resolved_ctx`, so `rawInput.cwd` can never mask `../` escapes.
    targets.raw_input_paths.iter().all(|raw| {
        if !path_strictly_inside_roots(raw, &canonical_cwd, writable_roots) {
            return false;
        }
        if let Some(ctx_base) = &resolved_ctx {
            if !path_strictly_inside_roots(raw, ctx_base, writable_roots) {
                return false;
            }
        }
        true
    })
}
