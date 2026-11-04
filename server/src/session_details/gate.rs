//! Whether the Session details panel is on for this computer, and the environment variable that
//! gives Claude sessions their task list while it is.

use ghostex_settings_catalog::built_in_extensions;
use serde_json::Value;

use crate::paths::GxserverPaths;

/// The setting that gives Claude its task tools (on by default, owned by the extension).
pub(super) const TASK_TOOLS_SETTING: &str = "sessionDetailsClaudeTaskTools";

/// The variable Claude Code reads before it offers its task tools on its newer models.
const TASK_TOOLS_VARIABLE: &str = "CLAUDE_CODE_ENABLE_TODO_TOOLS";

fn read_settings(paths: &GxserverPaths) -> Value {
    crate::shared_settings_file::read_shared_settings(paths).unwrap_or(Value::Null)
}

fn enabled_in(settings: &Value) -> bool {
    built_in_extensions::enabled_in_value(Some(settings), built_in_extensions::SESSION_DETAILS)
}

/// Whether the Session details panel built-in extension is on, read fresh from the settings file.
pub(crate) fn session_details_enabled(paths: &GxserverPaths) -> bool {
    enabled_in(&read_settings(paths))
}

/// `CLAUDE_CODE_ENABLE_TODO_TOOLS=1` while the panel is on and its task-list setting is too, for
/// the environment every session starts with.
///
/// CDXC:SessionDetails 2026-10-25 DECISION:
/// User (Q15): turn Claude's task tools back on "behind a setting that defaults to on", because Claude Code 2.1.x hides TaskCreate/TaskUpdate on Opus 5.5, Sonnet 5.5 and Fable unless this variable is set, so those sessions never had a task list to show. It is part of the beta extension, so nobody else's Claude changes. It goes into the session's environment rather than the launch command: Connectors already adds a `--settings` flag to Claude's command and Claude keeps only the last one, and an `X=1 claude` prefix breaks the account wrapper (`cswap run … -- <command>`). It takes effect when a session starts or wakes.
pub(crate) fn claude_task_tools_environment(
    paths: &GxserverPaths,
) -> Option<(&'static str, &'static str)> {
    let settings = read_settings(paths);
    if !enabled_in(&settings) {
        return None;
    }
    let on = settings
        .get(TASK_TOOLS_SETTING)
        .and_then(Value::as_bool)
        .or_else(|| {
            ghostex_settings_catalog::default_value(TASK_TOOLS_SETTING)
                .map(|value| *value == ghostex_settings_catalog::J::Bool(true))
        })
        .unwrap_or(true);
    on.then_some((TASK_TOOLS_VARIABLE, "1"))
}
