use super::*;

/*
One refresh pass over `cwds`. The cache lock is taken twice — to plan and to
merge — and never held while a subprocess runs. Returns the cwds whose published
status changed, which is exactly the set the caller turns into presentation
deltas.

CDXC:StateSync 2026-07-29:
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
    // Git first; the pull requests due are then asked for together (`probe_pull_requests`).
    let mut probed: Vec<(String, Option<SessionGitProbe>, PullRequestPlan)> =
        Vec::with_capacity(targets.len());
    let mut wanted: Vec<(String, String)> = Vec::new();
    for target in targets {
        let Some(probe) = prober.probe_git(&target.cwd) else {
            probed.push((target.cwd, None, PullRequestPlan::Unknown));
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

        let mut plan = PullRequestPlan::Unknown;
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
                    plan = PullRequestPlan::Kept(
                        previous_pull_request.clone(),
                        previous_pull_request_probed_at_ms,
                    );
                } else if pull_request_budget > 0 {
                    pull_request_budget -= 1;
                    plan = PullRequestPlan::Asked(wanted.len());
                    wanted.push((target.cwd.clone(), branch.to_string()));
                } else if previous_branch == Some(branch) {
                    // Out of network budget: keep the last answer AND its stamp,
                    // so the next pass still sees it as due.
                    plan = PullRequestPlan::Kept(
                        previous_pull_request.clone(),
                        previous_pull_request_probed_at_ms,
                    );
                }
            }
        }
        probed.push((target.cwd, Some(probe), plan));
    }
    let mut answers = if wanted.is_empty() {
        Vec::new()
    } else {
        prober.probe_pull_requests(&wanted)
    };
    answers.resize(wanted.len(), None);

    let mut results = Vec::with_capacity(probed.len());
    for (cwd, probe, plan) in probed {
        let Some(probe) = probe else {
            results.push((
                cwd,
                SessionGitStatusEntry {
                    git_probed_at_ms: clock.monotonic_now_ms,
                    pull_request_probed_at_ms: None,
                    status: None,
                },
            ));
            continue;
        };
        let (pull_request, pull_request_probed_at_ms) = match plan {
            PullRequestPlan::Unknown => (None, None),
            PullRequestPlan::Kept(pull_request, probed_at_ms) => (pull_request, probed_at_ms),
            PullRequestPlan::Asked(index) => (answers[index].take(), Some(clock.monotonic_now_ms)),
        };
        results.push((
            cwd,
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

/// What a pass does about one checkout's pull request.
enum PullRequestPlan {
    /// No answer: no `gh`, no branch, or a new branch past the budget.
    Unknown,
    /// The last answer and its stamp, still good (or kept while out of budget).
    Kept(Option<SessionPullRequest>, Option<i64>),
    /// Asked this pass, at this index of the wanted list.
    Asked(usize),
}

// ---------------------------------------------------------------------------
// process-wide cache
// ---------------------------------------------------------------------------

fn session_git_status_cache() -> &'static Mutex<SessionGitStatusCache> {
    static CACHE: OnceLock<Mutex<SessionGitStatusCache>> = OnceLock::new();
    CACHE.get_or_init(|| Mutex::new(SessionGitStatusCache::default()))
}

pub(super) fn monotonic_now_ms() -> i64 {
    static START: OnceLock<Instant> = OnceLock::new();
    START.get_or_init(Instant::now).elapsed().as_millis() as i64
}

pub(super) fn now_iso() -> String {
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
CDXC:Git 2026-07-30 (effective cwd):
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
