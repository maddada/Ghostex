//! How the gxserver the app started ended (Windows and Linux), and on Windows the crash evidence gxserver left, for the startup failure toast and its copyable report.
//!
//! CDXC:ServerDaemon 2026-10-28 WHY:
//! Linux started gxserver through a detached `nohup` shell, so the app never saw it end: a gxserver that quit at once (its port taken, an unwritable data folder) left "Loading sessions…" up for the whole three-minute start patience with the reason only in the launch log. Linux now keeps the process it starts too and records how it ended here, beside Windows, and the start-failure text (`gxserver_start_failure.rs`) reads what it printed from the launch log offset it was started at.
//!
//! CDXC:ServerDaemon 2026-10-24 WHY:
//! A Windows reporter's gxserver (GitHub #292, like #289 before it) died about a second after every start, the app relaunched it every 30 s, and the startup report held nothing but timeouts: the launcher log was empty, gxserver had written no log line, and the report could only say the recorded process was gone. Windows hands the exit status only to whoever holds the process, and a fail-fast abort, heap corruption or another program ending gxserver never reaches gxserver's own crash filter (server/src/crash_report.rs). So the app keeps the process it starts, and when it ends writes its exit status to the launcher log, the report's timeline and the toast, and the report adds gxserver's crash record and its latest warnings and errors.
//! SEE-ALSO: `crash_report` in server/src/crash_report.rs (writes `gxserver-crash.json` and `gxserverCrashed`), `gpui_spawn_local_gxserver_daemon` (Windows) in gxserver_health_and_daemon.rs.

#[cfg(windows)]
use std::io::{Read, Seek, SeekFrom};
use std::{
    collections::VecDeque,
    fs,
    io::Write,
    path::{Path, PathBuf},
    sync::Mutex,
    time::{Duration, Instant},
};

#[cfg(windows)]
use crate::shared_settings;

/// Exits kept for the report timeline; the app starts gxserver at most every 30 s.
const KEPT_EXITS: usize = 16;
/// An exit older than this no longer explains a failure toast.
#[cfg(windows)]
const TOAST_EXIT_MAX_AGE: Duration = Duration::from_secs(120);
/// Warnings and errors from gxserver's log copied into the report.
#[cfg(windows)]
const REPORTED_LOG_LINES: usize = 8;
/// How far back from the end of gxserver's log the report looks.
#[cfg(windows)]
const LOG_TAIL_BYTES: u64 = 256 * 1024;

#[derive(Clone)]
pub(crate) struct GpuiGxserverExit {
    pub(crate) pid: u32,
    pub(crate) ran_for: Duration,
    pub(crate) started_at: Instant,
    pub(crate) ended_at: Instant,
    pub(crate) code: Option<u32>,
    /// The signal that ended it (Unix), when it did not exit on its own.
    pub(crate) signal: Option<i32>,
    /// The launch log's length when it was started: what it printed starts here.
    pub(crate) log_offset: u64,
    /// Already copied into a startup report's timeline.
    reported: bool,
}

impl GpuiGxserverExit {
    /// It stopped on its own with status 0 (a handoff to a newer gxserver, a requested stop).
    pub(crate) fn succeeded(&self) -> bool {
        self.code == Some(0) && self.signal.is_none()
    }
}

static EXITS: Mutex<VecDeque<GpuiGxserverExit>> = Mutex::new(VecDeque::new());

/// Waits on the gxserver the app just started, on its own thread, and records how it ended.
/// `log_offset` is the launch log's length when it was started.
pub(crate) fn gpui_watch_gxserver_exit(
    mut child: std::process::Child,
    launch_log: PathBuf,
    log_offset: u64,
) {
    let pid = child.id();
    let started_at = Instant::now();
    let spawned = std::thread::Builder::new()
        .name("gxserver-exit-watch".into())
        .spawn(move || {
            // A wait that fails leaves the exit unknown, never a guessed status.
            let status = child.wait().ok();
            let code = status
                .and_then(|status| status.code())
                .map(|code| code as u32);
            #[cfg(unix)]
            let signal = {
                use std::os::unix::process::ExitStatusExt as _;
                status.and_then(|status| status.signal())
            };
            #[cfg(not(unix))]
            let signal = None;
            let exit = GpuiGxserverExit {
                pid,
                ran_for: started_at.elapsed(),
                started_at,
                ended_at: Instant::now(),
                code,
                signal,
                log_offset,
                reported: false,
            };
            append_launch_log_line(&launch_log, &exit);
            if let Ok(mut exits) = EXITS.lock() {
                if exits.len() == KEPT_EXITS {
                    exits.pop_front();
                }
                exits.push_back(exit);
            }
        });
    if let Err(error) = spawned {
        eprintln!("Ghostex could not watch gxserver pid {pid}: {error}");
    }
}

/// The newest exit of a gxserver started at or after `launched_since`, unless it stopped normally.
pub(crate) fn gpui_failed_gxserver_exit_since(launched_since: Instant) -> Option<GpuiGxserverExit> {
    let exits = EXITS.lock().ok()?;
    exits
        .iter()
        .rev()
        .find(|exit| exit.started_at >= launched_since)
        .filter(|exit| !exit.succeeded())
        .cloned()
}

/// The startup report's timeline lines for exits it has not shown yet, timed from `startup_started`.
pub(crate) fn gpui_take_gxserver_exit_notes(startup_started: Instant) -> Vec<String> {
    let Ok(mut exits) = EXITS.lock() else {
        return Vec::new();
    };
    exits
        .iter_mut()
        .filter(|exit| !exit.reported)
        .map(|exit| {
            exit.reported = true;
            format!(
                "+{}ms {}",
                exit.ended_at
                    .saturating_duration_since(startup_started)
                    .as_millis(),
                describe_exit(exit)
            )
        })
        .collect()
}

/// The toast sentence for a gxserver that ended with an error in the last two minutes.
#[cfg(windows)]
pub(crate) fn gpui_windows_gxserver_failure_toast_detail() -> Option<String> {
    let exits = EXITS.lock().ok()?;
    let exit = exits.back()?;
    if exit.ended_at.elapsed() > TOAST_EXIT_MAX_AGE || exit.code == Some(0) {
        return None;
    }
    let ran_for = format_seconds(exit.ran_for);
    Some(match exit.code {
        Some(code) => format!(
            "gxserver stopped {ran_for} after it started ({}{}). Ghostex keeps trying; copy diagnostics to share the failure details.",
            format_exit_code(code),
            exit_code_meaning(code)
                .map(|meaning| format!(", {meaning}"))
                .unwrap_or_default(),
        ),
        None => format!(
            "gxserver stopped {ran_for} after it started. Ghostex keeps trying; copy diagnostics to share the failure details."
        ),
    })
}

/// The crash gxserver recorded and its latest warnings and errors, for the startup report.
#[cfg(windows)]
pub(crate) fn gpui_windows_gxserver_crash_evidence_report() -> String {
    let logs_dir = &shared_settings::ghostex_storage_paths().logs_dir;
    let crash_path = logs_dir.join("gxserver-crash.json");
    let crash = match fs::read_to_string(&crash_path) {
        Ok(text) if !text.trim().is_empty() => text.trim().to_string(),
        Ok(_) => "none (the running or last gxserver has not crashed)".to_string(),
        Err(error) => format!("none ({error})"),
    };
    let log_path = logs_dir.join("gxserver.jsonl");
    let lines = recent_warning_and_error_lines(&log_path);
    let lines = if lines.is_empty() {
        "none".to_string()
    } else {
        lines.join("\n")
    };
    let mut report = format!(
        "\n\ngxserver crash record (gxserver-crash.json): {crash}\nLatest gxserver warnings and errors (gxserver.jsonl):\n{lines}"
    );
    if let Ok(profile) = std::env::var("USERPROFILE")
        && !profile.is_empty()
    {
        report = report.replace(&profile, "%USERPROFILE%");
    }
    report
}

#[cfg(windows)]
fn recent_warning_and_error_lines(log_path: &Path) -> Vec<String> {
    let Ok(mut file) = fs::File::open(log_path) else {
        return Vec::new();
    };
    let length = file.metadata().map(|metadata| metadata.len()).unwrap_or(0);
    let offset = length.saturating_sub(LOG_TAIL_BYTES);
    if file.seek(SeekFrom::Start(offset)).is_err() {
        return Vec::new();
    }
    let mut bytes = Vec::new();
    if file.read_to_end(&mut bytes).is_err() {
        return Vec::new();
    }
    let text = String::from_utf8_lossy(&bytes);
    let mut lines: Vec<String> = text
        .lines()
        // A tail that starts mid-file starts mid-line.
        .skip(usize::from(offset > 0))
        .filter(|line| {
            serde_json::from_str::<serde_json::Value>(line).is_ok_and(|entry| {
                matches!(
                    entry.get("level").and_then(serde_json::Value::as_str),
                    Some("warn" | "error")
                )
            })
        })
        .map(str::to_string)
        .collect();
    let start = lines.len().saturating_sub(REPORTED_LOG_LINES);
    lines.drain(..start);
    lines
}

fn append_launch_log_line(launch_log: &Path, exit: &GpuiGxserverExit) {
    let unix_seconds = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|duration| duration.as_secs())
        .unwrap_or_default();
    let Ok(mut file) = fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(launch_log)
    else {
        return;
    };
    let _ = writeln!(file, "[unix {unix_seconds}] {}", describe_exit(exit));
}

pub(crate) fn describe_exit(exit: &GpuiGxserverExit) -> String {
    let ran_for = format_seconds(exit.ran_for);
    match (exit.code, exit.signal) {
        (Some(code), _) => format!(
            "gxserver pid {} exited {ran_for} after Ghostex started it with {}{}",
            exit.pid,
            format_exit_code(code),
            exit_code_meaning(code)
                .map(|meaning| format!(": {meaning}"))
                .unwrap_or_default(),
        ),
        (None, Some(signal)) => format!(
            "gxserver pid {} was ended by signal {signal}{} {ran_for} after Ghostex started it",
            exit.pid,
            signal_meaning(signal)
                .map(|meaning| format!(" ({meaning})"))
                .unwrap_or_default(),
        ),
        (None, None) => format!(
            "gxserver pid {} exited {ran_for} after Ghostex started it; the system did not report its exit code",
            exit.pid
        ),
    }
}

/// What the signals a gxserver is likely to end on mean.
fn signal_meaning(signal: i32) -> Option<&'static str> {
    Some(match signal {
        4 => "an illegal instruction, gxserver crashed",
        6 => "it aborted",
        7 | 11 => "gxserver crashed",
        9 => "it was killed",
        15 => "it was asked to stop",
        _ => return None,
    })
}

pub(crate) fn format_seconds(duration: Duration) -> String {
    format!("{:.1} s", duration.as_secs_f64())
}

/// NTSTATUS-sized codes read best in hex, ordinary exit codes in decimal.
fn format_exit_code(code: u32) -> String {
    if code > 0xFFFF {
        format!("exit code {code:#010X}")
    } else {
        format!("exit code {code}")
    }
}

fn exit_code_meaning(code: u32) -> Option<&'static str> {
    #[cfg(not(windows))]
    return match code {
        0 => Some("it stopped normally"),
        1 => Some("it stopped on an error it printed"),
        _ => None,
    };
    #[cfg(windows)]
    Some(match code {
        0 => "it stopped normally",
        1 => "it stopped on an error it printed above",
        0xC000_0005 => "an access violation (gxserver crashed)",
        0xC000_0006 => "an in-page error reading a file it had mapped (gxserver crashed)",
        0xC000_001D => "an illegal instruction (gxserver crashed)",
        0xC000_00FD => "a stack overflow (gxserver crashed)",
        0xC000_0374 => "heap corruption (gxserver crashed)",
        0xC000_0409 => "a fail-fast abort (gxserver crashed or aborted)",
        0xC000_0022 => "access denied while it started (often an antivirus program)",
        0xC000_0135 => "a DLL it needs was not found",
        0xC000_0142 => "it could not initialise (often while Windows shuts down)",
        0xC000_013A => "it was ended by Ctrl+C or its console closing",
        _ => return None,
    })
}
