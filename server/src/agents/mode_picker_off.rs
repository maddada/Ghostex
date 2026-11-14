//! A computer whose chat box hides the mode picker (Settings > Experience, `chatComposerModePicker`
//! off, as Chill sets it) starts its Claude sessions in Auto, or in Bypass permissions while Skip
//! permissions (`agentAcceptAllEnabled`) is on, which the launch already adds.
//!
//! CDXC:Experience 2026-11-15 DECISION:
//! User, Q3281: "when mode is hidden in chill just always keep it on auto or bypass permissions based on the setting on that machine basically". With no picker there is no way out of Claude's own default (Manual), so a launch or a wake on this computer adds `--permission-mode auto` unless the command already names a mode or runs without asking. Read-only threads and side chats still take their own mode after this (they replace it). Only Claude has an Auto mode to start in; other agents start as before. Sessions already running keep their mode until they next start.
//! SEE-ALSO: packages/settings-catalog/src/experience.rs (the switch and the presets), server/src/agents/launch_plan.rs (`resolve_agent_launch_command`).

use crate::agents::helpers::default_agent_icon_to_id;

const MODE_PICKER_KEY: &str = "chatComposerModePicker";

/// Whether this computer's chat box hides the mode picker.
fn mode_picker_off() -> bool {
    let paths = crate::paths::get_gxserver_paths(None);
    crate::shared_settings_file::read_shared_settings(&paths)
        .and_then(|settings| settings.get(MODE_PICKER_KEY).cloned())
        .and_then(|value| value.as_bool())
        == Some(false)
}

/// `command` with Claude's Auto mode added when the mode picker is off and the launch does not
/// already run without asking or name a mode.
pub(crate) fn with_auto_mode(
    agent_id: &str,
    icon: Option<&str>,
    accept_all: bool,
    command: &str,
) -> String {
    if accept_all || command.trim().is_empty() || !is_claude(agent_id, icon) {
        return command.to_string();
    }
    auto_mode_command(command, mode_picker_off())
}

fn is_claude(agent_id: &str, icon: Option<&str>) -> bool {
    matches!(agent_id, "claude" | "openclaude")
        || icon
            .and_then(default_agent_icon_to_id)
            .is_some_and(|id| matches!(id, "claude" | "openclaude"))
}

fn auto_mode_command(command: &str, picker_off: bool) -> String {
    let names_mode = command.split_whitespace().any(|token| {
        token == "--permission-mode"
            || token.starts_with("--permission-mode=")
            || token == "--dangerously-skip-permissions"
    });
    if !picker_off || names_mode {
        return command.to_string();
    }
    format!("{} --permission-mode auto", command.trim_end())
}
