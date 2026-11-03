//! Running the `docker` CLI for the `local` provider: resolved on the user's login-shell PATH,
//! always with a timeout, never on an async executor thread.

use std::{
    io::{Read, Write},
    path::Path,
    process::Stdio,
    sync::mpsc,
    thread,
    time::{Duration, Instant},
};

pub(super) const QUICK_TIMEOUT: Duration = Duration::from_secs(20);
/// `docker run` may pull the base image first.
pub(super) const CREATE_TIMEOUT: Duration = Duration::from_secs(600);
pub(super) const BOOT_TIMEOUT: Duration = Duration::from_secs(120);

const OUTPUT_LIMIT_BYTES: usize = 4 * 1024 * 1024;

pub(super) struct DockerOutput {
    pub(super) success: bool,
    pub(super) stdout: String,
    pub(super) stderr: String,
}

impl DockerOutput {
    /// The sentence a failed command reports: its stderr's last lines, else its stdout's.
    pub(super) fn failure_message(&self) -> String {
        let text = if self.stderr.trim().is_empty() {
            self.stdout.trim()
        } else {
            self.stderr.trim()
        };
        let lines: Vec<&str> = text.lines().collect();
        let tail = lines[lines.len().saturating_sub(4)..].join("\n");
        if tail.is_empty() {
            "docker failed without saying why.".to_string()
        } else {
            tail
        }
    }

    /// stdout when the command succeeded, else its failure as the error.
    pub(super) fn into_stdout(self) -> Result<String, String> {
        if self.success {
            Ok(self.stdout)
        } else {
            Err(self.failure_message())
        }
    }
}

/// The `docker` executable the user's login shell would run.
pub(super) fn resolve_docker(home: &Path) -> Option<String> {
    if cfg!(windows) {
        return None;
    }
    crate::agent_hooks::probing::resolve_cli_command("docker", home)
        .filter(|path| Path::new(path).is_file())
}

/// Runs `docker <args…>` (with `stdin` piped in when given) and waits at most `timeout`.
/// Blocking: call it from `spawn_blocking`.
pub(super) fn run_docker(
    home: &Path,
    args: &[String],
    stdin: Option<&[u8]>,
    timeout: Duration,
) -> Result<DockerOutput, String> {
    let binary = resolve_docker(home)
        .ok_or_else(|| "Docker isn't installed on this computer.".to_string())?;
    let mut command = crate::platform::process::background_command(Path::new(&binary));
    command
        .args(args)
        .env("HOME", home)
        .env(
            "PATH",
            crate::agent_hooks::probing::normalize_gxserver_process_path(
                std::env::var("PATH").ok().as_deref(),
                home,
            ),
        )
        .stdin(if stdin.is_some() {
            Stdio::piped()
        } else {
            Stdio::null()
        })
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    let mut child = command
        .spawn()
        .map_err(|error| format!("Could not start docker: {error}"))?;
    if let (Some(bytes), Some(mut pipe)) = (stdin, child.stdin.take()) {
        let bytes = bytes.to_vec();
        thread::spawn(move || {
            let _ = pipe.write_all(&bytes);
        });
    }
    let (sender, receiver) = mpsc::channel::<(bool, Vec<u8>)>();
    let streams: [(bool, Option<Box<dyn Read + Send>>); 2] = [
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
    ];
    for (is_stdout, stream) in streams {
        let Some(mut stream) = stream else { continue };
        let sender = sender.clone();
        thread::spawn(move || {
            let mut bytes = Vec::new();
            let mut chunk = [0u8; 16 * 1024];
            while let Ok(read) = stream.read(&mut chunk) {
                if read == 0 {
                    break;
                }
                if bytes.len() < OUTPUT_LIMIT_BYTES {
                    bytes.extend_from_slice(&chunk[..read]);
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
                let _ = child.kill();
                let _ = child.wait();
                return Err(format!(
                    "docker {} did not finish within {} seconds.",
                    args.first().map(String::as_str).unwrap_or_default(),
                    timeout.as_secs()
                ));
            }
            Ok(None) => thread::sleep(Duration::from_millis(25)),
            Err(error) => {
                let _ = child.kill();
                return Err(format!("Could not wait for docker: {error}"));
            }
        }
    };
    let mut stdout = Vec::new();
    let mut stderr = Vec::new();
    let grace = Instant::now() + Duration::from_millis(750);
    for _ in 0..2 {
        match receiver.recv_timeout(grace.saturating_duration_since(Instant::now())) {
            Ok((true, bytes)) => stdout = bytes,
            Ok((false, bytes)) => stderr = bytes,
            Err(_) => break,
        }
    }
    Ok(DockerOutput {
        success: status.success(),
        stdout: String::from_utf8_lossy(&stdout).into_owned(),
        stderr: String::from_utf8_lossy(&stderr).into_owned(),
    })
}
