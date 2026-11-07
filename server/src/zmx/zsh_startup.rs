use super::scripts::shell_quote;

/// CDXC:Terminal 2026-09-05 WHY:
/// Running an agent with `zsh -lic 'agent; exec zsh -li'` reloads the user's profiles after the agent exits; a second Ctrl+C can interrupt that startup and leave the default prompt with a partially loaded .zshrc.
/// Launch once from the login shell's first precmd instead, so the same fully initialized shell survives the agent, including repeated interrupts.
///
/// CDXC:Terminal 2026-10-14 WHY:
/// Powerlevel10k's instant prompt points stdin at /dev/null and stdout/stderr at a temp file while .zshrc loads, and gives the terminal back only when the first prompt is expanded, after every precmd hook and `sched +0` event (moving the launch to `sched +0` was tried and still saw /dev/null). An agent started from the first precmd therefore had no terminal: Codex quit with "stdin is not a terminal" (GitHub #228, zsh with p10k under WSL). So the hook first asks p10k to end its instant prompt (`p10k clear-instant-prompt`, a no-op when none is showing), which restores the terminal and erases the cached prompt before the agent draws.
///
/// CDXC:AgentProviders 2026-10-18 WHY:
/// The `codex` wrapper (codex_wrapper.rs) must stay first on PATH for a `codex` typed at this shell, but the user's .zshrc and per-prompt PATH managers (mise, direnv) prepend their own folders after the provider's setup ran. The startup's setup puts the wrapper's folder in front when the first precmd runs, and `_ghostex_session_bin_first`, appended to precmd_functions after every hook the user's config registered, keeps it there before each prompt.
pub(super) fn agent_shell_command(shell_path: &str, startup: &str) -> String {
    let zshenv = format!(
        r#"
{RESTORE_ZDOTDIR}
{SESSION_BIN_FIRST}
function _ghostex_start_agent() {{
  precmd_functions=("${{(@)precmd_functions:#_ghostex_start_agent}}")
  builtin unfunction _ghostex_start_agent
  precmd_functions+=(_ghostex_session_bin_first)
  (( ! ${{+functions[p10k]}} )) || p10k clear-instant-prompt
  builtin eval {}
  return 0
}}
precmd_functions=(_ghostex_start_agent "${{precmd_functions[@]}}")
"#,
        shell_quote(startup),
    );
    exec_with_zshenv(shell_path, &zshenv)
}

/// A plain terminal's zsh, with only `_ghostex_session_bin_first` added, so a `zmx` or `codex` typed there finds session-bin's entry ahead of the user's own folders (CDXC:Zmx in session_bin.rs, CDXC:AgentProviders in codex_wrapper.rs).
///
/// CDXC:AgentProviders 2026-10-18 WHY:
/// A function appended to precmd_functions during a precmd pass only runs from the next prompt on, so the first prompt of a plain terminal still had mise's folder in front (seen live: `which codex` gave mise's Codex). The first precmd therefore also queues `sched +0 _ghostex_session_bin_first`, which zsh runs after the remaining precmd hooks of that pass. An agent session needs no such step: its first precmd runs the agent, and the user's next prompt is already a second pass.
pub(super) fn plain_shell_command(shell_path: &str) -> String {
    let zshenv = format!(
        r#"
{RESTORE_ZDOTDIR}
{SESSION_BIN_FIRST}
function _ghostex_session_bin_hook() {{
  precmd_functions=("${{(@)precmd_functions:#_ghostex_session_bin_hook}}")
  builtin unfunction _ghostex_session_bin_hook
  precmd_functions+=(_ghostex_session_bin_first)
  builtin zmodload zsh/sched 2>/dev/null && builtin sched +0 _ghostex_session_bin_first
  return 0
}}
precmd_functions=(_ghostex_session_bin_hook "${{precmd_functions[@]}}")
"#
    );
    exec_with_zshenv(shell_path, &zshenv)
}

/// Removes the temporary ZDOTDIR that carried Ghostex's `.zshenv`, restores the user's, and runs their `.zshenv`, so the user's startup files load exactly as without Ghostex.
const RESTORE_ZDOTDIR: &str = r#"command rm -f -- "$ZDOTDIR/.zshenv"
command rmdir -- "$ZDOTDIR"
if [[ -n ${GHOSTEX_ZSH_ORIGINAL_ZDOTDIR+x} ]]; then
  builtin export ZDOTDIR="$GHOSTEX_ZSH_ORIGINAL_ZDOTDIR"
else
  builtin unset ZDOTDIR
fi
builtin unset GHOSTEX_ZSH_ORIGINAL_ZDOTDIR
[[ ! -r "${ZDOTDIR-$HOME}/.zshenv" ]] || builtin source -- "${ZDOTDIR-$HOME}/.zshenv""#;

const SESSION_BIN_FIRST: &str = r#"function _ghostex_session_bin_first() {
  [[ -n ${GHOSTEX_SESSION_BIN_DIR-} ]] || return 0
  path=("$GHOSTEX_SESSION_BIN_DIR" "${(@)path:#${(b)GHOSTEX_SESSION_BIN_DIR}}")
}"#;

/// Starts the login zsh with `zshenv` as its first startup file, through a temporary ZDOTDIR.
fn exec_with_zshenv(shell_path: &str, zshenv: &str) -> String {
    format!(
        r#"
ghostex_zsh_startup_dir=$(mktemp -d "${{TMPDIR:-/tmp}}/ghostex-zsh.XXXXXXXX") || exit
printf '%s\n' {} > "$ghostex_zsh_startup_dir/.zshenv" || exit
if [ "${{ZDOTDIR+x}}" = x ]; then
  export GHOSTEX_ZSH_ORIGINAL_ZDOTDIR="$ZDOTDIR"
else
  unset GHOSTEX_ZSH_ORIGINAL_ZDOTDIR
fi
export ZDOTDIR="$ghostex_zsh_startup_dir"
exec {} -li
"#,
        shell_quote(zshenv.trim()),
        shell_quote(shell_path),
    )
    .trim()
    .to_string()
}
