use super::scripts::shell_quote;

/// Starts the user's login bash with `<state>/session-bin` kept first on PATH after their config has run.
///
/// CDXC:Zmx 2026-11-07 WHY:
/// The session's `zmx` entry (session_bin.rs) and `codex` wrapper must stay first on PATH, but a bash login reads .bash_profile (or .profile) after the provider's setup ran, and lines such as Homebrew's `eval "$(brew shellenv)"` or `PATH="$HOME/.local/bin:$PATH"` put their folders in front again, so `zmx` reached the user's own zmx. A login bash runs no startup file Ghostex can append to without editing the user's files, so the step travels in PROMPT_COMMAND, which bash takes from the environment and runs before each prompt. Prompt hooks that keep an existing PROMPT_COMMAND (mise, direnv, starship) put theirs in front of it, so this one runs after them, like `_ghostex_session_bin_first` at the end of zsh's precmd_functions. The step stops PROMPT_COMMAND being exported, so programs started from the prompt do not inherit it; a config that replaces PROMPT_COMMAND outright keeps the PATH its config sets, as before this change.
/// SEE-ALSO: zsh_startup.rs and fish_startup.rs (the same step for zsh and fish).
pub(super) fn login_shell_exec_command(shell_path: &str) -> String {
    format!(
        "export PROMPT_COMMAND=\"${{PROMPT_COMMAND:+$PROMPT_COMMAND\n}}\"{}\nexec {} -li",
        shell_quote(SESSION_BIN_FIRST),
        shell_quote(shell_path),
    )
}

/// Puts `$GHOSTEX_SESSION_BIN_DIR` first on PATH (once) when something else moved in front of it. Bash 3.2 syntax, for macOS's /bin/bash.
const SESSION_BIN_FIRST: &str = r#"export -n PROMPT_COMMAND 2>/dev/null
if [ -n "${GHOSTEX_SESSION_BIN_DIR:-}" ]; then case "$PATH" in "$GHOSTEX_SESSION_BIN_DIR"|"$GHOSTEX_SESSION_BIN_DIR":*) ;; *) ghostex_path_rest=":$PATH:"; while case "$ghostex_path_rest" in *":$GHOSTEX_SESSION_BIN_DIR:"*) true ;; *) false ;; esac; do ghostex_path_rest=${ghostex_path_rest/":$GHOSTEX_SESSION_BIN_DIR:"/:}; done; ghostex_path_rest=${ghostex_path_rest#:}; ghostex_path_rest=${ghostex_path_rest%:}; PATH="$GHOSTEX_SESSION_BIN_DIR${ghostex_path_rest:+:$ghostex_path_rest}"; unset ghostex_path_rest ;; esac; fi"#;
