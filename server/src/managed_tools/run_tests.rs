//! What `run::output_within` does with a command's output, input and time limit: it stops a
//! command at its deadline with every process it started, keeps the first 256 KB of stdout and of
//! stderr and drops the rest without saying so, reads both as lossy UTF-8, gives the command no
//! input, answers a missing program with `Unfinished::Start`, and reports a non-zero exit as a
//! finished run.

use super::run::{self, Unfinished};
use std::{
    path::Path,
    process::Command,
    time::{Duration, Instant},
};

/// What the runner keeps of each stream before it stops reading more.
const KEPT: usize = 256 * 1024;
/// How much one read can bring past that.
const READ_CHUNK: usize = 8192;

fn shell(script: &str) -> Command {
    let mut command = Command::new("/bin/sh");
    command.arg("-c").arg(script);
    command
}

fn alive(pid: i32) -> bool {
    unsafe { libc::kill(pid, 0) == 0 }
}

/// A killed orphan is reaped by init a moment later; until then `kill(pid, 0)` still finds it.
fn gone_soon(pid: i32) -> bool {
    let deadline = Instant::now() + Duration::from_secs(3);
    while Instant::now() < deadline {
        if !alive(pid) {
            return true;
        }
        std::thread::sleep(Duration::from_millis(20));
    }
    false
}

fn pids_in(file: &Path) -> Vec<i32> {
    std::fs::read_to_string(file)
        .expect("pids")
        .split_whitespace()
        .map(|pid| pid.parse().expect("pid"))
        .collect()
}

/// `count` bytes of numbered 16-byte lines, so a kept head can be checked byte for byte.
fn numbered(count: usize) -> String {
    (0..)
        .map(|line| format!("{line:015}\n"))
        .flat_map(|line| line.into_bytes())
        .take(count)
        .map(char::from)
        .collect()
}

fn numbered_script(count: usize, to_stderr: bool) -> String {
    let redirect = if to_stderr { " >&2" } else { "" };
    format!(
        "awk 'BEGIN {{ for (i = 0; i < {}; i++) printf \"%015d\\n\", i }}'{redirect}",
        count / 16
    )
}

#[test]
fn a_command_past_its_deadline_is_stopped_with_every_process_it_started() {
    let dir = tempfile::tempdir().expect("tempdir");
    let pids = dir.path().join("pids");
    let started = Instant::now();
    let answer = run::output_within(
        shell(&format!(
            "sleep 600 &\necho \"$$ $!\" > '{}'\nwait",
            pids.display()
        )),
        Duration::from_secs(2),
    );
    let took = started.elapsed();
    assert!(
        matches!(answer, Err(Unfinished::TimedOut(limit)) if limit == Duration::from_secs(2)),
        "it was not stopped at its time limit"
    );
    assert!(took >= Duration::from_secs(2), "stopped early: {took:?}");
    assert!(took < Duration::from_secs(6), "took {took:?}");
    for pid in pids_in(&pids) {
        assert!(gone_soon(pid), "{pid} is still running");
    }
}

#[test]
fn a_child_left_holding_the_output_is_stopped_at_the_deadline_and_what_was_printed_is_kept() {
    let dir = tempfile::tempdir().expect("tempdir");
    let pids = dir.path().join("pids");
    let started = Instant::now();
    let Ok(output) = run::output_within(
        shell(&format!(
            "echo printed\nsleep 600 &\necho $! > '{}'\nexit 3",
            pids.display()
        )),
        Duration::from_secs(2),
    ) else {
        panic!("the command exited, so it finished");
    };
    let took = started.elapsed();
    assert_eq!(output.status.code(), Some(3));
    assert_eq!(output.stdout, "printed\n");
    assert!(
        took >= Duration::from_secs(2),
        "did not wait for the pipes: {took:?}"
    );
    assert!(took < Duration::from_secs(6), "took {took:?}");
    for pid in pids_in(&pids) {
        assert!(gone_soon(pid), "the leftover child {pid} is still running");
    }
}

#[test]
fn stdout_past_256_kb_keeps_its_head_and_drops_the_rest_without_failing() {
    let sent = 1024 * 1024;
    let Ok(output) = run::output_within(
        shell(&numbered_script(sent, false)),
        Duration::from_secs(30),
    ) else {
        panic!("a long answer still finishes");
    };
    assert!(output.status.success());
    assert!(
        (KEPT..KEPT + READ_CHUNK).contains(&output.stdout.len()),
        "kept {} bytes",
        output.stdout.len()
    );
    assert!(numbered(sent).starts_with(&output.stdout));
    assert_eq!(output.stderr, "");
}

#[test]
fn stderr_is_kept_apart_from_stdout_with_the_same_head_cap() {
    let Ok(output) = run::output_within(
        shell("echo out; echo err >&2; exit 1"),
        Duration::from_secs(5),
    ) else {
        panic!("the command exited, so it finished");
    };
    assert_eq!(output.status.code(), Some(1));
    assert_eq!(output.stdout, "out\n");
    assert_eq!(output.stderr, "err\n");

    let sent = 1024 * 1024;
    let Ok(output) =
        run::output_within(shell(&numbered_script(sent, true)), Duration::from_secs(30))
    else {
        panic!("a long log still finishes");
    };
    assert!(
        (KEPT..KEPT + READ_CHUNK).contains(&output.stderr.len()),
        "kept {} bytes",
        output.stderr.len()
    );
    assert!(numbered(sent).starts_with(&output.stderr));
    assert_eq!(output.stdout, "");
}

#[test]
fn output_that_is_not_utf8_is_read_lossily() {
    let Ok(output) = run::output_within(shell("printf 'a\\377b'"), Duration::from_secs(5)) else {
        panic!("the command exited, so it finished");
    };
    assert_eq!(output.stdout, "a\u{FFFD}b");
}

#[test]
fn a_missing_program_does_not_start() {
    let dir = tempfile::tempdir().expect("tempdir");
    let answer = run::output_within(
        Command::new(dir.path().join("not-here")),
        Duration::from_secs(5),
    );
    assert!(matches!(answer, Err(Unfinished::Start)));
}

#[test]
fn the_command_gets_no_input() {
    let started = Instant::now();
    let Ok(output) = run::output_within(shell("cat; echo \"read:$?\""), Duration::from_secs(10))
    else {
        panic!("a command reading its input finds it empty and finishes");
    };
    assert!(started.elapsed() < Duration::from_secs(5));
    assert!(output.status.success());
    assert_eq!(output.stdout, "read:0\n");
}
