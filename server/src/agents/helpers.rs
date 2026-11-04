use serde_json::{json, Map, Value};

use super::*;

use crate::domain::{read_project_id, read_session_id, DomainRepository, DomainStateError};

pub(crate) fn default_agent_icon_to_id(icon: &str) -> Option<&'static str> {
    match icon {
        "amp-cli" => Some("amp"),
        "antigravity-cli" => Some("antigravity"),
        "claude" => Some("claude"),
        "claude-cloud" => Some("claude-cloud"),
        "codebuddy" => Some("codebuddy"),
        "codex" => Some("codex"),
        "command-code" => Some("command-code"),
        "copilot" => Some("copilot"),
        "cursor-cli" => Some("cursor"),
        "mastra" => Some("mastra"),
        "devin" => Some("devin"),
        // CDXC:AgentProviders 2026-10-06 DECISION: "`empryo` is a built-in agent id, with `em` as a command alias. Custom agents whose icon is Empryo resolve to the `empryo` family." The alias folds live in normalize_agent_id (agents/identity/agent_ids.rs) and the hook and process tables.
        "empryo" => Some("empryo"),
        "dsh" => Some("dsh"),
        "factory-droid" => Some("droid"),
        "freebuff" => Some("freebuff"),
        "gemini" => Some("gemini"),
        "grok-build" => Some("grok"),
        "hermes-agent" => Some("hermes-agent"),
        "kimi" => Some("kimi"),
        "kiro" => Some("kiro"),
        "omp" => Some("omp"),
        "openclaude" => Some("openclaude"),
        "opencode" => Some("opencode"),
        "pi" => Some("pi"),
        "qoder" => Some("qoder"),
        "rovo-dev" => Some("rovodev"),
        "zcode" => Some("zcode"),
        _ => None,
    }
}

pub(crate) fn default_agent_command(agent_id: &str) -> Option<&'static str> {
    match agent_id {
        "amp" => Some("amp"),
        "antigravity" => Some("agy"),
        "claude" => Some("claude"),
        "claude-cloud" => Some(crate::claude_cloud::CLAUDE_CLOUD_COMMAND),
        "codebuddy" => Some("codebuddy"),
        "codex" => Some("codex"),
        "command-code" => Some("commandcode"),
        "copilot" => Some("copilot"),
        "cursor" => Some("cursor-agent"),
        "mastra" => Some("mastracode"),
        "devin" => Some("devin"),
        "droid" => Some("droid"),
        "dsh" => Some("dsh-tui"),
        "empryo" => Some("empryo"),
        "freebuff" => Some("freebuff"),
        "gemini" => Some("gemini"),
        "grok" => Some("grok"),
        "hermes-agent" => Some("hermes"),
        "kimi" => Some("kimi"),
        "kiro" => Some("kiro-cli chat --agent ghostex"),
        "omp" => Some("omp"),
        "openclaude" => Some("openclaude"),
        "opencode" => Some("opencode"),
        "pi" => Some("pi"),
        "qoder" => Some("qodercli"),
        "rovodev" => Some("acli rovodev run"),
        "zcode" => Some("zcode"),
        _ => None,
    }
}

#[derive(Clone)]
pub(crate) struct LifecycleParams {
    pub(crate) project_id: String,
    pub(crate) session_id: String,
}

pub(crate) fn read_lifecycle(
    params: &Map<String, Value>,
) -> Result<LifecycleParams, DomainStateError> {
    Ok(LifecycleParams {
        project_id: read_project_id(params)?,
        session_id: read_session_id(params)?,
    })
}

pub(crate) fn lifecycle_update(lifecycle: &LifecycleParams) -> Map<String, Value> {
    let mut update = Map::new();
    update.insert("projectId".to_string(), json!(lifecycle.project_id));
    update.insert("sessionId".to_string(), json!(lifecycle.session_id));
    update
}

pub(crate) fn require_project(
    repository: &DomainRepository<'_>,
    project_id: &str,
) -> Result<Value, DomainStateError> {
    repository
        .get_project(project_id)?
        .ok_or_else(|| DomainStateError::not_found(format!("Project {project_id} does not exist.")))
}

pub(crate) fn require_session(
    repository: &DomainRepository<'_>,
    lifecycle: &LifecycleParams,
) -> Result<Value, DomainStateError> {
    repository
        .get_session(&lifecycle.project_id, &lifecycle.session_id)?
        .ok_or_else(|| {
            DomainStateError::not_found(format!(
                "Session {}/{} does not exist.",
                lifecycle.project_id, lifecycle.session_id
            ))
        })
}

pub(crate) fn read_required_text(
    value: Option<&Value>,
    field: &str,
) -> Result<String, DomainStateError> {
    value
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(str::to_string)
        .ok_or_else(|| DomainStateError::bad_request(format!("{field} is required.")))
}

pub(crate) fn read_text(params: &Map<String, Value>, key: &str) -> Option<String> {
    params
        .get(key)
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(str::to_string)
}

pub(crate) fn read_text_value(value: &Value, key: &str) -> Option<String> {
    value
        .get(key)
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(str::to_string)
}

pub(crate) fn read_text_from_map(map: &Map<String, Value>, key: &str) -> Option<String> {
    map.get(key)
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(str::to_string)
}

/// The staged first input draft, kept exactly as written. Every other runtime
/// text is trimmed, but the draft's trailing space (`@/path/export.md `) is
/// what separates the mention from the prompt the user types after it.
pub(crate) fn read_first_user_input_draft(session: &Value) -> Option<String> {
    session
        .get("runtimeSettings")
        .and_then(Value::as_object)
        .and_then(|settings| settings.get(FIRST_USER_INPUT_DRAFT_KEY))
        .and_then(Value::as_str)
        .filter(|value| !value.trim().is_empty())
        .map(str::to_string)
}

pub(crate) fn object_field(value: &Value, key: &str) -> Map<String, Value> {
    value
        .get(key)
        .and_then(Value::as_object)
        .cloned()
        .unwrap_or_default()
}

pub(crate) fn object_from_value(value: Value) -> Map<String, Value> {
    value.as_object().cloned().unwrap_or_default()
}

pub(crate) fn insert_optional_string(
    map: &mut Map<String, Value>,
    key: &str,
    value: Option<String>,
) {
    if let Some(value) = value {
        map.insert(key.to_string(), Value::String(value));
    }
}

pub(crate) fn insert_optional_from_params(
    target: &mut Map<String, Value>,
    params: &Map<String, Value>,
    key: &str,
) {
    if let Some(value) = params.get(key).cloned().filter(|value| !value.is_null()) {
        target.insert(key.to_string(), value);
    }
}

pub(crate) fn insert_truthy_from_params(
    target: &mut Map<String, Value>,
    params: &Map<String, Value>,
    key: &str,
) {
    if let Some(value) = params.get(key).cloned().filter(js_truthy_value) {
        target.insert(key.to_string(), value);
    }
}

pub(crate) fn js_truthy_value(value: &Value) -> bool {
    match value {
        Value::Null => false,
        Value::Bool(value) => *value,
        Value::Number(value) => value.as_f64() != Some(0.0),
        Value::String(value) => !value.is_empty(),
        Value::Array(_) | Value::Object(_) => true,
    }
}

pub(crate) fn read_session_target(session: &Value) -> Option<(String, String)> {
    Some((
        read_text_value(session, "projectId")?,
        read_text_value(session, "sessionId")?,
    ))
}

pub(crate) fn quote_shell_double_arg(value: &str) -> String {
    #[cfg(windows)]
    return crate::platform::shell::powershell_single_quote(value);
    #[cfg(not(windows))]
    format!(
        "\"{}\"",
        value
            .replace('\\', "\\\\")
            .replace('"', "\\\"")
            .replace('$', "\\$")
            .replace('`', "\\`")
    )
}

pub(crate) fn quote_shell_arg(value: &str) -> String {
    #[cfg(windows)]
    return crate::platform::shell::powershell_single_quote(value);
    #[cfg(not(windows))]
    format!("'{}'", value.replace('\'', "'\\''"))
}

/// CDXC:AgentProviders 2026-09-22 WHY:
/// Interrupting a restored CLI during an account switch is an intentional exit, not a failed resume. POSIX signal exit statuses must not launch the old account's fallback command while the switch is returning to the shell.
pub(crate) fn wrap_restored_terminal_resume_command(
    command: &str,
    display_command: &str,
    fallback_command: Option<&str>,
) -> String {
    #[cfg(windows)]
    {
        let mut script = format!(
            "Write-Host 'Restoring session...'; Write-Host {}; & {{ {} }}",
            quote_shell_arg(display_command),
            command
        );
        if let Some(fallback) = fallback_command.filter(|fallback| *fallback != command) {
            script.push_str(&format!("; if (-not $?) {{ & {{ {} }} }}", fallback));
        }
        return script;
    }
    #[cfg(not(windows))]
    {
        let mut lines = vec![
            format!("printf '%s\\n' {}", quote_shell_arg("Restoring session...")),
            format!("printf '> %s\\n\\n' {}", quote_shell_arg(display_command)),
            "__ghostex_restore_resume_status=0".to_string(),
            "__ghostex_restore_resume_primary() {".to_string(),
            command.to_string(),
            "}".to_string(),
            "__ghostex_restore_resume_primary || __ghostex_restore_resume_status=$?".to_string(),
            "unset -f __ghostex_restore_resume_primary".to_string(),
        ];
        if let Some(fallback_command) = fallback_command.filter(|fallback| *fallback != command) {
            lines.extend([
                "if [ \"$__ghostex_restore_resume_status\" -ne 0 ] && [ \"$__ghostex_restore_resume_status\" -lt 128 ]; then".to_string(),
                format!(
                    "  printf '%s\\n' {}",
                    quote_shell_arg("Exact resume failed; trying saved fallback resume command.")
                ),
                "  __ghostex_restore_resume_status=0".to_string(),
                "  __ghostex_restore_resume_fallback() {".to_string(),
                fallback_command.to_string(),
                "  }".to_string(),
                "  __ghostex_restore_resume_fallback || __ghostex_restore_resume_status=$?".to_string(),
                "  unset -f __ghostex_restore_resume_fallback".to_string(),
                "fi".to_string(),
            ]);
        }
        lines.push("unset __ghostex_restore_resume_status".to_string());
        lines.join("\n")
    }
}

pub(crate) fn as_atuin_ignored_shell_input(command: &str) -> String {
    let text = command.trim_end_matches(['\r', '\n']);
    if text.starts_with(' ') {
        format!("{text}\r")
    } else {
        format!(" {text}\r")
    }
}

pub(crate) fn parse_json_object(text: &str) -> Value {
    serde_json::from_str::<Value>(text).unwrap_or(Value::Null)
}

pub(crate) fn now_iso() -> String {
    chrono::Utc::now().to_rfc3339_opts(chrono::SecondsFormat::Millis, true)
}

pub(crate) fn now_ms() -> i64 {
    chrono::Utc::now().timestamp_millis()
}

pub(crate) fn sql_error(error: rusqlite::Error) -> DomainStateError {
    DomainStateError {
        code: "internalError",
        message: format!("SQLite agent-state error: {error}"),
    }
}

/// CDXC:AgentLauncher 2026-10-28 DECISION:
/// Ghostex starts DeepSeek Harness without dsh-tui's launchpad (`DSH_TUI_NO_LAUNCHPAD=1`), on new sessions and resumes alike (coordinator for the user, who said to decide small things: "yes, start DeepSeek Harness sessions with DSH_TUI_NO_LAUNCHPAD=1"). The launchpad's input box is one line, so a multi-line first message sent from the chat arrived with its lines joined (seen live 2026-10-28); without it dsh-tui opens on its chat page, whose input keeps them.
/// Without the launchpad a fresh launch opened dsh-tui's one-time workspace home instead (also seen live): dsh-tui shows it unless the launch names a workspace, which its own `dst` launchers do with `DSH_TUI_WORKSPACE_TARGET` set to the current folder. Ghostex starts the session in its folder already, so it names `.`.
/// The variables are set for this command only, the way a Claude or Codex resume sets its profile folder (`env NAME=value` on macOS and Linux, `$env:NAME=value;` in PowerShell), and a command that already names the launchpad switch is left alone.
pub(crate) fn with_dsh_launch_env(command: &str) -> String {
    if command.trim().is_empty() || command.contains("DSH_TUI_NO_LAUNCHPAD") {
        return command.to_string();
    }
    if cfg!(windows) {
        format!("$env:DSH_TUI_NO_LAUNCHPAD='1'; $env:DSH_TUI_WORKSPACE_TARGET='.'; {command}")
    } else {
        format!("env DSH_TUI_NO_LAUNCHPAD=1 DSH_TUI_WORKSPACE_TARGET=. {command}")
    }
}

#[cfg(test)]
mod dsh_launch_tests {
    use super::*;

    #[test]
    fn deepseek_harness_starts_without_its_launchpad_once() {
        let command = with_dsh_launch_env("dsh-tui --resume abc");
        if cfg!(windows) {
            assert_eq!(
                command,
                "$env:DSH_TUI_NO_LAUNCHPAD='1'; $env:DSH_TUI_WORKSPACE_TARGET='.'; dsh-tui --resume abc"
            );
        } else {
            assert_eq!(
                command,
                "env DSH_TUI_NO_LAUNCHPAD=1 DSH_TUI_WORKSPACE_TARGET=. dsh-tui --resume abc"
            );
        }
        assert_eq!(with_dsh_launch_env(&command), command);
        assert_eq!(
            crate::agents::infer_agent_id_from_command(&command).as_deref(),
            Some("dsh")
        );
    }
}
