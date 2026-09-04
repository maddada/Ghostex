use crate::platform::shell::{command_shell, user_login_shell_exec_command};

use super::*;

pub(crate) struct ZmxAttachCommandInput {
    pub(crate) cwd: String,
    pub(crate) global_session_ref: Option<String>,
    pub(crate) gxserver_auth_token_file: Option<String>,
    pub(crate) gxserver_base_url: Option<String>,
    pub(crate) gxserver_protocol_version: Option<u64>,
    pub(crate) prompt_editor: Option<String>,
    pub(crate) session_name: String,
    pub(crate) title: Option<String>,
    pub(crate) zmx_executable_path: String,
}

pub(crate) struct ZmxRunCommandInput {
    pub(crate) cwd: String,
    pub(crate) global_session_ref: Option<String>,
    pub(crate) gxserver_auth_token_file: Option<String>,
    pub(crate) gxserver_base_url: Option<String>,
    pub(crate) gxserver_protocol_version: Option<u64>,
    pub(crate) prompt_editor: Option<String>,
    pub(crate) session_name: String,
    pub(crate) startup_text: String,
    pub(crate) zmx_executable_path: String,
}

pub(crate) struct ZmxShellProviderCommandInput {
    pub(crate) cwd: String,
    pub(crate) global_session_ref: Option<String>,
    pub(crate) gxserver_auth_token_file: Option<String>,
    pub(crate) gxserver_base_url: Option<String>,
    pub(crate) gxserver_protocol_version: Option<u64>,
    pub(crate) prompt_editor: Option<String>,
    pub(crate) session_name: String,
    pub(crate) zmx_executable_path: String,
}

pub(crate) fn build_zmx_attach_command(input: ZmxAttachCommandInput) -> String {
    /*
    CDXC:Zmx 2026-07-15:
    gxserver-generated attach commands may only connect to providers already
    initialized through startSessionProvider. --require-existing closes the
    probe/attach race without changing direct zmx CLI create-if-missing
    behavior or the per-client prompt-editor capability decision.
    */
    let shell = command_shell();
    let prompt_editor_attach_args = zmx_prompt_editor_attach_args(input.prompt_editor.as_deref());
    let script = format!(
        r#"
zmx_session={}
zmx_cwd={}
zmx_global_session_ref={}
zmx_gxserver_auth_token_file={}
zmx_gxserver_base_url={}
zmx_gxserver_protocol_version={}
zmx_persistence_notice_command={}
zmx_title_notice_command={}
zmx_bin={}
zmx_prompt_editor_attach_args={}
if [ ! -x "$zmx_bin" ]; then
  printf '%s\n' 'session persistence is set to zmx, but Ghostex bundled zmx was not found.'
  exit 127
fi
export GHOSTEX_ZMX_BIN="$zmx_bin"
{}
if [ -n "$zmx_global_session_ref" ]; then
  export GHOSTEX_GLOBAL_SESSION_REF="$zmx_global_session_ref"
fi
if [ -n "$zmx_session" ]; then
  export GHOSTEX_SESSION_ID="$zmx_session"
fi
if [ -n "$zmx_gxserver_auth_token_file" ]; then
  export GHOSTEX_GXSERVER_AUTH_TOKEN_FILE="$zmx_gxserver_auth_token_file"
fi
if [ -n "$zmx_gxserver_base_url" ]; then
  export GHOSTEX_GXSERVER_BASE_URL="$zmx_gxserver_base_url"
fi
if [ -n "$zmx_gxserver_protocol_version" ]; then
  export GHOSTEX_GXSERVER_PROTOCOL_VERSION="$zmx_gxserver_protocol_version"
fi
if "$zmx_bin" list --short 2>/dev/null | grep -F -x -- "$zmx_session" >/dev/null 2>&1; then
  if [ -n "$zmx_title_notice_command" ]; then
    {} {} "$zmx_title_notice_command"
  fi
  exec "$zmx_bin" attach --require-existing $zmx_prompt_editor_attach_args "$zmx_session"
fi
if [ -n "$zmx_persistence_notice_command" ]; then
  {} {} "$zmx_persistence_notice_command"
fi
cd "$zmx_cwd" || exit
exec "$zmx_bin" attach --require-existing $zmx_prompt_editor_attach_args "$zmx_session"
"#,
        shell_quote(&input.session_name),
        shell_quote(&input.cwd),
        shell_quote(input.global_session_ref.as_deref().unwrap_or("")),
        shell_quote(input.gxserver_auth_token_file.as_deref().unwrap_or("")),
        shell_quote(input.gxserver_base_url.as_deref().unwrap_or("")),
        shell_quote(
            &input
                .gxserver_protocol_version
                .map(|value| value.to_string())
                .unwrap_or_default(),
        ),
        shell_quote(&persistence_notice_shell_command(&input.session_name)),
        shell_quote(&session_title_shell_command(input.title.as_deref())),
        shell_quote(&input.zmx_executable_path),
        shell_quote(prompt_editor_attach_args),
        zmx_session_identity_reset_shell_command(),
        &shell.executable,
        shell.command_flag(false),
        &shell.executable,
        shell.command_flag(false),
    )
    .trim()
    .to_string();
    shell.command_string(&script, false)
}

pub(crate) fn build_started_zmx_attach_command(input: ZmxAttachCommandInput) -> String {
    /*
    createWorkspaceTerminal has just started this exact provider under a
    never-reused session identity. Keep the normal executable/env validation
    and zmx's require-existing failure contract, but do not launch a redundant
    `zmx list` process before attaching. Other lifecycle callers continue
    through build_zmx_attach_command and retain their canonical probe path.
    */
    let shell = command_shell();
    let prompt_editor_attach_args = zmx_prompt_editor_attach_args(input.prompt_editor.as_deref());
    let script = format!(
        r#"
zmx_session={}
zmx_global_session_ref={}
zmx_gxserver_auth_token_file={}
zmx_gxserver_base_url={}
zmx_gxserver_protocol_version={}
zmx_title_notice_command={}
zmx_bin={}
zmx_prompt_editor_attach_args={}
if [ ! -x "$zmx_bin" ]; then
  printf '%s\n' 'session persistence is set to zmx, but Ghostex bundled zmx was not found.'
  exit 127
fi
export GHOSTEX_ZMX_BIN="$zmx_bin"
{}
if [ -n "$zmx_global_session_ref" ]; then
  export GHOSTEX_GLOBAL_SESSION_REF="$zmx_global_session_ref"
fi
if [ -n "$zmx_session" ]; then
  export GHOSTEX_SESSION_ID="$zmx_session"
fi
if [ -n "$zmx_gxserver_auth_token_file" ]; then
  export GHOSTEX_GXSERVER_AUTH_TOKEN_FILE="$zmx_gxserver_auth_token_file"
fi
if [ -n "$zmx_gxserver_base_url" ]; then
  export GHOSTEX_GXSERVER_BASE_URL="$zmx_gxserver_base_url"
fi
if [ -n "$zmx_gxserver_protocol_version" ]; then
  export GHOSTEX_GXSERVER_PROTOCOL_VERSION="$zmx_gxserver_protocol_version"
fi
if [ -n "$zmx_title_notice_command" ]; then
  {} {} "$zmx_title_notice_command"
fi
exec "$zmx_bin" attach --require-existing $zmx_prompt_editor_attach_args "$zmx_session"
"#,
        shell_quote(&input.session_name),
        shell_quote(input.global_session_ref.as_deref().unwrap_or("")),
        shell_quote(input.gxserver_auth_token_file.as_deref().unwrap_or("")),
        shell_quote(input.gxserver_base_url.as_deref().unwrap_or("")),
        shell_quote(
            &input
                .gxserver_protocol_version
                .map(|value| value.to_string())
                .unwrap_or_default(),
        ),
        shell_quote(&session_title_shell_command(input.title.as_deref())),
        shell_quote(&input.zmx_executable_path),
        shell_quote(prompt_editor_attach_args),
        zmx_session_identity_reset_shell_command(),
        &shell.executable,
        shell.command_flag(false),
    )
    .trim()
    .to_string();
    shell.command_string(&script, false)
}

pub(crate) fn build_zmx_kill_command(session_name: &str, zmx_executable_path: &str) -> String {
    format!(
        r#"
zmx_session={}
zmx_bin={}
if [ ! -x "$zmx_bin" ]; then
  printf '%s\n' 'session persistence is set to zmx, but Ghostex bundled zmx was not found.'
  exit 127
fi
unset ZMX_SESSION ZMX_SESSION_PREFIX
exec "$zmx_bin" kill "$zmx_session" --force
"#,
        shell_quote(session_name),
        shell_quote(zmx_executable_path),
    )
    .trim()
    .to_string()
}

pub(crate) fn build_zmx_history_command(session_name: &str, zmx_executable_path: &str) -> String {
    format!(
        r#"
zmx_session={}
zmx_bin={}
if [ ! -x "$zmx_bin" ]; then
  printf '%s\n' 'session persistence is set to zmx, but Ghostex bundled zmx was not found.' >&2
  exit 127
fi
unset ZMX_SESSION ZMX_SESSION_PREFIX
exec "$zmx_bin" history "$zmx_session"
"#,
        shell_quote(session_name),
        shell_quote(zmx_executable_path),
    )
    .trim()
    .to_string()
}

/// `zmx grid <session>`: the daemon grid plus every attached client and
/// whether each one is hidden, as JSON.
pub(crate) fn build_zmx_grid_command(session_name: &str, zmx_executable_path: &str) -> String {
    format!(
        r#"
zmx_session={}
zmx_bin={}
if [ ! -x "$zmx_bin" ]; then
  printf '%s\n' 'session persistence is set to zmx, but Ghostex bundled zmx was not found.' >&2
  exit 127
fi
unset ZMX_SESSION ZMX_SESSION_PREFIX
exec "$zmx_bin" grid "$zmx_session"
"#,
        shell_quote(session_name),
        shell_quote(zmx_executable_path),
    )
    .trim()
    .to_string()
}

pub(crate) fn build_zmx_send_command(session_name: &str, zmx_executable_path: &str) -> String {
    format!(
        r#"
zmx_session={}
zmx_bin={}
if [ ! -x "$zmx_bin" ]; then
  printf '%s\n' 'session persistence is set to zmx, but Ghostex bundled zmx was not found.' >&2
  exit 127
fi
unset ZMX_SESSION ZMX_SESSION_PREFIX
exec "$zmx_bin" send "$zmx_session"
"#,
        shell_quote(session_name),
        shell_quote(zmx_executable_path),
    )
    .trim()
    .to_string()
}

pub(crate) fn build_zmx_run_command(input: ZmxRunCommandInput) -> String {
    let startup_command =
        with_atuin_ignored_shell_history_prefix(input.startup_text.trim_end_matches(['\r', '\n']));
    let provider_shell_command = format!(
        "{}\n{}\n{}",
        zmx_provider_prompt_editor_setup_shell_command(input.prompt_editor.as_deref()),
        startup_command,
        user_login_shell_exec_command()
    );
    format_zmx_provider_run_script(
        &input.session_name,
        &input.cwd,
        input.global_session_ref.as_deref(),
        input.gxserver_auth_token_file.as_deref(),
        input.gxserver_base_url.as_deref(),
        input.gxserver_protocol_version,
        Some(&startup_command),
        &provider_shell_command,
        "zmx_startup_command",
        &input.zmx_executable_path,
    )
}

pub(crate) fn build_zmx_shell_provider_command(input: ZmxShellProviderCommandInput) -> String {
    let provider_shell_command = format!(
        "{}\n{}",
        zmx_provider_prompt_editor_setup_shell_command(input.prompt_editor.as_deref()),
        user_login_shell_exec_command()
    );
    format_zmx_provider_run_script(
        &input.session_name,
        &input.cwd,
        input.global_session_ref.as_deref(),
        input.gxserver_auth_token_file.as_deref(),
        input.gxserver_base_url.as_deref(),
        input.gxserver_protocol_version,
        None,
        &provider_shell_command,
        "zmx_shell_command",
        &input.zmx_executable_path,
    )
}

#[allow(clippy::too_many_arguments)]
fn format_zmx_provider_run_script(
    session_name: &str,
    cwd: &str,
    global_session_ref: Option<&str>,
    gxserver_auth_token_file: Option<&str>,
    gxserver_base_url: Option<&str>,
    gxserver_protocol_version: Option<u64>,
    startup_text: Option<&str>,
    provider_shell_command: &str,
    command_variable: &str,
    zmx_executable_path: &str,
) -> String {
    let shell = command_shell();
    let startup_text_assignment = startup_text
        .map(|text| format!("zmx_startup_text={}\n", shell_quote(text)))
        .unwrap_or_default();
    let startup_text_guard = if startup_text.is_some() {
        "if [ -z \"$zmx_startup_text\" ]; then\n  printf '%s\\n' 'gxserver startSessionProvider requires startup text.' >&2\n  exit 64\nfi\n"
    } else {
        ""
    };
    let command_arg = format!("${command_variable}");
    format!(
        r#"
zmx_session={}
zmx_cwd={}
zmx_global_session_ref={}
zmx_gxserver_auth_token_file={}
zmx_gxserver_base_url={}
zmx_gxserver_protocol_version={}
{}{}={}
zmx_bin={}
if [ ! -x "$zmx_bin" ]; then
  printf '%s\n' 'session persistence is set to zmx, but Ghostex bundled zmx was not found.' >&2
  exit 127
fi
export GHOSTEX_ZMX_BIN="$zmx_bin"
{}{}
if [ -n "$zmx_global_session_ref" ]; then
  export GHOSTEX_GLOBAL_SESSION_REF="$zmx_global_session_ref"
fi
if [ -n "$zmx_session" ]; then
  export GHOSTEX_SESSION_ID="$zmx_session"
fi
if [ -n "$zmx_gxserver_auth_token_file" ]; then
  export GHOSTEX_GXSERVER_AUTH_TOKEN_FILE="$zmx_gxserver_auth_token_file"
fi
if [ -n "$zmx_gxserver_base_url" ]; then
  export GHOSTEX_GXSERVER_BASE_URL="$zmx_gxserver_base_url"
fi
if [ -n "$zmx_gxserver_protocol_version" ]; then
  export GHOSTEX_GXSERVER_PROTOCOL_VERSION="$zmx_gxserver_protocol_version"
fi
cd "$zmx_cwd" || exit
exec "$zmx_bin" run "$zmx_session" -d --initial-command {} {} "{}"
"#,
        shell_quote(session_name),
        shell_quote(cwd),
        shell_quote(global_session_ref.unwrap_or("")),
        shell_quote(gxserver_auth_token_file.unwrap_or("")),
        shell_quote(gxserver_base_url.unwrap_or("")),
        shell_quote(
            &gxserver_protocol_version
                .map(|value| value.to_string())
                .unwrap_or_default(),
        ),
        startup_text_assignment,
        command_variable,
        shell_quote(provider_shell_command),
        shell_quote(zmx_executable_path),
        startup_text_guard,
        zmx_session_identity_reset_shell_command(),
        &shell.executable,
        shell.command_flag(true),
        command_arg,
    )
    .trim()
    .to_string()
}

pub(crate) fn build_zmx_exists_command(session_name: &str, zmx_executable_path: &str) -> String {
    format!(
        r#"
zmx_session={}
zmx_bin={}
if [ ! -x "$zmx_bin" ]; then
  printf '%s\n' 'session persistence is set to zmx, but Ghostex bundled zmx was not found.' >&2
  exit 127
fi
unset ZMX_SESSION ZMX_SESSION_PREFIX
zmx_sessions=$("$zmx_bin" list --short)
zmx_list_status=$?
if [ "$zmx_list_status" -ne 0 ]; then
  printf '%s\n' "zmx list --short failed with exit $zmx_list_status" >&2
  exit 2
fi
printf '%s\n' "$zmx_sessions" | grep -F -x -- "$zmx_session" >/dev/null 2>&1
"#,
        shell_quote(session_name),
        shell_quote(zmx_executable_path),
    )
    .trim()
    .to_string()
}

fn zmx_provider_prompt_editor_setup_shell_command(prompt_editor: Option<&str>) -> String {
    /*
    CDXC:PromptEditor 2026-06-30-03:11:
    zmx providers still need a stable EDITOR wrapper so Ctrl+G can block through
    Ghostex when Monaco is available, but the non-Monaco path must run the
    remote shell's own editor instead of gte. Capture VISUAL/EDITOR before
    installing the wrapper, and export Monaco only for attach clients that
    explicitly advertised it.
    */
    let mut script = r#"
case "${GHOSTEX_HOME:-}" in
  /*) ghostex_prompt_editor_state_dir="$GHOSTEX_HOME/state" ;;
  *) case "${XDG_STATE_HOME:-}" in
       /*) ghostex_prompt_editor_state_dir="${XDG_STATE_HOME%/}/ghostex" ;;
       *) ghostex_prompt_editor_state_dir="$HOME/.local/state/ghostex" ;;
     esac ;;
esac
ghostex_prompt_editor_wrapper="$ghostex_prompt_editor_state_dir/prompt-editor"
ghostex_prompt_editor_machine_visual="${VISUAL:-}"
ghostex_prompt_editor_machine_editor="${EDITOR:-}"
mkdir -p "$(dirname "$ghostex_prompt_editor_wrapper")" 2>/dev/null || true
cat > "$ghostex_prompt_editor_wrapper" <<'__GHOSTEX_PROMPT_EDITOR_WRAPPER__'
#!/bin/sh
if [ -n "${GHOSTEX_ZMX_BIN:-}" ] && [ -x "${GHOSTEX_ZMX_BIN:-}" ]; then
  export GHOSTEX_ZMX_BIN
fi
if [ -n "${GHOSTEX_CLI_EXECUTABLE:-}" ] && [ -x "${GHOSTEX_CLI_EXECUTABLE:-}" ]; then
  exec "$GHOSTEX_CLI_EXECUTABLE" prompt-editor "$@"
fi
if command -v ghostex >/dev/null 2>&1; then
  exec ghostex prompt-editor "$@"
fi
exec /bin/sh -lc 'exec ${GHOSTEX_PROMPT_EDITOR_MACHINE_VISUAL:-${GHOSTEX_PROMPT_EDITOR_MACHINE_EDITOR:-vi}} "$@"' ghostex-prompt-editor "$@"
__GHOSTEX_PROMPT_EDITOR_WRAPPER__
chmod 755 "$ghostex_prompt_editor_wrapper" 2>/dev/null || true
export GHOSTEX_PROMPT_EDITOR_MACHINE_VISUAL="$ghostex_prompt_editor_machine_visual"
export GHOSTEX_PROMPT_EDITOR_MACHINE_EDITOR="$ghostex_prompt_editor_machine_editor"
export EDITOR="$ghostex_prompt_editor_wrapper"
export VISUAL="$ghostex_prompt_editor_wrapper"
"#
    .trim()
    .to_string();
    if prompt_editor == Some("monaco") {
        script.push_str("\nexport GHOSTEX_PROMPT_EDITOR_BACKEND=monaco");
    }
    script.push_str("\nexport GHOSTEX_PROMPT_EDITING_ENABLED=1");
    script
}

fn zmx_prompt_editor_attach_args(prompt_editor: Option<&str>) -> &'static str {
    match prompt_editor {
        Some("monaco") => "--prompt-editor=monaco",
        Some("code-server") => "--prompt-editor=code-server",
        _ => "",
    }
}

fn zmx_session_identity_reset_shell_command() -> String {
    format!("unset {}", session_identity_environment_keys().join(" "))
}

fn persistence_notice_shell_command(session_name: &str) -> String {
    format!(
        "printf '%s\\n' {}",
        shell_quote(&format!(
            "This session is using zmx persistence: {session_name}"
        ))
    )
}

fn session_title_shell_command(title: Option<&str>) -> String {
    let Some(title) = title.map(str::trim).filter(|title| !title.is_empty()) else {
        return String::new();
    };
    format!("printf '%s\\n' {}", shell_quote(title))
}

fn with_atuin_ignored_shell_history_prefix(text: &str) -> String {
    let trimmed_right = text.trim_end();
    if trimmed_right.trim().is_empty() {
        return String::new();
    }
    if trimmed_right.starts_with(' ') {
        trimmed_right.to_string()
    } else {
        format!(" {}", trimmed_right.trim_start())
    }
}

pub(crate) fn shell_quote(value: &str) -> String {
    format!("'{}'", value.replace('\'', "'\\''"))
}
