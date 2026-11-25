//! The approval level an agent CLI's own settings start it in, for Native sessions under Keep default.
//!
//! CDXC:NativeApprovals 2026-11-25 DECISION:
//! Brief (#742, "Off keeps each agent CLI default approval mode"): under Settings > Agents > Agent approvals = Keep default (no `--approval-level` on `ghostex acp-host`), a Native session starts where the agent's own settings start it, the way Terminal-based does. Claude and Codex do it by leaving their own flags out (`driver_claude.rs`, `driver_codex.rs`). The drivers below route every approval through Ghostex so a later Mode-menu pick takes effect at once, so they read the CLI's own setting here and start at the matching level instead of a forced Ask: Antigravity `toolPermission: always-proceed` (its Native host runs under `--cli_dir_name ghostex-acp`, which hides that setting from `agy`), Grok `[ui] permission_mode` (the mode its TUI starts in; `grok agent` alone always starts in ask), Cursor `approvalMode: unrestricted` (its Native host writes a private `cli-config.json`), and Pi, which has no approvals of its own and so runs every tool. The level is never saved on the session, so it follows the CLI's settings on every start until the user picks one in the Mode menu.
//! SEE-ALSO: server/src/acp/host.rs, server/src/acp/driver_grok/run.rs,
//! server/src/acp/driver_cursor/mod.rs, server/src/acp/driver_pi/mod.rs,
//! server/src/acp/driver_opencode/approvals.rs (OpenCode keeps its own `permission` rules instead).

use std::fs;
use std::path::PathBuf;

use serde_json::Value;

use super::AcpApprovalLevel;

/// The level `family`'s own settings start it in, or `None` when they ask before acting (or say
/// nothing Ghostex can read), so the session follows what the agent itself asks about.
pub(crate) fn cli_default_level(family: &str) -> Option<AcpApprovalLevel> {
    match family {
        "antigravity" => antigravity_level(),
        "grok" => grok_level(),
        "cursor" => super::driver_cursor::user_cli_default_level(),
        "pi" => Some(AcpApprovalLevel::FullAccess),
        _ => None,
    }
}

fn antigravity_level() -> Option<AcpApprovalLevel> {
    let path =
        crate::session_chat_antigravity_mirror::antigravity_app_data_dir().join("settings.json");
    let settings: Value = serde_json::from_str(&fs::read_to_string(path).ok()?).ok()?;
    (settings.get("toolPermission")?.as_str()?.trim() == "always-proceed")
        .then_some(AcpApprovalLevel::FullAccess)
}

/// `$GROK_HOME/config.toml` (default `~/.grok`), `[ui] permission_mode`, or the older `[ui] yolo`.
fn grok_level() -> Option<AcpApprovalLevel> {
    let home = std::env::var("GROK_HOME")
        .ok()
        .map(|dir| dir.trim().to_string())
        .filter(|dir| !dir.is_empty())
        .map(PathBuf::from)
        .unwrap_or_else(|| crate::resume_lookup::home_dir().join(".grok"));
    let text = fs::read_to_string(home.join("config.toml")).ok()?;
    let document = text.parse::<toml_edit::DocumentMut>().ok()?;
    let ui = document.get("ui")?;
    let mode = ui
        .get("permission_mode")
        .and_then(|item| item.as_str())
        .map(|mode| mode.trim().to_ascii_lowercase().replace('_', "-"));
    match mode.as_deref() {
        Some("auto" | "auto-review") => Some(AcpApprovalLevel::Auto),
        Some(
            "yolo" | "always-approve" | "alwaysapprove" | "bypasspermissions" | "bypass"
            | "full-access",
        ) => Some(AcpApprovalLevel::FullAccess),
        Some(_) => None,
        None => (ui.get("yolo").and_then(|item| item.as_bool()) == Some(true))
            .then_some(AcpApprovalLevel::FullAccess),
    }
}
