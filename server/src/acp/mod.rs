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

pub(crate) mod channel;
pub mod host;
pub(crate) mod spec;

/// Settings key (object map of `<agentFamily>: bool`) storing per-agent ACP opt-in.
pub const ACP_MODE_SETTING_KEY: &str = "agentAcpMode";

/// Session `runtimeSettings` key marking a session currently running through `ghostex acp-host`.
pub const SESSION_ACP_ACTIVE_KEY: &str = "acpMode";

/// Session `runtimeSettings` key naming the ACP agent family (`antigravity`, future `codex`, ...).
pub const SESSION_ACP_FAMILY_KEY: &str = "acpFamily";

/// Whether Ghostex currently offers an ACP transport for `agent_family`.
pub fn supports_acp(agent_family: &str) -> bool {
    spec::spec_for_family(agent_family).is_some()
}

/// Explicit per-agent ACP opt-in setting from `settings`, or `None` when not set there.
pub fn agent_acp_setting(
    settings: &serde_json::Map<String, Value>,
    agent_family: &str,
) -> Option<bool> {
    if !supports_acp(agent_family) {
        return None;
    }
    settings
        .get(ACP_MODE_SETTING_KEY)
        .and_then(Value::as_object)
        .and_then(|map| map.get(agent_family))
        .and_then(Value::as_bool)
}

/// Whether the user has opted `agent_family` into ACP mode in Settings.
pub fn agent_acp_enabled(settings: &serde_json::Map<String, Value>, agent_family: &str) -> bool {
    agent_acp_setting(settings, agent_family) == Some(true)
}

/// Reads `agentAcpMode.<agent_family>` from Ghostex's shared settings file (`native-sidebar-settings.json`).
pub fn agent_acp_enabled_in_shared_settings(agent_family: &str) -> bool {
    if !supports_acp(agent_family) {
        return false;
    }
    let paths = crate::paths::get_gxserver_paths(None);
    crate::shared_settings_file::read_shared_settings(&paths)
        .and_then(|value| value.as_object().cloned())
        .is_some_and(|map| agent_acp_enabled(&map, agent_family))
}

/// Canonicalized Ghostex-owned attachment and temp directories that ACP image sends may read from.
/// Excludes daemon state roots (`root_dir`, `auth_dir`, `state.db`) and arbitrary user/system paths.
pub(crate) fn allowed_acp_image_dirs(paths: &GxserverPaths) -> Vec<PathBuf> {
    [
        paths.app_data_dir.join("i"),
        paths.app_data_dir.join("f"),
        paths.app_cache_dir.clone(),
        paths.runtime_dir.clone(),
    ]
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
            "Image attachments for ACP sessions must live inside Ghostex's attachment directory."
                .to_string(),
        );
    }
    let metadata = fs::metadata(&canonical)
        .map_err(|_| "Could not inspect image attachment.".to_string())?;
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
    let file = File::open(&canonical).map_err(|_| "Could not open image attachment.".to_string())?;
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

/// True while this session is running in ACP mode (cleared once switched to interactive CLI).
pub fn session_is_acp(session: &Value) -> bool {
    session
        .pointer("/runtimeSettings/acpMode")
        .and_then(Value::as_bool)
        == Some(true)
}

/// The ACP family for a session currently running in ACP mode.
pub fn session_acp_family(session: &Value) -> Option<String> {
    if !session_is_acp(session) {
        return None;
    }
    session
        .pointer("/runtimeSettings/acpFamily")
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(str::to_string)
        .or_else(|| crate::session_chat_follower::session_chat_agent_for_session(session))
}
