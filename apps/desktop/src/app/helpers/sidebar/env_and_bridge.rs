// C1 wave-1 deferred split: apps/desktop/src/app/helpers/sidebar.rs (~4.1k
// lines) further divided into responsibility-scoped submodules (pure move,
// no logic changes). This file holds sidebar visual constants, bridge-script builders, and pointer/menu-bar callback registries.
// See docs/2026-08-22/repo-restructure/SPLITS.md C1.

// C1 wave-1 extraction: stateless helper functions moved verbatim out of
// main.rs (pure move, no logic changes). See docs/2026-08-22/repo-restructure/SPLITS.md C1.

use std::{env, path::PathBuf, sync::atomic::Ordering, time::Duration};

// RefCell backs cross-platform runtime state (window frame persistence), not
// just the macOS-only shims that first introduced the import.

use anyhow::{Context as _, Result};
use gpui::{Hsla, rgb};

use crate::app::helpers::*;
use crate::*;

pub(crate) fn sidebar_cef_prepaint_background_color() -> u32 {
    /*
    CEF owns the sidebar's native child view right up to the sibling divider.
    Use the resolved sidebar base for Chromium's prepaint/background pixel so
    its AppKit edge cannot expose the old fixed #0e0e0e color.
    */
    0xff00_0000 | GPUI_TITLEBAR_BACKGROUND_RGB.load(Ordering::Relaxed) as u32
}

/// CDXC:Sidebar 2026-09-16 DECISION:
/// User: resize drag-area backgrounds are #F3F4F6 in light mode, superseding the earlier #C9C9C9 choice and the white sidebar grab handle requested on 2026-09-13.
pub(crate) fn sidebar_divider_background_color() -> Hsla {
    if CHROME_LIGHT_APPEARANCE.load(Ordering::Relaxed) {
        rgb(LIGHT_RESIZE_HANDLE_RGB).into()
    } else {
        workspace_background_color()
    }
}

/// CDXC:Sidebar 2026-09-19 DECISION:
/// User: the line between the sidebar and the workspace matches the companion sidepane's left border in both themes (#d4d4d4 in light mode, drawn in dark mode too). Since the same-day Workarea decision to make rails look like Waku's, the 2px resize rail is that line: it fills with this colour instead of drawing one pixel beside a black gap, and stays the single owner of the edge, so the sidebar itself must not add a border there as well.
pub(crate) fn sidebar_divider_line_color() -> Hsla {
    chrome_color(0x252525, 0xd4d4d4).into()
}

/// CDXC:Theming 2026-09-14 DECISION:
/// User: the resize drag handle's hovered/active color is light blue in light mode.
pub(crate) fn sidebar_divider_hover_line_color() -> Hsla {
    chrome_color(0xffffff, 0x93c5fd).into()
}

pub(crate) fn sidebar_url() -> Result<String> {
    if let Ok(value) = env::var("GHOSTEX_GPUI_SIDEBAR_URL") {
        if !value.trim().is_empty() {
            return Ok(value);
        }
    }

    let executable = env::current_exe().context("failed to resolve current executable")?;
    if let Some(bundle_root) = find_app_bundle_root(&executable) {
        let bundled = bundle_root.join("Contents/Resources/sidebar/index.html");
        if bundled.exists() {
            return Ok(file_url(&bundled));
        }
    }

    /*
    CDXC:PlatformSupport 2026-07-04:
    Packaged Windows and Linux layouts have no .app bundle: the packaging
    scripts (build-windows-app.ps1 / build-linux-app.sh) stage the sidebar at
    dist/sidebar beside the executable. That directory name is load-bearing —
    the CEF helper's first-party entry-URL check accepts only
    /Contents/Resources/sidebar/ or /dist/sidebar/ file URLs.
    */
    #[cfg(any(target_os = "windows", target_os = "linux"))]
    if let Some(exe_dir) = executable.parent() {
        let packaged = exe_dir.join("dist/sidebar/index.html");
        if packaged.exists() {
            return Ok(file_url(&packaged));
        }
    }

    let manifest_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let local = manifest_dir.join("dist/sidebar/index.html");
    if local.exists() {
        return Ok(file_url(&local));
    }

    anyhow::bail!(
        "sidebar bundle was not found; run gpui/scripts/build-macos-app.sh or bunx vite build --config gpui/vite.config.ts"
    );
}

pub(crate) fn gpui_app_modal_sidebar_session_id_allowed(value: &str) -> bool {
    gpui_combined_presentation_session_key(value).is_some()
        || gpui_sidebar_gxserver_presentation_session_id_allowed(value)
}

pub(crate) fn gpui_percent_decoded_id_part(value: &str) -> Option<String> {
    let decoded = browser_favicon_percent_decode(value, GPUI_PROJECT_CONTRACT_STRING_MAX_CHARS)
        .and_then(|bytes| String::from_utf8(bytes).ok())?;
    (!decoded.is_empty()
        && decoded.chars().count() <= GPUI_PROJECT_CONTRACT_STRING_MAX_CHARS
        && !decoded.chars().any(char::is_control))
    .then_some(decoded)
}

pub(crate) fn gpui_sidebar_reveal_browser_tab_script(payload: &serde_json::Value) -> String {
    format!(
        "(function(){{const bridge=window.ghostexGpui;if(bridge&&typeof bridge.onRevealBrowserTab==='function'){{bridge.onRevealBrowserTab({payload});}}}})(); undefined;"
    )
}

pub(crate) fn gpui_sidebar_host_message_script(message: &serde_json::Value) -> String {
    format!(
        "(function(){{const bridge=window.ghostexGpui;if(bridge&&typeof bridge.onSidebarHostMessage==='function'){{bridge.onSidebarHostMessage({message});}}}})(); undefined;"
    )
}

#[cfg(target_os = "macos")]
pub(crate) fn gpui_sidebar_native_pointer_inside_script(inside: bool) -> String {
    format!(
        "(function(){{if(document.body){{document.body.dataset.nativePointerInside={};}}}})(); undefined;",
        if inside { "'true'" } else { "'false'" }
    )
}

/*
CDXC:Sidebar 2026-08-02:
Dismissal needs page code — the open menus live in a module-scoped registry
inside the sidebar bundle — so it goes through the sidebar's own bridge. If the
bridge is not installed the page cannot have an open menu either, so there is
nothing to queue.
*/
#[cfg(target_os = "macos")]
pub(crate) const GPUI_SIDEBAR_DISMISS_CONTEXT_MENUS_SCRIPT: &str = "(function(){const bridge=window.ghostexGpui;if(bridge&&typeof bridge.dismissSidebarContextMenus==='function'){bridge.dismissSidebarContextMenus();}})(); undefined;";

/*
CDXC:Sidebar 2026-08-20:
Pointer-leave tooltip dismissal, the event-driven half of the same contract.
The sidebar deliberately has no `data-native-pointer-inside` tooltip CSS rule:
a persistent flag would also block the next hover from opening a tooltip.
*/
#[cfg(target_os = "macos")]
pub(crate) const GPUI_SIDEBAR_DISMISS_TOOLTIPS_SCRIPT: &str = "(function(){const bridge=window.ghostexGpui;if(bridge&&typeof bridge.dismissSidebarTooltips==='function'){bridge.dismissSidebarTooltips();}})(); undefined;";

/*
CDXC:Spaces 2026-08-29:
A finger scroll gesture began inside the sidebar's native frame. The page's
Space-swipe handler resets its gesture lock on this — DOM wheel events carry no
momentum phase, so only AppKit can mark where one physical swipe ends and the
next begins.
*/
#[cfg(target_os = "macos")]
pub(crate) const GPUI_SIDEBAR_SCROLL_GESTURE_BEGAN_SCRIPT: &str = "(function(){const bridge=window.ghostexGpui;if(bridge&&typeof bridge.onNativeScrollGestureBegan==='function'){bridge.onNativeScrollGestureBegan();}})(); undefined;";

pub(crate) fn gpui_sidebar_command_pane_sessions_script(sessions: &serde_json::Value) -> String {
    format!(
        "(function(){{const bridge=window.ghostexGpui=window.ghostexGpui||{{}};bridge.commandPaneSessions={sessions};if(typeof bridge.onCommandPaneSessionsChanged==='function'){{bridge.onCommandPaneSessionsChanged(bridge.commandPaneSessions);}}}})(); undefined;"
    )
}

pub(crate) fn gpui_sidebar_agents_delayed_sends_script(sessions: &serde_json::Value) -> String {
    format!(
        "(function(){{const bridge=window.ghostexGpui=window.ghostexGpui||{{}};bridge.workspaceSessionDelayedSends={sessions};if(typeof bridge.onWorkspaceSessionDelayedSendsChanged==='function'){{bridge.onWorkspaceSessionDelayedSendsChanged(bridge.workspaceSessionDelayedSends);}}}})(); undefined;"
    )
}

pub(crate) fn gpui_sidebar_displayed_sessions_script(session_ids_json: &str) -> String {
    format!(
        "(function(){{const bridge=window.ghostexGpui=window.ghostexGpui||{{}};bridge.displayedWorkspaceSessionIds={session_ids_json};if(typeof bridge.onDisplayedWorkspaceSessionIdsChanged==='function'){{bridge.onDisplayedWorkspaceSessionIdsChanged(bridge.displayedWorkspaceSessionIds);}}}})(); undefined;"
    )
}

pub(crate) fn gpui_sidebar_browser_tabs_script(tabs_json: &str) -> String {
    format!(
        "(function(){{const bridge=window.ghostexGpui=window.ghostexGpui||{{}};bridge.browserTabs={tabs_json};if(typeof bridge.onBrowserTabsChanged==='function'){{bridge.onBrowserTabsChanged(bridge.browserTabs);}}}})(); undefined;"
    )
}

pub(crate) fn gpui_action_completion_sound_from_settings() -> &'static str {
    let settings = shared_settings::shared_sidebar_settings_snapshot();
    gpui_normalize_completion_sound(
        settings
            .object()
            .get("actionCompletionSound")
            .and_then(serde_json::Value::as_str),
    )
}

#[cfg(target_os = "macos")]
pub(crate) fn register_gpui_sidebar_pointer_callback_target(
    app: gpui::WeakEntity<GhostexGpuiApp>,
    async_app: gpui::AsyncApp,
) {
    GPUI_SIDEBAR_POINTER_CALLBACK_TARGET.with(|target| {
        *target.borrow_mut() = Some(GpuiSidebarPointerCallbackTarget { app, async_app });
    });
}

#[cfg(target_os = "macos")]
pub(crate) fn unregister_gpui_sidebar_pointer_callback_target() {
    GPUI_SIDEBAR_POINTER_CALLBACK_TARGET.with(|target| {
        *target.borrow_mut() = None;
    });
}

#[cfg(target_os = "macos")]
pub(crate) fn gpui_sidebar_pointer_callback_target() -> Option<GpuiSidebarPointerCallbackTarget> {
    GPUI_SIDEBAR_POINTER_CALLBACK_TARGET.with(|target| target.borrow().clone())
}

#[cfg(target_os = "macos")]
pub(crate) fn register_gpui_menu_bar_status_callback_target(
    app: gpui::WeakEntity<GhostexGpuiApp>,
    async_app: gpui::AsyncApp,
) {
    /*
    CDXC:StatusPet 2026-06-26-06:05:
    The AppKit Running Agents dropdown is process-global, but project/session row actions must target only the live GPUI root. Register this callback target with the app lifecycle and keep row payloads to copied bounded ids that are later routed through fixed sidebar callbacks.
    */
    GPUI_MENU_BAR_STATUS_CALLBACK_TARGET.with(|target| {
        *target.borrow_mut() = Some(GpuiMenuBarStatusCallbackTarget { app, async_app });
    });
}

#[cfg(target_os = "macos")]
pub(crate) fn unregister_gpui_menu_bar_status_callback_target() {
    GPUI_MENU_BAR_STATUS_CALLBACK_TARGET.with(|target| {
        *target.borrow_mut() = None;
    });
}

#[cfg(target_os = "macos")]
pub(crate) fn gpui_menu_bar_status_callback_target() -> Option<GpuiMenuBarStatusCallbackTarget> {
    GPUI_MENU_BAR_STATUS_CALLBACK_TARGET.with(|target| target.borrow().clone())
}

pub(crate) fn gpui_sidebar_agent_icon(value: Option<&str>) -> Option<&'static str> {
    let value = value?.trim().to_ascii_lowercase();
    match value.as_str() {
        "browser" => Some("browser"),
        "codex" => Some("codex"),
        "claude" => Some("claude"),
        "cursor" | "cursor cli" | "cursor-cli" => Some("cursor-cli"),
        "pi" | "pi agent" => Some("pi"),
        "opencode" | "open code" => Some("opencode"),
        "gemini" => Some("gemini"),
        "copilot" => Some("copilot"),
        "droid" | "factory droid" | "factory-droid" => Some("factory-droid"),
        "grok" | "grok build" | "grok-build" => Some("grok-build"),
        "antigravity" | "antigravity cli" | "antigravity-cli" => Some("antigravity-cli"),
        "amp" | "amp cli" | "amp-cli" => Some("amp-cli"),
        "rovodev" | "rovo dev" | "rovo-dev" => Some("rovo-dev"),
        "hermes-agent" | "hermes agent" => Some("hermes-agent"),
        "codebuddy" => Some("codebuddy"),
        "qoder" => Some("qoder"),
        "kiro" | "kiro cli" => Some("kiro"),
        "omp" => Some("omp"),
        "kimi" | "kimi code" | "kimi-code" => Some("kimi"),
        "openclaude" | "open claude" | "open-claude" => Some("openclaude"),
        "command-code" | "command code" | "commandcode" => Some("command-code"),
        "mastra" | "mastracode" | "mastra code" => Some("mastra"),
        "zcode" => Some("zcode"),
        "devin" => Some("devin"),
        _ => None,
    }
}

pub(crate) fn gpui_previous_session_reference_from_history_id(
    history_id: &str,
) -> Option<(&str, &str)> {
    let payload = history_id.strip_prefix("gxserver:")?;
    let (project_id, session_id) = payload.split_once(':')?;
    if session_id.contains(':')
        || !gpui_remote_sidebar_project_id_allowed(project_id)
        || !gpui_remote_sidebar_session_id_allowed(session_id)
    {
        return None;
    }
    Some((project_id, session_id))
}

pub(crate) fn gpui_sidebar_portless_state_from_update_result(
    result: &serde_json::Value,
) -> Option<serde_json::Value> {
    let status = result.get("status")?.as_object().cloned()?;
    let presentation = result.get("presentation")?.as_object().cloned()?;
    let health = serde_json::Value::Object(status.clone());
    Some(serde_json::json!({
        "health": health.clone(),
        "nativeAdmin": {
            "actions": gpui_portless_native_admin_actions(&health),
            "available": gpui_portless_native_admin_available(),
        },
        "presentation": serde_json::Value::Object(presentation),
    }))
}

pub(crate) fn gpui_sidebar_portless_state() -> Option<serde_json::Value> {
    /*
    CDXC:Portless 2026-06-24-14:28:
    GPUI Settings hydration exposes privileged Portless admin actions only when this local macOS app bundle has the reviewed Web/code-server Node and Web/portless CLI runtime. Non-macOS, development binaries, incomplete bundles, and non-recommended actions stay unavailable rather than presenting fake setup capability.
    */
    let health = gpui_gxserver_server_health(Duration::from_millis(500)).ok()?;
    let portless = health.get("portless")?.clone();
    Some(serde_json::json!({
        "health": portless.clone(),
        "nativeAdmin": {
            "actions": gpui_portless_native_admin_actions(&portless),
            "available": gpui_portless_native_admin_available(),
        },
    }))
}

/// The setup-prompt gate needs `presentation.liveListenerCount`, which the
/// short health probe does not carry; macOS reads it from the startup
/// presentation snapshot (`createSidebarPortlessState`), so GPUI reads the
/// daemon's presentation snapshot the same way.
pub(crate) fn gpui_sidebar_portless_state_with_presentation() -> Option<serde_json::Value> {
    let mut state = gpui_sidebar_portless_state()?;
    let snapshot = gpui_read_gxserver_presentation_snapshot().ok()?;
    let presentation = snapshot.get("portless")?.clone();
    state["presentation"] = presentation;
    Some(state)
}
