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
//! The allocator underneath is a type parameter so a different one can replace `System` without
//! touching the counting.

use std::alloc::{GlobalAlloc, Layout, System};
use std::sync::atomic::{AtomicU64, AtomicUsize, Ordering};

#[global_allocator]
static GLOBAL: CountingAllocator<System> = CountingAllocator::new(System);

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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn counts_bytes_peak_and_allocations_through_a_realloc() {
        let counter = CountingAllocator::new(System);
        let layout = Layout::from_size_align(100, 8).unwrap();
        unsafe {
            let first = counter.alloc(layout);
            let second = counter.alloc_zeroed(layout);
            let grown = counter.realloc(first, layout, 300);
            let grown_layout = Layout::from_size_align(300, 8).unwrap();
            assert_eq!(
                counter.counts(),
                HeapCounts {
                    in_use_bytes: 400,
                    peak_bytes: 400,
                    live_allocations: 2,
                    total_allocations: 2,
                }
            );
            counter.dealloc(grown, grown_layout);
            counter.dealloc(second, layout);
        }
        assert_eq!(
            counter.counts(),
            HeapCounts {
                in_use_bytes: 0,
                peak_bytes: 400,
                live_allocations: 0,
                total_allocations: 2,
            }
        );
    }
}
