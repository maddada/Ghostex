//! Answers kept until something in a project folder or its Git folders changes (Windows and macOS).
//!
//! CDXC:Git 2026-11-04 WHY:
//! gxserver read the same Git facts again and again whether or not anything had changed: the sidebar's diff stats about once a second and every session's branch and line counts every minute, each read one or more git starts (and, on Windows, Defender scans). Windows signals a change-notification handle on any write, create, rename or delete under a folder, so a reader keeps its last answer per folder until a handle fires. A watch is reset before the reader runs git: what changes during the run, git's own index refresh included, is read again next time. A folder that cannot be watched (a network share, a path in no repository) is read every time. Linux keeps reading every time: a recursive watch there would cost one inotify watch per folder. macOS watches with FSEvents (`folder_watch_macos.rs`, CDXC:Git 2026-11-05).
//! SEE-ALSO: server/src/project_git_state/diff_stats_cache.rs, server/src/session_git_status/commands.rs.

#[cfg(target_os = "macos")]
use super::folder_watch_macos::FolderWatch;
#[cfg(windows)]
use std::os::windows::ffi::OsStrExt;
use std::{
    collections::HashMap,
    path::{Path, PathBuf},
    sync::Mutex,
    time::{Duration, Instant},
};
#[cfg(windows)]
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
    /// When the watch started (`WATCH_SETTLE`).
    #[cfg_attr(not(target_os = "macos"), allow(dead_code))]
    watched_at: Instant,
    /// When the answer was read, for a folder in no repository (`OUTSIDE_REPOSITORY_FOR`).
    #[cfg_attr(not(target_os = "macos"), allow(dead_code))]
    outside_repository_read: Option<Instant>,
    outside_repository: bool,
}

/// How long the answer for a folder in no repository is kept (macOS).
///
/// CDXC:Git 2026-11-05 WHY:
/// A project in no repository answered "not a repository" with two git starts (`diff`, then `rev-parse`) on every header poll, because a folder outside a repository has no Git folders to watch (measured 2026-11-05 on macOS, issues-1027/macos-perf). macOS watches the folder itself, which sees a `git init` in it; a repository created in a folder above it is not seen there, so the answer is also read again after this long.
#[cfg(target_os = "macos")]
const OUTSIDE_REPOSITORY_FOR: Duration = Duration::from_secs(5 * 60);

/// How long a new FSEvents stream may still miss a change (macOS).
///
/// CDXC:Git 2026-11-05 WHY:
/// FSEvents starts a stream asynchronously, so a write in its first moments can go unreported, and the answer read then was kept until something else changed: a checkout's numbers stayed at what they were when Ghostex first looked (seen as `the_default_branch_diffs_its_working_tree_against_head` failing whenever the first read and the write ran a few milliseconds apart). An answer read while the stream is this young is not kept, so the next ask reads again.
#[cfg(target_os = "macos")]
const WATCH_SETTLE: Duration = Duration::from_millis(500);

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
        let entry = entries.entry(key.to_string()).or_insert_with(|| {
            let folders = watched_folders(project, git);
            // macOS watches a folder in no repository too (`OUTSIDE_REPOSITORY_FOR`).
            #[cfg(target_os = "macos")]
            if folders.is_empty() && project.is_dir() {
                return Entry {
                    watch: FolderWatch::new(&[project.to_path_buf()]),
                    answer: None,
                    used: now,
                    watched_at: now,
                    outside_repository_read: None,
                    outside_repository: true,
                };
            }
            // macOS skips the changes git never sees (`folder_watch_macos::Signal`), once a probe
            // off this lock names them (`start_skip_probe`).
            #[cfg(target_os = "macos")]
            let watch = FolderWatch::skipping(&folders);
            #[cfg(not(target_os = "macos"))]
            let watch = FolderWatch::new(&folders);
            Entry {
                watch,
                answer: None,
                used: now,
                // When the stream started, for `WATCH_SETTLE`.
                watched_at: Instant::now(),
                outside_repository_read: None,
                outside_repository: false,
            }
        });
        entry.used = now;
        let watch = entry.watch.as_ref()?;
        if watch.take_changed() {
            entry.answer = None;
        }
        #[cfg(target_os = "macos")]
        if let Some(probe) = watch.skip_probe() {
            start_skip_probe(probe, project, git);
        }
        #[cfg(target_os = "macos")]
        if entry.outside_repository
            && entry
                .outside_repository_read
                .is_none_or(|read| now.duration_since(read) >= OUTSIDE_REPOSITORY_FOR)
        {
            entry.answer = None;
        }
        entry.answer.clone()
    }

    /// Whether `key`'s folders were found unwatchable (a network share, a folder in no repository
    /// on Windows), so nothing is kept for it. `false` before the first look.
    pub(crate) fn unwatched(&self, key: &str) -> bool {
        self.entries
            .lock()
            .is_ok_and(|entries| entries.get(key).is_some_and(|entry| entry.watch.is_none()))
    }

    /// Keeps `answer` for `key`, when its folders are watched.
    pub(crate) fn remember(&self, key: &str, answer: V) {
        if let Ok(mut entries) = self.entries.lock() {
            if let Some(entry) = entries.get_mut(key) {
                // `used` is when the caller was told to read (`unchanged`).
                #[cfg(target_os = "macos")]
                if entry.used.duration_since(entry.watched_at) < WATCH_SETTLE {
                    return;
                }
                if entry.watch.is_some() {
                    entry.answer = Some(answer);
                    if entry.outside_repository {
                        entry.outside_repository_read = Some(Instant::now());
                    }
                }
            }
        }
    }
}

/// Change-notification handles on the folders one answer depends on.
#[cfg(windows)]
struct FolderWatch(Vec<HANDLE>);

// SAFETY: change-notification handles are kernel handles, usable from any thread; every use is under the cache's lock.
#[cfg(windows)]
unsafe impl Send for FolderWatch {}

#[cfg(windows)]
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

#[cfg(windows)]
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

/// Folders builds and package managers write into, asked about even before they exist.
#[cfg(target_os = "macos")]
const BUILD_FOLDERS: &[&str] = &[
    "target",
    "node_modules",
    "dist",
    "build",
    "out",
    ".next",
    ".nuxt",
    ".turbo",
    ".cache",
    ".build",
    "coverage",
    "DerivedData",
    ".gradle",
    ".venv",
    "__pycache__",
];

/// Asks git which folders `probe`'s watch may skip, on a thread of its own: the cache's lock is
/// never held while git runs, and a project whose git hangs holds up only its own probe.
#[cfg(target_os = "macos")]
fn start_skip_probe(probe: super::folder_watch_macos::SkipProbe, project: &Path, git: GitFolders) {
    let project = project.to_path_buf();
    let spawned = std::thread::Builder::new()
        .name("git-skip-probe".to_string())
        .spawn(move || {
            let folders = watched_folders(&project, git);
            probe.install(git_ignored_folders(&project, &folders));
        });
    // No thread: nothing is skipped, and the next look tries again.
    drop(spawned);
}

/// The real paths of `project`'s top-level folders (and `BUILD_FOLDERS`) that git ignores, that
/// hold no tracked file and that hold none of `watched` (its Git folders): nothing written in them
/// changes a Git answer. Empty when git cannot say, so every change counts, as before.
#[cfg(target_os = "macos")]
fn git_ignored_folders(project: &Path, watched: &[PathBuf]) -> Vec<PathBuf> {
    use crate::session_git_status::run_git_probe_command;
    let Some(cwd) = project.to_str() else {
        return Vec::new();
    };
    let mut names: std::collections::BTreeSet<String> =
        BUILD_FOLDERS.iter().map(|name| name.to_string()).collect();
    if let Ok(entries) = std::fs::read_dir(project) {
        for entry in entries.flatten() {
            if !entry.file_type().is_ok_and(|kind| kind.is_dir()) {
                continue;
            }
            if let Some(name) = entry.file_name().to_str().filter(|name| *name != ".git") {
                names.insert(name.to_string());
            }
        }
    }
    // One name per line (`-z` needs `--stdin`); a name git quotes simply never matches an event.
    let mut args = vec!["check-ignore".to_string(), "--".to_string()];
    args.extend(names.iter().map(|name| format!("{name}/")));
    let args: Vec<&str> = args.iter().map(String::as_str).collect();
    // Exits 1 when nothing is ignored: no answer, nothing skipped.
    let Some(ignored) = run_git_probe_command(cwd, &args) else {
        return Vec::new();
    };
    let ignored: Vec<&str> = ignored
        .lines()
        .map(|name| name.trim_end_matches('/'))
        .filter(|name| !name.is_empty() && !name.contains('/'))
        .collect();
    if ignored.is_empty() {
        return Vec::new();
    }
    // A folder holding a tracked file (added with `git add -f`) shows its changes in git.
    let mut args = vec!["ls-files", "-z", "--"];
    args.extend(ignored.iter().copied());
    let Some(tracked) = run_git_probe_command(cwd, &args) else {
        return Vec::new();
    };
    // macOS folders ignore case, so `Target` holding a tracked file keeps `target` too.
    let tracked: std::collections::HashSet<String> = tracked
        .split('\0')
        .filter_map(|path| path.split('/').next())
        .filter(|name| !name.is_empty())
        .map(str::to_ascii_lowercase)
        .collect();
    let Ok(root) = std::fs::canonicalize(project) else {
        return Vec::new();
    };
    let watched: Vec<PathBuf> = watched
        .iter()
        .filter_map(|folder| std::fs::canonicalize(folder).ok())
        .collect();
    ignored
        .into_iter()
        .filter(|name| !tracked.contains(&name.to_ascii_lowercase()))
        .map(|name| root.join(name))
        // A linked worktree's Git folder can sit inside an ignored folder; its writes count.
        .filter(|folder| {
            !watched
                .iter()
                .any(|git| git != &root && git.starts_with(folder))
        })
        .collect()
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

    /// Whether the watch reports a change within two seconds: Windows signals at once, FSEvents
    /// after its latency.
    fn fires(watch: &FolderWatch) -> bool {
        (0..100).any(|_| {
            if watch.take_changed() {
                return true;
            }
            std::thread::sleep(Duration::from_millis(20));
            false
        })
    }

    #[test]
    fn a_change_under_the_folder_fires_and_rearms() {
        let temp = tempfile::tempdir().unwrap();
        std::fs::create_dir(temp.path().join(".git")).unwrap();
        let watch = FolderWatch::new(&watched_folders(temp.path(), GitFolders::WorkTree)).unwrap();
        assert!(!watch.take_changed());
        std::fs::create_dir(temp.path().join("src")).unwrap();
        std::fs::write(temp.path().join("src").join("main.rs"), "fn main() {}").unwrap();
        assert!(fires(&watch));
        // Windows reports a burst of changes as it delivers them, FSEvents in batches a latency
        // apart; once they are all taken the watch stays quiet until the next one.
        let quiet = (0..20).any(|_| {
            std::thread::sleep(Duration::from_millis(300));
            !watch.take_changed()
        });
        assert!(quiet);
        std::thread::sleep(Duration::from_millis(300));
        assert!(!watch.take_changed());
        std::fs::write(temp.path().join("src").join("main.rs"), "fn main() { }").unwrap();
        assert!(fires(&watch));
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
        assert!(fires(&watch));
    }

    /// Waits out a new macOS stream's `WATCH_SETTLE` and asks again, so the next answer is kept.
    fn settle(cache: &ChangeGatedCache<i32>, key: &str, folder: &Path) {
        #[cfg(target_os = "macos")]
        {
            std::thread::sleep(WATCH_SETTLE);
            assert_eq!(cache.unchanged(key, folder, GitFolders::WorkTree), None);
        }
        #[cfg(not(target_os = "macos"))]
        let _ = (cache, key, folder);
    }

    /// A write in a new stream's first moments can go unreported, so what was read then is read
    /// again, and the first answer read once the stream settled is kept.
    #[test]
    #[cfg(target_os = "macos")]
    fn an_answer_read_while_the_stream_starts_is_read_again() {
        let temp = tempfile::tempdir().unwrap();
        let repo = temp.path().join("repo");
        std::fs::create_dir_all(repo.join(".git")).unwrap();
        let cache = ChangeGatedCache::new();
        assert_eq!(cache.unchanged("r", &repo, GitFolders::WorkTree), None);
        cache.remember("r", 1);
        assert_eq!(cache.unchanged("r", &repo, GitFolders::WorkTree), None);
        std::thread::sleep(WATCH_SETTLE);
        assert_eq!(cache.unchanged("r", &repo, GitFolders::WorkTree), None);
        cache.remember("r", 2);
        assert_eq!(cache.unchanged("r", &repo, GitFolders::WorkTree), Some(2));
    }

    #[test]
    fn answers_are_kept_until_the_folder_changes_and_never_outside_a_repository() {
        let temp = tempfile::tempdir().unwrap();
        let repo = temp.path().join("repo");
        std::fs::create_dir_all(repo.join(".git")).unwrap();
        let cache = ChangeGatedCache::new();
        assert_eq!(cache.unchanged("r", &repo, GitFolders::WorkTree), None);
        settle(&cache, "r", &repo);
        cache.remember("r", 7);
        assert_eq!(cache.unchanged("r", &repo, GitFolders::WorkTree), Some(7));
        std::fs::write(repo.join("file.txt"), "x").unwrap();
        let dropped = (0..100).any(|_| {
            std::thread::sleep(Duration::from_millis(20));
            cache.unchanged("r", &repo, GitFolders::WorkTree).is_none()
        });
        assert!(dropped);

        let plain = temp.path().join("plain");
        std::fs::create_dir(&plain).unwrap();
        if watched_folders(&plain, GitFolders::WorkTree).is_empty() {
            assert_eq!(cache.unchanged("p", &plain, GitFolders::WorkTree), None);
            settle(&cache, "p", &plain);
            cache.remember("p", 1);
            // Windows reads a folder in no repository every time; macOS watches the folder itself.
            #[cfg(windows)]
            assert_eq!(cache.unchanged("p", &plain, GitFolders::WorkTree), None);
            #[cfg(target_os = "macos")]
            {
                assert_eq!(cache.unchanged("p", &plain, GitFolders::WorkTree), Some(1));
                // `git init` there is a change.
                std::fs::create_dir(plain.join(".git")).unwrap();
                let dropped = (0..100).any(|_| {
                    std::thread::sleep(Duration::from_millis(20));
                    cache.unchanged("p", &plain, GitFolders::WorkTree).is_none()
                });
                assert!(dropped);
            }
        }
    }

    /// The reads a poll runs must not wake the watch they are kept under, or no answer would ever be
    /// kept. `git status` refreshes the index unless optional locks are off.
    #[test]
    #[cfg(target_os = "macos")]
    fn the_polls_own_git_reads_leave_the_watch_quiet() {
        let temp = tempfile::tempdir().unwrap();
        let repo = temp.path();
        let git = |args: &[&str]| {
            std::process::Command::new("git")
                .args(args)
                .current_dir(repo)
                .env("GIT_CONFIG_GLOBAL", "/dev/null")
                .output()
                .unwrap()
        };
        git(&["init", "-q"]);
        std::fs::write(repo.join("a.txt"), "one\n").unwrap();
        git(&["add", "a.txt"]);
        git(&[
            "-c",
            "user.email=a@b",
            "-c",
            "user.name=n",
            "commit",
            "-qm",
            "a",
        ]);
        std::fs::write(repo.join("a.txt"), "one\ntwo\n").unwrap();
        // Let the index go stale against the file's timestamp, as it does between polls.
        std::thread::sleep(Duration::from_millis(1100));
        let watch = FolderWatch::new(&watched_folders(repo, GitFolders::WithSharedRefs)).unwrap();
        while fires(&watch) {}
        for _ in 0..3 {
            git(&[
                "--no-optional-locks",
                "diff",
                "--numstat",
                "--ignore-submodules=dirty",
                "HEAD",
            ]);
            git(&["--no-optional-locks", "status", "--porcelain"]);
            git(&["ls-files", "--others", "--exclude-standard", "-z"]);
            git(&["rev-parse", "--is-inside-work-tree"]);
        }
        std::thread::sleep(Duration::from_millis(400));
        assert!(!watch.take_changed());
        // A plain `git status` does write the index, which is why the polls turn optional locks off.
        git(&["status", "--porcelain"]);
        assert!(fires(&watch));
    }
}
