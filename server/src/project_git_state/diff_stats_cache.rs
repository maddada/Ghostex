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
            if let Some(answer) = cache.unchanged(&key, std::path::Path::new(path), folders) {
                return Ok(answer);
            }
            let result = read().await;
            if let Ok(answer) = &result {
                cache.remember(&key, answer.clone());
            }
            return result;
        }
    }
    read().await
}
