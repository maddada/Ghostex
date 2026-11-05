//! Whether a Claude Code session record (`<config>/sessions/<pid>.json`) still describes a running
//! `claude`, or only names a pid that a later process now holds.

use serde_json::Value;

/// How much later than the record's `startedAt` a process may have started and still be the one
/// that wrote it. Claude writes `startedAt` a second or two after its process starts, and the
/// kernel start time is read in whole seconds, so a genuine record never misses by more.
const RECORD_START_SLACK_MS: i64 = 2_000;

/*
CDXC:SessionIdentity 2026-11-05 WHY:
Claude leaves its session record behind when its process is killed or crashes, and macOS hands the
dead pid to a later process. The hook-free observer read every record in `~/.claude` and every
cswap profile and called one live as soon as `kill(pid, 0)` succeeded, so account 3's record
`76463.json` (conversation 4f56cc17, a maestro_swift checkout, left over from 2026-10-01) came
back to life when the shell that runs thread G1y7g's own Claude got pid 76463 on 2026-11-05.
That shell hangs directly below the thread's zmx pane, so the observer showed the pane two
conversations, flipped between them on every pass, and stored 4f56cc17 as the thread's agent
session id: the coordinator got "its final message could not be read" and `read-session-chat`
read the September transcript. A record now counts as live only while its pid runs a process
that started no later than the record says its own process did (`startedAt`, or the UTC `ps
lstart` text Claude writes as `procStart` on macOS). Windows keeps its exact FILETIME check.
SEE-ALSO: server/src/agent_hooks/wmx_pane.rs `claude_record_process_running` (Windows),
server/src/hookless_activity/claude.rs (the observer), server/src/session_chat_watchdog/claude_liveness.rs.
*/
pub(crate) fn claude_session_record_live(record: &Value) -> bool {
    let Some(pid) = record["pid"]
        .as_u64()
        .and_then(|pid| u32::try_from(pid).ok())
    else {
        return false;
    };
    #[cfg(windows)]
    {
        super::wmx_pane::claude_record_process_running(pid, record["procStart"].as_str())
    }
    #[cfg(unix)]
    {
        if pid <= 1 || unsafe { libc::kill(pid as libc::pid_t, 0) } != 0 {
            return false;
        }
        let process_start_ms =
            crate::session_chat_fleet_process::process_started_at(i64::from(pid)).ok();
        record_matches_process_start(
            process_start_ms,
            record["startedAt"].as_i64(),
            record["procStart"].as_str(),
        )
    }
    #[cfg(not(any(unix, windows)))]
    {
        let _ = pid;
        false
    }
}

/// Whether a process that started at `process_start_ms` (ms since the epoch, whole seconds) can be
/// the one a record describes. `startedAt` is the epoch time Claude wrote the record at, so a
/// process that started after it holds a reused pid. Without `startedAt` the UTC `procStart` text
/// must name the same second. A record that carries neither, or a process whose start cannot be
/// read, keeps the plain pid rule.
pub(crate) fn record_matches_process_start(
    process_start_ms: Option<i64>,
    started_at_ms: Option<i64>,
    proc_start: Option<&str>,
) -> bool {
    let Some(process_start_ms) = process_start_ms else {
        return true;
    };
    if let Some(started_at_ms) = started_at_ms.filter(|value| *value > 0) {
        return process_start_ms <= started_at_ms + RECORD_START_SLACK_MS;
    }
    match proc_start.and_then(parse_utc_lstart) {
        Some(record_start_ms) => (record_start_ms - process_start_ms).abs() <= 1_000,
        None => true,
    }
}

/// `Thu Nov  5 14:27:39 2026`, as `LC_ALL=C TZ=UTC ps -o lstart=` prints it.
fn parse_utc_lstart(text: &str) -> Option<i64> {
    chrono::NaiveDateTime::parse_from_str(text.trim(), "%a %b %e %H:%M:%S %Y")
        .ok()
        .map(|time| time.and_utc().timestamp_millis())
}

#[cfg(test)]
mod tests {
    use super::*;

    // 2026-11-05: G1y7g's pane shell got pid 76463 around 12:18 UTC, and account 3's
    // `76463.json` was written by a Claude that started on 2026-10-01.
    const PANE_SHELL_START_MS: i64 = 1_793_881_104_000;
    const STALE_STARTED_AT_MS: i64 = 1_790_829_608_184;
    const STALE_PROC_START: &str = "Thu Oct  1 04:40:07 2026";

    #[test]
    fn a_record_older_than_the_process_holding_its_pid_is_not_live() {
        assert!(!record_matches_process_start(
            Some(PANE_SHELL_START_MS),
            Some(STALE_STARTED_AT_MS),
            Some(STALE_PROC_START),
        ));
        assert!(!record_matches_process_start(
            Some(PANE_SHELL_START_MS),
            None,
            Some(STALE_PROC_START),
        ));
    }

    #[test]
    fn the_record_of_the_running_claude_is_live() {
        // A live record from the same day: the process started at 14:27:39 UTC and Claude wrote
        // `startedAt` 1.9 seconds later.
        let process_start_ms = 1_793_888_859_000;
        assert!(record_matches_process_start(
            Some(process_start_ms),
            Some(1_793_888_860_922),
            Some("Thu Nov  5 14:27:39 2026"),
        ));
        assert!(record_matches_process_start(
            Some(process_start_ms),
            None,
            Some("Thu Nov  5 14:27:39 2026"),
        ));
        // `/clear` and `/resume` run in the same process; a later `startedAt` still fits it.
        assert!(record_matches_process_start(
            Some(process_start_ms),
            Some(process_start_ms + 3_600_000),
            None,
        ));
    }

    #[test]
    fn an_unreadable_start_keeps_the_pid_rule() {
        assert!(record_matches_process_start(
            None,
            Some(STALE_STARTED_AT_MS),
            Some(STALE_PROC_START),
        ));
        assert!(record_matches_process_start(
            Some(PANE_SHELL_START_MS),
            None,
            Some("not a date"),
        ));
        assert!(record_matches_process_start(
            Some(PANE_SHELL_START_MS),
            None,
            None
        ));
    }

    #[test]
    fn a_record_without_a_pid_is_not_live() {
        assert!(!claude_session_record_live(
            &serde_json::json!({"sessionId": "x"})
        ));
    }
}
