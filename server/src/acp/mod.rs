//! Provider-agnostic ACP session support (`ghostex acp-host` running inside `zmx`).
//!
//! CDXC:AgentProviders 2026-11-22 DECISION:
//! User: keep CLI + `zmx` as the default for every agent, trial Antigravity on ACP behind an
//! opt-in per-agent toggle (with Codex expected next), preserve reload survival across `gxserver`
//! reloads and app rebuilds, and show a confirmation clarifying that switching an ACP session to
//! Terminal takes a moment and interrupts any running turn.
//!
//! Running `ghostex acp-host` inside an ordinary `zmx` session keeps every existing session
//! lifecycle contract (sleep, wake, close, Windows/Linux/macOS, remote hosts, older phones)
//! intact while giving `gxserver` structured send, cancel, approval, model/mode, and in-place
//! CLI handoff doors over the existing `/api/agentChatChannel` long-poll channel.

use std::fs::{self, File};
use std::io::Read as _;
use std::path::{Path, PathBuf};

use serde_json::Value;

use crate::domain::DomainStateError;
use crate::paths::GxserverPaths;
use crate::session_chat_files::SESSION_CHAT_IMAGE_MAX_BYTES;

pub mod approval;
pub(crate) mod approval_migration;
pub(crate) mod channel;
mod child_lifetime;
mod cli_default;
mod driver_claude;
mod driver_codex;
pub(crate) mod driver_cursor;
mod driver_grok;
pub(crate) mod driver_opencode;
mod driver_pi;
mod elicitation;
pub(crate) mod hermes;
pub mod host;
pub(crate) mod host_mode_switching;
pub(crate) mod ignored_flags;
mod omp_elicitation;
pub(crate) mod pending_switch;
pub(crate) mod spec;

/// What every Native host says it is in its handshake (`clientInfo`): Ghostex, by its own name
/// and the app's real version.
///
/// CDXC:AgentProviders 2026-11-26 DECISION:
/// User, on the Antigravity compliance page (Q5150): Ghostex always identifies as itself, never as
/// another editor or client, and with the app's version (it sent the server crate's `0.1.0`).
pub(crate) fn client_info() -> Value {
    serde_json::json!({
        "name": "ghostex",
        "title": "Ghostex",
        "version": crate::constants::GXSERVER_VERSION,
    })
}
pub(crate) mod startup_error;

pub use approval::{AcpApprovalLevel, SESSION_ACP_APPROVAL_LEVEL_KEY};
pub(crate) use child_lifetime::ensure_dead_host_child_stopped;
pub use child_lifetime::run_watcher_command;

/// Settings key (comma-separated canonical agent families, e.g. `"claude,codex"`) recording
/// per-agent dismissals of the chat strip's "Switch to Native" offer (`Q4770`).
pub const NATIVE_SWITCH_OFFER_DISMISSED_KEY: &str = "nativeSwitchOfferDismissedAgents";

/// Whether a live `ghostex acp-host` child process is running inside `zmx_name`.
/// Excludes `tree.root` (the `zmx` daemon itself) because its argv can retain the original
/// startup command containing `"acp-host"` even after the host exited back to the shell.
/// Unreadable process trees count as running so transient read errors never trigger a restart.
pub(crate) fn host_process_running(zmx_name: &str) -> bool {
    match crate::zmx::read_zmx_session_tree(zmx_name) {
        Ok(Some(tree)) => tree
            .processes
            .iter()
            .any(|process| process.pid != tree.root && process.command.contains("acp-host")),
        Ok(None) => false,
        Err(()) => true,
    }
}

/// Canonical Native-capable agent family (`"antigravity"`, `"claude"`, `"codex"`, or `"cursor"`).
pub fn canonical_native_family(agent_family: &str) -> Option<&'static str> {
    spec::spec_for_family(agent_family).map(|spec| spec.family)
}

/// Canonical families whose "Switch to Native" strip offer the user dismissed in Settings.
pub fn dismissed_native_switch_families(
    settings: &serde_json::Map<String, Value>,
) -> Vec<&'static str> {
    let mut out = Vec::new();
    match settings.get(NATIVE_SWITCH_OFFER_DISMISSED_KEY) {
        Some(Value::String(raw)) => {
            for part in raw.split(',') {
                if let Some(family) = canonical_native_family(part) {
                    if !out.contains(&family) {
                        out.push(family);
                    }
                }
            }
        }
        Some(Value::Array(items)) => {
            for item in items {
                if let Some(family) = item.as_str().and_then(canonical_native_family) {
                    if !out.contains(&family) {
                        out.push(family);
                    }
                }
            }
        }
        Some(Value::Object(map)) => {
            for (key, value) in map {
                if value.as_bool() == Some(true) {
                    if let Some(family) = canonical_native_family(key) {
                        if !out.contains(&family) {
                            out.push(family);
                        }
                    }
                }
            }
        }
        _ => {}
    }
    out
}

/// Settings key (object map of `<agentFamily>: "non-cli" | "agent-cli"`) storing per-agent
/// session mode (`Agent CLI` default vs `Non-CLI`).
pub const SESSION_MODE_SETTING_KEY: &str = "agentSessionMode";

/// Legacy settings key (`{ "<agentFamily>": true }`) still read as `"non-cli"` when
/// `agentSessionMode` has no entry for that family.
pub const ACP_MODE_SETTING_KEY: &str = "agentAcpMode";

/// Session `runtimeSettings` key marking a session currently running in Non-CLI mode.
pub const SESSION_NON_CLI_ACTIVE_KEY: &str = "nonCliMode";

/// Legacy session `runtimeSettings` key kept in sync with `nonCliMode` for ACP-hosted sessions.
pub const SESSION_ACP_ACTIVE_KEY: &str = "acpMode";

/// Session `runtimeSettings` key naming the Non-CLI agent family (`antigravity`, future `codex`, ...).
pub const SESSION_ACP_FAMILY_KEY: &str = "acpFamily";

/// Session `runtimeSettings` key holding extra owner-configured command flags/env vars that
/// Non-CLI mode ignored, so the chat UI can display a visible one-line info notice.
pub const SESSION_NON_CLI_IGNORED_COMMAND_KEY: &str = "nonCliIgnoredCommand";

/// Whether Ghostex currently offers a Non-CLI host for `agent_family`.
pub fn supports_acp(agent_family: &str) -> bool {
    spec::spec_for_family(agent_family).is_some()
}

/// Explicit per-agent Non-CLI session mode setting from `settings`, or `None` when not set there.
pub fn agent_acp_setting(
    settings: &serde_json::Map<String, Value>,
    agent_family: &str,
) -> Option<bool> {
    let canonical = canonical_native_family(agent_family)?;
    if let Some(mode) = settings
        .get(SESSION_MODE_SETTING_KEY)
        .and_then(Value::as_object)
        .and_then(|map| map.get(canonical).or_else(|| map.get(agent_family)))
        .and_then(Value::as_str)
    {
        return Some(mode.eq_ignore_ascii_case("non-cli"));
    }
    settings
        .get(ACP_MODE_SETTING_KEY)
        .and_then(Value::as_object)
        .and_then(|map| map.get(canonical).or_else(|| map.get(agent_family)))
        .and_then(Value::as_bool)
}

/// Whether the user has opted `agent_family` into Non-CLI mode in Settings.
pub fn agent_acp_enabled(settings: &serde_json::Map<String, Value>, agent_family: &str) -> bool {
    agent_acp_setting(settings, agent_family) == Some(true)
}

/// Reads `agentSessionMode.<agent_family>` (falling back to legacy `agentAcpMode`) from Ghostex's
/// shared settings file (`native-sidebar-settings.json`).
pub fn agent_acp_enabled_in_shared_settings(agent_family: &str) -> bool {
    if !supports_acp(agent_family) {
        return false;
    }
    let paths = crate::paths::get_gxserver_paths(None);
    crate::shared_settings_file::read_shared_settings(&paths)
        .and_then(|value| value.as_object().cloned())
        .is_some_and(|map| agent_acp_enabled(&map, agent_family))
}

/// Canonicalized Ghostex-owned attachment directories (`app_data_dir/i` and `app_data_dir/f`) that
/// Non-CLI image sends may read from. Excludes daemon state, cache, runtime, and arbitrary paths.
pub(crate) fn allowed_acp_image_dirs(paths: &GxserverPaths) -> Vec<PathBuf> {
    [paths.app_data_dir.join("i"), paths.app_data_dir.join("f")]
        .into_iter()
        .filter_map(|dir| fs::canonicalize(dir).ok())
        .collect()
}

/// Identify a supported raster image MIME type strictly from its leading magic bytes (never from
/// the file extension alone).
pub(crate) fn image_mime_from_magic(header: &[u8]) -> Option<&'static str> {
    if header.len() >= 8 && header.starts_with(b"\x89PNG\r\n\x1a\n") {
        return Some("image/png");
    }
    if header.len() >= 3 && header.starts_with(b"\xff\xd8\xff") {
        return Some("image/jpeg");
    }
    if header.len() >= 6 && (header.starts_with(b"GIF87a") || header.starts_with(b"GIF89a")) {
        return Some("image/gif");
    }
    if header.len() >= 12 && &header[0..4] == b"RIFF" && &header[8..12] == b"WEBP" {
        return Some("image/webp");
    }
    if header.len() >= 2 && header.starts_with(b"BM") {
        return Some("image/bmp");
    }
    if header.len() >= 12 && (&header[4..12] == b"ftypavif" || &header[4..12] == b"ftypavis") {
        return Some("image/avif");
    }
    None
}

/// Validate one ACP image path: canonicalize, require containment in Ghostex's attachment/temp
/// directories, require a regular file within the 32 MiB attachment cap, and verify image magic
/// bytes before reading the body.
pub(crate) fn validate_acp_image_path(
    paths: &GxserverPaths,
    raw_path: &str,
) -> Result<(PathBuf, &'static str), String> {
    let trimmed = raw_path.trim();
    if trimmed.is_empty() {
        return Err("Image path cannot be empty.".to_string());
    }
    let canonical = fs::canonicalize(Path::new(trimmed))
        .map_err(|_| "Image attachment does not exist on this machine.".to_string())?;
    let allowed_dirs = allowed_acp_image_dirs(paths);
    if !allowed_dirs.iter().any(|dir| canonical.starts_with(dir)) {
        return Err(
            "Image attachments for Native sessions must live inside Ghostex's attachment directory."
                .to_string(),
        );
    }
    let metadata =
        fs::metadata(&canonical).map_err(|_| "Could not inspect image attachment.".to_string())?;
    if !metadata.is_file() {
        return Err("Image attachment must be a regular file.".to_string());
    }
    let max_bytes = SESSION_CHAT_IMAGE_MAX_BYTES as u64;
    if metadata.len() == 0 || metadata.len() > max_bytes {
        return Err(format!(
            "Image attachment must be between 1 byte and {} MiB.",
            SESSION_CHAT_IMAGE_MAX_BYTES / (1024 * 1024)
        ));
    }
    let mut file =
        File::open(&canonical).map_err(|_| "Could not open image attachment.".to_string())?;
    let mut header = [0u8; 16];
    let read_len = file
        .read(&mut header)
        .map_err(|_| "Could not read image attachment header.".to_string())?;
    let mime = image_mime_from_magic(&header[..read_len])
        .ok_or_else(|| "Image attachment is not a recognized image file.".to_string())?;
    Ok((canonical, mime))
}

/// Validate all `image_paths` for an ACP send and return their canonical paths.
pub(crate) fn validate_acp_image_paths(
    paths: &GxserverPaths,
    image_paths: &[String],
) -> Result<Vec<String>, DomainStateError> {
    let mut validated = Vec::with_capacity(image_paths.len());
    for raw in image_paths {
        let (canonical, _) =
            validate_acp_image_path(paths, raw).map_err(|message| DomainStateError {
                code: "invalidParams",
                message,
            })?;
        validated.push(canonical.to_string_lossy().into_owned());
    }
    Ok(validated)
}

/// Defensively validate and read a bounded image file inside `acp-host` before base64-encoding it.
pub(crate) fn read_validated_acp_image(
    paths: &GxserverPaths,
    raw_path: &str,
) -> Result<(Vec<u8>, &'static str), String> {
    let (canonical, _) = validate_acp_image_path(paths, raw_path)?;
    let max_bytes = SESSION_CHAT_IMAGE_MAX_BYTES as u64;
    let file =
        File::open(&canonical).map_err(|_| "Could not open image attachment.".to_string())?;
    let mut bytes = Vec::new();
    file.take(max_bytes + 1)
        .read_to_end(&mut bytes)
        .map_err(|_| "Could not read image attachment.".to_string())?;
    if bytes.is_empty() || bytes.len() as u64 > max_bytes {
        return Err("Image attachment exceeds the 32 MiB limit.".to_string());
    }
    let mime = image_mime_from_magic(&bytes)
        .ok_or_else(|| "Image attachment is not a recognized image file.".to_string())?;
    Ok((bytes, mime))
}

/// True while this session is running in Non-CLI mode (cleared once switched to Agent CLI).
pub fn session_is_non_cli(session: &Value) -> bool {
    session
        .pointer("/runtimeSettings/nonCliMode")
        .and_then(Value::as_bool)
        .or_else(|| {
            session
                .pointer("/runtimeSettings/acpMode")
                .and_then(Value::as_bool)
        })
        == Some(true)
}

/// True while this session is running through Ghostex's ACP Non-CLI host.
pub fn session_is_acp(session: &Value) -> bool {
    session_is_non_cli(session)
}

/// Resolve the canonical Native family for a session row, distinguishing OMP from Pi even though
/// both share the `"pi"` transcript decoder.
pub(crate) fn resolve_native_family_for_session(session: &Value) -> Option<String> {
    session
        .pointer("/runtimeSettings/acpFamily")
        .and_then(Value::as_str)
        .and_then(canonical_native_family)
        .or_else(|| {
            crate::session_chat_composer::session_chat_composer_agent_id(session)
                .as_deref()
                .and_then(canonical_native_family)
        })
        .or_else(|| {
            crate::session_chat_follower::session_chat_agent_for_session(session)
                .as_deref()
                .and_then(canonical_native_family)
        })
        .map(str::to_string)
}

/// The ACP family for a session currently running in ACP mode.
pub fn session_acp_family(session: &Value) -> Option<String> {
    if !session_is_acp(session) {
        return None;
    }
    resolve_native_family_for_session(session)
        .or_else(|| crate::session_chat_follower::session_chat_agent_for_session(session))
}

/// Extract the canonical Native family from an `/api/agentChatChannel` agent name (`"acp:claude"`).
pub fn acp_family_from_channel_agent(channel_agent: &str) -> Option<String> {
    channel_agent
        .trim()
        .strip_prefix("acp:")
        .and_then(canonical_native_family)
        .map(str::to_string)
}

fn extract_acp_flag_value<'a>(command: &'a str, flag: &str) -> Option<&'a str> {
    let acp_pos = command.find("acp-host")?;
    // Ensure `acp-host` is a standalone token, not part of another identifier.
    if acp_pos > 0 {
        let prev = command.as_bytes()[acp_pos - 1];
        if !prev.is_ascii_whitespace() && prev != b'"' && prev != b'\'' && prev != b';' {
            return None;
        }
    }
    let after_host = &command[acp_pos + "acp-host".len()..];
    let mut tokens = after_host.split_whitespace();
    while let Some(tok) = tokens.next() {
        let cleaned = tok.trim_matches(|c| c == '\'' || c == '"' || c == ';');
        if cleaned == flag {
            let val = tokens.next()?;
            let trimmed = val.trim_matches(|c| c == '\'' || c == '"' || c == ';');
            if !trimmed.is_empty() {
                return Some(trimmed);
            }
            return None;
        }
        if let Some(rest) = cleaned.strip_prefix(&format!("{flag}=")) {
            let trimmed = rest.trim_matches(|c| c == '\'' || c == '"' || c == ';');
            if !trimmed.is_empty() {
                return Some(trimmed);
            }
            return None;
        }
    }
    None
}

/// Extract the canonical Native agent family from a launch/resume startup command that invokes
/// `ghostex acp-host --agent <family>` (including wrapped restore scripts and PowerShell forms).
pub fn acp_family_from_launch_command(command: &str) -> Option<String> {
    extract_acp_flag_value(command, "--agent")
        .and_then(canonical_native_family)
        .map(str::to_string)
}

/// Extract the `--approval-level <level>` value from an `acp-host` launch/resume command.
pub fn acp_approval_level_from_launch_command(command: &str) -> Option<AcpApprovalLevel> {
    acp_family_from_launch_command(command)?;
    extract_acp_flag_value(command, "--approval-level").and_then(AcpApprovalLevel::parse)
}

/// Persist `nonCliMode: true`, `acpMode: true`, and `acpFamily` into `runtime_settings` so the
/// stored session row matches a running `acp-host` process. Preserves any stored `acpApprovalLevel`,
/// seeding it from `approval_level` only when absent. Returns `true` when any key changed.
pub fn apply_acp_runtime_settings(
    runtime_settings: &mut serde_json::Map<String, Value>,
    family: &str,
    approval_level: Option<AcpApprovalLevel>,
) -> bool {
    let mut changed = false;
    if runtime_settings
        .get(SESSION_NON_CLI_ACTIVE_KEY)
        .and_then(Value::as_bool)
        != Some(true)
    {
        runtime_settings.insert(SESSION_NON_CLI_ACTIVE_KEY.to_string(), Value::Bool(true));
        changed = true;
    }
    if runtime_settings
        .get(SESSION_ACP_ACTIVE_KEY)
        .and_then(Value::as_bool)
        != Some(true)
    {
        runtime_settings.insert(SESSION_ACP_ACTIVE_KEY.to_string(), Value::Bool(true));
        changed = true;
    }
    if runtime_settings
        .get(SESSION_ACP_FAMILY_KEY)
        .and_then(Value::as_str)
        != Some(family)
    {
        runtime_settings.insert(
            SESSION_ACP_FAMILY_KEY.to_string(),
            Value::String(family.to_string()),
        );
        changed = true;
    }
    if let Some(level) = approval_level {
        let has_level = runtime_settings
            .get(SESSION_ACP_APPROVAL_LEVEL_KEY)
            .and_then(Value::as_str)
            .and_then(AcpApprovalLevel::parse)
            .is_some();
        if !has_level {
            runtime_settings.insert(
                SESSION_ACP_APPROVAL_LEVEL_KEY.to_string(),
                Value::String(level.as_str().to_string()),
            );
            changed = true;
        }
    }
    changed
}
