//! A project's Git answers read again only once something in its folder or its Git folders changed (Windows and macOS).
//!
//! CDXC:Git 2026-11-04 WHY:
//! The sidebar reads every visible project header's numbers on a cycle of about one project a second (apps/desktop/src/app/gx_store/git/poll.rs), and each read ran `git diff --numstat HEAD`: on Windows two git processes (Git for Windows' `cmd\git.exe` launcher, then the real git) plus a conhost and a Defender scan, about 0.85 a second with eight sessions open (measured 2026-11-03), whether or not anything had changed. The answer is kept per project until its folder or Git folder changes (`platform::folder_watch`).
//! CDXC:Git 2026-11-05 WHY:
//! The active project's full read (branch, status, numstat, untracked, upstream counts, remotes) ran every 15 seconds as well, about ten git processes each time and 0.4 s of CPU for `git status` alone in a repository with submodules (measured 2026-11-04 on macOS, issues-1027/macos-perf). It is kept the same way, with the shared Git folder watched too because the upstream counts read its refs. macOS watches with FSEvents. The GitHub CLI answer is never kept here; the client leases it.

use serde_json::Value;
use std::future::Future;

use crate::typed_operations::TypedOperationError;

/// `read` the project's diff stats, or the last answer for the same project when nothing it depends on changed since.
pub(super) async fn read_diff_stats_cached<F, Fut>(
    projects: &[Value],
    count_untracked_when_clean: bool,
    read: F,
) -> Result<Value, TypedOperationError>
where
    F: FnOnce() -> Fut,
    Fut: Future<Output = Result<Value, TypedOperationError>>,
{
    static CACHE: Cache<Value> = new_cache();
    read_change_gated(
        &CACHE,
        projects,
        &count_untracked_when_clean.to_string(),
        false,
        read,
    )
    .await
}

/// `read` the project's local Git facts (the full read), or the last answer while nothing changed.
pub(super) async fn read_git_state_cached<F, Fut>(
    projects: &[Value],
    read: F,
) -> Result<Option<Value>, TypedOperationError>
where
    F: FnOnce() -> Fut,
    Fut: Future<Output = Result<Option<Value>, TypedOperationError>>,
{
    static CACHE: Cache<Option<Value>> = new_cache();
    read_change_gated(&CACHE, projects, "", true, read).await
}

#[cfg(any(windows, target_os = "macos"))]
type Cache<V> = std::sync::OnceLock<crate::platform::folder_watch::ChangeGatedCache<V>>;
/// Linux has no folder watch here, so nothing is kept.
#[cfg(not(any(windows, target_os = "macos")))]
type Cache<V> = std::marker::PhantomData<V>;

const fn new_cache<V>() -> Cache<V> {
    #[cfg(any(windows, target_os = "macos"))]
    return std::sync::OnceLock::new();
    #[cfg(not(any(windows, target_os = "macos")))]
    return std::marker::PhantomData;
}

/// One kept answer per project folder (and `key`) in `cache`, dropped when the folder (and with
/// `shared_refs` the repository's shared Git folder) changes. A folder that cannot be watched is
/// read every time.
#[cfg_attr(not(any(windows, target_os = "macos")), allow(unused_variables))]
async fn read_change_gated<V, F, Fut>(
    cache: &'static Cache<V>,
    projects: &[Value],
    key: &str,
    shared_refs: bool,
    read: F,
) -> Result<V, TypedOperationError>
where
    V: Clone,
    F: FnOnce() -> Fut,
    Fut: Future<Output = Result<V, TypedOperationError>>,
{
    #[cfg(any(windows, target_os = "macos"))]
    {
        use crate::platform::folder_watch::{ChangeGatedCache, GitFolders};
        let path = projects
            .first()
            .and_then(|project| project.get("path"))
            .and_then(Value::as_str);
        if let Some(path) = path {
            let cache = cache.get_or_init(ChangeGatedCache::new);
            let key = format!("{path}\n{key}");
            let folders = if shared_refs {
                GitFolders::WithSharedRefs
            } else {
                GitFolders::WorkTree
            };
            // A read of the same project already running answers this one too, unless something
            // changed again since. The cache is looked at only under the project's lock: a look
            // before it would take a change meant for this request, and the read running then
            // (from before the change) would come back as unchanged (review of 6bc6f491d). A
            // folder that cannot be watched keeps nothing to share, so it takes no lock.
            let in_flight = (!cache.unwatched(&key)).then(|| in_flight_lock(&key));
            let _same_project = match &in_flight {
                Some(lock) => Some(lock.lock().await),
                None => None,
            };
            if let Some(answer) = cache.unchanged(&key, std::path::Path::new(path), folders) {
                return Ok(answer);
            }
            // A read whose git just timed out answers the requests that waited behind it for a
            // few seconds, instead of each running its own read that times out the same way.
            if let Some(failure) = recent_timeout(&key) {
                return Err(failure);
            }
            let result = capped(read).await;
            match &result {
                Ok(answer) => cache.remember(&key, answer.clone()),
                Err(error) if super::is_git_timeout(error) => note_timeout(&key, error),
                Err(_) => {}
            }
            return result;
        }
    }
    capped(read).await
}

/// Background Git reads running at once across every project.
///
/// CDXC:Git 2026-11-18 WHY:
/// GitHub #666 (Windows, 14 projects, large repositories): nothing bounded the sidebar's Git
/// reads, and two misses for one project both ran git, so once reads took longer than the poll's
/// cycle they stacked up (eight git processes for each full read). A read of the same project
/// already running now answers the others (`in_flight_lock`), and at most `MAX_GIT_READS` reads
/// run at once; the rest wait for a slot at most `MAX_GIT_READ_WAIT`, and a read gives its slot
/// back once it has run `MAX_GIT_READ_WAIT` too, so a few projects whose git hangs cannot hold up
/// a healthy one. (A read can still wait behind its own project's running read, which answers it,
/// for as long as that read takes.) The Git state the titlebar reads after a commit comes through
/// here too; Git actions themselves (commit, push, checkout) do not.
const MAX_GIT_READS: usize = 4;
const MAX_GIT_READ_WAIT: std::time::Duration = std::time::Duration::from_secs(8);
/// The longest a background read (its wait for a slot included) takes before it counts as timed out.
const MAX_GIT_READ_TIME: std::time::Duration = std::time::Duration::from_secs(50);

async fn capped<V, F, Fut>(read: F) -> Result<V, TypedOperationError>
where
    F: FnOnce() -> Fut,
    Fut: Future<Output = Result<V, TypedOperationError>>,
{
    static GIT_READS: tokio::sync::Semaphore = tokio::sync::Semaphore::const_new(MAX_GIT_READS);
    // One deadline for the wait for a slot and the read together, under the desktop's 60 s wait
    // (gx_store/rpc.rs), so it sees the answer, and its retry finds a timeout kept
    // (`note_timeout`) instead of starting the read again; the git processes end with the read.
    let deadline = tokio::time::Instant::now() + MAX_GIT_READ_TIME;
    // The semaphore is never closed, so acquiring only waits; past the wait the read runs anyway.
    let turn = tokio::time::timeout(MAX_GIT_READ_WAIT, GIT_READS.acquire())
        .await
        .ok()
        .and_then(Result::ok);
    let read = async {
        tokio::time::timeout_at(deadline, read())
            .await
            .unwrap_or_else(|_| Err(super::git_timed_out()))
    };
    tokio::pin!(read);
    if let Some(turn) = turn {
        tokio::select! {
            result = &mut read => return result,
            _ = tokio::time::sleep(MAX_GIT_READ_WAIT) => drop(turn),
        }
    }
    read.await
}

/// How long a read whose git timed out answers the next requests for the same project.
#[cfg(any(windows, target_os = "macos"))]
const TIMED_OUT_KEPT: std::time::Duration = std::time::Duration::from_secs(10);

#[cfg(any(windows, target_os = "macos"))]
fn timeouts() -> &'static std::sync::Mutex<
    std::collections::HashMap<String, (std::time::Instant, TypedOperationError)>,
> {
    static TIMEOUTS: std::sync::OnceLock<
        std::sync::Mutex<
            std::collections::HashMap<String, (std::time::Instant, TypedOperationError)>,
        >,
    > = std::sync::OnceLock::new();
    TIMEOUTS.get_or_init(Default::default)
}

#[cfg(any(windows, target_os = "macos"))]
fn recent_timeout(key: &str) -> Option<TypedOperationError> {
    let mut timeouts = timeouts().lock().ok()?;
    timeouts.retain(|_, (at, _)| at.elapsed() < TIMED_OUT_KEPT);
    timeouts.get(key).map(|(_, error)| error.clone())
}

#[cfg(any(windows, target_os = "macos"))]
fn note_timeout(key: &str, error: &TypedOperationError) {
    if let Ok(mut timeouts) = timeouts().lock() {
        timeouts.insert(key.to_string(), (std::time::Instant::now(), error.clone()));
    }
}

/// One lock per kept answer, held while its read runs.
#[cfg(any(windows, target_os = "macos"))]
fn in_flight_lock(key: &str) -> std::sync::Arc<tokio::sync::Mutex<()>> {
    use std::collections::HashMap;
    use std::sync::{Arc, Mutex, OnceLock};
    static LOCKS: OnceLock<Mutex<HashMap<String, Arc<tokio::sync::Mutex<()>>>>> = OnceLock::new();
    let Ok(mut locks) = LOCKS.get_or_init(Default::default).lock() else {
        return Arc::new(tokio::sync::Mutex::new(()));
    };
    // Locks nobody holds or waits on are dropped, so the map stays as small as the reads running.
    locks.retain(|_, lock| Arc::strong_count(lock) > 1);
    locks.entry(key.to_string()).or_default().clone()
}
