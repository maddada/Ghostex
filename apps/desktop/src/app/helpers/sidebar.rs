// C1 wave-1 extraction: stateless helper functions moved verbatim out of
// main.rs (pure move, no logic changes). See docs/2026-08-22/repo-restructure/SPLITS.md C1.

use std::{
    collections::HashSet,
    env, fs,
    io::Read,
    path::{Path, PathBuf},
    sync::atomic::Ordering,
    time::{Duration, SystemTime, UNIX_EPOCH},
};

// RefCell backs cross-platform runtime state (window frame persistence), not
// just the macOS-only shims that first introduced the import.


#[cfg(target_os = "windows")]
use windows_sys::Win32::Security::Cryptography::{
    BCRYPT_USE_SYSTEM_PREFERRED_RNG, BCryptGenRandom,
};

use anyhow::{Context as _, Result};
use futures::StreamExt as _;
use gpui::{
    Action, AppContext as _, Asset, Hsla, Styled as _, Window, prelude::FluentBuilder as _, rgb,
};

use crate::app::helpers::*;
use crate::*;

pub(crate) fn sidebar_background_fill() -> gpui::Background {
    /*
    CDXC:GPUISidebarDividerColor 2026-07-22:
    The resize rail is the final sibling inside the sidebar region. Paint it
    with the same vertical gradient stops as the CEF sidebar instead of a
    fixed #0e0e0e strip so custom tint/contrast settings reach the true edge.
    */
    gpui::linear_gradient(
        180.,
        gpui::linear_color_stop(
            rgb(GPUI_TITLEBAR_GRADIENT_LEFT_RGB.load(Ordering::Relaxed) as u32),
            0.,
        ),
        gpui::linear_color_stop(
            rgb(GPUI_TITLEBAR_GRADIENT_RIGHT_RGB.load(Ordering::Relaxed) as u32),
            1.,
        ),
    )
}

pub(crate) fn sidebar_cef_prepaint_background_color() -> u32 {
    /*
    CEF owns the sidebar's native child view right up to the sibling divider.
    Use the resolved sidebar base for Chromium's prepaint/background pixel so
    its AppKit edge cannot expose the old fixed #0e0e0e color.
    */
    0xff00_0000 | GPUI_TITLEBAR_BACKGROUND_RGB.load(Ordering::Relaxed) as u32
}

pub(crate) fn sidebar_divider_line_color() -> Hsla {
    rgb(0x000000).opacity(0.0).into()
}

pub(crate) fn sidebar_divider_hover_line_color() -> Hsla {
    rgb(0xffffff).into()
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
    CDXC:GPUIWindowsBringup 2026-07-04:
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
CDXC:GPUISidebarPointerTracking 2026-08-02:
Dismissal needs page code — the open menus live in a module-scoped registry
inside the sidebar bundle — so it goes through the sidebar's own bridge. If the
bridge is not installed the page cannot have an open menu either, so there is
nothing to queue.
*/
#[cfg(target_os = "macos")]
pub(crate) const GPUI_SIDEBAR_DISMISS_CONTEXT_MENUS_SCRIPT: &str = "(function(){const bridge=window.ghostexGpui;if(bridge&&typeof bridge.dismissSidebarContextMenus==='function'){bridge.dismissSidebarContextMenus();}})(); undefined;";

/*
CDXC:GPUISidebarPointerTracking 2026-08-20:
Pointer-leave tooltip dismissal, the event-driven half of the same contract.
The sidebar deliberately has no `data-native-pointer-inside` tooltip CSS rule:
a persistent flag would also block the next hover from opening a tooltip.
*/
#[cfg(target_os = "macos")]
pub(crate) const GPUI_SIDEBAR_DISMISS_TOOLTIPS_SCRIPT: &str = "(function(){const bridge=window.ghostexGpui;if(bridge&&typeof bridge.dismissSidebarTooltips==='function'){bridge.dismissSidebarTooltips();}})(); undefined;";

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
    CDXC:GPUIMenuBarStatusItem 2026-06-26-06:05:
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
        _ => None,
    }
}

pub(crate) fn gpui_previous_session_reference_from_history_id(history_id: &str) -> Option<(&str, &str)> {
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

pub(crate) fn gpui_quick_access_sidebar_groups_from_presentation_snapshot(
    snapshot: &serde_json::Value,
    active_project_id: Option<&str>,
) -> Vec<serde_json::Value> {
    gpui_titlebar_resource_groups_from_presentation_snapshot(snapshot, active_project_id)
        .into_iter()
        .filter_map(|mut group| {
            let group_object = group.as_object_mut()?;
            group_object.insert(
                "isFocusModeActive".to_string(),
                serde_json::Value::Bool(false),
            );
            group_object.insert("layoutVisibleCount".to_string(), serde_json::json!(1));
            group_object.insert("viewMode".to_string(), serde_json::json!("grid"));
            group_object.insert("visibleCount".to_string(), serde_json::json!(1));

            for session in group_object
                .get_mut("sessions")
                .and_then(serde_json::Value::as_array_mut)
                .into_iter()
                .flatten()
            {
                let Some(session_object) = session.as_object_mut() else {
                    continue;
                };
                session_object.insert("column".to_string(), serde_json::json!(0));
                session_object.insert("isFocused".to_string(), serde_json::Value::Bool(false));
                session_object.insert("isVisible".to_string(), serde_json::Value::Bool(false));
                session_object.insert("row".to_string(), serde_json::json!(0));
                session_object.insert(
                    "shortcutLabel".to_string(),
                    serde_json::Value::String(String::new()),
                );
            }
            Some(group)
        })
        .collect()
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
    CDXC:GPUIPortlessAdminBridge 2026-06-24-14:28:
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

#[derive(Clone, Copy)]
pub(crate) struct GpuiDefaultSidebarAgent {
    pub(crate) agent_id: &'static str,
    pub(crate) command: &'static str,
    pub(crate) hidden_by_default: bool,
    pub(crate) icon: &'static str,
    pub(crate) name: &'static str,
}

#[derive(Clone, Debug)]
pub(crate) struct GpuiStoredSidebarAgent {
    pub(crate) accept_all_mode: Option<String>,
    pub(crate) agent_id: String,
    pub(crate) command: String,
    pub(crate) hidden: bool,
    pub(crate) icon: Option<String>,
    pub(crate) name: String,
}

#[derive(Clone, Copy)]
pub(crate) struct GpuiDefaultSidebarCommand {
    pub(crate) command_id: &'static str,
    pub(crate) name: &'static str,
}

#[derive(Clone, Debug)]
pub(crate) struct GpuiStoredSidebarCommand {
    pub(crate) action_type: &'static str,
    pub(crate) close_terminal_on_exit: bool,
    pub(crate) command: Option<String>,
    pub(crate) command_id: String,
    pub(crate) icon: Option<String>,
    pub(crate) is_default: bool,
    pub(crate) name: String,
    pub(crate) play_completion_sound: bool,
    pub(crate) show_on_project_row: bool,
    pub(crate) url: Option<String>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum GpuiSidebarMetadataWriteKind {
    Agents,
    Commands,
}

impl GpuiSidebarMetadataWriteKind {
    pub(crate) fn failure_message(self) -> &'static str {
        match self {
            Self::Agents => "GPUI could not save the Agents settings.",
            Self::Commands => "GPUI could not save the Actions settings.",
        }
    }
}

#[derive(Clone, Debug)]
pub(crate) enum GpuiSidebarAgentMetadataWrite {
    Save {
        accept_all_mode: GpuiAgentAcceptAllModeUpdate,
        agent_id: Option<String>,
        command: String,
        icon: Option<String>,
        name: String,
    },
    Delete {
        agent_id: String,
    },
    SyncOrder {
        agent_ids: Vec<String>,
        request_id: String,
    },
}

impl GpuiSidebarAgentMetadataWrite {
    pub(crate) fn order_sync_result(
        &self,
        status: &'static str,
        item_ids: Vec<String>,
    ) -> Option<serde_json::Value> {
        match self {
            Self::SyncOrder { request_id, .. } => Some(gpui_sidebar_order_sync_result_message(
                "agent", request_id, status, item_ids,
            )),
            _ => None,
        }
    }
}

/*
CDXC:GlobalActions 2026-08-01-19:00:
Settings > Actions writes reach gxserver through this Rust path, not through the
sidebar TypeScript runtime, so Global Actions need their own scope here or the
message is parsed as a project write. Scope only selects the mutation target;
the payload and every validation rule stay identical between the two lists.
*/
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum GpuiSidebarCommandScope {
    Global,
    Project,
}

impl GpuiSidebarCommandScope {
    pub(crate) fn mutation_target(self) -> &'static str {
        match self {
            Self::Global => "globalCommand",
            Self::Project => "command",
        }
    }
}

#[derive(Clone, Debug)]
pub(crate) enum GpuiSidebarCommandMetadataWrite {
    Save {
        action_type: &'static str,
        active_project_id: String,
        close_terminal_on_exit: bool,
        command: Option<String>,
        command_id: Option<String>,
        icon: Option<String>,
        name: String,
        play_completion_sound: bool,
        scope: GpuiSidebarCommandScope,
        show_on_project_row: bool,
        url: Option<String>,
    },
    Delete {
        active_project_id: String,
        command_id: String,
        scope: GpuiSidebarCommandScope,
    },
    SyncOrder {
        active_project_id: String,
        command_ids: Vec<String>,
        request_id: String,
        scope: GpuiSidebarCommandScope,
    },
}

impl GpuiSidebarCommandMetadataWrite {
    pub(crate) fn scope(&self) -> GpuiSidebarCommandScope {
        match self {
            Self::Save { scope, .. }
            | Self::Delete { scope, .. }
            | Self::SyncOrder { scope, .. } => *scope,
        }
    }
}

impl GpuiSidebarCommandMetadataWrite {
    pub(crate) fn order_sync_result(
        &self,
        status: &'static str,
        item_ids: Vec<String>,
    ) -> Option<serde_json::Value> {
        match self {
            Self::SyncOrder { request_id, .. } => Some(gpui_sidebar_order_sync_result_message(
                "command", request_id, status, item_ids,
            )),
            _ => None,
        }
    }

    pub(crate) fn deleted_command_id(&self) -> Option<&str> {
        match self {
            Self::Delete { command_id, .. } => Some(command_id.as_str()),
            _ => None,
        }
    }
}

pub(crate) const GPUI_DEFAULT_SIDEBAR_AGENTS: &[GpuiDefaultSidebarAgent] = &[
    GpuiDefaultSidebarAgent {
        agent_id: "codex",
        command: "codex",
        hidden_by_default: false,
        icon: "codex",
        name: "Codex",
    },
    GpuiDefaultSidebarAgent {
        agent_id: "claude",
        command: "claude",
        hidden_by_default: false,
        icon: "claude",
        name: "Claude",
    },
    GpuiDefaultSidebarAgent {
        agent_id: "cursor",
        command: "cursor-agent",
        hidden_by_default: false,
        icon: "cursor-cli",
        name: "Cursor CLI",
    },
    GpuiDefaultSidebarAgent {
        agent_id: "pi",
        command: "pi",
        hidden_by_default: false,
        icon: "pi",
        name: "Pi Agent",
    },
    GpuiDefaultSidebarAgent {
        agent_id: "opencode",
        command: "opencode",
        hidden_by_default: false,
        icon: "opencode",
        name: "OpenCode",
    },
    GpuiDefaultSidebarAgent {
        agent_id: "gemini",
        command: "gemini",
        hidden_by_default: false,
        icon: "gemini",
        name: "Gemini",
    },
    GpuiDefaultSidebarAgent {
        agent_id: "copilot",
        command: "copilot",
        hidden_by_default: false,
        icon: "copilot",
        name: "Copilot",
    },
    GpuiDefaultSidebarAgent {
        agent_id: "droid",
        command: "droid",
        hidden_by_default: false,
        icon: "factory-droid",
        name: "Factory Droid",
    },
    GpuiDefaultSidebarAgent {
        agent_id: "grok",
        command: "grok",
        hidden_by_default: false,
        icon: "grok-build",
        name: "Grok Build",
    },
    GpuiDefaultSidebarAgent {
        agent_id: "antigravity",
        command: "agy",
        hidden_by_default: false,
        icon: "antigravity-cli",
        name: "Antigravity CLI",
    },
    GpuiDefaultSidebarAgent {
        agent_id: "amp",
        command: "amp",
        hidden_by_default: false,
        icon: "amp-cli",
        name: "Amp CLI",
    },
    GpuiDefaultSidebarAgent {
        agent_id: "rovodev",
        command: "acli rovodev run",
        hidden_by_default: true,
        icon: "rovo-dev",
        name: "Rovo Dev",
    },
    GpuiDefaultSidebarAgent {
        agent_id: "hermes-agent",
        command: "hermes",
        hidden_by_default: true,
        icon: "hermes-agent",
        name: "Hermes Agent",
    },
    GpuiDefaultSidebarAgent {
        agent_id: "codebuddy",
        command: "codebuddy",
        hidden_by_default: true,
        icon: "codebuddy",
        name: "CodeBuddy",
    },
    GpuiDefaultSidebarAgent {
        agent_id: "qoder",
        command: "qodercli",
        hidden_by_default: true,
        icon: "qoder",
        name: "Qoder",
    },
    GpuiDefaultSidebarAgent {
        agent_id: "kiro",
        command: "kiro-cli chat --agent ghostex",
        hidden_by_default: true,
        icon: "kiro",
        name: "Kiro CLI",
    },
    GpuiDefaultSidebarAgent {
        agent_id: "omp",
        command: "omp",
        hidden_by_default: true,
        icon: "omp",
        name: "OMP",
    },
];

pub(crate) const GPUI_DEFAULT_SIDEBAR_COMMANDS: &[GpuiDefaultSidebarCommand] = &[
    GpuiDefaultSidebarCommand {
        command_id: "dev",
        name: "Dev",
    },
    GpuiDefaultSidebarCommand {
        command_id: "build",
        name: "Build",
    },
    GpuiDefaultSidebarCommand {
        command_id: "test",
        name: "Test",
    },
    GpuiDefaultSidebarCommand {
        command_id: "setup",
        name: "Setup",
    },
];

pub(crate) const GPUI_DEFAULT_SIDEBAR_COMMAND_ICON: &str = "playerPlay";
pub(crate) const GPUI_SIDEBAR_COMMAND_ICON_IDS: &[&str] = &[
    "playerPlay",
    "api",
    "archive",
    "bell",
    "bolt",
    "book",
    "brain",
    "braces",
    "brandDocker",
    "brandGithub",
    "brandPython",
    "brandReact",
    "brandVscode",
    "bug",
    "chartBar",
    "cloud",
    "checklist",
    "clock",
    "code",
    "command",
    "cpu",
    "database",
    "deviceDesktop",
    "deviceLaptop",
    "download",
    "fileCode",
    "fileDiff",
    "fileSearch",
    "fileText",
    "flask",
    "folder",
    "folderOpen",
    "gitBranch",
    "gitCommit",
    "gitMerge",
    "gitPullRequest",
    "key",
    "layoutDashboard",
    "link",
    "lock",
    "messageCircle",
    "package",
    "pencilCode",
    "refresh",
    "robot",
    "route",
    "rocket",
    "search",
    "server",
    "settings",
    "shieldSearch",
    "sparkles",
    "stack",
    "terminal",
    "testPipe",
    "tool",
    "upload",
    "wand",
    "world",
];

#[derive(Clone, Debug)]
pub(crate) struct GpuiSidebarHudButtons {
    pub(crate) agents: serde_json::Value,
    pub(crate) commands: serde_json::Value,
    /*
    CDXC:GlobalActions 2026-08-01-19:00:
    The Settings app modal reads its Actions lists from this Rust-side HUD fetch,
    not from the sidebar TypeScript runtime, so Global Actions have to be carried
    here too or the new section renders empty no matter what is stored.
    */
    pub(crate) global_commands: serde_json::Value,
}

pub(crate) fn gpui_sidebar_hud_array_field(
    result: &serde_json::Value,
    key: &str,
) -> Result<serde_json::Value, String> {
    result
        .get(key)
        .and_then(serde_json::Value::as_array)
        .cloned()
        .map(serde_json::Value::Array)
        .ok_or_else(|| GPUI_SIDEBAR_METADATA_GENERIC_ERROR.to_string())
}

#[allow(dead_code)] // no caller: sidebar agent/command metadata is projected by gxserver now; kept as the local-state derivation
pub(crate) fn gpui_sidebar_agent_state_from_domain_projects(
    domain_projects: &[serde_json::Value],
) -> (Vec<GpuiStoredSidebarAgent>, Vec<String>) {
    let source_project = domain_projects.iter().find(|project| {
        let Some(project) = project.as_object() else {
            return false;
        };
        gpui_json_array_field_is_nonempty(project, "customAgents")
            || gpui_json_array_field_is_nonempty(project, "customAgentOrder")
    });
    let source_project = source_project.and_then(serde_json::Value::as_object);
    let stored_agents = gpui_normalized_stored_sidebar_agents(
        source_project.and_then(|project| project.get("customAgents")),
    );
    let stored_order = gpui_normalized_string_order(
        source_project.and_then(|project| project.get("customAgentOrder")),
    );
    (stored_agents, stored_order)
}

pub(crate) fn gpui_sidebar_agent_buttons_from_state(
    stored_agents: &[GpuiStoredSidebarAgent],
    stored_order: &[String],
) -> serde_json::Value {
    let mut buttons = Vec::<(String, serde_json::Value)>::new();
    for default_agent in GPUI_DEFAULT_SIDEBAR_AGENTS {
        let stored_agent = stored_agents
            .iter()
            .find(|agent| agent.agent_id == default_agent.agent_id);
        if stored_agent.is_none() && default_agent.hidden_by_default {
            continue;
        }
        if stored_agent.map(|agent| agent.hidden).unwrap_or(false) {
            continue;
        }

        let button = match stored_agent {
            Some(stored_agent) => {
                let name =
                    gpui_default_sidebar_agent_name(default_agent.agent_id, &stored_agent.name);
                gpui_sidebar_agent_button_value(
                    Some(stored_agent),
                    stored_agent.agent_id.as_str(),
                    stored_agent.command.as_str(),
                    stored_agent.icon.as_deref().unwrap_or(default_agent.icon),
                    true,
                    &name,
                )
            }
            None => gpui_sidebar_agent_button_value(
                None,
                default_agent.agent_id,
                default_agent.command,
                default_agent.icon,
                true,
                default_agent.name,
            ),
        };
        buttons.push((default_agent.agent_id.to_string(), button));
    }

    for stored_agent in stored_agents {
        if gpui_is_default_sidebar_agent_id(&stored_agent.agent_id) || stored_agent.hidden {
            continue;
        }
        let icon = stored_agent.icon.as_deref();
        buttons.push((
            stored_agent.agent_id.clone(),
            gpui_sidebar_agent_button_value(
                Some(stored_agent),
                &stored_agent.agent_id,
                &stored_agent.command,
                icon.unwrap_or(""),
                false,
                &stored_agent.name,
            ),
        ));
    }

    gpui_order_json_buttons(buttons, stored_order, "agentId")
}

pub(crate) fn gpui_sidebar_agent_button_value(
    stored_agent: Option<&GpuiStoredSidebarAgent>,
    agent_id: &str,
    command: &str,
    icon: &str,
    is_default: bool,
    name: &str,
) -> serde_json::Value {
    let mut button = serde_json::Map::new();
    if let Some(accept_all_mode) = stored_agent.and_then(|agent| agent.accept_all_mode.as_ref()) {
        button.insert(
            "acceptAllMode".to_string(),
            serde_json::Value::String(accept_all_mode.clone()),
        );
    }
    button.insert(
        "agentId".to_string(),
        serde_json::Value::String(agent_id.to_string()),
    );
    button.insert(
        "command".to_string(),
        serde_json::Value::String(command.to_string()),
    );
    if !icon.is_empty() {
        button.insert(
            "icon".to_string(),
            serde_json::Value::String(icon.to_string()),
        );
    }
    button.insert("isDefault".to_string(), serde_json::Value::Bool(is_default));
    button.insert(
        "name".to_string(),
        serde_json::Value::String(name.to_string()),
    );
    serde_json::Value::Object(button)
}

#[allow(dead_code)] // no caller: sidebar agent/command metadata is projected by gxserver now; kept as the local-state derivation
pub(crate) fn gpui_sidebar_command_state_for_active_project(
    domain_projects: &[serde_json::Value],
    active_project_id: &str,
) -> Result<
    (
        String,
        Vec<GpuiStoredSidebarCommand>,
        Vec<String>,
        Vec<String>,
    ),
    String,
> {
    let active_project = domain_projects
        .iter()
        .filter_map(serde_json::Value::as_object)
        .find(|project| {
            gpui_trimmed_json_string_field(project, "projectId") == Some(active_project_id)
        })
        .ok_or_else(|| GPUI_SIDEBAR_METADATA_GENERIC_ERROR.to_string())?;
    let active_project_id = gpui_trimmed_json_string_field(active_project, "projectId")
        .ok_or_else(|| GPUI_SIDEBAR_METADATA_GENERIC_ERROR.to_string())?;
    let owner_project_id = active_project
        .get("worktree")
        .and_then(serde_json::Value::as_object)
        .and_then(|worktree| gpui_trimmed_json_string_field(worktree, "parentProjectId"))
        .unwrap_or(active_project_id);
    let owner_project = domain_projects
        .iter()
        .filter_map(serde_json::Value::as_object)
        .find(|project| {
            gpui_trimmed_json_string_field(project, "projectId") == Some(owner_project_id)
        })
        .ok_or_else(|| GPUI_SIDEBAR_METADATA_GENERIC_ERROR.to_string())?;
    Ok((
        owner_project_id.to_string(),
        gpui_normalized_stored_sidebar_commands(owner_project.get("customCommands")),
        gpui_normalized_string_order(owner_project.get("customCommandOrder")),
        gpui_normalized_string_order(owner_project.get("deletedDefaultCommandIds")),
    ))
}

pub(crate) fn gpui_sidebar_command_buttons_from_state(
    stored_commands: &[GpuiStoredSidebarCommand],
    stored_order: &[String],
    deleted_default_command_ids: &[String],
) -> serde_json::Value {
    let deleted_default_command_ids = deleted_default_command_ids
        .iter()
        .map(String::as_str)
        .collect::<HashSet<_>>();
    let mut buttons = Vec::<(String, serde_json::Value)>::new();

    for default_command in GPUI_DEFAULT_SIDEBAR_COMMANDS {
        if deleted_default_command_ids.contains(default_command.command_id) {
            continue;
        }
        let button = stored_commands
            .iter()
            .find(|command| command.command_id == default_command.command_id)
            .map(gpui_sidebar_command_button_value)
            .unwrap_or_else(|| gpui_default_sidebar_command_button_value(default_command));
        buttons.push((default_command.command_id.to_string(), button));
    }

    for stored_command in stored_commands {
        if gpui_is_default_sidebar_command_id(&stored_command.command_id) {
            continue;
        }
        buttons.push((
            stored_command.command_id.clone(),
            gpui_sidebar_command_button_value(stored_command),
        ));
    }

    gpui_order_json_buttons(buttons, stored_order, "commandId")
}

pub(crate) fn gpui_default_sidebar_command_button_value(
    command: &GpuiDefaultSidebarCommand,
) -> serde_json::Value {
    serde_json::json!({
        "actionType": "terminal",
        "closeTerminalOnExit": false,
        "commandId": command.command_id,
        "isDefault": true,
        "name": command.name,
        "playCompletionSound": true,
        "showOnProjectRow": false,
    })
}

pub(crate) fn gpui_sidebar_command_button_value(command: &GpuiStoredSidebarCommand) -> serde_json::Value {
    let mut button = serde_json::Map::new();
    button.insert(
        "actionType".to_string(),
        serde_json::Value::String(command.action_type.to_string()),
    );
    button.insert(
        "closeTerminalOnExit".to_string(),
        serde_json::Value::Bool(command.close_terminal_on_exit),
    );
    if let Some(command_text) = command.command.as_ref() {
        button.insert(
            "command".to_string(),
            serde_json::Value::String(command_text.clone()),
        );
    }
    button.insert(
        "commandId".to_string(),
        serde_json::Value::String(command.command_id.clone()),
    );
    if let Some(icon) = command.icon.as_ref() {
        button.insert("icon".to_string(), serde_json::Value::String(icon.clone()));
    }
    button.insert(
        "isDefault".to_string(),
        serde_json::Value::Bool(command.is_default),
    );
    button.insert(
        "name".to_string(),
        serde_json::Value::String(command.name.clone()),
    );
    button.insert(
        "playCompletionSound".to_string(),
        serde_json::Value::Bool(command.play_completion_sound),
    );
    button.insert(
        "showOnProjectRow".to_string(),
        serde_json::Value::Bool(command.show_on_project_row),
    );
    if let Some(url) = command.url.as_ref() {
        button.insert("url".to_string(), serde_json::Value::String(url.clone()));
    }
    serde_json::Value::Object(button)
}

pub(crate) fn gpui_normalized_stored_sidebar_agents(
    candidate: Option<&serde_json::Value>,
) -> Vec<GpuiStoredSidebarAgent> {
    let Some(items) = candidate.and_then(serde_json::Value::as_array) else {
        return Vec::new();
    };
    let mut agents = Vec::new();
    let mut seen_agent_ids = HashSet::new();
    for item in items {
        let Some(item) = item.as_object() else {
            continue;
        };
        let Some(agent_id) = gpui_trimmed_json_string_field(item, "agentId") else {
            continue;
        };
        if seen_agent_ids.contains(agent_id) {
            continue;
        }
        let Some(name) = gpui_trimmed_json_string_field(item, "name") else {
            continue;
        };
        let Some(command) = gpui_trimmed_json_string_field(item, "command") else {
            continue;
        };
        agents.push(GpuiStoredSidebarAgent {
            accept_all_mode: item
                .get("acceptAllMode")
                .and_then(serde_json::Value::as_str)
                .filter(|mode| matches!(*mode, "inherit" | "enabled" | "disabled"))
                .map(str::to_string),
            agent_id: agent_id.to_string(),
            command: command.to_string(),
            hidden: item
                .get("hidden")
                .and_then(serde_json::Value::as_bool)
                .unwrap_or(false),
            icon: item
                .get("icon")
                .and_then(serde_json::Value::as_str)
                .and_then(gpui_strict_sidebar_agent_icon)
                .map(str::to_string),
            name: name.to_string(),
        });
        seen_agent_ids.insert(agent_id.to_string());
    }
    agents
}

pub(crate) fn gpui_normalized_stored_sidebar_commands(
    candidate: Option<&serde_json::Value>,
) -> Vec<GpuiStoredSidebarCommand> {
    let Some(items) = candidate.and_then(serde_json::Value::as_array) else {
        return Vec::new();
    };
    let mut commands = Vec::new();
    let mut seen_command_ids = HashSet::new();
    for item in items {
        let Some(item) = item.as_object() else {
            continue;
        };
        let Some(command_id) = gpui_trimmed_json_string_field(item, "commandId") else {
            continue;
        };
        if seen_command_ids.contains(command_id) {
            continue;
        }
        let url = item
            .get("url")
            .and_then(serde_json::Value::as_str)
            .and_then(|url| gpui_trimmed_nonempty_str(Some(url)))
            .map(str::to_string);
        let action_type = match item.get("actionType").and_then(serde_json::Value::as_str) {
            Some("browser") => "browser",
            Some("terminal") => "terminal",
            _ if url.is_some() => "browser",
            _ => "terminal",
        };
        let name = item
            .get("name")
            .and_then(serde_json::Value::as_str)
            .map(str::trim)
            .unwrap_or("")
            .to_string();
        let icon = item
            .get("icon")
            .and_then(serde_json::Value::as_str)
            .and_then(gpui_sidebar_command_icon)
            .map(str::to_string);
        let is_default = item
            .get("isDefault")
            .and_then(serde_json::Value::as_bool)
            .unwrap_or(false)
            || gpui_is_default_sidebar_command_id(command_id);

        let show_on_project_row = item
            .get("showOnProjectRow")
            .and_then(serde_json::Value::as_bool)
            .unwrap_or(false);

        if action_type == "browser" {
            let Some(url) = url else {
                continue;
            };
            commands.push(GpuiStoredSidebarCommand {
                action_type,
                close_terminal_on_exit: false,
                command: None,
                command_id: command_id.to_string(),
                icon,
                is_default,
                name,
                play_completion_sound: false,
                show_on_project_row,
                url: Some(url),
            });
            seen_command_ids.insert(command_id.to_string());
            continue;
        }

        let Some(command_text) = item
            .get("command")
            .and_then(serde_json::Value::as_str)
            .and_then(|command| gpui_trimmed_nonempty_str(Some(command)))
        else {
            continue;
        };
        commands.push(GpuiStoredSidebarCommand {
            action_type,
            close_terminal_on_exit: item
                .get("closeTerminalOnExit")
                .and_then(serde_json::Value::as_bool)
                .unwrap_or(false),
            command: Some(command_text.to_string()),
            command_id: command_id.to_string(),
            icon,
            is_default,
            name,
            play_completion_sound: item
                .get("playCompletionSound")
                .and_then(serde_json::Value::as_bool)
                .unwrap_or(true),
            show_on_project_row,
            url: None,
        });
        seen_command_ids.insert(command_id.to_string());
    }
    commands
}

pub(crate) fn gpui_normalized_string_order(candidate: Option<&serde_json::Value>) -> Vec<String> {
    let Some(items) = candidate.and_then(serde_json::Value::as_array) else {
        return Vec::new();
    };
    gpui_normalized_string_order_from_values(items)
}

pub(crate) fn gpui_normalized_string_order_from_values(items: &[serde_json::Value]) -> Vec<String> {
    let mut order = Vec::new();
    let mut seen_ids = HashSet::new();
    for item in items {
        let Some(item) = item
            .as_str()
            .and_then(|value| gpui_trimmed_nonempty_str(Some(value)))
        else {
            continue;
        };
        if seen_ids.insert(item.to_string()) {
            order.push(item.to_string());
        }
    }
    order
}

pub(crate) fn gpui_sidebar_agent_metadata_write_from_command(
    command: &serde_json::Map<String, serde_json::Value>,
) -> Result<GpuiSidebarAgentMetadataWrite, String> {
    match command.get("type").and_then(serde_json::Value::as_str) {
        Some("saveSidebarAgent") => {
            let name = gpui_trimmed_json_string_field(command, "name")
                .ok_or_else(|| GPUI_SIDEBAR_METADATA_GENERIC_ERROR.to_string())?
                .to_string();
            let command_text = gpui_trimmed_json_string_field(command, "command")
                .ok_or_else(|| GPUI_SIDEBAR_METADATA_GENERIC_ERROR.to_string())?
                .to_string();
            let agent_id = command
                .get("agentId")
                .and_then(serde_json::Value::as_str)
                .and_then(|value| gpui_trimmed_nonempty_str(Some(value)))
                .map(str::to_string);
            let icon = command
                .get("icon")
                .and_then(serde_json::Value::as_str)
                .and_then(gpui_strict_sidebar_agent_icon)
                .map(str::to_string);
            let accept_all_mode = match command.get("acceptAllMode") {
                None => GpuiAgentAcceptAllModeUpdate::Preserve,
                Some(value) => match value.as_str() {
                    Some("inherit") => GpuiAgentAcceptAllModeUpdate::Set(None),
                    Some("enabled") | Some("disabled") => {
                        GpuiAgentAcceptAllModeUpdate::Set(value.as_str().map(str::to_string))
                    }
                    _ => return Err(GPUI_SIDEBAR_METADATA_GENERIC_ERROR.to_string()),
                },
            };
            Ok(GpuiSidebarAgentMetadataWrite::Save {
                accept_all_mode,
                agent_id,
                command: command_text,
                icon,
                name,
            })
        }
        Some("deleteSidebarAgent") => {
            let agent_id = gpui_trimmed_json_string_field(command, "agentId")
                .ok_or_else(|| GPUI_SIDEBAR_METADATA_GENERIC_ERROR.to_string())?
                .to_string();
            Ok(GpuiSidebarAgentMetadataWrite::Delete { agent_id })
        }
        Some("syncSidebarAgentOrder") => {
            let request_id = command
                .get("requestId")
                .and_then(serde_json::Value::as_str)
                .unwrap_or_default()
                .to_string();
            let agent_ids = command
                .get("agentIds")
                .and_then(serde_json::Value::as_array)
                .map(|items| gpui_normalized_string_order_from_values(items))
                .unwrap_or_default();
            Ok(GpuiSidebarAgentMetadataWrite::SyncOrder {
                agent_ids,
                request_id,
            })
        }
        _ => Err(GPUI_SIDEBAR_METADATA_GENERIC_ERROR.to_string()),
    }
}

pub(crate) fn gpui_sidebar_command_metadata_write_from_command(
    command: &serde_json::Map<String, serde_json::Value>,
    active_project_id: Option<String>,
) -> Result<GpuiSidebarCommandMetadataWrite, String> {
    let active_project_id = active_project_id.unwrap_or_default();
    /*
    CDXC:GlobalActions 2026-08-01-19:00:
    Global and Project Action writes carry the identical payload and differ only
    in which list they land in, so they share one parser and one set of
    validation rules. Parsing them separately is how the two lists would start
    accepting different action shapes.
    */
    let message_type = command.get("type").and_then(serde_json::Value::as_str);
    let scope = match message_type {
        Some("saveGlobalSidebarCommand")
        | Some("deleteGlobalSidebarCommand")
        | Some("syncGlobalSidebarCommandOrder") => GpuiSidebarCommandScope::Global,
        _ => GpuiSidebarCommandScope::Project,
    };
    match message_type {
        Some("saveSidebarCommand") | Some("saveGlobalSidebarCommand") => {
            let name = command
                .get("name")
                .and_then(serde_json::Value::as_str)
                .map(str::trim)
                .unwrap_or_default()
                .to_string();
            let icon = command
                .get("icon")
                .and_then(serde_json::Value::as_str)
                .and_then(gpui_sidebar_command_icon)
                .map(str::to_string);
            if name.is_empty() && icon.is_none() {
                return Err(GPUI_SIDEBAR_METADATA_GENERIC_ERROR.to_string());
            }
            let action_type = match command
                .get("actionType")
                .and_then(serde_json::Value::as_str)
            {
                Some("browser") => "browser",
                Some("terminal") => "terminal",
                _ => return Err(GPUI_SIDEBAR_METADATA_GENERIC_ERROR.to_string()),
            };
            let terminal_command = command
                .get("command")
                .and_then(serde_json::Value::as_str)
                .and_then(|value| gpui_trimmed_nonempty_str(Some(value)))
                .map(str::to_string);
            let url = command
                .get("url")
                .and_then(serde_json::Value::as_str)
                .and_then(|value| gpui_trimmed_nonempty_str(Some(value)))
                .map(str::to_string);
            if action_type == "browser" && url.is_none() {
                return Err(GPUI_SIDEBAR_METADATA_GENERIC_ERROR.to_string());
            }
            if action_type == "terminal" && terminal_command.is_none() {
                return Err(GPUI_SIDEBAR_METADATA_GENERIC_ERROR.to_string());
            }
            Ok(GpuiSidebarCommandMetadataWrite::Save {
                action_type,
                active_project_id,
                scope,
                close_terminal_on_exit: action_type == "terminal"
                    && command
                        .get("closeTerminalOnExit")
                        .and_then(serde_json::Value::as_bool)
                        .unwrap_or(false),
                command: (action_type == "terminal")
                    .then_some(terminal_command)
                    .flatten(),
                command_id: command
                    .get("commandId")
                    .and_then(serde_json::Value::as_str)
                    .and_then(|value| gpui_trimmed_nonempty_str(Some(value)))
                    .map(str::to_string),
                icon,
                name,
                play_completion_sound: action_type == "terminal"
                    && command
                        .get("playCompletionSound")
                        .and_then(serde_json::Value::as_bool)
                        .unwrap_or(true),
                show_on_project_row: command
                    .get("showOnProjectRow")
                    .and_then(serde_json::Value::as_bool)
                    .unwrap_or(false),
                url: (action_type == "browser").then_some(url).flatten(),
            })
        }
        Some("deleteSidebarCommand") | Some("deleteGlobalSidebarCommand") => {
            let command_id = gpui_trimmed_json_string_field(command, "commandId")
                .ok_or_else(|| GPUI_SIDEBAR_METADATA_GENERIC_ERROR.to_string())?
                .to_string();
            Ok(GpuiSidebarCommandMetadataWrite::Delete {
                active_project_id,
                command_id,
                scope,
            })
        }
        Some("syncSidebarCommandOrder") | Some("syncGlobalSidebarCommandOrder") => {
            let request_id = command
                .get("requestId")
                .and_then(serde_json::Value::as_str)
                .unwrap_or_default()
                .to_string();
            let command_ids = command
                .get("commandIds")
                .and_then(serde_json::Value::as_array)
                .map(|items| gpui_normalized_string_order_from_values(items))
                .unwrap_or_default();
            Ok(GpuiSidebarCommandMetadataWrite::SyncOrder {
                active_project_id,
                command_ids,
                request_id,
                scope,
            })
        }
        _ => Err(GPUI_SIDEBAR_METADATA_GENERIC_ERROR.to_string()),
    }
}

pub(crate) fn gpui_sidebar_agent_mutation_params(
    write: &GpuiSidebarAgentMetadataWrite,
    active_project_id: Option<String>,
) -> serde_json::Value {
    let mut params = serde_json::Map::new();
    params.insert(
        "target".to_string(),
        serde_json::Value::String("agent".to_string()),
    );
    if let Some(active_project_id) = active_project_id
        .as_deref()
        .and_then(|value| gpui_trimmed_nonempty_str(Some(value)))
    {
        params.insert(
            "activeProjectId".to_string(),
            serde_json::Value::String(active_project_id.to_string()),
        );
    }
    match write {
        GpuiSidebarAgentMetadataWrite::Save {
            accept_all_mode,
            agent_id,
            command,
            icon,
            name,
        } => {
            params.insert(
                "operation".to_string(),
                serde_json::Value::String("save".to_string()),
            );
            gpui_insert_optional_nonempty_string(&mut params, "agentId", agent_id.as_deref());
            params.insert(
                "command".to_string(),
                serde_json::Value::String(command.clone()),
            );
            gpui_insert_optional_nonempty_string(&mut params, "icon", icon.as_deref());
            params.insert("name".to_string(), serde_json::Value::String(name.clone()));
            match accept_all_mode {
                GpuiAgentAcceptAllModeUpdate::Preserve => {}
                GpuiAgentAcceptAllModeUpdate::Set(None) => {
                    params.insert(
                        "acceptAllMode".to_string(),
                        serde_json::Value::String("inherit".to_string()),
                    );
                }
                GpuiAgentAcceptAllModeUpdate::Set(Some(mode)) => {
                    params.insert(
                        "acceptAllMode".to_string(),
                        serde_json::Value::String(mode.clone()),
                    );
                }
            }
        }
        GpuiSidebarAgentMetadataWrite::Delete { agent_id } => {
            params.insert(
                "operation".to_string(),
                serde_json::Value::String("delete".to_string()),
            );
            params.insert(
                "agentId".to_string(),
                serde_json::Value::String(agent_id.clone()),
            );
        }
        GpuiSidebarAgentMetadataWrite::SyncOrder { agent_ids, .. } => {
            params.insert(
                "operation".to_string(),
                serde_json::Value::String("order".to_string()),
            );
            params.insert("agentIds".to_string(), gpui_string_array_value(agent_ids));
        }
    }
    serde_json::Value::Object(params)
}

pub(crate) fn gpui_sidebar_command_mutation_params(
    write: &GpuiSidebarCommandMetadataWrite,
) -> serde_json::Value {
    let mut params = serde_json::Map::new();
    params.insert(
        "target".to_string(),
        serde_json::Value::String(write.scope().mutation_target().to_string()),
    );
    match write {
        GpuiSidebarCommandMetadataWrite::Save {
            action_type,
            active_project_id,
            close_terminal_on_exit,
            command,
            command_id,
            scope: _,
            icon,
            name,
            play_completion_sound,
            show_on_project_row,
            url,
        } => {
            params.insert(
                "operation".to_string(),
                serde_json::Value::String("save".to_string()),
            );
            params.insert(
                "actionType".to_string(),
                serde_json::Value::String((*action_type).to_string()),
            );
            params.insert(
                "activeProjectId".to_string(),
                serde_json::Value::String(active_project_id.clone()),
            );
            params.insert(
                "closeTerminalOnExit".to_string(),
                serde_json::Value::Bool(*close_terminal_on_exit),
            );
            gpui_insert_optional_nonempty_string(&mut params, "command", command.as_deref());
            gpui_insert_optional_nonempty_string(&mut params, "commandId", command_id.as_deref());
            gpui_insert_optional_nonempty_string(&mut params, "icon", icon.as_deref());
            params.insert("name".to_string(), serde_json::Value::String(name.clone()));
            params.insert(
                "playCompletionSound".to_string(),
                serde_json::Value::Bool(*play_completion_sound),
            );
            params.insert(
                "showOnProjectRow".to_string(),
                serde_json::Value::Bool(*show_on_project_row),
            );
            gpui_insert_optional_nonempty_string(&mut params, "url", url.as_deref());
        }
        GpuiSidebarCommandMetadataWrite::Delete {
            active_project_id,
            command_id,
            scope: _,
        } => {
            params.insert(
                "operation".to_string(),
                serde_json::Value::String("delete".to_string()),
            );
            params.insert(
                "activeProjectId".to_string(),
                serde_json::Value::String(active_project_id.clone()),
            );
            params.insert(
                "commandId".to_string(),
                serde_json::Value::String(command_id.clone()),
            );
        }
        GpuiSidebarCommandMetadataWrite::SyncOrder {
            active_project_id,
            command_ids,
            ..
        } => {
            params.insert(
                "operation".to_string(),
                serde_json::Value::String("order".to_string()),
            );
            params.insert(
                "activeProjectId".to_string(),
                serde_json::Value::String(active_project_id.clone()),
            );
            params.insert(
                "commandIds".to_string(),
                gpui_string_array_value(command_ids),
            );
        }
    }
    serde_json::Value::Object(params)
}

pub(crate) fn gpui_sidebar_metadata_mutation_item_ids(result: &serde_json::Value) -> Vec<String> {
    result
        .get("itemIds")
        .and_then(serde_json::Value::as_array)
        .map(|items| gpui_normalized_string_order_from_values(items))
        .unwrap_or_default()
}

pub(crate) fn gpui_apply_sidebar_agent_metadata_write(
    write: GpuiSidebarAgentMetadataWrite,
    active_project_id: Option<String>,
) -> Result<Option<serde_json::Value>, String> {
    let params = gpui_sidebar_agent_mutation_params(&write, active_project_id);
    let result = gpui_gxserver_rpc_result(
        "/api/mutateSidebarHudSettings",
        &params,
        Duration::from_secs(10),
    )?;
    let item_ids = gpui_sidebar_metadata_mutation_item_ids(&result);
    Ok(match write {
        GpuiSidebarAgentMetadataWrite::SyncOrder { request_id, .. } => Some(
            gpui_sidebar_order_sync_result_message("agent", &request_id, "success", item_ids),
        ),
        _ => None,
    })
}

#[allow(dead_code)] // no caller: sidebar agent/command metadata is projected by gxserver now; kept as the local-state derivation
pub(crate) fn gpui_next_sidebar_agent_metadata_state(
    stored_agents: Vec<GpuiStoredSidebarAgent>,
    stored_order: Vec<String>,
    write: GpuiSidebarAgentMetadataWrite,
) -> Result<
    (
        Vec<GpuiStoredSidebarAgent>,
        Vec<String>,
        Option<serde_json::Value>,
    ),
    String,
> {
    match write {
        GpuiSidebarAgentMetadataWrite::Save {
            accept_all_mode,
            agent_id,
            command,
            icon,
            name,
        } => {
            let current_agent_ids = gpui_sidebar_button_ids(
                &gpui_sidebar_agent_buttons_from_state(&stored_agents, &stored_order),
                "agentId",
            );
            let selected_default_agent_id = icon
                .as_deref()
                .and_then(gpui_default_sidebar_agent_by_icon)
                .map(|agent| agent.agent_id);
            let should_restore_hidden_default = agent_id.is_none()
                && selected_default_agent_id
                    .map(|agent_id| !gpui_is_sidebar_agent_visible(&stored_agents, agent_id))
                    .unwrap_or(false);
            let next_agent_id = agent_id
                .or_else(|| {
                    should_restore_hidden_default
                        .then_some(selected_default_agent_id)
                        .flatten()
                        .map(str::to_string)
                })
                .unwrap_or_else(|| gpui_create_custom_sidebar_agent_id(&name));
            let existing_index = stored_agents
                .iter()
                .position(|agent| agent.agent_id == next_agent_id);
            let previous_agent = existing_index.and_then(|index| stored_agents.get(index));
            let default_agent = gpui_default_sidebar_agent_by_id(&next_agent_id);
            let next_agent = GpuiStoredSidebarAgent {
                accept_all_mode: match accept_all_mode {
                    GpuiAgentAcceptAllModeUpdate::Preserve => previous_agent
                        .and_then(|agent| agent.accept_all_mode.as_ref())
                        .cloned(),
                    GpuiAgentAcceptAllModeUpdate::Set(mode) => mode,
                },
                agent_id: next_agent_id.clone(),
                command,
                hidden: false,
                icon: icon
                    .or_else(|| {
                        previous_agent
                            .and_then(|agent| agent.icon.as_ref())
                            .cloned()
                    })
                    .or_else(|| default_agent.map(|agent| agent.icon.to_string())),
                name,
            };
            let mut next_agents = stored_agents.clone();
            if let Some(existing_index) = existing_index {
                next_agents[existing_index] = next_agent;
            } else {
                next_agents.push(next_agent);
            }
            let next_order = if existing_index.is_some()
                || stored_order
                    .iter()
                    .any(|candidate| candidate == &next_agent_id)
                || gpui_is_default_sidebar_agent_id(&next_agent_id)
            {
                stored_order
            } else {
                let mut next_order = current_agent_ids;
                next_order.push(next_agent_id);
                next_order
            };
            Ok((next_agents, next_order, None))
        }
        GpuiSidebarAgentMetadataWrite::Delete { agent_id } => {
            if !gpui_is_default_sidebar_agent_id(&agent_id) {
                let next_agents = stored_agents
                    .into_iter()
                    .filter(|agent| agent.agent_id != agent_id)
                    .collect::<Vec<_>>();
                let next_order = stored_order
                    .into_iter()
                    .filter(|candidate| candidate != &agent_id)
                    .collect::<Vec<_>>();
                return Ok((next_agents, next_order, None));
            }
            let Some(default_agent) = gpui_default_sidebar_agent_by_id(&agent_id) else {
                return Ok((stored_agents, stored_order, None));
            };
            let existing_index = stored_agents
                .iter()
                .position(|agent| agent.agent_id == agent_id);
            let previous_agent = existing_index.and_then(|index| stored_agents.get(index));
            let next_agent = GpuiStoredSidebarAgent {
                accept_all_mode: None,
                agent_id: default_agent.agent_id.to_string(),
                command: previous_agent
                    .map(|agent| agent.command.clone())
                    .unwrap_or_else(|| default_agent.command.to_string()),
                hidden: true,
                icon: previous_agent
                    .and_then(|agent| agent.icon.as_ref())
                    .cloned()
                    .or_else(|| Some(default_agent.icon.to_string())),
                name: previous_agent
                    .map(|agent| agent.name.clone())
                    .unwrap_or_else(|| default_agent.name.to_string()),
            };
            let mut next_agents = stored_agents.clone();
            if let Some(existing_index) = existing_index {
                next_agents[existing_index] = next_agent;
            } else {
                next_agents.push(next_agent);
            }
            let next_order = stored_order
                .into_iter()
                .filter(|candidate| candidate != &agent_id)
                .collect::<Vec<_>>();
            Ok((next_agents, next_order, None))
        }
        GpuiSidebarAgentMetadataWrite::SyncOrder {
            agent_ids,
            request_id,
        } => {
            let current_agent_ids = gpui_sidebar_button_ids(
                &gpui_sidebar_agent_buttons_from_state(&stored_agents, &stored_order),
                "agentId",
            );
            let mut next_order = agent_ids
                .into_iter()
                .filter(|agent_id| {
                    current_agent_ids
                        .iter()
                        .any(|candidate| candidate == agent_id)
                })
                .collect::<Vec<_>>();
            for agent_id in current_agent_ids {
                if !next_order.iter().any(|candidate| candidate == &agent_id) {
                    next_order.push(agent_id);
                }
            }
            let item_ids = gpui_sidebar_button_ids(
                &gpui_sidebar_agent_buttons_from_state(&stored_agents, &next_order),
                "agentId",
            );
            Ok((
                stored_agents,
                next_order,
                Some(gpui_sidebar_order_sync_result_message(
                    "agent",
                    &request_id,
                    "success",
                    item_ids,
                )),
            ))
        }
    }
}

pub(crate) fn gpui_apply_sidebar_command_metadata_write(
    write: GpuiSidebarCommandMetadataWrite,
) -> Result<Option<serde_json::Value>, String> {
    let params = gpui_sidebar_command_mutation_params(&write);
    let result = gpui_gxserver_rpc_result(
        "/api/mutateSidebarHudSettings",
        &params,
        Duration::from_secs(10),
    )?;
    let item_ids = gpui_sidebar_metadata_mutation_item_ids(&result);
    Ok(match write {
        GpuiSidebarCommandMetadataWrite::SyncOrder { request_id, .. } => Some(
            gpui_sidebar_order_sync_result_message("command", &request_id, "success", item_ids),
        ),
        _ => None,
    })
}

#[allow(dead_code)] // no caller: sidebar agent/command metadata is projected by gxserver now; kept as the local-state derivation
pub(crate) fn gpui_sidebar_command_write_active_project_id(write: &GpuiSidebarCommandMetadataWrite) -> &str {
    match write {
        GpuiSidebarCommandMetadataWrite::Save {
            active_project_id, ..
        }
        | GpuiSidebarCommandMetadataWrite::Delete {
            active_project_id, ..
        }
        | GpuiSidebarCommandMetadataWrite::SyncOrder {
            active_project_id, ..
        } => active_project_id,
    }
}

#[allow(dead_code)] // no caller: sidebar agent/command metadata is projected by gxserver now; kept as the local-state derivation
pub(crate) fn gpui_next_sidebar_command_metadata_state(
    stored_commands: Vec<GpuiStoredSidebarCommand>,
    stored_order: Vec<String>,
    deleted_default_command_ids: Vec<String>,
    write: GpuiSidebarCommandMetadataWrite,
) -> Result<
    (
        Vec<GpuiStoredSidebarCommand>,
        Vec<String>,
        Vec<String>,
        Option<serde_json::Value>,
    ),
    String,
> {
    match write {
        GpuiSidebarCommandMetadataWrite::Save {
            action_type,
            close_terminal_on_exit,
            command,
            command_id,
            icon,
            name,
            play_completion_sound,
            show_on_project_row,
            url,
            ..
        } => {
            let current_command_ids = gpui_sidebar_button_ids(
                &gpui_sidebar_command_buttons_from_state(
                    &stored_commands,
                    &stored_order,
                    &deleted_default_command_ids,
                ),
                "commandId",
            );
            let next_command_id = command_id.unwrap_or_else(gpui_create_custom_sidebar_command_id);
            let next_command = GpuiStoredSidebarCommand {
                action_type,
                close_terminal_on_exit: action_type == "terminal" && close_terminal_on_exit,
                command: (action_type == "terminal").then_some(command).flatten(),
                command_id: next_command_id.clone(),
                icon,
                is_default: gpui_is_default_sidebar_command_id(&next_command_id),
                name,
                play_completion_sound: action_type == "terminal" && play_completion_sound,
                show_on_project_row,
                url: (action_type == "browser").then_some(url).flatten(),
            };
            gpui_reject_duplicate_sidebar_command_title(
                &next_command,
                &stored_commands,
                &stored_order,
                &deleted_default_command_ids,
            )?;
            let existing_index = stored_commands
                .iter()
                .position(|command| command.command_id == next_command_id);
            let mut next_commands = stored_commands.clone();
            if let Some(existing_index) = existing_index {
                next_commands[existing_index] = next_command;
            } else {
                next_commands.push(next_command);
            }
            let next_order = if existing_index.is_some()
                || stored_order
                    .iter()
                    .any(|candidate| candidate == &next_command_id)
                || gpui_is_default_sidebar_command_id(&next_command_id)
            {
                stored_order
            } else if current_command_ids
                .iter()
                .any(|candidate| candidate == &next_command_id)
            {
                current_command_ids
            } else {
                let mut next_order = current_command_ids;
                next_order.push(next_command_id.clone());
                next_order
            };
            let next_deleted_default_ids = if gpui_is_default_sidebar_command_id(&next_command_id) {
                deleted_default_command_ids
                    .into_iter()
                    .filter(|candidate| candidate != &next_command_id)
                    .collect::<Vec<_>>()
            } else {
                deleted_default_command_ids
            };
            Ok((next_commands, next_order, next_deleted_default_ids, None))
        }
        GpuiSidebarCommandMetadataWrite::Delete { command_id, .. } => {
            let next_commands = stored_commands
                .into_iter()
                .filter(|command| command.command_id != command_id)
                .collect::<Vec<_>>();
            let next_order = stored_order
                .into_iter()
                .filter(|candidate| candidate != &command_id)
                .collect::<Vec<_>>();
            let mut next_deleted_default_ids = deleted_default_command_ids;
            if gpui_is_default_sidebar_command_id(&command_id)
                && !next_deleted_default_ids
                    .iter()
                    .any(|candidate| candidate == &command_id)
            {
                next_deleted_default_ids.push(command_id);
            }
            Ok((next_commands, next_order, next_deleted_default_ids, None))
        }
        GpuiSidebarCommandMetadataWrite::SyncOrder {
            command_ids,
            request_id,
            ..
        } => {
            let current_command_ids = gpui_sidebar_button_ids(
                &gpui_sidebar_command_buttons_from_state(
                    &stored_commands,
                    &stored_order,
                    &deleted_default_command_ids,
                ),
                "commandId",
            );
            let mut next_order = command_ids
                .into_iter()
                .filter(|command_id| {
                    current_command_ids
                        .iter()
                        .any(|candidate| candidate == command_id)
                })
                .collect::<Vec<_>>();
            for command_id in current_command_ids {
                if !next_order.iter().any(|candidate| candidate == &command_id) {
                    next_order.push(command_id);
                }
            }
            let item_ids = gpui_sidebar_button_ids(
                &gpui_sidebar_command_buttons_from_state(
                    &stored_commands,
                    &next_order,
                    &deleted_default_command_ids,
                ),
                "commandId",
            );
            Ok((
                stored_commands,
                next_order,
                deleted_default_command_ids,
                Some(gpui_sidebar_order_sync_result_message(
                    "command",
                    &request_id,
                    "success",
                    item_ids,
                )),
            ))
        }
    }
}

pub(crate) fn gpui_reject_duplicate_sidebar_command_title(
    next_command: &GpuiStoredSidebarCommand,
    stored_commands: &[GpuiStoredSidebarCommand],
    stored_order: &[String],
    deleted_default_command_ids: &[String],
) -> Result<(), String> {
    let next_title_key = gpui_sidebar_command_title_key(
        &next_command.name,
        next_command.command.as_deref(),
        next_command.url.as_deref(),
    );
    let buttons = gpui_sidebar_command_buttons_from_state(
        stored_commands,
        stored_order,
        deleted_default_command_ids,
    );
    let duplicate = buttons
        .as_array()
        .into_iter()
        .flatten()
        .filter_map(serde_json::Value::as_object)
        .any(|candidate| {
            gpui_trimmed_json_string_field(candidate, "commandId")
                .map(|command_id| command_id != next_command.command_id)
                .unwrap_or(false)
                && gpui_sidebar_command_title_key(
                    candidate
                        .get("name")
                        .and_then(serde_json::Value::as_str)
                        .unwrap_or_default(),
                    candidate.get("command").and_then(serde_json::Value::as_str),
                    candidate.get("url").and_then(serde_json::Value::as_str),
                ) == next_title_key
        });
    if duplicate {
        Err(GPUI_SIDEBAR_DUPLICATE_ACTION_TITLE_ERROR.to_string())
    } else {
        Ok(())
    }
}

pub(crate) fn gpui_sidebar_command_title_key(name: &str, command: Option<&str>, url: Option<&str>) -> String {
    let title = gpui_normalized_sidebar_command_title(Some(name))
        .or_else(|| {
            gpui_normalized_sidebar_command_title(command.or(url))
                .map(gpui_sidebar_command_short_title)
        })
        .unwrap_or_default();
    title.to_lowercase()
}

pub(crate) fn gpui_sidebar_command_short_title(value: String) -> String {
    value.chars().take(20).collect()
}

pub(crate) fn gpui_normalized_sidebar_command_title(value: Option<&str>) -> Option<String> {
    let normalized = value?.split_whitespace().collect::<Vec<_>>().join(" ");
    (!normalized.is_empty()).then_some(normalized)
}

pub(crate) fn gpui_sidebar_button_ids(buttons: &serde_json::Value, id_key: &str) -> Vec<String> {
    buttons
        .as_array()
        .into_iter()
        .flatten()
        .filter_map(serde_json::Value::as_object)
        .filter_map(|button| gpui_trimmed_json_string_field(button, id_key))
        .map(str::to_string)
        .collect()
}

pub(crate) fn gpui_sidebar_order_sync_result_message(
    kind: &'static str,
    request_id: &str,
    status: &'static str,
    item_ids: Vec<String>,
) -> serde_json::Value {
    serde_json::json!({
        "itemIds": item_ids,
        "kind": kind,
        "requestId": request_id,
        "status": status,
        "type": "sidebarOrderSyncResult",
    })
}

pub(crate) fn gpui_is_sidebar_agent_visible(agents: &[GpuiStoredSidebarAgent], agent_id: &str) -> bool {
    agents
        .iter()
        .find(|agent| agent.agent_id == agent_id)
        .map(|agent| !agent.hidden)
        .unwrap_or(true)
}

pub(crate) fn gpui_default_sidebar_agent_by_id(agent_id: &str) -> Option<&'static GpuiDefaultSidebarAgent> {
    GPUI_DEFAULT_SIDEBAR_AGENTS
        .iter()
        .find(|agent| agent.agent_id == agent_id)
}

pub(crate) fn gpui_default_sidebar_agent_by_icon(icon: &str) -> Option<&'static GpuiDefaultSidebarAgent> {
    if icon == "browser" {
        return None;
    }
    GPUI_DEFAULT_SIDEBAR_AGENTS
        .iter()
        .find(|agent| agent.icon == icon)
}

pub(crate) fn gpui_create_custom_sidebar_agent_id(name: &str) -> String {
    let slug = name
        .trim()
        .to_lowercase()
        .chars()
        .map(|ch| if ch.is_ascii_alphanumeric() { ch } else { '-' })
        .collect::<String>()
        .split('-')
        .filter(|segment| !segment.is_empty())
        .collect::<Vec<_>>()
        .join("-");
    let slug = if slug.is_empty() {
        "agent".to_string()
    } else {
        slug.chars().take(24).collect::<String>()
    };
    format!("custom-{slug}-{}", gpui_generated_sidebar_metadata_suffix())
}

pub(crate) fn gpui_create_custom_sidebar_command_id() -> String {
    format!("custom-{}", gpui_generated_sidebar_metadata_suffix())
}

pub(crate) fn gpui_generated_sidebar_metadata_suffix() -> String {
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_nanos())
        .unwrap_or_default();
    format!(
        "{}-{}",
        gpui_base36(nanos),
        gpui_base36(std::process::id() as u128)
    )
}

pub(crate) fn gpui_base36(mut value: u128) -> String {
    const DIGITS: &[u8; 36] = b"0123456789abcdefghijklmnopqrstuvwxyz";
    if value == 0 {
        return "0".to_string();
    }
    let mut output = Vec::new();
    while value > 0 {
        let digit = (value % 36) as usize;
        output.push(DIGITS[digit] as char);
        value /= 36;
    }
    output.iter().rev().collect()
}

pub(crate) fn gpui_stored_sidebar_agents_value(agents: &[GpuiStoredSidebarAgent]) -> serde_json::Value {
    serde_json::Value::Array(
        agents
            .iter()
            .map(|agent| {
                let mut item = serde_json::Map::new();
                if let Some(accept_all_mode) = agent.accept_all_mode.as_ref() {
                    item.insert(
                        "acceptAllMode".to_string(),
                        serde_json::Value::String(accept_all_mode.clone()),
                    );
                }
                item.insert(
                    "agentId".to_string(),
                    serde_json::Value::String(agent.agent_id.clone()),
                );
                item.insert(
                    "command".to_string(),
                    serde_json::Value::String(agent.command.clone()),
                );
                item.insert("hidden".to_string(), serde_json::Value::Bool(agent.hidden));
                if let Some(icon) = agent.icon.as_ref() {
                    item.insert("icon".to_string(), serde_json::Value::String(icon.clone()));
                }
                item.insert(
                    "isDefault".to_string(),
                    serde_json::Value::Bool(gpui_is_default_sidebar_agent_id(&agent.agent_id)),
                );
                item.insert(
                    "name".to_string(),
                    serde_json::Value::String(agent.name.clone()),
                );
                serde_json::Value::Object(item)
            })
            .collect(),
    )
}

pub(crate) fn gpui_stored_sidebar_commands_value(commands: &[GpuiStoredSidebarCommand]) -> serde_json::Value {
    serde_json::Value::Array(
        commands
            .iter()
            .map(|command| {
                let mut item = serde_json::Map::new();
                item.insert(
                    "actionType".to_string(),
                    serde_json::Value::String(command.action_type.to_string()),
                );
                item.insert(
                    "closeTerminalOnExit".to_string(),
                    serde_json::Value::Bool(command.close_terminal_on_exit),
                );
                if let Some(command_text) = command.command.as_ref() {
                    item.insert(
                        "command".to_string(),
                        serde_json::Value::String(command_text.clone()),
                    );
                }
                item.insert(
                    "commandId".to_string(),
                    serde_json::Value::String(command.command_id.clone()),
                );
                if let Some(icon) = command.icon.as_ref() {
                    item.insert("icon".to_string(), serde_json::Value::String(icon.clone()));
                }
                item.insert(
                    "isDefault".to_string(),
                    serde_json::Value::Bool(command.is_default),
                );
                item.insert(
                    "name".to_string(),
                    serde_json::Value::String(command.name.clone()),
                );
                item.insert(
                    "playCompletionSound".to_string(),
                    serde_json::Value::Bool(command.play_completion_sound),
                );
                item.insert(
                    "showOnProjectRow".to_string(),
                    serde_json::Value::Bool(command.show_on_project_row),
                );
                if let Some(url) = command.url.as_ref() {
                    item.insert("url".to_string(), serde_json::Value::String(url.clone()));
                }
                serde_json::Value::Object(item)
            })
            .collect(),
    )
}

pub(crate) fn gpui_string_array_value(items: &[String]) -> serde_json::Value {
    serde_json::Value::Array(
        items
            .iter()
            .map(|item| serde_json::Value::String(item.clone()))
            .collect(),
    )
}

pub(crate) fn gpui_order_json_buttons(
    buttons: Vec<(String, serde_json::Value)>,
    stored_order: &[String],
    id_key: &str,
) -> serde_json::Value {
    let mut ordered_buttons = Vec::new();
    let mut used_ids = HashSet::new();
    for item_id in stored_order {
        if let Some((_, button)) = buttons.iter().find(|(button_id, _)| button_id == item_id) {
            ordered_buttons.push(button.clone());
            used_ids.insert(item_id.clone());
        }
    }
    for (button_id, button) in buttons {
        let actual_id = button
            .get(id_key)
            .and_then(serde_json::Value::as_str)
            .unwrap_or(button_id.as_str());
        if used_ids.insert(actual_id.to_string()) {
            ordered_buttons.push(button);
        }
    }
    serde_json::Value::Array(ordered_buttons)
}

pub(crate) fn gpui_json_array_field_is_nonempty(
    object: &serde_json::Map<String, serde_json::Value>,
    key: &str,
) -> bool {
    object
        .get(key)
        .and_then(serde_json::Value::as_array)
        .map(|items| !items.is_empty())
        .unwrap_or(false)
}

pub(crate) fn gpui_is_default_sidebar_agent_id(agent_id: &str) -> bool {
    GPUI_DEFAULT_SIDEBAR_AGENTS
        .iter()
        .any(|agent| agent.agent_id == agent_id)
}

pub(crate) fn gpui_default_sidebar_agent_name(agent_id: &str, stored_name: &str) -> String {
    let default_name = GPUI_DEFAULT_SIDEBAR_AGENTS
        .iter()
        .find(|agent| agent.agent_id == agent_id)
        .map(|agent| agent.name);
    let Some(default_name) = default_name else {
        return stored_name.to_string();
    };
    let normalized = stored_name.trim().to_ascii_lowercase();
    if (agent_id == "codex" && normalized == "codex cli")
        || (agent_id == "claude" && normalized == "claude code")
        || (agent_id == "cursor" && normalized == "cursor")
        || (agent_id == "pi" && normalized == "pi")
    {
        default_name.to_string()
    } else {
        stored_name.to_string()
    }
}

pub(crate) fn gpui_is_default_sidebar_command_id(command_id: &str) -> bool {
    GPUI_DEFAULT_SIDEBAR_COMMANDS
        .iter()
        .any(|command| command.command_id == command_id)
}

pub(crate) fn gpui_strict_sidebar_agent_icon(candidate: &str) -> Option<&str> {
    if candidate == "browser" {
        return Some(candidate);
    }
    GPUI_DEFAULT_SIDEBAR_AGENTS
        .iter()
        .any(|agent| agent.icon == candidate)
        .then_some(candidate)
}

/*
CDXC:GlobalActions 2026-08-01-16:00:
Action icon slugs are camelCase ids shared with the TypeScript surfaces, while
the bundled assets are kebab-case files under `titlebar/`. Convert rather than
hand-maintaining a 59-entry table that would silently rot whenever an icon is
added on the shared side. `terminal` is the one id whose asset is not its
kebab-case name — the bundle ships `terminal-2.svg`, which the tab strip's own
Terminal View button already points at.

Actions saved without an icon fall back to the same default the sidebar uses, so
an icon-less Global Action still renders a button rather than an empty slot.
*/
pub(crate) fn gpui_sidebar_command_icon_asset_path(icon: Option<&str>) -> gpui::SharedString {
    let icon = icon.unwrap_or(GPUI_DEFAULT_SIDEBAR_COMMAND_ICON);
    if icon == "terminal" {
        return gpui::SharedString::new_static("titlebar/terminal-2.svg");
    }
    let mut asset = String::with_capacity(icon.len() + 1);
    for character in icon.chars() {
        if character.is_ascii_uppercase() {
            asset.push('-');
            asset.push(character.to_ascii_lowercase());
        } else {
            asset.push(character);
        }
    }
    gpui::SharedString::from(format!("titlebar/{asset}.svg"))
}

pub(crate) fn gpui_sidebar_command_icon(candidate: &str) -> Option<&str> {
    GPUI_SIDEBAR_COMMAND_ICON_IDS
        .iter()
        .any(|icon| *icon == candidate)
        .then_some(candidate)
}

pub(crate) fn gpui_sidebar_command_session_indicators_from_command_pane_sources(
    commands: &serde_json::Value,
    command_pane_sessions: &serde_json::Value,
) -> serde_json::Value {
    /*
    CDXC:GPUICommandSessionHud 2026-06-27-08:45:
    Command-session HUD matching consumes the same external bridge shape emitted by command-pane sources: `sessionId` must be a string accepted by the canonical `G{u64}` parser and `status` must be a known lifecycle value before commandId/title matching runs. Stale legacy numeric, lowercase, malformed, non-string, or invalid-status rows must be invisible to matching so they cannot mask a live canonical row, and emitted indicators must continue to omit command text, paths, URLs, run ids, status files, terminal output, and unknown raw fields.
    */
    let Some(commands) = commands.as_array() else {
        return serde_json::Value::Array(Vec::new());
    };
    let sessions = command_pane_sessions
        .as_array()
        .map(Vec::as_slice)
        .unwrap_or(&[]);
    serde_json::Value::Array(
        commands
            .iter()
            .filter_map(|command| {
                if command
                    .get("actionType")
                    .and_then(serde_json::Value::as_str)
                    != Some("terminal")
                {
                    return None;
                }
                let command_id = command
                    .get("commandId")
                    .and_then(serde_json::Value::as_str)
                    .and_then(gpui_command_pane_sidebar_indicator_text)?;
                let command_title = gpui_sidebar_command_indicator_title_from_command(command)?;
                let command_title_key = gpui_command_pane_sidebar_indicator_key(&command_title);
                if command_title_key.is_empty() {
                    return None;
                }
                let mapped_session = sessions.iter().find(|session| {
                    gpui_sidebar_command_indicator_matchable_session_source(session)
                        && session
                            .get("commandId")
                            .and_then(serde_json::Value::as_str)
                            .is_some_and(|candidate| candidate == command_id.as_str())
                        && gpui_command_pane_sidebar_indicator_key(
                            session
                                .get("title")
                                .and_then(serde_json::Value::as_str)
                                .unwrap_or_default(),
                        ) == command_title_key
                });
                let session = mapped_session.or_else(|| {
                    sessions.iter().find(|session| {
                        gpui_sidebar_command_indicator_matchable_session_source(session)
                            && gpui_command_pane_sidebar_indicator_key(
                                session
                                    .get("title")
                                    .and_then(serde_json::Value::as_str)
                                    .unwrap_or_default(),
                            ) == command_title_key
                    })
                })?;
                let session_id = gpui_sidebar_command_indicator_session_id(session)?;
                let status = gpui_sidebar_command_indicator_status(session)?;
                let title = session
                    .get("title")
                    .and_then(serde_json::Value::as_str)
                    .and_then(gpui_command_pane_sidebar_indicator_text);
                let mut indicator = serde_json::json!({
                    "commandId": command_id,
                    "isActive": session
                        .get("isActive")
                        .and_then(serde_json::Value::as_bool)
                        .unwrap_or(false),
                    "sessionId": session_id,
                    "status": status,
                });
                if let Some(title) = title {
                    indicator["title"] = serde_json::json!(title);
                }
                for key in [
                    "delayedSendDeadlineAt",
                    "delayedSendRemainingLabel",
                    "closeAfterDoneDeadlineAt",
                    "closeAfterDoneRemainingLabel",
                ] {
                    if let Some(value) = session
                        .get(key)
                        .and_then(serde_json::Value::as_str)
                        .and_then(gpui_command_pane_sidebar_indicator_text)
                    {
                        indicator[key] = serde_json::json!(value);
                    }
                }
                for key in ["delayedSendRemainingMs", "closeAfterDoneRemainingMs"] {
                    if let Some(value) = session.get(key).and_then(serde_json::Value::as_u64) {
                        indicator[key] = serde_json::json!(value);
                    }
                }
                if session
                    .get("closeAfterDone")
                    .and_then(serde_json::Value::as_bool)
                    == Some(true)
                {
                    indicator["closeAfterDone"] = serde_json::json!(true);
                }
                Some(indicator)
            })
            .collect(),
    )
}

pub(crate) fn gpui_sidebar_command_indicator_matchable_session_source(session: &serde_json::Value) -> bool {
    gpui_sidebar_command_indicator_session_id(session).is_some()
        && gpui_sidebar_command_indicator_status(session).is_some()
}

pub(crate) fn gpui_sidebar_command_indicator_session_id(session: &serde_json::Value) -> Option<&str> {
    let session_id = session.get("sessionId")?.as_str()?;
    gpui_command_session_id_from_external_id(session_id)?;
    Some(session_id)
}

pub(crate) fn gpui_sidebar_command_indicator_status(session: &serde_json::Value) -> Option<&'static str> {
    match session.get("status").and_then(serde_json::Value::as_str) {
        Some("idle") => Some("idle"),
        Some("running") => Some("running"),
        Some("error") => Some("error"),
        _ => None,
    }
}

pub(crate) fn gpui_sidebar_command_indicator_title_from_command(
    command: &serde_json::Value,
) -> Option<String> {
    if let Some(name) = command
        .get("name")
        .and_then(serde_json::Value::as_str)
        .and_then(gpui_command_pane_sidebar_indicator_text)
    {
        return Some(name);
    }
    let command_text = command
        .get("command")
        .and_then(serde_json::Value::as_str)
        .unwrap_or_default()
        .trim()
        .chars()
        .take(20)
        .collect::<String>();
    gpui_command_pane_sidebar_indicator_text(&command_text)
}

pub(crate) fn gpui_command_pane_sidebar_indicator_key(value: &str) -> String {
    gpui_command_pane_sidebar_indicator_text(value)
        .map(|text| text.to_lowercase())
        .unwrap_or_default()
}

pub(crate) fn gpui_app_modal_sidebar_state_message(
    latest_snapshot: Option<&GpuiProjectSnapshot>,
) -> serde_json::Value {
    gpui_app_modal_sidebar_state_message_for_active_project_id(
        gpui_active_project_id_from_snapshot(latest_snapshot),
    )
}

pub(crate) fn gpui_app_modal_sidebar_state_message_for_active_project_id(
    active_project_id: Option<&str>,
) -> serde_json::Value {
    gpui_app_modal_sidebar_state_message_with_portless_state_for_active_project_id(
        None,
        active_project_id,
    )
}

pub(crate) fn gpui_app_modal_sidebar_state_message_with_portless_state_for_active_project_id(
    portless_state: Option<serde_json::Value>,
    active_project_id: Option<&str>,
) -> serde_json::Value {
    let settings_snapshot = shared_settings::shared_sidebar_settings_snapshot();
    gpui_app_modal_sidebar_state_message_from_settings_snapshot_and_portless_state_for_active_project_id(
        &settings_snapshot,
        portless_state,
        active_project_id,
    )
}

pub(crate) fn gpui_app_modal_sidebar_state_message_from_settings_snapshot(
    settings_snapshot: &shared_settings::SharedSidebarSettingsSnapshot,
    latest_snapshot: Option<&GpuiProjectSnapshot>,
) -> serde_json::Value {
    gpui_app_modal_sidebar_state_message_from_settings_snapshot_and_portless_state(
        settings_snapshot,
        None,
        latest_snapshot,
    )
}

pub(crate) fn gpui_app_modal_sidebar_state_message_from_settings_snapshot_and_portless_state(
    settings_snapshot: &shared_settings::SharedSidebarSettingsSnapshot,
    portless_state_override: Option<serde_json::Value>,
    latest_snapshot: Option<&GpuiProjectSnapshot>,
) -> serde_json::Value {
    gpui_app_modal_sidebar_state_message_from_settings_snapshot_and_portless_state_for_active_project_id(
        settings_snapshot,
        portless_state_override,
        gpui_active_project_id_from_snapshot(latest_snapshot),
    )
}

pub(crate) fn gpui_app_modal_sidebar_state_message_from_settings_snapshot_and_portless_state_for_active_project_id(
    settings_snapshot: &shared_settings::SharedSidebarSettingsSnapshot,
    portless_state_override: Option<serde_json::Value>,
    active_project_id: Option<&str>,
) -> serde_json::Value {
    /*
    CDXC:GPUISettingsModalHost 2026-06-24-11:14:
    The GPUI Settings modal host hydrates from the shared settings service snapshot, not a second ad hoc file read. The payload remains intentionally minimal for current GPUI parity, but it carries the saved settings object and service revision for both open-time hydration and post-save `sidebarState` refreshes.

    CDXC:GPUISettingsStatusBridge 2026-06-24-11:40:
    Settings hydration must no longer send empty launcher/action chrome. Mirror the shared SidebarApp agent and command button contract from real gxserver project metadata, include gxserver Portless health when a short localhost health read is available, and hydrate project settings rows only from real gxserver project/presentation metadata; do not invent project paths or custom buttons for the modal.

    CDXC:GPUISettingsPortlessBridge 2026-06-24-11:48:
    After a Portless Settings RPC succeeds, prefer the canonical gxserver update result for `hud.portless` so status, setup state, and assigned-domain presentation refresh in the open Settings modal without a second daemon read. Generic hydration may still use the short health probe and keeps native admin unavailable in GPUI.

    CDXC:GPUIRecentProjects 2026-06-24-12:27:
    The Rust app-modal hydrate reads Recent Projects from gxserver's
    `/api/listRecentProjects` contract. Keep the drawer empty when gxserver has
    no explicit parked rows; do not derive recent projects from inactive
    sessions, presentation labels, local paths, command text, or filesystem
    guesses.

    CDXC:GxserverAppUserData 2026-06-24-13:30:
    Hydrate Pinned Prompts and Scratch Pad from gxserver app-user-data every
    time the app-modal host opens or refreshes. Settings, Previous Sessions,
    Running Sessions, and Portless data keep their existing
    stored-vs-transient behavior; these two user-data modal fields must not
    read or write the old GPUI-only product-state file.

    CDXC:GPUISettingsActiveProjectActions 2026-06-24-13:34:
    App-modal Settings action hydration uses the explicit active project id from the latest live sidebar snapshot when one exists, matching the SidebarApp gxserver runtime's project-scoped command owner selection. Quick/projectless, no-snapshot, and no-active-id hydrates keep the existing no-active-project behavior; an unknown explicit id returns default actions instead of silently using another project's custom commands.

    CDXC:SidebarHudContract 2026-06-24-20:34:
    Agent/action HUD rows are read through gxserver's `/api/readSidebarHud` production projection. The app-modal host still reads project settings and Recent Projects from their existing gxserver contracts, but it no longer hand-normalizes custom agent/action metadata from `/api/listProjects`.
    */
    let settings_object = settings_snapshot.object().clone();
    let runtime_settings =
        sidebar_runtime_settings_snapshot_from_shared_settings(settings_snapshot);
    let completion_sound = settings_object
        .get("completionSound")
        .and_then(serde_json::Value::as_str)
        .unwrap_or("arcade")
        .to_string();
    let completion_sound_label = gpui_completion_sound_label(&completion_sound);
    let theme = gpui_app_modal_sidebar_theme_from_settings(&settings_object);
    let settings = serde_json::Value::Object(settings_object);
    let portless_state = GPUI_PORTLESS_APP_INTEGRATION_ENABLED
        .then(|| portless_state_override.or_else(gpui_sidebar_portless_state))
        .flatten();
    let domain_projects = gpui_gxserver_domain_projects(Duration::from_secs(2));
    let sidebar_hud =
        gpui_sidebar_hud_from_gxserver(Duration::from_secs(2), active_project_id).ok();
    let agents = sidebar_hud
        .as_ref()
        .map(|hud| hud.agents.clone())
        .unwrap_or_else(|| serde_json::Value::Array(Vec::new()));
    let commands = sidebar_hud
        .as_ref()
        .map(|hud| hud.commands.clone())
        .unwrap_or_else(|| serde_json::Value::Array(Vec::new()));
    let global_commands = sidebar_hud
        .as_ref()
        .map(|hud| hud.global_commands.clone())
        .unwrap_or_else(|| serde_json::Value::Array(Vec::new()));
    let project_settings_projects =
        gpui_project_settings_projects_from_domain_projects_or_presentation(&domain_projects);
    let recent_projects = gpui_gxserver_recent_projects(Duration::from_secs(2));
    let product_state = gpui_read_gxserver_app_user_data(Duration::from_secs(2));
    let pinned_prompts = product_state
        .pinned_prompts
        .iter()
        .map(gpui_pinned_prompt_value)
        .collect::<Vec<_>>();

    let mut message = serde_json::json!({
        "groups": [],
        "hud": {
            "activeSessionsSortMode": "lastActivity",
            "agentManagerZoomPercent": 100,
            "agents": agents,
            // GPUI uses the same shared picker and resolved Ghostex icons
            // store as the Swift app. Other platforms keep the section hidden
            // until they have an equivalent native app-icon implementation.
            "appIconPickerUnavailable": !cfg!(target_os = "macos"),
            "commands": commands,
            "globalCommands": global_commands,
            "commandSessionIndicators": [],
            "completionBellEnabled": false,
            "completionSound": completion_sound,
            "completionSoundLabel": completion_sound_label,
            "debuggingMode": runtime_settings.debugging_mode,
            "git": {
                "additions": 0,
                "aheadCount": 0,
                "behindCount": 0,
                "branch": null,
                "confirmSuggestedCommit": false,
                "deletions": 0,
                "files": [],
                "generateCommitBody": true,
                "hasCheckedGitHubRemote": false,
                "hasGitHubCli": false,
                "hasGitHubRemote": false,
                "hasOriginRemote": false,
                "hasUpstream": false,
                "hasWorkingTreeChanges": false,
                "isBusy": false,
                "isRepo": false,
                "isWorktree": false,
                "pr": null,
                "primaryAction": "commit",
            },
            "highlightedVisibleCount": 1,
            "isFocusModeActive": false,
            "pendingAgentIds": [],
            "projectSettingsProjects": project_settings_projects,
            "recentProjects": recent_projects,
            "settings": settings,
            "createSessionOnSidebarDoubleClick": false,
            "renameSessionOnDoubleClick": false,
            "showCloseButtonOnSessionCards": true,
            "theme": theme,
            "viewMode": "grid",
            "visibleCount": 1,
            "visibleSlotLabels": [],
        },
        "pinnedPrompts": pinned_prompts,
        "previousSessions": [],
        "revision": settings_snapshot.revision(),
        "scratchPadContent": product_state.scratch_pad_content,
        "type": "hydrate",
    });
    if let Some(portless_state) = portless_state {
        message["hud"]["portless"] = portless_state;
    }
    message
}

pub(crate) fn gpui_app_modal_sidebar_theme_from_settings(
    settings: &serde_json::Map<String, serde_json::Value>,
) -> &'static str {
    match settings
        .get("sidebarTheme")
        .and_then(serde_json::Value::as_str)
    {
        Some("dark-1") => "dark-1",
        Some("dark-2") => "dark-2",
        Some("plain-light") => "plain-light",
        Some("dark-green") => "dark-green",
        Some("dark-blue") => "dark-blue",
        Some("dark-red") => "dark-red",
        Some("dark-pink") => "dark-pink",
        Some("dark-orange") => "dark-orange",
        Some("light-blue") => "light-blue",
        Some("light-green") => "light-green",
        Some("light-pink") => "light-pink",
        Some("light-orange") => "light-orange",
        _ => "plain-dark",
    }
}

pub(crate) fn gpui_session_chat_theme_from_settings(
    settings: &serde_json::Map<String, serde_json::Value>,
) -> &'static str {
    match settings
        .get("sessionChatTheme")
        .and_then(serde_json::Value::as_str)
    {
        Some("light") => "light",
        _ => "dark",
    }
}

pub(crate) fn gpui_session_chat_font_family_from_settings(
    settings: &serde_json::Map<String, serde_json::Value>,
) -> String {
    settings
        .get("sessionChatFontFamily")
        .and_then(serde_json::Value::as_str)
        .unwrap_or_default()
        .trim()
        .to_string()
}

pub(crate) fn gpui_session_chat_transcript_width_percent_from_settings(
    settings: &serde_json::Map<String, serde_json::Value>,
) -> u8 {
    const MIN_PERCENT: f64 = 50.0;
    const MAX_PERCENT: f64 = 100.0;
    const STEP_PERCENT: f64 = 5.0;
    const DEFAULT_PERCENT: f64 = 75.0;

    let value = settings
        .get("sessionChatTranscriptWidthPercent")
        .and_then(serde_json::Value::as_f64)
        .unwrap_or(DEFAULT_PERCENT)
        .clamp(MIN_PERCENT, MAX_PERCENT);
    ((value / STEP_PERCENT).round() * STEP_PERCENT) as u8
}

pub(crate) fn gpui_session_chat_verbose_mode_from_settings(
    settings: &serde_json::Map<String, serde_json::Value>,
) -> bool {
    settings
        .get("sessionChatVerboseMode")
        .and_then(serde_json::Value::as_bool)
        .unwrap_or(false)
}

pub(crate) fn current_sidebar_max_width(window: &Window) -> f32 {
    let max_for_window =
        window.bounds().size.width.as_f32() - SIDEBAR_DIVIDER_WIDTH - WORKSPACE_MIN_WIDTH;
    SIDEBAR_MAX_WIDTH.min(max_for_window).max(SIDEBAR_MIN_WIDTH)
}

pub(crate) fn clamp_sidebar_width(width: f32, max_width: f32) -> f32 {
    /*
    CDXC:GPUISidebarDividerColor 2026-07-22:
    Sidebar and divider are separately painted normal-layout siblings. Keep
    their shared boundary on a whole logical point so 1x and Retina backing
    stores cannot expose a half-covered workspace-colored seam between them.
    */
    width
        .round()
        .clamp(SIDEBAR_MIN_WIDTH, max_width.floor().max(SIDEBAR_MIN_WIDTH))
}

pub(crate) fn read_sidebar_width_setting() -> Option<f32> {
    read_json_number_field(&native_chrome_settings_path(), "sidebarWidth")
}

pub(crate) fn read_sidebar_default_width_setting() -> Option<f32> {
    shared_settings::shared_sidebar_settings_snapshot()
        .sidebar_default_width_px()
        .map(|width| width.clamp(SIDEBAR_MIN_WIDTH, SIDEBAR_MAX_WIDTH))
}

pub(crate) fn gpui_sidebar_side_from_shared_settings(
    settings: &shared_settings::SharedSidebarSettingsSnapshot,
) -> GpuiSidebarSide {
    /*
    CDXC:GPUISidebarSide 2026-06-26-23:35:
    GPUI reads the typed shared `sidebarSide` snapshot as the startup placement source. Missing or malformed values are normalized by the shared settings boundary to native's left-side default without adding a second durable setting.
    */
    match settings.sidebar_side() {
        shared_settings::SharedSidebarSide::Left => GpuiSidebarSide::Left,
        shared_settings::SharedSidebarSide::Right => GpuiSidebarSide::Right,
    }
}

pub(crate) fn write_gpui_sidebar_side_to_shared_settings(side: GpuiSidebarSide) {
    /*
    CDXC:GPUISidebarSide 2026-06-26-23:35:
    GPUI persists Move Sidebar through the typed shared settings writer so only `sidebarSide` changes and sidebar width, collapsed state, and unrelated settings remain untouched.
    */
    let shared_side = match side {
        GpuiSidebarSide::Left => shared_settings::SharedSidebarSide::Left,
        GpuiSidebarSide::Right => shared_settings::SharedSidebarSide::Right,
    };
    let _ = shared_settings::write_shared_sidebar_side(shared_side);
}

pub(crate) fn persist_sidebar_width_setting(width: f32) {
    /*
    CDXC:GPUIPrivacyAudit 2026-06-23-13:18:
    The GPUI settings write is limited to merging the finite numeric sidebarWidth into the existing native settings JSON object. Do not add project/session/browser/runtime fields here, and do not persist paths, names, command text, URLs, page titles, terminal content, tokens, cookies, secrets, stdout/stderr, or raw CEF/sidebar payloads from GPUI state.
    */
    let path = native_chrome_settings_path();
    let mut settings = read_json_object(&path).unwrap_or_default();
    settings.insert(
        "sidebarWidth".to_string(),
        serde_json::Value::Number(
            serde_json::Number::from_f64(width as f64)
                .expect("clamped sidebar width should be finite"),
        ),
    );

    if let Some(parent) = path.parent() {
        let _ = fs::create_dir_all(parent);
    }
    if let Ok(data) = serde_json::to_vec_pretty(&serde_json::Value::Object(settings)) {
        let _ = fs::write(path, data);
    }
}

pub(crate) fn read_json_number_field(path: &Path, key: &str) -> Option<f32> {
    let object = read_json_object(path)?;
    json_value_to_f32(object.get(key)?)
}

pub(crate) fn read_json_object(path: &Path) -> Option<serde_json::Map<String, serde_json::Value>> {
    let text = fs::read_to_string(path).ok()?;
    match serde_json::from_str::<serde_json::Value>(&text).ok()? {
        serde_json::Value::Object(object) => Some(object),
        _ => None,
    }
}

pub(crate) fn json_value_to_f32(value: &serde_json::Value) -> Option<f32> {
    let number = match value {
        serde_json::Value::Number(number) => number.as_f64()?,
        serde_json::Value::String(text) => text.parse::<f64>().ok()?,
        _ => return None,
    };
    number.is_finite().then_some(number as f32)
}

pub(crate) fn project_scoped_workarea_availability_from_latest_sidebar_snapshot(
    latest_snapshot: Option<&GpuiProjectSnapshot>,
    fallback_availability: ProjectScopedWorkareaAvailability,
) -> ProjectScopedWorkareaAvailability {
    latest_snapshot.map_or(fallback_availability, |snapshot| {
        snapshot.titlebar_availability()
    })
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct GpuiSidebarNativeProjectPathActionMessage {
    pub(crate) action: GpuiSidebarNativeProjectPathAction,
    pub(crate) file_path: Option<String>,
    pub(crate) preferred_interface: GpuiPreferredAgentInterface,
    pub(crate) project_id: String,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum GpuiSidebarNativeProjectPathAction {
    CopyRecentProjectPath,
    OpenRecentProjectInFinder,
    CopyWorkspaceProjectPath,
    OpenWorkspaceProjectInFinder,
    OpenWorkspaceProjectInIde,
    OpenActiveWorkspaceProjectInFinder,
    OpenActiveWorkspaceProjectInVscode,
    OpenActiveWorkspaceProjectInZed,
    OpenExistingPullRequestInBrowser,
    OpenSidebarGitChangedFileInIde,
    CopyRemoteProjectPath,
    OpenRemoteProjectTerminal,
    OpenRemoteWorkspaceProjectInIde,
    OpenRemoteWorkspaceProjectInVscode,
    OpenRemoteWorkspaceProjectInZed,
    OpenRemoteExistingPullRequestInBrowser,
    OpenRemoteSidebarGitChangedFileInIde,
    OpenRemoteProjectPortsBrowser,
    OpenRemoteSessionTerminal,
    CopyRemoteAttachCommand,
    CopyRemoteResumeCommand,
}

impl GpuiSidebarNativeProjectPathAction {
    pub(crate) fn from_str(value: &str) -> Option<Self> {
        match value {
            "copyRecentProjectPath" => Some(Self::CopyRecentProjectPath),
            "openRecentProjectInFinder" => Some(Self::OpenRecentProjectInFinder),
            "copyWorkspaceProjectPath" => Some(Self::CopyWorkspaceProjectPath),
            "openWorkspaceProjectInFinder" => Some(Self::OpenWorkspaceProjectInFinder),
            "openWorkspaceProjectInIde" => Some(Self::OpenWorkspaceProjectInIde),
            "openActiveWorkspaceProjectInFinder" => Some(Self::OpenActiveWorkspaceProjectInFinder),
            "openActiveWorkspaceProjectInVscode" => Some(Self::OpenActiveWorkspaceProjectInVscode),
            "openActiveWorkspaceProjectInZed" => Some(Self::OpenActiveWorkspaceProjectInZed),
            "openExistingPullRequestInBrowser" => Some(Self::OpenExistingPullRequestInBrowser),
            "openSidebarGitChangedFileInIde" => Some(Self::OpenSidebarGitChangedFileInIde),
            "copyRemoteProjectPath" => Some(Self::CopyRemoteProjectPath),
            "openRemoteProjectTerminal" => Some(Self::OpenRemoteProjectTerminal),
            "openRemoteWorkspaceProjectInIde" => Some(Self::OpenRemoteWorkspaceProjectInIde),
            "openRemoteWorkspaceProjectInVscode" => Some(Self::OpenRemoteWorkspaceProjectInVscode),
            "openRemoteWorkspaceProjectInZed" => Some(Self::OpenRemoteWorkspaceProjectInZed),
            "openRemoteExistingPullRequestInBrowser" => {
                Some(Self::OpenRemoteExistingPullRequestInBrowser)
            }
            "openRemoteSidebarGitChangedFileInIde" => {
                Some(Self::OpenRemoteSidebarGitChangedFileInIde)
            }
            "openRemoteProjectPortsBrowser" => Some(Self::OpenRemoteProjectPortsBrowser),
            "openRemoteSessionTerminal" => Some(Self::OpenRemoteSessionTerminal),
            "copyRemoteAttachCommand" => Some(Self::CopyRemoteAttachCommand),
            "copyRemoteResumeCommand" => Some(Self::CopyRemoteResumeCommand),
            _ => None,
        }
    }

    pub(crate) fn uses_recent_projects(self) -> bool {
        matches!(
            self,
            Self::CopyRecentProjectPath | Self::OpenRecentProjectInFinder
        )
    }

    pub(crate) fn copies_path(self) -> bool {
        matches!(
            self,
            Self::CopyRecentProjectPath | Self::CopyWorkspaceProjectPath
        )
    }

    pub(crate) fn opens_in_ide(self) -> bool {
        matches!(
            self,
            Self::OpenWorkspaceProjectInIde
                | Self::OpenActiveWorkspaceProjectInVscode
                | Self::OpenActiveWorkspaceProjectInZed
        )
    }

    pub(crate) fn requires_file_path(self) -> bool {
        matches!(
            self,
            Self::OpenSidebarGitChangedFileInIde | Self::OpenRemoteSidebarGitChangedFileInIde
        )
    }

    pub(crate) fn is_remote_session_action(self) -> bool {
        matches!(
            self,
            Self::OpenRemoteSessionTerminal
                | Self::CopyRemoteAttachCommand
                | Self::CopyRemoteResumeCommand
        )
    }

    pub(crate) fn is_remote_project_action(self) -> bool {
        matches!(
            self,
            Self::CopyRemoteProjectPath
                | Self::OpenRemoteProjectTerminal
                | Self::OpenRemoteWorkspaceProjectInIde
                | Self::OpenRemoteWorkspaceProjectInVscode
                | Self::OpenRemoteWorkspaceProjectInZed
                | Self::OpenRemoteExistingPullRequestInBrowser
                | Self::OpenRemoteSidebarGitChangedFileInIde
                | Self::OpenRemoteProjectPortsBrowser
        )
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum GpuiSidebarNativeProjectPathActionResult {
    Copied(String),
    Opened,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) enum GpuiStatusIndicatorStatus {
    Attention,
    Working,
    #[default]
    Available,
}

impl GpuiStatusIndicatorStatus {
    pub(crate) fn from_str(value: &str) -> Option<Self> {
        match value {
            "attention" => Some(Self::Attention),
            "working" => Some(Self::Working),
            "available" => Some(Self::Available),
            _ => None,
        }
    }

    pub(crate) fn slug(self) -> &'static str {
        match self {
            Self::Attention => "attention",
            Self::Working => "working",
            Self::Available => "available",
        }
    }
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub(crate) struct GpuiStatusIndicatorSessionState {
    pub(crate) last_active_at: Option<String>,
    pub(crate) order: u64,
    pub(crate) session_id: String,
    pub(crate) status: GpuiStatusIndicatorStatus,
    pub(crate) title: String,
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub(crate) struct GpuiSidebarGlobalActionState {
    pub(crate) command_id: String,
    pub(crate) icon: Option<String>,
    pub(crate) name: String,
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub(crate) struct GpuiSidebarSessionStatusIndicatorsState {
    pub(crate) attention_count: u64,
    pub(crate) available_count: u64,
    pub(crate) hide_menu_bar_indicators: bool,
    pub(crate) projects: Vec<GpuiStatusIndicatorProjectState>,
    pub(crate) working_count: u64,
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub(crate) struct GpuiSidebarPetOverlayState {
    pub(crate) activities: Vec<GpuiPetOverlayActivityState>,
    pub(crate) enabled: bool,
    pub(crate) selected_pet_id: String,
    pub(crate) status_items: Vec<GpuiPetOverlayStatusItemState>,
}

pub(crate) fn gpui_sidebar_session_status_indicators_from_json(
    text: &str,
) -> Result<GpuiSidebarSessionStatusIndicatorsState, ()> {
    /*
    CDXC:GPUIStatusPetOverlay 2026-06-26-04:38:
    Status indicator parser coverage must keep the GPUI bridge fixed and privacy-safe: accept only version/type, enum counts, menu-bar visibility, bounded project/session rows with ids/order/titles, and optional bounded image data URLs for project notification icons. Reject generic action names, paths, external URLs, command text, stdout/stderr, tokens, terminal content, oversized arrays, and unknown keys before app state is updated.

    CDXC:GPUIStatusPetOverlay 2026-06-27-20:11:
    The standalone GPUI floating session indicator was removed. The status
    bridge still feeds the menu bar item, pet badges, and attention
    notifications, but it no longer accepts floating visibility or size fields.
    */
    let value = serde_json::from_str::<serde_json::Value>(text).map_err(|_| ())?;
    let object = value.as_object().ok_or(())?;
    reject_unexpected_gpui_status_keys(
        object,
        &[
            "version",
            "type",
            "attentionCount",
            "availableCount",
            "workingCount",
            "hideMenuBarIndicators",
            "projects",
        ],
    )?;
    if object.get("version").and_then(serde_json::Value::as_u64)
        != Some(GPUI_SIDEBAR_SESSION_STATUS_INDICATORS_MESSAGE_VERSION)
        || object.get("type").and_then(serde_json::Value::as_str)
            != Some(GPUI_SIDEBAR_SESSION_STATUS_INDICATORS_MESSAGE_TYPE)
    {
        return Err(());
    }
    let projects = object
        .get("projects")
        .and_then(serde_json::Value::as_array)
        .filter(|projects| projects.len() <= GPUI_STATUS_INDICATOR_MAX_PROJECTS)
        .ok_or(())?
        .iter()
        .map(gpui_status_indicator_project_from_value)
        .collect::<Result<Vec<_>, _>>()?;
    Ok(GpuiSidebarSessionStatusIndicatorsState {
        attention_count: gpui_status_count_field(object, "attentionCount")?,
        available_count: gpui_status_count_field(object, "availableCount")?,
        hide_menu_bar_indicators: gpui_status_bool_field(object, "hideMenuBarIndicators")?,
        projects,
        working_count: gpui_status_count_field(object, "workingCount")?,
    })
}

/*
CDXC:GlobalActions 2026-08-01-16:00:
The Global Actions bridge accepts only version/type plus a bounded action list of
id, display name, and optional icon slug. Reject command text, URLs, paths, run
state, project ids, and unknown keys before app state is updated, matching the
status-indicator bridge: the tab strip renders a label and an icon, and running
the action goes back through the Action selector by id.
*/
pub(crate) fn gpui_sidebar_global_actions_from_json(
    text: &str,
) -> Result<Vec<GpuiSidebarGlobalActionState>, ()> {
    let value = serde_json::from_str::<serde_json::Value>(text).map_err(|_| ())?;
    let object = value.as_object().ok_or(())?;
    reject_unexpected_gpui_status_keys(object, &["version", "type", "actions"])?;
    if object.get("version").and_then(serde_json::Value::as_u64)
        != Some(GPUI_SIDEBAR_GLOBAL_ACTIONS_MESSAGE_VERSION)
        || object.get("type").and_then(serde_json::Value::as_str)
            != Some(GPUI_SIDEBAR_GLOBAL_ACTIONS_MESSAGE_TYPE)
    {
        return Err(());
    }
    object
        .get("actions")
        .and_then(serde_json::Value::as_array)
        .filter(|actions| actions.len() <= GPUI_TAB_STRIP_MAX_GLOBAL_ACTIONS)
        .ok_or(())?
        .iter()
        .map(gpui_sidebar_global_action_from_value)
        .collect::<Result<Vec<_>, _>>()
}

pub(crate) fn gpui_sidebar_global_action_from_value(
    value: &serde_json::Value,
) -> Result<GpuiSidebarGlobalActionState, ()> {
    let object = value.as_object().ok_or(())?;
    reject_unexpected_gpui_status_keys(object, &["commandId", "icon", "name"])?;
    /*
    A Global Action may carry an icon and an empty name, so the name is bounded
    like a title but allowed to be empty; the icon slug is validated against the
    known sidebar icon set rather than trusted as an arbitrary asset path.
    */
    let name = object
        .get("name")
        .and_then(serde_json::Value::as_str)
        .map(str::trim)
        .filter(|value| value.is_empty() || gpui_status_title_allowed(value))
        .map(str::to_string)
        .ok_or(())?;
    let icon = match object.get("icon") {
        None => None,
        Some(serde_json::Value::String(icon)) => Some(
            gpui_sidebar_command_icon(icon.trim())
                .ok_or(())?
                .to_string(),
        ),
        Some(_) => return Err(()),
    };
    if name.is_empty() && icon.is_none() {
        return Err(());
    }
    Ok(GpuiSidebarGlobalActionState {
        command_id: gpui_status_id_field(object, "commandId")?,
        icon,
        name,
    })
}

pub(crate) fn gpui_sidebar_pet_overlay_state_from_json(text: &str) -> Result<GpuiSidebarPetOverlayState, ()> {
    /*
    CDXC:GPUIStatusPetOverlay 2026-06-26-04:38:
    Pet overlay parser accepts only the saved enabled flag, bounded selected pet id, status items, and explicit project/session activity ids from the fixed sidebar bridge. It must not accept renderer paths, URLs, generic activation payloads, command text, stdout/stderr, tokens, terminal content, or menu-bar status-item data.

    CDXC:GPUIStatusPetOverlay 2026-06-26-05:30:
    The selected pet id must match a bundled GPUI pet spritesheet. Reject unknown ids instead of silently substituting a default asset so broken pet settings or bridge regressions remain visible during validation.
    */
    let value = serde_json::from_str::<serde_json::Value>(text).map_err(|_| ())?;
    let object = value.as_object().ok_or(())?;
    reject_unexpected_gpui_status_keys(
        object,
        &[
            "version",
            "type",
            "enabled",
            "selectedPetId",
            "statusItems",
            "activities",
        ],
    )?;
    if object.get("version").and_then(serde_json::Value::as_u64)
        != Some(GPUI_SIDEBAR_PET_OVERLAY_STATE_MESSAGE_VERSION)
        || object.get("type").and_then(serde_json::Value::as_str)
            != Some(GPUI_SIDEBAR_PET_OVERLAY_STATE_MESSAGE_TYPE)
    {
        return Err(());
    }
    let status_items = object
        .get("statusItems")
        .and_then(serde_json::Value::as_array)
        .filter(|items| items.len() <= 3)
        .ok_or(())?
        .iter()
        .map(gpui_pet_overlay_status_item_from_value)
        .collect::<Result<Vec<_>, _>>()?;
    let activities = object
        .get("activities")
        .and_then(serde_json::Value::as_array)
        .filter(|activities| activities.len() <= GPUI_STATUS_INDICATOR_MAX_ACTIVITIES)
        .ok_or(())?
        .iter()
        .map(gpui_pet_overlay_activity_from_value)
        .collect::<Result<Vec<_>, _>>()?;
    Ok(GpuiSidebarPetOverlayState {
        activities,
        enabled: gpui_status_bool_field(object, "enabled")?,
        selected_pet_id: gpui_pet_overlay_selected_pet_id_field(object, "selectedPetId")?,
        status_items,
    })
}

pub(crate) fn gpui_command_palette_run_sidebar_command_script(message: &serde_json::Value) -> String {
    format!(
        "(function(){{const bridge=window.ghostexGpui=window.ghostexGpui||{{}};const payload={message};if(typeof bridge.onCommandPaletteRunSidebarCommand==='function'){{bridge.onCommandPaletteRunSidebarCommand(payload);}}else{{const pending=Array.isArray(bridge.pendingCommandPaletteRunSidebarCommands)?bridge.pendingCommandPaletteRunSidebarCommands:[];pending.push(payload);bridge.pendingCommandPaletteRunSidebarCommands=pending;}}}})(); undefined;"
    )
}

pub(crate) fn gpui_sidebar_native_project_path_action_from_json(
    text: &str,
) -> Result<GpuiSidebarNativeProjectPathActionMessage, ()> {
    /*
    CDXC:GPUISidebarProjectPathActions 2026-06-24-14:18:
    Sidebar-native project path actions intentionally contain no path field. Keep their parsing strict and pathless so renderer compromise cannot turn copy/open project actions into arbitrary filesystem operations; only gxserver project ids may authorize those project-path side effects.

    CDXC:GPUISidebarGit 2026-06-24-15:43:
    The same fixed native side-effect bridge now accepts `filePath` only for the changed-file IDE-open action. Treat it as a project-relative candidate to re-validate against gxserver Git state; all project path and PR actions remain pathless, and no renderer URL or absolute path is authoritative.

    CDXC:GPUIRemoteAttach 2026-06-24-19:06:
    Remote session actions reuse the `projectId` string slot for a machine-scoped remote presentation session id because the bridge remains fixed-shape and pathless. Rust must parse that id before side effects and reject any payload that tries to add SSH details, paths, tokens, URLs, command text, or daemon responses.

    CDXC:GPUIRemoteNativeActions 2026-06-24-19:25:
    Remote project actions reuse the `projectId` string slot for a machine-scoped remote presentation project id. The parser still accepts only fixed action names and an optional relative file candidate for changed-file opens; remote paths, PR URLs, SSH details, command text, tokens, and daemon responses are never accepted from CEF.

    CDXC:GPUIRecentProjects 2026-08-14:
    Remote Recent Projects terminal creation enters this parser as `openRemoteProjectTerminal`. The renderer may identify only the saved machine and project id; Rust must restore the parked project and own remote gxserver creation plus SSH attach preparation.

    CDXC:GPUIRemoteNativeActions 2026-06-24-20:26:
    Remote IDE opens use the same pathless fixed-action bridge as copy-path and PR browser opens. Rust must resolve saved machine settings and remote gxserver project paths before constructing fixed editor argv/URI targets; CEF must not send remote paths, URI strings, SSH details, or Settings editor command text.

    CDXC:GPUIRemoteNativeActions 2026-06-24-21:33:
    Zed remote opens are fixed native actions using Zed's documented `zed ssh://[user@]host[:port]/path` CLI target after Rust resolves the remote path. Keep Cursor, Windsurf, VSCodium, Sublime, and custom remote editor commands unsupported until they have an equally reviewed deterministic remote opener.
    */
    let value = serde_json::from_str::<serde_json::Value>(text).map_err(|_| ())?;
    let object = value.as_object().ok_or(())?;
    if object.keys().any(|key| {
        ![
            "version",
            "type",
            "action",
            "projectId",
            "filePath",
            "preferredInterface",
        ]
        .contains(&key.as_str())
    }) {
        return Err(());
    }
    if object.get("version").and_then(serde_json::Value::as_u64)
        != Some(GPUI_SIDEBAR_NATIVE_PROJECT_PATH_ACTION_MESSAGE_VERSION)
        || object.get("type").and_then(serde_json::Value::as_str)
            != Some(GPUI_SIDEBAR_NATIVE_PROJECT_PATH_ACTION_MESSAGE_TYPE)
    {
        return Err(());
    }
    let action = object
        .get("action")
        .and_then(serde_json::Value::as_str)
        .and_then(GpuiSidebarNativeProjectPathAction::from_str)
        .ok_or(())?;
    let file_path = object
        .get("filePath")
        .map(|value| {
            value
                .as_str()
                .and_then(gpui_normalized_relative_git_file_path)
                .ok_or(())
        })
        .transpose()?;
    if action.requires_file_path() != file_path.is_some() {
        return Err(());
    }
    let preferred_interface = match object.get("preferredInterface") {
        None => GpuiPreferredAgentInterface::Terminal,
        Some(value) => value
            .as_str()
            .and_then(GpuiPreferredAgentInterface::from_str)
            .ok_or(())?,
    };
    if object.contains_key("preferredInterface")
        && action != GpuiSidebarNativeProjectPathAction::OpenRemoteSessionTerminal
    {
        return Err(());
    }
    let project_id = object
        .get("projectId")
        .and_then(serde_json::Value::as_str)
        .map(str::trim)
        .filter(|project_id| {
            !project_id.is_empty()
                && project_id.chars().count() <= GPUI_PROJECT_CONTRACT_STRING_MAX_CHARS
                && !project_id.contains('/')
                && !project_id.contains('\\')
        })
        .ok_or(())?
        .to_string();
    Ok(GpuiSidebarNativeProjectPathActionMessage {
        action,
        file_path,
        preferred_interface,
        project_id,
    })
}

pub(crate) fn gpui_sidebar_command_action_from_json(text: &str) -> Result<GpuiTitlebarAction, ()> {
    /*
    CDXC:GPUICommandPane 2026-06-24-23:17:
    Sidebar command-action payloads are fixed action metadata from the live gxserver HUD projection. Accept only command id, name, action type, and the one action target needed for that type; reject project paths, renderer cwd, env, stdout/stderr, terminal content, shell-state fields, generic IPC names, and mismatched command/url pairs before the existing action runner can create a Browser tab or command-pane launch payload.

    CDXC:GPUICommandPane 2026-06-25-10:29:
    `runMode:"debug"` is a terminal-only control bit from the shared sidebar click contract. It may select the visible debug workspace-terminal path, but it must not allow browser Actions, project paths, cwd/env, logs, or renderer-provided shell metadata to influence command-pane execution.

    CDXC:GPUICommandPaneActions 2026-06-26-04:59:
    `closeTerminalOnExit` remains accepted only as legacy terminal Action metadata for saved-config compatibility. Browser Actions must not carry it, terminal payloads must reject non-booleans, and command-pane runtime must still normalize it to false instead of inferring close behavior from renderer strings, command text, URLs, paths, or shell state.
    */
    let value = serde_json::from_str::<serde_json::Value>(text).map_err(|_| ())?;
    let object = value.as_object().ok_or(())?;
    if object.keys().any(|key| {
        ![
            "version",
            "type",
            "actionType",
            "closeTerminalOnExit",
            "commandId",
            "icon",
            "links",
            "name",
            "playCompletionSound",
            "runMode",
            "command",
            "url",
        ]
        .contains(&key.as_str())
    }) {
        return Err(());
    }
    if object.get("version").and_then(serde_json::Value::as_u64)
        != Some(GPUI_SIDEBAR_COMMAND_ACTION_MESSAGE_VERSION)
        || object.get("type").and_then(serde_json::Value::as_str)
            != Some(GPUI_SIDEBAR_COMMAND_ACTION_MESSAGE_TYPE)
    {
        return Err(());
    }
    let command_id = gpui_trimmed_json_string_field(object, "commandId")
        .filter(|value| value.chars().count() <= GPUI_PROJECT_CONTRACT_STRING_MAX_CHARS)
        .ok_or(())?
        .to_string();
    let name = object
        .get("name")
        .and_then(serde_json::Value::as_str)
        .map(str::trim)
        .filter(|value| value.chars().count() <= GPUI_PROJECT_CONTRACT_STRING_MAX_CHARS)
        .unwrap_or_default()
        .to_string();
    let icon = match object.get("icon") {
        None | Some(serde_json::Value::Null) => None,
        Some(value) => Some(
            value
                .as_str()
                .and_then(gpui_sidebar_command_icon)
                .map(str::to_string)
                .ok_or(())?,
        ),
    };
    let command = object
        .get("command")
        .and_then(serde_json::Value::as_str)
        .and_then(|command| gpui_trimmed_nonempty_str(Some(command)))
        .map(str::to_string);
    let url = object
        .get("url")
        .and_then(serde_json::Value::as_str)
        .and_then(|url| gpui_trimmed_nonempty_str(Some(url)))
        .map(str::to_string);
    let action_type = match object.get("actionType").and_then(serde_json::Value::as_str) {
        Some("browser") if url.is_some() && command.is_none() => GpuiTitlebarActionType::Browser,
        Some("terminal") if command.is_some() && url.is_none() => GpuiTitlebarActionType::Terminal,
        _ => return Err(()),
    };
    let play_completion_sound = match object.get("playCompletionSound") {
        None => action_type == GpuiTitlebarActionType::Terminal,
        Some(value) if action_type == GpuiTitlebarActionType::Terminal => {
            value.as_bool().ok_or(())?
        }
        Some(_) => return Err(()),
    };
    let close_terminal_on_exit = match object.get("closeTerminalOnExit") {
        None => false,
        Some(value) if action_type == GpuiTitlebarActionType::Terminal => {
            value.as_bool().ok_or(())?
        }
        Some(_) => return Err(()),
    };
    let run_mode = match object.get("runMode").and_then(serde_json::Value::as_str) {
        None | Some("default") => GpuiTitlebarActionRunMode::Default,
        Some("debug") if action_type == GpuiTitlebarActionType::Terminal => {
            GpuiTitlebarActionRunMode::Debug
        }
        Some(_) => return Err(()),
    };
    /*
    CDXC:ProjectActions 2026-07-31-12:00:
    Saved links are terminal-only Action metadata from the trusted HUD command.
    Reject links on browser Actions, non-array shapes, unknown per-link keys,
    unknown targets, and empty or oversized URLs instead of stripping them.
    */
    let links = match object.get("links") {
        None => Vec::new(),
        Some(value) if action_type == GpuiTitlebarActionType::Terminal => value
            .as_array()
            .ok_or(())?
            .iter()
            .map(|item| {
                let item = item.as_object().ok_or(())?;
                if item
                    .keys()
                    .any(|key| !["target", "url"].contains(&key.as_str()))
                {
                    return Err(());
                }
                let url = item
                    .get("url")
                    .and_then(serde_json::Value::as_str)
                    .and_then(|url| gpui_trimmed_nonempty_str(Some(url)))
                    .filter(|url| url.chars().count() <= GPUI_SIDEBAR_OPEN_BROWSER_URL_MAX_CHARS)
                    .ok_or(())?
                    .to_string();
                let target = match item.get("target").and_then(serde_json::Value::as_str) {
                    Some("external") => GpuiTitlebarActionLinkTarget::External,
                    Some("integrated") => GpuiTitlebarActionLinkTarget::Integrated,
                    _ => return Err(()),
                };
                Ok(GpuiTitlebarActionLink { target, url })
            })
            .collect::<Result<Vec<_>, ()>>()?,
        Some(_) => return Err(()),
    };
    Ok(GpuiTitlebarAction {
        action_type,
        close_terminal_on_exit,
        command,
        command_id,
        icon,
        links,
        name,
        play_completion_sound,
        run_mode,
        url,
    })
}

pub(crate) fn gpui_sidebar_command_run_end_from_json(text: &str) -> Result<String, ()> {
    /*
    CDXC:GPUICommandPane 2026-06-25-10:34:
    Sidebar command-run-end payloads close the existing live Action tab by command id only. Keep the parser stricter than the launch bridge so closing a run cannot carry command text, URLs, project paths, cwd/env, run ids, status paths, terminal output, persisted shell state, or generic IPC fields.
    */
    let value = serde_json::from_str::<serde_json::Value>(text).map_err(|_| ())?;
    let object = value.as_object().ok_or(())?;
    if object
        .keys()
        .any(|key| !["version", "type", "commandId"].contains(&key.as_str()))
    {
        return Err(());
    }
    if object.get("version").and_then(serde_json::Value::as_u64)
        != Some(GPUI_SIDEBAR_COMMAND_RUN_END_MESSAGE_VERSION)
        || object.get("type").and_then(serde_json::Value::as_str)
            != Some(GPUI_SIDEBAR_COMMAND_RUN_END_MESSAGE_TYPE)
    {
        return Err(());
    }
    let command_id = gpui_trimmed_json_string_field(object, "commandId")
        .filter(|value| value.chars().count() <= GPUI_PROJECT_CONTRACT_STRING_MAX_CHARS)
        .ok_or(())?
        .to_string();
    Ok(command_id)
}

pub(crate) fn gpui_sidebar_ghostex_hotkey_action_from_json(text: &str) -> Result<String, ()> {
    /*
    CDXC:GPUICommandPalette 2026-06-27-08:17:
    Command-palette hotkey payloads are selector authority only. Accept `type` plus a bounded non-empty `actionId`, then let the existing hotkey dispatcher decide support; reject renderer-owned command text, cwd/env, session ids, paths, URLs, launch metadata, generic IPC fields, and versioned action payloads before they can influence command-pane focus or modal routing.
    */
    let value = serde_json::from_str::<serde_json::Value>(text).map_err(|_| ())?;
    let object = value.as_object().ok_or(())?;
    if object
        .keys()
        .any(|key| !["type", "actionId"].contains(&key.as_str()))
    {
        return Err(());
    }
    if object.get("type").and_then(serde_json::Value::as_str) != Some("runGhostexHotkeyAction") {
        return Err(());
    }
    let action_id = gpui_trimmed_json_string_field(object, "actionId")
        .filter(|value| value.chars().count() <= GPUI_PROJECT_CONTRACT_STRING_MAX_CHARS)
        .ok_or(())?
        .to_string();
    Ok(action_id)
}

pub(crate) fn execute_gpui_sidebar_native_project_path_action(
    message: GpuiSidebarNativeProjectPathActionMessage,
) -> Result<GpuiSidebarNativeProjectPathActionResult, String> {
    if message.action == GpuiSidebarNativeProjectPathAction::OpenExistingPullRequestInBrowser {
        return gpui_open_existing_project_pull_request_in_browser(&message.project_id)
            .map(|_| GpuiSidebarNativeProjectPathActionResult::Opened);
    }
    if message.action == GpuiSidebarNativeProjectPathAction::OpenSidebarGitChangedFileInIde {
        let file_path = message
            .file_path
            .as_deref()
            .ok_or_else(|| "Choose a changed file from the current Git state.".to_string())?;
        return gpui_open_sidebar_git_changed_file_in_ide(&message.project_id, file_path)
            .map(|_| GpuiSidebarNativeProjectPathActionResult::Opened);
    }
    let path = if message.action.uses_recent_projects() {
        gpui_gxserver_recent_project_path_by_id(&message.project_id)?
    } else {
        gpui_gxserver_workspace_project_path_by_id(&message.project_id)?
    };
    if message.action.copies_path() {
        return Ok(GpuiSidebarNativeProjectPathActionResult::Copied(
            gpui_path_string(&path),
        ));
    }
    if message.action.opens_in_ide() {
        return gpui_open_project_path_for_native_ide_action(message.action, &path)
            .map(|_| GpuiSidebarNativeProjectPathActionResult::Opened);
    }
    gpui_open_path(&path)
        .map(|_| GpuiSidebarNativeProjectPathActionResult::Opened)
        .map_err(|_| "GPUI could not open that project in Finder.".to_string())
}

pub(crate) fn gpui_open_sidebar_git_changed_file_in_ide(
    project_id: &str,
    file_path: &str,
) -> Result<(), String> {
    /*
    CDXC:GPUISidebarGit 2026-06-24-15:43:
    Changed-file IDE opens resolve project id plus a project-relative file candidate in Rust. Rebuild the current gxserver changed-file set before joining under the project root so CEF cannot open arbitrary absolute paths, sibling paths, URLs, command text, or stale renderer-only filenames.
    */
    let relative_file_path = gpui_normalized_relative_git_file_path(file_path)
        .ok_or_else(|| "Choose a changed file from the current Git state.".to_string())?;
    let changed_files = gpui_project_git_changed_file_paths(project_id)?;
    if !changed_files.contains(&relative_file_path) {
        return Err("Choose a changed file from the current Git state.".to_string());
    }
    let project_path = gpui_gxserver_workspace_project_path_by_id(project_id)?;
    let absolute_file_path = project_path.join(&relative_file_path);
    if !path_is_inside_or_equal(&absolute_file_path, &project_path) {
        return Err("Choose a changed file from the current Git state.".to_string());
    }
    gpui_open_project_path_in_default_editor(&absolute_file_path)
        .map_err(|_| "Configured editor could not open that file.".to_string())
}

pub(crate) fn store_latest_gpui_project_snapshot_from_sidebar_contract_json(
    latest_snapshot: &mut Option<GpuiProjectSnapshot>,
    text: &str,
) -> Result<GpuiProjectSnapshotStoreResult, GpuiProjectSnapshotContractError> {
    /*
    CDXC:GPUIProjectSidebarBridge 2026-06-22-19:32:
    The live CEF sidebar bridge may update only the in-memory latest active-project snapshot after the strict contract parser succeeds. Malformed payloads leave the prior snapshot untouched, and the snapshot remains non-persistent.

    CDXC:GPUIProjectSidebarBridge 2026-06-22-19:44:
    Once stored, the latest valid sidebar snapshot becomes the App runtime availability source; the env bridge is only the fallback before any valid sidebar payload arrives. The store helper itself still does not log raw JSON or project details and does not coerce active mode without the App context.

    CDXC:GPUIProjectSidebarBridge 2026-06-23-06:53:
    The store helper returns an explicit change result so bridge callers can no-op duplicate valid payloads. Parse and validate exactly as before, preserve the previous snapshot on errors, and replace the in-memory snapshot only after the parsed snapshot differs; do not add project/path/name heuristics, fallbacks, persistence, or logging of raw contract data.
    */
    let snapshot = gpui_project_snapshot_from_sidebar_contract_json(text)?;
    if latest_snapshot
        .as_ref()
        .is_some_and(|latest_snapshot| latest_snapshot == &snapshot)
    {
        return Ok(GpuiProjectSnapshotStoreResult::Unchanged);
    }

    *latest_snapshot = Some(snapshot);
    Ok(GpuiProjectSnapshotStoreResult::Changed)
}

#[allow(dead_code)]
pub(crate) fn gpui_project_snapshot_from_sidebar_contract_json(
    text: &str,
) -> Result<GpuiProjectSnapshot, GpuiProjectSnapshotContractError> {
    let value = serde_json::from_str::<serde_json::Value>(text)
        .map_err(|_| GpuiProjectSnapshotContractError::MalformedJson)?;
    gpui_project_snapshot_from_sidebar_contract_value(&value)
}

#[allow(dead_code)]
pub(crate) fn gpui_project_snapshot_from_sidebar_contract_value(
    value: &serde_json::Value,
) -> Result<GpuiProjectSnapshot, GpuiProjectSnapshotContractError> {
    let object = gpui_contract_object(value)?;
    reject_unexpected_contract_keys(object, &["version", "type", "activeProject"])?;

    let version = object
        .get("version")
        .and_then(serde_json::Value::as_u64)
        .ok_or(GpuiProjectSnapshotContractError::UnexpectedVersion)?;
    if version != GPUI_SIDEBAR_PROJECT_CONTEXT_MESSAGE_VERSION {
        return Err(GpuiProjectSnapshotContractError::UnexpectedVersion);
    }

    let message_type = object
        .get("type")
        .and_then(serde_json::Value::as_str)
        .ok_or(GpuiProjectSnapshotContractError::UnexpectedMessageType)?;
    if message_type != GPUI_SIDEBAR_PROJECT_CONTEXT_MESSAGE_TYPE {
        return Err(GpuiProjectSnapshotContractError::UnexpectedMessageType);
    }

    let active_project = object
        .get("activeProject")
        .ok_or(GpuiProjectSnapshotContractError::MissingField)?;
    gpui_project_snapshot_from_contract_project_value(active_project)
}

pub(crate) fn gpui_sidebar_workspace_terminal_focus_from_json(
    text: &str,
) -> Result<GpuiSidebarWorkspaceTerminalFocusMessage, GpuiGxserverPresentationFocusStateContractError>
{
    let value = serde_json::from_str::<serde_json::Value>(text)
        .map_err(|_| GpuiGxserverPresentationFocusStateContractError::MalformedJson)?;
    gpui_sidebar_workspace_terminal_focus_from_value(&value)
}

pub(crate) fn gpui_sidebar_workspace_terminal_focus_from_value(
    value: &serde_json::Value,
) -> Result<GpuiSidebarWorkspaceTerminalFocusMessage, GpuiGxserverPresentationFocusStateContractError>
{
    let object = gpui_gxserver_focus_contract_object(value)?;
    reject_unexpected_gxserver_focus_contract_keys(
        object,
        &[
            "version",
            "type",
            "forceRemount",
            "placementTargetSessionId",
            "preferredInterface",
            "projectId",
            "sessionId",
        ],
    )?;

    let version = object
        .get("version")
        .and_then(serde_json::Value::as_u64)
        .ok_or(GpuiGxserverPresentationFocusStateContractError::UnexpectedVersion)?;
    if version != GPUI_SIDEBAR_WORKSPACE_TERMINAL_FOCUS_MESSAGE_VERSION {
        return Err(GpuiGxserverPresentationFocusStateContractError::UnexpectedVersion);
    }

    let message_type = object
        .get("type")
        .and_then(serde_json::Value::as_str)
        .ok_or(GpuiGxserverPresentationFocusStateContractError::UnexpectedMessageType)?;
    if message_type != GPUI_SIDEBAR_WORKSPACE_TERMINAL_FOCUS_MESSAGE_TYPE {
        return Err(GpuiGxserverPresentationFocusStateContractError::UnexpectedMessageType);
    }

    let project_id = gxserver_workspace_focus_project_id_field(object, "projectId")?;
    let session_id = gxserver_workspace_focus_session_id_field(object, "sessionId")?;
    let placement_target_session_id = object
        .get("placementTargetSessionId")
        .map(|_| gxserver_workspace_focus_session_id_field(object, "placementTargetSessionId"))
        .transpose()?;
    let force_remount = match object.get("forceRemount") {
        None => false,
        Some(value) => value
            .as_bool()
            .ok_or(GpuiGxserverPresentationFocusStateContractError::MalformedJson)?,
    };
    let preferred_interface = match object.get("preferredInterface") {
        None => GpuiPreferredAgentInterface::Terminal,
        Some(value) => value
            .as_str()
            .and_then(GpuiPreferredAgentInterface::from_str)
            .ok_or(GpuiGxserverPresentationFocusStateContractError::MalformedJson)?,
    };
    Ok(GpuiSidebarWorkspaceTerminalFocusMessage {
        force_remount,
        placement_target_session_id,
        preferred_interface,
        project_id,
        session_id,
    })
}

pub(crate) fn gpui_sidebar_create_project_agent_from_json(
    text: &str,
) -> Result<GpuiSidebarCreateProjectAgentMessage, GpuiGxserverPresentationFocusStateContractError> {
    let value = serde_json::from_str::<serde_json::Value>(text)
        .map_err(|_| GpuiGxserverPresentationFocusStateContractError::MalformedJson)?;
    let object = gpui_gxserver_focus_contract_object(&value)?;
    reject_unexpected_gxserver_focus_contract_keys(
        object,
        &[
            "version",
            "type",
            "projectId",
            "agentId",
            "preferredInterface",
        ],
    )?;

    let version = object
        .get("version")
        .and_then(serde_json::Value::as_u64)
        .ok_or(GpuiGxserverPresentationFocusStateContractError::UnexpectedVersion)?;
    if version != GPUI_SIDEBAR_CREATE_PROJECT_AGENT_MESSAGE_VERSION {
        return Err(GpuiGxserverPresentationFocusStateContractError::UnexpectedVersion);
    }

    let message_type = object
        .get("type")
        .and_then(serde_json::Value::as_str)
        .ok_or(GpuiGxserverPresentationFocusStateContractError::UnexpectedMessageType)?;
    if message_type != GPUI_SIDEBAR_CREATE_PROJECT_AGENT_MESSAGE_TYPE {
        return Err(GpuiGxserverPresentationFocusStateContractError::UnexpectedMessageType);
    }

    let project_id = gxserver_workspace_focus_project_id_field(object, "projectId")?;
    let preferred_interface = object
        .get("preferredInterface")
        .and_then(serde_json::Value::as_str)
        .and_then(GpuiPreferredAgentInterface::from_str)
        .ok_or(GpuiGxserverPresentationFocusStateContractError::MalformedField)?;
    let agent_id = object
        .get("agentId")
        .and_then(serde_json::Value::as_str)
        .map(str::trim)
        .filter(|value| gpui_remote_sidebar_agent_id_allowed(value))
        .ok_or(GpuiGxserverPresentationFocusStateContractError::MalformedField)?
        .to_string();
    Ok(GpuiSidebarCreateProjectAgentMessage {
        agent_id,
        preferred_interface,
        project_id,
    })
}

pub(crate) fn gpui_sidebar_create_project_terminal_from_json(
    text: &str,
) -> Result<GpuiSidebarCreateProjectTerminalMessage, GpuiGxserverPresentationFocusStateContractError>
{
    let value = serde_json::from_str::<serde_json::Value>(text)
        .map_err(|_| GpuiGxserverPresentationFocusStateContractError::MalformedJson)?;
    let object = gpui_gxserver_focus_contract_object(&value)?;
    reject_unexpected_gxserver_focus_contract_keys(object, &["version", "type", "projectId"])?;

    let version = object
        .get("version")
        .and_then(serde_json::Value::as_u64)
        .ok_or(GpuiGxserverPresentationFocusStateContractError::UnexpectedVersion)?;
    if version != GPUI_SIDEBAR_CREATE_PROJECT_TERMINAL_MESSAGE_VERSION {
        return Err(GpuiGxserverPresentationFocusStateContractError::UnexpectedVersion);
    }

    let message_type = object
        .get("type")
        .and_then(serde_json::Value::as_str)
        .ok_or(GpuiGxserverPresentationFocusStateContractError::UnexpectedMessageType)?;
    if message_type != GPUI_SIDEBAR_CREATE_PROJECT_TERMINAL_MESSAGE_TYPE {
        return Err(GpuiGxserverPresentationFocusStateContractError::UnexpectedMessageType);
    }

    let project_id = gxserver_workspace_focus_project_id_field(object, "projectId")?;
    Ok(GpuiSidebarCreateProjectTerminalMessage { project_id })
}

pub(crate) fn gpui_sidebar_workspace_terminal_rename_command_from_json(
    text: &str,
) -> Result<
    GpuiSidebarWorkspaceTerminalRenameCommandMessage,
    GpuiGxserverPresentationFocusStateContractError,
> {
    let value = serde_json::from_str::<serde_json::Value>(text)
        .map_err(|_| GpuiGxserverPresentationFocusStateContractError::MalformedJson)?;
    gpui_sidebar_workspace_terminal_rename_command_from_value(&value)
}

pub(crate) fn gpui_sidebar_workspace_terminal_rename_command_from_value(
    value: &serde_json::Value,
) -> Result<
    GpuiSidebarWorkspaceTerminalRenameCommandMessage,
    GpuiGxserverPresentationFocusStateContractError,
> {
    /*
    CDXC:GPUIWorkspaceRenameCommand 2026-06-27-02:27:
    The fixed renderer payload must contain only version/type, raw local gxserver project/session ids, one already-trimmed bounded title, and an optional literal command selector. Reject extra keys, remote or combined ids, missing ids, untrimmed/empty/oversized/control-character titles, paths, free-text command fields, stdout/stderr, terminal content, tokens, and raw renderer envelopes before any terminal surface is consulted.
    */
    let object = gpui_gxserver_focus_contract_object(value)?;
    reject_unexpected_gxserver_focus_contract_keys(
        object,
        &[
            "version",
            "type",
            "projectId",
            "sessionId",
            "title",
            "command",
        ],
    )?;

    let version = object
        .get("version")
        .and_then(serde_json::Value::as_u64)
        .ok_or(GpuiGxserverPresentationFocusStateContractError::UnexpectedVersion)?;
    if version != GPUI_SIDEBAR_WORKSPACE_TERMINAL_RENAME_COMMAND_MESSAGE_VERSION {
        return Err(GpuiGxserverPresentationFocusStateContractError::UnexpectedVersion);
    }

    let message_type = object
        .get("type")
        .and_then(serde_json::Value::as_str)
        .ok_or(GpuiGxserverPresentationFocusStateContractError::UnexpectedMessageType)?;
    if message_type != GPUI_SIDEBAR_WORKSPACE_TERMINAL_RENAME_COMMAND_MESSAGE_TYPE {
        return Err(GpuiGxserverPresentationFocusStateContractError::UnexpectedMessageType);
    }

    let project_id = gxserver_workspace_focus_project_id_field(object, "projectId")?;
    let session_id = gxserver_workspace_focus_session_id_field(object, "sessionId")?;
    let title = gxserver_workspace_terminal_rename_title_field(object, "title")?;
    let command = match object.get("command") {
        None => GpuiWorkspaceTerminalRenameCommandKind::Rename,
        Some(serde_json::Value::String(command)) if command == "rename" => {
            GpuiWorkspaceTerminalRenameCommandKind::Rename
        }
        Some(serde_json::Value::String(command)) if command == "name" => {
            GpuiWorkspaceTerminalRenameCommandKind::Name
        }
        Some(serde_json::Value::String(command)) if command == "title" => {
            GpuiWorkspaceTerminalRenameCommandKind::Title
        }
        Some(_) => {
            return Err(GpuiGxserverPresentationFocusStateContractError::MalformedField);
        }
    };
    Ok(GpuiSidebarWorkspaceTerminalRenameCommandMessage {
        command,
        project_id,
        session_id,
        title,
    })
}

pub(crate) fn gpui_sidebar_workspace_terminal_enter_from_json(
    text: &str,
) -> Result<GpuiSidebarWorkspaceTerminalEnterMessage, GpuiGxserverPresentationFocusStateContractError>
{
    let value = serde_json::from_str::<serde_json::Value>(text)
        .map_err(|_| GpuiGxserverPresentationFocusStateContractError::MalformedJson)?;
    gpui_sidebar_workspace_terminal_enter_from_value(&value)
}

pub(crate) fn gpui_sidebar_workspace_terminal_enter_from_value(
    value: &serde_json::Value,
) -> Result<GpuiSidebarWorkspaceTerminalEnterMessage, GpuiGxserverPresentationFocusStateContractError>
{
    let object = gpui_gxserver_focus_contract_object(value)?;
    reject_unexpected_gxserver_focus_contract_keys(
        object,
        &["version", "type", "projectId", "sessionId"],
    )?;

    let version = object
        .get("version")
        .and_then(serde_json::Value::as_u64)
        .ok_or(GpuiGxserverPresentationFocusStateContractError::UnexpectedVersion)?;
    if version != GPUI_SIDEBAR_WORKSPACE_TERMINAL_ENTER_MESSAGE_VERSION {
        return Err(GpuiGxserverPresentationFocusStateContractError::UnexpectedVersion);
    }

    let message_type = object
        .get("type")
        .and_then(serde_json::Value::as_str)
        .ok_or(GpuiGxserverPresentationFocusStateContractError::UnexpectedMessageType)?;
    if message_type != GPUI_SIDEBAR_WORKSPACE_TERMINAL_ENTER_MESSAGE_TYPE {
        return Err(GpuiGxserverPresentationFocusStateContractError::UnexpectedMessageType);
    }

    let project_id = gxserver_workspace_focus_project_id_field(object, "projectId")?;
    let session_id = gxserver_workspace_focus_session_id_field(object, "sessionId")?;
    Ok(GpuiSidebarWorkspaceTerminalEnterMessage {
        project_id,
        session_id,
    })
}

pub(crate) fn gpui_sidebar_session_completion_sound_from_json(
    text: &str,
) -> Result<String, GpuiGxserverPresentationFocusStateContractError> {
    let value = serde_json::from_str::<serde_json::Value>(text)
        .map_err(|_| GpuiGxserverPresentationFocusStateContractError::MalformedJson)?;
    let object = gpui_gxserver_focus_contract_object(&value)?;
    reject_unexpected_gxserver_focus_contract_keys(object, &["version", "type", "sound"])?;

    let version = object
        .get("version")
        .and_then(serde_json::Value::as_u64)
        .ok_or(GpuiGxserverPresentationFocusStateContractError::UnexpectedVersion)?;
    if version != GPUI_SIDEBAR_SESSION_COMPLETION_SOUND_MESSAGE_VERSION {
        return Err(GpuiGxserverPresentationFocusStateContractError::UnexpectedVersion);
    }

    let message_type = object
        .get("type")
        .and_then(serde_json::Value::as_str)
        .ok_or(GpuiGxserverPresentationFocusStateContractError::UnexpectedMessageType)?;
    if message_type != GPUI_SIDEBAR_SESSION_COMPLETION_SOUND_MESSAGE_TYPE {
        return Err(GpuiGxserverPresentationFocusStateContractError::UnexpectedMessageType);
    }

    let sound = object
        .get("sound")
        .ok_or(GpuiGxserverPresentationFocusStateContractError::MissingField)?
        .as_str()
        .ok_or(GpuiGxserverPresentationFocusStateContractError::MalformedField)?;
    let trimmed = sound.trim();
    if trimmed.is_empty() || trimmed.len() > GPUI_SIDEBAR_SESSION_COMPLETION_SOUND_MAX_CHARS {
        return Err(GpuiGxserverPresentationFocusStateContractError::MalformedField);
    }
    Ok(trimmed.to_string())
}

pub(crate) fn gpui_sidebar_open_browser_url_from_json(
    text: &str,
) -> Result<GpuiSidebarOpenBrowserUrlMessage, GpuiGxserverPresentationFocusStateContractError> {
    /*
    The renderer-command browser open carries only one URL-or-search string and
    a fixed reuse selector. Rust re-normalizes the address through the same
    toolbar path as typed input, so renderer payloads cannot smuggle project
    ids, paths, commands, tokens, or raw renderer envelopes into Browser state.

    CDXC:GPUIRemoteBrowserTabs 2026-07-12:
    The one exception is the optional first-party sidebar `projectId`, which
    must be a known browser project key shape — a local `P…` workspace id or a
    machine-scoped `remote:<machine>:project:<id>` reference — so project
    headers can target their own browser tab model without racing the async
    active-project context round-trip. Any other string still rejects the
    whole payload.
    */
    let value = serde_json::from_str::<serde_json::Value>(text)
        .map_err(|_| GpuiGxserverPresentationFocusStateContractError::MalformedJson)?;
    let object = gpui_gxserver_focus_contract_object(&value)?;
    reject_unexpected_gxserver_focus_contract_keys(
        object,
        &["version", "type", "url", "reuse", "origin", "projectId"],
    )?;

    let version = object
        .get("version")
        .and_then(serde_json::Value::as_u64)
        .ok_or(GpuiGxserverPresentationFocusStateContractError::UnexpectedVersion)?;
    if version != GPUI_SIDEBAR_OPEN_BROWSER_URL_MESSAGE_VERSION {
        return Err(GpuiGxserverPresentationFocusStateContractError::UnexpectedVersion);
    }

    let message_type = object
        .get("type")
        .and_then(serde_json::Value::as_str)
        .ok_or(GpuiGxserverPresentationFocusStateContractError::UnexpectedMessageType)?;
    if message_type != GPUI_SIDEBAR_OPEN_BROWSER_URL_MESSAGE_TYPE {
        return Err(GpuiGxserverPresentationFocusStateContractError::UnexpectedMessageType);
    }

    let url = object
        .get("url")
        .ok_or(GpuiGxserverPresentationFocusStateContractError::MissingField)?
        .as_str()
        .ok_or(GpuiGxserverPresentationFocusStateContractError::MalformedField)?
        .trim();
    if url.len() > GPUI_SIDEBAR_OPEN_BROWSER_URL_MAX_CHARS
        || url.chars().any(|character| character.is_control())
    {
        return Err(GpuiGxserverPresentationFocusStateContractError::MalformedField);
    }

    let reuse = match object
        .get("reuse")
        .ok_or(GpuiGxserverPresentationFocusStateContractError::MissingField)?
        .as_str()
        .ok_or(GpuiGxserverPresentationFocusStateContractError::MalformedField)?
    {
        "exact" => GpuiBrowserRendererOpenReuse::Exact,
        "none" => GpuiBrowserRendererOpenReuse::None,
        "similar" => GpuiBrowserRendererOpenReuse::Similar,
        _ => return Err(GpuiGxserverPresentationFocusStateContractError::MalformedField),
    };

    let from_quick_header = match object.get("origin") {
        None => false,
        Some(serde_json::Value::String(origin)) if origin == "quickHeader" => true,
        Some(_) => return Err(GpuiGxserverPresentationFocusStateContractError::MalformedField),
    };

    let project_id = match object.get("projectId") {
        None => None,
        Some(serde_json::Value::String(project_id))
            if gpui_browser_tabs_project_key_allowed(project_id.trim()) =>
        {
            Some(project_id.trim().to_string())
        }
        Some(_) => return Err(GpuiGxserverPresentationFocusStateContractError::MalformedField),
    };

    Ok(GpuiSidebarOpenBrowserUrlMessage {
        url: url.to_string(),
        reuse,
        from_quick_header,
        project_id,
    })
}

pub(crate) fn gpui_sidebar_workspace_terminal_lifecycle_result_from_json(
    text: &str,
) -> Result<
    GpuiSidebarWorkspaceTerminalLifecycleResultMessage,
    GpuiGxserverPresentationFocusStateContractError,
> {
    let value = serde_json::from_str::<serde_json::Value>(text)
        .map_err(|_| GpuiGxserverPresentationFocusStateContractError::MalformedJson)?;
    gpui_sidebar_workspace_terminal_lifecycle_result_from_value(&value)
}

pub(crate) fn gpui_sidebar_workspace_terminal_lifecycle_result_from_value(
    value: &serde_json::Value,
) -> Result<
    GpuiSidebarWorkspaceTerminalLifecycleResultMessage,
    GpuiGxserverPresentationFocusStateContractError,
> {
    let object = gpui_gxserver_focus_contract_object(value)?;
    reject_unexpected_gxserver_focus_contract_keys(
        object,
        &["version", "type", "requestId", "ok"],
    )?;

    let version = object
        .get("version")
        .and_then(serde_json::Value::as_u64)
        .ok_or(GpuiGxserverPresentationFocusStateContractError::UnexpectedVersion)?;
    if version != GPUI_SIDEBAR_WORKSPACE_TERMINAL_LIFECYCLE_RESULT_MESSAGE_VERSION {
        return Err(GpuiGxserverPresentationFocusStateContractError::UnexpectedVersion);
    }

    let message_type = object
        .get("type")
        .and_then(serde_json::Value::as_str)
        .ok_or(GpuiGxserverPresentationFocusStateContractError::UnexpectedMessageType)?;
    if message_type != GPUI_SIDEBAR_WORKSPACE_TERMINAL_LIFECYCLE_RESULT_MESSAGE_TYPE {
        return Err(GpuiGxserverPresentationFocusStateContractError::UnexpectedMessageType);
    }

    let request_id = object
        .get("requestId")
        .and_then(serde_json::Value::as_u64)
        .filter(|request_id| {
            (1..=GPUI_SIDEBAR_WORKSPACE_TERMINAL_LIFECYCLE_REQUEST_ID_MAX).contains(request_id)
        })
        .ok_or(GpuiGxserverPresentationFocusStateContractError::MalformedField)?;
    let ok = object
        .get("ok")
        .and_then(serde_json::Value::as_bool)
        .ok_or(GpuiGxserverPresentationFocusStateContractError::MalformedField)?;
    Ok(GpuiSidebarWorkspaceTerminalLifecycleResultMessage { ok, request_id })
}

pub(crate) fn sidebar_runtime_settings_snapshot_from_shared_settings(
    settings: &shared_settings::SharedSidebarSettingsSnapshot,
) -> cef::SidebarRuntimeSettingsSnapshot {
    /*
    CDXC:GPUIProjectSidebarBridge 2026-06-23-06:36:
    The sidebar CEF runtime settings handoff must use the same shared sidebar settings file and strict boolean interpretation as SidebarApp. These booleans seed TS-side payload and workarea behavior only; Docs titlebar visibility stays governed by project context, not debuggingMode/showBetaFeatures.

    CDXC:GPUISettingsSidebarHandoff 2026-06-24-11:22:
    The GPUI sidebar runtime snapshot now also carries the saved shared Settings object as serialized first-party payload so the mounted SidebarApp can normalize real user preferences immediately on initial CEF install and after Settings saves. This is not a generic settings bus and must not write logs, persist another copy, or expose settings to Browser/workarea/modal CEF clients.
    */
    cef::SidebarRuntimeSettingsSnapshot {
        debugging_mode: settings.debugging_mode(),
        show_beta_features: settings.show_beta_features(),
        saved_settings_json: sidebar_runtime_saved_settings_json(settings),
    }
}

pub(crate) fn sidebar_runtime_saved_settings_json(
    settings: &shared_settings::SharedSidebarSettingsSnapshot,
) -> String {
    serde_json::to_string(&serde_json::Value::Object(settings.object().clone()))
        .unwrap_or_else(|_| "{}".to_string())
}

pub(crate) fn changed_sidebar_runtime_settings_snapshot(
    current: &cef::SidebarRuntimeSettingsSnapshot,
    next: cef::SidebarRuntimeSettingsSnapshot,
) -> Option<cef::SidebarRuntimeSettingsSnapshot> {
    (current != &next).then_some(next)
}

