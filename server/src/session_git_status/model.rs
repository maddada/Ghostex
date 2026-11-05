use super::*;

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
    /// The pull request of each `(cwd, branch)`, in order. The real prober reads a repo's
    /// branches in one call (`pull_request_batch.rs`).
    fn probe_pull_requests(&self, wanted: &[(String, String)]) -> Vec<Option<SessionPullRequest>> {
        wanted
            .iter()
            .map(|(cwd, branch)| self.probe_pull_request(cwd, branch))
            .collect()
    }
}

#[derive(Clone, Debug)]
pub struct SessionGitStatusRefreshClock {
    /// Monotonic milliseconds; only differences matter.
    pub monotonic_now_ms: i64,
    /// Wall-clock RFC3339 stamped onto every freshly probed status.
    pub now_iso: String,
}

#[derive(Clone, Debug)]
pub(super) struct SessionGitStatusEntry {
    pub(super) git_probed_at_ms: i64,
    /// `None` when this entry has never had a `gh` answer (no `gh`, detached
    /// HEAD, or the pass ran out of PR budget before reaching it).
    pub(super) pull_request_probed_at_ms: Option<i64>,
    /// `None` is the negative entry: probed, and not a git worktree.
    pub(super) status: Option<SessionGitStatus>,
}

#[derive(Debug)]
pub(super) struct SessionGitStatusRefreshTarget {
    pub(super) cwd: String,
    pub(super) previous: Option<SessionGitStatusEntry>,
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
    pub(super) fn plan_refresh(
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
    pub(super) fn apply_refresh(
        &mut self,
        results: Vec<(String, SessionGitStatusEntry)>,
    ) -> Vec<String> {
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
