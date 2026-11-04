//! A project's diff stats read again only once something in its folder or its Git folder changed (Windows).
//!
//! CDXC:Git 2026-11-04 WHY:
//! The sidebar reads every visible project header's numbers on a cycle of about one project a second (apps/desktop/src/app/gx_store/git/poll.rs), and each read ran `git diff --numstat HEAD`: on Windows two git processes (Git for Windows' `cmd\git.exe` launcher, then the real git) plus a conhost and a Defender scan, about 0.85 a second with eight sessions open (measured 2026-11-03), whether or not anything had changed. Windows signals a change-notification handle on any write, create, rename or delete under a folder, so gxserver keeps one per project folder (and its Git folder, which holds HEAD and the index outside a linked worktree) and answers from the last read until one fires. A watch is reset before git runs: what changes during the run, git's own index refresh included, re-reads once on the next poll. A folder that cannot be watched (a network share, a missing path) is read every time, as before. macOS and Linux keep reading every time: they start git without a launcher or conhost, and a recursive watch there would cost one inotify watch per folder.

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
        let path = projects
            .first()
            .and_then(|project| project.get("path"))
            .and_then(Value::as_str)
            .map(str::to_string);
        if let Some(path) = path {
            let key = (path, count_untracked_when_clean);
            if let Some(answer) = windows::unchanged_answer(&key) {
                return Ok(answer);
            }
            let result = read().await;
            if let Ok(answer) = &result {
                windows::remember(key, answer.clone());
            }
            return result;
        }
    }
    #[cfg(not(windows))]
    let _ = (projects, count_untracked_when_clean);
    read().await
}

#[cfg(windows)]
mod windows {
    use serde_json::Value;
    use std::{
        collections::HashMap,
        os::windows::ffi::OsStrExt,
        path::{Path, PathBuf},
        sync::{Mutex, OnceLock},
        time::{Duration, Instant},
    };
    use windows_sys::Win32::{
        Foundation::{HANDLE, INVALID_HANDLE_VALUE, WAIT_OBJECT_0},
        Storage::FileSystem::{
            FindCloseChangeNotification, FindFirstChangeNotificationW, FindNextChangeNotification,
            FILE_NOTIFY_CHANGE_DIR_NAME, FILE_NOTIFY_CHANGE_FILE_NAME,
            FILE_NOTIFY_CHANGE_LAST_WRITE, FILE_NOTIFY_CHANGE_SIZE,
        },
        System::Threading::WaitForSingleObject,
    };

    /// A project and whether its untracked files are counted.
    pub(super) type Key = (String, bool);

    /// Projects nobody asked about for this long stop being watched.
    const UNUSED_FOR: Duration = Duration::from_secs(10 * 60);

    /// Change-notification handles on the folders one project's numbers depend on.
    struct FolderWatch(Vec<HANDLE>);

    // SAFETY: change-notification handles are kernel handles, usable from any thread; every use is under the cache's lock.
    unsafe impl Send for FolderWatch {}

    impl FolderWatch {
        /// Watches every folder (with its subfolders), or `None` when any one cannot be watched.
        fn new(folders: &[PathBuf]) -> Option<Self> {
            if folders.is_empty() {
                return None;
            }
            let mut watch = FolderWatch(Vec::new());
            for folder in folders {
                let wide: Vec<u16> = folder.as_os_str().encode_wide().chain(Some(0)).collect();
                // SAFETY: `wide` is NUL-terminated.
                let handle = unsafe {
                    FindFirstChangeNotificationW(
                        wide.as_ptr(),
                        1,
                        FILE_NOTIFY_CHANGE_FILE_NAME
                            | FILE_NOTIFY_CHANGE_DIR_NAME
                            | FILE_NOTIFY_CHANGE_SIZE
                            | FILE_NOTIFY_CHANGE_LAST_WRITE,
                    )
                };
                if handle == INVALID_HANDLE_VALUE || handle.is_null() {
                    return None;
                }
                watch.0.push(handle);
            }
            Some(watch)
        }

        /// Whether anything changed since the watch started or since the last call, re-arming the handles that fired.
        fn take_changed(&self) -> bool {
            let mut changed = false;
            for &handle in &self.0 {
                // SAFETY: `handle` is a live change-notification handle owned by this watch.
                if unsafe { WaitForSingleObject(handle, 0) } == WAIT_OBJECT_0 {
                    changed = true;
                    unsafe { FindNextChangeNotification(handle) };
                }
            }
            changed
        }
    }

    impl Drop for FolderWatch {
        fn drop(&mut self) {
            for &handle in &self.0 {
                // SAFETY: each handle came from FindFirstChangeNotificationW and is closed once.
                unsafe { FindCloseChangeNotification(handle) };
            }
        }
    }

    struct Entry {
        watch: Option<FolderWatch>,
        answer: Option<Value>,
        used: Instant,
    }

    fn cache() -> &'static Mutex<HashMap<Key, Entry>> {
        static CACHE: OnceLock<Mutex<HashMap<Key, Entry>>> = OnceLock::new();
        CACHE.get_or_init(Default::default)
    }

    /// The last answer for `key` when its folders have not changed since it was read. Otherwise the watch is reset (started on the first read) and `None` tells the caller to read.
    pub(super) fn unchanged_answer(key: &Key) -> Option<Value> {
        let mut cache = cache().lock().ok()?;
        let now = Instant::now();
        cache.retain(|_, entry| now.duration_since(entry.used) < UNUSED_FOR);
        let entry = cache.entry(key.clone()).or_insert_with(|| Entry {
            watch: FolderWatch::new(&watched_folders(Path::new(&key.0))),
            answer: None,
            used: now,
        });
        entry.used = now;
        let watch = entry.watch.as_ref()?;
        // Reset before the read, so a change made while git runs is read next time.
        if watch.take_changed() {
            entry.answer = None;
        }
        entry.answer.clone()
    }

    pub(super) fn remember(key: Key, answer: Value) {
        if let Ok(mut cache) = cache().lock() {
            if let Some(entry) = cache.get_mut(&key) {
                if entry.watch.is_some() {
                    entry.answer = Some(answer);
                }
            }
        }
    }

    /// The project folder, plus the Git folder when it lies outside it: a linked worktree's `.git` file names its folder under the main repository, and a project inside a repository has the repository's `.git` above it. Empty when the project is in no repository, so it is never cached.
    fn watched_folders(project: &Path) -> Vec<PathBuf> {
        for directory in project.ancestors() {
            let dot_git = directory.join(".git");
            if dot_git.is_dir() {
                return if directory == project {
                    vec![project.to_path_buf()]
                } else {
                    vec![project.to_path_buf(), dot_git]
                };
            }
            if dot_git.is_file() {
                let Some(git_dir) = std::fs::read_to_string(&dot_git).ok().and_then(|text| {
                    let named = text
                        .lines()
                        .next()?
                        .strip_prefix("gitdir:")?
                        .trim()
                        .to_string();
                    let named = PathBuf::from(named);
                    Some(if named.is_absolute() {
                        named
                    } else {
                        directory.join(named)
                    })
                }) else {
                    return Vec::new();
                };
                return if git_dir.is_dir() {
                    vec![project.to_path_buf(), git_dir]
                } else {
                    Vec::new()
                };
            }
        }
        Vec::new()
    }

    #[cfg(test)]
    mod tests {
        use super::*;

        #[test]
        fn a_change_under_the_folder_fires_once_and_rearms() {
            let temp = tempfile::tempdir().unwrap();
            std::fs::create_dir(temp.path().join(".git")).unwrap();
            let watch = FolderWatch::new(&watched_folders(temp.path())).unwrap();
            assert!(!watch.take_changed());
            std::fs::create_dir(temp.path().join("src")).unwrap();
            std::fs::write(temp.path().join("src").join("main.rs"), "fn main() {}").unwrap();
            assert!(watch.take_changed());
            // Windows reports a burst of changes as it delivers them; once they are all taken the watch is quiet until the next one.
            let quiet = (0..50).any(|_| {
                std::thread::sleep(Duration::from_millis(20));
                !watch.take_changed()
            });
            assert!(quiet);
            std::thread::sleep(Duration::from_millis(100));
            assert!(!watch.take_changed());
            std::fs::write(temp.path().join("src").join("main.rs"), "fn main() { }").unwrap();
            assert!(watch.take_changed());
        }

        #[test]
        fn a_linked_worktree_also_watches_its_git_folder() {
            let temp = tempfile::tempdir().unwrap();
            let git_dir = temp
                .path()
                .join("main")
                .join(".git")
                .join("worktrees")
                .join("w");
            std::fs::create_dir_all(&git_dir).unwrap();
            let worktree = temp.path().join("w");
            std::fs::create_dir(&worktree).unwrap();
            std::fs::write(
                worktree.join(".git"),
                format!("gitdir: {}\n", git_dir.display()),
            )
            .unwrap();
            assert_eq!(
                watched_folders(&worktree),
                vec![worktree.clone(), git_dir.clone()]
            );
            let watch = FolderWatch::new(&watched_folders(&worktree)).unwrap();
            assert!(!watch.take_changed());
            std::fs::write(git_dir.join("index"), "changed").unwrap();
            assert!(watch.take_changed());
        }

        #[test]
        fn a_folder_in_no_repository_is_not_cached() {
            let temp = tempfile::tempdir().unwrap();
            let key = (
                temp.path().join("plain").to_string_lossy().into_owned(),
                false,
            );
            std::fs::create_dir(&key.0).unwrap();
            if watched_folders(Path::new(&key.0)).is_empty() {
                assert!(unchanged_answer(&key).is_none());
                remember(key.clone(), serde_json::json!({"isRepo": false}));
                assert!(unchanged_answer(&key).is_none());
            }
        }
    }
}
