// C1 wave-1 deferred split: apps/desktop/src/app/helpers/sidebar.rs (~4.1k
// lines) further divided into responsibility-scoped submodules (pure move,
// no logic changes). This file holds app-modal sidebar state message builders, theme/session-chat settings readers, sidebar width persistence, and JSON/workarea helpers.
// See docs/2026-08-22/repo-restructure/SPLITS.md C1.

// C1 wave-1 extraction: stateless helper functions moved verbatim out of
// main.rs (pure move, no logic changes). See docs/2026-08-22/repo-restructure/SPLITS.md C1.

use std::{fs, path::Path, time::Duration};

// RefCell backs cross-platform runtime state (window frame persistence), not
// just the macOS-only shims that first introduced the import.

use gpui::Window;

use crate::app::helpers::*;
use crate::*;

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
