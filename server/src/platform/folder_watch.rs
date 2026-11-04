//! Answers kept until something in a project folder or its Git folders changes (Windows).
//!
//! CDXC:Git 2026-11-04 WHY:
//! gxserver read the same Git facts again and again whether or not anything had changed: the sidebar's diff stats about once a second and every session's branch and line counts every minute, each read one or more git starts (and, on Windows, Defender scans). Windows signals a change-notification handle on any write, create, rename or delete under a folder, so a reader keeps its last answer per folder until a handle fires. A watch is reset before the reader runs git: what changes during the run, git's own index refresh included, is read again next time. A folder that cannot be watched (a network share, a path in no repository) is read every time. macOS and Linux keep reading every time: they start git without a launcher or conhost, and a recursive watch there would cost one inotify watch per folder.
//! SEE-ALSO: server/src/project_git_state/diff_stats_cache.rs, server/src/session_git_status/commands.rs.

use std::{
    collections::HashMap,
    os::windows::ffi::OsStrExt,
    path::{Path, PathBuf},
    sync::Mutex,
    time::{Duration, Instant},
};
use windows_sys::Win32::{
    Foundation::{HANDLE, INVALID_HANDLE_VALUE, WAIT_OBJECT_0},
    Storage::FileSystem::{
        FindCloseChangeNotification, FindFirstChangeNotificationW, FindNextChangeNotification,
        FILE_NOTIFY_CHANGE_DIR_NAME, FILE_NOTIFY_CHANGE_FILE_NAME, FILE_NOTIFY_CHANGE_LAST_WRITE,
        FILE_NOTIFY_CHANGE_SIZE,
    },
    System::Threading::WaitForSingleObject,
};

/// Folders nobody asked about for this long stop being watched.
const UNUSED_FOR: Duration = Duration::from_secs(10 * 60);

/// Which Git folders, beside the project folder, an answer depends on.
#[derive(Clone, Copy, PartialEq, Eq)]
pub(crate) enum GitFolders {
    /// HEAD and the index: what a diff against HEAD reads.
    WorkTree,
    /// Those and the repository's shared folder too, whose refs (the default branch, `origin/…`) a comparison with another branch reads.
    WithSharedRefs,
}

/// Last answers per key, each kept while its folders have not changed.
pub(crate) struct ChangeGatedCache<V> {
    entries: Mutex<HashMap<String, Entry<V>>>,
}

struct Entry<V> {
    watch: Option<FolderWatch>,
    answer: Option<V>,
    used: Instant,
}

impl<V: Clone> ChangeGatedCache<V> {
    pub(crate) fn new() -> Self {
        Self {
            entries: Mutex::new(HashMap::new()),
        }
    }

    /// The last answer for `key` when nothing under `project` (and `git`'s folders) changed since it was read. Otherwise the watch is reset, started on the first call, and `None` tells the caller to read and `remember`.
    pub(crate) fn unchanged(&self, key: &str, project: &Path, git: GitFolders) -> Option<V> {
        let mut entries = self.entries.lock().ok()?;
        let now = Instant::now();
        entries.retain(|_, entry| now.duration_since(entry.used) < UNUSED_FOR);
        let entry = entries.entry(key.to_string()).or_insert_with(|| Entry {
            watch: FolderWatch::new(&watched_folders(project, git)),
            answer: None,
            used: now,
        });
        entry.used = now;
        let watch = entry.watch.as_ref()?;
        if watch.take_changed() {
            entry.answer = None;
        }
        entry.answer.clone()
    }

    /// Keeps `answer` for `key`, when its folders are watched.
    pub(crate) fn remember(&self, key: &str, answer: V) {
        if let Ok(mut entries) = self.entries.lock() {
            if let Some(entry) = entries.get_mut(key) {
                if entry.watch.is_some() {
                    entry.answer = Some(answer);
                }
            }
        }
    }
}

/// Change-notification handles on the folders one answer depends on.
struct FolderWatch(Vec<HANDLE>);

// SAFETY: change-notification handles are kernel handles, usable from any thread; every use is under the cache's lock.
unsafe impl Send for FolderWatch {}

impl FolderWatch {
    /// Watches every folder with its subfolders, or `None` when there are none or any one cannot be watched.
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

/// The project folder plus the Git folders outside it that `git` asks for: a linked worktree's `.git` file names its folder under the main repository, whose `commondir` names the shared one, and a project inside a repository has the repository's `.git` above it. Empty when the project is in no repository, so nothing is kept.
fn watched_folders(project: &Path, git: GitFolders) -> Vec<PathBuf> {
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
            let Some(git_dir) = named_folder(&dot_git, directory, "gitdir:") else {
                return Vec::new();
            };
            let mut folders = vec![project.to_path_buf(), git_dir.clone()];
            if git == GitFolders::WithSharedRefs {
                match named_folder(&git_dir.join("commondir"), &git_dir, "") {
                    Some(common) if !folders.iter().any(|folder| folder == &common) => {
                        folders.push(common)
                    }
                    Some(_) => {}
                    None => return Vec::new(),
                }
            }
            return folders;
        }
    }
    Vec::new()
}

/// The folder a one-line Git pointer file names after `prefix`, resolved against `base` when relative.
fn named_folder(file: &Path, base: &Path, prefix: &str) -> Option<PathBuf> {
    let text = std::fs::read_to_string(file).ok()?;
    let named = PathBuf::from(text.lines().next()?.strip_prefix(prefix)?.trim());
    let folder = if named.is_absolute() {
        named
    } else {
        base.join(named)
    };
    folder.is_dir().then_some(folder)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_change_under_the_folder_fires_and_rearms() {
        let temp = tempfile::tempdir().unwrap();
        std::fs::create_dir(temp.path().join(".git")).unwrap();
        let watch = FolderWatch::new(&watched_folders(temp.path(), GitFolders::WorkTree)).unwrap();
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
    fn a_linked_worktree_watches_its_git_folder_and_on_request_the_shared_one() {
        let temp = tempfile::tempdir().unwrap();
        let common = temp.path().join("main").join(".git");
        let git_dir = common.join("worktrees").join("w");
        std::fs::create_dir_all(&git_dir).unwrap();
        std::fs::write(git_dir.join("commondir"), "../..\n").unwrap();
        let worktree = temp.path().join("w");
        std::fs::create_dir(&worktree).unwrap();
        std::fs::write(
            worktree.join(".git"),
            format!("gitdir: {}\n", git_dir.display()),
        )
        .unwrap();
        assert_eq!(
            watched_folders(&worktree, GitFolders::WorkTree),
            vec![worktree.clone(), git_dir.clone()]
        );
        let shared = watched_folders(&worktree, GitFolders::WithSharedRefs);
        assert_eq!(shared.len(), 3);
        assert_eq!(
            std::fs::canonicalize(&shared[2]).unwrap(),
            std::fs::canonicalize(&common).unwrap()
        );
        let watch = FolderWatch::new(&watched_folders(&worktree, GitFolders::WorkTree)).unwrap();
        assert!(!watch.take_changed());
        std::fs::write(git_dir.join("index"), "changed").unwrap();
        assert!(watch.take_changed());
    }

    #[test]
    fn answers_are_kept_until_the_folder_changes_and_never_outside_a_repository() {
        let temp = tempfile::tempdir().unwrap();
        let repo = temp.path().join("repo");
        std::fs::create_dir_all(repo.join(".git")).unwrap();
        let cache = ChangeGatedCache::new();
        assert_eq!(cache.unchanged("r", &repo, GitFolders::WorkTree), None);
        cache.remember("r", 7);
        assert_eq!(cache.unchanged("r", &repo, GitFolders::WorkTree), Some(7));
        std::fs::write(repo.join("file.txt"), "x").unwrap();
        std::thread::sleep(Duration::from_millis(50));
        assert_eq!(cache.unchanged("r", &repo, GitFolders::WorkTree), None);

        let plain = temp.path().join("plain");
        std::fs::create_dir(&plain).unwrap();
        if watched_folders(&plain, GitFolders::WorkTree).is_empty() {
            assert_eq!(cache.unchanged("p", &plain, GitFolders::WorkTree), None);
            cache.remember("p", 1);
            assert_eq!(cache.unchanged("p", &plain, GitFolders::WorkTree), None);
        }
    }
}
