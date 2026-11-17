//! What Ghostex was doing when it crashed: the view on screen, where the keyboard was, and the
//! names of the last app events. Kept in memory only (nothing is written to disk while the app
//! runs).
//!
//! A panic or an uncaught exception reads it from here as it saves its record. A native crash
//! (a bad memory access in AppKit, CEF or Ghostty) runs no Ghostex code, so on macOS the same
//! facts also go into the scope of the crash handler the terminal library runs (Sentry native,
//! see native.rs): it saves its tags and breadcrumbs with the crash file, and native.rs reads them
//! back at the next open.
//!
//! CDXC:CrashReports 2026-11-18 WHY:
//! GitHub #653 (a native crash in 11.9.0) arrived with only addresses: no log lines, nothing about
//! the screen or what had just happened. The user: "if details arent enough pls add more details
//! to be sent so we can track this better and fix next time". Only fixed labels and event names
//! are kept here, never values, paths, titles or text, so nothing personal can reach a report
//! this way.

use std::collections::VecDeque;
use std::sync::Mutex;
use std::time::Instant;

/// Event names kept for a report.
pub(crate) const RECENT_EVENTS_MAX: usize = 20;
/// Longest event name kept.
const EVENT_MAX_CHARS: usize = 80;

struct State {
    view: &'static str,
    focus: &'static str,
    /// `(ms since 1970, event name)`, oldest first.
    recent: VecDeque<(u64, String)>,
    /// When the screen tags were last handed to the crash handler.
    #[cfg_attr(not(target_os = "macos"), allow(dead_code))]
    tags_applied_at: Option<Instant>,
}

static STATE: Mutex<State> = Mutex::new(State {
    view: "",
    focus: "",
    recent: VecDeque::new(),
    tags_applied_at: None,
});

/// An event name as kept: letters, digits and `._:-` only, so no value can ride along.
fn clean_event_name(event: &str) -> String {
    event
        .chars()
        .filter(|c| c.is_ascii_alphanumeric() || matches!(c, '.' | '_' | ':' | '-'))
        .take(EVENT_MAX_CHARS)
        .collect()
}

/// Notes that `event` happened (a support log event name such as `gpui.cefRuntime.ready`). The
/// same event twice in a row is kept once.
pub(crate) fn note_event(event: &str) {
    let name = clean_event_name(event);
    if name.is_empty() {
        return;
    }
    let Ok(mut state) = STATE.lock() else {
        return;
    };
    if state.recent.back().is_some_and(|(_, last)| *last == name) {
        return;
    }
    state
        .recent
        .push_back((super::record::now_ms(), name.clone()));
    while state.recent.len() > RECENT_EVENTS_MAX {
        state.recent.pop_front();
    }
    #[cfg(target_os = "macos")]
    {
        handler::add_breadcrumb(&name);
        // The handler starts on its own thread after the terminal library starts and begins with
        // an empty scope, so tags set before then are lost: hand them over again now and then.
        let stale = state
            .tags_applied_at
            .is_none_or(|applied| applied.elapsed().as_secs() >= 5);
        if stale && !state.view.is_empty() {
            handler::set_screen_tags(state.view, state.focus);
            state.tags_applied_at = Some(Instant::now());
        }
    }
}

/// Notes what is on screen: the view (`Agents`, `Browser`, ...) and where the keyboard is
/// (`agent pane`, `commands pane`, ...). Cheap when nothing changed, so it can run every frame.
pub(crate) fn note_screen(view: &'static str, focus: &'static str) {
    let Ok(mut state) = STATE.lock() else {
        return;
    };
    if state.view == view && state.focus == focus {
        return;
    }
    state.view = view;
    state.focus = focus;
    #[cfg(target_os = "macos")]
    {
        handler::set_screen_tags(view, focus);
        state.tags_applied_at = Some(Instant::now());
    }
}

/// `Agents view, keyboard in an agent pane`, when known.
pub(crate) fn screen_text(view: &str, focus: &str) -> Option<String> {
    match (view.is_empty(), focus.is_empty()) {
        (true, _) => None,
        (false, true) => Some(format!("{view} view")),
        (false, false) => Some(format!("{view} view, keyboard in {focus}")),
    }
}

/// What a crash record keeps, read in the crashing process (panic hook, exception handler). Never
/// blocks: a lock held elsewhere gives nothing rather than a hang while the app is dying.
pub(crate) fn snapshot() -> (Option<String>, Vec<String>) {
    let Ok(state) = STATE.try_lock() else {
        return (None, Vec::new());
    };
    let recent = state
        .recent
        .iter()
        .map(|(at_ms, name)| format!("{} {name}", clock_text(*at_ms)))
        .collect();
    (screen_text(state.view, state.focus), recent)
}

/// `13:33:02` (UTC).
pub(crate) fn clock_text(at_ms: u64) -> String {
    chrono::DateTime::from_timestamp_millis(at_ms as i64)
        .map(|time| time.format("%H:%M:%S").to_string())
        .unwrap_or_default()
}

/// The terminal library's crash handler (Sentry native, linked from the GhosttyKit archive on
/// macOS only; see apps/desktop/build.rs). Its scope is locked inside Sentry, so these may be
/// called from any thread, and before it starts they are simply dropped.
#[cfg(target_os = "macos")]
mod handler {
    use std::ffi::{CString, c_char};

    /// `sentry_value_t` (`union { uint64_t _bits; double _double; }`).
    #[repr(C)]
    #[derive(Clone, Copy)]
    union SentryValue {
        bits: u64,
        double: f64,
    }

    unsafe extern "C" {
        fn sentry_set_tag(key: *const c_char, value: *const c_char);
        fn sentry_value_new_breadcrumb(kind: *const c_char, message: *const c_char) -> SentryValue;
        fn sentry_add_breadcrumb(breadcrumb: SentryValue);
    }

    pub(super) fn set_screen_tags(view: &str, focus: &str) {
        for (key, value) in [(super::VIEW_TAG, view), (super::FOCUS_TAG, focus)] {
            let (Ok(key), Ok(value)) = (CString::new(key), CString::new(value)) else {
                continue;
            };
            unsafe { sentry_set_tag(key.as_ptr(), value.as_ptr()) };
        }
    }

    pub(super) fn add_breadcrumb(name: &str) {
        let Ok(message) = CString::new(name) else {
            return;
        };
        unsafe {
            let breadcrumb = sentry_value_new_breadcrumb(std::ptr::null(), message.as_ptr());
            sentry_add_breadcrumb(breadcrumb);
        }
    }
}

/// The tag names native.rs reads back from a crash file.
pub(crate) const VIEW_TAG: &str = "ghostex.view";
pub(crate) const FOCUS_TAG: &str = "ghostex.focus";
