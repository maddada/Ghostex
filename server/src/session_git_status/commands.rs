use super::*;
use crate::platform::process::COMMAND_READER_GRACE;

const GIT_COMMAND_TIMEOUT: Duration = Duration::from_secs(5);
const GH_COMMAND_TIMEOUT: Duration = Duration::from_secs(10);
const COMMAND_POLL_INTERVAL: Duration = Duration::from_millis(10);

/// Draining threads abandoned because their pipe outlived the command that
/// owned it. See `run_command_bounded`.
pub(super) static ABANDONED_COMMAND_READERS: AtomicUsize = AtomicUsize::new(0);

/// The same fact for the per-project `origin` probe, counted apart so the two
/// passes are distinguishable in the log: a leaked helper under
/// `project_git_remote` points at a different command set (and a different
/// user-visible symptom) than one under the session git-status probe, and one
/// shared counter would report both under the other's name.
pub(super) static ABANDONED_PROJECT_GIT_REMOTE_READERS: AtomicUsize = AtomicUsize::new(0);

// ---------------------------------------------------------------------------
// the real prober
// ---------------------------------------------------------------------------

pub struct SystemSessionGitStatusProber;

impl SessionGitStatusProber for SystemSessionGitStatusProber {
    fn supports_pull_requests(&self) -> bool {
        gh_cli_is_available()
    }

    fn probe_git(&self, cwd: &str) -> Option<SessionGitProbe> {
        let owned_cwd = cwd.to_string();
        probe_git_status_with(&move |args: &[&str]| run_git_probe_command(&owned_cwd, args))
    }

    fn probe_pull_request(&self, cwd: &str, branch: &str) -> Option<SessionPullRequest> {
        let output = run_gh_command(
            Some(cwd),
            &["pr", "view", branch, "--json", "number,state,url,isDraft"],
        )?;
        parse_gh_pull_request_json(&output)
    }
}

/*
`gh` detection is a whole-machine fact, so it is cached process-wide instead of
per cwd. `gh auth status` is the only check that answers both halves of the
question ("installed" and "logged in") and it exits non-zero for either failure,
so a machine without `gh` degrades to no PR badges with one cheap spawn every
`GH_AVAILABILITY_TTL`.
*/
fn gh_availability_cache() -> &'static Mutex<Option<(Instant, bool)>> {
    static CACHE: OnceLock<Mutex<Option<(Instant, bool)>>> = OnceLock::new();
    CACHE.get_or_init(|| Mutex::new(None))
}

/// What the last `gh auth status` answered, without running it: for projections, which must not
/// spawn. `false` until something has asked [`gh_cli_is_available`] once (the background passes do
/// within a minute of start).
pub(crate) fn cached_gh_cli_available() -> bool {
    #[cfg(test)]
    if GH_ASSUMED_AVAILABLE.with(|assumed| assumed.get()) {
        return true;
    }
    gh_availability_cache()
        .lock()
        .ok()
        .and_then(|cache| cache.map(|(_, is_available)| is_available))
        .unwrap_or(false)
}

#[cfg(test)]
thread_local! {
    static GH_ASSUMED_AVAILABLE: std::cell::Cell<bool> = const { std::cell::Cell::new(false) };
}

/// Puts `gh_cli_is_available` and `cached_gh_cli_available` back to what they were when dropped.
#[cfg(test)]
#[must_use = "bind it for the whole test: `let _gh = ...`; a bare `_` drops it at once"]
pub(crate) struct GhAssumedAvailable {
    previous: bool,
}

#[cfg(test)]
impl Drop for GhAssumedAvailable {
    fn drop(&mut self) {
        GH_ASSUMED_AVAILABLE.with(|assumed| assumed.set(self.previous));
    }
}

/// CDXC:SessionLinks 2026-10-28 WHY:
/// A test that needs `gh` signed in says so for its own thread, for as long as it holds the guard. The process-wide answer is shared by every test and tests run in parallel, so on a computer without `gh` another test's `gh auth status` probe could start before a seeded `true` and store `false` after it. A guard rather than a flag that stays set, because libtest runs a test on the harness thread when it cannot spawn one, and a flag left set would leak into the next test run there.
#[cfg(test)]
pub(crate) fn assume_gh_cli_available_for_test() -> GhAssumedAvailable {
    GhAssumedAvailable {
        previous: GH_ASSUMED_AVAILABLE.with(|assumed| assumed.replace(true)),
    }
}

pub(crate) fn gh_cli_is_available() -> bool {
    #[cfg(test)]
    if GH_ASSUMED_AVAILABLE.with(|assumed| assumed.get()) {
        return true;
    }
    let cache = gh_availability_cache();
    if let Ok(cache) = cache.lock() {
        if let Some((probed_at, is_available)) = *cache {
            if probed_at.elapsed() < GH_AVAILABILITY_TTL {
                return is_available;
            }
        }
    }
    let is_available = run_gh_command(None, &["auth", "status"]).is_some();
    if let Ok(mut cache) = cache.lock() {
        *cache = Some((Instant::now(), is_available));
    }
    is_available
}

/*
The shared git-probe runner: time-boxed, process-tree killed on timeout, output
drained by a helper thread (see `run_command_bounded`). `project_git_remote` runs
its `origin` probe through this same plumbing (via
`run_project_git_remote_probe_command`) rather than mirroring the hardening, so
there is exactly one place where a background git spawn's safety rules live.

The only thing the two surfaces do NOT share is the abandoned-reader counter:
each passes its own, so a stranded pipe is reported under the pass that caused
it instead of under whichever pass happened to own the counter.
*/
pub fn run_git_probe_command(cwd: &str, args: &[&str]) -> Option<String> {
    run_git_probe_command_counted(cwd, args, &ABANDONED_COMMAND_READERS)
}

/// The `project_git_remote` probe's entry point into the shared runner. Same
/// hardening, own abandoned-reader counter (see `run_git_probe_command`).
pub fn run_project_git_remote_probe_command(cwd: &str, args: &[&str]) -> Option<String> {
    run_git_probe_command_counted(cwd, args, &ABANDONED_PROJECT_GIT_REMOTE_READERS)
}

fn run_git_probe_command_counted(
    cwd: &str,
    args: &[&str],
    abandoned_readers: &'static AtomicUsize,
) -> Option<String> {
    let mut command = Command::new("git");
    command
        .args(args)
        .current_dir(cwd)
        /*
        `git diff` normally refreshes the index, which takes `.git/index.lock`.
        A background probe must never contend with the user's own git, so it
        reads without taking optional locks and never blocks on a credential
        prompt.
        */
        .env("GIT_OPTIONAL_LOCKS", "0")
        .env("GIT_TERMINAL_PROMPT", "0");
    run_command_with_timeout(command, GIT_COMMAND_TIMEOUT, abandoned_readers)
}

pub(crate) fn run_gh_command(cwd: Option<&str>, args: &[&str]) -> Option<String> {
    run_command_with_timeout(
        gh_command(cwd, args),
        GH_COMMAND_TIMEOUT,
        &ABANDONED_COMMAND_READERS,
    )
}

/// `gh` whose output matters even when it exits non-zero: `gh api` exits 1 on a 304 Not Modified
/// (the headers `-i` prints still say so) and on a GraphQL answer that carries errors beside its
/// data. `None` only when it could not run or timed out.
pub(crate) fn run_gh_command_any_status(cwd: Option<&str>, args: &[&str]) -> Option<String> {
    run_command_bounded_keeping_output(
        gh_command(cwd, args),
        GH_COMMAND_TIMEOUT,
        COMMAND_READER_GRACE,
        &ABANDONED_COMMAND_READERS,
        true,
    )
}

/// `gh` processes started since the last `take_gh_command_count`: what the links and work mode
/// passes log per pass.
static GH_COMMANDS: AtomicUsize = AtomicUsize::new(0);

pub(crate) fn take_gh_command_count() -> usize {
    GH_COMMANDS.swap(0, Ordering::Relaxed)
}

fn gh_command(cwd: Option<&str>, args: &[&str]) -> Command {
    GH_COMMANDS.fetch_add(1, Ordering::Relaxed);
    let mut command = Command::new("gh");
    command
        .args(args)
        .env("PATH", crate::managed_tools::run::job_path(&[]))
        .env("GH_NO_UPDATE_NOTIFIER", "1")
        .env("GH_PROMPT_DISABLED", "1")
        .env("NO_COLOR", "1")
        .env("GIT_TERMINAL_PROMPT", "0");
    if let Some(cwd) = cwd {
        command.current_dir(cwd);
    }
    command
}

/*
Time-boxed capture. stdout is drained by a helper thread for the whole lifetime
of the child: `git diff --numstat` on a large branch easily exceeds a pipe
buffer, and a poll-then-read loop would deadlock against a child blocked writing
into a full pipe. On timeout the child's whole process tree (its process group
on Unix, its job on Windows: `ProcessTree`) is killed and the child is reaped,
and the caller sees the same `None` as any other failure — probes degrade to
"no git status", never to a wedged worker.

Killing the tree rather than only the child is what makes the time box real: a
killed `git` can leave a grandchild behind (an external diff driver, a
credential helper) still holding the write end of the pipe, and the draining
thread's `read_to_end` then never returns. For anything that survives even that
— a helper that detached itself out of the group or broke away from the job —
the parent waits no longer than `COMMAND_READER_GRACE` for the drained output
and abandons the thread otherwise, so one pathological checkout can never wedge
the refresh pass (a tokio blocking worker) permanently. Abandonments are counted
so the leak shows up in the log instead of being silent.
*/
fn run_command_with_timeout(
    command: Command,
    timeout: Duration,
    abandoned_readers: &'static AtomicUsize,
) -> Option<String> {
    run_command_bounded(command, timeout, COMMAND_READER_GRACE, abandoned_readers)
}

pub(super) fn run_command_bounded(
    command: Command,
    timeout: Duration,
    reader_grace: Duration,
    abandoned_readers: &'static AtomicUsize,
) -> Option<String> {
    run_command_bounded_keeping_output(command, timeout, reader_grace, abandoned_readers, false)
}

fn run_command_bounded_keeping_output(
    mut command: Command,
    timeout: Duration,
    reader_grace: Duration,
    abandoned_readers: &'static AtomicUsize,
    keep_failed_output: bool,
) -> Option<String> {
    command
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::null());
    let mut tree = crate::platform::process::ProcessTree::spawn(&mut command).ok()?;
    let mut stdout = tree.child().stdout.take()?;
    let (output_tx, output_rx) = mpsc::channel();
    std::thread::spawn(move || {
        let mut buffer = Vec::new();
        let _ = stdout.read_to_end(&mut buffer);
        let _ = output_tx.send(buffer);
    });

    let deadline = Instant::now() + timeout;
    let status = loop {
        match tree.child().try_wait() {
            Ok(Some(status)) => break Some(status),
            Ok(None) => {
                if Instant::now() >= deadline {
                    tree.end();
                    break None;
                }
                std::thread::sleep(COMMAND_POLL_INTERVAL);
            }
            Err(_) => {
                tree.end();
                break None;
            }
        }
    };
    /*
    A failed or timed-out command has no output worth waiting for, so the
    receiver is dropped here: the draining thread ends with the pipe the tree
    kill just closed, and nothing blocks in the meantime.
    */
    if !status?.success() && !keep_failed_output {
        return None;
    }
    let Ok(output) = output_rx.recv_timeout(reader_grace) else {
        abandoned_readers.fetch_add(1, Ordering::Relaxed);
        return None;
    };
    Some(String::from_utf8_lossy(&output).trim().to_string())
}

/// Drains the abandoned-reader count so the refresh pass can log it. Zero on a
/// healthy machine, and a non-zero value means some probe command left output
/// readers stranded rather than that the pass itself failed.
pub fn take_abandoned_command_readers() -> usize {
    ABANDONED_COMMAND_READERS.swap(0, Ordering::Relaxed)
}

/// The same drain for the per-project `origin` probe, so its leaks are logged
/// under their own event instead of being attributed to the git-status pass.
pub fn take_abandoned_project_git_remote_readers() -> usize {
    ABANDONED_PROJECT_GIT_REMOTE_READERS.swap(0, Ordering::Relaxed)
}
