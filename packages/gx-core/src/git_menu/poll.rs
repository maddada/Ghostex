//! The background Git poll's schedule: which projects it reads and when.
//!
//! CDXC:Git 2026-08-16:
//! Poll only projects that currently render a sidebar group header, instead of every registered
//! project of every machine: diff stats exist purely for those headers. The cycle stretches past
//! the base interval once the sidebar renders more project rows than the interval can hold at the
//! capped probe rate, so a sidebar with 100+ rows polls each row less often instead of probing
//! many times per second, and the probes are staggered across the cycle so a large sidebar does
//! not shell out for every repository at once.

/// The base cycle.
pub const GIT_POLL_INTERVAL_MS: u64 = 15 * 1000;
/// The closest two probes of one cycle may be.
pub const GIT_POLL_MIN_PROBE_SPACING_MS: u64 = 1000;
/// How long after a switch or a poll the skipped GitHub probe runs (`CDXC:Git 2026-07-29`).
pub const GIT_HUB_DEFERRED_PROBE_DELAY_MS: u64 = 1500;

/// One project the poll reads.
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub struct GitPollTarget {
    /// `local:<projectId>` or `remote:<scoped project id>`: the order the cycle probes in.
    pub key: String,
    /// The project id the sidebar keys the header's numbers by (a machine-scoped id for a remote
    /// project).
    pub scoped_project_id: String,
}

impl GitPollTarget {
    pub fn local(project_id: &str) -> Self {
        Self {
            key: format!("local:{project_id}"),
            scoped_project_id: project_id.to_string(),
        }
    }

    pub fn remote(scoped_project_id: &str) -> Self {
        Self {
            key: format!("remote:{scoped_project_id}"),
            scoped_project_id: scoped_project_id.to_string(),
        }
    }
}

/// When each target of one cycle is probed, and when the next cycle starts.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct GitPollCycle {
    pub cycle_ms: u64,
    /// `(delay from the cycle start, target)`, in probe order.
    pub probes: Vec<(u64, GitPollTarget)>,
}

/// `scheduleGitPollingCycle`: targets deduplicated and sorted by key, spread evenly over the
/// cycle.
pub fn plan_git_poll_cycle(mut targets: Vec<GitPollTarget>) -> GitPollCycle {
    targets.sort();
    targets.dedup_by(|left, right| left.key == right.key);
    let count = targets.len() as u64;
    let cycle_ms = GIT_POLL_INTERVAL_MS.max(count * GIT_POLL_MIN_PROBE_SPACING_MS);
    let step = cycle_ms as f64 / count.max(1) as f64;
    let probes = targets
        .into_iter()
        .enumerate()
        .map(|(index, target)| ((index as f64 * step).floor() as u64, target))
        .collect();
    GitPollCycle { cycle_ms, probes }
}
