//! Why this computer's gxserver could not start, in plain words, for the empty sidebar and the
//! failure toast (Linux and macOS; Windows has its own wording in gxserver_exit.rs).
//!
//! CDXC:ServerDaemon 2026-10-28 WHY:
//! A gxserver that quit right after starting (its port taken by another program, an unwritable
//! data folder) left "Loading sessions…" on screen for the whole three-minute start patience, then
//! a generic "did not become healthy in time", with the real reason only in the launch log. The
//! bootstrap now asks here after every probe: once the gxserver it started last is known to have
//! stopped with an error (Linux holds the process, gxserver_exit.rs; macOS asks launchd), it shows
//! what that gxserver printed, and for a taken port which program holds it, at once.
//! SEE-ALSO: `start_gpui_local_gxserver_bootstrap` in app/os_integration/gxserver_bootstrap.rs,
//! `SERVICE_START_FAILED_TITLE` in packages/gx-core/src/sidebar_view/daemon_wait.rs.

use std::{
    fs,
    io::{Read, Seek, SeekFrom},
    path::Path,
    sync::Mutex,
    time::Instant,
};

#[cfg(target_os = "macos")]
use std::time::Duration;

use super::gpui_gxserver_launch_log_path;

/// The launch log bytes read after a start, at most: its last lines are what matters.
const LAUNCH_OUTPUT_TAIL_BYTES: u64 = 16 * 1024;
/// Lines of its output the reason quotes.
const QUOTED_LINES: usize = 3;
/// Characters of quoted output, at most.
const QUOTED_CHARS: usize = 400;

/// When the app last started gxserver, and the launch log's length then.
static LAST_LAUNCH: Mutex<Option<(Instant, u64)>> = Mutex::new(None);

/// Records a gxserver start: what it prints from now on starts at `log_offset` in the launch log.
pub(crate) fn gpui_note_gxserver_launch(log_offset: u64) {
    if let Ok(mut last) = LAST_LAUNCH.lock() {
        *last = Some((Instant::now(), log_offset));
    }
}

/// A gxserver start that is known to have failed.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct GpuiGxserverStartFailure {
    /// How it ended, for the startup report's timeline.
    pub(crate) exit_note: String,
    /// What the user reads: why, and what to do about it.
    pub(crate) reason: String,
}

/// The failure of the gxserver started last, once it has stopped with an error; `None` while it is
/// still starting, was never started by this app, or stopped normally.
pub(crate) fn gpui_gxserver_start_failure() -> Option<GpuiGxserverStartFailure> {
    let (launched_at, log_offset) = (*LAST_LAUNCH.lock().ok()?)?;
    #[cfg(target_os = "linux")]
    let (exit_note, log_offset) = {
        let exit = super::gpui_failed_gxserver_exit_since(launched_at)?;
        (super::describe_exit(&exit), exit.log_offset.max(log_offset))
    };
    #[cfg(target_os = "macos")]
    let exit_note = {
        // launchd reports the job's last run; give a fresh kickstart a moment to be that run.
        if launched_at.elapsed() < Duration::from_secs(1) {
            return None;
        }
        launchd_failed_exit_note()?
    };
    #[cfg(not(any(target_os = "linux", target_os = "macos")))]
    let exit_note: String = {
        let _ = launched_at;
        return None;
    };
    let output = launch_output_since(&gpui_gxserver_launch_log_path(), log_offset);
    let reason = plain_reason(&output, &exit_note, port_holder);
    Some(GpuiGxserverStartFailure { exit_note, reason })
}

/// What gxserver printed since `offset`, the end of it at most.
fn launch_output_since(log: &Path, offset: u64) -> String {
    let Ok(mut file) = fs::File::open(log) else {
        return String::new();
    };
    let length = file.metadata().map_or(0, |metadata| metadata.len());
    // A log shorter than the offset was replaced since; read its end.
    let from = if offset <= length { offset } else { 0 };
    let from = from.max(length.saturating_sub(LAUNCH_OUTPUT_TAIL_BYTES));
    if file.seek(SeekFrom::Start(from)).is_err() {
        return String::new();
    }
    let mut bytes = Vec::new();
    let _ = file.read_to_end(&mut bytes);
    String::from_utf8_lossy(&bytes).into_owned()
}

/// The reason in plain words: a taken port names its holder; anything else quotes the last lines
/// gxserver printed, or says how it ended when it printed nothing.
fn plain_reason(
    output: &str,
    exit_note: &str,
    holder_of: impl Fn(u16) -> Option<String>,
) -> String {
    let lines = last_lines(output, QUOTED_LINES);
    if let Some(port) = port_in_use(&lines) {
        let holder = holder_of(port).unwrap_or_else(|| "another program".to_string());
        return format!(
            "Port {port} is already in use by {holder}, so it can\u{2019}t listen there. Quit that program, then click Try again."
        );
    }
    if lines.is_empty() {
        return format!("It stopped right after starting and printed nothing ({exit_note}).");
    }
    let mut quoted = lines.join(" ");
    if quoted.chars().count() > QUOTED_CHARS {
        quoted = quoted.chars().take(QUOTED_CHARS).collect::<String>() + "\u{2026}";
    }
    format!("It stopped right after starting: {quoted}")
}

/// The last non-empty lines gxserver printed, oldest first, without terminal colour codes. The note
/// the app itself appends when gxserver exits (`[unix …] gxserver pid … exited …`) is left out.
fn last_lines(output: &str, count: usize) -> Vec<String> {
    let mut lines: Vec<String> = output
        .lines()
        .map(strip_ansi)
        .map(|line| line.trim().to_string())
        .filter(|line| !line.is_empty() && !is_app_exit_note(line))
        .collect();
    let start = lines.len().saturating_sub(count);
    lines.drain(..start);
    lines
}

fn is_app_exit_note(line: &str) -> bool {
    line.strip_prefix("[unix ")
        .and_then(|rest| rest.split_once("] "))
        .is_some_and(|(seconds, note)| {
            seconds.bytes().all(|byte| byte.is_ascii_digit()) && note.starts_with("gxserver pid ")
        })
}

fn strip_ansi(line: &str) -> String {
    let mut out = String::with_capacity(line.len());
    let mut chars = line.chars().peekable();
    while let Some(c) = chars.next() {
        if c == '\u{1b}' {
            if chars.peek() == Some(&'[') {
                chars.next();
                // CSI: parameters and intermediates up to a final byte in @..~.
                for next in chars.by_ref() {
                    if ('@'..='~').contains(&next) {
                        break;
                    }
                }
            }
            continue;
        }
        out.push(c);
    }
    out
}

/// The port of gxserver's "Port N is already in use" line, when one of `lines` says it.
fn port_in_use(lines: &[String]) -> Option<u16> {
    lines.iter().rev().find_map(|line| {
        let lower = line.to_ascii_lowercase();
        if !lower.contains("already in use") && !lower.contains("address in use") {
            return None;
        }
        let after = &line[lower.find("port ")? + "port ".len()..];
        after
            .split(|c: char| !c.is_ascii_digit())
            .next()?
            .parse()
            .ok()
    })
}

/// The program listening on `port` on this computer, as "name (process N)", when it is cheap to
/// find: Linux reads /proc (only the user's own processes are visible), macOS asks lsof.
fn port_holder(port: u16) -> Option<String> {
    #[cfg(target_os = "linux")]
    {
        let inodes: Vec<u64> = ["/proc/net/tcp", "/proc/net/tcp6"]
            .iter()
            .filter_map(|table| fs::read_to_string(table).ok())
            .flat_map(|table| listening_socket_inodes(&table, port))
            .collect();
        if inodes.is_empty() {
            return None;
        }
        let (pid, name) = socket_owner(&inodes)?;
        Some(format!("{name} (process {pid})"))
    }
    #[cfg(target_os = "macos")]
    {
        let output = crate::app::helpers::gpui_run_command_with_captured_output_timeout(
            Path::new("/usr/sbin/lsof"),
            &["-nP", &format!("-iTCP:{port}"), "-sTCP:LISTEN", "-Fpc"],
            Duration::from_secs(2),
            4096,
        )
        .ok()?;
        let (pid, name) = parse_lsof_listener(&output.stdout)?;
        Some(format!("{name} (process {pid})"))
    }
    #[cfg(not(any(target_os = "linux", target_os = "macos")))]
    {
        let _ = port;
        None
    }
}

/// The socket inodes listening on `port` in a /proc/net/tcp or tcp6 table.
#[cfg_attr(not(target_os = "linux"), allow(dead_code))]
fn listening_socket_inodes(table: &str, port: u16) -> Vec<u64> {
    const LISTEN: &str = "0A";
    table
        .lines()
        .skip(1)
        .filter_map(|line| {
            let fields: Vec<&str> = line.split_whitespace().collect();
            let local = fields.get(1)?;
            if *fields.get(3)? != LISTEN {
                return None;
            }
            let local_port = u16::from_str_radix(local.rsplit(':').next()?, 16).ok()?;
            if local_port != port {
                return None;
            }
            fields.get(9)?.parse().ok()
        })
        .collect()
}

/// The process holding one of these socket inodes, and its name.
#[cfg(target_os = "linux")]
fn socket_owner(inodes: &[u64]) -> Option<(u32, String)> {
    let targets: Vec<String> = inodes
        .iter()
        .map(|inode| format!("socket:[{inode}]"))
        .collect();
    for entry in fs::read_dir("/proc").ok()?.flatten() {
        let Some(pid) = entry
            .file_name()
            .to_str()
            .and_then(|name| name.parse::<u32>().ok())
        else {
            continue;
        };
        let Ok(fds) = fs::read_dir(entry.path().join("fd")) else {
            continue;
        };
        let holds = fds.flatten().any(|fd| {
            fs::read_link(fd.path())
                .ok()
                .and_then(|target| target.to_str().map(str::to_owned))
                .is_some_and(|target| targets.contains(&target))
        });
        if holds {
            let name = fs::read_to_string(entry.path().join("comm"))
                .map(|name| name.trim().to_string())
                .unwrap_or_else(|_| "a program".to_string());
            return Some((pid, name));
        }
    }
    None
}

/// The first listener in `lsof -Fpc` output: a `p<pid>` line and its `c<command>` line.
#[cfg_attr(not(target_os = "macos"), allow(dead_code))]
fn parse_lsof_listener(output: &str) -> Option<(u32, String)> {
    let mut pid = None;
    for line in output.lines() {
        if let Some(value) = line.strip_prefix('p') {
            pid = value.trim().parse().ok();
        } else if let (Some(name), Some(pid)) = (line.strip_prefix('c'), pid) {
            return Some((pid, name.trim().to_string()));
        }
    }
    None
}

/// launchd's account of the gxserver job's last run, when it is not running and did not stop
/// normally.
#[cfg(target_os = "macos")]
fn launchd_failed_exit_note() -> Option<String> {
    let target = super::gpui_gxserver_launchd_job_target();
    let output = crate::app::helpers::gpui_run_command_with_captured_output_timeout(
        Path::new("/bin/launchctl"),
        &["print", &target],
        Duration::from_secs(2),
        64 * 1024,
    )
    .ok()?;
    let status = parse_launchctl_print(&output.stdout);
    if status.running || matches!(status.last_exit_code, None | Some(0)) {
        return None;
    }
    Some(format!(
        "launchd reports gxserver exited with code {}",
        status.last_exit_code.unwrap_or_default()
    ))
}

#[derive(Debug, Default, PartialEq, Eq)]
#[cfg_attr(not(target_os = "macos"), allow(dead_code))]
struct LaunchdJobStatus {
    running: bool,
    last_exit_code: Option<i64>,
}

/// `launchctl print gui/<uid>/<label>`: its `state = …` and `last exit code = …` lines.
#[cfg_attr(not(target_os = "macos"), allow(dead_code))]
fn parse_launchctl_print(text: &str) -> LaunchdJobStatus {
    let mut status = LaunchdJobStatus::default();
    for line in text.lines().map(str::trim) {
        if let Some(state) = line.strip_prefix("state = ") {
            status.running = state.trim() == "running";
        } else if let Some(code) = line.strip_prefix("last exit code = ") {
            // "(never exited)" stays unknown.
            status.last_exit_code = code.trim().parse().ok();
        }
    }
    status
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_taken_port_names_the_program_holding_it() {
        let output = "\u{1b}[31mPort 58744 is already in use and did not respond as a compatible gxserver. Stop the conflicting process or update Ghostex/gxserver so their protocol versions match.\u{1b}[0m\n";
        let reason = plain_reason(output, "exit code 1", |port| {
            assert_eq!(port, 58744);
            Some("python3 (process 4242)".to_string())
        });
        assert_eq!(
            reason,
            "Port 58744 is already in use by python3 (process 4242), so it can\u{2019}t listen there. Quit that program, then click Try again."
        );
        let unknown = plain_reason(output, "exit code 1", |_| None);
        assert!(unknown.contains("by another program"), "{unknown}");
    }

    #[test]
    fn other_failures_quote_the_last_lines_it_printed() {
        let output = "starting\n\nError: Permission denied (os error 13)\ncould not open /home/u/.local/state/ghostex/gxserver/state.db\n";
        assert_eq!(
            plain_reason(output, "exit code 1", |_| None),
            "It stopped right after starting: starting Error: Permission denied (os error 13) could not open /home/u/.local/state/ghostex/gxserver/state.db"
        );
        // The exit note the app appends to the same log is not something gxserver printed.
        let output = "read gxserver config: Permission denied (os error 13)\n[unix 1793143538] gxserver pid 310083 exited 0.0 s after Ghostex started it with exit code 1: it stopped on an error it printed\n";
        assert_eq!(
            plain_reason(output, "exit code 1", |_| None),
            "It stopped right after starting: read gxserver config: Permission denied (os error 13)"
        );
        assert_eq!(
            plain_reason("", "gxserver pid 9 was ended by signal 11", |_| None),
            "It stopped right after starting and printed nothing (gxserver pid 9 was ended by signal 11)."
        );
    }

    #[test]
    fn the_proc_tcp_table_gives_the_listening_socket() {
        let table = "  sl  local_address rem_address   st tx_queue rx_queue tr tm->when retrnsmt   uid  timeout inode\n   0: 0100007F:E588 00000000:0000 0A 00000000:00000000 00:00000000 00000000  1000        0 98765 1 0000000000000000 100 0 0 10 0\n   1: 0100007F:E588 0100007F:9C40 01 00000000:00000000 00:00000000 00000000  1000        0 11111 1 0000000000000000 20 4 30 10 -1\n   2: 0100007F:1F90 00000000:0000 0A 00000000:00000000 00:00000000 00000000  1000        0 22222 1 0000000000000000 100 0 0 10 0\n";
        assert_eq!(listening_socket_inodes(table, 58760), vec![98765]);
        assert_eq!(listening_socket_inodes(table, 8080), vec![22222]);
        assert!(listening_socket_inodes(table, 58744).is_empty());
    }

    #[test]
    fn lsof_and_launchctl_output_parse() {
        assert_eq!(
            parse_lsof_listener("p4242\ncpython3\nf5\n"),
            Some((4242, "python3".to_string()))
        );
        assert_eq!(parse_lsof_listener(""), None);
        let print = "gui/501/com.madda.ghostex.gxserver = {\n\tactive count = 0\n\tstate = not running\n\tprogram = /Applications/Ghostex.app/gxserver\n\tlast exit code = 1\n}\n";
        assert_eq!(
            parse_launchctl_print(print),
            LaunchdJobStatus {
                running: false,
                last_exit_code: Some(1)
            }
        );
        let never = "\tstate = running\n\tlast exit code = (never exited)\n";
        assert_eq!(
            parse_launchctl_print(never),
            LaunchdJobStatus {
                running: true,
                last_exit_code: None
            }
        );
    }

    #[test]
    fn output_after_the_start_offset_is_read() {
        let dir =
            std::env::temp_dir().join(format!("ghostex-launch-log-test-{}", std::process::id()));
        fs::create_dir_all(&dir).unwrap();
        let log = dir.join("launch.log");
        fs::write(&log, "old run\nnew run line\n").unwrap();
        assert_eq!(launch_output_since(&log, 8), "new run line\n");
        // A log replaced since the start is read from its beginning.
        assert_eq!(launch_output_since(&log, 10_000), "old run\nnew run line\n");
        fs::remove_dir_all(&dir).unwrap();
    }
}
