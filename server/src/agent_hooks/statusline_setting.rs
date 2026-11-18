//! Settings > Agents > Advanced > Read live status from Claude Code and Cursor: whether Ghostex's
//! status line helper is registered in the agents' own configs.
//!
//! CDXC:AgentHooks 2026-11-18 DECISION:
//! The user wants Ghostex to change Claude's and Codex's configs only where it must, and asked
//! (Q4160) for the status line helper behind a setting, on by default, under a new Advanced fold
//! on Settings > Agents, its tooltip saying why it is essential. Claude Code and Cursor hand their
//! live model, effort, fast mode, context, cost and 5h / 7d limits only to a statusLine command,
//! so the chat needs it. It no longer rides on the hook installer: since Use agent hooks went off
//! by default (2026-11-12) that installer skipped Claude, so a fresh install never got the helper.
//! On, every Claude config folder (the default one and each cswap profile) and Cursor's
//! `cli-config.json` get it, wrapping the user's own command; Codex's footer gets
//! `model-with-reasoning` when the user's `tui.status_line` lacks it. Off, Ghostex puts the
//! user's own command back (or drops the entry it created) and stops adding the Codex item; it
//! does not take that item out, since it can't tell it from one the user listed. Account hub
//! profiles follow the setting when Ghostex next prepares them.
//! A portable copy registers the helper only where no Ghostex helper is yet and removes only its
//! own, like its hooks (foreign_owner.rs).
//! SEE-ALSO: server/src/agent_hooks/statusline.rs (the helper), server/src/hookless_activity/hook_free_fallback.rs (`upkeep` calls `keep_in_step`), packages/settings-catalog/src/pages/agents.rs (the row).

use std::{
    path::{Path, PathBuf},
    sync::Mutex,
    time::{Duration, Instant},
};

use serde_json::Value;

use super::config::HookPaths;
use super::cursor_statusline::{
    cursor_cli_config_path, ensure_cursor_statusline, remove_cursor_statusline,
};
use super::probing::{path_string, read_file_text};
use super::resolution::provider_hook_paths;
use super::statusline::{
    ghostex_statusline_registered, register_statusline, statusline_is_current,
    unregister_statusline, StatuslineAgent,
};
use crate::paths::GxserverPaths;

/// The setting's key in the shared settings file.
pub(crate) const READ_AGENT_LIVE_STATUS: &str = "readAgentLiveStatus";

/// How often the configs are checked again while the setting stays the same: a Claude or Cursor
/// installed after gxserver started, or a config the agent rewrote, gets the helper within this.
const RECHECK_EVERY: Duration = Duration::from_secs(60);

/// Whether the helper should be registered. A missing key is the shipped default (on).
pub(crate) fn read_agent_live_status(paths: &GxserverPaths) -> bool {
    crate::shared_settings_file::read_shared_settings(paths)
        .and_then(|settings| {
            settings
                .get(READ_AGENT_LIVE_STATUS)
                .and_then(Value::as_bool)
        })
        .unwrap_or(true)
}

static LAST_PASS: Mutex<Option<(bool, Instant)>> = Mutex::new(None);

/// Registers or removes the helper when the setting changed, at the first call, and every
/// `RECHECK_EVERY` otherwise. Returns the config files it changed.
pub(crate) fn keep_in_step(paths: &GxserverPaths) -> Vec<String> {
    let on = read_agent_live_status(paths);
    {
        let mut last = LAST_PASS
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        if last.is_some_and(|(was_on, at)| was_on == on && at.elapsed() < RECHECK_EVERY) {
            return Vec::new();
        }
        *last = Some((on, Instant::now()));
    }
    apply(&HookPaths::from_paths(paths), on)
}

/// Brings every config in line with `on`. Errors are skipped: the next pass tries again.
pub(crate) fn apply(hook_paths: &HookPaths, on: bool) -> Vec<String> {
    if on {
        // The script must exist before a config names it.
        let _ = super::statusline::install_statusline_hook(hook_paths);
    }
    let portable = ghostex_paths::portable::current().is_some();
    let mut changed = Vec::new();
    for settings in provider_hook_paths("claude", hook_paths) {
        if update_claude_settings(&settings, &hook_paths.statusline_hook_path, on, portable) {
            changed.push(path_string(&settings));
        }
    }
    for hooks_path in provider_hook_paths("cursor", hook_paths) {
        if let Some(path) = update_cursor_config(&hooks_path, hook_paths, on, portable) {
            changed.push(path_string(&path));
        }
    }
    if on {
        for hooks_path in provider_hook_paths("codex", hook_paths) {
            let config = super::codex_trust::codex_config_path_for_hooks(&hooks_path);
            if super::codex_status_line::ensure_codex_status_line_names_model(&config)
                .unwrap_or(false)
            {
                changed.push(path_string(&config));
            }
        }
    }
    changed
}

/// What to do with one config's statusLine.
fn wanted_change(data: &Value, script: &Path, on: bool, portable: bool) -> Option<bool> {
    let ours = statusline_is_current(data, script);
    let any_ghostex = ghostex_statusline_registered(data);
    if on {
        // Register where it is missing, or re-point our own stale entry (an installed Ghostex
        // also takes over another copy's, as with its hooks).
        (!ours && !(portable && any_ghostex)).then_some(true)
    } else {
        (ours || (any_ghostex && !portable)).then_some(false)
    }
}

/// Claude's `settings.json` (or a profile's). A folder Claude never created is left alone; a
/// file that does not parse as a JSON object is never overwritten.
fn update_claude_settings(path: &Path, script: &Path, on: bool, portable: bool) -> bool {
    if !path.parent().is_some_and(Path::is_dir) {
        return false;
    }
    let text = read_file_text(path);
    let mut data = if text.trim().is_empty() {
        Value::Object(Default::default())
    } else {
        match serde_json::from_str::<Value>(&text) {
            Ok(value) if value.is_object() => value,
            _ => return false,
        }
    };
    let changed = match wanted_change(&data, script, on, portable) {
        Some(true) => register_statusline(&mut data, script),
        Some(false) => unregister_statusline(&mut data),
        None => false,
    };
    changed && super::install::write_json_file(path, &data).is_ok()
}

fn update_cursor_config(
    hooks_path: &Path,
    hook_paths: &HookPaths,
    on: bool,
    portable: bool,
) -> Option<PathBuf> {
    let config = cursor_cli_config_path(hooks_path)?;
    let text = read_file_text(&config);
    let data = serde_json::from_str::<Value>(&text)
        .ok()
        .filter(Value::is_object)?;
    let script = StatuslineAgent::Cursor.script_path(hook_paths);
    match wanted_change(&data, script, on, portable)? {
        true => ensure_cursor_statusline(hooks_path, hook_paths)
            .ok()
            .flatten(),
        false => remove_cursor_statusline(hooks_path).ok().flatten(),
    }
}
