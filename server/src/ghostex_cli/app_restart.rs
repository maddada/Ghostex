//! `ghostex app-update restart`: quit the Ghostex app a Linux package runs and open it again on the
//! version the package manager just installed.
//!
//! CDXC:Updates 2026-11-25 WHY: the Linux update command (server/src/app_update/linux_install.rs) ends with this verb, so a `yay`, `pacman`, `apt`, `dnf` or `zypper` install leaves the new version running the way the in-app updater does. It runs as the user in their terminal, after the install, from the CLI the install just put down, so the app it quits is always an older build that knows no new message: it is ended by pid (only this user's app processes from this install folder and this Ghostex home, never a pattern match), the way `cargo xtask rebuild` closes it. gxserver and the zmx sessions are not touched: the reopened app finds a gxserver whose binary was replaced (`running_image_superseded`) or whose build differs, restarts only its control plane ("Finishing the update…"), and the sessions and their agents keep running. The app is reopened in its own `systemd-run --user --scope` with the environment the desktop gave the systemd user manager, not this shell's (a terminal inside Ghostex carries GHOSTEX_*, ZMX_* and agent variables, an SSH shell has no display), on the display the quit app was using. Nothing is opened when Ghostex was not running.
//! SEE-ALSO: tooling/xtask/src/linux_desktop.rs and tooling/xtask/src/start/install.rs (`launch_linux_app`, the same launch for `cargo xtask rebuild`), apps/desktop/src/app/os_integration/gxserver_bootstrap.rs (the reopened app's gxserver handover).
use crate::ghostex_cli::args::parse_args;
use crate::ghostex_cli::rpc::{CliError, CliResult};

pub fn app_restart_command(args: &[String]) -> CliResult<()> {
    let parsed = parse_args(args);
    let dry_run = parsed.flags.truthy("dryRun");
    #[cfg(target_os = "linux")]
    {
        if !super::launchers::is_wsl() {
            return linux::restart(dry_run);
        }
    }
    let _ = dry_run;
    Err(CliError::Other(
        "ghostex app-update restart is for Ghostex installed from a Linux package. On macOS and Windows, Ghostex restarts itself after its own update.".to_string(),
    ))
}

#[cfg(target_os = "linux")]
mod linux {
    use std::os::unix::fs::MetadataExt as _;
    use std::os::unix::process::CommandExt as _;
    use std::path::{Path, PathBuf};
    use std::process::{Command, Stdio};
    use std::time::{Duration, Instant};

    use crate::ghostex_cli::launchers::{
        current_cli_executable, is_linux_ghostex_desktop_executable, linux_environ_ghostex_home,
        resolve_linux_desktop_launch_from_cli,
    };
    use crate::ghostex_cli::rpc::{CliError, CliResult};

    const CEF_HELPER: &str = "ghostex-gpui-cef-helper";
    const QUIT_WAIT: Duration = Duration::from_secs(10);
    const KILL_WAIT: Duration = Duration::from_secs(3);
    /// How long the reopened app must stay up before the restart counts as done.
    const LAUNCH_SETTLE: Duration = Duration::from_secs(2);

    struct Running {
        app: Vec<i32>,
        helpers: Vec<i32>,
        /// The environment the first app process was started with.
        environ: Vec<u8>,
    }

    struct Desktop {
        env: Vec<(String, String)>,
        via_systemd: bool,
        summary: String,
    }

    pub(super) fn restart(dry_run: bool) -> CliResult<()> {
        let launch = resolve_linux_desktop_launch_from_cli(&current_cli_executable())?;
        let executable = PathBuf::from(&launch.command);
        let install = executable
            .parent()
            .map(Path::to_path_buf)
            .ok_or_else(|| CliError::Other("Ghostex desktop app was not found.".to_string()))?;
        let running = running_processes(&install);
        if running.app.is_empty() {
            println!("Ghostex was not open, so nothing was restarted.");
            return Ok(());
        }
        // Worked out before anything is quit: without a desktop to reopen in, Ghostex stays open.
        let desktop = desktop(&running.environ)
            .map_err(|why| CliError::Other(format!("Ghostex was left open: {why}.")))?;
        if dry_run {
            println!(
                "Would quit Ghostex (pids {}; helpers {}) and open {} in {}.",
                join(&running.app),
                if running.helpers.is_empty() {
                    "none".to_string()
                } else {
                    join(&running.helpers)
                },
                executable.display(),
                desktop.summary
            );
            return Ok(());
        }
        quit(&running);
        let mut command = if desktop.via_systemd {
            let mut scope = Command::new("systemd-run");
            scope
                .args(["--user", "--scope", "--collect", "--quiet"])
                .arg(format!(
                    "--unit=app-ghostex-{}-{}.scope",
                    std::process::id(),
                    std::time::SystemTime::now()
                        .duration_since(std::time::UNIX_EPOCH)
                        .map(|elapsed| elapsed.as_secs())
                        .unwrap_or_default()
                ))
                .arg("--description=Ghostex")
                .arg("--")
                .arg(&executable);
            scope
        } else {
            Command::new(&executable)
        };
        command
            .env_clear()
            .envs(desktop.env)
            .current_dir(&install)
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null());
        // SAFETY: setsid only detaches the child from this terminal's session; it allocates nothing.
        unsafe {
            command.pre_exec(|| {
                libc::setsid();
                Ok(())
            });
        }
        let mut child = command.spawn().map_err(|error| {
            CliError::Other(format!(
                "Ghostex was closed but could not be opened again ({error}). Open it from your app launcher."
            ))
        })?;
        let deadline = Instant::now() + LAUNCH_SETTLE;
        while Instant::now() < deadline {
            if let Ok(Some(status)) = child.try_wait() {
                if !status.success() {
                    return Err(CliError::Other(format!(
                        "Ghostex was closed but did not open again ({status}). Open it from your app launcher."
                    )));
                }
                break;
            }
            std::thread::sleep(Duration::from_millis(100));
        }
        println!("Restarted Ghostex. Your terminals and agents kept running.");
        Ok(())
    }

    fn join(pids: &[i32]) -> String {
        pids.iter()
            .map(i32::to_string)
            .collect::<Vec<_>>()
            .join(", ")
    }

    /// This user's Ghostex app and CEF helper processes started from `install` for this Ghostex
    /// home. gxserver and zmx run from the same folder and are never included.
    fn running_processes(install: &Path) -> Running {
        let mut running = Running {
            app: Vec::new(),
            helpers: Vec::new(),
            environ: Vec::new(),
        };
        let Ok(entries) = std::fs::read_dir("/proc") else {
            return running;
        };
        let own_pid = std::process::id() as i32;
        let own_uid = unsafe { libc::getuid() };
        let own_home = std::env::var("GHOSTEX_HOME")
            .ok()
            .filter(|home| !home.is_empty());
        for entry in entries.filter_map(Result::ok) {
            let Some(pid) = entry
                .file_name()
                .to_str()
                .and_then(|name| name.parse::<i32>().ok())
            else {
                continue;
            };
            if pid == own_pid
                || entry
                    .path()
                    .metadata()
                    .map_or(true, |meta| meta.uid() != own_uid)
            {
                continue;
            }
            let Ok(exe) = std::fs::read_link(entry.path().join("exe")) else {
                continue;
            };
            let Some(name) = exe.file_name().and_then(|name| name.to_str()) else {
                continue;
            };
            // A package upgrade that replaced the file leaves " (deleted)" on the link.
            let name = name.strip_suffix(" (deleted)").unwrap_or(name);
            if exe.parent() != Some(install) {
                continue;
            }
            let helper = name == CEF_HELPER;
            if !helper && !is_linux_ghostex_desktop_executable(&install.join(name)) {
                continue;
            }
            let Ok(environ) = std::fs::read(entry.path().join("environ")) else {
                continue;
            };
            if linux_environ_ghostex_home(&environ) != own_home.as_deref().map(str::as_bytes) {
                continue;
            }
            if helper {
                running.helpers.push(pid);
            } else {
                if running.app.is_empty() {
                    running.environ = environ;
                }
                running.app.push(pid);
            }
        }
        running
    }

    fn alive(pid: i32) -> bool {
        // A zombie still answers kill(0); its state says it is gone.
        let Ok(stat) = std::fs::read_to_string(format!("/proc/{pid}/stat")) else {
            return false;
        };
        let state = stat
            .rsplit_once(')')
            .and_then(|(_, rest)| rest.split_whitespace().next());
        !matches!(state, Some("Z" | "X"))
    }

    fn wait_gone(pids: &[i32], limit: Duration) -> bool {
        let deadline = Instant::now() + limit;
        loop {
            if pids.iter().all(|pid| !alive(*pid)) {
                return true;
            }
            if Instant::now() >= deadline {
                return false;
            }
            std::thread::sleep(Duration::from_millis(100));
        }
    }

    fn signal(pids: &[i32], signal: i32) {
        for pid in pids {
            if alive(*pid) {
                // SAFETY: plain kill(2) on a pid read from /proc for this user.
                unsafe {
                    libc::kill(*pid, signal);
                }
            }
        }
    }

    /// SIGTERM, then SIGKILL for what is left; the CEF helpers end with the app and are only ended
    /// here when they outlive it.
    fn quit(running: &Running) {
        signal(&running.app, libc::SIGTERM);
        if !wait_gone(&running.app, QUIT_WAIT) {
            signal(&running.app, libc::SIGKILL);
            wait_gone(&running.app, KILL_WAIT);
        }
        if !wait_gone(&running.helpers, KILL_WAIT) {
            signal(&running.helpers, libc::SIGKILL);
            wait_gone(&running.helpers, KILL_WAIT);
        }
    }

    /// The environment to reopen the app in: the desktop's (the systemd user manager's), on the
    /// display the quit app used when that display is still there.
    fn desktop(app_environ: &[u8]) -> Result<Desktop, String> {
        let app_env: Vec<(String, String)> = app_environ
            .split(|byte| *byte == 0)
            .filter_map(|entry| {
                let entry = String::from_utf8_lossy(entry);
                let (key, value) = entry.split_once('=')?;
                Some((key.to_string(), value.to_string()))
            })
            .collect();
        let manager = Command::new("systemctl")
            .args(["--user", "show-environment"])
            .stderr(Stdio::null())
            .output()
            .ok()
            .filter(|output| output.status.success())
            .map(|output| parse_show_environment(&String::from_utf8_lossy(&output.stdout)));
        let systemd_run = Command::new("systemd-run")
            .arg("--version")
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .status()
            .is_ok_and(|status| status.success());
        let (mut env, via_systemd) = match manager {
            Some(env) if systemd_run => (env, true),
            _ => (
                std::env::vars()
                    .filter(|(key, _)| !is_shell_session_variable(key))
                    .collect(),
                false,
            ),
        };
        if live_display(&app_env).is_some() {
            for key in ["DISPLAY", "WAYLAND_DISPLAY", "XDG_RUNTIME_DIR"] {
                env.retain(|(name, _)| name != key);
                if let Some(value) = get(&app_env, key) {
                    env.push((key.to_string(), value.to_string()));
                }
            }
        }
        env.retain(|(name, _)| name != "GHOSTEX_HOME");
        if let Some(home) = get(&app_env, "GHOSTEX_HOME") {
            env.push(("GHOSTEX_HOME".to_string(), home.to_string()));
        }
        let display = live_display(&env).ok_or_else(|| {
            "there is no desktop session to open it in (no WAYLAND_DISPLAY or DISPLAY whose socket exists)".to_string()
        })?;
        let summary = if via_systemd {
            format!("the desktop session ({display}) with systemd-run --user --scope")
        } else {
            format!("this shell's desktop ({display})")
        };
        Ok(Desktop {
            env,
            via_systemd,
            summary,
        })
    }

    fn get<'a>(env: &'a [(String, String)], name: &str) -> Option<&'a str> {
        env.iter()
            .find(|(key, value)| key == name && !value.is_empty())
            .map(|(_, value)| value.as_str())
    }

    /// Variables a shell inside a Ghostex session or an agent carries that must not reach the app.
    fn is_shell_session_variable(key: &str) -> bool {
        (key.starts_with("GHOSTEX_") && key != "GHOSTEX_HOME")
            || key.starts_with("ZMX_")
            || key == "CLAUDECODE"
            || key.starts_with("CLAUDE_CODE_")
    }

    /// The display the environment points at when its socket exists, e.g. `WAYLAND_DISPLAY=wayland-1`.
    fn live_display(env: &[(String, String)]) -> Option<String> {
        if let Some(wayland) = get(env, "WAYLAND_DISPLAY") {
            let socket = if wayland.starts_with('/') {
                PathBuf::from(wayland)
            } else {
                Path::new(get(env, "XDG_RUNTIME_DIR").unwrap_or_default()).join(wayland)
            };
            if socket.exists() {
                return Some(format!("WAYLAND_DISPLAY={wayland}"));
            }
        }
        let x11 = get(env, "DISPLAY")?;
        let number = x11
            .strip_prefix(':')?
            .split('.')
            .next()
            .filter(|n| !n.is_empty() && n.bytes().all(|b| b.is_ascii_digit()))?;
        Path::new(&format!("/tmp/.X11-unix/X{number}"))
            .exists()
            .then(|| format!("DISPLAY={x11}"))
    }

    /// `systemctl --user show-environment` prints `KEY=value`, quoting values with `$'...'` and C
    /// escapes when they hold special characters.
    fn parse_show_environment(text: &str) -> Vec<(String, String)> {
        text.lines()
            .filter_map(|line| {
                let (key, value) = line.split_once('=')?;
                if key.is_empty() {
                    return None;
                }
                let value = match value
                    .strip_prefix("$'")
                    .and_then(|rest| rest.strip_suffix('\''))
                {
                    Some(quoted) => unescape_c(quoted),
                    None => value.to_string(),
                };
                Some((key.to_string(), value))
            })
            .collect()
    }

    fn unescape_c(text: &str) -> String {
        let mut bytes = Vec::with_capacity(text.len());
        let mut chars = text.bytes().peekable();
        while let Some(byte) = chars.next() {
            if byte != b'\\' {
                bytes.push(byte);
                continue;
            }
            let Some(escape) = chars.next() else {
                bytes.push(b'\\');
                break;
            };
            match escape {
                b'a' => bytes.push(0x07),
                b'b' => bytes.push(0x08),
                b'e' => bytes.push(0x1b),
                b'f' => bytes.push(0x0c),
                b'n' => bytes.push(b'\n'),
                b'r' => bytes.push(b'\r'),
                b't' => bytes.push(b'\t'),
                b'v' => bytes.push(0x0b),
                b'x' => {
                    let mut value = 0u8;
                    for _ in 0..2 {
                        match chars.peek().and_then(|b| (*b as char).to_digit(16)) {
                            Some(digit) => {
                                value = value * 16 + digit as u8;
                                chars.next();
                            }
                            None => break,
                        }
                    }
                    bytes.push(value);
                }
                b'0'..=b'7' => {
                    let mut value = u32::from(escape - b'0');
                    for _ in 0..2 {
                        match chars.peek().filter(|b| (b'0'..=b'7').contains(*b)) {
                            Some(digit) => {
                                value = value * 8 + u32::from(digit - b'0');
                                chars.next();
                            }
                            None => break,
                        }
                    }
                    bytes.push(value as u8);
                }
                other => bytes.push(other),
            }
        }
        String::from_utf8_lossy(&bytes).into_owned()
    }
}
