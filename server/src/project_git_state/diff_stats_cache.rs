//! A project's diff stats read again only once something in its folder or its Git folder changed (Windows).
//!
//! CDXC:Git 2026-11-04 WHY:
//! The sidebar reads every visible project header's numbers on a cycle of about one project a second (apps/desktop/src/app/gx_store/git/poll.rs), and each read ran `git diff --numstat HEAD`: on Windows two git processes (Git for Windows' `cmd\git.exe` launcher, then the real git) plus a conhost and a Defender scan, about 0.85 a second with eight sessions open (measured 2026-11-03), whether or not anything had changed. The answer is kept per project until its folder or Git folder changes (`platform::folder_watch`).

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
    #[cfg(windows)]
    {
        use crate::platform::folder_watch::{ChangeGatedCache, GitFolders};
        use std::sync::OnceLock;
        static CACHE: OnceLock<ChangeGatedCache<Value>> = OnceLock::new();
        let cache = CACHE.get_or_init(ChangeGatedCache::new);
        let path = projects
            .first()
            .and_then(|project| project.get("path"))
            .and_then(Value::as_str);
        if let Some(path) = path {
            let key = format!("{path}\n{count_untracked_when_clean}");
            let folder = std::path::Path::new(path);
            if let Some(answer) = cache.unchanged(&key, folder, GitFolders::WorkTree) {
                return Ok(answer);
            }
            let result = read().await;
            if let Ok(answer) = &result {
                cache.remember(&key, answer.clone());
            }
            return result;
        }
    }
    #[cfg(not(windows))]
    let _ = (projects, count_untracked_when_clean);
    read().await
}
