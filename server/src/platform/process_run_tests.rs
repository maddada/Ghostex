//! What `run_bounded` adds to a time-boxed run: input on stdin, a stdout cap that fails instead of
//! cutting, and the end of stderr.

use super::{run_bounded, Keep, RunLimits, Unfinished};
use std::{
    process::Command,
    time::{Duration, Instant},
};

fn shell(script: &str) -> Command {
    let mut command = Command::new("/bin/sh");
    command.arg("-c").arg(script);
    command
}

fn limits(stdin: Option<&[u8]>, stdout: Keep, stderr: Keep) -> RunLimits {
    RunLimits {
        timeout: Duration::from_secs(20),
        stdin: stdin.map(<[u8]>::to_vec),
        stdout,
        stderr,
    }
}

const MIB: usize = 1024 * 1024;

#[test]
fn the_command_reads_its_input_from_stdin() {
    let output = run_bounded(
        shell("cat"),
        limits(
            Some(b"{\"id\":\"ACM-1\"}"),
            Keep::AtMost(MIB),
            Keep::Tail(8192),
        ),
    )
    .expect("finished");
    assert!(output.status.success());
    assert_eq!(output.stdout, b"{\"id\":\"ACM-1\"}");
}

#[test]
fn a_command_that_prints_a_lot_before_it_reads_its_input_still_gets_all_of_it() {
    // A megabyte each way is more than any pipe buffer holds, so writing the input on the waiting
    // thread would leave both sides waiting on each other.
    let input = vec![b'x'; MIB];
    let output = run_bounded(
        shell("head -c 1048576 /dev/zero; wc -c | tr -d ' '"),
        limits(Some(&input), Keep::AtMost(2 * MIB), Keep::Tail(8192)),
    )
    .expect("finished");
    assert!(output.status.success());
    assert_eq!(output.stdout.len(), MIB + "1048576\n".len());
    assert!(output.stdout.ends_with(b"\x001048576\n"));
}

#[test]
fn stdout_past_its_cap_stops_the_command_and_fails() {
    let started = Instant::now();
    let answer = run_bounded(
        shell("while :; do echo 0123456789abcdef; done"),
        limits(None, Keep::AtMost(64 * 1024), Keep::Tail(8192)),
    );
    assert!(
        matches!(answer, Err(Unfinished::TooLarge(cap)) if cap == 64 * 1024),
        "{answer:?}"
    );
    assert!(started.elapsed() < Duration::from_secs(5));
}

#[test]
fn stdout_of_exactly_its_cap_is_kept_whole() {
    let output = run_bounded(
        shell("head -c 65536 /dev/zero"),
        limits(None, Keep::AtMost(64 * 1024), Keep::Tail(8192)),
    )
    .expect("finished");
    assert_eq!(output.stdout.len(), 64 * 1024);
}

#[test]
fn stderr_keeps_its_last_bytes() {
    let output = run_bounded(
        shell("awk 'BEGIN { for (i = 0; i < 10000; i++) printf \"%07d\\n\", i }' >&2; exit 2"),
        limits(None, Keep::AtMost(MIB), Keep::Tail(8000)),
    )
    .expect("finished");
    assert_eq!(output.status.code(), Some(2));
    let expected: String = (9000..10000).map(|line| format!("{line:07}\n")).collect();
    assert_eq!(String::from_utf8(output.stderr).unwrap(), expected);
}

#[test]
fn a_missing_program_says_why_it_did_not_start() {
    let dir = tempfile::tempdir().expect("tempdir");
    let answer = run_bounded(
        Command::new(dir.path().join("not-here")),
        limits(Some(b"{}"), Keep::AtMost(MIB), Keep::Tail(8192)),
    );
    assert!(
        matches!(&answer, Err(Unfinished::Start(error)) if error.kind() == std::io::ErrorKind::NotFound),
        "{answer:?}"
    );
}

/// A killed orphan is reaped by init a moment later; until then `kill(pid, 0)` still finds it.
fn gone_soon(pid: i32) -> bool {
    let deadline = Instant::now() + Duration::from_secs(3);
    while Instant::now() < deadline {
        if unsafe { libc::kill(pid, 0) } != 0 {
            return true;
        }
        std::thread::sleep(Duration::from_millis(20));
    }
    false
}

/// The processes a test's command started, killed if the test fails, so a test that catches a
/// process left running doesn't leave it running for the tests after it. The command writes their
/// pids to the guard's file in one step (`write_pids`); reading it never panics. A pid the test
/// has seen exit is taken off (`gone`), so the guard doesn't kill whatever reuses it later.
struct Leftovers {
    file: std::path::PathBuf,
    gone: Vec<i32>,
}

impl Leftovers {
    fn new(dir: &std::path::Path) -> Self {
        Self {
            file: dir.join("pids"),
            gone: Vec::new(),
        }
    }

    /// Shell that writes `pids` to the file whole: a reader never sees a number half written.
    fn write_pids(&self, pids: &str) -> String {
        let file = self.file.display();
        format!("echo {pids} > '{file}.tmp' && mv '{file}.tmp' '{file}'")
    }

    /// The pids written so far: none for a missing file, without a token that isn't a pid.
    fn pids(&self) -> Vec<i32> {
        std::fs::read_to_string(&self.file)
            .unwrap_or_default()
            .split_whitespace()
            .filter_map(|pid| pid.parse().ok())
            .filter(|pid| *pid > 0)
            .collect()
    }

    fn gone(&mut self, pid: i32) {
        self.gone.push(pid);
    }
}

impl Drop for Leftovers {
    fn drop(&mut self) {
        if !std::thread::panicking() {
            return;
        }
        for pid in self.pids() {
            if !self.gone.contains(&pid) && unsafe { libc::kill(pid, 0) } == 0 {
                unsafe {
                    libc::kill(pid, libc::SIGKILL);
                }
            }
        }
    }
}

/// Checks each pid the command wrote is gone, taking it off the guard once it is.
fn assert_all_gone(leftovers: &mut Leftovers, what: &str) {
    let pids = leftovers.pids();
    assert!(!pids.is_empty(), "the command never wrote its pids");
    for pid in pids {
        assert!(gone_soon(pid), "{what} {pid} is still running");
        leftovers.gone(pid);
    }
}

#[test]
fn a_child_left_holding_the_input_is_ended_at_the_deadline() {
    let dir = tempfile::tempdir().expect("tempdir");
    let mut leftovers = Leftovers::new(dir.path());
    // More input than a pipe holds, so the write waits on the child that never reads it.
    let input = vec![b'x'; MIB];
    let output = run_bounded(
        shell(&format!(
            "sleep 600 <&0 >/dev/null 2>&1 &\n{}\nexit 0",
            leftovers.write_pids("$!")
        )),
        RunLimits {
            timeout: Duration::from_secs(2),
            ..limits(Some(&input), Keep::AtMost(MIB), Keep::Tail(8192))
        },
    )
    .expect("the command exited, so it finished");
    assert!(output.status.success());
    assert_all_gone(&mut leftovers, "the child holding the input");
}

#[test]
fn an_answer_still_held_open_at_the_deadline_fails_instead_of_passing_cut_short() {
    let dir = tempfile::tempdir().expect("tempdir");
    let mut leftovers = Leftovers::new(dir.path());
    // It prints the start of an answer and exits, while a child it left behind keeps stdout open.
    let answer = run_bounded(
        shell(&format!(
            "printf '{{\"a\":'\nsleep 600 &\n{}\nexit 0",
            leftovers.write_pids("$!")
        )),
        RunLimits {
            timeout: Duration::from_secs(2),
            ..limits(None, Keep::AtMost(MIB), Keep::Tail(8192))
        },
    );
    assert!(
        matches!(answer, Err(Unfinished::TimedOut(limit)) if limit == Duration::from_secs(2)),
        "{answer:?}"
    );
    assert_all_gone(&mut leftovers, "the child holding the answer");
}

#[test]
fn passing_the_cap_ends_the_whole_tree() {
    let dir = tempfile::tempdir().expect("tempdir");
    let mut leftovers = Leftovers::new(dir.path());
    // It ignores the closed pipe, so only ending the tree stops it and the child it started.
    let answer = run_bounded(
        shell(&format!(
            "trap '' PIPE\nsleep 600 >/dev/null 2>&1 &\n{}\nwhile :; do echo 0123456789abcdef; done 2>/dev/null",
            leftovers.write_pids("\"$$ $!\"")
        )),
        limits(None, Keep::AtMost(64 * 1024), Keep::Tail(8192)),
    );
    assert!(
        matches!(answer, Err(Unfinished::TooLarge(cap)) if cap == 64 * 1024),
        "{answer:?}"
    );
    assert_all_gone(&mut leftovers, "the command or its child");
}

#[test]
fn run_output_debug_shows_the_status_and_sizes_never_the_bytes() {
    let output = run_bounded(
        shell("printf out-secret; printf err-secret-x >&2; exit 3"),
        limits(None, Keep::AtMost(MIB), Keep::Tail(8192)),
    )
    .expect("finished");
    let printed = format!("{output:?}");
    assert!(!printed.contains("secret"), "{printed}");
    assert!(printed.contains("stdout_bytes: 10"), "{printed}");
    assert!(printed.contains("stderr_bytes: 12"), "{printed}");
    assert_eq!(output.status.code(), Some(3));
    assert!(
        printed.contains(&format!("{:?}", output.status)),
        "{printed}"
    );
}
