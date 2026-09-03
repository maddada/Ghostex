use std::{
    collections::{HashMap, HashSet},
    sync::{Mutex, OnceLock},
    time::Instant,
};

use serde_json::Value;

use crate::session_git_status::run_project_git_remote_probe_command;

/*
CDXC:StateSync 2026-07-29-00:00:
Server side of project Git origin presentation. Sidebar V2 uses the origin to
merge matching cross-machine checkouts, while the classic sidebar exposes the
exact value through Copy Remote URL. gxserver publishes that URL on
`GxserverPresentationProject.gitRemoteOriginUrl`:

  ABSENT  the path is not a git work tree, or has not been probed yet
  null    a git work tree with no `origin` remote
  string  the URL exactly as git reports it

The same probe also publishes the checkout's repository root on
`GxserverPresentationProject.gitRepositoryRootPath` (absent whenever git cannot
report one, with no `null` state — a missing root simply means "cannot tell
where in the repository this project sits"). That field is what makes the
client's "Repository + path" grouping mode real: without a root there is nothing
to measure a project's sub-path against, so every sub-project of a monorepo
would key on the bare repository and the mode could never split anything.

Rules this module holds itself to:

- The URL is published RAW (trimmed only). Normalization — scp-style vs https,
  `.git` suffix, host case — is the CLIENT's job (`normalizeGitRemoteUrl` in
  `packages/shared/sidebar-v2-logical-project.ts`), so one machine's git version can never
  change how another machine's projects group.
- One probe per unique FAMILY ROOT path. A registered worktree project probes its
  parent project's checkout (see `project_git_remote_key`): a linked worktree
  shares its repository's config, so the family shares one cache entry and one
  answer instead of one git spawn per worktree.
- Never on a request path except the first sighting of a brand-new project (see
  `ensure_project_git_remote_probed`). A background pass refreshes the cache and
  presentation only ever READS it, exactly like the P3 git-status cache.
- Every subprocess is the hardened, time-boxed runner the git-status probe uses
  (`session_git_status::run_git_probe_command`), so a hung git on a network
  filesystem can never wedge the daemon.
*/

/// How long a probed repository's `origin` URL stays authoritative.
///
/// Ten minutes, an order of magnitude longer than the 60s git-status TTL,
/// because this answer is an order of magnitude more stable: a project's remote
/// is set once when the repository is cloned and then effectively never changes.
/// The cost of the staleness is bounded and benign — a repository that gains an
/// `origin` joins its cross-machine group within ten minutes — while the saving
/// is real, since the refresh pass otherwise re-spawns git for every registered
/// project every minute.
pub const GIT_REMOTE_TTL_MS: i64 = 10 * 60_000;

/// Non-repository paths are re-checked far less often than repositories, for the
/// same reason the git-status cache does it: a directory rarely becomes a
/// repository, and a machine whose projects are plain folders should not pay a
/// git spawn per project for that fact.
pub const NON_REPOSITORY_GIT_REMOTE_TTL_MS: i64 = 30 * 60_000;

/// Upper bound on probes in one pass. Registered projects are far fewer than
/// session cwds, but a machine restored from a large workspace file still
/// spreads its first pass over a few minutes instead of spending them all at
/// once; the oldest entries go first.
pub const MAX_GIT_REMOTE_PROBES_PER_PASS: usize = 24;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ProjectGitRemote {
    /// `None` when the work tree has no `origin` remote; published as an
    /// explicit `null`.
    pub origin_url: Option<String>,
    /*
    The work tree's repository root (`git rev-parse --show-toplevel`), resolved
    in the SAME probe as the URL because it answers the same question about the
    same directory and would otherwise need its own entry, TTL, and delta rule.

    For a registered worktree project this is the FAMILY ROOT's toplevel, for
    exactly the reason the cache is keyed on the family root at all (see
    `project_git_remote_key`): the family shares one answer, and the client
    measures every member's sub-path against the same root.

    `None` when git reports nothing usable; the key is then omitted entirely,
    which the client reads as "no known root" and falls back to the bare
    repository key.
    */
    pub repository_root_path: Option<String>,
}

/*
The probe surface, injected so the cache's TTL / budget / delta rules are
testable without spawning git.
*/
pub trait ProjectGitRemoteProber {
    /// `None` when the path is not inside a git work tree (or git is unusable).
    fn probe(&self, path: &str) -> Option<ProjectGitRemote>;
}

#[derive(Clone, Debug)]
struct ProjectGitRemoteEntry {
    probed_at_ms: i64,
    /// `None` is the negative entry: probed, and not a git work tree.
    remote: Option<ProjectGitRemote>,
}

#[derive(Default)]
pub struct ProjectGitRemoteCache {
    entries: HashMap<String, ProjectGitRemoteEntry>,
}

impl ProjectGitRemoteCache {
    pub fn get(&self, path: &str) -> Option<ProjectGitRemote> {
        self.entries
            .get(path)
            .and_then(|entry| entry.remote.clone())
    }

    pub fn contains(&self, path: &str) -> bool {
        self.entries.contains_key(path)
    }

    pub fn len(&self) -> usize {
        self.entries.len()
    }

    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }

    /// Seeds an entry directly. Only the refresh pass, the first-sighting warm,
    /// and tests should use this.
    pub fn set(&mut self, path: &str, remote: Option<ProjectGitRemote>, monotonic_now_ms: i64) {
        self.entries.insert(
            path.to_string(),
            ProjectGitRemoteEntry {
                probed_at_ms: monotonic_now_ms,
                remote,
            },
        );
    }

    /*
    Phase one of a pass: drop paths no registered project points at any more,
    then pick the stale ones, oldest first, up to the per-pass budget. The paths
    are copied out so the pass can spawn git with the lock RELEASED —
    presentation reads this cache and must never wait on a subprocess.
    */
    fn plan_refresh(&mut self, paths: &[String], monotonic_now_ms: i64) -> Vec<String> {
        let mut wanted: Vec<&str> = Vec::new();
        let mut seen: HashSet<&str> = HashSet::new();
        for path in paths {
            let path = path.trim();
            if path.is_empty() {
                continue;
            }
            if seen.insert(path) {
                wanted.push(path);
            }
        }
        self.entries.retain(|path, _| seen.contains(path.as_str()));

        let mut stale: Vec<(i64, String)> = wanted
            .into_iter()
            .filter_map(|path| match self.entries.get(path) {
                None => Some((i64::MIN, path.to_string())),
                Some(entry) => {
                    let ttl = if entry.remote.is_some() {
                        GIT_REMOTE_TTL_MS
                    } else {
                        NON_REPOSITORY_GIT_REMOTE_TTL_MS
                    };
                    (monotonic_now_ms - entry.probed_at_ms >= ttl)
                        .then(|| (entry.probed_at_ms, path.to_string()))
                }
            })
            .collect();
        stale.sort_by(|left, right| left.0.cmp(&right.0).then_with(|| left.1.cmp(&right.1)));
        stale
            .into_iter()
            .take(MAX_GIT_REMOTE_PROBES_PER_PASS)
            .map(|(_, path)| path)
            .collect()
    }

    /*
    Phase three: fold the probe results back in and report which paths actually
    CHANGED. Projects change their remote approximately never, so a pass over a
    settled machine returns an empty list and publishes no deltas at all.
    */
    fn apply_refresh(
        &mut self,
        results: Vec<(String, Option<ProjectGitRemote>)>,
        monotonic_now_ms: i64,
    ) -> Vec<String> {
        let mut changed = Vec::new();
        for (path, remote) in results {
            let previous = self
                .entries
                .get(&path)
                .and_then(|entry| entry.remote.clone());
            if previous != remote {
                changed.push(path.clone());
            }
            self.entries.insert(
                path,
                ProjectGitRemoteEntry {
                    probed_at_ms: monotonic_now_ms,
                    remote,
                },
            );
        }
        changed
    }
}

/*
One refresh pass over `paths`. The cache lock is taken twice — to plan and to
merge — and never held while a subprocess runs. Returns the paths whose published
remote changed, which is exactly the set the caller turns into project
presentation deltas.
*/
pub fn run_project_git_remote_refresh_pass(
    cache: &Mutex<ProjectGitRemoteCache>,
    paths: &[String],
    prober: &dyn ProjectGitRemoteProber,
    monotonic_now_ms: i64,
    enabled: bool,
) -> Vec<String> {
    /*
    The explicit caller gate sits before `plan_refresh`: when disabled, nothing
    is probed, published, or evicted. Production enables this for both sidebar
    versions because project menus and V2 grouping consume the same URL.
    */
    if !enabled {
        return Vec::new();
    }
    let targets = {
        let Ok(mut cache) = cache.lock() else {
            return Vec::new();
        };
        cache.plan_refresh(paths, monotonic_now_ms)
    };
    if targets.is_empty() {
        return Vec::new();
    }

    let results = targets
        .into_iter()
        .map(|path| {
            let remote = prober.probe(&path);
            (path, remote)
        })
        .collect::<Vec<_>>();

    let Ok(mut cache) = cache.lock() else {
        return Vec::new();
    };
    cache.apply_refresh(results, monotonic_now_ms)
}

// ---------------------------------------------------------------------------
// process-wide cache
// ---------------------------------------------------------------------------

fn project_git_remote_cache() -> &'static Mutex<ProjectGitRemoteCache> {
    static CACHE: OnceLock<Mutex<ProjectGitRemoteCache>> = OnceLock::new();
    CACHE.get_or_init(|| Mutex::new(ProjectGitRemoteCache::default()))
}

fn monotonic_now_ms() -> i64 {
    static START: OnceLock<Instant> = OnceLock::new();
    START.get_or_init(Instant::now).elapsed().as_millis() as i64
}

/// Runs one pass against the process-wide cache with the real git prober.
/// Blocking: callers must be on a blocking worker, never on a request path.
pub fn refresh_project_git_remote_cache(paths: &[String], enabled: bool) -> Vec<String> {
    run_project_git_remote_refresh_pass(
        project_git_remote_cache(),
        paths,
        &SystemProjectGitRemoteProber,
        monotonic_now_ms(),
        enabled,
    )
}

/// Read-only cache lookup. Never probes, so it is safe on the request path.
pub fn cached_project_git_remote(path: &str) -> Option<ProjectGitRemote> {
    let path = path.trim();
    if path.is_empty() {
        return None;
    }
    project_git_remote_cache().lock().ok()?.get(path)
}

/// The published `gitRemoteOriginUrl` value for a project path: the URL string,
/// an explicit `Value::Null` for a repository with no `origin`, and `None` for a
/// path that is unknown to the cache or is not a git work tree (the key is then
/// omitted entirely).
pub fn published_project_git_remote_origin_url(path: &str) -> Option<Value> {
    cached_project_git_remote(path).map(|remote| match remote.origin_url {
        Some(url) => Value::String(url),
        None => Value::Null,
    })
}

/// The published `gitRepositoryRootPath` value for a project path. Unlike the
/// origin URL this has no `null` state: either the probe resolved a repository
/// root or the key is omitted.
pub fn published_project_git_repository_root_path(path: &str) -> Option<Value> {
    cached_project_git_remote(path)
        .and_then(|remote| remote.repository_root_path)
        .map(Value::String)
}

/*
The path a project's remote is resolved from, and the cache key everywhere
(presentation, the refresh pass, and the first-sighting warm must agree on it
exactly).

For a registered worktree project (decision 4's legacy shape: a worktree that was
registered as its own sibling project) that is the FAMILY ROOT — the parent
project's checkout. A linked worktree shares its repository's config, so both
answers are the same URL, but keying on the root means the whole family costs one
probe and can never disagree with itself mid-refresh. A project whose parent
checkout is gone publishes no remote at all, and the client groups it by path,
which is the correct answer for a worktree with no discoverable repository.
*/
pub fn project_git_remote_key(project: &Value) -> Option<String> {
    let family_root = project
        .get("worktree")
        .and_then(Value::as_object)
        .and_then(|worktree| worktree.get("parentProjectPath"))
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|path| !path.is_empty());
    let own_path = project
        .get("path")
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|path| !path.is_empty());
    family_root.or(own_path).map(str::to_string)
}

/*
First sighting of a project. A brand-new registration would otherwise show no
remote until the next background pass, which is up to a minute of a fresh project
sitting outside its cross-machine group. This probes ONLY when the cache has no
entry for the path at all: every later delta for the same project is a pure cache
read, and refreshes stay the background pass's job.
*/
pub fn ensure_project_git_remote_probed(project: &Value, enabled: bool) {
    /*
    Keep the caller gate at the probe rather than at each hook. Production
    enables it for both sidebar versions; retaining the explicit gate also lets
    bounded maintenance callers suppress work without unwiring publication.
    */
    if !enabled {
        return;
    }
    let Some(path) = project_git_remote_key(project) else {
        return;
    };
    let already_probed = match project_git_remote_cache().lock() {
        Ok(cache) => cache.contains(&path),
        Err(_) => true,
    };
    if already_probed {
        return;
    }
    // Probed with the lock RELEASED: presentation reads this cache and must
    // never wait on a subprocess. A concurrent first sighting of the same path
    // costs one duplicate probe and nothing else.
    let remote = SystemProjectGitRemoteProber.probe(&path);
    if let Ok(mut cache) = project_git_remote_cache().lock() {
        cache.set(&path, remote, monotonic_now_ms());
    }
}

/*
CDXC:StateSync 2026-07-29 (P5 fix round):
The warm every project DELTA runs, and the reason it is not limited to
`projectAdded`.

A project that leaves presentation — parked as a Recent Project, or a hidden
carrier — is dropped from the refresh pass's path set, and the pass therefore
evicts its cache entry (that eviction is what keeps the cache bounded to the
projects that exist). Restoring it later published a project with NO remote for
up to a minute, so the restored row sat outside its cross-machine group until the
next background pass reached it — the same "announced before it was probed"
problem the `projectAdded` warm was written to solve, arriving through a
different door.

So the warm follows PUBLICATION rather than a delta type: whenever a delta is
about to announce a project presentation actually publishes, the cache is warmed
first. The gate matters in both directions — a parked or hidden project is never
probed at all now, which also stops a parked project's own updates from
re-probing a path the next pass would immediately evict again.

Still at most ONE probe per path (`ensure_project_git_remote_probed` returns
immediately once an entry exists), and still off the presentation sequencer.
*/
pub fn ensure_published_project_git_remote_probed(project: &Value, enabled: bool) {
    if !crate::presentation::should_include_presentation_project(project) {
        return;
    }
    ensure_project_git_remote_probed(project, enabled);
}

#[cfg(test)]
pub fn set_cached_project_git_remote_for_test(path: &str, remote: Option<ProjectGitRemote>) {
    if let Ok(mut cache) = project_git_remote_cache().lock() {
        cache.set(path, remote, monotonic_now_ms());
    }
}

/// Drops one path's entry exactly as `plan_refresh` does for a project that is
/// no longer published, so a test can reproduce the park→pass→restore sequence
/// without running a pass against the process-wide cache (which would evict
/// every other test's entries).
#[cfg(test)]
pub fn forget_cached_project_git_remote_for_test(path: &str) {
    if let Ok(mut cache) = project_git_remote_cache().lock() {
        cache.entries.remove(path);
    }
}

// ---------------------------------------------------------------------------
// the real prober
// ---------------------------------------------------------------------------

pub struct SystemProjectGitRemoteProber;

impl ProjectGitRemoteProber for SystemProjectGitRemoteProber {
    fn probe(&self, path: &str) -> Option<ProjectGitRemote> {
        let owned_path = path.to_string();
        probe_project_git_remote_with(&move |args: &[&str]| {
            run_project_git_remote_probe_command(&owned_path, args)
        })
    }
}

/*
The plumbing, split from the process spawn so it is testable without git.

The work-tree gate comes first for the same reason the topology probe has one: it
is the only way to tell "not a repository" (key absent) from "a repository with
no origin" (key null), and it also keeps a stray global `remote.origin.url` from
attaching a remote identity to a plain folder.
*/
pub fn probe_project_git_remote_with(
    run: &dyn Fn(&[&str]) -> Option<String>,
) -> Option<ProjectGitRemote> {
    if run(&["rev-parse", "--is-inside-work-tree"])
        .as_deref()
        .map(str::trim)
        != Some("true")
    {
        return None;
    }
    let origin_url = run(&["config", "--get", "remote.origin.url"])
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty());
    /*
    One extra command in the same probe, behind the same work-tree gate: a plain
    folder must publish no root any more than it publishes a remote, and a bare
    repository (where `--show-toplevel` fails) simply has no root to report.
    */
    let repository_root_path = run(&["rev-parse", "--show-toplevel"])
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty());
    Some(ProjectGitRemote {
        origin_url,
        repository_root_path,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    use std::{
        path::Path,
        process::{Command, Stdio},
        sync::atomic::{AtomicUsize, Ordering},
    };

    // -----------------------------------------------------------------------
    // fakes
    // -----------------------------------------------------------------------

    #[derive(Default)]
    struct FakeProber {
        remotes: Mutex<HashMap<String, Option<ProjectGitRemote>>>,
        probes: AtomicUsize,
    }

    impl FakeProber {
        fn set(&self, path: &str, remote: Option<ProjectGitRemote>) {
            self.remotes
                .lock()
                .expect("remotes")
                .insert(path.to_string(), remote);
        }
    }

    impl ProjectGitRemoteProber for FakeProber {
        fn probe(&self, path: &str) -> Option<ProjectGitRemote> {
            self.probes.fetch_add(1, Ordering::SeqCst);
            self.remotes
                .lock()
                .expect("remotes")
                .get(path)
                .cloned()
                .flatten()
        }
    }

    fn remote(origin_url: Option<&str>) -> ProjectGitRemote {
        ProjectGitRemote {
            origin_url: origin_url.map(str::to_string),
            repository_root_path: None,
        }
    }

    fn remote_rooted(origin_url: Option<&str>, repository_root_path: &str) -> ProjectGitRemote {
        ProjectGitRemote {
            origin_url: origin_url.map(str::to_string),
            repository_root_path: Some(repository_root_path.to_string()),
        }
    }

    fn cache() -> Mutex<ProjectGitRemoteCache> {
        Mutex::new(ProjectGitRemoteCache::default())
    }

    fn remote_of(cache: &Mutex<ProjectGitRemoteCache>, path: &str) -> Option<ProjectGitRemote> {
        cache.lock().expect("cache").get(path)
    }

    // -----------------------------------------------------------------------
    // probe plumbing
    // -----------------------------------------------------------------------

    #[test]
    fn a_path_outside_a_work_tree_probes_as_no_remote_at_all() {
        /*
        The three-state contract's first state: ABSENT. A plain folder must not
        publish `gitRemoteOriginUrl` at all, so a stray global
        `remote.origin.url` can never give it a repository identity.
        */
        let run = |args: &[&str]| -> Option<String> {
            match args {
                ["rev-parse", "--is-inside-work-tree"] => Some("false".to_string()),
                _ => panic!("nothing else may run outside a work tree: {args:?}"),
            }
        };
        assert_eq!(probe_project_git_remote_with(&run), None);

        let failing = |_args: &[&str]| -> Option<String> { None };
        assert_eq!(
            probe_project_git_remote_with(&failing),
            None,
            "a deleted directory degrades to no remote instead of failing the pass"
        );
    }

    #[test]
    fn a_work_tree_without_an_origin_probes_as_an_explicit_null() {
        let run = |args: &[&str]| -> Option<String> {
            match args {
                ["rev-parse", "--is-inside-work-tree"] => Some("true".to_string()),
                ["rev-parse", "--show-toplevel"] => Some("/repos/quiet".to_string()),
                // `git config --get` exits non-zero when the key is unset.
                _ => None,
            }
        };
        assert_eq!(
            probe_project_git_remote_with(&run),
            Some(remote_rooted(None, "/repos/quiet")),
            "a repository with no origin still reports where its root is"
        );

        let empty = |args: &[&str]| -> Option<String> {
            match args {
                ["rev-parse", "--is-inside-work-tree"] => Some("true".to_string()),
                _ => Some("   ".to_string()),
            }
        };
        assert_eq!(
            probe_project_git_remote_with(&empty),
            Some(remote(None)),
            "a blank remote or root value is nothing, not an empty-string identity"
        );
    }

    #[test]
    fn the_origin_url_is_published_exactly_as_git_reports_it() {
        /*
        Normalization is the CLIENT's job (`normalizeGitRemoteUrl`), so the
        server must not touch case, the `.git` suffix, or the scp-style shape —
        only surrounding whitespace from the command output.
        */
        for raw in [
            "git@github.com:Owner/Repo.git",
            "https://github.com/Owner/Repo",
            "ssh://git@example.invalid:2222/Owner/Repo.git",
            "/srv/mirrors/Repo.git",
        ] {
            let run = |args: &[&str]| -> Option<String> {
                match args {
                    ["rev-parse", "--is-inside-work-tree"] => Some("true".to_string()),
                    ["rev-parse", "--show-toplevel"] => Some("  /repos/ghostex \n".to_string()),
                    _ => Some(format!("  {raw}\n")),
                }
            };
            assert_eq!(
                probe_project_git_remote_with(&run),
                Some(remote_rooted(Some(raw), "/repos/ghostex")),
                "{raw} must survive the probe byte for byte"
            );
        }
    }

    #[test]
    fn the_repository_root_rides_the_same_probe_and_the_same_work_tree_gate() {
        /*
        CDXC:StateSync 2026-07-29 (P5 fix round):
        `gitRepositoryRootPath` is what makes the client's "Repository + path"
        mode able to split a monorepo, so the probe has to answer it in the same
        pass as the URL — and answer it with the key ABSENT (never null, never
        an empty string) whenever git has nothing to say.
        */
        let unresolvable_root = |args: &[&str]| -> Option<String> {
            match args {
                ["rev-parse", "--is-inside-work-tree"] => Some("true".to_string()),
                ["rev-parse", "--show-toplevel"] => None,
                _ => Some("git@github.com:Owner/Repo.git".to_string()),
            }
        };
        assert_eq!(
            probe_project_git_remote_with(&unresolvable_root),
            Some(remote(Some("git@github.com:Owner/Repo.git"))),
            "a repository whose root git will not report still publishes its remote"
        );

        let seen: Mutex<Vec<String>> = Mutex::new(Vec::new());
        let record = |args: &[&str]| -> Option<String> {
            seen.lock().expect("seen").push(args.join(" "));
            match args {
                ["rev-parse", "--is-inside-work-tree"] => Some("true".to_string()),
                ["rev-parse", "--show-toplevel"] => Some("/repos/ghostex".to_string()),
                _ => Some("git@github.com:Owner/Repo.git".to_string()),
            }
        };
        assert_eq!(
            probe_project_git_remote_with(&record),
            Some(remote_rooted(
                Some("git@github.com:Owner/Repo.git"),
                "/repos/ghostex"
            ))
        );
        assert_eq!(
            seen.lock().expect("seen").clone(),
            vec![
                "rev-parse --is-inside-work-tree".to_string(),
                "config --get remote.origin.url".to_string(),
                "rev-parse --show-toplevel".to_string(),
            ],
            "the root costs exactly one extra command inside the existing probe"
        );
    }

    // -----------------------------------------------------------------------
    // the family root
    // -----------------------------------------------------------------------

    #[test]
    fn a_registered_worktree_project_resolves_its_family_root() {
        let plain = json!({ "path": "/repos/ghostex", "projectId": "P100" });
        assert_eq!(
            project_git_remote_key(&plain),
            Some("/repos/ghostex".to_string())
        );

        let worktree = json!({
            "path": "/repos/ghostex-a1b2c3d4",
            "projectId": "P101",
            "worktree": {
                "branch": "ghostex/a1b2c3d4",
                "parentProjectId": "P100",
                "parentProjectPath": "/repos/ghostex",
            },
        });
        assert_eq!(
            project_git_remote_key(&worktree),
            Some("/repos/ghostex".to_string()),
            "a worktree family shares its root's probe, so both rows carry the same remote"
        );

        let orphan = json!({
            "path": "/repos/ghostex-9f8e7d6c",
            "projectId": "P102",
            "worktree": { "parentProjectPath": "   " },
        });
        assert_eq!(
            project_git_remote_key(&orphan),
            Some("/repos/ghostex-9f8e7d6c".to_string()),
            "a worktree row with no recorded root falls back to its own checkout"
        );

        assert_eq!(
            project_git_remote_key(&json!({ "projectId": "P103" })),
            None
        );
        assert_eq!(
            project_git_remote_key(&json!({ "path": "  ", "projectId": "P104" })),
            None
        );
    }

    // -----------------------------------------------------------------------
    // the published value
    // -----------------------------------------------------------------------

    #[test]
    fn the_published_value_keeps_absent_and_null_apart() {
        let path = "/tmp/ghostex-project-git-remote/published";
        assert_eq!(
            published_project_git_remote_origin_url(path),
            None,
            "an unprobed path publishes no key"
        );

        assert_eq!(
            published_project_git_repository_root_path(path),
            None,
            "an unprobed path publishes no root either"
        );

        set_cached_project_git_remote_for_test(path, Some(remote(None)));
        assert_eq!(
            published_project_git_remote_origin_url(path),
            Some(Value::Null),
            "a repository with no origin publishes an explicit null"
        );
        assert_eq!(
            published_project_git_repository_root_path(path),
            None,
            "the root has no null state: an unresolved root is an absent key"
        );

        set_cached_project_git_remote_for_test(
            path,
            Some(remote_rooted(Some("git@github.com:o/r.git"), "/repos/r")),
        );
        assert_eq!(
            published_project_git_remote_origin_url(path),
            Some(json!("git@github.com:o/r.git"))
        );
        assert_eq!(
            published_project_git_repository_root_path(path),
            Some(json!("/repos/r"))
        );

        set_cached_project_git_remote_for_test(path, None);
        assert_eq!(
            published_project_git_remote_origin_url(path),
            None,
            "a probed non-repository publishes no key either"
        );
        assert_eq!(published_project_git_repository_root_path(path), None);
    }

    // -----------------------------------------------------------------------
    // cache behavior
    // -----------------------------------------------------------------------

    #[test]
    fn one_probe_serves_every_project_sharing_a_family_root() {
        let cache = cache();
        let prober = FakeProber::default();
        prober.set(
            "/repos/ghostex",
            Some(remote(Some("git@github.com:o/r.git"))),
        );

        // A project and two of its worktrees all resolve to the same root.
        let paths = vec![
            "/repos/ghostex".to_string(),
            "/repos/ghostex".to_string(),
            " /repos/ghostex ".to_string(),
        ];
        let changed = run_project_git_remote_refresh_pass(&cache, &paths, &prober, 0, true);

        assert_eq!(prober.probes.load(Ordering::SeqCst), 1);
        assert_eq!(changed, vec!["/repos/ghostex".to_string()]);
        assert_eq!(
            remote_of(&cache, "/repos/ghostex"),
            Some(remote(Some("git@github.com:o/r.git")))
        );
    }

    /*
    CDXC:StateSync 2026-07-29:
    Cross-machine grouping is a Sidebar V2 concept, so a V1 machine must spawn no
    git for it, publish nothing, and evict nothing — and must start probing again
    on the first pass after the user selects V2, without a daemon restart.
    */
    #[test]
    fn sidebar_v1_probes_nothing_and_flipping_to_v2_warms_in_the_next_pass() {
        let cache = cache();
        let prober = FakeProber::default();
        prober.set(
            "/repos/ghostex",
            Some(remote(Some("git@github.com:o/r.git"))),
        );
        cache.lock().expect("cache").set(
            "/repos/gone",
            Some(remote(Some("git@github.com:o/g.git"))),
            0,
        );
        let paths = vec!["/repos/ghostex".to_string()];

        let changed = run_project_git_remote_refresh_pass(&cache, &paths, &prober, 0, false);
        assert!(
            changed.is_empty(),
            "a gated pass publishes no delta: {changed:?}"
        );
        assert_eq!(
            prober.probes.load(Ordering::SeqCst),
            0,
            "a machine on Sidebar V1 spawns no git"
        );
        assert!(remote_of(&cache, "/repos/ghostex").is_none());
        assert!(
            remote_of(&cache, "/repos/gone").is_some(),
            "a gated pass evicts nothing an earlier V2 stretch cached"
        );

        let changed = run_project_git_remote_refresh_pass(&cache, &paths, &prober, 0, true);
        assert_eq!(
            changed,
            vec!["/repos/ghostex".to_string()],
            "the first pass after the flip warms the cache and publishes"
        );
        assert_eq!(prober.probes.load(Ordering::SeqCst), 1);
        assert!(
            remote_of(&cache, "/repos/gone").is_none(),
            "and normal eviction resumes with it"
        );
    }

    #[test]
    fn cached_remotes_survive_until_their_ttl_and_non_repositories_last_longer() {
        let cache = cache();
        let prober = FakeProber::default();
        prober.set(
            "/repos/ghostex",
            Some(remote(Some("git@github.com:o/r.git"))),
        );
        prober.set("/home/notes", None);
        let paths = vec!["/repos/ghostex".to_string(), "/home/notes".to_string()];

        run_project_git_remote_refresh_pass(&cache, &paths, &prober, 0, true);
        assert_eq!(prober.probes.load(Ordering::SeqCst), 2);
        assert!(
            remote_of(&cache, "/home/notes").is_none(),
            "a directory outside a repository caches as a negative entry"
        );

        run_project_git_remote_refresh_pass(&cache, &paths, &prober, GIT_REMOTE_TTL_MS - 1, true);
        assert_eq!(
            prober.probes.load(Ordering::SeqCst),
            2,
            "nothing is re-probed inside the TTL"
        );

        run_project_git_remote_refresh_pass(&cache, &paths, &prober, GIT_REMOTE_TTL_MS, true);
        assert_eq!(
            prober.probes.load(Ordering::SeqCst),
            3,
            "only the repository is due at the ten-minute mark"
        );

        run_project_git_remote_refresh_pass(
            &cache,
            &paths,
            &prober,
            NON_REPOSITORY_GIT_REMOTE_TTL_MS,
            true,
        );
        assert_eq!(
            prober.probes.load(Ordering::SeqCst),
            5,
            "at the half-hour mark both the repository and the plain folder are due"
        );
    }

    #[test]
    fn a_pass_reports_only_the_paths_whose_remote_actually_changed() {
        /*
        Projects change their remote approximately never, so a re-probe that
        finds the same URL must publish NO delta at all — otherwise every pass
        would churn a presentation revision for every project on the machine.
        */
        let cache = cache();
        let prober = FakeProber::default();
        prober.set(
            "/repos/ghostex",
            Some(remote(Some("git@github.com:o/r.git"))),
        );
        prober.set("/repos/quiet", Some(remote(None)));
        let paths = vec!["/repos/ghostex".to_string(), "/repos/quiet".to_string()];

        let changed = run_project_git_remote_refresh_pass(&cache, &paths, &prober, 0, true);
        assert_eq!(changed.len(), 2, "first sighting always changes");

        let changed =
            run_project_git_remote_refresh_pass(&cache, &paths, &prober, GIT_REMOTE_TTL_MS, true);
        assert!(
            changed.is_empty(),
            "an unchanged remote publishes no delta: {changed:?}"
        );

        prober.set(
            "/repos/quiet",
            Some(remote(Some("https://example.invalid/quiet.git"))),
        );
        let changed = run_project_git_remote_refresh_pass(
            &cache,
            &paths,
            &prober,
            GIT_REMOTE_TTL_MS * 2,
            true,
        );
        assert_eq!(
            changed,
            vec!["/repos/quiet".to_string()],
            "gaining an origin is a real change, and only that project is re-published"
        );

        /*
        CDXC:StateSync 2026-07-29 (P5 fix round):
        The repository root shares the entry, so it must share the delta rule:
        a checkout that moves under a different root republishes exactly like one
        that changed its remote, or the client's "Repository + path" keys would
        go stale for as long as the row stayed unchanged otherwise.
        */
        prober.set(
            "/repos/quiet",
            Some(remote_rooted(
                Some("https://example.invalid/quiet.git"),
                "/repos/quiet",
            )),
        );
        let changed = run_project_git_remote_refresh_pass(
            &cache,
            &paths,
            &prober,
            GIT_REMOTE_TTL_MS * 3,
            true,
        );
        assert_eq!(
            changed,
            vec!["/repos/quiet".to_string()],
            "gaining a repository root is a change even when the remote is identical"
        );
        let changed = run_project_git_remote_refresh_pass(
            &cache,
            &paths,
            &prober,
            GIT_REMOTE_TTL_MS * 4,
            true,
        );
        assert!(
            changed.is_empty(),
            "an unchanged root publishes no delta: {changed:?}"
        );

        prober.set("/repos/quiet", None);
        let changed = run_project_git_remote_refresh_pass(
            &cache,
            &paths,
            &prober,
            GIT_REMOTE_TTL_MS * 5,
            true,
        );
        assert_eq!(
            changed,
            vec!["/repos/quiet".to_string()],
            "losing the repository itself flips the key from a URL to absent"
        );
    }

    #[test]
    fn unregistered_paths_are_dropped_and_the_pass_is_budgeted() {
        let cache = cache();
        let prober = FakeProber::default();
        let mut paths = Vec::new();
        for index in 0..(MAX_GIT_REMOTE_PROBES_PER_PASS + 4) {
            let path = format!("/repos/project-{index:03}");
            prober.set(&path, Some(remote(Some("git@github.com:o/r.git"))));
            paths.push(path);
        }

        let changed = run_project_git_remote_refresh_pass(&cache, &paths, &prober, 0, true);
        assert_eq!(changed.len(), MAX_GIT_REMOTE_PROBES_PER_PASS);
        assert_eq!(
            prober.probes.load(Ordering::SeqCst),
            MAX_GIT_REMOTE_PROBES_PER_PASS,
            "one pass never spends more than its budget"
        );

        // The remaining four are the never-probed ones, so the next pass takes
        // them even though nothing has expired.
        let changed = run_project_git_remote_refresh_pass(&cache, &paths, &prober, 1, true);
        assert_eq!(changed.len(), 4);

        // A project the user closed is no longer wanted, so its entry goes.
        let survivor = paths[0].clone();
        run_project_git_remote_refresh_pass(
            &cache,
            std::slice::from_ref(&survivor),
            &prober,
            2,
            true,
        );
        assert_eq!(cache.lock().expect("cache").len(), 1);
        assert!(remote_of(&cache, &survivor).is_some());
        assert!(remote_of(&cache, &paths[1]).is_none());
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

    fn create_repository(root: &Path) {
        std::fs::create_dir_all(root).expect("repo dir");
        git(root, &["init", "--quiet", "-b", "main"]);
        git(root, &["config", "user.name", "Ghostex Tests"]);
        git(
            root,
            &["config", "user.email", "ghostex-tests@example.invalid"],
        );
        std::fs::write(root.join("base.txt"), "1\n").expect("base file");
        git(root, &["add", "."]);
        git(root, &["commit", "--quiet", "-m", "base"]);
    }

    fn probe_path(path: &Path) -> Option<ProjectGitRemote> {
        SystemProjectGitRemoteProber.probe(&path.to_string_lossy())
    }

    /// git reports the RESOLVED work-tree root, and a temp dir on macOS is a
    /// symlink (`/var/folders/...` -> `/private/var/folders/...`), so the
    /// expectation has to be resolved the same way.
    fn resolved(path: &Path) -> String {
        std::fs::canonicalize(path)
            .expect("canonical path")
            .to_string_lossy()
            .to_string()
    }

    #[test]
    fn a_real_repository_publishes_its_configured_origin() {
        if !git_available() {
            return;
        }
        let temp = tempfile::tempdir().expect("tempdir");
        let root = temp.path().join("repo");
        create_repository(&root);

        assert_eq!(
            probe_path(&root),
            Some(remote_rooted(None, &resolved(&root))),
            "a repository with no remote is a null, not an absent key"
        );

        git(
            &root,
            &["remote", "add", "origin", "git@github.com:Owner/Repo.git"],
        );
        assert_eq!(
            probe_path(&root),
            Some(remote_rooted(
                Some("git@github.com:Owner/Repo.git"),
                &resolved(&root)
            ))
        );

        git(
            &root,
            &[
                "remote",
                "set-url",
                "origin",
                "https://github.com/Owner/Repo.git",
            ],
        );
        assert_eq!(
            probe_path(&root),
            Some(remote_rooted(
                Some("https://github.com/Owner/Repo.git"),
                &resolved(&root)
            ))
        );

        // A second remote must not be mistaken for the origin.
        git(
            &root,
            &[
                "remote",
                "add",
                "upstream",
                "git@github.com:Upstream/Repo.git",
            ],
        );
        assert_eq!(
            probe_path(&root),
            Some(remote_rooted(
                Some("https://github.com/Owner/Repo.git"),
                &resolved(&root)
            ))
        );

        /*
        CDXC:StateSync 2026-07-29 (P5 fix round):
        A sub-directory of the repository must report the REPOSITORY root, not
        itself — that difference is exactly what lets the client derive a
        monorepo sub-project's relative path and keep it apart under
        "Repository + path".
        */
        let sub_package = root.join("packages/ui");
        std::fs::create_dir_all(&sub_package).expect("sub package dir");
        assert_eq!(
            probe_path(&sub_package),
            Some(remote_rooted(
                Some("https://github.com/Owner/Repo.git"),
                &resolved(&root)
            ))
        );
    }

    #[test]
    fn a_linked_worktree_reports_the_family_root_remote() {
        if !git_available() {
            return;
        }
        let temp = tempfile::tempdir().expect("tempdir");
        let root = temp.path().join("repo");
        create_repository(&root);
        git(
            &root,
            &["remote", "add", "origin", "git@github.com:Owner/Repo.git"],
        );

        let worktree = temp.path().join("repo-a1b2c3d4");
        git(
            &root,
            &[
                "worktree",
                "add",
                "--quiet",
                "-b",
                "ghostex/a1b2c3d4",
                worktree.to_str().expect("worktree path"),
            ],
        );

        assert_eq!(
            probe_path(&worktree),
            Some(remote_rooted(
                Some("git@github.com:Owner/Repo.git"),
                &resolved(&worktree)
            )),
            "a linked worktree shares its repository's config, so the family agrees"
        );

        /*
        The published answer for a registered worktree project comes from its
        FAMILY ROOT key, so the whole family — parent and worktree rows alike —
        reports the parent checkout's root and the client measures every member's
        sub-path against the same origin point.
        */
        let worktree_project = json!({
            "path": worktree.to_string_lossy(),
            "projectId": "P101",
            "worktree": { "parentProjectPath": root.to_string_lossy() },
        });
        let key = project_git_remote_key(&worktree_project).expect("family root key");
        set_cached_project_git_remote_for_test(&key, probe_path(&root));
        assert_eq!(
            published_project_git_repository_root_path(&key),
            Some(json!(resolved(&root))),
            "the worktree row publishes the family root's repository root"
        );
    }

    #[test]
    fn a_plain_directory_and_a_missing_directory_publish_no_remote() {
        if !git_available() {
            return;
        }
        let temp = tempfile::tempdir().expect("tempdir");
        assert_eq!(probe_path(temp.path()), None);
        assert_eq!(
            SystemProjectGitRemoteProber.probe("/definitely/not/a/real/path"),
            None,
            "a deleted project path degrades to no remote instead of failing the pass"
        );
    }

    #[test]
    fn a_freshly_registered_project_carries_its_origin_in_the_delta_that_announces_it() {
        /*
        The registration warm: a project the user just added must reach Sidebar
        V2 already grouped with the same repository on other machines, instead of
        joining its logical project up to a background pass later.
        */
        if !git_available() {
            return;
        }
        let temp = tempfile::tempdir().expect("tempdir");
        let root = temp.path().join("repo");
        create_repository(&root);
        git(
            &root,
            &["remote", "add", "origin", "git@github.com:Owner/Repo.git"],
        );

        let paths = crate::paths::get_gxserver_paths(Some(temp.path().join("home")));
        crate::storage::initialize_gxserver_storage(&paths).expect("storage init");
        let db = crate::storage::open_gxserver_database(&paths).expect("open db");
        let repository = crate::domain::DomainRepository::new(&db, "S7k");
        let project = repository
            .create_project(
                json!({ "name": "Repo", "path": root.to_string_lossy() })
                    .as_object()
                    .expect("project params"),
            )
            .expect("project created");
        let project_id = project
            .get("projectId")
            .and_then(Value::as_str)
            .expect("project id")
            .to_string();

        ensure_published_project_git_remote_probed(&project, true);
        let delta = crate::presentation::build_presentation_project_delta(
            &repository,
            &project_id,
            "projectAdded",
        )
        .expect("project delta");
        assert_eq!(
            delta
                .get("project")
                .and_then(|project| project.get("gitRemoteOriginUrl")),
            Some(&json!("git@github.com:Owner/Repo.git"))
        );
        assert_eq!(
            delta
                .get("project")
                .and_then(|project| project.get("gitRepositoryRootPath")),
            Some(&json!(resolved(&root))),
            "the same delta carries the repository root the client keys sub-paths on"
        );
    }

    /*
    CDXC:StateSync 2026-07-29:
    The warm answers to the same version gate as the pass, and it is gated at the
    PROBE rather than by unwiring the hook: a V1 daemon still announces the
    project through exactly the same delta path, it just spawns no git on the way.
    */
    #[test]
    fn the_registration_warm_probes_nothing_while_sidebar_v1_is_selected() {
        if !git_available() {
            return;
        }
        let temp = tempfile::tempdir().expect("tempdir");
        let root = temp.path().join("repo");
        create_repository(&root);
        git(
            &root,
            &["remote", "add", "origin", "git@github.com:Owner/Gated.git"],
        );
        let project = json!({
            "name": "Gated",
            "path": root.to_string_lossy(),
            "projectId": "P-gated",
        });
        let cache_key = project_git_remote_key(&project).expect("cache key");
        forget_cached_project_git_remote_for_test(&cache_key);

        ensure_published_project_git_remote_probed(&project, false);
        assert!(
            cached_project_git_remote(&cache_key).is_none(),
            "a V1 daemon's projectAdded must not spawn git"
        );

        ensure_published_project_git_remote_probed(&project, true);
        assert!(
            cached_project_git_remote(&cache_key).is_some(),
            "the same hook on a V2 daemon warms the cache as before"
        );
        forget_cached_project_git_remote_for_test(&cache_key);
    }

    #[test]
    fn a_restored_parked_project_carries_its_origin_in_the_delta_that_restores_it() {
        /*
        CDXC:StateSync 2026-07-29 (P5 fix round):
        The reported regression, end to end. A parked project leaves
        presentation, so the refresh pass stops wanting its path and evicts the
        cache entry. Restoring it then published a project with NO remote until
        the next background pass reached it — up to a minute of a restored
        project sitting outside its cross-machine group.

        The warm now follows publication rather than the `projectAdded` delta
        type, so the delta that restores the project already carries the remote.
        The parked half is asserted too: a project presentation does not publish
        must never cost a probe, or the eviction and the warm would fight each
        other once a minute.
        */
        if !git_available() {
            return;
        }
        let temp = tempfile::tempdir().expect("tempdir");
        let root = temp.path().join("repo");
        create_repository(&root);
        git(
            &root,
            &["remote", "add", "origin", "git@github.com:Owner/Repo.git"],
        );

        let paths = crate::paths::get_gxserver_paths(Some(temp.path().join("home")));
        crate::storage::initialize_gxserver_storage(&paths).expect("storage init");
        let db = crate::storage::open_gxserver_database(&paths).expect("open db");
        let repository = crate::domain::DomainRepository::new(&db, "S7l");
        let project = repository
            .create_project(
                json!({ "name": "Repo", "path": root.to_string_lossy() })
                    .as_object()
                    .expect("project params"),
            )
            .expect("project created");
        let project_id = project
            .get("projectId")
            .and_then(Value::as_str)
            .expect("project id")
            .to_string();
        let cache_key = project_git_remote_key(&project).expect("cache key");
        ensure_published_project_git_remote_probed(&project, true);
        assert!(cached_project_git_remote(&cache_key).is_some());

        // Park it: presentation drops the project, so the next refresh pass
        // stops wanting its path and drops the entry with it.
        let parked = repository
            .close_project_to_recent(&project_id)
            .expect("parked");
        assert!(
            !crate::presentation::should_include_presentation_project(&parked),
            "a parked project must be outside presentation for this repro to mean anything"
        );
        forget_cached_project_git_remote_for_test(&cache_key);

        ensure_published_project_git_remote_probed(&parked, true);
        assert!(
            cached_project_git_remote(&cache_key).is_none(),
            "a parked project must never be probed: the next pass would evict it again"
        );

        let restored = repository
            .restore_recent_project(&project_id)
            .expect("restored");
        assert_eq!(
            crate::presentation::build_presentation_project_delta(
                &repository,
                &project_id,
                "projectUpdated",
            )
            .expect("project delta")
            .get("project")
            .and_then(|project| project.get("gitRemoteOriginUrl")),
            None,
            "without the warm the restoring delta publishes no remote at all — \
             that gap is the bug"
        );

        ensure_published_project_git_remote_probed(&restored, true);
        let delta = crate::presentation::build_presentation_project_delta(
            &repository,
            &project_id,
            "projectUpdated",
        )
        .expect("project delta");
        assert_eq!(
            delta
                .get("project")
                .and_then(|project| project.get("gitRemoteOriginUrl")),
            Some(&json!("git@github.com:Owner/Repo.git")),
            "the restoring delta must carry the origin, not wait for the 60s pass"
        );
        assert_eq!(
            delta
                .get("project")
                .and_then(|project| project.get("gitRepositoryRootPath")),
            Some(&json!(resolved(&root)))
        );
    }
}
