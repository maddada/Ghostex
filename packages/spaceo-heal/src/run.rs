//! Runs a command with a deadline and captures what it printed.

use std::{
    io::Read,
    path::Path,
    process::{Command, Stdio},
    thread,
    time::{Duration, Instant},
};

const CAPTURE_LIMIT: usize = 256 * 1024;

#[derive(Clone, Debug, Default)]
pub struct Output {
    pub success: bool,
    pub code: Option<i32>,
    pub stdout: String,
    pub stderr: String,
}

impl Output {
    /// Both streams, trimmed, for a report.
    pub fn text(&self) -> String {
        let mut text = self.stdout.trim().to_string();
        let stderr = self.stderr.trim();
        if !stderr.is_empty() {
            if !text.is_empty() {
                text.push('\n');
            }
            text.push_str(stderr);
        }
        text
    }
}

/// Runs `program args` with stdin closed and kills its process group when `timeout` passes.
pub fn run(program: &Path, args: &[&str], timeout: Duration) -> Result<Output, String> {
    run_capturing(program, args, timeout, CAPTURE_LIMIT)
}

/// `run` keeping up to `limit` bytes of each stream (a full `ps` listing can pass 256 KiB).
pub fn run_capturing(
    program: &Path,
    args: &[&str],
    timeout: Duration,
    limit: usize,
) -> Result<Output, String> {
    let mut command = Command::new(program);
    command
        .args(args)
        .env("NO_COLOR", "1")
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    #[cfg(unix)]
    {
        use std::os::unix::process::CommandExt;
        command.process_group(0);
    }
    let mut child = command
        .spawn()
        .map_err(|error| format!("could not start {}: {error}", program.display()))?;
    let drain = |pipe: Option<Box<dyn Read + Send>>| {
        thread::spawn(move || {
            let mut bytes = Vec::new();
            if let Some(mut pipe) = pipe {
                let mut chunk = [0u8; 8192];
                while let Ok(read) = pipe.read(&mut chunk) {
                    if read == 0 {
                        break;
                    }
                    if bytes.len() < limit {
                        bytes.extend_from_slice(&chunk[..read]);
                    }
                }
            }
            String::from_utf8_lossy(&bytes).into_owned()
        })
    };
    let stdout = drain(
        child
            .stdout
            .take()
            .map(|pipe| Box::new(pipe) as Box<dyn Read + Send>),
    );
    let stderr = drain(
        child
            .stderr
            .take()
            .map(|pipe| Box::new(pipe) as Box<dyn Read + Send>),
    );
    let deadline = Instant::now() + timeout;
    let status = loop {
        match child.try_wait() {
            Ok(Some(status)) => break status,
            Ok(None) if Instant::now() >= deadline => {
                #[cfg(unix)]
                // SAFETY: signals only the process group this call started.
                unsafe {
                    libc::kill(-(child.id() as i32), libc::SIGKILL);
                }
                let _ = child.kill();
                let _ = child.wait();
                return Err(format!(
                    "{} did not finish within {} seconds",
                    program.display(),
                    timeout.as_secs()
                ));
            }
            Ok(None) => thread::sleep(Duration::from_millis(50)),
            Err(error) => return Err(error.to_string()),
        }
    };
    Ok(Output {
        success: status.success(),
        code: status.code(),
        stdout: stdout.join().unwrap_or_default(),
        stderr: stderr.join().unwrap_or_default(),
    })
}

/// The JSON object a `--json` command printed, ignoring any text around it.
pub fn json_object(stdout: &str) -> Option<serde_json::Value> {
    let start = stdout.find('{')?;
    let end = stdout.rfind('}')?;
    if start > end {
        return None;
    }
    serde_json::from_str(&stdout[start..=end]).ok()
}
