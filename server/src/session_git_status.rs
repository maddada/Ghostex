use std::{
    collections::{HashMap, HashSet},
    io::Read,
    process::{Command, Stdio},
    sync::{
        atomic::{AtomicUsize, Ordering},
        mpsc, Mutex, OnceLock,
    },
    time::{Duration, Instant},
};

use serde_json::{json, Map, Value};

/*
CDXC:SidebarV2GitStatus 2026-07-29-00:00:
Server side of Sidebar V2's git/PR card row (spec `plans/009-sidebar-v2-inbox.md`,
decision 7 + the P3 wire contract). Each worktree session's cwd IS the checkout it
works in, so per-session git state is resolved from the session cwd and published
on `GxserverPresentationSession.gitStatus`:

  { branch: string|null, additions: number, deletions: number,
    prNumber?: number, prState?: "open"|"draft"|"merged"|"closed",
    prUrl?: string, updatedAt: string }

Rules this module holds itself to:

- One probe per UNIQUE cwd. Many sessions share a checkout (a project's terminal,
  its agent, its browser row), so the cache is keyed by cwd and every session
  pointing at it reads the same answer.
- Never on a request path. A background pass (server.rs) refreshes the cache;
  presentation and the lifecycle sweep only ever READ it, and read a cwd that has
  never been probed as "no git status" rather than probing inline.
- Every subprocess is time-boxed and killed on timeout, so a hung git (network
  filesystem, credential prompt) can never wedge the daemon.
- `gh` absent or unauthed is not an error: the PR fields simply do not exist. The
  detection itself is cached so a machine without `gh` spawns one probe per
  `GH_AVAILABILITY_TTL`, not one per pass.
- Non-git cwds are cached as negative entries (a longer TTL than real repos —
  a directory rarely becomes a repository) so a terminal parked in ~/Downloads
  does not cost a git spawn a minute.
*/

/// Background refresh cadence, in seconds. Matches the git TTL: a pass that runs
/// on the same clock as the TTL re-probes every live cwd once per minute.
pub const SESSION_GIT_STATUS_REFRESH_INTERVAL_SECONDS: u64 = 60;

/// How long a successful git probe stays authoritative.
pub const GIT_STATUS_TTL_MS: i64 = 60_000;

/// Non-repository cwds are re-checked far less often than repositories.
pub const NON_REPOSITORY_STATUS_TTL_MS: i64 = 5 * 60_000;

/// How long a `gh pr view` answer stays authoritative. A PR's state changes on
/// human timescales and the call costs a network round trip, so it outlives
/// several git passes; a branch change invalidates it immediately.
pub const PULL_REQUEST_TTL_MS: i64 = 5 * 60_000;

/// How long the "is `gh` installed and authed" answer is reused.
pub const GH_AVAILABILITY_TTL: Duration = Duration::from_secs(5 * 60);

/// Upper bound on git probes in one pass. A machine with a hundred sessions
/// spread over a hundred checkouts spreads the work over passes instead of
/// spending a minute of a blocking worker in one go; the oldest entries go first.
pub const MAX_GIT_PROBES_PER_PASS: usize = 24;

/// Upper bound on `gh` calls in one pass. These are network round trips, so they
/// are rationed harder than the local git commands.
pub const MAX_PULL_REQUEST_PROBES_PER_PASS: usize = 12;

const GIT_COMMAND_TIMEOUT: Duration = Duration::from_secs(5);
const GH_COMMAND_TIMEOUT: Duration = Duration::from_secs(10);
const COMMAND_POLL_INTERVAL: Duration = Duration::from_millis(10);

/// How long the parent waits for the output-draining thread after the command
/// has exited successfully. A command cannot exit before it has written its
/// output, so at most a pipe buffer is ever still in flight and the thread
/// finishes at once; the grace only expires when something outlived the command
/// and its process group and still holds the write end of the pipe.
const COMMAND_READER_GRACE: Duration = Duration::from_secs(2);

/// Draining threads abandoned because their pipe outlived the command that
/// owned it. See `run_command_bounded`.
static ABANDONED_COMMAND_READERS: AtomicUsize = AtomicUsize::new(0);

/// The same fact for the per-project `origin` probe, counted apart so the two
/// passes are distinguishable in the log: a leaked helper under
/// `project_git_remote` points at a different command set (and a different
/// user-visible symptom) than one under the session git-status probe, and one
/// shared counter would report both under the other's name.
static ABANDONED_PROJECT_GIT_REMOTE_READERS: AtomicUsize = AtomicUsize::new(0);

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PullRequestState {
    Open,
    Draft,
    Merged,
    Closed,
}

impl PullRequestState {
    pub fn as_wire(&self) -> &'static str {
        match self {
            PullRequestState::Open => "open",
            PullRequestState::Draft => "draft",
            PullRequestState::Merged => "merged",
            PullRequestState::Closed => "closed",
        }
    }

    pub fn disposition(&self) -> PullRequestDisposition {
        match self {
            PullRequestState::Open | PullRequestState::Draft => PullRequestDisposition::Open,
            PullRequestState::Merged | PullRequestState::Closed => PullRequestDisposition::Finished,
        }
    }
}

/*
The coarse view the auto-settle sweep needs. `Unknown` covers everything that is
not a definite answer — no `gh`, no repository, a branch with no PR, a cwd that
has not been probed yet — and never settles anything on its own.
*/
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum PullRequestDisposition {
    #[default]
    Unknown,
    Open,
    Finished,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SessionPullRequest {
    pub number: i64,
    pub state: PullRequestState,
    pub url: Option<String>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SessionGitStatus {
    /// `None` when the checkout is detached; published as an explicit `null`.
    pub branch: Option<String>,
    pub additions: i64,
    pub deletions: i64,
    pub pull_request: Option<SessionPullRequest>,
    /// Probe time, RFC3339. Wall-clock, unlike the cache's monotonic stamps.
    pub updated_at: String,
}

impl SessionGitStatus {
    /// The published wire object. `branch` is a required nullable key; the PR
    /// keys exist only when a pull request was actually found.
    pub fn to_presentation_value(&self) -> Value {
        let mut output = Map::new();
        output.insert(
            "branch".to_string(),
            match &self.branch {
                Some(branch) => Value::String(branch.clone()),
                None => Value::Null,
            },
        );
        output.insert("additions".to_string(), json!(self.additions));
        output.insert("deletions".to_string(), json!(self.deletions));
        if let Some(pull_request) = &self.pull_request {
            output.insert("prNumber".to_string(), json!(pull_request.number));
            output.insert(
                "prState".to_string(),
                Value::String(pull_request.state.as_wire().to_string()),
            );
            if let Some(url) = &pull_request.url {
                output.insert("prUrl".to_string(), Value::String(url.clone()));
            }
        }
        output.insert(
            "updatedAt".to_string(),
            Value::String(self.updated_at.clone()),
        );
        Value::Object(output)
    }

    pub fn pull_request_disposition(&self) -> PullRequestDisposition {
        match &self.pull_request {
            Some(pull_request) => pull_request.state.disposition(),
            None => PullRequestDisposition::Unknown,
        }
    }
}

/// One raw git answer for a cwd, before caching or PR enrichment.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SessionGitProbe {
    pub branch: Option<String>,
    pub additions: i64,
    pub deletions: i64,
}

/*
The probe surface, injected so the cache's TTL/budget/delta rules are testable
without spawning git or reaching the network.
*/
pub trait SessionGitStatusProber {
    /// Resolved once per pass: `gh` missing or unauthed means no PR fields at all.
    fn supports_pull_requests(&self) -> bool;
    /// `None` when the cwd is not inside a git worktree (or git is unusable).
    fn probe_git(&self, cwd: &str) -> Option<SessionGitProbe>;
    fn probe_pull_request(&self, cwd: &str, branch: &str) -> Option<SessionPullRequest>;
}

#[derive(Clone, Debug)]
pub struct SessionGitStatusRefreshClock {
    /// Monotonic milliseconds; only differences matter.
    pub monotonic_now_ms: i64,
    /// Wall-clock RFC3339 stamped onto every freshly probed status.
    pub now_iso: String,
}

#[derive(Clone, Debug)]
struct SessionGitStatusEntry {
    git_probed_at_ms: i64,
    /// `None` when this entry has never had a `gh` answer (no `gh`, detached
    /// HEAD, or the pass ran out of PR budget before reaching it).
    pull_request_probed_at_ms: Option<i64>,
    /// `None` is the negative entry: probed, and not a git worktree.
    status: Option<SessionGitStatus>,
}

#[derive(Debug)]
struct SessionGitStatusRefreshTarget {
    cwd: String,
    previous: Option<SessionGitStatusEntry>,
}

#[derive(Default)]
pub struct SessionGitStatusCache {
    entries: HashMap<String, SessionGitStatusEntry>,
}

impl SessionGitStatusCache {
    pub fn get(&self, cwd: &str) -> Option<SessionGitStatus> {
        self.entries.get(cwd).and_then(|entry| entry.status.clone())
    }

    pub fn len(&self) -> usize {
        self.entries.len()
    }

    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }

    /// Seeds an entry directly. Only the refresh pass and tests should use this.
    pub fn set(&mut self, cwd: &str, status: Option<SessionGitStatus>, monotonic_now_ms: i64) {
        self.entries.insert(
            cwd.to_string(),
            SessionGitStatusEntry {
                git_probed_at_ms: monotonic_now_ms,
                pull_request_probed_at_ms: status
                    .as_ref()
                    .and_then(|status| status.pull_request.as_ref())
                    .map(|_| monotonic_now_ms),
                status,
            },
        );
    }

    /*
    Phase one of a pass: drop cwds no live session points at any more, then pick
    the stale ones, oldest first, up to the per-pass budget. Everything the
    prober needs is copied out so the pass can spawn git with the lock RELEASED —
    presentation reads this cache and must never wait on a subprocess.
    */
    fn plan_refresh(
        &mut self,
        cwds: &[String],
        monotonic_now_ms: i64,
    ) -> Vec<SessionGitStatusRefreshTarget> {
        let mut wanted: Vec<&str> = Vec::new();
        let mut seen: HashSet<&str> = HashSet::new();
        for cwd in cwds {
            let cwd = cwd.trim();
            if cwd.is_empty() {
                continue;
            }
            if seen.insert(cwd) {
                wanted.push(cwd);
            }
        }
        self.entries.retain(|cwd, _| seen.contains(cwd.as_str()));

        let mut stale: Vec<(i64, String)> = wanted
            .into_iter()
            .filter_map(|cwd| match self.entries.get(cwd) {
                None => Some((i64::MIN, cwd.to_string())),
                Some(entry) => {
                    let ttl = if entry.status.is_some() {
                        GIT_STATUS_TTL_MS
                    } else {
                        NON_REPOSITORY_STATUS_TTL_MS
                    };
                    (monotonic_now_ms - entry.git_probed_at_ms >= ttl)
                        .then(|| (entry.git_probed_at_ms, cwd.to_string()))
                }
            })
            .collect();
        stale.sort_by(|left, right| left.0.cmp(&right.0).then_with(|| left.1.cmp(&right.1)));
        stale
            .into_iter()
            .take(MAX_GIT_PROBES_PER_PASS)
            .map(|(_, cwd)| SessionGitStatusRefreshTarget {
                previous: self.entries.get(&cwd).cloned(),
                cwd,
            })
            .collect()
    }

    /*
    Phase three: fold the probe results back in and report which cwds MEANINGFULLY
    changed. `updatedAt` moves on every successful probe (it is the freshness
    stamp clients render), so it is deliberately excluded from the comparison —
    otherwise every pass would emit a presentation delta for every session.
    */
    fn apply_refresh(&mut self, results: Vec<(String, SessionGitStatusEntry)>) -> Vec<String> {
        let mut changed = Vec::new();
        for (cwd, entry) in results {
            let previous = self
                .entries
                .get(&cwd)
                .and_then(|entry| entry.status.clone());
            if !git_status_is_meaningfully_equal(previous.as_ref(), entry.status.as_ref()) {
                changed.push(cwd.clone());
            }
            self.entries.insert(cwd, entry);
        }
        changed
    }
}

fn git_status_is_meaningfully_equal(
    left: Option<&SessionGitStatus>,
    right: Option<&SessionGitStatus>,
) -> bool {
    match (left, right) {
        (None, None) => true,
        (Some(left), Some(right)) => {
            left.branch == right.branch
                && left.additions == right.additions
                && left.deletions == right.deletions
                && left.pull_request == right.pull_request
        }
        _ => false,
    }
}

/*
One refresh pass over `cwds`. The cache lock is taken twice — to plan and to
merge — and never held while a subprocess runs. Returns the cwds whose published
status changed, which is exactly the set the caller turns into presentation
deltas.

CDXC:SidebarV2DataGate 2026-07-29:
`sidebar_v2_selected` is the version gate (`session_lifecycle::
read_sidebar_v2_selected`), taken as an argument so no entry point into this
module can probe without stating it. It is checked BEFORE `plan_refresh`, which
is what makes a gated-off pass truly free: no git spawn, no `gh` network call, no
delta — and no eviction either, so entries an earlier V2 stretch left behind stay
in memory (harmless: only V2 surfaces read them) and the first pass after the
user flips to V2 refreshes them normally.
*/
pub fn run_session_git_status_refresh_pass(
    cache: &Mutex<SessionGitStatusCache>,
    cwds: &[String],
    prober: &dyn SessionGitStatusProber,
    clock: &SessionGitStatusRefreshClock,
    sidebar_v2_selected: bool,
) -> Vec<String> {
    if !sidebar_v2_selected {
        return Vec::new();
    }
    let targets = {
        let Ok(mut cache) = cache.lock() else {
            return Vec::new();
        };
        cache.plan_refresh(cwds, clock.monotonic_now_ms)
    };
    if targets.is_empty() {
        return Vec::new();
    }

    let supports_pull_requests = prober.supports_pull_requests();
    let mut pull_request_budget = MAX_PULL_REQUEST_PROBES_PER_PASS;
    let mut results = Vec::with_capacity(targets.len());
    for target in targets {
        let Some(probe) = prober.probe_git(&target.cwd) else {
            results.push((
                target.cwd,
                SessionGitStatusEntry {
                    git_probed_at_ms: clock.monotonic_now_ms,
                    pull_request_probed_at_ms: None,
                    status: None,
                },
            ));
            continue;
        };
        let previous_status = target
            .previous
            .as_ref()
            .and_then(|entry| entry.status.as_ref());
        let previous_branch = previous_status.and_then(|status| status.branch.as_deref());
        let previous_pull_request = previous_status.and_then(|status| status.pull_request.clone());
        let previous_pull_request_probed_at_ms = target
            .previous
            .as_ref()
            .and_then(|entry| entry.pull_request_probed_at_ms);

        let mut pull_request = None;
        let mut pull_request_probed_at_ms = None;
        if supports_pull_requests {
            if let Some(branch) = probe.branch.as_deref() {
                /*
                A PR answer survives several git passes, but only while the
                branch it was asked about is still checked out. A branch switch
                (or a rename) invalidates it immediately instead of showing the
                old branch's PR badge against new work.
                */
                let is_reusable = previous_branch == Some(branch)
                    && previous_pull_request_probed_at_ms.is_some_and(|probed_at_ms| {
                        clock.monotonic_now_ms - probed_at_ms < PULL_REQUEST_TTL_MS
                    });
                if is_reusable {
                    pull_request = previous_pull_request.clone();
                    pull_request_probed_at_ms = previous_pull_request_probed_at_ms;
                } else if pull_request_budget > 0 {
                    pull_request_budget -= 1;
                    pull_request = prober.probe_pull_request(&target.cwd, branch);
                    pull_request_probed_at_ms = Some(clock.monotonic_now_ms);
                } else if previous_branch == Some(branch) {
                    // Out of network budget: keep the last answer AND its stamp,
                    // so the next pass still sees it as due.
                    pull_request = previous_pull_request.clone();
                    pull_request_probed_at_ms = previous_pull_request_probed_at_ms;
                }
            }
        }

        results.push((
            target.cwd,
            SessionGitStatusEntry {
                git_probed_at_ms: clock.monotonic_now_ms,
                pull_request_probed_at_ms,
                status: Some(SessionGitStatus {
                    branch: probe.branch,
                    additions: probe.additions,
                    deletions: probe.deletions,
                    pull_request,
                    updated_at: clock.now_iso.clone(),
                }),
            },
        ));
    }

    let Ok(mut cache) = cache.lock() else {
        return Vec::new();
    };
    cache.apply_refresh(results)
}

// ---------------------------------------------------------------------------
// process-wide cache
// ---------------------------------------------------------------------------

fn session_git_status_cache() -> &'static Mutex<SessionGitStatusCache> {
    static CACHE: OnceLock<Mutex<SessionGitStatusCache>> = OnceLock::new();
    CACHE.get_or_init(|| Mutex::new(SessionGitStatusCache::default()))
}

fn monotonic_now_ms() -> i64 {
    static START: OnceLock<Instant> = OnceLock::new();
    START.get_or_init(Instant::now).elapsed().as_millis() as i64
}

fn now_iso() -> String {
    chrono::Utc::now().to_rfc3339_opts(chrono::SecondsFormat::Millis, true)
}

/// Runs one pass against the process-wide cache with the real git/`gh` prober.
/// Blocking: callers must be on a blocking worker, never on a request path.
/// `sidebar_v2_selected` must come from `session_lifecycle::
/// read_sidebar_v2_selected`, resolved once per pass — see the gate note on
/// `run_session_git_status_refresh_pass`.
pub fn refresh_session_git_status_cache(cwds: &[String], sidebar_v2_selected: bool) -> Vec<String> {
    run_session_git_status_refresh_pass(
        session_git_status_cache(),
        cwds,
        &SystemSessionGitStatusProber,
        &SessionGitStatusRefreshClock {
            monotonic_now_ms: monotonic_now_ms(),
            now_iso: now_iso(),
        },
        sidebar_v2_selected,
    )
}

/// Read-only cache lookup. Never probes, so it is safe on the request path.
pub fn cached_session_git_status(cwd: &str) -> Option<SessionGitStatus> {
    let cwd = cwd.trim();
    if cwd.is_empty() {
        return None;
    }
    session_git_status_cache().lock().ok()?.get(cwd)
}

/// The published `gitStatus` object for a session cwd, or `None` when the cwd is
/// unknown to the cache or is not a git worktree.
pub fn published_session_git_status(cwd: &str) -> Option<Value> {
    cached_session_git_status(cwd).map(|status| status.to_presentation_value())
}

/// The session cwd used as the cache key everywhere (presentation, the refresh
/// pass, and the lifecycle sweep must agree on it exactly).
pub fn session_cwd_key(session: &Value) -> Option<String> {
    session
        .get("cwd")
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|cwd| !cwd.is_empty())
        .map(str::to_string)
}

/*
CDXC:SidebarV2GitStatus 2026-07-30 (effective cwd):
A session row's git state lives in the directory the session actually RUNS in,
and that is not always `session.cwd`. Agent sessions are created without a cwd on
purpose — they run in their project's path — so `zmx.rs` and `agents.rs` resolve
`session.cwd` else `project.path` at every launch site. The git-status subsystem
was the only place reading `session.cwd` raw, which is why agent cards never
carried a branch: the probe set skipped them and presentation had nothing to
attach.

This is the one resolver for that rule, so the probe pass, presentation, and the
auto-settle sweep cannot drift apart. What gets PUBLISHED as `session.cwd` is
deliberately unchanged (Sidebar V2 reads it to tell a managed worktree checkout
apart from a project-root session), and nothing persists `project.path` into the
session row: that would go stale the moment a project moves and would not heal
the rows that already exist.

Note the project's OWN `path` is used, not the worktree family root: a worktree
project is a different checkout on a different branch, so its sessions must probe
the worktree, not the parent.
*/
pub fn effective_session_git_cwd(session: &Value, project: Option<&Value>) -> Option<String> {
    session_cwd_key(session).or_else(|| project.and_then(project_path_key))
}

/// The project path a session with no `cwd` of its own falls back to.
fn project_path_key(project: &Value) -> Option<String> {
    project
        .get("path")
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|path| !path.is_empty())
        .map(str::to_string)
}

/// What the auto-settle sweep sees for one session. Anything short of a definite
/// merged/closed pull request is `Unknown` and settles nothing. Takes the
/// session's project so a project-root session resolves the same cwd the probe
/// pass used (see `effective_session_git_cwd`); `None` means the caller could not
/// resolve one, which simply leaves a cwd-less session `Unknown`.
pub fn session_pull_request_disposition(
    session: &Value,
    project: Option<&Value>,
) -> PullRequestDisposition {
    let Some(cwd) = effective_session_git_cwd(session, project) else {
        return PullRequestDisposition::Unknown;
    };
    match cached_session_git_status(&cwd) {
        Some(status) => status.pull_request_disposition(),
        None => PullRequestDisposition::Unknown,
    }
}

#[cfg(test)]
pub fn set_cached_session_git_status_for_test(cwd: &str, status: Option<SessionGitStatus>) {
    if let Ok(mut cache) = session_git_status_cache().lock() {
        cache.set(cwd, status, monotonic_now_ms());
    }
}

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
fn gh_cli_is_available() -> bool {
    static CACHE: OnceLock<Mutex<Option<(Instant, bool)>>> = OnceLock::new();
    let cache = CACHE.get_or_init(|| Mutex::new(None));
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
The shared git-probe runner: time-boxed, process-group killed on timeout, output
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

fn run_gh_command(cwd: Option<&str>, args: &[&str]) -> Option<String> {
    let mut command = Command::new("gh");
    command
        .args(args)
        .env("GH_NO_UPDATE_NOTIFIER", "1")
        .env("GH_PROMPT_DISABLED", "1")
        .env("NO_COLOR", "1")
        .env("GIT_TERMINAL_PROMPT", "0");
    if let Some(cwd) = cwd {
        command.current_dir(cwd);
    }
    run_command_with_timeout(command, GH_COMMAND_TIMEOUT, &ABANDONED_COMMAND_READERS)
}

/*
Time-boxed capture. stdout is drained by a helper thread for the whole lifetime
of the child: `git diff --numstat` on a large branch easily exceeds a pipe
buffer, and a poll-then-read loop would deadlock against a child blocked writing
into a full pipe. On timeout the child's whole process GROUP is killed and the
child is reaped, and the caller sees the same `None` as any other failure —
probes degrade to "no git status", never to a wedged worker.

Killing the group rather than only the child is what makes the time box real: a
killed `git` can leave a grandchild behind (an external diff driver, a
credential helper) still holding the write end of the pipe, and the draining
thread's `read_to_end` then never returns. For anything that survives even that
— a helper that detached itself out of the group — the parent waits no longer
than `COMMAND_READER_GRACE` for the drained output and abandons the thread
otherwise, so one pathological checkout can never wedge the refresh pass (a
tokio blocking worker) permanently. Abandonments are counted so the leak shows
up in the log instead of being silent.
*/
fn run_command_with_timeout(
    command: Command,
    timeout: Duration,
    abandoned_readers: &'static AtomicUsize,
) -> Option<String> {
    run_command_bounded(command, timeout, COMMAND_READER_GRACE, abandoned_readers)
}

fn run_command_bounded(
    mut command: Command,
    timeout: Duration,
    reader_grace: Duration,
    abandoned_readers: &'static AtomicUsize,
) -> Option<String> {
    command
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::null());
    configure_command_process_group(&mut command);
    let mut child = command.spawn().ok()?;
    let mut stdout = child.stdout.take()?;
    let (output_tx, output_rx) = mpsc::channel();
    std::thread::spawn(move || {
        let mut buffer = Vec::new();
        let _ = stdout.read_to_end(&mut buffer);
        let _ = output_tx.send(buffer);
    });

    let deadline = Instant::now() + timeout;
    let status = loop {
        match child.try_wait() {
            Ok(Some(status)) => break Some(status),
            Ok(None) => {
                if Instant::now() >= deadline {
                    kill_command_process_group(&child);
                    let _ = child.kill();
                    let _ = child.wait();
                    break None;
                }
                std::thread::sleep(COMMAND_POLL_INTERVAL);
            }
            Err(_) => {
                kill_command_process_group(&child);
                let _ = child.kill();
                let _ = child.wait();
                break None;
            }
        }
    };
    /*
    A failed or timed-out command has no output worth waiting for, so the
    receiver is dropped here: the draining thread ends with the pipe the group
    kill just closed, and nothing blocks in the meantime.
    */
    if !status?.success() {
        return None;
    }
    let Ok(output) = output_rx.recv_timeout(reader_grace) else {
        abandoned_readers.fetch_add(1, Ordering::Relaxed);
        return None;
    };
    Some(String::from_utf8_lossy(&output).trim().to_string())
}

/*
Probe commands run in their own process group so a timeout can take out
everything the command started, not just the command.
*/
fn configure_command_process_group(command: &mut Command) {
    #[cfg(unix)]
    {
        use std::os::unix::process::CommandExt;
        command.process_group(0);
    }

    #[cfg(not(unix))]
    let _ = command;
}

#[cfg(unix)]
fn kill_command_process_group(child: &std::process::Child) {
    let process_group_id = child.id() as libc::pid_t;
    if process_group_id <= 0 {
        return;
    }
    unsafe {
        libc::kill(-process_group_id, libc::SIGKILL);
    }
}

#[cfg(not(unix))]
fn kill_command_process_group(_child: &std::process::Child) {}

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

// ---------------------------------------------------------------------------
// pure git plumbing
// ---------------------------------------------------------------------------

/// The repository's default branch and the ref to diff against.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DefaultBranch {
    pub name: String,
    pub git_ref: String,
}

/*
Default-branch resolution, most authoritative first:

1. `origin/HEAD` — what the remote itself says its default branch is.
2. `origin/main` then `origin/master` — repositories cloned before git started
   writing `origin/HEAD`, which is most of them.
3. local `main` then `master` — repositories with no remote at all.

`None` means "this repository has no recognizable default branch" (a fresh repo
whose only branch is the feature branch, say), and the caller then reports the
working tree against HEAD instead of inventing a base.
*/
pub fn resolve_default_branch(run: &dyn Fn(&[&str]) -> Option<String>) -> Option<DefaultBranch> {
    if let Some(value) = run(&[
        "symbolic-ref",
        "--quiet",
        "--short",
        "refs/remotes/origin/HEAD",
    ]) {
        if let Some(name) = value
            .trim()
            .strip_prefix("origin/")
            .map(str::trim)
            .filter(|name| !name.is_empty())
        {
            return Some(DefaultBranch {
                name: name.to_string(),
                git_ref: format!("refs/remotes/origin/{name}"),
            });
        }
    }
    for name in ["main", "master"] {
        let git_ref = format!("refs/remotes/origin/{name}");
        if git_ref_exists(run, &git_ref) {
            return Some(DefaultBranch {
                name: name.to_string(),
                git_ref,
            });
        }
    }
    for name in ["main", "master"] {
        let git_ref = format!("refs/heads/{name}");
        if git_ref_exists(run, &git_ref) {
            return Some(DefaultBranch {
                name: name.to_string(),
                git_ref,
            });
        }
    }
    None
}

fn git_ref_exists(run: &dyn Fn(&[&str]) -> Option<String>, git_ref: &str) -> bool {
    run(&["rev-parse", "--verify", "--quiet", git_ref])
        .map(|value| !value.trim().is_empty())
        .unwrap_or(false)
}

/*
The whole per-cwd git answer, expressed over a command runner so the plumbing is
testable against fakes as well as real repositories.

Diff base:
- On a feature branch, the base is `merge-base(default, HEAD)`, so the counts are
  "everything this branch did": commits on the branch PLUS staged PLUS unstaged.
  `git diff <base>` compares a commit against the WORKING TREE, so one command
  covers all three.
- On the default branch itself (or with no default branch, or with unrelated
  histories) the base is HEAD, so the counts are the uncommitted work.

Untracked files are deliberately not counted: they are not part of any diff and
counting them would need a per-file `--no-index` pass whose cost scales with
whatever build output happens to be lying around.
*/
pub fn probe_git_status_with(run: &dyn Fn(&[&str]) -> Option<String>) -> Option<SessionGitProbe> {
    let is_inside_work_tree = run(&["rev-parse", "--is-inside-work-tree"])
        .map(|value| value.trim() == "true")
        .unwrap_or(false);
    if !is_inside_work_tree {
        return None;
    }
    let branch = run(&["symbolic-ref", "--quiet", "--short", "HEAD"])
        .map(|value| value.trim().to_string())
        .filter(|branch| !branch.is_empty());
    let default_branch = resolve_default_branch(run);
    let base = match &default_branch {
        Some(default_branch) if branch.as_deref() != Some(default_branch.name.as_str()) => {
            run(&["merge-base", default_branch.git_ref.as_str(), "HEAD"])
                .map(|value| value.trim().to_string())
                .filter(|base| !base.is_empty())
        }
        _ => None,
    };
    let base = base.unwrap_or_else(|| "HEAD".to_string());
    let numstat = run(&["diff", "--numstat", base.as_str(), "--"]).unwrap_or_default();
    let (additions, deletions) = parse_git_numstat(&numstat);
    Some(SessionGitProbe {
        branch,
        additions,
        deletions,
    })
}

/// `git diff --numstat` is machine output ("adds<TAB>dels<TAB>path"), unlike
/// `--shortstat`, so it survives locales and rename lines. Binary files report
/// `-` and contribute nothing.
pub fn parse_git_numstat(output: &str) -> (i64, i64) {
    let mut additions = 0_i64;
    let mut deletions = 0_i64;
    for line in output.lines() {
        let mut columns = line.split('\t');
        let Some(added) = columns.next() else {
            continue;
        };
        let Some(removed) = columns.next() else {
            continue;
        };
        additions += added.trim().parse::<i64>().unwrap_or(0);
        deletions += removed.trim().parse::<i64>().unwrap_or(0);
    }
    (additions, deletions)
}

/*
`gh pr view --json number,state,url,isDraft`. gh's `state` is OPEN/CLOSED/MERGED
and draft-ness is a separate flag, so a draft is an OPEN PR with `isDraft: true`
while a merged or closed PR keeps its terminal state even if it was a draft. A
PR with no usable number or an unrecognized state is dropped rather than guessed
at: no badge beats a wrong badge, and the auto-settle rule reads the same value.
*/
pub fn parse_gh_pull_request_json(output: &str) -> Option<SessionPullRequest> {
    let value: Value = serde_json::from_str(output.trim()).ok()?;
    let number = value
        .get("number")
        .and_then(Value::as_i64)
        .filter(|number| *number > 0)?;
    let raw_state = value.get("state").and_then(Value::as_str)?;
    let is_draft = value
        .get("isDraft")
        .and_then(Value::as_bool)
        .unwrap_or(false);
    let state = parse_gh_pull_request_state(raw_state, is_draft)?;
    let url = value
        .get("url")
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|url| !url.is_empty())
        .map(str::to_string);
    Some(SessionPullRequest { number, state, url })
}

pub fn parse_gh_pull_request_state(raw_state: &str, is_draft: bool) -> Option<PullRequestState> {
    match raw_state.trim().to_ascii_uppercase().as_str() {
        "MERGED" => Some(PullRequestState::Merged),
        "CLOSED" => Some(PullRequestState::Closed),
        "OPEN" => Some(if is_draft {
            PullRequestState::Draft
        } else {
            PullRequestState::Open
        }),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::{
        path::Path,
        sync::atomic::{AtomicUsize, Ordering},
    };

    // -----------------------------------------------------------------------
    // fakes
    // -----------------------------------------------------------------------

    #[derive(Default)]
    struct FakeProber {
        supports_pull_requests: bool,
        git: Mutex<HashMap<String, Option<SessionGitProbe>>>,
        pull_requests: Mutex<HashMap<String, Option<SessionPullRequest>>>,
        git_probes: AtomicUsize,
        pull_request_probes: AtomicUsize,
    }

    impl FakeProber {
        fn new(supports_pull_requests: bool) -> Self {
            Self {
                supports_pull_requests,
                ..Self::default()
            }
        }

        fn set_git(&self, cwd: &str, probe: Option<SessionGitProbe>) {
            self.git.lock().expect("git").insert(cwd.to_string(), probe);
        }

        fn set_pull_request(&self, cwd: &str, pull_request: Option<SessionPullRequest>) {
            self.pull_requests
                .lock()
                .expect("pull requests")
                .insert(cwd.to_string(), pull_request);
        }
    }

    impl SessionGitStatusProber for FakeProber {
        fn supports_pull_requests(&self) -> bool {
            self.supports_pull_requests
        }

        fn probe_git(&self, cwd: &str) -> Option<SessionGitProbe> {
            self.git_probes.fetch_add(1, Ordering::SeqCst);
            self.git.lock().expect("git").get(cwd).cloned().flatten()
        }

        fn probe_pull_request(&self, cwd: &str, _branch: &str) -> Option<SessionPullRequest> {
            self.pull_request_probes.fetch_add(1, Ordering::SeqCst);
            self.pull_requests
                .lock()
                .expect("pull requests")
                .get(cwd)
                .cloned()
                .flatten()
        }
    }

    fn probe(branch: Option<&str>, additions: i64, deletions: i64) -> SessionGitProbe {
        SessionGitProbe {
            branch: branch.map(str::to_string),
            additions,
            deletions,
        }
    }

    fn clock(monotonic_now_ms: i64) -> SessionGitStatusRefreshClock {
        let base = chrono::DateTime::parse_from_rfc3339("2026-07-29T12:00:00.000Z").expect("base");
        let now = base + chrono::Duration::milliseconds(monotonic_now_ms);
        SessionGitStatusRefreshClock {
            monotonic_now_ms,
            now_iso: now.to_rfc3339_opts(chrono::SecondsFormat::Millis, true),
        }
    }

    fn cache() -> Mutex<SessionGitStatusCache> {
        Mutex::new(SessionGitStatusCache::default())
    }

    fn status_of(cache: &Mutex<SessionGitStatusCache>, cwd: &str) -> Option<SessionGitStatus> {
        cache.lock().expect("cache").get(cwd)
    }

    // -----------------------------------------------------------------------
    // wire shape
    // -----------------------------------------------------------------------

    #[test]
    fn published_git_status_matches_the_p3_wire_contract() {
        let status = SessionGitStatus {
            branch: Some("ghostex/a1b2c3d4".to_string()),
            additions: 128,
            deletions: 7,
            pull_request: Some(SessionPullRequest {
                number: 42,
                state: PullRequestState::Draft,
                url: Some("https://github.com/o/r/pull/42".to_string()),
            }),
            updated_at: "2026-07-29T12:00:00.000Z".to_string(),
        };
        assert_eq!(
            status.to_presentation_value(),
            json!({
                "additions": 128,
                "branch": "ghostex/a1b2c3d4",
                "deletions": 7,
                "prNumber": 42,
                "prState": "draft",
                "prUrl": "https://github.com/o/r/pull/42",
                "updatedAt": "2026-07-29T12:00:00.000Z",
            })
        );

        let detached = SessionGitStatus {
            branch: None,
            additions: 0,
            deletions: 0,
            pull_request: None,
            updated_at: "2026-07-29T12:00:00.000Z".to_string(),
        };
        assert_eq!(
            detached.to_presentation_value(),
            json!({
                "additions": 0,
                "branch": Value::Null,
                "deletions": 0,
                "updatedAt": "2026-07-29T12:00:00.000Z",
            }),
            "a detached checkout publishes an explicit null branch and no PR keys"
        );
    }

    #[test]
    fn gh_pull_request_states_map_onto_the_wire_vocabulary() {
        for (raw_state, is_draft, expected) in [
            ("OPEN", false, Some(PullRequestState::Open)),
            ("OPEN", true, Some(PullRequestState::Draft)),
            ("MERGED", false, Some(PullRequestState::Merged)),
            ("MERGED", true, Some(PullRequestState::Merged)),
            ("CLOSED", false, Some(PullRequestState::Closed)),
            ("CLOSED", true, Some(PullRequestState::Closed)),
            ("open", false, Some(PullRequestState::Open)),
            ("QUEUED", false, None),
            ("", false, None),
        ] {
            assert_eq!(
                parse_gh_pull_request_state(raw_state, is_draft),
                expected,
                "gh state {raw_state} (draft: {is_draft})"
            );
        }

        let parsed = parse_gh_pull_request_json(
            r#"{"isDraft":false,"number":12,"state":"MERGED","url":"https://github.com/o/r/pull/12"}"#,
        )
        .expect("merged pull request");
        assert_eq!(parsed.number, 12);
        assert_eq!(parsed.state, PullRequestState::Merged);
        assert_eq!(
            parsed.url.as_deref(),
            Some("https://github.com/o/r/pull/12")
        );
        assert_eq!(parsed.state.disposition(), PullRequestDisposition::Finished);

        assert_eq!(
            PullRequestState::Open.disposition(),
            PullRequestDisposition::Open
        );
        assert_eq!(
            PullRequestState::Draft.disposition(),
            PullRequestDisposition::Open
        );
        assert_eq!(
            PullRequestState::Closed.disposition(),
            PullRequestDisposition::Finished
        );

        assert!(parse_gh_pull_request_json("no pull requests found").is_none());
        assert!(parse_gh_pull_request_json(r#"{"state":"OPEN"}"#).is_none());
        assert!(parse_gh_pull_request_json(r#"{"number":0,"state":"OPEN"}"#).is_none());
        assert!(
            parse_gh_pull_request_json(r#"{"number":3,"state":"OPEN"}"#)
                .expect("url-less pull request")
                .url
                .is_none(),
            "a PR without a url still publishes its number and state"
        );
    }

    // -----------------------------------------------------------------------
    // pure git plumbing
    // -----------------------------------------------------------------------

    fn scripted_git(
        answers: Vec<(&'static str, &'static str)>,
    ) -> impl Fn(&[&str]) -> Option<String> {
        move |args: &[&str]| {
            let joined = args.join(" ");
            answers
                .iter()
                .find(|(command, _)| *command == joined)
                .map(|(_, output)| (*output).to_string())
        }
    }

    #[test]
    fn default_branch_prefers_origin_head_then_remote_then_local_fallbacks() {
        let run = scripted_git(vec![(
            "symbolic-ref --quiet --short refs/remotes/origin/HEAD",
            "origin/trunk",
        )]);
        assert_eq!(
            resolve_default_branch(&run),
            Some(DefaultBranch {
                name: "trunk".to_string(),
                git_ref: "refs/remotes/origin/trunk".to_string(),
            })
        );

        let run = scripted_git(vec![(
            "rev-parse --verify --quiet refs/remotes/origin/master",
            "0f1e2d3",
        )]);
        assert_eq!(
            resolve_default_branch(&run),
            Some(DefaultBranch {
                name: "master".to_string(),
                git_ref: "refs/remotes/origin/master".to_string(),
            }),
            "a clone with no origin/HEAD falls back to origin/main then origin/master"
        );

        let run = scripted_git(vec![
            ("rev-parse --verify --quiet refs/remotes/origin/main", "aaa"),
            (
                "rev-parse --verify --quiet refs/remotes/origin/master",
                "bbb",
            ),
        ]);
        assert_eq!(
            resolve_default_branch(&run).map(|branch| branch.name),
            Some("main".to_string()),
            "main wins over master when both remote branches exist"
        );

        let run = scripted_git(vec![(
            "rev-parse --verify --quiet refs/heads/master",
            "ccc",
        )]);
        assert_eq!(
            resolve_default_branch(&run),
            Some(DefaultBranch {
                name: "master".to_string(),
                git_ref: "refs/heads/master".to_string(),
            }),
            "a repository with no remote falls back to its local default branch"
        );

        assert_eq!(
            resolve_default_branch(&scripted_git(vec![])),
            None,
            "no recognizable default branch is not an error"
        );
        assert_eq!(
            resolve_default_branch(&scripted_git(vec![(
                "symbolic-ref --quiet --short refs/remotes/origin/HEAD",
                "origin/",
            )])),
            None,
            "a malformed origin/HEAD does not produce an empty branch name"
        );
    }

    #[test]
    fn numstat_parsing_sums_text_diffs_and_ignores_binaries() {
        assert_eq!(parse_git_numstat(""), (0, 0));
        assert_eq!(
            parse_git_numstat("12\t4\tsrc/a.rs\n0\t9\tsrc/b.rs\n-\t-\tassets/icon.png\n"),
            (12, 13)
        );
        assert_eq!(
            parse_git_numstat("3\t1\tsrc/{old => new}/a.rs\n"),
            (3, 1),
            "rename lines still carry their counts"
        );
    }

    #[test]
    fn a_directory_outside_a_repository_is_not_probed_further() {
        let run = scripted_git(vec![("rev-parse --is-inside-work-tree", "false")]);
        assert_eq!(probe_git_status_with(&run), None);
        assert_eq!(probe_git_status_with(&scripted_git(vec![])), None);
    }

    #[test]
    fn a_detached_checkout_reports_a_null_branch_and_still_diffs() {
        let run = scripted_git(vec![
            ("rev-parse --is-inside-work-tree", "true"),
            (
                "symbolic-ref --quiet --short refs/remotes/origin/HEAD",
                "origin/main",
            ),
            ("merge-base refs/remotes/origin/main HEAD", "deadbeef"),
            ("diff --numstat deadbeef --", "5\t2\tsrc/a.rs"),
        ]);
        assert_eq!(
            probe_git_status_with(&run),
            Some(probe(None, 5, 2)),
            "symbolic-ref fails on a detached HEAD, which is a null branch, not a failed probe"
        );
    }

    // -----------------------------------------------------------------------
    // real repositories
    // -----------------------------------------------------------------------

    fn git_available() -> bool {
        Command::new("git")
            .arg("--version")
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .status()
            .map(|status| status.success())
            .unwrap_or(false)
    }

    fn git(cwd: &Path, args: &[&str]) {
        let output = Command::new("git")
            .args(args)
            .current_dir(cwd)
            .env("GIT_AUTHOR_NAME", "Ghostex Tests")
            .env("GIT_AUTHOR_EMAIL", "ghostex-tests@example.invalid")
            .env("GIT_COMMITTER_NAME", "Ghostex Tests")
            .env("GIT_COMMITTER_EMAIL", "ghostex-tests@example.invalid")
            .output()
            .expect("git command");
        assert!(
            output.status.success(),
            "git {:?} failed: {}",
            args,
            String::from_utf8_lossy(&output.stderr)
        );
    }

    fn create_repository(root: &Path, default_branch: &str) {
        std::fs::create_dir_all(root).expect("repo dir");
        git(root, &["init", "--quiet", "-b", default_branch]);
        git(root, &["config", "user.name", "Ghostex Tests"]);
        git(
            root,
            &["config", "user.email", "ghostex-tests@example.invalid"],
        );
        std::fs::write(root.join("base.txt"), "1\n2\n3\n").expect("base file");
        git(root, &["add", "."]);
        git(root, &["commit", "--quiet", "-m", "base"]);
    }

    fn probe_repository(root: &Path) -> Option<SessionGitProbe> {
        SystemSessionGitStatusProber.probe_git(&root.to_string_lossy())
    }

    #[test]
    fn branch_diff_counts_committed_staged_and_unstaged_work_against_the_merge_base() {
        if !git_available() {
            return;
        }
        let temp = tempfile::tempdir().expect("tempdir");
        let root = temp.path().join("repo");
        create_repository(&root, "main");

        git(&root, &["checkout", "--quiet", "-b", "feature"]);
        std::fs::write(root.join("committed.txt"), "a\nb\n").expect("committed file");
        git(&root, &["add", "committed.txt"]);
        git(&root, &["commit", "--quiet", "-m", "committed work"]);

        // main advancing after the branch point must not leak into the branch's
        // counts: that is exactly what the merge-base is for.
        git(&root, &["checkout", "--quiet", "main"]);
        std::fs::write(root.join("mainline.txt"), "x\ny\nz\n").expect("mainline file");
        git(&root, &["add", "mainline.txt"]);
        git(&root, &["commit", "--quiet", "-m", "mainline work"]);
        git(&root, &["checkout", "--quiet", "feature"]);

        std::fs::write(root.join("staged.txt"), "c\n").expect("staged file");
        git(&root, &["add", "staged.txt"]);
        std::fs::write(root.join("base.txt"), "1\n2\n3\n4\n").expect("unstaged edit");

        let probed = probe_repository(&root).expect("feature branch probe");
        assert_eq!(probed.branch.as_deref(), Some("feature"));
        assert_eq!(
            (probed.additions, probed.deletions),
            (4, 0),
            "2 committed + 1 staged + 1 unstaged insertion, and nothing from main"
        );

        // Untracked files are not part of any diff and are not counted.
        std::fs::write(root.join("scratch.txt"), "ignored\n").expect("untracked file");
        let probed = probe_repository(&root).expect("feature branch probe");
        assert_eq!((probed.additions, probed.deletions), (4, 0));
    }

    #[test]
    fn the_default_branch_diffs_its_working_tree_against_head() {
        if !git_available() {
            return;
        }
        let temp = tempfile::tempdir().expect("tempdir");
        let root = temp.path().join("repo");
        create_repository(&root, "master");

        let probed = probe_repository(&root).expect("clean probe");
        assert_eq!(probed.branch.as_deref(), Some("master"));
        assert_eq!((probed.additions, probed.deletions), (0, 0));

        std::fs::write(root.join("base.txt"), "1\n2\n").expect("delete a line");
        std::fs::write(root.join("added.txt"), "new\n").expect("new file");
        git(&root, &["add", "added.txt"]);
        let probed = probe_repository(&root).expect("dirty probe");
        assert_eq!(
            (probed.additions, probed.deletions),
            (1, 1),
            "on the default branch the counts are the uncommitted work, resolved via the local master fallback"
        );
    }

    #[test]
    fn a_non_repository_directory_probes_as_no_git_status() {
        if !git_available() {
            return;
        }
        let temp = tempfile::tempdir().expect("tempdir");
        assert_eq!(probe_repository(temp.path()), None);
        assert_eq!(
            SystemSessionGitStatusProber.probe_git("/definitely/not/a/real/path"),
            None,
            "a deleted cwd degrades to no git status instead of failing the pass"
        );
    }

    // -----------------------------------------------------------------------
    // cache behavior
    // -----------------------------------------------------------------------

    #[test]
    fn one_probe_per_unique_cwd_serves_every_session_that_shares_it() {
        let cache = cache();
        let prober = FakeProber::new(false);
        prober.set_git("/repo", Some(probe(Some("feature"), 3, 1)));

        // Three sessions, one checkout: the caller may hand the same cwd in
        // several times and it still costs exactly one probe.
        let cwds = vec![
            "/repo".to_string(),
            "/repo".to_string(),
            " /repo ".to_string(),
        ];
        let changed = run_session_git_status_refresh_pass(&cache, &cwds, &prober, &clock(0), true);

        assert_eq!(prober.git_probes.load(Ordering::SeqCst), 1);
        assert_eq!(changed, vec!["/repo".to_string()]);
        let status = status_of(&cache, "/repo").expect("status");
        assert_eq!(status.branch.as_deref(), Some("feature"));
        assert_eq!((status.additions, status.deletions), (3, 1));
    }

    /*
    CDXC:SidebarV2DataGate 2026-07-29:
    The version gate, asserted where the cost actually is. A V1 machine renders
    none of this data, so its pass must spawn no git, make no `gh` network call,
    publish nothing — and evict nothing either, so entries an earlier V2 stretch
    left behind survive. The second half is the flip: the very next pass after
    the user selects V2 probes normally, which is what makes the setting take
    effect within one interval instead of at the next daemon restart.
    */
    #[test]
    fn sidebar_v1_probes_nothing_and_flipping_to_v2_warms_in_the_next_pass() {
        let cache = cache();
        let prober = FakeProber::new(true);
        prober.set_git("/repo", Some(probe(Some("feature"), 3, 1)));
        prober.set_pull_request(
            "/repo",
            Some(SessionPullRequest {
                number: 7,
                state: PullRequestState::Open,
                url: None,
            }),
        );
        cache.lock().expect("cache").set(
            "/gone",
            Some(SessionGitStatus {
                branch: Some("left-over".to_string()),
                additions: 0,
                deletions: 0,
                pull_request: None,
                updated_at: "2026-07-29T12:00:00.000Z".to_string(),
            }),
            0,
        );
        let cwds = vec!["/repo".to_string()];

        let changed = run_session_git_status_refresh_pass(&cache, &cwds, &prober, &clock(0), false);
        assert!(
            changed.is_empty(),
            "a gated pass publishes no delta: {changed:?}"
        );
        assert_eq!(
            prober.git_probes.load(Ordering::SeqCst),
            0,
            "a machine on Sidebar V1 spawns no git"
        );
        assert_eq!(
            prober.pull_request_probes.load(Ordering::SeqCst),
            0,
            "and makes no `gh` network call"
        );
        assert!(
            status_of(&cache, "/repo").is_none(),
            "nothing is probed, so nothing is cached"
        );
        assert!(
            status_of(&cache, "/gone").is_some(),
            "a gated pass evicts nothing either — leaving stale entries costs nothing, dropping them would be work"
        );

        let changed = run_session_git_status_refresh_pass(&cache, &cwds, &prober, &clock(0), true);
        assert_eq!(
            changed,
            vec!["/repo".to_string()],
            "the first pass after the flip warms the cache and publishes"
        );
        assert_eq!(prober.git_probes.load(Ordering::SeqCst), 1);
        assert_eq!(prober.pull_request_probes.load(Ordering::SeqCst), 1);
        assert_eq!(
            status_of(&cache, "/repo").and_then(|status| status.branch),
            Some("feature".to_string())
        );
        assert!(
            status_of(&cache, "/gone").is_none(),
            "and normal eviction resumes with it"
        );
    }

    #[test]
    fn cached_entries_survive_until_their_ttl_and_negative_entries_last_longer() {
        let cache = cache();
        let prober = FakeProber::new(false);
        prober.set_git("/repo", Some(probe(Some("main"), 1, 0)));
        prober.set_git("/plain", None);
        let cwds = vec!["/repo".to_string(), "/plain".to_string()];

        run_session_git_status_refresh_pass(&cache, &cwds, &prober, &clock(0), true);
        assert_eq!(prober.git_probes.load(Ordering::SeqCst), 2);
        assert!(
            status_of(&cache, "/plain").is_none(),
            "a directory outside a repository caches as a negative entry"
        );

        run_session_git_status_refresh_pass(
            &cache,
            &cwds,
            &prober,
            &clock(GIT_STATUS_TTL_MS - 1),
            true,
        );
        assert_eq!(
            prober.git_probes.load(Ordering::SeqCst),
            2,
            "nothing is re-probed inside the TTL"
        );

        run_session_git_status_refresh_pass(
            &cache,
            &cwds,
            &prober,
            &clock(GIT_STATUS_TTL_MS),
            true,
        );
        assert_eq!(
            prober.git_probes.load(Ordering::SeqCst),
            3,
            "only the repository is due; the negative entry has a longer TTL"
        );

        run_session_git_status_refresh_pass(
            &cache,
            &cwds,
            &prober,
            &clock(NON_REPOSITORY_STATUS_TTL_MS),
            true,
        );
        assert_eq!(
            prober.git_probes.load(Ordering::SeqCst),
            5,
            "past the negative TTL both entries are due again"
        );
    }

    #[test]
    fn a_cwd_no_live_session_points_at_is_dropped_from_the_cache() {
        let cache = cache();
        let prober = FakeProber::new(false);
        prober.set_git("/repo", Some(probe(Some("main"), 0, 0)));
        prober.set_git("/other", Some(probe(Some("main"), 0, 0)));

        run_session_git_status_refresh_pass(
            &cache,
            &["/repo".to_string(), "/other".to_string()],
            &prober,
            &clock(0),
            true,
        );
        assert_eq!(cache.lock().expect("cache").len(), 2);

        run_session_git_status_refresh_pass(
            &cache,
            &["/repo".to_string()],
            &prober,
            &clock(1),
            true,
        );
        assert_eq!(cache.lock().expect("cache").len(), 1);
        assert!(status_of(&cache, "/other").is_none());
    }

    #[test]
    fn a_pass_reports_only_the_cwds_whose_status_actually_changed() {
        let cache = cache();
        let prober = FakeProber::new(false);
        prober.set_git("/a", Some(probe(Some("main"), 1, 1)));
        prober.set_git("/b", Some(probe(Some("main"), 2, 2)));
        let cwds = vec!["/a".to_string(), "/b".to_string()];

        let changed = run_session_git_status_refresh_pass(&cache, &cwds, &prober, &clock(0), true);
        assert_eq!(changed.len(), 2, "the first pass is a change for both");

        let changed = run_session_git_status_refresh_pass(
            &cache,
            &cwds,
            &prober,
            &clock(GIT_STATUS_TTL_MS),
            true,
        );
        assert!(
            changed.is_empty(),
            "an identical re-probe must not emit presentation deltas, even though updatedAt moved"
        );
        assert_ne!(
            status_of(&cache, "/a").expect("status").updated_at,
            clock(0).now_iso,
            "the freshness stamp still follows the probe"
        );

        prober.set_git("/b", Some(probe(Some("main"), 9, 2)));
        let changed = run_session_git_status_refresh_pass(
            &cache,
            &cwds,
            &prober,
            &clock(2 * GIT_STATUS_TTL_MS),
            true,
        );
        assert_eq!(changed, vec!["/b".to_string()]);

        prober.set_git("/a", None);
        let changed = run_session_git_status_refresh_pass(
            &cache,
            &cwds,
            &prober,
            &clock(3 * GIT_STATUS_TTL_MS),
            true,
        );
        assert_eq!(
            changed,
            vec!["/a".to_string()],
            "losing a repository is a change too"
        );
    }

    #[test]
    fn pull_requests_are_probed_only_when_gh_is_usable_and_reused_until_their_own_ttl() {
        let cache = cache();
        let without_gh = FakeProber::new(false);
        without_gh.set_git("/repo", Some(probe(Some("feature"), 1, 0)));
        without_gh.set_pull_request(
            "/repo",
            Some(SessionPullRequest {
                number: 7,
                state: PullRequestState::Open,
                url: None,
            }),
        );
        run_session_git_status_refresh_pass(
            &cache,
            &["/repo".to_string()],
            &without_gh,
            &clock(0),
            true,
        );
        assert_eq!(without_gh.pull_request_probes.load(Ordering::SeqCst), 0);
        assert!(
            status_of(&cache, "/repo")
                .expect("status")
                .pull_request
                .is_none(),
            "no gh means no PR fields, not an error"
        );

        let cache = Mutex::new(SessionGitStatusCache::default());
        let prober = FakeProber::new(true);
        prober.set_git("/repo", Some(probe(Some("feature"), 1, 0)));
        prober.set_pull_request(
            "/repo",
            Some(SessionPullRequest {
                number: 7,
                state: PullRequestState::Open,
                url: Some("https://github.com/o/r/pull/7".to_string()),
            }),
        );
        let cwds = vec!["/repo".to_string()];

        run_session_git_status_refresh_pass(&cache, &cwds, &prober, &clock(0), true);
        assert_eq!(prober.pull_request_probes.load(Ordering::SeqCst), 1);
        assert_eq!(
            status_of(&cache, "/repo")
                .expect("status")
                .pull_request
                .expect("pull request")
                .number,
            7
        );

        run_session_git_status_refresh_pass(
            &cache,
            &cwds,
            &prober,
            &clock(GIT_STATUS_TTL_MS),
            true,
        );
        assert_eq!(
            prober.pull_request_probes.load(Ordering::SeqCst),
            1,
            "a git refresh inside the PR TTL reuses the last gh answer"
        );

        run_session_git_status_refresh_pass(
            &cache,
            &cwds,
            &prober,
            &clock(PULL_REQUEST_TTL_MS),
            true,
        );
        assert_eq!(
            prober.pull_request_probes.load(Ordering::SeqCst),
            2,
            "past the PR TTL gh is asked again"
        );

        // A branch switch invalidates the cached PR immediately.
        prober.set_git("/repo", Some(probe(Some("other"), 1, 0)));
        run_session_git_status_refresh_pass(
            &cache,
            &cwds,
            &prober,
            &clock(PULL_REQUEST_TTL_MS + GIT_STATUS_TTL_MS),
            true,
        );
        assert_eq!(
            prober.pull_request_probes.load(Ordering::SeqCst),
            3,
            "the PR answer belongs to the branch it was asked about"
        );

        // A detached checkout has no branch to ask about at all.
        prober.set_git("/repo", Some(probe(None, 1, 0)));
        run_session_git_status_refresh_pass(
            &cache,
            &cwds,
            &prober,
            &clock(PULL_REQUEST_TTL_MS + 2 * GIT_STATUS_TTL_MS),
            true,
        );
        assert_eq!(prober.pull_request_probes.load(Ordering::SeqCst), 3);
        assert!(status_of(&cache, "/repo")
            .expect("status")
            .pull_request
            .is_none());
    }

    #[test]
    fn a_pass_is_bounded_so_a_large_machine_spreads_its_work() {
        let cache = cache();
        let prober = FakeProber::new(true);
        let cwds: Vec<String> = (0..(MAX_GIT_PROBES_PER_PASS + 5))
            .map(|index| {
                let cwd = format!("/repo-{index:03}");
                prober.set_git(&cwd, Some(probe(Some("feature"), 1, 0)));
                prober.set_pull_request(
                    &cwd,
                    Some(SessionPullRequest {
                        number: index as i64 + 1,
                        state: PullRequestState::Open,
                        url: None,
                    }),
                );
                cwd
            })
            .collect();

        run_session_git_status_refresh_pass(&cache, &cwds, &prober, &clock(0), true);
        assert_eq!(
            prober.git_probes.load(Ordering::SeqCst),
            MAX_GIT_PROBES_PER_PASS
        );
        assert_eq!(
            prober.pull_request_probes.load(Ordering::SeqCst),
            MAX_PULL_REQUEST_PROBES_PER_PASS
        );

        // Never-probed cwds are the oldest, so the leftovers go first next pass.
        run_session_git_status_refresh_pass(&cache, &cwds, &prober, &clock(1), true);
        assert_eq!(
            prober.git_probes.load(Ordering::SeqCst),
            MAX_GIT_PROBES_PER_PASS + 5
        );
    }

    #[test]
    fn session_pull_request_disposition_reads_the_cwd_cache() {
        let unique = "/tmp/ghostex-session-git-status-disposition";
        let session = json!({ "cwd": unique, "sessionId": "G1" });
        assert_eq!(
            session_pull_request_disposition(&session, None),
            PullRequestDisposition::Unknown,
            "an unprobed cwd never settles anything"
        );

        set_cached_session_git_status_for_test(
            unique,
            Some(SessionGitStatus {
                branch: Some("feature".to_string()),
                additions: 0,
                deletions: 0,
                pull_request: None,
                updated_at: "2026-07-29T12:00:00.000Z".to_string(),
            }),
        );
        assert_eq!(
            session_pull_request_disposition(&session, None),
            PullRequestDisposition::Unknown,
            "a branch with no pull request is not a finished pull request"
        );

        for (state, expected) in [
            (PullRequestState::Open, PullRequestDisposition::Open),
            (PullRequestState::Draft, PullRequestDisposition::Open),
            (PullRequestState::Merged, PullRequestDisposition::Finished),
            (PullRequestState::Closed, PullRequestDisposition::Finished),
        ] {
            set_cached_session_git_status_for_test(
                unique,
                Some(SessionGitStatus {
                    branch: Some("feature".to_string()),
                    additions: 0,
                    deletions: 0,
                    pull_request: Some(SessionPullRequest {
                        number: 5,
                        state,
                        url: None,
                    }),
                    updated_at: "2026-07-29T12:00:00.000Z".to_string(),
                }),
            );
            assert_eq!(session_pull_request_disposition(&session, None), expected);
        }

        assert_eq!(
            session_pull_request_disposition(&json!({ "sessionId": "G2" }), None),
            PullRequestDisposition::Unknown
        );
        assert_eq!(
            session_pull_request_disposition(&json!({ "cwd": "   ", "sessionId": "G3" }), None),
            PullRequestDisposition::Unknown
        );
    }

    #[test]
    fn session_pull_request_disposition_falls_back_to_the_project_path() {
        /*
        CDXC:SidebarV2GitStatus 2026-07-30 (effective cwd):
        An agent session carries no cwd, so PR-driven auto-settle must read the
        cache entry for its PROJECT path — otherwise the whole auto-settle trigger
        is dead for every agent row on the machine.
        */
        let project_path = "/tmp/ghostex-session-git-status-disposition-project";
        set_cached_session_git_status_for_test(
            project_path,
            Some(SessionGitStatus {
                branch: Some("main".to_string()),
                additions: 0,
                deletions: 0,
                pull_request: Some(SessionPullRequest {
                    number: 9,
                    state: PullRequestState::Merged,
                    url: None,
                }),
                updated_at: "2026-07-30T12:00:00.000Z".to_string(),
            }),
        );
        let session = json!({ "projectId": "P1", "sessionId": "G4" });
        let project = json!({ "path": project_path, "projectId": "P1" });

        assert_eq!(
            session_pull_request_disposition(&session, Some(&project)),
            PullRequestDisposition::Finished,
            "a cwd-less agent session resolves its project's checkout"
        );
        assert_eq!(
            session_pull_request_disposition(&session, None),
            PullRequestDisposition::Unknown,
            "with no project to resolve, a cwd-less session still settles nothing"
        );
    }

    #[test]
    fn session_cwd_keys_are_trimmed_and_never_empty() {
        assert_eq!(
            session_cwd_key(&json!({ "cwd": "  /repo  " })),
            Some("/repo".to_string())
        );
        assert_eq!(session_cwd_key(&json!({ "cwd": "" })), None);
        assert_eq!(session_cwd_key(&json!({ "cwd": Value::Null })), None);
        assert_eq!(session_cwd_key(&json!({})), None);
    }

    #[test]
    fn effective_session_git_cwds_fall_back_to_the_project_path() {
        /*
        CDXC:SidebarV2GitStatus 2026-07-30 (effective cwd):
        The same rule `zmx.rs`/`agents.rs` launch with: an explicit session cwd
        wins, anything blank falls through to the project's path, and a project
        with no usable path resolves nothing at all (no probe, no key).
        */
        let project = json!({ "path": "  /repo/project  ", "projectId": "P1" });

        assert_eq!(
            effective_session_git_cwd(&json!({ "cwd": " /repo/worktree " }), Some(&project)),
            Some("/repo/worktree".to_string()),
            "an explicit session cwd always wins"
        );
        assert_eq!(
            effective_session_git_cwd(&json!({ "cwd": " /repo/worktree " }), None),
            Some("/repo/worktree".to_string())
        );
        for blank in [
            json!({}),
            json!({ "cwd": Value::Null }),
            json!({ "cwd": "  " }),
        ] {
            assert_eq!(
                effective_session_git_cwd(&blank, Some(&project)),
                Some("/repo/project".to_string()),
                "a session with no cwd of its own runs in its project's path"
            );
            assert_eq!(
                effective_session_git_cwd(&blank, None),
                None,
                "no session cwd and no project resolves nothing"
            );
        }
        assert_eq!(
            effective_session_git_cwd(&json!({}), Some(&json!({ "projectId": "P2" }))),
            None,
            "a project with no path resolves nothing"
        );
        assert_eq!(
            effective_session_git_cwd(&json!({}), Some(&json!({ "path": "   " }))),
            None,
            "a blank project path resolves nothing"
        );
        assert_eq!(
            effective_session_git_cwd(
                &json!({}),
                Some(&json!({
                    "path": "/repo/worktree-checkout",
                    "worktree": { "parentProjectPath": "/repo/project" },
                })),
            ),
            Some("/repo/worktree-checkout".to_string()),
            "a worktree project probes its OWN checkout, not the family root"
        );
    }

    // -----------------------------------------------------------------------
    // subprocess capture
    // -----------------------------------------------------------------------

    #[cfg(unix)]
    fn shell_command(script: &str) -> Command {
        let mut command = Command::new("/bin/sh");
        command.arg("-c").arg(script);
        command
    }

    #[cfg(unix)]
    #[test]
    fn command_capture_returns_trimmed_stdout() {
        let output = run_command_bounded(
            shell_command("echo '  hello  '"),
            Duration::from_secs(10),
            Duration::from_secs(5),
            &ABANDONED_COMMAND_READERS,
        );
        assert_eq!(output.as_deref(), Some("hello"));
    }

    #[cfg(unix)]
    #[test]
    fn command_capture_reports_a_failed_command_as_no_output() {
        let output = run_command_bounded(
            shell_command("echo partial; exit 3"),
            Duration::from_secs(10),
            Duration::from_secs(5),
            &ABANDONED_COMMAND_READERS,
        );
        assert_eq!(output, None, "a non-zero exit is not a git status");
    }

    /*
    The reason the draining thread exists at all: this output is far larger than
    a pipe buffer, so a parent that waited for exit before reading would deadlock
    against a child blocked writing into a full pipe.
    */
    #[cfg(unix)]
    #[test]
    fn command_capture_drains_output_larger_than_a_pipe_buffer() {
        let output = run_command_bounded(
            shell_command("seq 1 200000"),
            Duration::from_secs(30),
            Duration::from_secs(10),
            &ABANDONED_COMMAND_READERS,
        )
        .expect("large output");
        assert!(
            output.len() > 1_000_000,
            "expected a multi-megabyte capture"
        );
        assert!(output.starts_with("1\n2\n"));
        assert!(output.ends_with("\n200000"));
    }

    #[cfg(unix)]
    #[test]
    fn command_capture_times_out_without_waiting_for_the_reader() {
        let started = Instant::now();
        // Backgrounded work inherits stdout, so the pipe outlives the shell:
        // the same shape as a diff driver surviving a killed `git`.
        let output = run_command_bounded(
            shell_command("sleep 30 & sleep 30"),
            Duration::from_millis(200),
            Duration::from_secs(30),
            &ABANDONED_COMMAND_READERS,
        );
        assert_eq!(output, None);
        assert!(
            started.elapsed() < Duration::from_secs(10),
            "a timed-out command must not wait on its output reader"
        );
    }

    /*
    MINOR-1 regression: the command itself succeeds and is reaped, but something
    it left behind still holds the write end of the pipe, so `read_to_end` never
    returns. The parent must give up on the reader instead of blocking the
    refresh pass forever.
    */
    #[cfg(unix)]
    #[test]
    fn command_capture_abandons_a_reader_whose_pipe_outlived_the_command() {
        let _ = take_abandoned_command_readers();
        let started = Instant::now();
        let output = run_command_bounded(
            shell_command("sleep 10 & echo done"),
            Duration::from_secs(10),
            Duration::from_millis(200),
            &ABANDONED_COMMAND_READERS,
        );
        assert_eq!(output, None, "an undrainable command has no git status");
        assert!(
            started.elapsed() < Duration::from_secs(5),
            "the bounded wait must return long before the stranded pipe closes"
        );
        assert!(
            take_abandoned_command_readers() >= 1,
            "the abandoned reader must be counted so the pass can log it"
        );
    }

    /*
    CDXC:SidebarV2LogicalProjects 2026-07-29:
    The two probe surfaces share the runner but NOT the counter, so a leak under
    the `origin` probe is logged as `projectGitRemoteReaderAbandoned` instead of
    being reported under the git-status pass's event name.
    */
    #[cfg(unix)]
    #[test]
    fn each_probe_surface_counts_its_own_abandoned_readers() {
        /*
        Only the git-remote counter is asserted on, because it is the one this
        test owns: the git-status counter is process-wide and another test may
        be leaking into it concurrently.
        */
        let _ = take_abandoned_project_git_remote_readers();

        assert_eq!(
            run_command_bounded(
                shell_command("sleep 10 & echo done"),
                Duration::from_secs(10),
                Duration::from_millis(200),
                &ABANDONED_COMMAND_READERS,
            ),
            None
        );
        assert_eq!(
            take_abandoned_project_git_remote_readers(),
            0,
            "a git-status leak must not be attributed to the origin probe"
        );

        assert_eq!(
            run_command_bounded(
                shell_command("sleep 10 & echo done"),
                Duration::from_secs(10),
                Duration::from_millis(200),
                &ABANDONED_PROJECT_GIT_REMOTE_READERS,
            ),
            None
        );
        assert!(
            take_abandoned_project_git_remote_readers() >= 1,
            "an origin-probe leak must reach its own counter so it can be logged \
             as projectGitRemoteReaderAbandoned"
        );
        assert_eq!(
            take_abandoned_project_git_remote_readers(),
            0,
            "draining the counter resets it, so each pass logs only new leaks"
        );
    }
}
