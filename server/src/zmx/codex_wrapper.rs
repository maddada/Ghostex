//! CDXC:AgentProviders 2026-10-18 DECISION:
//! User: "can we force running it as no daemon like we do for others?" and "i wanna unify a and b and do a for all". Every Ghostex session puts a `codex` wrapper first on its PATH, so a `codex`, `codex resume` or `codex fork` typed by hand in a session's shell runs without Codex's shared background server, like the Codex sessions Ghostex launches itself (`with_codex_no_daemon`). The wrapper's folder goes back to the front of PATH after the user's shell config has run (zsh and fish sessions, agent and plain, and every PowerShell session).
//!
//! CDXC:AgentProviders 2026-10-18 WHY:
//! A hand-typed `codex` joined Codex's shared server, which keeps the conversation loaded and its writer lock held for 30 minutes after the TUI quits (`thread_unload_delay_secs`, default 1800). Ghostex's own `codex --no-daemon resume <id>` then stopped on "This conversation is open in another app" and every chat send hung (GitHub #256). Codex 0.163 has no environment variable or config key that turns the shared server off: only command-line flags do, and `features.daemon_auto_start = false` still joins a server that is already running. Hence a wrapper rather than session environment. It adds `--no-daemon` only to the interactive TUI and passes every other subcommand, and any call already carrying `--no-daemon` or `--remote`, through untouched.
//! SEE-ALSO: server/src/agents/codex_daemon.rs (`with_codex_no_daemon`, the same flag on Ghostex's launches); server/src/zmx/zsh_startup.rs and fish_startup.rs (the folder back in front after the user's config); server/src/zmx/scripts_windows.rs (`start`).

/// First-line marker every wrapper copy carries, so a wrapper never resolves another Ghostex copy's wrapper as the real `codex`.
const MARKER: &str = "ghostex-codex-wrapper";

/// Codex subcommands that never start the interactive TUI (`codex --help`, 0.163). `resume` and `fork` are TUI and get the flag.
const NON_TUI_SUBCOMMANDS: &str = "exec e review login logout mcp mcp-server plugin app-server remote-control app completion update doctor sandbox debug apply a queue archive delete migrate-rollouts unarchive cloud exec-server features help agents";

/// Codex options that take their value as the next argument (`codex --help`, 0.163).
const VALUE_OPTIONS: &str = "-c --config --enable --disable --remote-auth-token-env -i --image -m --model --local-provider -p --profile -s --sandbox -C --cd --add-dir -a --ask-for-approval";

#[cfg(not(windows))]
fn posix_wrapper() -> String {
    format!(
        r#"#!/bin/sh
# {MARKER}
# Written by Ghostex for its sessions: runs the interactive Codex TUI without Codex's shared background server.
ghostex_real=""
ghostex_ifs=$IFS
set -f
IFS=:
for ghostex_dir in $PATH; do
  [ -n "$ghostex_dir" ] || ghostex_dir=.
  ghostex_candidate="$ghostex_dir/codex"
  [ -f "$ghostex_candidate" ] && [ -x "$ghostex_candidate" ] || continue
  head -c 200 "$ghostex_candidate" 2>/dev/null | grep -q '{MARKER}' && continue
  ghostex_real=$ghostex_candidate
  break
done
IFS=$ghostex_ifs
set +f
if [ -z "$ghostex_real" ]; then
  printf '%s\n' 'codex: command not found' >&2
  exit 127
fi
ghostex_add=1
for ghostex_arg in "$@"; do
  case "$ghostex_arg" in
    --) break ;;
    --no-daemon|--remote|--remote=*|-h|--help|-V|--version) ghostex_add=0; break ;;
  esac
done
if [ "$ghostex_add" = 1 ]; then
  ghostex_skip=0
  for ghostex_arg in "$@"; do
    if [ "$ghostex_skip" = 1 ]; then ghostex_skip=0; continue; fi
    case " {VALUE_OPTIONS} " in
      *" $ghostex_arg "*) ghostex_skip=1; continue ;;
    esac
    case "$ghostex_arg" in
      --) break ;;
      -*) continue ;;
    esac
    case " {NON_TUI_SUBCOMMANDS} " in
      *" $ghostex_arg "*) ghostex_add=0 ;;
    esac
    break
  done
fi
if [ "$ghostex_add" = 1 ]; then
  exec "$ghostex_real" --no-daemon "$@"
fi
exec "$ghostex_real" "$@"
"#
    )
}

/// Whether sessions get the wrapper: only when the installed Codex accepts `--no-daemon` (0.156+), the same gate as Ghostex's own Codex launches.
pub(crate) fn codex_wrapper_enabled() -> bool {
    crate::agents::installed_codex_supports_no_daemon()
}

/// POSIX shell lines for a zmx provider's setup: write the wrapper into `<state>/session-bin` (atomically, since other sessions may be running it) and put that folder first on PATH. The provider's own shell writes it, the same way it writes the prompt-editor wrapper, so it lands on the filesystem the session runs on (WSL, a remote Linux host). Empty when the installed Codex predates `--no-daemon`, the same gate as Ghostex's own Codex launches.
#[cfg(not(windows))]
pub(crate) fn zmx_provider_codex_wrapper_setup_shell_command() -> String {
    if !codex_wrapper_enabled() {
        return String::new();
    }
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
cat > "$ghostex_session_bin_dir/codex.$$" <<'__GHOSTEX_CODEX_WRAPPER__'
{}__GHOSTEX_CODEX_WRAPPER__
if chmod 755 "$ghostex_session_bin_dir/codex.$$" 2>/dev/null && mv -f "$ghostex_session_bin_dir/codex.$$" "$ghostex_session_bin_dir/codex" 2>/dev/null; then :; else rm -f "$ghostex_session_bin_dir/codex.$$" 2>/dev/null; fi
if [ -x "$ghostex_session_bin_dir/codex" ]; then
  PATH=$(printf '%s\n' "$PATH" | tr ':' '\n' | grep -v -x -F -- "$ghostex_session_bin_dir" | paste -s -d: -)
  export PATH="$ghostex_session_bin_dir${{PATH:+:$PATH}}"
  export GHOSTEX_SESSION_BIN_DIR="$ghostex_session_bin_dir"
fi
"#,
        posix_wrapper()
    )
    .trim()
    .to_string()
}

#[cfg(windows)]
fn powershell_wrapper() -> String {
    let value_options = VALUE_OPTIONS
        .split_whitespace()
        .map(|option| format!("'{option}'"))
        .collect::<Vec<_>>()
        .join(",");
    let subcommands = NON_TUI_SUBCOMMANDS
        .split_whitespace()
        .map(|name| format!("'{name}'"))
        .collect::<Vec<_>>()
        .join(",");
    format!(
        r#"# {MARKER}
# Written by Ghostex for its sessions: runs the interactive Codex TUI without Codex's shared background server.
$ghostexReal = Get-Command -Name codex -CommandType Application,ExternalScript -All -ErrorAction SilentlyContinue | Where-Object {{
  $item = Get-Item -LiteralPath $_.Source -ErrorAction SilentlyContinue
  -not ($item -and $item.Length -lt 65536 -and (Select-String -LiteralPath $_.Source -Pattern '{MARKER}' -SimpleMatch -Quiet))
}} | Select-Object -First 1
if (-not $ghostexReal) {{
  [Console]::Error.WriteLine('codex: command not found')
  $global:LASTEXITCODE = 127
  if (-not $ghostexCodexFunction) {{ exit 127 }}
  return
}}
$ghostexAdd = $true
foreach ($ghostexArg in $args) {{
  if ($ghostexArg -eq '--') {{ break }}
  if ($ghostexArg -in @('--no-daemon','--remote','-h','--help','-V','--version') -or "$ghostexArg".StartsWith('--remote=')) {{ $ghostexAdd = $false; break }}
}}
if ($ghostexAdd) {{
  $ghostexSkip = $false
  foreach ($ghostexArg in $args) {{
    if ($ghostexSkip) {{ $ghostexSkip = $false; continue }}
    if ($ghostexArg -cin @({value_options})) {{ $ghostexSkip = $true; continue }}
    if ($ghostexArg -eq '--') {{ break }}
    if ("$ghostexArg".StartsWith('-')) {{ continue }}
    if ($ghostexArg -cin @({subcommands})) {{ $ghostexAdd = $false }}
    break
  }}
}}
if ($ghostexAdd) {{ & $ghostexReal.Source --no-daemon @args }} else {{ & $ghostexReal.Source @args }}
# The session's `codex` function runs this text in the shell itself, where `exit` would close the shell.
if (-not $ghostexCodexFunction) {{ exit $LASTEXITCODE }}
"#
    )
}

/// What cmd.exe and child processes resolve `codex` to. It hands the call to the PowerShell wrapper beside it instead of repeating its rules in batch.
#[cfg(windows)]
fn cmd_wrapper() -> String {
    format!(
        "@echo off\r\nrem {MARKER}\r\nrem Written by Ghostex for its sessions: runs the interactive Codex TUI without Codex's shared background server.\r\npowershell.exe -NoLogo -NoProfile -ExecutionPolicy Bypass -File \"%~dp0codex-wrapper.ps1\" %*\r\nexit /b %ERRORLEVEL%\r\n"
    )
}

/// Writes `codex-wrapper.ps1` and `codex.cmd` into `<state>/session-bin` and returns the PowerShell, run after the user's profile, that puts the folder first on PATH and defines the session's `codex` function. Empty when the installed Codex predates `--no-daemon` or the folder cannot be written.
///
/// CDXC:AgentProviders 2026-10-18 WHY: a `codex.ps1` first on PATH would stop every `codex` under the Restricted execution policy (Windows PowerShell's default on client Windows), even where the real Codex is `codex.exe`. The session's own PowerShell gets a `codex` function instead, which runs the wrapper's text through `ScriptBlock::Create` (not subject to the policy) and outranks every file on PATH; the wrapper file is named so nothing resolves it as `codex`, and `codex.cmd` serves cmd.exe and child processes.
#[cfg(windows)]
pub(crate) fn wmx_codex_wrapper_path_command() -> String {
    if !codex_wrapper_enabled() {
        return String::new();
    }
    let directory = ghostex_paths::GhostexPaths::resolve()
        .state_dir
        .join("session-bin");
    let written = std::fs::create_dir_all(&directory).is_ok()
        && write_if_changed(&directory.join("codex-wrapper.ps1"), &powershell_wrapper())
        && write_if_changed(&directory.join("codex.cmd"), &cmd_wrapper());
    if !written {
        return String::new();
    }
    let directory =
        crate::platform::shell::powershell_single_quoted_content(&directory.to_string_lossy());
    format!(
        "$env:Path='{directory}' + ';' + ((($env:Path -split ';') | Where-Object {{ $_ -and $_ -ne '{directory}' }}) -join ';'); function global:codex {{ $ghostexCodexFunction = $true; & ([ScriptBlock]::Create([IO.File]::ReadAllText('{directory}\\codex-wrapper.ps1'))) @args }}; "
    )
}

/// Replaces the file through a sibling temp file, so a session running the wrapper never reads a half-written one.
#[cfg(windows)]
fn write_if_changed(path: &std::path::Path, contents: &str) -> bool {
    if std::fs::read_to_string(path).is_ok_and(|current| current == contents) {
        return true;
    }
    let temp = path.with_extension(format!("{}.tmp", std::process::id()));
    if std::fs::write(&temp, contents).is_err() {
        return false;
    }
    std::fs::rename(&temp, path).is_ok() || {
        let _ = std::fs::remove_file(&temp);
        false
    }
}
