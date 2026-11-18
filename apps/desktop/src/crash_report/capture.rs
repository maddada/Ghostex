//! Saving a crash as it happens, and gathering the crashes to offer at the next open.

use std::sync::atomic::{AtomicBool, Ordering};

use serde_json::Value;

use super::context;
use super::native;
use super::record::{self, CrashKind, CrashRecord, STACK_MAX_FRAMES, SentCrash};
use super::system;

/// Whether this process already saved a main-thread panic.
static RECORDED_MAIN_THREAD_PANIC: AtomicBool = AtomicBool::new(false);

/// CDXC:CrashReports 2026-11-19 WHY:
/// A panic that unwinds into a frame that cannot unwind (a Windows window procedure, an
/// `extern "C"` callback) raises a second panic, "panic in a function that cannot unwind", and the
/// process aborts. Both reach the panic hook, and the second one's record replaced the first (same
/// millisecond, same file name; or newer, so it was the one offered), so the report named
/// `panicking.rs` instead of the real panic (GitHub #675). The first panic's record is kept.
fn is_follow_up_panic(message: &str) -> bool {
    const FOLLOW_UP_MESSAGES: [&str; 2] = [
        "panic in a function that cannot unwind",
        "panic in a destructor during cleanup",
    ];
    FOLLOW_UP_MESSAGES
        .iter()
        .any(|follow_up| message.starts_with(follow_up))
}

/// Called by the panic hook with the details it already cleaned (`panic_crash_details` in
/// support_logs.rs: paths in the message become `[path]`, dependency paths keep only the file
/// name). Only a panic on the main thread is a crash: GPUI runs the app there, and a panic on it
/// ends the process; a panic on another thread ends that thread and the app keeps running.
pub(crate) fn record_panic(details: &Value) {
    if std::thread::current().name() != Some("main") {
        return;
    }
    let text = |key: &str| {
        details
            .get(key)
            .and_then(Value::as_str)
            .unwrap_or_default()
            .to_string()
    };
    let already_recorded = RECORDED_MAIN_THREAD_PANIC.swap(true, Ordering::SeqCst);
    if already_recorded && is_follow_up_panic(&text("message")) {
        return;
    }
    let backtrace: Vec<String> = details
        .get("backtrace")
        .and_then(Value::as_array)
        .map(|lines| {
            lines
                .iter()
                .filter_map(Value::as_str)
                .map(str::to_string)
                .collect()
        })
        .unwrap_or_default();
    let (screen, recent_events) = context::snapshot();
    record::save_pending(&CrashRecord {
        kind: CrashKind::Panic,
        version: system::APP_VERSION.to_string(),
        commit: system::build_commit().map(str::to_string),
        at_ms: record::now_ms(),
        uptime_ms: Some(crate::support_logs::ms_since_launch()),
        location: text("location"),
        message: text("message"),
        stack: compact_rust_backtrace(&backtrace),
        log_tail: record::log_tail(),
        thread: Some("main".to_string()),
        screen,
        recent_events,
        build_id: None,
    });
}

/// Called by the uncaught Objective-C exception handler (macOS) with the cleaned name, reason
/// and call stack, right before the process ends.
#[cfg_attr(not(target_os = "macos"), allow(dead_code))]
pub(crate) fn record_uncaught_exception(name: &str, reason: &str, call_stack: &[String]) {
    let (screen, recent_events) = context::snapshot();
    record::save_pending(&CrashRecord {
        kind: CrashKind::UncaughtException,
        version: system::APP_VERSION.to_string(),
        commit: system::build_commit().map(str::to_string),
        at_ms: record::now_ms(),
        uptime_ms: Some(crate::support_logs::ms_since_launch()),
        location: if name.is_empty() {
            "NSException".to_string()
        } else {
            name.to_string()
        },
        message: reason.to_string(),
        stack: call_stack
            .iter()
            .take(STACK_MAX_FRAMES)
            .map(|frame| frame.split_whitespace().collect::<Vec<_>>().join(" "))
            .collect(),
        log_tail: record::log_tail(),
        thread: Some(
            if std::thread::current().name() == Some("main") {
                "main"
            } else {
                "another thread"
            }
            .to_string(),
        ),
        screen,
        recent_events,
        build_id: None,
    });
}

/// std's backtrace as one line per frame (`name (file:line)`), without the panic machinery
/// frames on top and the runtime frames under `main`.
fn compact_rust_backtrace(lines: &[String]) -> Vec<String> {
    let mut frames: Vec<String> = Vec::new();
    for line in lines {
        let trimmed = line.trim();
        if let Some(source) = trimmed.strip_prefix("at ") {
            if let Some(last) = frames.last_mut() {
                if !last.ends_with(')') {
                    last.push_str(&format!(" ({source})"));
                }
            }
            continue;
        }
        // `12: ghostex_gpui::app::…` → `ghostex_gpui::app::…`
        let name = trimmed
            .split_once(": ")
            .filter(|(index, _)| index.chars().all(|c| c.is_ascii_digit()))
            .map(|(_, name)| name)
            .unwrap_or(trimmed);
        // A frame the symbolizer could not name says nothing; a stack of only these is dropped.
        if name == "<unknown>" {
            continue;
        }
        frames.push(name.to_string());
    }
    const MACHINERY: [&str; 12] = [
        "std::backtrace",
        "install_panic_hook",
        "std::panicking",
        "core::panicking",
        "std::sys::backtrace",
        "rust_begin_unwind",
        "__rustc::rust_begin_unwind",
        "core::option::unwrap_failed",
        "core::option::expect_failed",
        "core::result::unwrap_failed",
        "core::cell::panic_already",
        "<std::panicking",
    ];
    let first_useful = frames
        .iter()
        .rposition(|frame| MACHINERY.iter().any(|marker| frame.contains(marker)))
        .map_or(0, |index| index + 1);
    let end = frames
        .iter()
        .position(|frame| {
            frame.contains("__rust_begin_short_backtrace") || frame.contains("std::rt::lang_start")
        })
        .filter(|end| *end > first_useful)
        .unwrap_or(frames.len());
    frames[first_useful..end]
        .iter()
        .take(STACK_MAX_FRAMES)
        .cloned()
        .collect()
}

/// The crash to offer at this open, if any.
#[derive(Clone, Debug)]
pub(crate) struct PendingCrash {
    pub(crate) record: CrashRecord,
    /// How many pending crashes are this same crash (1 when it happened once).
    pub(crate) times: usize,
}

/// Reads the native crashes written since the last look (saving them as pending), then returns
/// the newest pending crash. A crash this computer already reported is dropped quietly. Blocking:
/// call it off the main thread.
pub(crate) fn collect_pending() -> Option<PendingCrash> {
    pick_up_native_crashes();
    let pending = record::pending();
    let (_, newest) = pending.first()?.clone();
    let fingerprint = newest.fingerprint();
    if record::sent_issue(&fingerprint).is_some() {
        record::clear_pending();
        return None;
    }
    let times = pending
        .iter()
        .filter(|(_, record)| record.fingerprint() == fingerprint)
        .count();
    Some(PendingCrash {
        record: newest,
        times,
    })
}

/// The first time this runs it only notes the time, so crashes from before the crash reporter
/// existed are never offered.
fn pick_up_native_crashes() {
    let mut state = record::reader_state();
    let now = record::now_ms();
    let Some(read_until) = state.native_crashes_read_until_ms else {
        state.native_crashes_read_until_ms = Some(now);
        record::save_reader_state(&state);
        return;
    };
    let files = native::crash_files_after(read_until);
    if files.is_empty() {
        return;
    }
    let mut known: Vec<u64> = record::pending()
        .into_iter()
        .filter(|(_, record)| record.kind != CrashKind::Native)
        .map(|(_, record)| record.at_ms)
        .chain(record::sent().into_iter().map(|sent: SentCrash| sent.at_ms))
        .chain(state.handled_at_ms.iter().copied())
        .collect();
    let mut newest = read_until;
    for (path, written) in files {
        newest = newest.max(written);
        let Some(crash) = native::read_crash_file(&path, written) else {
            continue;
        };
        // The abort after a recorded panic or exception is that same crash.
        let follows_recorded = known.iter().any(|recorded| {
            crash.at_ms >= *recorded && crash.at_ms - recorded <= native::SAME_CRASH_WINDOW_MS
        });
        if !follows_recorded {
            known.push(crash.at_ms);
            record::save_pending(&crash);
        }
    }
    state.native_crashes_read_until_ms = Some(newest);
    record::save_reader_state(&state);
}

#[cfg(test)]
mod tests {
    use super::is_follow_up_panic;

    #[test]
    fn the_abort_after_a_panic_is_a_follow_up() {
        assert!(is_follow_up_panic("panic in a function that cannot unwind"));
        assert!(is_follow_up_panic("panic in a destructor during cleanup"));
        assert!(!is_follow_up_panic(
            "called `Option::unwrap()` on a `None` value"
        ));
        assert!(!is_follow_up_panic(""));
    }
}
