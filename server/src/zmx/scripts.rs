#[cfg(not(windows))]
use crate::platform::shell::{command_shell, user_login_shell_exec_command, user_login_shell_path};

#[cfg(not(windows))]
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
    /// `GHOSTEX_COORDINATOR_ROLE` and `GHOSTEX_COORDINATOR_SESSION` for a coordinator's or a
    /// thread's agent (coordinators/launch_env.rs); empty for every other session.
    pub(crate) coordinator_environment: Vec<(&'static str, String)>,
    /// The session's own `gxs_` token, exported as `GHOSTEX_SESSION_TOKEN` (crate::security).
    pub(crate) session_token: Option<String>,
}

impl ZmxRunCommandInput {
    /// The coordinator variables and the session token, exported after the identity reset.
    pub(crate) fn launch_environment(&self) -> Vec<(&'static str, String)> {
        let mut environment = self.coordinator_environment.clone();
        environment.extend(session_token_environment(self.session_token.as_deref()));
        environment
    }
}

pub(crate) fn session_token_environment(token: Option<&str>) -> Vec<(&'static str, String)> {
    token
        .filter(|token| !token.is_empty())
        .map(|token| (crate::security::SESSION_TOKEN_ENV, token.to_string()))
        .into_iter()
        .collect()
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
    /// The terminal's own `gxs_` token, exported as `GHOSTEX_SESSION_TOKEN` (crate::security).
    pub(crate) session_token: Option<String>,
}

#[cfg(not(windows))]
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

#[cfg(not(windows))]
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

#[cfg(not(windows))]
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

#[cfg(not(windows))]
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

/// Bounded history, with the old full-history request only when the daemon
/// explicitly reports that it predates scoped capture (CLI exit status 3).
#[cfg(not(unix))]
#[cfg(not(windows))]
pub(crate) fn build_zmx_screen_capture_command(
    session_name: &str,
    zmx_executable_path: &str,
    scrollback_rows: u32,
    vt: bool,
) -> String {
    format!(
        r#"
zmx_session={}
zmx_bin={}
unset ZMX_SESSION ZMX_SESSION_PREFIX
if "$zmx_bin" history {format_flag} --scrollback {} "$zmx_session"; then
  exit 0
else
  zmx_status=$?
fi
if [ "$zmx_status" -eq 3 ]; then
  exec "$zmx_bin" history {format_flag} "$zmx_session"
fi
exit "$zmx_status"
"#,
        shell_quote(session_name),
        shell_quote(zmx_executable_path),
        scrollback_rows,
        format_flag = if vt { "--vt" } else { "" },
    )
    .trim()
    .to_string()
}

/// `zmx grid <session>`: the daemon grid plus every attached client and
/// whether each one is hidden, as JSON.
#[cfg(not(windows))]
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

#[cfg(not(windows))]
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

#[cfg(not(windows))]
pub(crate) fn build_zmx_run_command(input: ZmxRunCommandInput) -> String {
    let startup_command =
        with_atuin_ignored_shell_history_prefix(input.startup_text.trim_end_matches(['\r', '\n']));
    let prompt_editor_setup = zmx_provider_session_setup_shell_command(
        input.prompt_editor.as_deref(),
        &input.zmx_executable_path,
    );
    let startup = format!("{}\n{}", prompt_editor_setup, startup_command);
    let login_shell = user_login_shell_path();
    let login_shell_name = std::path::Path::new(&login_shell)
        .file_name()
        .and_then(|name| name.to_str())
        .unwrap_or_default();
    let provider_shell_command = match login_shell_name {
        "zsh" => super::zsh_startup::agent_shell_command(&login_shell, &startup),
        "fish" => super::fish_startup::agent_shell_command(
            &login_shell,
            &command_shell(),
            &prompt_editor_setup,
            &startup,
        ),
        _ => format!("{}\n{}", startup, login_shell_exec_command(&login_shell)),
    };
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
        &input.launch_environment(),
    )
}

#[cfg(not(windows))]
pub(crate) fn build_zmx_shell_provider_command(input: ZmxShellProviderCommandInput) -> String {
    let setup = zmx_provider_session_setup_shell_command(
        input.prompt_editor.as_deref(),
        &input.zmx_executable_path,
    );
    let login_shell = user_login_shell_path();
    let login_shell_name = std::path::Path::new(&login_shell)
        .file_name()
        .and_then(|name| name.to_str())
        .unwrap_or_default();
    // Plain terminals keep `<state>/session-bin` (the `zmx` entry and the `codex` wrapper) first after the user's config, like agent sessions (session_bin.rs).
    let provider_shell_command = match login_shell_name {
        "zsh" => {
            format!(
                "{setup}\n{}",
                super::zsh_startup::plain_shell_command(&login_shell)
            )
        }
        "fish" => super::fish_startup::plain_shell_command(&login_shell, &setup),
        _ => format!("{setup}\n{}", login_shell_exec_command(&login_shell)),
    };
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
        &session_token_environment(input.session_token.as_deref()),
    )
}

/// The start script's `zmx run` arguments. On macOS the session's launchd job swaps them for
/// `ZMX_FOREGROUND_RUN_ARGUMENTS` (launch.rs), so the two must stay one edit apart.
#[cfg(not(windows))]
pub(crate) const ZMX_DETACHED_RUN_ARGUMENTS: &str = r#"run "$zmx_session" -d --initial-command"#;
#[cfg(target_os = "macos")]
pub(crate) const ZMX_FOREGROUND_RUN_ARGUMENTS: &str =
    r#"run "$zmx_session" --foreground --initial-command"#;

#[allow(clippy::too_many_arguments)]
#[cfg(not(windows))]
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
    extra_environment: &[(&'static str, String)],
) -> String {
    let shell = command_shell();
    // After the identity reset below, which clears any inherited copy of them.
    let extra_exports: String = extra_environment
        .iter()
        .map(|(key, value)| format!("export {key}={}\n", shell_quote(value)))
        .collect();
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
{}cd "$zmx_cwd" || exit
exec "$zmx_bin" {ZMX_DETACHED_RUN_ARGUMENTS} {} {} "{}"
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
        extra_exports,
        &shell.executable,
        shell.command_flag(true),
        command_arg,
    )
    .trim()
    .to_string()
}

#[cfg(not(windows))]
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

/// The setup every zmx provider's shell runs before the session's shell or agent: the prompt-editor wrapper, then `<state>/session-bin` first on PATH with the session's `zmx` entry and the `codex` wrapper (`session_bin.rs`).
#[cfg(not(windows))]
fn zmx_provider_session_setup_shell_command(
    prompt_editor: Option<&str>,
    zmx_executable_path: &str,
) -> String {
    format!(
        "{}\n{}",
        zmx_provider_prompt_editor_setup_shell_command(prompt_editor),
        super::session_bin::zmx_provider_session_bin_setup_shell_command(zmx_executable_path),
    )
}

/// The `exec` of the session's login shell once the setup and any agent have run. Bash gets the step that keeps `<state>/session-bin` first after .bash_profile (bash_startup.rs); zsh and fish get theirs from zsh_startup.rs and fish_startup.rs.
#[cfg(not(windows))]
fn login_shell_exec_command(login_shell: &str) -> String {
    match std::path::Path::new(login_shell)
        .file_name()
        .and_then(|name| name.to_str())
    {
        Some("bash") => super::bash_startup::login_shell_exec_command(login_shell),
        _ => user_login_shell_exec_command(),
    }
}

#[cfg(not(windows))]
pub(crate) fn zmx_provider_prompt_editor_setup_shell_command(
    prompt_editor: Option<&str>,
) -> String {
    /*
    CDXC:PromptEditor 2026-06-30-03:11:
    zmx providers still need a stable EDITOR wrapper so Ctrl+G can block through
    Ghostex when Monaco is available, but the non-Monaco path must run the
    remote shell's own editor instead of gte. Capture VISUAL/EDITOR before
    installing the wrapper, and export Monaco only for attach clients that
    explicitly advertised it.
    A second run in the same environment (the fish launch runs it before
    `exec fish` and again in the agent's script shell) sees the wrapper in
    VISUAL/EDITOR and keeps the machine editor captured by the first run.
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
[ "$ghostex_prompt_editor_machine_visual" != "$ghostex_prompt_editor_wrapper" ] || ghostex_prompt_editor_machine_visual="${GHOSTEX_PROMPT_EDITOR_MACHINE_VISUAL:-}"
[ "$ghostex_prompt_editor_machine_editor" != "$ghostex_prompt_editor_wrapper" ] || ghostex_prompt_editor_machine_editor="${GHOSTEX_PROMPT_EDITOR_MACHINE_EDITOR:-}"
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
printf '%s\n' 'Ghostex could not find its ghostex command, so the prompt editor did not open. Restart this session from Ghostex.' >&2
exit 127
__GHOSTEX_PROMPT_EDITOR_WRAPPER__
chmod 755 "$ghostex_prompt_editor_wrapper" 2>/dev/null || true
export GHOSTEX_PROMPT_EDITOR_MACHINE_VISUAL="$ghostex_prompt_editor_machine_visual"
export GHOSTEX_PROMPT_EDITOR_MACHINE_EDITOR="$ghostex_prompt_editor_machine_editor"
export EDITOR="$ghostex_prompt_editor_wrapper"
export VISUAL="$ghostex_prompt_editor_wrapper"
"#
    .trim()
    .to_string();
    if let Some(cli) = session_ghostex_cli_executable() {
        script.push_str(&format!(
            "\nexport GHOSTEX_CLI_EXECUTABLE={}",
            shell_quote(&cli)
        ));
    }
    script.push_str(&format!(
        "\nexport {}={}",
        super::PROMPT_HANDSHAKE_DIR_ENV,
        shell_quote(&super::prompt_handshake_dir().to_string_lossy())
    ));
    if prompt_editor == Some("monaco") {
        script.push_str("\nexport GHOSTEX_PROMPT_EDITOR_BACKEND=monaco");
    }
    script.push_str("\nexport GHOSTEX_PROMPT_EDITING_ENABLED=1");
    script
}

/// CDXC:PromptEditor 2026-09-24 WHY:
/// Chat sends reach the agent through its external editor (Ctrl+G), and the `$EDITOR` wrapper can only answer that handshake by running `ghostex prompt-editor`. The wrapper used to find the CLI through `GHOSTEX_CLI_EXECUTABLE`, which nothing had exported since the Swift host was retired, then through `ghostex` on PATH, then fell back to `vi`; on a computer without the CLI on PATH every chat send opened `vi`, the handshake never completed, and the agent stayed wedged in the editor. Every session is now pinned to the `ghostex` binary shipped beside this gxserver (the same pairing the Windows provider uses), and the wrapper fails with a message instead of opening an editor that cannot answer. The machine's own VISUAL/EDITOR still runs when the user chose it, through `ghostex prompt-editor`'s inherit backend.
/// SEE-ALSO: `scripts_windows.rs` `start` pins the Windows CLI the same way; `gpui_auto_install_ghostex_cli_wrappers` in apps/desktop keeps `ghostex` on PATH for sessions started before the pin.
#[cfg(not(windows))]
fn session_ghostex_cli_executable() -> Option<String> {
    let gxserver = std::env::current_exe().ok()?;
    let gxserver = std::fs::canonicalize(&gxserver).unwrap_or(gxserver);
    let cli = crate::ghostex_cli::launchers::package_bin_dir(&gxserver)?.join("ghostex");
    crate::ghostex_cli::launchers::is_executable_file_sync(&cli)
        .then(|| cli.to_string_lossy().into_owned())
}

#[cfg(not(windows))]
fn zmx_prompt_editor_attach_args(prompt_editor: Option<&str>) -> &'static str {
    match prompt_editor {
        Some("monaco") => "--prompt-editor=monaco",
        Some("code-server") => "--prompt-editor=code-server",
        _ => "",
    }
}

#[cfg(not(windows))]
fn zmx_session_identity_reset_shell_command() -> String {
    format!("unset {}", session_identity_environment_keys().join(" "))
}

#[cfg(not(windows))]
fn persistence_notice_shell_command(session_name: &str) -> String {
    format!(
        "printf '%s\\n' {}",
        shell_quote(&format!(
            "This session is using zmx persistence: {session_name}"
        ))
    )
}

#[cfg(not(windows))]
fn session_title_shell_command(title: Option<&str>) -> String {
    let Some(title) = title.map(str::trim).filter(|title| !title.is_empty()) else {
        return String::new();
    };
    format!("printf '%s\\n' {}", shell_quote(title))
}

#[cfg(not(windows))]
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

#[cfg(not(windows))]
pub(crate) fn shell_quote(value: &str) -> String {
    format!("'{}'", value.replace('\'', "'\\''"))
}
