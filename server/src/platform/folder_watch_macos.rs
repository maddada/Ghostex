//! `folder_watch`'s change signal on macOS: one FSEvents stream per watch.
//!
//! CDXC:Git 2026-11-05 WHY:
//! macOS read every Git answer again on each poll as well: the sidebar's diff stats, the active project's full read every 15 seconds and every session's branch and line counts every minute, about ten git processes a minute per project and 0.4 s of CPU for one `git status` in a repository with submodules (measured 2026-11-04, issues-1027/macos-perf). FSEvents watches a whole tree with one stream and no per-folder cost, so the Windows change-gated answers (`folder_watch`) apply here too. A stream only raises a flag; the reader reads again when it finds the flag raised. Events arrive after `LATENCY`, so a change made in the moment before a poll is read on the next one.

use std::{
    ffi::c_void,
    os::unix::ffi::OsStrExt,
    path::PathBuf,
    sync::{
        Arc, Mutex, OnceLock, RwLock,
        atomic::{AtomicBool, AtomicU64, Ordering},
    },
};

/// How long FSEvents gathers events before it calls back, in seconds.
const LATENCY: f64 = 0.1;
// No `kFSEventStreamCreateFlagWatchRoot`: it opens every folder above each watched path to notice
// the path moving, about eight descriptors per watch, which ran gxserver out of its 256 descriptors
// with a few dozen projects and sessions watched (no process could start; rebuild 111, 2026-11-05).
// A stream without it holds no descriptor in gxserver.

type CFAllocatorRef = *const c_void;
type CFStringRef = *const c_void;
type CFArrayRef = *const c_void;
type FSEventStreamRef = *mut c_void;
type DispatchQueue = *mut c_void;

#[repr(C)]
struct FSEventStreamContext {
    version: isize,
    info: *mut c_void,
    retain: *const c_void,
    release: *const c_void,
    copy_description: *const c_void,
}

type FSEventStreamCallback = extern "C" fn(
    stream: FSEventStreamRef,
    info: *mut c_void,
    count: usize,
    paths: *mut c_void,
    flags: *const u32,
    ids: *const u64,
);

const K_CF_STRING_ENCODING_UTF8: u32 = 0x0800_0100;
const K_FS_EVENT_STREAM_EVENT_ID_SINCE_NOW: u64 = u64::MAX;
const K_FS_EVENT_STREAM_CREATE_FLAG_NO_DEFER: u32 = 0x02;

#[link(name = "CoreServices", kind = "framework")]
unsafe extern "C" {
    fn FSEventStreamCreate(
        allocator: CFAllocatorRef,
        callback: FSEventStreamCallback,
        context: *const FSEventStreamContext,
        paths: CFArrayRef,
        since_when: u64,
        latency: f64,
        flags: u32,
    ) -> FSEventStreamRef;
    fn FSEventStreamSetDispatchQueue(stream: FSEventStreamRef, queue: DispatchQueue);
    fn FSEventStreamStart(stream: FSEventStreamRef) -> u8;
    fn FSEventStreamStop(stream: FSEventStreamRef);
    fn FSEventStreamInvalidate(stream: FSEventStreamRef);
    fn FSEventStreamRelease(stream: FSEventStreamRef);
}

#[link(name = "CoreFoundation", kind = "framework")]
unsafe extern "C" {
    static kCFTypeArrayCallBacks: c_void;
    fn CFStringCreateWithBytes(
        allocator: CFAllocatorRef,
        bytes: *const u8,
        count: isize,
        encoding: u32,
        external: u8,
    ) -> CFStringRef;
    fn CFArrayCreate(
        allocator: CFAllocatorRef,
        values: *const *const c_void,
        count: isize,
        callbacks: *const c_void,
    ) -> CFArrayRef;
    fn CFRelease(object: *const c_void);
}

unsafe extern "C" {
    fn dispatch_queue_create(
        label: *const std::ffi::c_char,
        attributes: *const c_void,
    ) -> DispatchQueue;
    fn dispatch_sync_f(
        queue: DispatchQueue,
        context: *mut c_void,
        work: extern "C" fn(*mut c_void),
    );
}

/// The one serial queue every watch's callbacks run on. Never released: it lives as long as gxserver.
fn queue() -> DispatchQueue {
    struct Queue(DispatchQueue);
    // SAFETY: a dispatch queue is usable from any thread.
    unsafe impl Send for Queue {}
    unsafe impl Sync for Queue {}
    static QUEUE: OnceLock<Queue> = OnceLock::new();
    QUEUE
        .get_or_init(|| {
            // SAFETY: a NUL-terminated label and the default (serial) attributes.
            Queue(unsafe {
                dispatch_queue_create(c"gxserver.folder-watch".as_ptr(), std::ptr::null())
            })
        })
        .0
}

/// File events, so a change names its file (an ignore rule edited) and not only its folder.
const K_FS_EVENT_STREAM_CREATE_FLAG_FILE_EVENTS: u32 = 0x10;
/// Event flags that say events were lost or the watched tree moved: always a change.
const K_FS_EVENT_STREAM_EVENT_FLAGS_LOST: u32 = 0x01 | 0x02 | 0x04 | 0x20 | 0x40 | 0x80;

/// What a stream's callback writes: the flag the reader takes, and the folders whose changes git
/// never sees.
///
/// CDXC:Git 2026-11-18 WHY:
/// GitHub #666: a build writing into an ignored folder (`target/`, `node_modules/`) raised the
/// flag like any other write, so a busy project's Git answers were thrown away on almost every
/// poll and read again with several git processes each time. A change inside a folder git
/// ignores and that holds no tracked file cannot change any Git answer, so it no longer raises
/// the flag. Which folders those are is asked of git off the cache's lock, on a thread of its own
/// (`folder_watch::start_skip_probe`), and installed here; until then, and whenever the list may
/// be out of date (an ignore rule or a watched Git folder's index written, a `git add -f` or a
/// checkout among them, or events lost), every change counts, as before, and git is asked again.
/// `generation` moves each time the list goes out of date, and the list counts only while the
/// generation its probe began with (`installed`) is the current one, so a probe that began before
/// cannot make it current. Rules from outside the watched folders (a global excludes file, a
/// parent repository's `.gitignore`) are picked up by asking again every `REASK_AFTER`.
struct Signal {
    changed: AtomicBool,
    /// Each skipped folder's real path with a trailing `/`.
    ignored: RwLock<Vec<Vec<u8>>>,
    generation: AtomicU64,
    /// The generation the installed list's probe began with.
    installed: AtomicU64,
    probing: AtomicBool,
    /// When the list was last installed, or a probe git did not answer gave up.
    asked_at: Mutex<Option<std::time::Instant>>,
    /// Probes in a row git did not answer: the next waits `RETRY_AFTER` for that many.
    unanswered: AtomicU64,
    /// This watch's share of `REASK_JITTER`, so watches started together do not ask together.
    reask_jitter: std::time::Duration,
    /// The real paths of the watched Git folders' index files.
    index_files: Vec<Vec<u8>>,
    /// Whether this watch skips anything at all (a repository's watch).
    skips: bool,
}

/// How long after git did not answer a probe before asking again: longer each time in a row.
const RETRY_AFTER: [std::time::Duration; 3] = [
    std::time::Duration::from_secs(30),
    std::time::Duration::from_secs(60),
    std::time::Duration::from_secs(5 * 60),
];
/// Up to this much is added to `REASK_AFTER`, a different amount per watch.
const REASK_JITTER: std::time::Duration = std::time::Duration::from_secs(60);
/// How long an installed list is trusted before git is asked again.
const REASK_AFTER: std::time::Duration = std::time::Duration::from_secs(5 * 60);

impl Signal {
    fn mark_stale(&self) {
        self.generation.fetch_add(1, Ordering::AcqRel);
    }

    /// Whether the installed list is current.
    fn current(&self) -> bool {
        self.installed.load(Ordering::Acquire) == self.generation.load(Ordering::Acquire)
    }

    /// Whether one event can change a Git answer.
    fn counts(&self, path: &[u8], flags: u32) -> bool {
        if !self.skips {
            return true;
        }
        if flags & K_FS_EVENT_STREAM_EVENT_FLAGS_LOST != 0 {
            self.mark_stale();
            return true;
        }
        if self.current()
            && self
                .ignored
                .read()
                .is_ok_and(|ignored| ignored.iter().any(|folder| path.starts_with(folder)))
        {
            return false;
        }
        let index = |file: &Vec<u8>| {
            path == file.as_slice()
                || path
                    .strip_suffix(b".lock")
                    .is_some_and(|base| base == file.as_slice())
        };
        if path.ends_with(b"/.gitignore")
            || path.ends_with(b"/info/exclude")
            || self.index_files.iter().any(index)
        {
            self.mark_stale();
        }
        true
    }
}

/// The right to ask git for one watch's skip list, held by one probe at a time.
pub(super) struct SkipProbe {
    signal: Arc<Signal>,
    generation: u64,
}

impl SkipProbe {
    /// Installs `ignored` (real paths) as the skipped folders; the list counts as current only when
    /// nothing made it stale since the probe began.
    pub(super) fn install(self, ignored: Vec<PathBuf>) {
        self.signal.unanswered.store(0, Ordering::Release);
        if let Ok(mut asked_at) = self.signal.asked_at.lock() {
            *asked_at = Some(std::time::Instant::now());
        }
        let ignored: Vec<Vec<u8>> = ignored
            .iter()
            .map(|folder| {
                let mut bytes = folder.as_os_str().as_bytes().to_vec();
                bytes.push(b'/');
                bytes
            })
            .collect();
        // Only a list that was in force skipped anything: the kept answer goes when a re-ask
        // finds different rules (review of 6f2bf25bc), not when a stale list is replaced.
        let was_current = self.signal.current();
        if let Ok(mut list) = self.signal.ignored.write() {
            if was_current && *list != ignored {
                self.signal.changed.store(true, Ordering::Release);
            }
            *list = ignored;
        }
        self.signal
            .installed
            .store(self.generation, Ordering::Release);
    }

    /// Git did not answer: the list stays as it was, and the next probe waits `RETRY_AFTER`.
    pub(super) fn unanswered(self) {
        self.signal.unanswered.fetch_add(1, Ordering::AcqRel);
        if let Ok(mut asked_at) = self.signal.asked_at.lock() {
            *asked_at = Some(std::time::Instant::now());
        }
    }
}

impl Drop for SkipProbe {
    fn drop(&mut self) {
        self.signal.probing.store(false, Ordering::Release);
    }
}

extern "C" fn changed(
    _stream: FSEventStreamRef,
    info: *mut c_void,
    count: usize,
    paths: *mut c_void,
    flags: *const u32,
    _ids: *const u64,
) {
    if count == 0 {
        return;
    }
    // SAFETY: `info` is the `Signal` the watch keeps alive until its stream is invalidated and the
    // queue has drained (`Drop`).
    let signal = unsafe { &*(info as *const Signal) };
    // Every event is looked at (no early stop), so an ignore rule written in the same batch as
    // another change still turns the filtering off.
    let counts = (0..count).fold(false, |counts, index| {
        // SAFETY: without `kFSEventStreamCreateFlagUseCFTypes`, `paths` is an array of `count`
        // NUL-terminated C strings and `flags` an array of `count` flags, valid for this call.
        let (path, flags) = unsafe {
            (
                std::ffi::CStr::from_ptr(*(paths as *const *const std::ffi::c_char).add(index))
                    .to_bytes(),
                *flags.add(index),
            )
        };
        signal.counts(path, flags) || counts
    });
    if counts {
        signal.changed.store(true, Ordering::Release);
    }
}

extern "C" fn nothing(_: *mut c_void) {}

/// An FSEvents stream over the folders one answer depends on.
pub(super) struct FolderWatch {
    stream: FSEventStreamRef,
    signal: Arc<Signal>,
}

// SAFETY: the stream is only started, stopped and released through this value, under the cache's lock.
unsafe impl Send for FolderWatch {}

impl FolderWatch {
    /// Watches every folder with everything under it, or `None` when there are none or the stream cannot start.
    pub(super) fn new(folders: &[PathBuf]) -> Option<Self> {
        Self::start(folders, false)
    }

    /// `new` for a repository's folders (the project first, then its Git folders): it starts
    /// counting every change, and skips the folders a `SkipProbe` installs (`Signal`).
    pub(super) fn skipping(folders: &[PathBuf]) -> Option<Self> {
        Self::start(folders, true)
    }

    fn start(folders: &[PathBuf], file_events: bool) -> Option<Self> {
        if folders.is_empty() {
            return None;
        }
        let real =
            |folder: &PathBuf| std::fs::canonicalize(folder).unwrap_or_else(|_| folder.clone());
        // The project's own `.git` folder, or the Git folders named after it.
        let git_folders: Vec<PathBuf> = if folders.len() == 1 {
            vec![real(&folders[0]).join(".git")]
        } else {
            folders[1..].iter().map(real).collect()
        };
        let signal = Arc::new(Signal {
            changed: AtomicBool::new(false),
            ignored: RwLock::new(Vec::new()),
            // Nothing installed yet: not current until the first probe answers.
            generation: AtomicU64::new(1),
            installed: AtomicU64::new(0),
            probing: AtomicBool::new(false),
            asked_at: Mutex::new(None),
            unanswered: AtomicU64::new(0),
            // Spread over the watches by the order they start in.
            reask_jitter: {
                static STARTED: AtomicU64 = AtomicU64::new(0);
                let nth = STARTED.fetch_add(1, Ordering::Relaxed);
                REASK_JITTER.mul_f64((nth.wrapping_mul(7919) % 997) as f64 / 997.0)
            },
            index_files: git_folders
                .iter()
                .map(|folder| folder.join("index").as_os_str().as_bytes().to_vec())
                .collect(),
            skips: file_events,
        });
        let create_flags = if file_events {
            K_FS_EVENT_STREAM_CREATE_FLAG_NO_DEFER | K_FS_EVENT_STREAM_CREATE_FLAG_FILE_EVENTS
        } else {
            K_FS_EVENT_STREAM_CREATE_FLAG_NO_DEFER
        };
        // SAFETY: CoreFoundation objects are created here and released once the stream holds them.
        unsafe {
            // FSEvents reports real paths; a folder reached through a symlink (`/var` is
            // `/private/var`) is watched where it really is.
            let folders: Vec<PathBuf> = folders
                .iter()
                .map(|folder| std::fs::canonicalize(folder).unwrap_or_else(|_| folder.clone()))
                .collect();
            let strings: Vec<CFStringRef> = folders
                .iter()
                .map(|folder| {
                    let bytes = folder.as_os_str().as_bytes();
                    CFStringCreateWithBytes(
                        std::ptr::null(),
                        bytes.as_ptr(),
                        bytes.len() as isize,
                        K_CF_STRING_ENCODING_UTF8,
                        0,
                    )
                })
                .collect();
            if strings.iter().any(|string| string.is_null()) {
                strings
                    .iter()
                    .filter(|string| !string.is_null())
                    .for_each(|string| CFRelease(*string));
                return None;
            }
            let paths = CFArrayCreate(
                std::ptr::null(),
                strings.as_ptr(),
                strings.len() as isize,
                &kCFTypeArrayCallBacks as *const c_void,
            );
            strings.iter().for_each(|string| CFRelease(*string));
            if paths.is_null() {
                return None;
            }
            let context = FSEventStreamContext {
                version: 0,
                info: Arc::as_ptr(&signal) as *mut c_void,
                retain: std::ptr::null(),
                release: std::ptr::null(),
                copy_description: std::ptr::null(),
            };
            let stream = FSEventStreamCreate(
                std::ptr::null(),
                changed_callback(),
                &context,
                paths,
                K_FS_EVENT_STREAM_EVENT_ID_SINCE_NOW,
                LATENCY,
                create_flags,
            );
            CFRelease(paths);
            if stream.is_null() {
                return None;
            }
            FSEventStreamSetDispatchQueue(stream, queue());
            if FSEventStreamStart(stream) == 0 {
                FSEventStreamInvalidate(stream);
                FSEventStreamRelease(stream);
                return None;
            }
            Some(Self { stream, signal })
        }
    }

    /// Whether anything changed since the watch started or since the last call.
    pub(super) fn take_changed(&self) -> bool {
        self.signal.changed.swap(false, Ordering::AcqRel)
    }

    /// The right to ask git for the skip list, when it is not current and no probe runs.
    pub(super) fn skip_probe(&self) -> Option<SkipProbe> {
        if !self.signal.skips {
            return None;
        }
        let asked_at = self.signal.asked_at.lock().ok().and_then(|at| *at);
        let since_asked = asked_at.map(|at| at.elapsed());
        let unanswered = self.signal.unanswered.load(Ordering::Acquire) as usize;
        let due = if unanswered > 0 {
            let wait = RETRY_AFTER[(unanswered - 1).min(RETRY_AFTER.len() - 1)];
            since_asked.is_none_or(|since| since >= wait)
        } else {
            !self.signal.current()
                || since_asked.is_none_or(|since| since >= REASK_AFTER + self.signal.reask_jitter)
        };
        if !due {
            return None;
        }
        // Read before the probe starts: a change after this makes its answer stale again.
        let generation = self.signal.generation.load(Ordering::Acquire);
        self.signal
            .probing
            .compare_exchange(false, true, Ordering::AcqRel, Ordering::Acquire)
            .ok()?;
        Some(SkipProbe {
            signal: self.signal.clone(),
            generation,
        })
    }
}

fn changed_callback() -> FSEventStreamCallback {
    changed
}

impl Drop for FolderWatch {
    fn drop(&mut self) {
        // SAFETY: the stream came from FSEventStreamCreate and is released once. Draining the queue
        // after invalidating it lets a callback already running finish before `signal` is freed.
        unsafe {
            FSEventStreamStop(self.stream);
            FSEventStreamInvalidate(self.stream);
            FSEventStreamRelease(self.stream);
            dispatch_sync_f(queue(), std::ptr::null_mut(), nothing);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn open_descriptors() -> usize {
        std::fs::read_dir("/dev/fd")
            .map(|dir| dir.count())
            .unwrap_or(0)
    }

    /// Watches must not hold file descriptors: gxserver keeps one per project and session.
    #[test]
    fn a_watch_holds_no_file_descriptors() {
        let temp = tempfile::tempdir().unwrap();
        let folders: Vec<PathBuf> = (0..20)
            .map(|n| {
                let folder = temp.path().join(format!("p{n}"));
                std::fs::create_dir(&folder).unwrap();
                folder
            })
            .collect();
        // The count is the whole process's, and tests running at the same time open and close files
        // of their own in bursts. A watch that held a descriptor would hold 20 of them in every
        // attempt; a burst passes, so the test passes when any attempt shows no growth.
        let mut seen = Vec::new();
        for _ in 0..10 {
            let before = open_descriptors();
            let watches: Vec<FolderWatch> = folders
                .iter()
                .map(|folder| FolderWatch::new(std::slice::from_ref(folder)).unwrap())
                .collect();
            let during = open_descriptors();
            drop(watches);
            if during <= before + 4 {
                return;
            }
            seen.push((before, during));
            std::thread::sleep(std::time::Duration::from_millis(100));
        }
        panic!("(open before, open with 20 watches) in every attempt: {seen:?}");
    }
}
