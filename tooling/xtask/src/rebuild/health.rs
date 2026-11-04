//! The post-start health check `cargo xtask rebuild` runs about a minute after a successful start.
//!
//! CDXC:Build 2026-11-05 WHY:
//! Rebuild 111 shipped a gxserver that ran out of file descriptors (launchd's soft limit of 256 at the time) within minutes, and nobody noticed until agents' messages failed. A minute after the start the check asks gxserver's health endpoint, counts its open descriptors twice 15 s apart (a leak shows as growth long before the limit), and reads the gxserver log lines written since the start for "os error 24" or a gxserverPanic. The limit is the one gxserver reports in `openFiles` (it raises its own at start), launchctl's only for an older gxserver. Any of these makes the rebuild exit non-zero; `--no-health-check` skips it.

use std::fs;
use std::io::{Read, Seek, SeekFrom};
use std::path::PathBuf;
use std::process::Command;
use std::time::Duration;

use crate::gxserver;
use crate::util::stdout_if_ok;

/// launchd's default soft limit, used when `launchctl limit maxfiles` can't be read.
const DEFAULT_FD_LIMIT: u64 = 256;
pub const SETTLE: Duration = Duration::from_secs(60);
pub const SAMPLE_GAP: Duration = Duration::from_secs(15);

/// Where the log scan starts: each gxserver log's size before the start.
pub struct LogMarks(Vec<(PathBuf, u64)>);

impl LogMarks {
    pub fn now() -> Self {
        Self(
            log_files()
                .into_iter()
                .map(|path| {
                    let size = fs::metadata(&path).map_or(0, |meta| meta.len());
                    (path, size)
                })
                .collect(),
        )
    }

    /// The lines appended since the marks; a file that shrank was rotated, so all of it is new.
    fn new_lines(&self) -> String {
        let mut text = String::new();
        for (path, mark) in &self.0 {
            let Ok(mut file) = fs::File::open(path) else {
                continue;
            };
            let size = file.metadata().map_or(0, |meta| meta.len());
            let from = if size < *mark { 0 } else { *mark };
            if file.seek(SeekFrom::Start(from)).is_ok() {
                let mut bytes = Vec::new();
                let _ = file.read_to_end(&mut bytes);
                text.push_str(&String::from_utf8_lossy(&bytes));
                text.push('\n');
            }
        }
        text
    }
}

fn log_files() -> Vec<PathBuf> {
    let logs = gxserver::ghostex_state_dir().join("logs");
    vec![
        logs.join("gxserver.jsonl"),
        logs.join("gxserver").join("launch.log"),
    ]
}

/// What the check found; `problems` empty means healthy.
#[derive(Debug, PartialEq)]
pub struct Verdict {
    pub summary: String,
    pub problems: Vec<String>,
}

impl Verdict {
    pub fn healthy(&self) -> bool {
        self.problems.is_empty()
    }

    pub fn line(&self) -> String {
        if self.healthy() {
            format!("healthy ({})", self.summary)
        } else {
            format!("UNHEALTHY: {} ({})", self.problems.join("; "), self.summary)
        }
    }
}

/// Runs the check against the gxserver at `pid` (None when it isn't answering).
pub fn check(pid: Option<u64>, marks: &LogMarks, settle: Duration, gap: Duration) -> Verdict {
    std::thread::sleep(settle);
    let answering = super::read_gxserver();
    let restarted = match (pid, &answering) {
        (Some(before), Some(run)) if run.pid != before => Some(format!(
            "gxserver restarted during the check (pid {before} -> {})",
            run.pid
        )),
        _ => None,
    };
    let pid = answering.as_ref().map(|run| run.pid).or(pid);
    // Counted from outside so an older gxserver is measured too; gxserver's own count when that fails.
    let sample = || {
        pid.and_then(open_descriptors)
            .or_else(|| super::read_gxserver().and_then(|run| reported_open_files(&run.health).1))
    };
    let first = sample();
    std::thread::sleep(gap);
    let second = sample();
    let (limit, source) = fd_limit(
        answering
            .as_ref()
            .and_then(|run| reported_open_files(&run.health).0),
        || {
            stdout_if_ok(Command::new("launchctl").args(["limit", "maxfiles"]))
                .and_then(|text| parse_maxfiles_soft(&text))
        },
    );
    let mut verdict = judge(
        answering.is_some(),
        first,
        second,
        limit,
        gap,
        &restarted
            .into_iter()
            .chain(scan_log(&marks.new_lines()))
            .collect::<Vec<_>>(),
    );
    verdict.summary.push_str(&format!(", limit from {source}"));
    verdict
}

/// `openFiles.softLimit` and `openFiles.open` from a health answer; gxserver builds before 2026-11-05 leave them out.
pub fn reported_open_files(health: &serde_json::Value) -> (Option<u64>, Option<u64>) {
    let report = health.get("openFiles");
    let field = |name: &str| report.and_then(|r| r.get(name)).and_then(|v| v.as_u64());
    (field("softLimit"), field("open"))
}

/// gxserver's own soft limit (it raises it at start, CDXC:ServerDaemon 2026-11-05), else launchctl's, else launchd's default.
pub fn fd_limit(
    reported: Option<u64>,
    launchctl: impl FnOnce() -> Option<u64>,
) -> (u64, &'static str) {
    if let Some(limit) = reported.filter(|limit| *limit > 0) {
        return (limit, "gxserver");
    }
    match launchctl() {
        Some(limit) => (limit, "launchctl (gxserver didn't report one)"),
        None => (
            DEFAULT_FD_LIMIT,
            "launchd's default (gxserver didn't report one)",
        ),
    }
}

/// The soft limit from `launchctl limit maxfiles` ("\tmaxfiles    256            unlimited").
pub fn parse_maxfiles_soft(text: &str) -> Option<u64> {
    text.lines().find_map(|line| {
        let mut fields = line.split_whitespace();
        (fields.next()? == "maxfiles").then_some(())?;
        fields.next()?.parse().ok()
    })
}

/// Log lines that mean gxserver is in trouble: descriptor exhaustion or a panic.
pub fn scan_log(text: &str) -> Vec<String> {
    let mut exhausted = 0;
    let mut panics = Vec::new();
    for line in text.lines() {
        if line.contains("os error 24") || line.contains("Too many open files") {
            exhausted += 1;
        }
        if line.contains("\"gxserverPanic\"") {
            let message = serde_json::from_str::<serde_json::Value>(line)
                .ok()
                .and_then(|entry| entry.get("error")?.as_str().map(str::to_string))
                .unwrap_or_default();
            let message: String = message.chars().take(160).collect();
            panics.push(if message.is_empty() {
                "gxserverPanic in the log".to_string()
            } else {
                format!("gxserverPanic in the log: {message}")
            });
        }
    }
    let mut findings = Vec::new();
    if exhausted > 0 {
        findings.push(format!(
            "{exhausted} log line(s) with \"os error 24\" (out of file descriptors)"
        ));
    }
    findings.extend(panics.into_iter().take(3));
    findings
}

/// Healthy unless gxserver isn't answering, restarted, is near its descriptor limit, gains descriptors fast enough to reach it within minutes, or logged descriptor exhaustion or a panic (`findings`).
pub fn judge(
    answering: bool,
    first: Option<u64>,
    second: Option<u64>,
    limit: u64,
    gap: Duration,
    findings: &[String],
) -> Verdict {
    let mut problems = Vec::new();
    if !answering {
        problems.push("gxserver is not answering its health endpoint".to_string());
    }
    let summary = match (first, second) {
        (Some(first), Some(second)) => {
            if second * 5 >= limit * 4 {
                problems.push(format!("{second} of {limit} file descriptors open"));
            } else if second > first && (second - first) * 10 >= limit {
                let per_minute = (second - first) * 60 / gap.as_secs().max(1);
                problems.push(format!(
                    "file descriptors grew {first} -> {second} in {}s (~{per_minute}/min, limit {limit})",
                    gap.as_secs()
                ));
            }
            format!("{first} -> {second} of {limit} file descriptors")
        }
        _ => "file descriptors not readable".to_string(),
    };
    problems.extend(findings.iter().cloned());
    Verdict { summary, problems }
}

/// Open descriptors of `pid`, from proc_pidinfo(PROC_PIDLISTFDS).
#[cfg(target_os = "macos")]
pub fn open_descriptors(pid: u64) -> Option<u64> {
    let pid = libc::c_int::try_from(pid).ok()?;
    let entry = std::mem::size_of::<libc::proc_fdinfo>();
    // With no buffer the call returns the size needed for the current table.
    let needed =
        unsafe { libc::proc_pidinfo(pid, libc::PROC_PIDLISTFDS, 0, std::ptr::null_mut(), 0) };
    if needed <= 0 {
        return None;
    }
    // Room for descriptors opened between the two calls.
    let capacity = needed as usize / entry + 64;
    let mut buffer: Vec<libc::proc_fdinfo> = Vec::with_capacity(capacity);
    let written = unsafe {
        libc::proc_pidinfo(
            pid,
            libc::PROC_PIDLISTFDS,
            0,
            buffer.as_mut_ptr().cast(),
            (capacity * entry) as libc::c_int,
        )
    };
    (written > 0).then(|| written as u64 / entry as u64)
}

#[cfg(not(target_os = "macos"))]
pub fn open_descriptors(_pid: u64) -> Option<u64> {
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    const GAP: Duration = Duration::from_secs(15);

    #[test]
    fn reads_the_soft_maxfiles_limit() {
        assert_eq!(
            parse_maxfiles_soft("\tmaxfiles    256            unlimited\n"),
            Some(256)
        );
        assert_eq!(parse_maxfiles_soft("\tmaxproc 2666 4000\n"), None);
        assert_eq!(parse_maxfiles_soft(""), None);
    }

    #[test]
    fn finds_descriptor_exhaustion_and_panics_in_new_log_lines() {
        let log = concat!(
            r#"{"ts":"t","level":"debug","event":"hooklessActivity.match"}"#,
            "\n",
            r#"{"ts":"t","level":"error","event":"accept","error":"Too many open files (os error 24)"}"#,
            "\n",
            r#"{"ts":"t","level":"error","event":"spawn","error":"os error 24"}"#,
            "\n",
            r#"{"ts":"t","level":"error","event":"gxserverPanic","error":"index out of bounds"}"#,
            "\n",
        );
        assert_eq!(
            scan_log(log),
            vec![
                "2 log line(s) with \"os error 24\" (out of file descriptors)".to_string(),
                "gxserverPanic in the log: index out of bounds".to_string(),
            ]
        );
        assert!(scan_log(r#"{"event":"hooklessActivity.drive","error":"os error 2"}"#).is_empty());
    }

    #[test]
    fn uses_the_limit_gxserver_reports() {
        let health = serde_json::json!({"product": "gxserver", "openFiles": {"softLimit": 8192, "open": 310}});
        assert_eq!(reported_open_files(&health), (Some(8192), Some(310)));
        let (limit, source) = fd_limit(reported_open_files(&health).0, || Some(256));
        assert_eq!((limit, source), (8192, "gxserver"));
        // 310 descriptors would be over 80% of launchctl's 256, but is far from gxserver's own limit.
        assert!(judge(true, Some(300), Some(310), limit, GAP, &[]).healthy());
        assert!(!judge(true, Some(300), Some(310), 256, GAP, &[]).healthy());
        // A leak is still judged against the real limit.
        assert!(!judge(true, Some(300), Some(1200), limit, GAP, &[]).healthy());
    }

    #[test]
    fn falls_back_to_launchctl_for_an_older_gxserver() {
        let health = serde_json::json!({"product": "gxserver", "pid": 84393});
        assert_eq!(reported_open_files(&health), (None, None));
        assert_eq!(
            fd_limit(reported_open_files(&health).0, || Some(256)),
            (256, "launchctl (gxserver didn't report one)")
        );
        assert_eq!(
            fd_limit(None, || None),
            (256, "launchd's default (gxserver didn't report one)")
        );
    }

    #[test]
    fn steady_descriptors_are_healthy() {
        let verdict = judge(true, Some(82), Some(84), 256, GAP, &[]);
        assert!(verdict.healthy());
        assert_eq!(verdict.line(), "healthy (82 -> 84 of 256 file descriptors)");
    }

    #[test]
    fn near_the_limit_or_leaking_is_unhealthy() {
        assert_eq!(
            judge(true, Some(200), Some(210), 256, GAP, &[]).problems,
            vec!["210 of 256 file descriptors open".to_string()]
        );
        assert_eq!(
            judge(true, Some(90), Some(130), 256, GAP, &[]).problems,
            vec!["file descriptors grew 90 -> 130 in 15s (~160/min, limit 256)".to_string()]
        );
    }

    #[test]
    fn not_answering_or_log_findings_are_unhealthy() {
        let verdict = judge(
            false,
            None,
            None,
            256,
            GAP,
            &["gxserverPanic in the log".into()],
        );
        assert_eq!(
            verdict.line(),
            "UNHEALTHY: gxserver is not answering its health endpoint; gxserverPanic in the log (file descriptors not readable)"
        );
    }
}
