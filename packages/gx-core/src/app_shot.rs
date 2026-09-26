//! App Shots: the capture as the prompt reads it, the prompt itself, and the session a new App
//! Shot goes to.
//!
//! CDXC:AppShots 2026-09-25 WHY:
//! The runtime did this (`handleNativeAppShotCaptured` and its helpers in
//! apps/desktop/sidebar/gxserver-runtime/helpers/app-shot.ts) and Rust only captured the image
//! and typed the finished prompt; the two talked over three bridge callbacks and a 2 s answer
//! timeout. The app runtime port (family F6) moves the rules here unchanged: the capture's
//! bounds, the prompt format of CDXC:AppShots 2026-06-29-02:59, and the target order of
//! CDXC:AppShots 2026-06-25-23:28 on [`app_shot_target`].
//!
//! SEE-ALSO: apps/desktop/src/app/gx_store/app_shot.rs (the host that stages it).

use crate::keys::SessionKey;
use crate::sidebar_view::agents::resolve_agent_icon;
use crate::sidebar_view::rows::{provider_session_state, session_kind, sidebar_lifecycle_state};
use crate::sidebar_view::text::js_trim;
use crate::Core;

/// How long the last App Shot target keeps precedence over the focused row.
pub const APP_SHOT_RECENT_TARGET_MS: u64 = 60_000;
/// How long a target's terminal gets to be ready for the prompt.
pub const APP_SHOT_PROMPT_INSERT_TIMEOUT_MS: u64 = 2_000;

/// A capture, bounded as the prompt may quote it.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct AppShotCapture {
    pub app_name: String,
    pub image_path: String,
    pub bundle_identifier: Option<String>,
    pub window_title: Option<String>,
    pub window_width: Option<i64>,
    pub window_height: Option<i64>,
    pub trigger: Option<String>,
}

/// What the native capture reported, before the bounds.
#[derive(Clone, Debug, Default)]
pub struct AppShotCaptureInput<'a> {
    pub app_name: &'a str,
    pub image_path: &'a str,
    pub bundle_identifier: Option<&'a str>,
    pub window_title: Option<&'a str>,
    pub window_width: Option<i64>,
    pub window_height: Option<i64>,
    pub trigger: Option<&'a str>,
}

/// `normalizeGpuiNativeAppShotString`: trimmed, at most `max` UTF-16 units, no control
/// characters.
fn bounded(value: Option<&str>, max: usize) -> Option<String> {
    let text = js_trim(value?);
    if text.is_empty()
        || text.encode_utf16().count() > max
        || text
            .chars()
            .any(|character| character <= '\u{1f}' || character == '\u{7f}')
    {
        return None;
    }
    Some(text.to_string())
}

/// `normalizeGpuiNativeAppShotDimension`.
fn dimension(value: Option<i64>) -> Option<i64> {
    value.filter(|value| *value > 0 && *value <= 100_000)
}

/// `normalizeGpuiNativeAppShotCapture`: `None` without an app name or an absolute image path.
pub fn normalize_app_shot_capture(input: &AppShotCaptureInput<'_>) -> Option<AppShotCapture> {
    let app_name = bounded(Some(input.app_name), 256)?;
    let image_path = bounded(Some(input.image_path), 4096)
        .filter(|path| path.starts_with("~/") || path.starts_with('/'))?;
    let trigger = bounded(input.trigger, 80).filter(|trigger| {
        matches!(
            trigger.as_str(),
            "both-command"
                | "both-shift"
                | "both-option"
                | "double-left-shift"
                | "double-left-option"
        )
    });
    Some(AppShotCapture {
        app_name,
        image_path,
        bundle_identifier: bounded(input.bundle_identifier, 256),
        window_title: bounded(input.window_title, 512),
        window_width: dimension(input.window_width),
        window_height: dimension(input.window_height),
        trigger,
    })
}

/// `formatGpuiNativeAppShotPrompt`.
///
/// CDXC:AppShots 2026-06-29-02:59:
/// App Shot prompt text should paste only the image link by default, with no intro sentence, no closing instruction, no blank spacer lines, and one newline of padding before and after. Add WindowServer metadata only when the Settings App Shots metadata toggle is enabled.
pub fn format_app_shot_prompt(capture: &AppShotCapture, include_metadata: bool) -> String {
    let mut lines = vec![format!("[Image #1]({})", capture.image_path)];
    if include_metadata {
        lines.push("Metadata:".to_string());
        lines.push(format!("App: {}", capture.app_name));
        if let Some(bundle) = &capture.bundle_identifier {
            lines.push(format!("Bundle ID: {bundle}"));
        }
        if let Some(title) = &capture.window_title {
            lines.push(format!("Window title: {title}"));
        }
        if let (Some(width), Some(height)) = (capture.window_width, capture.window_height) {
            lines.push(format!("Window size: {width} x {height} px"));
        }
    }
    format!("\n{}\n", lines.join("\n"))
}

/// The last session an App Shot went to, and when.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct AppShotRecentTarget {
    pub session: SessionKey,
    pub at_ms: u64,
}

/// `isNativeAppShotAgentSession`: a live, awake agent terminal row. `mounted` is whether the
/// app has the session's terminal on screen, which counts as live like a running provider does.
pub fn is_app_shot_agent_session(core: &Core, session: &SessionKey, mounted: bool) -> bool {
    let Some(row) = core
        .presentation()
        .machine(&session.machine)
        .and_then(|machine| machine.effective_session(&session.project_id, &session.session_id))
    else {
        return false;
    };
    session_kind(&row.kind) == "terminal"
        && sidebar_lifecycle_state(&row.lifecycle_state) != "sleeping"
        && (mounted || provider_session_state(&row) == "exists")
        && resolve_agent_icon(
            row.agent_icon
                .as_deref()
                .or(row.agent_name.as_deref())
                .or(row.agent_id.as_deref()),
        )
        .is_some()
}

/// `resolveNativeAppShotTargetSession`: the recent target while it still qualifies, else the
/// focused session, else the first visible session of the active project that does.
///
/// CDXC:AppShots 2026-06-25-23:28:
/// GPUI App Shots mirror macOS target order for local sessions: reuse the last successful local App Shot target for 60 seconds when it is still a live local agent row, otherwise use the focused/visible local agent row, and create a default prompt-agent session only when the exact local insert declines. Keep command-pane, sleeping, stale, non-agent, and sidebar-only rows out of insertion.
pub fn app_shot_target(
    core: &Core,
    recent: Option<&AppShotRecentTarget>,
    now_ms: u64,
    mounted: &dyn Fn(&SessionKey) -> bool,
) -> Option<SessionKey> {
    let qualifies =
        |session: &SessionKey| is_app_shot_agent_session(core, session, mounted(session));
    if let Some(recent) = recent {
        if now_ms.saturating_sub(recent.at_ms) <= APP_SHOT_RECENT_TARGET_MS
            && qualifies(&recent.session)
        {
            return Some(recent.session.clone());
        }
    }
    let focus = core.focus();
    if let Some(focused) = &focus.focused_session {
        if qualifies(focused) {
            return Some(focused.clone());
        }
    }
    // A visible row is one of the active project's: `isVisible` is set only inside the active
    // group.
    focus
        .visible_sessions
        .iter()
        .filter(|session| focus.active_project.as_ref() == Some(&session.project_key()))
        .find(|session| qualifies(session))
        .cloned()
}
