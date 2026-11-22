//! Version checks against fake tools that never answer (GitHub #336): a wrapper that re-execs itself
//! forever, one that starts a child and waits on it, one that exits but leaves a child holding its
//! output, and a normal one. Set `GHOSTEX_TOOL_PROBE_SCRATCH` to keep the fake tools somewhere
//! other than the temp folder.

use super::{
    flight::{Flight, Flights},
    run::{self, Unfinished},
    tools,
};
use std::{
    path::{Path, PathBuf},
    process::Command,
    sync::{
        atomic::{AtomicUsize, Ordering},
        Arc, LazyLock,
    },
    time::{Duration, Instant},
};

fn scratch() -> tempfile::TempDir {
    let root = std::env::var_os("GHOSTEX_TOOL_PROBE_SCRATCH")
        .map(PathBuf::from)
        .unwrap_or_else(std::env::temp_dir);
    std::fs::create_dir_all(&root).expect("scratch root");
    tempfile::TempDir::new_in(root).expect("scratch")
}

fn fake_tool(dir: &Path, name: &str, body: &str) -> PathBuf {
    use std::os::unix::fs::PermissionsExt;
    let path = dir.join(name);
    std::fs::write(&path, format!("#!/bin/sh\n{body}\n")).expect("write tool");
    std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o755)).expect("chmod");
    path
}

/// A shell condition that holds while this test process runs. Every endless fake tool loops on it:
/// a test run killed mid-test (an interrupted `cargo test`) never reaches the code that stops its
/// tools, and two such loops kept starting `sleep` every second for days (found 2026-11-04).
fn while_tests_run() -> String {
    format!("kill -0 {} 2>/dev/null", std::process::id())
}

fn alive(pid: i32) -> bool {
    unsafe { libc::kill(pid, 0) == 0 }
}

/// The pid a fake tool wrote, once it has written it.
fn read_pid(file: &Path) -> i32 {
    let deadline = Instant::now() + Duration::from_secs(5);
    loop {
        if let Some(pid) = std::fs::read_to_string(file)
            .ok()
            .and_then(|text| text.trim().parse().ok())
        {
            return pid;
        }
        assert!(
            Instant::now() < deadline,
            "the fake tool never wrote {file:?}"
        );
        std::thread::sleep(Duration::from_millis(20));
    }
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

#[test]
fn a_wrapper_that_re_execs_itself_forever_is_reported_not_answering_within_the_limit() {
    let dir = scratch();
    // The reporter's `gh`: `mise x` found the wrapper again, which ran `mise x` again, forever.
    let gh = fake_tool(
        dir.path(),
        "gh",
        &format!("{} || exit 0\nexec \"$0\" \"$@\"", while_tests_run()),
    );
    let started = Instant::now();
    let answer = tools::version_check(&gh, &["--version"]);
    let took = started.elapsed();
    assert_eq!(
        answer,
        Err("`gh --version` didn't answer within 5 seconds, so Ghostex stopped it.".to_string())
    );
    assert!(took >= run::PROBE_TIMEOUT, "stopped early: {took:?}");
    assert!(
        took < run::PROBE_TIMEOUT + Duration::from_secs(2),
        "took {took:?}"
    );
    assert_eq!(tools::version_of(&gh, &["--version"]), None);
}

#[test]
fn a_stuck_tool_is_stopped_with_the_child_it_started() {
    let dir = scratch();
    let pids = dir.path().join("pids");
    let tool = fake_tool(
        dir.path(),
        "spawner",
        &format!(
            "/bin/sh -c 'while {}; do sleep 1; done' &\necho \"$$ $!\" > '{}'\nwait",
            while_tests_run(),
            pids.display()
        ),
    );
    let started = Instant::now();
    let answer = run::output_within(Command::new(&tool), Duration::from_secs(1));
    assert!(matches!(answer, Err(Unfinished::TimedOut(_))));
    assert!(started.elapsed() < Duration::from_secs(3));
    let text = std::fs::read_to_string(&pids).expect("pids");
    let mut ids = text
        .split_whitespace()
        .map(|id| id.parse::<i32>().expect("pid"));
    let (wrapper, child) = (ids.next().expect("wrapper"), ids.next().expect("child"));
    assert!(gone_soon(wrapper), "the wrapper {wrapper} is still running");
    assert!(gone_soon(child), "its child {child} is still running");
}

#[test]
fn a_child_left_holding_the_output_does_not_hold_the_answer() {
    let dir = scratch();
    let pid = dir.path().join("pid");
    // Prints its version and exits, but a background child keeps its stdout open.
    let tool = fake_tool(
        dir.path(),
        "leaky",
        &format!(
            "echo 'leaky version 3.4.5'\nwhile {}; do sleep 1; done &\necho $! > '{}'\nexit 0",
            while_tests_run(),
            pid.display()
        ),
    );
    let started = Instant::now();
    let Ok(output) = run::output_within(Command::new(&tool), Duration::from_secs(1)) else {
        panic!("the tool exited, so it finished");
    };
    assert!(started.elapsed() < Duration::from_secs(3));
    assert!(output.status.success());
    assert_eq!(tools::version_in(&output.stdout).as_deref(), Some("3.4.5"));
    let child = read_pid(&pid);
    assert!(
        gone_soon(child),
        "the leftover child {child} is still running"
    );
}

#[test]
fn a_normal_tool_still_reports_its_version() {
    let dir = scratch();
    let gh = fake_tool(
        dir.path(),
        "gh",
        "echo 'gh version 2.63.0 (2024-11-27)'\necho 'https://github.com/cli/cli/releases/tag/v2.63.0'",
    );
    let started = Instant::now();
    assert_eq!(
        tools::version_check(&gh, &["--version"]),
        Ok(Some("2.63.0".to_string()))
    );
    assert!(started.elapsed() < Duration::from_secs(2));
    let missing = dir.path().join("not-here");
    assert_eq!(tools::version_check(&missing, &["--version"]), Ok(None));
}

#[test]
fn a_stuck_check_marks_the_tool_not_answering_and_keeps_its_place() {
    let mut status = tools::Status::new(String::new());
    let dir = scratch();
    let stuck = fake_tool(
        dir.path(),
        "uv",
        &format!("while {}; do sleep 1; done", while_tests_run()),
    );
    status.read_version(&stuck, &["--version"]);
    assert_eq!(status.version, None);
    assert!(status
        .not_answering
        .as_deref()
        .is_some_and(|reason| reason.starts_with("`uv --version` didn't answer")));
    let placeholder = super::endpoint::still_checking(tools::ToolId::Gh);
    assert_eq!(placeholder["id"], "gh");
    assert_eq!(placeholder["label"], "GitHub CLI");
    assert_eq!(placeholder["actions"], serde_json::json!([]));
    assert!(placeholder["notAnswering"]
        .as_str()
        .is_some_and(|reason| reason.starts_with("Checking GitHub CLI took longer than")));
}

static FLIGHTS: LazyLock<Flights<&'static str, u32>> = LazyLock::new(Flights::new);

#[test]
fn a_request_during_a_running_check_shares_it_instead_of_starting_another() {
    let runs = Arc::new(AtomicUsize::new(0));
    let check = |runs: &Arc<AtomicUsize>| -> Arc<Flight<u32>> {
        let runs = runs.clone();
        FLIGHTS.join("gh", move || {
            let run = runs.fetch_add(1, Ordering::SeqCst) as u32 + 1;
            std::thread::sleep(Duration::from_millis(400));
            run
        })
    };
    let first = check(&runs);
    let second = check(&runs);
    // The list's deadline passes while the check still runs: the waiter is told, the check goes on.
    assert_eq!(
        second.wait_until(Instant::now() + Duration::from_millis(50)),
        None
    );
    let third = check(&runs);
    let deadline = Instant::now() + Duration::from_secs(5);
    assert_eq!(first.wait_until(deadline), Some(1));
    assert_eq!(second.wait_until(deadline), Some(1));
    assert_eq!(third.wait_until(deadline), Some(1));
    assert_eq!(runs.load(Ordering::SeqCst), 1);
    // Once it has answered, the next request checks again.
    assert_eq!(check(&runs).wait_until(deadline), Some(2));
    // A check that panics frees its tool and answers nothing.
    let panicked = FLIGHTS.join("glab", || panic!("check failed"));
    assert_eq!(panicked.wait_until(deadline), None);
    assert_eq!(FLIGHTS.join("glab", || 7).wait_until(deadline), Some(7));
}
