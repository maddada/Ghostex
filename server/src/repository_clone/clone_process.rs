use std::{collections::HashMap, env, sync::Arc, time::Duration};

use serde_json::{json, Value};
use tokio::{
    io::{AsyncRead, AsyncReadExt},
    process::{Child, Command},
    sync::{mpsc, Mutex},
    time::{sleep, Instant},
};

use super::*;

/// How often the latest git progress line is copied onto the job the dialog polls.
const PROGRESS_PUBLISH_INTERVAL: Duration = Duration::from_millis(250);
/// How long the output readers may run on after git exits before what they have is used.
const OUTPUT_DRAIN_TIMEOUT: Duration = Duration::from_secs(3);

/*
CDXC:AddProject 2026-10-09 WHY:
A clone can run for many minutes (a 13 GB repository took ten on Windows), so git runs with
`--progress` and its latest progress line is published on the job as `progress`; the dialog shows
it instead of a frozen "Cloning...". Progress redraws end in `\r` and are never stored, so they
cannot trip the output cap. Git runs in a console nobody can see, so it must never prompt: terminal
prompts are off and SSH runs in BatchMode, which turns a sign-in or host-key question into an error
the dialog can show. The timeout counts from git's last progress, not from the start, so a long
clone that keeps moving is never cut off. On Windows `git` is a shim that starts the real git.exe, so cancel and timeout
end the whole process tree; killing the shim alone left git cloning while the job said canceled.
*/
pub(super) async fn run_git_clone_process(
    args: Vec<String>,
    cwd: String,
    jobs: Arc<Mutex<HashMap<String, Value>>>,
    job_id: String,
) -> Result<CloneRunOutput, RepositoryCloneError> {
    let clone_url = args.iter().rev().nth(1).cloned().unwrap_or_default();
    let mut environment = vec![("GIT_TERMINAL_PROMPT".to_string(), "0".to_string())];
    if let Some(ssh_command) = non_interactive_ssh_command(&clone_url, &cwd).await {
        environment.push(("GIT_SSH_COMMAND".to_string(), ssh_command));
    }
    run_clone_process_with_environment("git", args, cwd, environment, jobs, job_id).await
}

#[cfg(test)]
pub(super) async fn run_clone_process(
    executable: &str,
    args: Vec<String>,
    cwd: String,
    jobs: Arc<Mutex<HashMap<String, Value>>>,
    job_id: String,
) -> Result<CloneRunOutput, RepositoryCloneError> {
    run_clone_process_with_environment(executable, args, cwd, Vec::new(), jobs, job_id).await
}

async fn run_clone_process_with_environment(
    executable: &str,
    args: Vec<String>,
    cwd: String,
    extra_environment: Vec<(String, String)>,
    jobs: Arc<Mutex<HashMap<String, Value>>>,
    job_id: String,
) -> Result<CloneRunOutput, RepositoryCloneError> {
    let mut command = Command::new(executable);
    command
        .args(args)
        .current_dir(cwd)
        .env_clear()
        .envs(repository_clone_environment())
        .envs(extra_environment)
        .kill_on_drop(true)
        .stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped());
    crate::platform::process::NoConsoleWindow::no_console_window(&mut command);
    let mut child = command.spawn().map_err(|error| {
        RepositoryCloneError::dependency_unavailable(format!("Could not start git clone: {error}"))
    })?;
    let (limit_sender, mut limit_receiver) = mpsc::unbounded_channel();
    let progress = Arc::new(std::sync::Mutex::new(String::new()));
    let stdout_buffer = Arc::new(std::sync::Mutex::new(Vec::new()));
    let stderr_buffer = Arc::new(std::sync::Mutex::new(Vec::new()));
    let stdout_task = child.stdout.take().map(|stdout| {
        tokio::spawn(read_capped_output(
            stdout,
            "stdout",
            stdout_buffer.clone(),
            None,
            limit_sender.clone(),
        ))
    });
    let stderr_task = child.stderr.take().map(|stderr| {
        tokio::spawn(read_capped_output(
            stderr,
            "stderr",
            stderr_buffer.clone(),
            Some(progress.clone()),
            limit_sender,
        ))
    });
    let idle_timeout = Duration::from_millis(REPOSITORY_CLONE_TIMEOUT_MS);
    let mut last_activity = Instant::now();
    let mut seen_progress = String::new();
    let mut terminal_error: Option<RepositoryCloneError> = None;
    let mut sigkill_deadline: Option<Instant> = None;
    let mut sigkill_sent = false;
    let mut termination_requested = false;
    let mut published_progress = String::new();
    let mut progress_published_at = Instant::now();
    let status = loop {
        match child.try_wait() {
            Ok(Some(status)) => break status,
            Ok(None) => {}
            Err(error) => {
                return Err(RepositoryCloneError::dependency_unavailable(format!(
                    "git clone failed: {error}"
                )));
            }
        }
        while let Ok(limit_error) = limit_receiver.try_recv() {
            if terminal_error.is_none() {
                terminal_error = Some(limit_error);
                if !termination_requested {
                    sigkill_deadline = request_child_termination(&mut child).await;
                    termination_requested = true;
                }
            }
        }
        let latest = progress.lock().map(|line| line.clone()).unwrap_or_default();
        if latest != seen_progress {
            seen_progress = latest;
            last_activity = Instant::now();
        }
        if terminal_error.is_none() && last_activity.elapsed() >= idle_timeout {
            terminal_error = Some(RepositoryCloneError::dependency_unavailable(format!(
                "git clone made no progress for {} minutes, so it was stopped.",
                REPOSITORY_CLONE_TIMEOUT_MS / 60_000
            )));
            if !termination_requested {
                sigkill_deadline = request_child_termination(&mut child).await;
                termination_requested = true;
            }
        }
        if terminal_error.is_none()
            && !termination_requested
            && job_is_canceled(&jobs, &job_id).await
        {
            sigkill_deadline = request_child_termination(&mut child).await;
            termination_requested = true;
        }
        if let Some(deadline) = sigkill_deadline {
            if !sigkill_sent && Instant::now() >= deadline {
                let _ = child.start_kill();
                sigkill_sent = true;
            }
        }
        if progress_published_at.elapsed() >= PROGRESS_PUBLISH_INTERVAL {
            progress_published_at = Instant::now();
            if !seen_progress.is_empty() && seen_progress != published_progress {
                publish_job_progress(&jobs, &job_id, &seen_progress).await;
                published_progress = seen_progress.clone();
            }
        }
        tokio::select! {
            Some(limit_error) = limit_receiver.recv() => {
                if terminal_error.is_none() {
                    terminal_error = Some(limit_error);
                    if !termination_requested {
                        sigkill_deadline = request_child_termination(&mut child).await;
                        termination_requested = true;
                    }
                }
            }
            _ = sleep(Duration::from_millis(25)) => {}
        }
    };
    // A helper git started (a credential manager, say) can outlive it and keep the pipes open.
    drain_output_task(stdout_task).await;
    drain_output_task(stderr_task).await;
    let stdout = buffered_output(&stdout_buffer);
    let stderr = buffered_output(&stderr_buffer);
    while let Ok(limit_error) = limit_receiver.try_recv() {
        if terminal_error.is_none() {
            terminal_error = Some(limit_error);
        }
    }
    if let Some(error) = terminal_error {
        return Err(error);
    }
    let was_canceled = job_is_canceled(&jobs, &job_id).await;
    Ok(CloneRunOutput {
        exit_code: status.code().unwrap_or(if was_canceled { 130 } else { 1 }),
        stderr,
        stdout,
    })
}

/// Reads one stream into `buffer` up to the output cap. With `progress`, carriage-return
/// redraws (git's progress meter) only replace the latest progress line and are not stored.
async fn read_capped_output<R>(
    mut reader: R,
    stream_name: &'static str,
    buffer: Arc<std::sync::Mutex<Vec<u8>>>,
    progress: Option<Arc<std::sync::Mutex<String>>>,
    limit_sender: mpsc::UnboundedSender<RepositoryCloneError>,
) where
    R: AsyncRead + Unpin,
{
    let mut total_bytes = 0usize;
    let mut reported_limit = false;
    let mut pending_line: Vec<u8> = Vec::new();
    let mut chunk = [0u8; 8192];
    loop {
        let read = match reader.read(&mut chunk).await {
            Ok(0) => break,
            Ok(read) => read,
            Err(_) => break,
        };
        let stored: Vec<u8> = match &progress {
            None => chunk[..read].to_vec(),
            Some(progress) => {
                let mut stored = Vec::new();
                for &byte in &chunk[..read] {
                    match byte {
                        b'\r' | b'\n' => {
                            let line = String::from_utf8_lossy(&pending_line).trim().to_string();
                            if !line.is_empty() {
                                if let Ok(mut latest) = progress.lock() {
                                    *latest = line;
                                }
                            }
                            if byte == b'\n' {
                                stored.extend_from_slice(&pending_line);
                                stored.push(b'\n');
                            }
                            pending_line.clear();
                        }
                        _ => pending_line.push(byte),
                    }
                }
                stored
            }
        };
        let next_bytes = total_bytes.saturating_add(stored.len());
        let remaining = REPOSITORY_CLONE_OUTPUT_LIMIT_BYTES.saturating_sub(total_bytes);
        if remaining > 0 {
            if let Ok(mut buffer) = buffer.lock() {
                buffer.extend_from_slice(&stored[..stored.len().min(remaining)]);
            }
        }
        if next_bytes > REPOSITORY_CLONE_OUTPUT_LIMIT_BYTES && !reported_limit {
            let _ = limit_sender.send(RepositoryCloneError::dependency_unavailable(format!(
                "git clone {stream_name} exceeded {REPOSITORY_CLONE_OUTPUT_LIMIT_BYTES} bytes."
            )));
            reported_limit = true;
        }
        total_bytes = next_bytes;
    }
    if !pending_line.is_empty() {
        if let Ok(mut buffer) = buffer.lock() {
            let remaining = REPOSITORY_CLONE_OUTPUT_LIMIT_BYTES.saturating_sub(buffer.len());
            buffer.extend_from_slice(&pending_line[..pending_line.len().min(remaining)]);
        }
    }
}

async fn drain_output_task(task: Option<tokio::task::JoinHandle<()>>) {
    let Some(mut task) = task else {
        return;
    };
    if tokio::time::timeout(OUTPUT_DRAIN_TIMEOUT, &mut task)
        .await
        .is_err()
    {
        task.abort();
    }
}

fn buffered_output(buffer: &Arc<std::sync::Mutex<Vec<u8>>>) -> String {
    buffer
        .lock()
        .map(|buffer| String::from_utf8_lossy(&buffer).trim().to_string())
        .unwrap_or_default()
}

async fn publish_job_progress(
    jobs: &Arc<Mutex<HashMap<String, Value>>>,
    job_id: &str,
    progress: &str,
) {
    let mut jobs = jobs.lock().await;
    if let Some(job) = jobs.get_mut(job_id).and_then(Value::as_object_mut) {
        if job.get("state").and_then(Value::as_str) == Some("running") {
            job.insert("progress".to_string(), json!(progress));
        }
    }
}

async fn request_child_termination(child: &mut Child) -> Option<Instant> {
    #[cfg(unix)]
    {
        if let Some(pid) = child.id() {
            unsafe {
                libc::kill(pid as libc::pid_t, libc::SIGTERM);
            }
            return Some(Instant::now() + Duration::from_millis(1_000));
        }
    }
    #[cfg(windows)]
    if let Some(pid) = child.id() {
        let mut kill = Command::new("taskkill.exe");
        crate::platform::process::NoConsoleWindow::no_console_window(&mut kill);
        let _ = kill
            .args(["/PID", &pid.to_string(), "/T", "/F"])
            .stdin(std::process::Stdio::null())
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::null())
            .status()
            .await;
    }
    let _ = child.start_kill();
    None
}

/// `GIT_SSH_COMMAND` for an SSH remote: the user's own SSH command (environment or
/// `core.sshCommand`) with BatchMode on, so it fails instead of waiting on a hidden prompt.
/// `None` for HTTP remotes and when `GIT_SSH` picks a program that may not take `-o`.
async fn non_interactive_ssh_command(clone_url: &str, cwd: &str) -> Option<String> {
    let lower = clone_url.to_ascii_lowercase();
    if lower.starts_with("https://") || lower.starts_with("http://") || lower.starts_with("file://")
    {
        return None;
    }
    if env::var_os("GIT_SSH").is_some() {
        return None;
    }
    let configured = match env::var("GIT_SSH_COMMAND") {
        Ok(command) if !command.trim().is_empty() => command,
        _ => {
            let mut command = Command::new("git");
            command
                .args(["config", "--get", "core.sshCommand"])
                .current_dir(cwd)
                .stdin(std::process::Stdio::null())
                .kill_on_drop(true);
            crate::platform::process::NoConsoleWindow::no_console_window(&mut command);
            tokio::time::timeout(Duration::from_secs(5), command.output())
                .await
                .ok()
                .and_then(Result::ok)
                .map(|output| String::from_utf8_lossy(&output.stdout).trim().to_string())
                .filter(|command| !command.is_empty())
                .unwrap_or_else(|| "ssh".to_string())
        }
    };
    Some(format!("{configured} -o BatchMode=yes"))
}

/// One sentence for the dialog from a failed clone's stderr: git's first `fatal:`/`error:` line
/// (else its last line), with a next step for the two prompts a background clone cannot answer.
pub(super) fn summarize_clone_failure(stderr: &str, exit_code: i32) -> String {
    let lines: Vec<&str> = stderr
        .lines()
        .map(str::trim)
        .filter(|line| !line.is_empty())
        .collect();
    // The first fatal line is the cause; later ones (`invalid index-pack output`) follow from it.
    let detail = lines
        .iter()
        .find_map(|line| {
            line.strip_prefix("fatal:")
                .or_else(|| line.strip_prefix("error:"))
                .map(str::trim)
        })
        .or_else(|| lines.last().copied())
        .unwrap_or("")
        .to_string();
    let lower = stderr.to_ascii_lowercase();
    if lower.contains("terminal prompts disabled") || lower.contains("could not read username") {
        return "Git needs you to sign in to this repository. Sign in with Git Credential Manager or `gh auth login`, then clone again.".to_string();
    }
    if lower.contains("host key verification failed") {
        return "SSH does not trust this host yet. Connect to it once from a terminal (for example `ssh -T git@github.com`), then clone again.".to_string();
    }
    if detail.is_empty() {
        return format!("git clone exited with code {exit_code}.");
    }
    let mut sentence = detail;
    if !sentence.ends_with('.') {
        sentence.push('.');
    }
    format!("Clone failed: {sentence}")
}

fn repository_clone_environment() -> Vec<(String, String)> {
    let mut environment: Vec<(String, String)> = env::vars().collect();
    environment.retain(|(key, _)| {
        !matches!(
            key.as_str(),
            "ANSI_COLORS_DISABLED" | "NO_COLOR" | "NODE_DISABLE_COLORS"
        )
    });
    environment
}
