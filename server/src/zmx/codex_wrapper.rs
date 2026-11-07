//! CDXC:AgentProviders 2026-10-18 DECISION:
//! User: "can we force running it as no daemon like we do for others?" and "i wanna unify a and b and do a for all". Every Ghostex session puts a `codex` wrapper first on its PATH, so a `codex`, `codex resume` or `codex fork` typed by hand in a session's shell runs without Codex's shared background server, like the Codex sessions Ghostex launches itself (`with_codex_no_daemon`). The wrapper's folder goes back to the front of PATH after the user's shell config has run (zsh and fish sessions, agent and plain, and every PowerShell session).
//!
//! CDXC:AgentProviders 2026-10-18 WHY:
//! A hand-typed `codex` joined Codex's shared server, which keeps the conversation loaded and its writer lock held for 30 minutes after the TUI quits (`thread_unload_delay_secs`, default 1800). Ghostex's own `codex --no-daemon resume <id>` then stopped on "This conversation is open in another app" and every chat send hung (GitHub #256). Codex 0.163 has no environment variable or config key that turns the shared server off: only command-line flags do, and `features.daemon_auto_start = false` still joins a server that is already running. Hence a wrapper rather than session environment. It adds `--no-daemon` only to the interactive TUI and passes every other subcommand, and any call already carrying `--no-daemon` or `--remote`, through untouched.
//!
//! CDXC:AgentProviders 2026-10-27 DECISION:
//! GitHub #325: with Superset installed, Ghostex's wrapper ran Superset's `codex` wrapper, which runs the first `codex` outside its own folder: Ghostex's wrapper again. The two called each other until macOS refused to fork. Every pass now records the `codex` it ran in `GHOSTEX_CODEX_WRAPPER_TRIED` and a re-entered wrapper skips those, so the chain reaches each other wrapper once (Superset keeps its hooks) and then the real Codex, whatever the other wrapper is called. When every `codex` on PATH was tried, the last one runs again: that is the real Codex when a Codex agent runs `codex exec` itself. `GHOSTEX_CODEX_WRAPPER_DEPTH` stops a chain after `MAX_DEPTH` passes, for wrappers that loop with no real Codex behind them. Each session's setup clears both, so a gxserver started from inside a Codex does not hand its chain to new sessions.
//! SEE-ALSO: server/src/agents/codex_daemon.rs (`with_codex_no_daemon`, the same flag on Ghostex's launches); server/src/zmx/session_bin.rs (the folder, its `zmx` entry and PATH); server/src/zmx/zsh_startup.rs, fish_startup.rs and bash_startup.rs (the folder back in front after the user's config); server/src/zmx/scripts_windows.rs (`start`).

/// First-line marker every wrapper copy carries, so a wrapper never resolves another Ghostex copy's wrapper as the real `codex`.
const MARKER: &str = "ghostex-codex-wrapper";

/// The `codex` files this call chain already ran, in order (`:`-separated on POSIX, `;` on Windows). A wrapper re-entered through another tool's `codex` wrapper skips them, so it moves on to the next `codex` on PATH instead of handing the call back to that tool (GitHub #325).
const TRIED_VARIABLE: &str = "GHOSTEX_CODEX_WRAPPER_TRIED";

/// How many times this call chain has passed through a Ghostex wrapper. Stops any loop the tried list cannot break (every other `codex` on PATH leads back here).
const DEPTH_VARIABLE: &str = "GHOSTEX_CODEX_WRAPPER_DEPTH";

/// Passes allowed per chain. A Codex agent running `codex exec` inside Codex adds one per level, so this leaves room for real nesting.
const MAX_DEPTH: u32 = 8;

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
ghostex_tried=${{{TRIED_VARIABLE}:-}}
ghostex_depth=${{{DEPTH_VARIABLE}:-0}}
case "$ghostex_depth" in ''|*[!0-9]*) ghostex_depth=0 ;; esac
if [ "$ghostex_depth" -ge {MAX_DEPTH} ]; then
  printf '%s\n' "codex: the codex commands on PATH keep running each other, so Ghostex stopped. Tried: $ghostex_tried" >&2
  exit 126
fi
ghostex_real=""
ghostex_ifs=$IFS
set -f
IFS=:
for ghostex_dir in $PATH; do
  [ -n "$ghostex_dir" ] || ghostex_dir=.
  ghostex_candidate="$ghostex_dir/codex"
  [ -f "$ghostex_candidate" ] && [ -x "$ghostex_candidate" ] || continue
  head -c 200 "$ghostex_candidate" 2>/dev/null | grep -q '{MARKER}' && continue
  case ":$ghostex_tried:" in *":$ghostex_candidate:"*) continue ;; esac
  ghostex_real=$ghostex_candidate
  break
done
IFS=$ghostex_ifs
set +f
if [ -n "$ghostex_real" ]; then
  ghostex_tried=${{ghostex_tried:+$ghostex_tried:}}$ghostex_real
elif [ -n "$ghostex_tried" ] && [ -f "${{ghostex_tried##*:}}" ] && [ -x "${{ghostex_tried##*:}}" ]; then
  ghostex_real=${{ghostex_tried##*:}}
else
  printf '%s\n' 'codex: command not found' >&2
  exit 127
fi
export {TRIED_VARIABLE}="$ghostex_tried"
export {DEPTH_VARIABLE}=$((ghostex_depth + 1))
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

/// POSIX shell lines, run by session_bin.rs's setup with `$ghostex_session_bin_dir` set, that write the wrapper into `<state>/session-bin` (atomically, since other sessions may be running it). Empty when the installed Codex predates `--no-daemon`, the same gate as Ghostex's own Codex launches.
///
/// CDXC:Zmx 2026-11-07 WHY: the folder is now first on every session's PATH for its `zmx` entry (session_bin.rs), not only when this gate passes. A closed gate still leaves a wrapper an earlier session wrote in place rather than removing it: the gate is a `codex --version` probe with a timeout, so it can read closed for a moment, and removing the shared file would take `--no-daemon` from the `codex` of every running session. A wrapper left over from before a Codex downgrade below 0.156 is the same exposure running sessions already had.
#[cfg(not(windows))]
pub(crate) fn zmx_provider_codex_wrapper_write_shell_command() -> String {
    if !codex_wrapper_enabled() {
        return String::new();
    }
    format!(
        r#"unset {TRIED_VARIABLE} {DEPTH_VARIABLE}
cat > "$ghostex_session_bin_dir/codex.$$" <<'__GHOSTEX_CODEX_WRAPPER__'
{}__GHOSTEX_CODEX_WRAPPER__
if chmod 755 "$ghostex_session_bin_dir/codex.$$" 2>/dev/null && mv -f "$ghostex_session_bin_dir/codex.$$" "$ghostex_session_bin_dir/codex" 2>/dev/null; then :; else rm -f "$ghostex_session_bin_dir/codex.$$" 2>/dev/null; fi"#,
        posix_wrapper()
    )
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
$ghostexSavedTried = $env:{TRIED_VARIABLE}
$ghostexSavedDepth = $env:{DEPTH_VARIABLE}
$ghostexTried = @("$ghostexSavedTried" -split ';' | Where-Object {{ $_ }})
$ghostexDepth = 0
if (-not [int]::TryParse("$ghostexSavedDepth", [ref]$ghostexDepth)) {{ $ghostexDepth = 0 }}
if ($ghostexDepth -ge {MAX_DEPTH}) {{
  [Console]::Error.WriteLine("codex: the codex commands on PATH keep running each other, so Ghostex stopped. Tried: $ghostexSavedTried")
  $global:LASTEXITCODE = 126
  if (-not $ghostexCodexFunction) {{ exit 126 }}
  return
}}
$ghostexReal = Get-Command -Name codex -CommandType Application,ExternalScript -All -ErrorAction SilentlyContinue | Where-Object {{
  $item = Get-Item -LiteralPath $_.Source -ErrorAction SilentlyContinue
  -not ($item -and $item.Length -lt 65536 -and (Select-String -LiteralPath $_.Source -Pattern '{MARKER}' -SimpleMatch -Quiet))
}} | Where-Object {{ $ghostexTried -notcontains $_.Source }} | Select-Object -First 1 | ForEach-Object {{ $_.Source }}
if ($ghostexReal) {{
  $ghostexTried += $ghostexReal
}} elseif ($ghostexTried.Count -gt 0 -and (Test-Path -LiteralPath $ghostexTried[-1] -PathType Leaf)) {{
  $ghostexReal = $ghostexTried[-1]
}} else {{
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
$env:{TRIED_VARIABLE} = $ghostexTried -join ';'
$env:{DEPTH_VARIABLE} = "$($ghostexDepth + 1)"
# The session's `codex` function runs in the session's own shell, so the chain's variables go back to what they were afterwards.
try {{
  if ($ghostexAdd) {{ & $ghostexReal --no-daemon @args }} else {{ & $ghostexReal @args }}
}} finally {{
  $env:{TRIED_VARIABLE} = $ghostexSavedTried
  $env:{DEPTH_VARIABLE} = $ghostexSavedDepth
}}
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
        "Remove-Item Env:{TRIED_VARIABLE},Env:{DEPTH_VARIABLE} -ErrorAction SilentlyContinue; $env:Path='{directory}' + ';' + ((($env:Path -split ';') | Where-Object {{ $_ -and $_ -ne '{directory}' }}) -join ';'); function global:codex {{ $ghostexCodexFunction = $true; & ([ScriptBlock]::Create([IO.File]::ReadAllText('{directory}\\codex-wrapper.ps1'))) @args }}; "
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

#[cfg(all(test, not(windows)))]
mod tests {
    use super::*;
    use std::os::unix::fs::PermissionsExt;
    use std::path::{Path, PathBuf};

    /// Like Superset's agent wrapper (GitHub #325): runs the first `codex` outside its own folder as a child, adding its flags. It gives up after 20 passes so a broken wrapper cannot fork-bomb the test machine.
    const LOOPING_WRAPPER: &str = r#"#!/bin/sh
self_dir=$(cd "$(dirname "$0")" && pwd)
echo pass >> "$WRAPPER_LOG"
[ "$(wc -l < "$WRAPPER_LOG")" -le 20 ] || { echo 'looped' >&2; exit 99; }
IFS=:
for dir in $PATH; do
  [ "$dir" = "$self_dir" ] && continue
  if [ -x "$dir/codex" ]; then "$dir/codex" --enable hooks "$@"; exit $?; fi
done
exit 127
"#;

    /// Stands in for Codex: prints its arguments, and for `nest` runs `codex exec inner` the way a Codex agent's shell command would.
    const FAKE_CODEX: &str = r#"#!/bin/sh
echo "codex $*"
case " $* " in *" nest "*) codex exec inner ;; esac
"#;

    fn write_script(dir: &Path, contents: &str) -> PathBuf {
        std::fs::create_dir_all(dir).unwrap();
        let path = dir.join("codex");
        std::fs::write(&path, contents).unwrap();
        std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o755)).unwrap();
        path
    }

    /// Runs `codex <args>` with PATH made of the given folders, the way a session's shell finds it.
    fn run_codex(
        root: &Path,
        path_dirs: &[PathBuf],
        args: &[&str],
    ) -> (i32, String, String, usize) {
        let log = root.join("wrapper.log");
        let _ = std::fs::remove_file(&log);
        std::fs::write(&log, "").unwrap();
        let mut path = path_dirs
            .iter()
            .map(|dir| dir.to_string_lossy().into_owned())
            .collect::<Vec<_>>();
        path.push("/usr/bin:/bin".to_string());
        let output = std::process::Command::new("/bin/sh")
            .arg("-c")
            .arg("codex \"$@\"")
            .arg("sh")
            .args(args)
            .env("PATH", path.join(":"))
            .env("WRAPPER_LOG", &log)
            .env_remove(TRIED_VARIABLE)
            .env_remove(DEPTH_VARIABLE)
            .output()
            .unwrap();
        let passes = std::fs::read_to_string(&log).unwrap().lines().count();
        (
            output.status.code().unwrap_or(-1),
            String::from_utf8_lossy(&output.stdout).into_owned(),
            String::from_utf8_lossy(&output.stderr).into_owned(),
            passes,
        )
    }

    #[test]
    fn another_tools_wrapper_runs_once_and_then_the_real_codex() {
        let root = tempfile::tempdir().unwrap();
        let ghostex = root.path().join("session-bin");
        let superset = root.path().join("superset-bin");
        let real = root.path().join("real-bin");
        write_script(&ghostex, &posix_wrapper());
        write_script(&superset, LOOPING_WRAPPER);
        write_script(&real, FAKE_CODEX);
        let (status, stdout, stderr, passes) = run_codex(
            root.path(),
            &[ghostex.clone(), superset.clone(), real.clone()],
            &["resume", "abc"],
        );
        assert_eq!(status, 0, "{stderr}");
        assert_eq!(passes, 1, "Superset's wrapper ran {passes} times");
        assert_eq!(stdout, "codex --enable hooks --no-daemon resume abc\n");
    }

    #[test]
    fn without_another_wrapper_the_real_codex_runs_without_the_daemon() {
        let root = tempfile::tempdir().unwrap();
        let ghostex = root.path().join("session-bin");
        let real = root.path().join("real-bin");
        write_script(&ghostex, &posix_wrapper());
        write_script(&real, FAKE_CODEX);
        let (status, stdout, _, _) = run_codex(root.path(), &[ghostex, real], &[]);
        assert_eq!(status, 0);
        assert_eq!(stdout, "codex --no-daemon\n");
    }

    #[test]
    fn codex_run_inside_codex_still_finds_the_real_codex() {
        let root = tempfile::tempdir().unwrap();
        let ghostex = root.path().join("session-bin");
        let superset = root.path().join("superset-bin");
        let real = root.path().join("real-bin");
        write_script(&ghostex, &posix_wrapper());
        write_script(&superset, LOOPING_WRAPPER);
        write_script(&real, FAKE_CODEX);
        let (status, stdout, stderr, passes) =
            run_codex(root.path(), &[ghostex, superset, real], &["nest"]);
        assert_eq!(status, 0, "{stderr}");
        assert_eq!(passes, 1);
        assert_eq!(
            stdout,
            "codex --enable hooks --no-daemon nest\ncodex exec inner\n"
        );
    }

    #[test]
    fn wrappers_with_no_real_codex_behind_them_stop() {
        let root = tempfile::tempdir().unwrap();
        let ghostex = root.path().join("session-bin");
        let superset = root.path().join("superset-bin");
        write_script(&ghostex, &posix_wrapper());
        write_script(&superset, LOOPING_WRAPPER);
        let (status, _, stderr, passes) =
            run_codex(root.path(), &[ghostex, superset], &["--version"]);
        assert_eq!(status, 126, "{stderr}");
        assert!(stderr.contains("keep running each other"), "{stderr}");
        assert_eq!(passes, MAX_DEPTH as usize);
    }
}
