use super::scripts::shell_quote;

/// CDXC:Terminal 2026-09-05 WHY:
/// Running an agent with `zsh -lic 'agent; exec zsh -li'` reloads the user's profiles after the agent exits; a second Ctrl+C can interrupt that startup and leave the default prompt with a partially loaded .zshrc.
/// Launch once from the login shell's first precmd instead, so the same fully initialized shell survives the agent, including repeated interrupts.
pub(super) fn agent_shell_command(shell_path: &str, startup: &str) -> String {
    let zshenv = format!(
        r#"
command rm -f -- "$ZDOTDIR/.zshenv"
command rmdir -- "$ZDOTDIR"
if [[ -n ${{GHOSTEX_ZSH_ORIGINAL_ZDOTDIR+x}} ]]; then
  builtin export ZDOTDIR="$GHOSTEX_ZSH_ORIGINAL_ZDOTDIR"
else
  builtin unset ZDOTDIR
fi
builtin unset GHOSTEX_ZSH_ORIGINAL_ZDOTDIR
[[ ! -r "${{ZDOTDIR-$HOME}}/.zshenv" ]] || builtin source -- "${{ZDOTDIR-$HOME}}/.zshenv"
function _ghostex_start_agent() {{
  precmd_functions=("${{(@)precmd_functions:#_ghostex_start_agent}}")
  builtin unfunction _ghostex_start_agent
  builtin eval {}
  return 0
}}
precmd_functions=(_ghostex_start_agent "${{precmd_functions[@]}}")
"#,
        shell_quote(startup),
    );
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
