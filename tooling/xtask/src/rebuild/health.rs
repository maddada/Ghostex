//! The post-start health check `cargo xtask rebuild` runs about a minute after a successful start.
//!
//! CDXC:Build 2026-11-05 WHY:
//! Rebuild 111 shipped a gxserver that ran out of file descriptors (launchd's soft limit of 256 at the time) within minutes, and nobody noticed until agents' messages failed. A minute after the start the check asks gxserver's health endpoint, counts its open descriptors twice 15 s apart (a leak shows as growth long before the limit), and reads the gxserver log lines written since the start for "os error 24" or a gxserverPanic. The limit is the one gxserver reports in `openFiles` (it raises its own at start), launchctl's only for an older gxserver. Any of these makes the rebuild exit non-zero; `--no-health-check` skips it.

use std::fs;
use std::io::{Read, Seek, SeekFrom};
use std::path::PathBuf;
#[cfg(target_os = "macos")]
use std::process::Command;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use crate::gxserver;
#[cfg(target_os = "macos")]
use crate::util::stdout_if_ok;

/// launchd's default soft limit, used when `launchctl limit maxfiles` can't be read.
#[cfg(target_os = "macos")]
const DEFAULT_FD_LIMIT: u64 = 256;
/// The usual Linux soft limit, used when /proc/<pid>/limits can't be read.
#[cfg(not(target_os = "macos"))]
const DEFAULT_FD_LIMIT: u64 = 1024;
#[cfg(target_os = "macos")]
const OS_LIMIT_SOURCE: (&str, &str) = (
    "launchctl (gxserver didn't report one)",
    "launchd's default (gxserver didn't report one)",
);
#[cfg(not(target_os = "macos"))]
const OS_LIMIT_SOURCE: (&str, &str) = (
    "/proc/<pid>/limits (gxserver didn't report one)",
    "the usual Linux default (gxserver didn't report one)",
);
pub const SETTLE: Duration = Duration::from_secs(60);
pub const SAMPLE_GAP: Duration = Duration::from_secs(15);

/// One log file as it was before the start.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct FileMark {
    pub size: u64,
    pub inode: u64,
}

/// Where the log scan starts: when the start began and each gxserver log's size and inode then.
pub struct LogMarks {
    began: SystemTime,
    files: Vec<(PathBuf, Option<FileMark>)>,
}

impl LogMarks {
    pub fn now() -> Self {
        Self {
            began: SystemTime::now(),
            files: log_files()
                .into_iter()
                .map(|path| {
                    let mark = fs::metadata(&path).ok().map(|meta| file_mark(&meta));
                    (path, mark)
                })
                .collect(),
        }
    }

    /// The lines written since the start began.
    fn new_lines(&self) -> String {
        let began_iso = iso_utc(self.began);
        let mut text = String::new();
        for (path, mark) in &self.files {
            let Ok(mut file) = fs::File::open(path) else {
                continue;
            };
            let Ok(meta) = file.metadata() else {
                continue;
            };
            let modified = meta.modified().unwrap_or(UNIX_EPOCH);
            let Some(from) = read_from(*mark, file_mark(&meta), modified, self.began) else {
                continue;
            };
            let mut bytes = Vec::new();
            if file.seek(SeekFrom::Start(from)).is_err() || file.read_to_end(&mut bytes).is_err() {
                continue;
            }
            for line in String::from_utf8_lossy(&bytes).lines() {
                if written_since(line, &began_iso) {
                    text.push_str(line);
                    text.push('\n');
                }
            }
        }
        text
    }
}

#[cfg(unix)]
fn file_mark(meta: &fs::Metadata) -> FileMark {
    use std::os::unix::fs::MetadataExt;
    FileMark {
        size: meta.len(),
        inode: meta.ino(),
    }
}

#[cfg(not(unix))]
fn file_mark(meta: &fs::Metadata) -> FileMark {
    FileMark {
        size: meta.len(),
        inode: 0,
    }
}

/// CDXC:Build 2026-11-05 WHY:
/// The first real check flagged old "os error 24" lines and an old panic: a minute after it starts, gxserver trims gxserver.jsonl in place to its newest 25,000 lines (`prune_gxserver_log_lines`), so the file shrinks while keeping lines from before the start, and "smaller means rotated, read all of it" re-read them. A file untouched since the start began is skipped; the same file grown past its mark is read from the mark; a file that shrank, was replaced (new inode) or is new is read whole, and its JSON lines then count only when their `ts` is after the start began (`written_since`).
pub fn read_from(
    before: Option<FileMark>,
    now: FileMark,
    modified: SystemTime,
    began: SystemTime,
) -> Option<u64> {
    if modified < began {
        return None;
    }
    match before {
        Some(before) if before.inode == now.inode && now.size >= before.size => Some(before.size),
        _ => Some(0),
    }
}

/// Whether a log line was written at or after `began_iso`: JSON lines by their `ts`, plain lines (launch.log) always, since the file itself was written after the start.
pub fn written_since(line: &str, began_iso: &str) -> bool {
    if !line.trim_start().starts_with('{') {
        return true;
    }
    match serde_json::from_str::<serde_json::Value>(line)
        .ok()
        .and_then(|entry| entry.get("ts")?.as_str().map(str::to_string))
    {
        // Both are `YYYY-MM-DDTHH:MM:SS.mmmZ`, so they compare as text.
        Some(ts) => ts.as_str() >= began_iso,
        None => true,
    }
}

/// `time` as `YYYY-MM-DDTHH:MM:SS.mmmZ`, the format gxserver's `ts` uses.
pub fn iso_utc(time: SystemTime) -> String {
    let since = time.duration_since(UNIX_EPOCH).unwrap_or_default();
    let secs = since.as_secs();
    let (days, rest) = ((secs / 86_400) as i64, secs % 86_400);
    // Howard Hinnant's civil_from_days.
    let z = days + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z - era * 146_097;
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let day = doy - (153 * mp + 2) / 5 + 1;
    let month = if mp < 10 { mp + 3 } else { mp - 9 };
    let year = yoe + era * 400 + i64::from(month <= 2);
    format!(
        "{year:04}-{month:02}-{day:02}T{:02}:{:02}:{:02}.{:03}Z",
        rest / 3600,
        rest % 3600 / 60,
        rest % 60,
        since.subsec_millis()
    )
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
        || os_fd_limit(pid),
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

/// gxserver's own soft limit (it raises it at start, CDXC:ServerDaemon 2026-11-05), else the OS's (launchctl's, or /proc/<pid>/limits on Linux), else that OS's default.
pub fn fd_limit(
    reported: Option<u64>,
    launchctl: impl FnOnce() -> Option<u64>,
) -> (u64, &'static str) {
    if let Some(limit) = reported.filter(|limit| *limit > 0) {
        return (limit, "gxserver");
    }
    match launchctl() {
        Some(limit) => (limit, OS_LIMIT_SOURCE.0),
        None => (DEFAULT_FD_LIMIT, OS_LIMIT_SOURCE.1),
    }
}

/// The soft limit from `launchctl limit maxfiles` ("\tmaxfiles    256            unlimited").
#[cfg(target_os = "macos")]
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

/// Open descriptors of `pid`, from /proc/<pid>/fd.
#[cfg(not(target_os = "macos"))]
pub fn open_descriptors(pid: u64) -> Option<u64> {
    Some(fs::read_dir(format!("/proc/{pid}/fd")).ok()?.count() as u64)
}

/// The soft descriptor limit launchd gives processes.
#[cfg(target_os = "macos")]
fn os_fd_limit(_pid: Option<u64>) -> Option<u64> {
    stdout_if_ok(Command::new("launchctl").args(["limit", "maxfiles"]))
        .and_then(|text| parse_maxfiles_soft(&text))
}

/// gxserver's soft descriptor limit from /proc/<pid>/limits.
#[cfg(not(target_os = "macos"))]
fn os_fd_limit(pid: Option<u64>) -> Option<u64> {
    parse_proc_limits_soft(&fs::read_to_string(format!("/proc/{}/limits", pid?)).ok()?)
}

/// The soft limit from /proc/<pid>/limits ("Max open files            1024                 524288               files").
#[cfg(not(target_os = "macos"))]
fn parse_proc_limits_soft(text: &str) -> Option<u64> {
    text.lines()
        .find_map(|line| line.strip_prefix("Max open files"))?
        .split_whitespace()
        .next()?
        .parse()
        .ok()
}

#[cfg(test)]
mod tests {
    use super::*;

    const GAP: Duration = Duration::from_secs(15);

    #[test]
    #[cfg(target_os = "macos")]
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
    #[cfg(target_os = "macos")]
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

    fn at(secs: u64) -> SystemTime {
        UNIX_EPOCH + Duration::from_secs(secs)
    }

    #[test]
    fn formats_times_like_gxserver_ts() {
        assert_eq!(iso_utc(at(0)), "1970-01-01T00:00:00.000Z");
        assert_eq!(
            iso_utc(at(1_793_830_408) + Duration::from_millis(5)),
            "2026-11-04T22:13:28.005Z"
        );
    }

    #[test]
    fn reads_only_what_was_written_after_the_start() {
        let began = at(2_000);
        let mark = Some(FileMark {
            size: 1152,
            inode: 7,
        });
        // launch.log untouched since before the start, even when it looks smaller or replaced.
        assert_eq!(
            read_from(
                mark,
                FileMark {
                    size: 1152,
                    inode: 7
                },
                at(1_900),
                began
            ),
            None
        );
        assert_eq!(
            read_from(
                mark,
                FileMark {
                    size: 600,
                    inode: 7
                },
                at(1_900),
                began
            ),
            None
        );
        assert_eq!(
            read_from(
                mark,
                FileMark {
                    size: 600,
                    inode: 9
                },
                at(1_900),
                began
            ),
            None
        );
        // Appended to after the start: from the mark.
        assert_eq!(
            read_from(
                mark,
                FileMark {
                    size: 1500,
                    inode: 7
                },
                at(2_010),
                began
            ),
            Some(1152)
        );
        // Truncated, trimmed in place, replaced, or new since the start: all of it, filtered by `ts`.
        assert_eq!(
            read_from(
                mark,
                FileMark {
                    size: 600,
                    inode: 7
                },
                at(2_010),
                began
            ),
            Some(0)
        );
        assert_eq!(
            read_from(
                mark,
                FileMark {
                    size: 1500,
                    inode: 9
                },
                at(2_010),
                began
            ),
            Some(0)
        );
        assert_eq!(
            read_from(None, FileMark { size: 80, inode: 3 }, at(2_010), began),
            Some(0)
        );
    }

    #[test]
    fn a_trimmed_jsonl_keeps_only_lines_after_the_start() {
        let began = "2026-11-04T22:13:28.000Z";
        let old = r#"{"ts":"2026-11-04T22:00:18.811Z","level":"error","event":"apiRequest","error":"internalError: Too many open files (os error 24)"}"#;
        let old_panic = r#"{"ts":"2026-11-04T21:50:00.000Z","level":"error","event":"gxserverPanic","error":"there is no reactor running"}"#;
        let new = r#"{"ts":"2026-11-04T22:14:02.100Z","level":"error","event":"apiRequest","error":"os error 24"}"#;
        assert!(!written_since(old, began));
        assert!(!written_since(old_panic, began));
        assert!(written_since(new, began));
        assert!(written_since(
            "session chat send diagnostic could not be written: Too many open files (os error 24)",
            began
        ));
        let kept: Vec<&str> = [old, old_panic, new]
            .into_iter()
            .filter(|line| written_since(line, began))
            .collect();
        assert_eq!(
            scan_log(&kept.join("\n")),
            vec!["1 log line(s) with \"os error 24\" (out of file descriptors)".to_string()]
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
