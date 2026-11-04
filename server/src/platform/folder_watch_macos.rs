//! `folder_watch`'s change signal on macOS: one FSEvents stream per watch.
//!
//! CDXC:Git 2026-11-05 WHY:
//! macOS read every Git answer again on each poll as well: the sidebar's diff stats, the active project's full read every 15 seconds and every session's branch and line counts every minute, about ten git processes a minute per project and 0.4 s of CPU for one `git status` in a repository with submodules (measured 2026-11-04, issues-1027/macos-perf). FSEvents watches a whole tree with one stream and no per-folder cost, so the Windows change-gated answers (`folder_watch`) apply here too. A stream only raises a flag; the reader reads again when it finds the flag raised. Events arrive after `LATENCY`, so a change made in the moment before a poll is read on the next one.

use std::{
    ffi::c_void,
    os::unix::ffi::OsStrExt,
    path::PathBuf,
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc, OnceLock,
    },
};

/// How long FSEvents gathers events before it calls back, in seconds.
const LATENCY: f64 = 0.1;

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
const K_FS_EVENT_STREAM_CREATE_FLAG_WATCH_ROOT: u32 = 0x04;

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

extern "C" fn changed(
    _stream: FSEventStreamRef,
    info: *mut c_void,
    count: usize,
    _paths: *mut c_void,
    _flags: *const u32,
    _ids: *const u64,
) {
    if count > 0 {
        // SAFETY: `info` is the `AtomicBool` the watch keeps alive until its stream is invalidated
        // and the queue has drained (`Drop`).
        unsafe { &*(info as *const AtomicBool) }.store(true, Ordering::Release);
    }
}

extern "C" fn nothing(_: *mut c_void) {}

/// An FSEvents stream over the folders one answer depends on.
pub(super) struct FolderWatch {
    stream: FSEventStreamRef,
    changed: Arc<AtomicBool>,
}

// SAFETY: the stream is only started, stopped and released through this value, under the cache's lock.
unsafe impl Send for FolderWatch {}

impl FolderWatch {
    /// Watches every folder with everything under it, or `None` when there are none or the stream cannot start.
    pub(super) fn new(folders: &[PathBuf]) -> Option<Self> {
        if folders.is_empty() {
            return None;
        }
        let changed = Arc::new(AtomicBool::new(false));
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
                info: Arc::as_ptr(&changed) as *mut c_void,
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
                K_FS_EVENT_STREAM_CREATE_FLAG_NO_DEFER | K_FS_EVENT_STREAM_CREATE_FLAG_WATCH_ROOT,
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
            Some(Self { stream, changed })
        }
    }

    /// Whether anything changed since the watch started or since the last call.
    pub(super) fn take_changed(&self) -> bool {
        self.changed.swap(false, Ordering::AcqRel)
    }
}

fn changed_callback() -> FSEventStreamCallback {
    changed
}

impl Drop for FolderWatch {
    fn drop(&mut self) {
        // SAFETY: the stream came from FSEventStreamCreate and is released once. Draining the queue
        // after invalidating it lets a callback already running finish before `changed` is freed.
        unsafe {
            FSEventStreamStop(self.stream);
            FSEventStreamInvalidate(self.stream);
            FSEventStreamRelease(self.stream);
            dispatch_sync_f(queue(), std::ptr::null_mut(), nothing);
        }
    }
}
