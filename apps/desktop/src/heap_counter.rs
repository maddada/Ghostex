//! The app's own heap counters: a `#[global_allocator]` that counts every Rust allocation before
//! handing it to the allocator underneath.
//!
//! CDXC:Memory 2026-11-10 WHY:
//! The user's running app held 1.2 to 1.9 GB of heap while its 24 chats, loaded offline, hold
//! about 105 MB, and nothing outside the process can say where the rest is: on macOS CEF installs
//! its allocator as the process `malloc`, so the app's Rust allocations and the browser's land in
//! the same zones (`perf-1110/audits/memory-audit.md`). Counting at the Rust allocator is the one
//! place that sees only the app's own memory. `gx_chat/diagnostics.rs` writes these numbers into
//! the once-a-minute `gxChat.host.summary` record.
//!
//! The cost per allocation is four relaxed atomic operations (bytes, live, total, and a load that
//! only becomes a `fetch_max` when a new peak is reached) and nothing else: no lock, no thread
//! local, no allocation of its own, since anything that allocates here would recurse. Relaxed
//! ordering is enough because each counter is read alone; the four numbers of one snapshot can be
//! a few allocations apart from each other, which a once-a-minute summary does not care about.
//!
//! The allocator underneath is a type parameter, so choosing it does not touch the counting.

use std::alloc::{GlobalAlloc, Layout};
use std::sync::atomic::{AtomicU64, AtomicUsize, Ordering};

/// The allocator under the counters: mimalloc on macOS, the system allocator elsewhere.
///
/// CDXC:Memory 2026-11-10 WHY:
/// On macOS CEF installs PartitionAlloc as the process `malloc`, so `System` put every Rust
/// allocation of the app in the browser's allocator, where the running app held 1.2 to 1.9 GB
/// against about 0.15 GB of chat data. The same chat workload replayed under mimalloc held 1.2 to
/// 1.4 times its live data and ran 5% faster (`perf-1110/audits/memory-audit.md`).
///
/// It is safe only because no memory crosses allocators. mimalloc is built without its `override`
/// feature (`Cargo.toml`), so the process `malloc` and `free` stay CEF's and C, Objective-C, Zig
/// and CEF code is unchanged; only Rust's own allocations move. That holds as long as nothing hands
/// a Rust allocation to C to `free`, or takes a C allocation back into a `Box`, `Vec`, `String` or
/// `CString`. As of this change every crossing pairs its own allocator and free: the shims in
/// `native/macos/` free only what they `strdup`/`malloc`/`calloc`ed; Ghostty frees what it
/// allocated (the PNG decoder copies into `ghostty_alloc`); every `Box::into_raw` handed to C as
/// userdata (Ghostty VT callbacks, the notification sender, the mic capture context) comes back
/// through `Box::from_raw`; cef-rs keeps CEF strings in libcef's memory and frees its ref-counted
/// objects in Rust; objc2 and core-foundation hand Rust buffers over with Rust deallocators. A new
/// crossing must keep that pairing.
///
/// Windows and Linux stay on `System`: CEF's allocator is not shown to be the process `malloc`
/// there, and nothing was measured. Only this binary changes; the CEF helper and the other binaries
/// of the crate keep their default allocator.
#[cfg(target_os = "macos")]
type Underlying = mimalloc::MiMalloc;
#[cfg(target_os = "macos")]
const UNDERLYING: Underlying = mimalloc::MiMalloc;
#[cfg(not(target_os = "macos"))]
type Underlying = std::alloc::System;
#[cfg(not(target_os = "macos"))]
const UNDERLYING: Underlying = std::alloc::System;

#[global_allocator]
static GLOBAL: CountingAllocator<Underlying> = CountingAllocator::new(UNDERLYING);

/// Counts what passes through to `inner`.
pub(crate) struct CountingAllocator<A> {
    inner: A,
    in_use_bytes: AtomicUsize,
    peak_bytes: AtomicUsize,
    live_allocations: AtomicUsize,
    total_allocations: AtomicU64,
}

/// The counters at one moment.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) struct HeapCounts {
    /// Bytes requested by live allocations (the layouts' sizes, not the allocator's rounding).
    pub(crate) in_use_bytes: usize,
    /// The highest `in_use_bytes` has been since the app started.
    pub(crate) peak_bytes: usize,
    /// Allocations not yet freed.
    pub(crate) live_allocations: usize,
    /// Allocations made since the app started. A `realloc` resizes one rather than adding one.
    pub(crate) total_allocations: u64,
}

impl<A> CountingAllocator<A> {
    pub(crate) const fn new(inner: A) -> Self {
        Self {
            inner,
            in_use_bytes: AtomicUsize::new(0),
            peak_bytes: AtomicUsize::new(0),
            live_allocations: AtomicUsize::new(0),
            total_allocations: AtomicU64::new(0),
        }
    }

    fn grew(&self, bytes: usize) {
        let now = self
            .in_use_bytes
            .fetch_add(bytes, Ordering::Relaxed)
            .wrapping_add(bytes);
        if now > self.peak_bytes.load(Ordering::Relaxed) {
            self.peak_bytes.fetch_max(now, Ordering::Relaxed);
        }
    }

    fn shrank(&self, bytes: usize) {
        self.in_use_bytes.fetch_sub(bytes, Ordering::Relaxed);
    }

    fn allocated(&self, bytes: usize) {
        self.grew(bytes);
        self.live_allocations.fetch_add(1, Ordering::Relaxed);
        self.total_allocations.fetch_add(1, Ordering::Relaxed);
    }

    fn counts(&self) -> HeapCounts {
        HeapCounts {
            in_use_bytes: self.in_use_bytes.load(Ordering::Relaxed),
            peak_bytes: self.peak_bytes.load(Ordering::Relaxed),
            live_allocations: self.live_allocations.load(Ordering::Relaxed),
            total_allocations: self.total_allocations.load(Ordering::Relaxed),
        }
    }
}

// SAFETY: every call is forwarded to `inner` unchanged; the counters never touch the memory.
unsafe impl<A: GlobalAlloc> GlobalAlloc for CountingAllocator<A> {
    unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
        let ptr = unsafe { self.inner.alloc(layout) };
        if !ptr.is_null() {
            self.allocated(layout.size());
        }
        ptr
    }

    unsafe fn alloc_zeroed(&self, layout: Layout) -> *mut u8 {
        let ptr = unsafe { self.inner.alloc_zeroed(layout) };
        if !ptr.is_null() {
            self.allocated(layout.size());
        }
        ptr
    }

    unsafe fn dealloc(&self, ptr: *mut u8, layout: Layout) {
        unsafe { self.inner.dealloc(ptr, layout) };
        self.shrank(layout.size());
        self.live_allocations.fetch_sub(1, Ordering::Relaxed);
    }

    unsafe fn realloc(&self, ptr: *mut u8, layout: Layout, new_size: usize) -> *mut u8 {
        let moved = unsafe { self.inner.realloc(ptr, layout, new_size) };
        // A null answer leaves the old block allocated and unchanged, so nothing moved.
        if !moved.is_null() {
            let old_size = layout.size();
            if new_size >= old_size {
                self.grew(new_size - old_size);
            } else {
                self.shrank(old_size - new_size);
            }
        }
        moved
    }
}

/// The app's heap counters now.
pub(crate) fn counts() -> HeapCounts {
    GLOBAL.counts()
}
