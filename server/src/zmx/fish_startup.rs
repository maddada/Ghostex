use super::scripts::shell_quote;
use crate::platform::shell::PlatformShell;

/// CDXC:Terminal 2026-10-04 WHY:
/// Running a fish user's agent from the `/bin/zsh -lic` script shell gave it zsh's PATH, so agents installed where only config.fish looks (`~/.local/bin` for omp and claude) failed with "command not found" and the session opened on an empty fish prompt.
/// Launch it from fish's `--init-command`, after config.fish has run, so the agent sees the user's real PATH and the same fish survives the agent. The startup (the prompt-editor setup and the agent command) still runs in the script shell as an interactive login shell (`zsh -lic`, as before this change) because launch commands and custom agent commands are POSIX syntax and may rely on aliases or variables from the user's zsh profiles; the agent therefore keeps everything it had before and only gains fish's PATH. The setup must run there, after config.fish and .zshrc: a config.fish that sets EDITOR or VISUAL (`set -x VISUAL code`) otherwise replaced Ghostex's prompt-editor wrapper, so a chat send that opened the agent's external editor launched VS Code and the agent waited on "Save and close editor to continue...". The startup travels in an environment variable that fish removes before running it, so no quoting crosses the two shells.
/// The setup also runs once before `exec fish` (superseding the 2026-10-01 version, which ran it only for the agent): the fish left after the agent exits otherwise lacked the prompt-editor environment, so an agent restarted by hand in it opened the user's own editor and could hang on the same prompt.
/// SEE-ALSO: server/src/zmx/zsh_startup.rs starts zsh users' agents from the first precmd for the same reason.
pub(super) fn agent_shell_command(
    shell_path: &str,
    script_shell: &PlatformShell,
    prompt_editor_setup: &str,
    startup: &str,
) -> String {
    let init_command = format!(
        "{SESSION_BIN_FIRST} set -l ghostex_agent_command $GHOSTEX_AGENT_STARTUP_COMMAND; set -e GHOSTEX_AGENT_STARTUP_COMMAND; {} {} $ghostex_agent_command; set -e ghostex_agent_command",
        shell_quote(&script_shell.executable),
        script_shell.command_flag(true),
    );
    format!(
        "{}\nexport GHOSTEX_AGENT_STARTUP_COMMAND={}\nexec {} -l -i -C {}",
        prompt_editor_setup,
        shell_quote(startup),
        shell_quote(shell_path),
        shell_quote(&init_command),
    )
}

/// A plain terminal's fish, with only the session-bin step from the agent launch, so a `zmx` or `codex` typed there finds session-bin's entry ahead of the user's own folders (CDXC:Zmx in session_bin.rs, CDXC:AgentProviders in codex_wrapper.rs).
pub(super) fn plain_shell_command(shell_path: &str, setup: &str) -> String {
    format!(
        "{}\nexec {} -l -i -C {}",
        setup,
        shell_quote(shell_path),
        shell_quote(SESSION_BIN_FIRST.trim_end()),
    )
}

/// CDXC:AgentProviders 2026-10-18 WHY: config.fish and per-prompt PATH managers prepend their folders after the provider's setup ran, so the `codex` wrapper's folder (codex_wrapper.rs) goes back in front once config.fish has run (this runs in `--init-command`) and before each prompt (a `fish_prompt` handler defined after config.fish's). SEE-ALSO: `_ghostex_session_bin_first` in zsh_startup.rs.
const SESSION_BIN_FIRST: &str = "if set -q GHOSTEX_SESSION_BIN_DIR; function _ghostex_session_bin_first --on-event fish_prompt; while contains -- $GHOSTEX_SESSION_BIN_DIR $PATH; set -e PATH[(contains -i -- $GHOSTEX_SESSION_BIN_DIR $PATH)]; end; set -gx PATH $GHOSTEX_SESSION_BIN_DIR $PATH; end; _ghostex_session_bin_first; end;";
