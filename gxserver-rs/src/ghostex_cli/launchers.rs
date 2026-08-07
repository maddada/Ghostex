use std::path::{Component, Path, PathBuf};
use std::process::Command;

use serde_json::{json, Value};

use crate::ghostex_cli::args::{parse_args, Flags};
use crate::ghostex_cli::rpc::{call_gxserver_rpc, CliError, CliResult};

/*
CDXC:GhostexRustCli 2026-07-13:
Faithful port of the Node CLI's tool launchers: interactive shell resolution,
bare-`gx` TUI launch, zehn/ghostex-history/bd/gxserver binary discovery, and
the shared interactive process runner. Resolution order and error strings must
match scripts/ghostex-cli.mjs so installed bundles, remote Ubuntu packages,
and source checkouts keep resolving the exact same binaries.
*/

const CLI_POSIX_SHELL_NAMES: [&str; 7] = ["ash", "bash", "dash", "ksh", "mksh", "sh", "zsh"];
const CLI_LOGIN_COMMAND_SHELL_NAMES: [&str; 2] = ["bash", "zsh"];

/// A resolved external tool launch plan (command, prefix args, optional cwd,
/// and environment overrides layered over the inherited environment).
#[derive(Clone, Debug)]
pub struct Launch {
    pub command: String,
    pub args: Vec<String>,
    pub cwd: Option<PathBuf>,
    pub env: Vec<(String, String)>,
}

#[derive(Clone, Debug, PartialEq)]
pub struct ShellLaunch {
    pub command_flag: String,
    pub executable: String,
    pub login_flag: String,
}

/// JS shellQuote: always single-quote, escaping embedded single quotes.
pub fn shell_quote(value: &str) -> String {
    format!("'{}'", value.replace('\'', "'\\''"))
}

/// JS shellWord: bare word when safe, otherwise shellQuote.
pub fn shell_word(value: &str) -> String {
    let is_safe = !value.is_empty()
        && value.chars().all(|c| {
            c.is_ascii_alphanumeric()
                || matches!(c, '_' | '@' | '%' | '+' | '=' | ':' | ',' | '.' | '/' | '-')
        });
    if is_safe {
        value.to_string()
    } else {
        shell_quote(value)
    }
}

pub fn resolve_cli_interactive_shell_launch() -> ShellLaunch {
    resolve_cli_interactive_shell_launch_impl(
        cfg!(target_os = "macos"),
        std::env::var("SHELL").ok().as_deref(),
        &|candidate| is_executable_file_sync(Path::new(candidate)),
    )
}

fn resolve_cli_interactive_shell_launch_impl(
    is_darwin: bool,
    shell_env: Option<&str>,
    is_executable: &dyn Fn(&str) -> bool,
) -> ShellLaunch {
    /*
    Remote `ghostex attach` runs from the bundled CLI on the target machine, so
    it must not spawn macOS-only /bin/zsh on Ubuntu. Keep macOS pinned to zsh
    for compatibility, but resolve Linux attaches through an installed POSIX
    shell so SSH attaches reach zmx instead of failing with ENOENT.
    */
    if is_darwin {
        return ShellLaunch {
            command_flag: "-lc".to_string(),
            executable: "/bin/zsh".to_string(),
            login_flag: "-l".to_string(),
        };
    }
    let candidates = cli_interactive_shell_candidates(shell_env);
    let executable = candidates
        .iter()
        .find(|candidate| is_executable(candidate))
        .cloned()
        .or_else(|| candidates.first().cloned())
        .unwrap_or_else(|| "/bin/sh".to_string());
    ShellLaunch {
        command_flag: cli_shell_command_flag(&executable),
        login_flag: cli_shell_login_flag(&executable),
        executable,
    }
}

fn cli_interactive_shell_candidates(shell_env: Option<&str>) -> Vec<String> {
    let mut candidates: Vec<String> = Vec::new();
    let shell = shell_env.unwrap_or("").trim().to_string();
    if !shell.is_empty() && is_supported_cli_posix_shell(&shell) {
        candidates.push(shell);
    }
    candidates.extend(
        ["/bin/bash", "/usr/bin/bash", "/bin/sh", "/usr/bin/sh"]
            .into_iter()
            .map(str::to_string),
    );
    unique_strings(&candidates)
}

pub fn unique_strings(values: &[String]) -> Vec<String> {
    let mut seen = std::collections::HashSet::new();
    let mut unique = Vec::new();
    for value in values {
        let normalized = value.trim().to_string();
        if !normalized.is_empty() && seen.insert(normalized.clone()) {
            unique.push(normalized);
        }
    }
    unique
}

fn shell_basename_lowercase(shell_path: &str) -> String {
    Path::new(shell_path)
        .file_name()
        .map(|name| name.to_string_lossy().to_lowercase())
        .unwrap_or_default()
}

fn is_supported_cli_posix_shell(shell_path: &str) -> bool {
    CLI_POSIX_SHELL_NAMES.contains(&shell_basename_lowercase(shell_path).as_str())
}

fn cli_shell_command_flag(shell_path: &str) -> String {
    if CLI_LOGIN_COMMAND_SHELL_NAMES.contains(&shell_basename_lowercase(shell_path).as_str()) {
        "-lc".to_string()
    } else {
        "-c".to_string()
    }
}

fn cli_shell_login_flag(shell_path: &str) -> String {
    if CLI_LOGIN_COMMAND_SHELL_NAMES.contains(&shell_basename_lowercase(shell_path).as_str()) {
        "-l".to_string()
    } else {
        String::new()
    }
}

/// JS cliShellCommandString: `<shell> <flag> '<command>'` for embedding in a
/// remote/provider attach command line.
pub fn cli_shell_command_string(command: &str) -> String {
    let shell = resolve_cli_interactive_shell_launch();
    format!(
        "{} {} {}",
        shell_word(&shell.executable),
        shell.command_flag,
        shell_quote(command)
    )
}

/// JS sleep(ms).
pub fn sleep(ms: u64) {
    std::thread::sleep(std::time::Duration::from_millis(ms));
}

// ---------------------------------------------------------------------------
// Path helpers (fileExistsSync / isExecutableFileSync / uniquePaths / roots)
// ---------------------------------------------------------------------------

/// JS fileExistsSync: realpathSync succeeds (existing file or directory,
/// following symlinks; broken symlinks are treated as missing).
pub fn file_exists_sync(path: &Path) -> bool {
    std::fs::canonicalize(path).is_ok()
}

/// JS isExecutableFileSync: accessSync(path, X_OK).
pub fn is_executable_file_sync(path: &Path) -> bool {
    #[cfg(unix)]
    {
        use std::os::unix::ffi::OsStrExt;
        let Ok(c_path) = std::ffi::CString::new(path.as_os_str().as_bytes()) else {
            return false;
        };
        unsafe { libc::access(c_path.as_ptr(), libc::X_OK) == 0 }
    }
    #[cfg(not(unix))]
    {
        path.exists()
    }
}

/// JS path.resolve: absolute against cwd plus lexical `.`/`..` normalization.
pub fn js_path_resolve(path: &Path) -> PathBuf {
    let absolute = if path.is_absolute() {
        path.to_path_buf()
    } else {
        std::env::current_dir()
            .unwrap_or_else(|_| PathBuf::from("/"))
            .join(path)
    };
    let mut resolved = PathBuf::new();
    for component in absolute.components() {
        match component {
            Component::CurDir => {}
            Component::ParentDir => {
                resolved.pop();
            }
            other => resolved.push(other.as_os_str()),
        }
    }
    resolved
}

/// JS uniquePaths: skip falsy entries, normalize with path.resolve, dedupe.
pub fn unique_paths(paths: &[Option<PathBuf>]) -> Vec<PathBuf> {
    let mut seen = std::collections::HashSet::new();
    let mut unique = Vec::new();
    for candidate in paths.iter().flatten() {
        if candidate.as_os_str().is_empty() {
            continue;
        }
        let normalized = js_path_resolve(candidate);
        if seen.insert(normalized.clone()) {
            unique.push(normalized);
        }
    }
    unique
}

/// The installed CLI executable (realpath, mirroring Node module URL
/// resolution through the /usr/local/bin symlink into the app bundle).
pub fn current_cli_executable() -> PathBuf {
    std::env::current_exe()
        .map(|exe| std::fs::canonicalize(&exe).unwrap_or(exe))
        .unwrap_or_else(|_| PathBuf::from("ghostex"))
}

/// Directory holding this CLI (Node: path.dirname(fileURLToPath(import.meta.url))).
pub fn cli_dir() -> PathBuf {
    current_cli_executable()
        .parent()
        .map(Path::to_path_buf)
        .unwrap_or_else(|| PathBuf::from("."))
}

fn env_path(name: &str) -> Option<PathBuf> {
    let value = std::env::var(name).ok()?;
    if value.is_empty() {
        return None;
    }
    Some(PathBuf::from(value))
}

pub fn ghostex_bundled_web_resource_roots(cli_dir: &Path) -> Vec<PathBuf> {
    /*
    Installed app CLIs moved from Web/cli to CLI, but zmx/zehn/gxserver/TUI
    runtime assets still live under Web. Check both the new sibling Web folder
    and the legacy parent layout so old dev bundles and new release bundles
    resolve app-owned tools without PATH fallbacks.
    */
    unique_paths(&[
        Some(cli_dir.join("..").join("Web")),
        Some(cli_dir.join("..")),
    ])
}

pub fn find_ghostex_source_root(start_path: Option<&Path>) -> Option<PathBuf> {
    let mut current = js_path_resolve(
        start_path
            .map(Path::to_path_buf)
            .or_else(|| std::env::current_dir().ok())
            .unwrap_or_else(|| PathBuf::from("."))
            .as_path(),
    );
    loop {
        if file_exists_sync(&current.join("scripts").join("ghostex-cli.mjs"))
            && file_exists_sync(&current.join("tui2").join("Cargo.toml"))
        {
            return Some(current);
        }
        let Some(parent) = current.parent().map(Path::to_path_buf) else {
            return None;
        };
        if parent == current {
            return None;
        }
        current = parent;
    }
}

fn default_launch_roots(cli_dir: &Path) -> Vec<PathBuf> {
    let mut candidates: Vec<Option<PathBuf>> = ghostex_bundled_web_resource_roots(cli_dir)
        .into_iter()
        .map(Some)
        .collect();
    candidates.push(Some(js_path_resolve(&cli_dir.join(".."))));
    candidates.push(env_path("GHOSTEX_SOURCE_ROOT"));
    candidates.push(find_ghostex_source_root(None));
    unique_paths(&candidates)
}

// ---------------------------------------------------------------------------
// Interactive process runner
// ---------------------------------------------------------------------------

fn spawn_error(command: &str, error: &std::io::Error) -> CliError {
    // Node's spawn rejects with `spawn <command> ENOENT` style messages.
    let detail = match error.kind() {
        std::io::ErrorKind::NotFound => "ENOENT".to_string(),
        std::io::ErrorKind::PermissionDenied => "EACCES".to_string(),
        _ => error.to_string(),
    };
    CliError::Other(format!("spawn {command} {detail}"))
}

/// Spawn an interactive child sharing the terminal (stdio inherit), wait for
/// exit, return the exit code. Mirrors runInteractiveProcess in the Node CLI.
pub fn run_interactive_process(
    command: &str,
    args: &[String],
    cwd: Option<&std::path::Path>,
    env: &[(String, String)],
) -> CliResult<i32> {
    let mut child = Command::new(command);
    child.args(args);
    if let Some(cwd) = cwd {
        child.current_dir(cwd);
    }
    for (key, value) in env {
        child.env(key, value);
    }
    let status = child
        .status()
        .map_err(|error| spawn_error(command, &error))?;
    #[cfg(unix)]
    {
        use std::os::unix::process::ExitStatusExt;
        if let Some(signal) = status.signal() {
            // Node re-raises the child's terminating signal on itself.
            unsafe {
                libc::signal(signal, libc::SIG_DFL);
                libc::kill(std::process::id() as libc::pid_t, signal);
            }
            let code = 128 + signal;
            crate::ghostex_cli::set_exit_code(code);
            return Ok(code);
        }
    }
    let code = status.code().unwrap_or(0);
    crate::ghostex_cli::set_exit_code(code);
    Ok(code)
}

/// Run a command string through the user's interactive shell, mirrors
/// runInteractiveShellCommand (resolveCliInteractiveShellLaunch semantics).
pub fn run_interactive_shell_command(
    command: &str,
    cwd: Option<&std::path::Path>,
) -> CliResult<i32> {
    let shell = resolve_cli_interactive_shell_launch();
    run_interactive_process(
        &shell.executable,
        &[shell.command_flag.clone(), command.to_string()],
        cwd,
        &[],
    )
}

// ---------------------------------------------------------------------------
// TUI launch (bare `gx` / `gx 2`)
// ---------------------------------------------------------------------------

fn is_interactive_terminal() -> bool {
    use crossterm::tty::IsTty;
    std::io::stdin().is_tty() && std::io::stdout().is_tty()
}

pub fn ghostex_tui_command(args: &[String]) -> CliResult<()> {
    let parsed = parse_args(args);
    /*
    Bare `ghostex` / `gx` launches the full Ghostex terminal TUI; without an
    interactive terminal it falls back to the plain session picker rows.
    */
    if !is_interactive_terminal() {
        return interactive_session_picker_command(args);
    }
    let tui = resolve_ghostex_tui_launch(&parsed.flags)?;
    /*
    The bare `gx` launcher must pass TUI environment through the spawn env so
    the app build keeps the callback command the TUI uses to list and attach
    Ghostex sessions.
    */
    let mut env = tui.env.clone();
    env.push((
        "GHOSTEX_TUI_CLI_COMMAND".to_string(),
        shell_quote(&current_cli_executable().to_string_lossy()),
    ));
    run_interactive_process(&tui.command, &tui.args, tui.cwd.as_deref(), &env)?;
    Ok(())
}

pub fn ghostex_tui2_command(args: &[String]) -> CliResult<()> {
    /*
    Keep `gx 2` as a compatibility alias after promoting TUI2 to bare `gx`.
    It must share the same resolver as the public command so there is only one
    launch contract to package and support.
    */
    ghostex_tui_command(args)
}

pub fn resolve_ghostex_tui_launch(flags: &Flags) -> CliResult<Launch> {
    let explicit_bin = flags
        .text("tuiBin")
        .or_else(|| std::env::var("GHOSTEX_TUI_BIN").ok())
        .unwrap_or_default()
        .trim()
        .to_string();
    let tui_args = || vec!["--ghostex".to_string(), "--no-session".to_string()];
    if !explicit_bin.is_empty() {
        return Ok(Launch {
            command: explicit_bin,
            args: tui_args(),
            cwd: None,
            env: Vec::new(),
        });
    }
    let cli_dir = cli_dir();
    for root in default_launch_roots(&cli_dir) {
        if let Some(launch) = resolve_ghostex_tui_launch_from_root(&root) {
            return Ok(launch);
        }
    }
    Err(CliError::Other(
        "Ghostex TUI binary was not found. Build the TUI with `cargo build --bin ghostex-tui --manifest-path tui2/Cargo.toml`, pass `--tui-bin <path>`, or set GHOSTEX_TUI_BIN.".to_string(),
    ))
}

pub fn resolve_ghostex_tui2_launch(flags: &Flags) -> CliResult<Launch> {
    resolve_ghostex_tui_launch(flags)
}

pub fn resolve_ghostex_tui_launch_from_root(root: &Path) -> Option<Launch> {
    let tui_args = vec!["--ghostex".to_string(), "--no-session".to_string()];
    let simple = |command: &Path| Launch {
        command: command.to_string_lossy().to_string(),
        args: tui_args.clone(),
        cwd: None,
        env: Vec::new(),
    };
    let bundled_bin = root.join("bin").join("ghostex-tui");
    if file_exists_sync(&bundled_bin) {
        return Some(simple(&bundled_bin));
    }
    let debug_bin = root
        .join("tui2")
        .join("target")
        .join("debug")
        .join("ghostex-tui");
    let release_bin = root
        .join("tui2")
        .join("target")
        .join("release")
        .join("ghostex-tui");
    if file_exists_sync(&release_bin) {
        return Some(simple(&release_bin));
    }
    if file_exists_sync(&debug_bin) {
        return Some(simple(&debug_bin));
    }
    let manifest_path = root.join("tui2").join("Cargo.toml");
    if !file_exists_sync(&manifest_path) {
        return None;
    }
    Some(Launch {
        command: "cargo".to_string(),
        args: vec![
            "run".to_string(),
            "--quiet".to_string(),
            "--bin".to_string(),
            "ghostex-tui".to_string(),
            "--manifest-path".to_string(),
            manifest_path.to_string_lossy().to_string(),
            "--".to_string(),
            "--ghostex".to_string(),
            "--no-session".to_string(),
        ],
        cwd: None,
        env: ghostex_tui_cargo_env(),
    })
}

pub fn resolve_ghostex_tui2_launch_from_root(root: &Path) -> Option<Launch> {
    /*
    `ghostex-tui2` is no longer a separate packaged app. Preserve the exported
    resolver as a legacy alias while returning the canonical `ghostex-tui`
    launch plan.
    */
    resolve_ghostex_tui_launch_from_root(root)
}

pub fn ghostex_tui_cargo_env() -> Vec<(String, String)> {
    /*
    On macOS 26.4+, unpatched Zig 0.15.2 cannot link libc from Xcode 26 SDKs;
    prefer Homebrew's patched `zig@0.15` keg when it exists so first-run
    fallback builds can produce the real Ghostty terminal backend.
    */
    let patched_homebrew_zig = "/opt/homebrew/opt/zig@0.15/bin/zig";
    if file_exists_sync(Path::new(patched_homebrew_zig)) {
        vec![("ZIG".to_string(), patched_homebrew_zig.to_string())]
    } else {
        Vec::new()
    }
}

// ---------------------------------------------------------------------------
// Non-interactive picker fallback (private copy of the row printer used by
// the bare-`gx` launch when stdin/stdout are not TTYs; the interactive picker
// itself is owned by the picker module).
// ---------------------------------------------------------------------------

fn interactive_session_picker_command(args: &[String]) -> CliResult<()> {
    // Delegate to the attach module's full picker command (TTY detection,
    // alias-cache refresh, picker, attach) so there is one implementation.
    crate::ghostex_cli::attach::interactive_session_picker_command(args)
}

// ---------------------------------------------------------------------------
// zehn (`gx find`) and ghostex-history (`gx h`)
// ---------------------------------------------------------------------------

pub fn zehn_search_command(args: &[String]) -> CliResult<()> {
    let launch = resolve_zehn_launch()?;
    let zehn_args = resolve_zehn_search_args(args);
    let mut full_args = launch.args.clone();
    full_args.extend(zehn_args);
    /*
    CDXC:AgentHistoryFocus 2026-08-07-09:18:
    `ghostex find` must give bundled Zehn the exact Ghostex CLI executable that launched it. Zehn can then request agent-session focus through the authenticated gxserver control path without PATH discovery or coupling itself to Ghostex storage/protocol internals.
    */
    let mut env = launch.env.clone();
    env.retain(|(key, _)| key != "GHOSTEX_CLI_EXECUTABLE");
    env.push((
        "GHOSTEX_CLI_EXECUTABLE".to_string(),
        current_cli_executable().to_string_lossy().to_string(),
    ));
    run_interactive_process(&launch.command, &full_args, launch.cwd.as_deref(), &env)?;
    Ok(())
}

pub fn history_command(args: &[String]) -> CliResult<()> {
    /*
    Forward all args to ghostex-history so --agent, --list, --home, and future
    transcript-viewer flags stay owned by the Rust TUI. `gx h` resumes with the
    same Accept All policy that `gx find` passes to Zehn.
    */
    let launch = resolve_ghostex_history_launch()?;
    let history_args = resolve_ghostex_history_args(args);
    let mut full_args = launch.args.clone();
    full_args.extend(history_args);
    run_interactive_process(
        &launch.command,
        &full_args,
        launch.cwd.as_deref(),
        &launch.env,
    )?;
    Ok(())
}

pub fn resolve_ghostex_history_launch() -> CliResult<Launch> {
    let explicit_bin = std::env::var("GHOSTEX_HISTORY_BIN")
        .unwrap_or_default()
        .trim()
        .to_string();
    if !explicit_bin.is_empty() {
        return Ok(Launch {
            command: explicit_bin,
            args: Vec::new(),
            cwd: None,
            env: Vec::new(),
        });
    }
    let cli_dir = cli_dir();
    for root in default_launch_roots(&cli_dir) {
        if let Some(launch) = resolve_ghostex_history_launch_from_root(&root) {
            return Ok(launch);
        }
    }
    Err(CliError::Other(
        "ghostex-history was not found. Build or reinstall Ghostex so Web/bin/ghostex-history is staged, run from a source checkout with ghostex-history/Cargo.toml, or set GHOSTEX_HISTORY_BIN.".to_string(),
    ))
}

pub fn resolve_ghostex_history_launch_from_root(root: &Path) -> Option<Launch> {
    let plain = |command: &Path| Launch {
        command: command.to_string_lossy().to_string(),
        args: Vec::new(),
        cwd: None,
        env: Vec::new(),
    };
    let bundled_bin = root.join("bin").join("ghostex-history");
    if file_exists_sync(&bundled_bin) {
        return Some(plain(&bundled_bin));
    }
    let manifest_path = root.join("ghostex-history").join("Cargo.toml");
    if file_exists_sync(&manifest_path) {
        /*
        Local `gx h` should reflect source edits immediately. Prefer Cargo over
        target/debug when a source manifest is present so a stale development
        binary cannot mask transcript UI fixes during verification.
        */
        return Some(Launch {
            command: "cargo".to_string(),
            args: vec![
                "run".to_string(),
                "--quiet".to_string(),
                "--manifest-path".to_string(),
                manifest_path.to_string_lossy().to_string(),
                "--".to_string(),
            ],
            cwd: None,
            env: Vec::new(),
        });
    }
    let debug_bin = root
        .join("ghostex-history")
        .join("target")
        .join("debug")
        .join("ghostex-history");
    let release_bin = root
        .join("ghostex-history")
        .join("target")
        .join("release")
        .join("ghostex-history");
    if file_exists_sync(&release_bin) {
        return Some(plain(&release_bin));
    }
    if file_exists_sync(&debug_bin) {
        return Some(plain(&debug_bin));
    }
    None
}

fn read_agent_accept_all_enabled() -> bool {
    let result = call_gxserver_rpc("/api/readAgentSettings", &json!({}), &Flags::default()).ok();
    result
        .as_ref()
        .and_then(|value| value.get("settings"))
        .and_then(|settings| settings.get("agentAcceptAllEnabled"))
        == Some(&Value::Bool(true))
}

pub fn resolve_zehn_search_args(args: &[String]) -> Vec<String> {
    if has_zehn_accept_all_override(args) {
        return args.to_vec();
    }
    apply_zehn_accept_all_args(args, read_agent_accept_all_enabled())
}

pub fn resolve_ghostex_history_args(args: &[String]) -> Vec<String> {
    if has_zehn_accept_all_override(args) {
        return args.to_vec();
    }
    apply_zehn_accept_all_args(args, read_agent_accept_all_enabled())
}

pub fn apply_zehn_accept_all_args(args: &[String], accept_all_enabled: bool) -> Vec<String> {
    if has_zehn_accept_all_override(args) || !accept_all_enabled {
        return args.to_vec();
    }
    let mut with_accept_all = vec!["--accept-all".to_string()];
    with_accept_all.extend(args.to_vec());
    with_accept_all
}

pub fn has_zehn_accept_all_override(args: &[String]) -> bool {
    args.iter()
        .any(|arg| arg == "--accept-all" || arg == "--no-accept-all")
}

pub fn resolve_zehn_launch() -> CliResult<Launch> {
    let explicit_bin = std::env::var("GHOSTEX_ZEHN_BIN")
        .unwrap_or_default()
        .trim()
        .to_string();
    if !explicit_bin.is_empty() {
        return Ok(Launch {
            command: explicit_bin,
            args: Vec::new(),
            cwd: None,
            env: Vec::new(),
        });
    }

    let cli_dir = cli_dir();
    for bundled_root in ghostex_bundled_web_resource_roots(&cli_dir) {
        let bundled_bin = js_path_resolve(&bundled_root.join("bin").join("zehn"));
        if file_exists_sync(&bundled_bin) {
            return Ok(Launch {
                command: bundled_bin.to_string_lossy().to_string(),
                args: Vec::new(),
                cwd: None,
                env: Vec::new(),
            });
        }
    }

    let repo_root = js_path_resolve(&cli_dir.join(".."));
    let roots = unique_paths(&[
        Some(repo_root),
        env_path("GHOSTEX_SOURCE_ROOT"),
        find_ghostex_source_root(None),
    ]);
    for root in roots {
        if let Some(launch) = resolve_zehn_launch_from_root(&root) {
            return Ok(launch);
        }
    }

    Err(CliError::Other(
        "Bundled zehn was not found. Initialize the submodule with `git submodule update --init zehn`, build it with Zig 0.16+, or set GHOSTEX_ZEHN_BIN to a reviewed zehn binary.".to_string(),
    ))
}

pub fn resolve_zehn_launch_from_root(root: &Path) -> Option<Launch> {
    let bin = root.join("zehn").join("zig-out").join("bin").join("zehn");
    if file_exists_sync(&bin) {
        return Some(Launch {
            command: bin.to_string_lossy().to_string(),
            args: Vec::new(),
            cwd: None,
            env: Vec::new(),
        });
    }
    let manifest_path = root.join("zehn").join("build.zig");
    if !file_exists_sync(&manifest_path) {
        return None;
    }
    let zig_bin = std::env::var("GHOSTEX_ZEHN_ZIG")
        .or_else(|_| std::env::var("ZEHN_ZIG"))
        .unwrap_or_else(|_| "zig".to_string())
        .trim()
        .to_string();
    Some(Launch {
        command: if zig_bin.is_empty() {
            "zig".to_string()
        } else {
            zig_bin
        },
        args: vec!["build".to_string(), "run".to_string(), "--".to_string()],
        cwd: Some(root.join("zehn")),
        env: Vec::new(),
    })
}

// ---------------------------------------------------------------------------
// Bundled Beads (`gx bd`)
// ---------------------------------------------------------------------------

pub fn beads_command(args: &[String]) -> CliResult<()> {
    let launch = resolve_bundled_beads_launch()?;
    let mut full_args = launch.args.clone();
    full_args.extend(args.to_vec());
    let cwd = std::env::current_dir().ok();
    run_interactive_process(&launch.command, &full_args, cwd.as_deref(), &launch.env)?;
    Ok(())
}

pub fn resolve_bundled_beads_launch() -> CliResult<Launch> {
    /*
    `gx bd` is the supported agent/user shell boundary for Beads operations.
    Resolve only Ghostex-bundled or source-staged Beads binaries so
    Project/Kanban, agent prompts, and manual commands cannot split state
    across different shell-installed `bd` versions.
    */
    let cli_dir = cli_dir();
    for root in default_launch_roots(&cli_dir) {
        if let Some(launch) = resolve_bundled_beads_launch_from_root(&root) {
            return Ok(launch);
        }
    }
    Err(CliError::Other(
        "Bundled bd was not found. Rebuild or reinstall Ghostex so Web/bin/bd is staged; shell-installed bd is intentionally ignored.".to_string(),
    ))
}

pub fn resolve_bundled_beads_launch_from_root(root: &Path) -> Option<Launch> {
    for candidate in [
        root.join("bin").join("bd"),
        root.join("native")
            .join("macos")
            .join("ghostexHost")
            .join("Web")
            .join("bin")
            .join("bd"),
    ] {
        if file_exists_sync(&candidate) {
            return Some(Launch {
                command: candidate.to_string_lossy().to_string(),
                args: Vec::new(),
                cwd: None,
                env: Vec::new(),
            });
        }
    }
    None
}

// ---------------------------------------------------------------------------
// gxserver CLI passthrough (`gx server ...`)
// ---------------------------------------------------------------------------

pub fn run_gxserver_cli_command(args: &[String]) -> CliResult<()> {
    let launch = resolve_gxserver_cli_launch()?;
    let mut full_args = launch.args.clone();
    full_args.extend(args.to_vec());
    run_interactive_process(
        &launch.command,
        &full_args,
        launch.cwd.as_deref(),
        &launch.env,
    )?;
    Ok(())
}

pub fn resolve_gxserver_cli_launch() -> CliResult<Launch> {
    let explicit_cli = std::env::var("GHOSTEX_GXSERVER_CLI")
        .or_else(|_| std::env::var("GHOSTEX_GXSERVER_BIN"))
        .unwrap_or_default()
        .trim()
        .to_string();
    if !explicit_cli.is_empty() {
        return resolve_gxserver_cli_launch_for_path(Path::new(&explicit_cli), true);
    }

    let cli_dir = cli_dir();
    /*
    The Rust CLI ships as a `ghostex` binary next to the real `gxserver`
    binary (where the Node CLI resolved its sibling node entrypoint), so the
    sibling native binary wins before the shared root fallbacks.
    */
    let sibling = cli_dir.join("gxserver");
    if file_exists_sync(&sibling) {
        return resolve_gxserver_cli_launch_for_path(&sibling, false);
    }

    for root in default_launch_roots(&cli_dir) {
        if let Some(launch) = resolve_gxserver_cli_launch_from_root(&root)? {
            return Ok(launch);
        }
    }

    Err(CliError::Other(
        "Bundled gxserver binary is missing. Rebuild or reinstall Ghostex so Web/gxserver is present, or set GHOSTEX_GXSERVER_CLI/BIN for an explicit source/reference daemon.".to_string(),
    ))
}

pub fn resolve_gxserver_cli_launch_from_root(root: &Path) -> CliResult<Option<Launch>> {
    /*
    `gx server ...` must prefer the packaged gxserver-rs binary. Remote Ubuntu
    packages are standalone gxserver roots rather than macOS Web roots, so
    root/bin/gxserver stays resolvable without a source checkout or PATH
    fallback. JavaScript CLI discovery comes only after the native binary.
    */
    for candidate in [
        root.join("gxserver").join("bin").join("gxserver"),
        root.join("bin").join("gxserver"),
        root.join("native")
            .join("macos")
            .join("ghostexHost")
            .join("Web")
            .join("gxserver")
            .join("bin")
            .join("gxserver"),
        root.join("gxserver")
            .join("dist")
            .join("src")
            .join("cli.js"),
        root.join("native")
            .join("macos")
            .join("ghostexHost")
            .join("Web")
            .join("gxserver")
            .join("dist")
            .join("src")
            .join("cli.js"),
    ] {
        if file_exists_sync(&candidate) {
            return resolve_gxserver_cli_launch_for_path(&candidate, false).map(Some);
        }
    }
    Ok(None)
}

pub fn resolve_gxserver_cli_launch_for_path(cli_path: &Path, explicit: bool) -> CliResult<Launch> {
    let resolved_path = resolve_gxserver_cli_path(cli_path, explicit);
    if !file_exists_sync(&resolved_path) {
        return Err(CliError::Other(format!(
            "gxserver CLI path does not exist: {}",
            resolved_path.display()
        )));
    }
    if resolved_path.extension().and_then(|ext| ext.to_str()) == Some("js") {
        // The Node CLI ran .js entrypoints with its own process.execPath; the
        // Rust CLI is not a Node runtime, so JavaScript daemons run via node.
        return Ok(Launch {
            command: "node".to_string(),
            args: vec![resolved_path.to_string_lossy().to_string()],
            cwd: None,
            env: Vec::new(),
        });
    }
    if !is_executable_file_sync(&resolved_path) {
        return Err(CliError::Other(format!(
            "gxserver binary is not executable: {}",
            resolved_path.display()
        )));
    }
    Ok(Launch {
        command: resolved_path.to_string_lossy().to_string(),
        args: Vec::new(),
        cwd: None,
        env: Vec::new(),
    })
}

pub fn resolve_gxserver_cli_path(cli_path: &Path, explicit: bool) -> PathBuf {
    let normalized_path = cli_path.to_string_lossy().trim().to_string();
    if normalized_path.is_empty() {
        return js_path_resolve(Path::new(""));
    }
    let normalized = PathBuf::from(&normalized_path);
    if normalized.is_absolute() || !explicit {
        return js_path_resolve(&normalized);
    }
    /*
    Explicit GHOSTEX_GXSERVER_CLI/BIN selections use the current shell and
    source-root hints so developers can point `gx server ...` at either
    gxserver-rs/target/debug/gxserver or a TypeScript CLI without falling back
    when the selected path is wrong.
    */
    let source_root = find_ghostex_source_root(None);
    let candidates = unique_paths(&[
        std::env::current_dir()
            .ok()
            .map(|cwd| cwd.join(&normalized)),
        env_path("GHOSTEX_SOURCE_ROOT").map(|root| root.join(&normalized)),
        env_path("ghostex_REPO_ROOT").map(|root| root.join(&normalized)),
        source_root.map(|root| root.join(&normalized)),
    ]);
    candidates
        .into_iter()
        .find(|candidate| file_exists_sync(candidate))
        .unwrap_or_else(|| js_path_resolve(&normalized))
}

// ---------------------------------------------------------------------------
// Bare-path detection for `ghostex <path...>`
// ---------------------------------------------------------------------------

/// Private copy of parseVsCodePathPosition (owned by the editors module):
/// strips up to two trailing `:<line>`/`:<column>` positive-integer segments.
fn parse_vscode_path_position(value: &str) -> (String, Option<u64>, Option<u64>) {
    fn split_position_tail(text: &str) -> Option<(&str, u64)> {
        let index = text.rfind(':')?;
        let (head, tail) = (&text[..index], &text[index + 1..]);
        if head.is_empty() || tail.is_empty() {
            return None;
        }
        if !tail.starts_with(|c: char| ('1'..='9').contains(&c))
            || !tail.chars().all(|c| c.is_ascii_digit())
        {
            return None;
        }
        Some((head, tail.parse().ok()?))
    }

    let mut path = value;
    let mut numbers: Vec<u64> = Vec::new();
    for _ in 0..2 {
        match split_position_tail(path) {
            Some((head, number)) => {
                path = head;
                numbers.push(number);
            }
            None => break,
        }
    }
    numbers.reverse();
    match numbers.as_slice() {
        [line, column] => (path.to_string(), Some(*line), Some(*column)),
        [line] => (path.to_string(), Some(*line), None),
        _ => (path.to_string(), None, None),
    }
}

pub fn is_existing_bare_path_argument(value: &str) -> bool {
    if value.is_empty() || value.starts_with('-') {
        return false;
    }
    let (path, _line, _column) = parse_vscode_path_position(value);
    js_path_resolve(Path::new(&path)).exists()
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicU32, Ordering};

    static TEMP_COUNTER: AtomicU32 = AtomicU32::new(0);

    fn temp_root(name: &str) -> PathBuf {
        let root = std::env::temp_dir().join(format!(
            "gx-cli-launchers-{}-{}-{}",
            std::process::id(),
            name,
            TEMP_COUNTER.fetch_add(1, Ordering::SeqCst)
        ));
        std::fs::create_dir_all(&root).expect("create temp root");
        root
    }

    fn touch(path: &Path) {
        std::fs::create_dir_all(path.parent().expect("parent")).expect("mkdir");
        std::fs::write(path, b"").expect("write");
    }

    #[cfg(unix)]
    fn make_executable(path: &Path) {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o755)).expect("chmod");
    }

    #[test]
    fn shell_quote_always_wraps_and_escapes() {
        assert_eq!(shell_quote("simple"), "'simple'");
        assert_eq!(shell_quote("it's"), "'it'\\''s'");
        assert_eq!(shell_quote(""), "''");
    }

    #[test]
    fn shell_word_matches_js_charset() {
        assert_eq!(shell_word("/bin/zsh"), "/bin/zsh");
        assert_eq!(shell_word("a_b@%+=:,./-9"), "a_b@%+=:,./-9");
        assert_eq!(shell_word("has space"), "'has space'");
        assert_eq!(shell_word(""), "''");
        assert_eq!(shell_word("semi;colon"), "'semi;colon'");
    }

    #[test]
    fn shell_launch_is_pinned_to_zsh_on_darwin() {
        let launch = resolve_cli_interactive_shell_launch_impl(true, Some("/bin/fish"), &|_| true);
        assert_eq!(
            launch,
            ShellLaunch {
                command_flag: "-lc".to_string(),
                executable: "/bin/zsh".to_string(),
                login_flag: "-l".to_string(),
            }
        );
    }

    #[test]
    fn shell_launch_resolves_posix_candidates_off_darwin() {
        // Supported $SHELL wins when executable.
        let launch =
            resolve_cli_interactive_shell_launch_impl(false, Some("/usr/bin/zsh"), &|_| true);
        assert_eq!(launch.executable, "/usr/bin/zsh");
        assert_eq!(launch.command_flag, "-lc");
        assert_eq!(launch.login_flag, "-l");
        // Unsupported $SHELL is skipped entirely.
        let launch =
            resolve_cli_interactive_shell_launch_impl(false, Some("/usr/bin/fish"), &|_| true);
        assert_eq!(launch.executable, "/bin/bash");
        // Non-login shells use plain -c and no login flag.
        let launch = resolve_cli_interactive_shell_launch_impl(false, None, &|candidate| {
            candidate == "/bin/sh"
        });
        assert_eq!(launch.executable, "/bin/sh");
        assert_eq!(launch.command_flag, "-c");
        assert_eq!(launch.login_flag, "");
        // Nothing executable falls back to the first candidate.
        let launch = resolve_cli_interactive_shell_launch_impl(false, None, &|_| false);
        assert_eq!(launch.executable, "/bin/bash");
    }

    #[test]
    fn unique_strings_trims_and_dedupes() {
        let values: Vec<String> = ["/bin/bash", " /bin/bash ", "", "  ", "/bin/sh"]
            .iter()
            .map(|s| s.to_string())
            .collect();
        assert_eq!(unique_strings(&values), vec!["/bin/bash", "/bin/sh"]);
    }

    #[test]
    fn unique_paths_normalizes_and_dedupes() {
        let root = temp_root("unique-paths");
        let with_dots = root.join("a").join("..").join("b");
        let plain = root.join("b");
        let unique = unique_paths(&[Some(with_dots), None, Some(plain.clone()), Some(plain)]);
        assert_eq!(unique.len(), 1);
        assert!(unique[0].ends_with("b"));
    }

    #[test]
    fn accept_all_args_only_prepend_without_override() {
        let args: Vec<String> = vec!["--list".to_string()];
        assert_eq!(
            apply_zehn_accept_all_args(&args, true),
            vec!["--accept-all", "--list"]
        );
        assert_eq!(apply_zehn_accept_all_args(&args, false), vec!["--list"]);
        let overridden: Vec<String> = vec!["--no-accept-all".to_string()];
        assert_eq!(
            apply_zehn_accept_all_args(&overridden, true),
            vec!["--no-accept-all"]
        );
        assert!(has_zehn_accept_all_override(&overridden));
        assert!(!has_zehn_accept_all_override(&args));
    }

    #[test]
    fn parse_vscode_path_position_matches_js_regex() {
        assert_eq!(
            parse_vscode_path_position("src/main.rs:12:34"),
            ("src/main.rs".to_string(), Some(12), Some(34))
        );
        assert_eq!(
            parse_vscode_path_position("src/main.rs:12"),
            ("src/main.rs".to_string(), Some(12), None)
        );
        assert_eq!(
            parse_vscode_path_position("src/main.rs"),
            ("src/main.rs".to_string(), None, None)
        );
        // Zero-leading segments are not positions.
        assert_eq!(
            parse_vscode_path_position("file:012"),
            ("file:012".to_string(), None, None)
        );
        // Only the trailing two numeric groups strip.
        assert_eq!(
            parse_vscode_path_position("a:1:2:3"),
            ("a:1".to_string(), Some(2), Some(3))
        );
        // The path itself must stay non-empty.
        assert_eq!(
            parse_vscode_path_position(":12"),
            (":12".to_string(), None, None)
        );
    }

    #[test]
    fn bare_path_argument_requires_existing_path() {
        let root = temp_root("bare-path");
        let file = root.join("notes.txt");
        touch(&file);
        let arg = format!("{}:3:4", file.display());
        assert!(is_existing_bare_path_argument(&arg));
        assert!(is_existing_bare_path_argument(&file.display().to_string()));
        assert!(!is_existing_bare_path_argument(&format!(
            "{}-missing",
            file.display()
        )));
        assert!(!is_existing_bare_path_argument("--flag"));
        assert!(!is_existing_bare_path_argument(""));
    }

    #[test]
    fn history_launch_prefers_bundled_then_manifest_then_targets() {
        let root = temp_root("history");
        assert!(resolve_ghostex_history_launch_from_root(&root).is_none());

        let release = root
            .join("ghostex-history")
            .join("target")
            .join("release")
            .join("ghostex-history");
        touch(&release);
        let launch = resolve_ghostex_history_launch_from_root(&root).expect("release launch");
        assert_eq!(launch.command, release.to_string_lossy());
        assert!(launch.args.is_empty());

        let manifest = root.join("ghostex-history").join("Cargo.toml");
        touch(&manifest);
        let launch = resolve_ghostex_history_launch_from_root(&root).expect("cargo launch");
        assert_eq!(launch.command, "cargo");
        assert_eq!(
            launch.args,
            vec![
                "run",
                "--quiet",
                "--manifest-path",
                manifest.to_string_lossy().as_ref(),
                "--"
            ]
        );

        let bundled = root.join("bin").join("ghostex-history");
        touch(&bundled);
        let launch = resolve_ghostex_history_launch_from_root(&root).expect("bundled launch");
        assert_eq!(launch.command, bundled.to_string_lossy());
    }

    #[test]
    fn zehn_launch_prefers_built_binary_then_zig_build() {
        let root = temp_root("zehn");
        assert!(resolve_zehn_launch_from_root(&root).is_none());

        touch(&root.join("zehn").join("build.zig"));
        let launch = resolve_zehn_launch_from_root(&root).expect("zig launch");
        assert_eq!(launch.args, vec!["build", "run", "--"]);
        assert_eq!(launch.cwd, Some(root.join("zehn")));

        let bin = root.join("zehn").join("zig-out").join("bin").join("zehn");
        touch(&bin);
        let launch = resolve_zehn_launch_from_root(&root).expect("bin launch");
        assert_eq!(launch.command, bin.to_string_lossy());
        assert!(launch.args.is_empty());
        assert_eq!(launch.cwd, None);
    }

    #[test]
    fn tui_launch_prefers_bundled_then_release_then_debug_then_cargo() {
        let root = temp_root("tui");
        assert!(resolve_ghostex_tui_launch_from_root(&root).is_none());

        touch(&root.join("tui2").join("Cargo.toml"));
        let launch = resolve_ghostex_tui_launch_from_root(&root).expect("cargo launch");
        assert_eq!(launch.command, "cargo");
        assert_eq!(launch.args[0], "run");
        assert_eq!(launch.args.last().map(String::as_str), Some("--no-session"));

        let debug = root
            .join("tui2")
            .join("target")
            .join("debug")
            .join("ghostex-tui");
        touch(&debug);
        let launch = resolve_ghostex_tui_launch_from_root(&root).expect("debug launch");
        assert_eq!(launch.command, debug.to_string_lossy());
        assert_eq!(launch.args, vec!["--ghostex", "--no-session"]);

        let release = root
            .join("tui2")
            .join("target")
            .join("release")
            .join("ghostex-tui");
        touch(&release);
        let launch = resolve_ghostex_tui_launch_from_root(&root).expect("release launch");
        assert_eq!(launch.command, release.to_string_lossy());

        let bundled = root.join("bin").join("ghostex-tui");
        touch(&bundled);
        let launch = resolve_ghostex_tui_launch_from_root(&root).expect("bundled launch");
        assert_eq!(launch.command, bundled.to_string_lossy());
        // Legacy alias resolves the identical plan.
        let alias = resolve_ghostex_tui2_launch_from_root(&root).expect("alias launch");
        assert_eq!(alias.command, launch.command);
    }

    #[test]
    fn beads_launch_checks_bundled_and_source_staged_paths() {
        let root = temp_root("beads");
        assert!(resolve_bundled_beads_launch_from_root(&root).is_none());
        let staged = root
            .join("native")
            .join("macos")
            .join("ghostexHost")
            .join("Web")
            .join("bin")
            .join("bd");
        touch(&staged);
        let launch = resolve_bundled_beads_launch_from_root(&root).expect("staged launch");
        assert_eq!(launch.command, staged.to_string_lossy());
        let bundled = root.join("bin").join("bd");
        touch(&bundled);
        let launch = resolve_bundled_beads_launch_from_root(&root).expect("bundled launch");
        assert_eq!(launch.command, bundled.to_string_lossy());
    }

    #[test]
    fn gxserver_launch_prefers_native_binary_and_supports_js_fallback() {
        let root = temp_root("gxserver");
        assert!(resolve_gxserver_cli_launch_from_root(&root)
            .expect("no launch")
            .is_none());

        let js_cli = root
            .join("gxserver")
            .join("dist")
            .join("src")
            .join("cli.js");
        touch(&js_cli);
        let launch = resolve_gxserver_cli_launch_from_root(&root)
            .expect("js launch")
            .expect("some");
        assert_eq!(launch.command, "node");
        assert_eq!(launch.args, vec![js_cli.to_string_lossy().to_string()]);

        let bin = root.join("bin").join("gxserver");
        touch(&bin);
        #[cfg(unix)]
        {
            // A staged but non-executable binary is a hard error, not a fallback.
            let error = resolve_gxserver_cli_launch_from_root(&root).expect_err("not executable");
            assert!(error
                .to_string()
                .starts_with("gxserver binary is not executable: "));
            make_executable(&bin);
        }
        let launch = resolve_gxserver_cli_launch_from_root(&root)
            .expect("bin launch")
            .expect("some");
        assert_eq!(launch.command, bin.to_string_lossy());
        assert!(launch.args.is_empty());

        let nested = root.join("gxserver").join("bin").join("gxserver");
        touch(&nested);
        #[cfg(unix)]
        make_executable(&nested);
        let launch = resolve_gxserver_cli_launch_from_root(&root)
            .expect("nested launch")
            .expect("some");
        assert_eq!(launch.command, nested.to_string_lossy());
    }

    #[test]
    fn gxserver_launch_for_missing_path_matches_js_error() {
        let root = temp_root("gxserver-missing");
        let missing = root.join("gxserver-nope");
        let error = resolve_gxserver_cli_launch_for_path(&missing, false).expect_err("missing");
        assert_eq!(
            error.to_string(),
            format!("gxserver CLI path does not exist: {}", missing.display())
        );
    }

    #[test]
    fn find_ghostex_source_root_walks_up_to_marker_pair() {
        let root = temp_root("source-root");
        touch(&root.join("scripts").join("ghostex-cli.mjs"));
        touch(&root.join("tui2").join("Cargo.toml"));
        let nested = root.join("a").join("b");
        std::fs::create_dir_all(&nested).expect("mkdir");
        let canonical_root = std::fs::canonicalize(&root).expect("canonical root");
        assert_eq!(
            find_ghostex_source_root(Some(&std::fs::canonicalize(&nested).expect("nested"))),
            Some(canonical_root)
        );
        let unrelated = temp_root("source-root-miss");
        assert_eq!(
            find_ghostex_source_root(Some(&std::fs::canonicalize(&unrelated).expect("unrelated"))),
            None
        );
    }

    #[test]
    fn bundled_web_resource_roots_prefer_sibling_web_folder() {
        let cli_dir = temp_root("web-roots")
            .join("Contents")
            .join("Resources")
            .join("CLI");
        std::fs::create_dir_all(&cli_dir).expect("mkdir");
        let roots = ghostex_bundled_web_resource_roots(&cli_dir);
        assert_eq!(roots.len(), 2);
        assert!(roots[0].ends_with("Resources/Web") || roots[0].ends_with("Resources\\Web"));
        assert!(roots[1].ends_with("Resources"));
    }

    #[test]
    fn run_interactive_process_reports_exit_code_and_spawn_errors() {
        #[cfg(unix)]
        {
            let code = run_interactive_process(
                "/bin/sh",
                &["-c".to_string(), "exit 7".to_string()],
                None,
                &[],
            )
            .expect("run");
            assert_eq!(code, 7);
            crate::ghostex_cli::set_exit_code(0);
        }
        let error = run_interactive_process("definitely-not-a-real-binary-gx", &[], None, &[])
            .expect_err("spawn failure");
        assert_eq!(
            error.to_string(),
            "spawn definitely-not-a-real-binary-gx ENOENT"
        );
    }
}
