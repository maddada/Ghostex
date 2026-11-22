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
    cef_phase: &'static str,
    cef_version: &'static str,
    cef_trigger: &'static str,
    cef_pump: &'static str,
    /// `(ms since 1970, event name)`, oldest first.
    recent: VecDeque<(u64, String)>,
    /// When the scope tags were last handed to the crash handler.
    #[cfg_attr(not(target_os = "macos"), allow(dead_code))]
    tags_applied_at: Option<Instant>,
}

static STATE: Mutex<State> = Mutex::new(State {
    view: "",
    focus: "",
    cef_phase: "",
    cef_version: "",
    cef_trigger: "",
    cef_pump: "",
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

fn cef_phase_rank(phase: &str) -> u8 {
    match phase {
        "loadLibraryStart" => 1,
        "loadLibraryDone" => 2,
        "executeProcessDone" => 3,
        "initializeStart" => 4,
        "initializeDone" => 5,
        "messageLoopStart" => 6,
        "contextInitialized" => 7,
        _ => 0,
    }
}

#[cfg(target_os = "macos")]
fn apply_tags(state: &mut State) {
    for (key, value) in [
        (VIEW_TAG, state.view),
        (FOCUS_TAG, state.focus),
        (CEF_PHASE_TAG, state.cef_phase),
        (CEF_VERSION_TAG, state.cef_version),
        (CEF_TRIGGER_TAG, state.cef_trigger),
        (CEF_PUMP_TAG, state.cef_pump),
    ] {
        if !value.is_empty() {
            handler::set_tag(key, value);
        }
    }
    state.tags_applied_at = Some(Instant::now());
}

fn push_event(state: &mut State, name: String) -> bool {
    if state.recent.back().is_some_and(|(_, last)| *last == name) {
        return false;
    }
    state
        .recent
        .push_back((super::record::now_ms(), name.clone()));
    while state.recent.len() > RECENT_EVENTS_MAX {
        state.recent.pop_front();
    }
    #[cfg(target_os = "macos")]
    handler::add_breadcrumb(&name);
    true
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
    if !push_event(&mut state, name) {
        return;
    }
    #[cfg(target_os = "macos")]
    {
        // The handler starts on its own thread after the terminal library starts and begins with
        // an empty scope, so tags set before then are lost: hand them over again now and then.
        let stale = state
            .tags_applied_at
            .is_none_or(|applied| applied.elapsed().as_secs() >= 5);
        if stale {
            apply_tags(&mut state);
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
    apply_tags(&mut state);
}

/// Notes a fixed-label CEF startup milestone in the crash breadcrumbs and `ghostex.cef_phase` tag.
/// Chronological breadcrumbs are always recorded; `ghostex.cef_phase` only advances monotonically
/// so a synchronous `on_context_initialized` inside `cef::initialize` is never overwritten by
/// `initializeDone` or `messageLoopStart`.
#[cfg_attr(
    not(any(target_os = "macos", target_os = "windows", target_os = "linux")),
    allow(dead_code)
)]
pub(crate) fn note_cef_phase(phase: &'static str) {
    let Ok(mut state) = STATE.lock() else {
        return;
    };
    if cef_phase_rank(phase) > cef_phase_rank(state.cef_phase) {
        state.cef_phase = phase;
    }
    let breadcrumb = clean_event_name(&format!("gpui.cef.phase.{phase}"));
    if !breadcrumb.is_empty() {
        push_event(&mut state, breadcrumb);
    }
    #[cfg(target_os = "macos")]
    apply_tags(&mut state);
}

/// Notes how CEF was first triggered (`warmUp` or `onDemand`); the first trigger wins.
pub(crate) fn note_cef_trigger(trigger: &'static str) {
    let Ok(mut state) = STATE.lock() else {
        return;
    };
    if !state.cef_trigger.is_empty() {
        return;
    }
    state.cef_trigger = trigger;
    #[cfg(target_os = "macos")]
    handler::set_tag(CEF_TRIGGER_TAG, trigger);
}

/// Notes the exact compiled CEF version string before loading the runtime.
#[cfg_attr(
    not(any(target_os = "macos", target_os = "windows", target_os = "linux")),
    allow(dead_code)
)]
pub(crate) fn note_cef_version(version: &'static str) {
    let Ok(mut state) = STATE.lock() else {
        return;
    };
    state.cef_version = version;
    #[cfg(target_os = "macos")]
    handler::set_tag(CEF_VERSION_TAG, version);
}

/// Notes the CEF message-pump mode (`cefMessageLoop` or `externalPump`).
#[cfg_attr(
    not(any(target_os = "macos", target_os = "windows", target_os = "linux")),
    allow(dead_code)
)]
pub(crate) fn note_cef_pump(pump: &'static str) {
    let Ok(mut state) = STATE.lock() else {
        return;
    };
    state.cef_pump = pump;
    #[cfg(target_os = "macos")]
    handler::set_tag(CEF_PUMP_TAG, pump);
}

/// `Agents view, keyboard in an agent pane`, when known.
pub(crate) fn screen_text(view: &str, focus: &str) -> Option<String> {
    match (view.is_empty(), focus.is_empty()) {
        (true, _) => None,
        (false, true) => Some(format!("{view} view")),
        (false, false) => Some(format!("{view} view, keyboard in {focus}")),
    }
}

/// `phase initializeStart · trigger warmUp · pump cefMessageLoop · version ... · build id ...`,
/// when any CEF startup state or loaded framework UUID is known.
pub(crate) fn cef_text(
    phase: &str,
    trigger: &str,
    pump: &str,
    version: &str,
    build_id: Option<&str>,
) -> Option<String> {
    let mut parts = Vec::new();
    if !phase.is_empty() {
        parts.push(format!("phase {phase}"));
    }
    if !trigger.is_empty() {
        parts.push(format!("trigger {trigger}"));
    }
    if !pump.is_empty() {
        parts.push(format!("pump {pump}"));
    }
    if !version.is_empty() {
        parts.push(format!("version {version}"));
    }
    if let Some(id) = build_id.filter(|id| !id.is_empty()) {
        parts.push(format!("build id {id}"));
    }
    (!parts.is_empty()).then(|| parts.join(" · "))
}

/// What a crash record keeps, read in the crashing process (panic hook, exception handler). Never
/// blocks: a lock held elsewhere gives nothing rather than a hang while the app is dying.
pub(crate) fn snapshot() -> (Option<String>, Option<String>, Vec<String>) {
    let Ok(state) = STATE.try_lock() else {
        return (None, None, Vec::new());
    };
    let recent = state
        .recent
        .iter()
        .map(|(at_ms, name)| format!("{} {name}", clock_text(*at_ms)))
        .collect();
    let cef = cef_text(
        state.cef_phase,
        state.cef_trigger,
        state.cef_pump,
        state.cef_version,
        None,
    );
    (screen_text(state.view, state.focus), cef, recent)
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

    pub(super) fn set_tag(key: &str, value: &str) {
        let (Ok(key), Ok(value)) = (CString::new(key), CString::new(value)) else {
            return;
        };
        unsafe { sentry_set_tag(key.as_ptr(), value.as_ptr()) };
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
pub(crate) const CEF_PHASE_TAG: &str = "ghostex.cef_phase";
pub(crate) const CEF_VERSION_TAG: &str = "ghostex.cef_version";
pub(crate) const CEF_TRIGGER_TAG: &str = "ghostex.cef_trigger";
pub(crate) const CEF_PUMP_TAG: &str = "ghostex.cef_pump";
