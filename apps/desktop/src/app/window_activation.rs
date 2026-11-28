//! Where Ghostex brings itself forward, or makes one of its windows key on its own, so one rule
//! decides it and each time leaves a line in `gpui-host-lifecycle.log`.
//!
//! The rule: only the user brings Ghostex forward. Something the user did (a click on its
//! notification, a session or ghostex:// link they opened, a file opened with Ghostex, Floating
//! Capture's own picks) activates the app (`bring_ghostex_forward_for_user`, the only caller of
//! `cx.activate`). Anything Ghostex does on its own, mostly because an agent did something (an
//! answer, a turn ending, a notice or a dialog arriving, an ask leaving the dock), makes a window
//! key only while Ghostex already is the app the user is in (`activate_window_if_frontmost`); while
//! another app is frontmost it is held, and a dialog it would have opened waits for the user to
//! come back (`release_popups_held_for_return` in os_integration/first_run_onboarding.rs).
//!
//! CDXC:FocusRouting 2026-11-28 DECISION:
//! Issue #792: "While I'm working in another app ... the main Ghostex window comes to the front and
//! takes keyboard focus as soon as an agent writes an answer." A sound, a system notification, the
//! Dock badge or a bell row are how Ghostex says an agent needs you; it never takes the front for
//! it. The reporter could not tell which path raised the window because nothing logged it, so
//! each activation (and each one held back) is logged with its reason: `gpui.activation.app`,
//! `gpui.activation.window` and `gpui.activation.held`. No setting: nothing an agent does needs
//! the front.

use gpui::{App, Window};
use std::sync::Mutex;
use std::time::{Duration, Instant};

use crate::support_logs::{self, GpuiSupportLog};

/// How long after the user asked Ghostex forward its windows may still be made key although the
/// operating system has not reported Ghostex active yet (`activateIgnoringOtherApps` and
/// `SetForegroundWindow` settle a moment later than the call).
const USER_ACTIVATION_GRACE: Duration = Duration::from_millis(1500);

static LAST_USER_ACTIVATION: Mutex<Option<Instant>> = Mutex::new(None);

/// Whether Ghostex is the app the user is in: a dialog of Ghostex's own (a file picker, an alert)
/// counts as Ghostex.
pub(crate) fn ghostex_is_frontmost(cx: &App) -> bool {
    #[cfg(target_os = "macos")]
    {
        let _ = cx;
        unsafe extern "C" {
            fn GhostexGpuiApplicationIsActive() -> bool;
        }
        unsafe { GhostexGpuiApplicationIsActive() }
    }
    #[cfg(target_os = "windows")]
    {
        use windows_sys::Win32::System::Threading::GetCurrentProcessId;
        use windows_sys::Win32::UI::WindowsAndMessaging::{
            GetForegroundWindow, GetWindowThreadProcessId,
        };
        let foreground = unsafe { GetForegroundWindow() };
        if foreground.is_null() {
            return cx.active_window().is_some();
        }
        let mut process_id = 0u32;
        unsafe { GetWindowThreadProcessId(foreground, &mut process_id) };
        process_id == unsafe { GetCurrentProcessId() }
    }
    #[cfg(not(any(target_os = "macos", target_os = "windows")))]
    {
        cx.active_window().is_some()
    }
}

/// Whether the user brought Ghostex forward a moment ago.
fn user_asked_recently() -> bool {
    LAST_USER_ACTIVATION
        .lock()
        .ok()
        .and_then(|last| *last)
        .is_some_and(|at| at.elapsed() < USER_ACTIVATION_GRACE)
}

/// Whether something Ghostex does on its own may make one of its windows key now.
pub(crate) fn may_take_keyboard(cx: &App) -> bool {
    ghostex_is_frontmost(cx) || user_asked_recently()
}

/// Brings Ghostex in front of the app the user is in, because the user asked for it: `reason`
/// names what they did ("notification click", "session link", ...). Never call this for
/// something an agent or Ghostex itself did.
pub(crate) fn bring_ghostex_forward_for_user(cx: &mut App, reason: &'static str) {
    let was_frontmost = ghostex_is_frontmost(cx);
    if let Ok(mut last) = LAST_USER_ACTIVATION.lock() {
        *last = Some(Instant::now());
    }
    support_logs::append_always(
        GpuiSupportLog::HostLifecycle,
        "gpui.activation.app",
        serde_json::json!({ "reason": reason, "wasFrontmost": was_frontmost }),
    );
    cx.activate(true);
}

/// Makes `window` key for something Ghostex does on its own (`reason`), only while Ghostex is the
/// app the user is in. Answers whether it did; while another app is frontmost nothing moves and
/// a `gpui.activation.held` line says what was held.
pub(crate) fn activate_window_if_frontmost(
    window: &mut Window,
    cx: &App,
    reason: &'static str,
) -> bool {
    if !may_take_keyboard(cx) {
        log_held(reason);
        return false;
    }
    support_logs::append(
        GpuiSupportLog::HostLifecycle,
        "gpui.activation.window",
        serde_json::json!({ "reason": reason, "user": false, "wasFrontmost": true }),
    );
    window.activate_window();
    true
}

/// Logs that `reason` would have brought Ghostex forward and was held because another app is
/// frontmost.
pub(crate) fn log_held(reason: &str) {
    support_logs::append_always(
        GpuiSupportLog::HostLifecycle,
        "gpui.activation.held",
        serde_json::json!({ "reason": reason }),
    );
}
