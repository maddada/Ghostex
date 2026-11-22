//! Running the `agentbox` CLI from gxserver: resolved on the user's login-shell PATH, always with a
//! timeout, never on an async executor thread.

use std::{
    io::Read,
    path::{Path, PathBuf},
    process::Stdio,
    sync::{mpsc, Mutex, OnceLock},
    thread,
    time::{Duration, Instant},
};

/// Timeouts by command class (PLAN.md "Running the CLI from gxserver").
pub(crate) const STATUS_TIMEOUT: Duration = Duration::from_secs(20);
pub(crate) const URL_TIMEOUT: Duration = Duration::from_secs(60);
pub(crate) const STOP_TIMEOUT: Duration = Duration::from_secs(120);

/// How long a resolved binary path is reused before `command -v` runs again.
const RESOLVE_CACHE_TTL: Duration = Duration::from_secs(60);
/// Output kept per stream; `agentbox list -g --json` for dozens of boxes stays far below this.
const OUTPUT_LIMIT_BYTES: usize = 4 * 1024 * 1024;

pub(crate) struct AgentboxOutput {
    pub(crate) success: bool,
    pub(crate) exit_code: Option<i32>,
    pub(crate) stdout: String,
    pub(crate) stderr: String,
}

impl AgentboxOutput {
    /// The sentence a failed command reports: its stderr, else its stdout, else the exit status.
    pub(crate) fn failure_message(&self) -> String {
        let stderr = self.stderr.trim();
        if !stderr.is_empty() {
            return last_lines(stderr, 6);
        }
        let stdout = self.stdout.trim();
        if !stdout.is_empty() {
            return last_lines(stdout, 6);
        }
        match self.exit_code {
            Some(code) => format!("agentbox exited with status {code}."),
            None => "agentbox was stopped by a signal.".to_string(),
        }
    }
}

fn last_lines(text: &str, count: usize) -> String {
    let lines: Vec<&str> = text.lines().collect();
    lines[lines.len().saturating_sub(count)..].join("\n")
}

fn resolve_cache() -> &'static Mutex<Option<(Option<String>, Instant)>> {
    static CACHE: OnceLock<Mutex<Option<(Option<String>, Instant)>>> = OnceLock::new();
    CACHE.get_or_init(|| Mutex::new(None))
}

/// The `agentbox` executable the user's login shell would run, or `None` when it is not installed.
/// `fresh` re-probes now (the Settings page's Refresh); otherwise a minute-old answer is reused so
/// the activity poller does not start a shell every few seconds.
pub(crate) fn resolve_agentbox_binary(home: &Path, fresh: bool) -> Option<String> {
    if cfg!(windows) {
        return None;
    }
    if !fresh {
        if let Ok(cache) = resolve_cache().lock() {
            if let Some((path, at)) = cache.as_ref() {
                if at.elapsed() < RESOLVE_CACHE_TTL {
                    return path.clone();
                }
            }
        }
    }
    let path = crate::agent_hooks::probing::resolve_cli_command("agentbox", home)
        .filter(|path| Path::new(path).is_file());
    if let Ok(mut cache) = resolve_cache().lock() {
        *cache = Some((path.clone(), Instant::now()));
    }
    path
}

/// PATH for an agentbox child: the folder agentbox lives in first (its `#!/usr/bin/env node`
/// resolves the node it was installed with), then the user's login-shell PATH, which also holds the
/// `docker`, `git`, `ssh` and `tmux` it shells out to.
fn child_path(binary: &Path, home: &Path) -> String {
    let login_path = crate::agent_hooks::probing::normalize_gxserver_process_path(
        std::env::var("PATH").ok().as_deref(),
        home,
    );
    match binary.parent() {
        Some(folder) => format!("{}:{login_path}", folder.to_string_lossy()),
        None => login_path,
    }
}

/// Runs `agentbox <args…>` and waits at most `timeout`. Blocking: call it from `spawn_blocking`.
pub(crate) fn run_agentbox(
    home: &Path,
    args: &[&str],
    timeout: Duration,
    cwd: Option<&Path>,
) -> Result<AgentboxOutput, String> {
    let binary = resolve_agentbox_binary(home, false)
        .map(PathBuf::from)
        .ok_or_else(|| "agentbox is not installed on this computer.".to_string())?;
    let mut command = crate::platform::process::background_command(&binary);
    command
        .args(args)
        .env("HOME", home)
        .env("PATH", child_path(&binary, home))
        .env("NO_COLOR", "1")
        .env("FORCE_COLOR", "0")
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    command.current_dir(cwd.filter(|cwd| cwd.is_dir()).unwrap_or(home));
    // CDXC:AgentBox 2026-10-01 WHY: agentbox runs docker and ssh children; a timeout that killed only agentbox left them (and the threads reading their pipes) behind on every poll of an unreachable host. Its own process group lets a timeout kill all of them.
    #[cfg(unix)]
    {
        use std::os::unix::process::CommandExt;
        command.process_group(0);
    }
    let mut child = command
        .spawn()
        .map_err(|error| format!("Could not start agentbox: {error}"))?;
    let (sender, receiver) = mpsc::channel::<(bool, Vec<u8>)>();
    for (is_stdout, stream) in [
        (
            true,
            child
                .stdout
                .take()
                .map(|s| Box::new(s) as Box<dyn Read + Send>),
        ),
        (
            false,
            child
                .stderr
                .take()
                .map(|s| Box::new(s) as Box<dyn Read + Send>),
        ),
    ] {
        let Some(mut stream) = stream else {
            continue;
        };
        let sender = sender.clone();
        thread::spawn(move || {
            let mut bytes = Vec::new();
            let mut chunk = [0u8; 16 * 1024];
            loop {
                match stream.read(&mut chunk) {
                    Ok(0) | Err(_) => break,
                    Ok(read) => {
                        if bytes.len() < OUTPUT_LIMIT_BYTES {
                            bytes.extend_from_slice(&chunk[..read]);
                        }
                    }
                }
            }
            let _ = sender.send((is_stdout, bytes));
        });
    }
    drop(sender);
    let started = Instant::now();
    let status = loop {
        match child.try_wait() {
            Ok(Some(status)) => break status,
            Ok(None) if started.elapsed() >= timeout => {
                #[cfg(unix)]
                if let Ok(pid) = i32::try_from(child.id()) {
                    // SAFETY: signals the process group this call created; no memory is shared.
                    unsafe {
                        libc::kill(-pid, libc::SIGKILL);
                    }
                }
                let _ = child.kill();
                let _ = child.wait();
                return Err(format!(
                    "agentbox {} did not finish within {} seconds.",
                    args.first().copied().unwrap_or_default(),
                    timeout.as_secs()
                ));
            }
            Ok(None) => thread::sleep(Duration::from_millis(25)),
            Err(error) => {
                let _ = child.kill();
                return Err(format!("Could not wait for agentbox: {error}"));
            }
        }
    };
    // CDXC:AgentBox 2026-10-01 WHY: commands that open an SSH forward (`url` on a cloud or remote Docker box) can leave a background ssh holding the output pipes after agentbox itself exits, so the readers get a short grace instead of being joined.
    let mut stdout = Vec::new();
    let mut stderr = Vec::new();
    let grace = Instant::now() + Duration::from_millis(750);
    for _ in 0..2 {
        let remaining = grace.saturating_duration_since(Instant::now());
        match receiver.recv_timeout(remaining) {
            Ok((true, bytes)) => stdout = bytes,
            Ok((false, bytes)) => stderr = bytes,
            Err(_) => break,
        }
    }
    Ok(AgentboxOutput {
        success: status.success(),
        exit_code: status.code(),
        stdout: String::from_utf8_lossy(&stdout).into_owned(),
        stderr: String::from_utf8_lossy(&stderr).into_owned(),
    })
}
