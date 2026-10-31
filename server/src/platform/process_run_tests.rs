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

#[test]
fn a_child_left_holding_the_input_is_ended_at_the_deadline() {
    let dir = tempfile::tempdir().expect("tempdir");
    let pid_file = dir.path().join("pid");
    // More input than a pipe holds, so the write waits on the child that never reads it.
    let input = vec![b'x'; MIB];
    let output = run_bounded(
        shell(&format!(
            "sleep 600 <&0 >/dev/null 2>&1 &\necho $! > '{}'\nexit 0",
            pid_file.display()
        )),
        RunLimits {
            timeout: Duration::from_secs(2),
            ..limits(Some(&input), Keep::AtMost(MIB), Keep::Tail(8192))
        },
    )
    .expect("the command exited, so it finished");
    assert!(output.status.success());
    let pid: i32 = std::fs::read_to_string(&pid_file)
        .expect("pid")
        .trim()
        .parse()
        .expect("pid");
    assert!(
        gone_soon(pid),
        "the child holding the input {pid} is still running"
    );
}
