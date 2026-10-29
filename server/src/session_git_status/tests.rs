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

fn scripted_git(answers: Vec<(&'static str, &'static str)>) -> impl Fn(&[&str]) -> Option<String> {
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
CDXC:StateSync 2026-07-29:
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

    run_session_git_status_refresh_pass(&cache, &cwds, &prober, &clock(GIT_STATUS_TTL_MS), true);
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

    run_session_git_status_refresh_pass(&cache, &["/repo".to_string()], &prober, &clock(1), true);
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

    run_session_git_status_refresh_pass(&cache, &cwds, &prober, &clock(GIT_STATUS_TTL_MS), true);
    assert_eq!(
        prober.pull_request_probes.load(Ordering::SeqCst),
        1,
        "a git refresh inside the PR TTL reuses the last gh answer"
    );

    run_session_git_status_refresh_pass(&cache, &cwds, &prober, &clock(PULL_REQUEST_TTL_MS), true);
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
    CDXC:Git 2026-07-30 (effective cwd):
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
    CDXC:Git 2026-07-30 (effective cwd):
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

#[cfg(unix)]
#[test]
fn command_capture_timeout_ends_what_the_command_started() {
    let temp = tempfile::tempdir().expect("tempdir");
    /*
    The check needs the shell to have started its background child before the
    time limit; on a loaded machine it may not even have run yet. An attempt
    whose shell never wrote the pid proves nothing either way, so the next one
    gets a longer limit; normally the first succeeds within half a second.
    */
    for (attempt, limit_ms) in [500, 2_000, 8_000].into_iter().enumerate() {
        let pid_file = temp.path().join(format!("background-{attempt}.pid"));
        let output = run_command_bounded(
            shell_command(&format!(
                "sleep 30 & echo $! > '{0}.tmp' && mv '{0}.tmp' '{0}'; sleep 30",
                pid_file.display()
            )),
            Duration::from_millis(limit_ms),
            Duration::from_secs(30),
            &ABANDONED_COMMAND_READERS,
        );
        assert_eq!(output, None);
        let Ok(text) = std::fs::read_to_string(&pid_file) else {
            continue;
        };
        let background: u32 = text.trim().parse().expect("a pid");
        // The killed process is reaped by init a moment later; until then it still answers.
        let deadline = Instant::now() + Duration::from_secs(5);
        while crate::platform::process::process_is_alive(background) && Instant::now() < deadline {
            std::thread::sleep(Duration::from_millis(20));
        }
        assert!(
            !crate::platform::process::process_is_alive(background),
            "the command's background child must end with it"
        );
        return;
    }
    panic!("the shell never started its background child before the time limit");
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
CDXC:StateSync 2026-07-29:
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
