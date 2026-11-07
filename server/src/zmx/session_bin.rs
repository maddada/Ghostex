//! `<state>/session-bin`: the folder Ghostex puts first on every zmx session's PATH, holding the `zmx` entry below and, when the installed Codex allows it, the `codex` wrapper (codex_wrapper.rs).
//!
//! CDXC:Zmx 2026-11-07 DECISION:
//! User, asked whether Ghostex's zmx or the zmx already on PATH should win in a Ghostex session (Q48): "Ghostex's zmx comes first". A `zmx` typed in any zmx session (agent or plain terminal, zsh, bash or fish, on macOS, Linux, WSL and remote Linux hosts) runs the zmx this session's gxserver uses, ahead of any other zmx on PATH. The user's own zmx stays where it was and runs by its full path; their shell config files are not touched.
//!
//! CDXC:Zmx 2026-11-07 WHY:
//! On the user's Mac `zmx` ran a Homebrew zmx installed in May (it reports 0.6.0) that cannot read Ghostex's sessions: `zmx list` showed every session as unreachable and `zmx history` printed nothing, while Ghostex's own zmx (0.8.1, wire generation 3) lists them. The entry is a tiny script that execs `$GHOSTEX_ZMX_BIN`, not a symlink. Every provider script exports `GHOSTEX_ZMX_BIN` as the exact zmx path gxserver used for that session (the same build, so the same wire generation and socket folder), while session-bin is one folder shared by every session of the install: a symlink holds a single target that the last session to start would set, and on Linux each update unpacks a new release folder, so older sessions can run a different zmx than the newest. When that path is gone (the app was moved, an old Linux release was removed), the script runs the zmx the most recent session start wrote into it; a symlink would dangle instead. The script never runs a file carrying its own marker, so it cannot loop through another copy of itself, and Ghostex's own zmx calls are unaffected because they all run `$zmx_bin` by full path (server/src/zmx/scripts.rs).
//! SEE-ALSO: codex_wrapper.rs (the `codex` wrapper in the same folder); zsh_startup.rs, fish_startup.rs and bash_startup.rs (the folder back in front after the user's config).

/// First-line marker of the `zmx` entry, so the entry never runs another copy of itself.
const ZMX_MARKER: &str = "ghostex-zmx-entry";

/// The `zmx` entry. `written_zmx` is the zmx path of the session start that writes it, run only when the session's own `GHOSTEX_ZMX_BIN` is unusable.
fn posix_zmx_entry(written_zmx: &str) -> String {
    // A newline in the path could end the heredoc that writes this file; such a path is only skipped as the fallback.
    let written_zmx = if written_zmx.contains('\n') {
        ""
    } else {
        written_zmx
    };
    format!(
        r#"#!/bin/sh
# {ZMX_MARKER}
# Written by Ghostex for its sessions: runs the zmx this session's Ghostex uses, ahead of any other zmx on PATH. Another zmx still runs by its full path.
ghostex_zmx_written={}
for ghostex_zmx in "${{GHOSTEX_ZMX_BIN:-}}" "$ghostex_zmx_written"; do
  [ -n "$ghostex_zmx" ] && [ -f "$ghostex_zmx" ] && [ -x "$ghostex_zmx" ] || continue
  head -c 200 "$ghostex_zmx" 2>/dev/null | grep -q '{ZMX_MARKER}' && continue
  exec "$ghostex_zmx" "$@"
done
printf '%s\n' "zmx: Ghostex's zmx was not found (${{GHOSTEX_ZMX_BIN:-GHOSTEX_ZMX_BIN is not set}}). Open a new Ghostex session, or run another zmx by its full path." >&2
exit 127
"#,
        super::scripts::shell_quote(written_zmx),
    )
}

/// POSIX shell lines for a zmx provider's setup: write the `zmx` entry and the `codex` wrapper into `<state>/session-bin` and put that folder first on PATH. Each file is replaced atomically, since other sessions may be running it. The provider's own shell writes them, the same way it writes the prompt-editor wrapper, so they land on the filesystem the session runs on (WSL, a remote Linux host).
pub(crate) fn zmx_provider_session_bin_setup_shell_command(zmx_executable_path: &str) -> String {
    format!(
        r#"
case "${{GHOSTEX_HOME:-}}" in
  /*) ghostex_session_bin_dir="$GHOSTEX_HOME/state/session-bin" ;;
  *) case "${{XDG_STATE_HOME:-}}" in
       /*) ghostex_session_bin_dir="${{XDG_STATE_HOME%/}}/ghostex/session-bin" ;;
       *) ghostex_session_bin_dir="$HOME/.local/state/ghostex/session-bin" ;;
     esac ;;
esac
mkdir -p "$ghostex_session_bin_dir" 2>/dev/null || true
cat > "$ghostex_session_bin_dir/zmx.$$" <<'__GHOSTEX_ZMX_ENTRY__'
{}__GHOSTEX_ZMX_ENTRY__
if chmod 755 "$ghostex_session_bin_dir/zmx.$$" 2>/dev/null && mv -f "$ghostex_session_bin_dir/zmx.$$" "$ghostex_session_bin_dir/zmx" 2>/dev/null; then :; else rm -f "$ghostex_session_bin_dir/zmx.$$" 2>/dev/null; fi
{}
if [ -x "$ghostex_session_bin_dir/zmx" ] || [ -x "$ghostex_session_bin_dir/codex" ]; then
  PATH=$(printf '%s\n' "$PATH" | tr ':' '\n' | grep -v -x -F -- "$ghostex_session_bin_dir" | paste -s -d: -)
  export PATH="$ghostex_session_bin_dir${{PATH:+:$PATH}}"
  export GHOSTEX_SESSION_BIN_DIR="$ghostex_session_bin_dir"
fi
"#,
        posix_zmx_entry(zmx_executable_path),
        super::codex_wrapper::zmx_provider_codex_wrapper_write_shell_command(),
    )
    .trim()
    .to_string()
}
