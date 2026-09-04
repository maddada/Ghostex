// C1 wave-1 deferred split: apps/desktop/src/app/helpers/sidebar.rs (~4.1k
// lines) further divided into responsibility-scoped submodules (pure move,
// no logic changes). This file holds native project-path action types, status-indicator types, and their JSON parsing (status indicators, global actions, pet overlay).
// See docs/2026-08-22/repo-restructure/SPLITS.md C1.

// C1 wave-1 extraction: stateless helper functions moved verbatim out of
// main.rs (pure move, no logic changes). See docs/2026-08-22/repo-restructure/SPLITS.md C1.

// RefCell backs cross-platform runtime state (window frame persistence), not
// just the macOS-only shims that first introduced the import.

use anyhow::Result;

use crate::app::helpers::*;
use crate::*;

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct GpuiSidebarNativeProjectPathActionMessage {
    pub(crate) action: GpuiSidebarNativeProjectPathAction,
    pub(crate) file_path: Option<String>,
    /// Only `openRemoteSessionTerminal` carries it; see `GpuiWorkspaceTerminalFocusPlacement`.
    pub(crate) placement: GpuiWorkspaceTerminalFocusPlacement,
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
    CDXC:StatusPet 2026-06-26-04:38:
    Status indicator parser coverage must keep the GPUI bridge fixed and privacy-safe: accept only version/type, enum counts, menu-bar visibility, bounded project/session rows with ids/order/titles, and optional bounded image data URLs for project notification icons. Reject generic action names, paths, external URLs, command text, stdout/stderr, tokens, terminal content, oversized arrays, and unknown keys before app state is updated.

    CDXC:StatusPet 2026-06-27-20:11:
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
CDXC:AgentLauncher 2026-08-01-16:00:
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

pub(crate) fn gpui_sidebar_pet_overlay_state_from_json(
    text: &str,
) -> Result<GpuiSidebarPetOverlayState, ()> {
    /*
    CDXC:StatusPet 2026-06-26-04:38:
    Pet overlay parser accepts only the saved enabled flag, bounded selected pet id, status items, and explicit project/session activity ids from the fixed sidebar bridge. It must not accept renderer paths, URLs, generic activation payloads, command text, stdout/stderr, tokens, terminal content, or menu-bar status-item data.

    CDXC:StatusPet 2026-06-26-05:30:
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

pub(crate) fn gpui_command_palette_run_sidebar_command_script(
    message: &serde_json::Value,
) -> String {
    format!(
        "(function(){{const bridge=window.ghostexGpui=window.ghostexGpui||{{}};const payload={message};if(typeof bridge.onCommandPaletteRunSidebarCommand==='function'){{bridge.onCommandPaletteRunSidebarCommand(payload);}}else{{const pending=Array.isArray(bridge.pendingCommandPaletteRunSidebarCommands)?bridge.pendingCommandPaletteRunSidebarCommands:[];pending.push(payload);bridge.pendingCommandPaletteRunSidebarCommands=pending;}}}})(); undefined;"
    )
}
