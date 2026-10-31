//! Running an installer command from a blocking install job, streaming its output into the job.

use std::{
    ffi::OsStr,
    io::Read,
    path::{Path, PathBuf},
    process::{Command, ExitStatus, Stdio},
    time::{Duration, Instant},
};

use super::jobs::Log;
use crate::platform::process::{self, ProcessTree};

/// The PATH a tool command runs with: `prefix` first (a tool installed moments ago), then the
/// normalized session PATH, which already ends with Ghostex's tool folders.
pub(crate) fn job_path(prefix: &[PathBuf]) -> std::ffi::OsString {
    #[cfg(windows)]
    let base = crate::platform::live_path::value();
    #[cfg(not(windows))]
    let base = crate::agent_hooks::probing::normalize_gxserver_process_path(
        std::env::var("PATH").ok().as_deref(),
        &ghostex_paths::GhostexPaths::resolve().home_dir,
    );
    let mut entries: Vec<PathBuf> = prefix.to_vec();
    entries.extend(std::env::split_paths(&base));
    std::env::join_paths(entries).unwrap_or_else(|_| base.into())
}

pub(crate) fn run(
    program: impl AsRef<OsStr>,
    args: &[&str],
    env: &[(&str, &OsStr)],
    prefix: &[PathBuf],
    log: &Log,
    timeout: Duration,
) -> Result<(), String> {
    let program = program.as_ref();
    let mut command = Command::new(program);
    command
        .args(args)
        .env("PATH", job_path(prefix))
        .env("NO_COLOR", "1")
        .env("TERM", "dumb")
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    for (key, value) in env {
        command.env(key, value);
    }
    let name = Path::new(program)
        .file_name()
        .map(|name| name.to_string_lossy().into_owned())
        .unwrap_or_default();
    let mut tree = ProcessTree::spawn(&mut command)
        .map_err(|error| format!("Could not start {name}: {error}"))?;
    let readers = [
        tree.child()
            .stdout
            .take()
            .map(|pipe| drain(pipe, log.clone())),
        tree.child()
            .stderr
            .take()
            .map(|pipe| drain(pipe, log.clone())),
    ];
    let deadline = Instant::now() + timeout;
    let status = loop {
        match tree.child().try_wait() {
            Ok(Some(status)) => break status,
            Ok(None) if Instant::now() >= deadline => {
                tree.end();
                return Err(format!(
                    "{name} did not finish within {} minutes.",
                    timeout.as_secs() / 60
                ));
            }
            Ok(None) => std::thread::sleep(Duration::from_millis(200)),
            Err(error) => return Err(error.to_string()),
        }
    };
    // What a reader collected is already in the job's log; one held open by a leftover is let go.
    // Both readers share one grace, so a leftover holding both pipes costs it once.
    let readers_deadline = Instant::now() + crate::platform::process::COMMAND_READER_GRACE;
    for reader in readers.into_iter().flatten() {
        let _ = crate::platform::process::join_reader_within(reader, readers_deadline, &name);
    }
    if status.success() {
        Ok(())
    } else {
        Err(format!("{name} exited with {status}."))
    }
}

fn drain(mut pipe: impl Read + Send + 'static, log: Log) -> std::thread::JoinHandle<()> {
    std::thread::spawn(move || {
        let mut buffer = [0u8; 8192];
        while let Ok(read) = pipe.read(&mut buffer) {
            if read == 0 {
                break;
            }
            log.chunk(String::from_utf8_lossy(&buffer[..read]).into_owned());
        }
    })
}

/// How long a tool gets to print its version or status before Ghostex stops it.
///
/// CDXC:ManagedTools 2026-10-27 WHY:
/// GitHub #336: a `gh` wrapper (`mise x gh -- gh`) found itself again and looped forever, and the
/// version check had no time limit, so Settings > Integrations' Tools list never loaded and every
/// open of the page left another looping `gh --version` burning CPU. A real `--version` answers in
/// well under a second; five seconds leaves room for a cold shim (mise, asdf, a Node script).
pub(crate) const PROBE_TIMEOUT: Duration = Duration::from_secs(5);

/// What a bounded command printed before it exited.
pub(crate) struct Finished {
    pub status: ExitStatus,
    pub stdout: String,
    pub stderr: String,
}

pub(crate) enum Unfinished {
    /// It could not start at all (missing, not executable).
    Start,
    /// It ran past its time limit and was stopped with everything it started.
    TimedOut(Duration),
}

/// Runs `command` with no input and at most `timeout`, then stops it and every process it started.
/// The command leads its own process tree (`ProcessTree`), so a wrapper's children go too.
pub(crate) fn output_within(command: Command, timeout: Duration) -> Result<Finished, Unfinished> {
    // A version or status answer is short; a runaway printer is not kept whole.
    let keep = process::Keep::Head(256 * 1024);
    let limits = process::RunLimits {
        timeout,
        stdin: None,
        stdout: keep,
        stderr: keep,
    };
    match process::run_bounded(command, limits) {
        Ok(output) => Ok(Finished {
            status: output.status,
            stdout: String::from_utf8_lossy(&output.stdout).into_owned(),
            stderr: String::from_utf8_lossy(&output.stderr).into_owned(),
        }),
        Err(process::Unfinished::Start(_)) => Err(Unfinished::Start),
        Err(process::Unfinished::TimedOut(limit)) => Err(Unfinished::TimedOut(limit)),
        Err(process::Unfinished::TooLarge(_)) => {
            unreachable!("a stream kept by its head never runs over its cap")
        }
    }
}
