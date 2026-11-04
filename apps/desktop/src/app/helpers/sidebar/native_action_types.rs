// C1 wave-1 deferred split: apps/desktop/src/app/helpers/sidebar.rs (~4.1k
// lines) further divided into responsibility-scoped submodules (pure move,
// no logic changes). This file holds native project-path action types, status-indicator types, and their JSON parsing (status indicators, global actions).
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
    pub(crate) preferred_interface: GpuiPreferredAgentInterface,
    pub(crate) project_id: String,
    /// Only `openRemoteSessionTerminal` carries it; the remote twin of
    /// `GpuiSidebarWorkspaceTerminalFocusMessage::keep_view`.
    pub(crate) keep_view: bool,
    /// Only `openWorkspaceProjectInTarget` and `openRemoteWorkspaceProjectInTarget` carry it: the
    /// id of one visible Open In target.
    pub(crate) target_id: Option<String>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum GpuiSidebarNativeProjectPathAction {
    CopyRecentProjectPath,
    OpenRecentProjectInFinder,
    CopyWorkspaceProjectPath,
    OpenWorkspaceProjectInFinder,
    OpenWorkspaceProjectInIde,
    OpenWorkspaceProjectInTarget,
    OpenActiveWorkspaceProjectInFinder,
    OpenActiveWorkspaceProjectInVscode,
    OpenActiveWorkspaceProjectInZed,
    OpenExistingPullRequestInBrowser,
    OpenSidebarGitChangedFileInIde,
    RevealSidebarGitChangedFile,
    CopyRemoteProjectPath,
    OpenRemoteProjectTerminal,
    OpenRemoteWorkspaceProjectInIde,
    /// A remote project in one Open In target that can open a folder over SSH; carries `targetId`.
    OpenRemoteWorkspaceProjectInTarget,
    /// A remote project's Open Folder: the project in Ghostex's Files view.
    OpenRemoteProjectInFiles,
    OpenRemoteWorkspaceProjectInVscode,
    OpenRemoteWorkspaceProjectInZed,
    OpenRemoteExistingPullRequestInBrowser,
    OpenRemoteSidebarGitChangedFileInIde,
    OpenRemoteProjectPortsBrowser,
    OpenRemoteSessionTerminal,
}

impl GpuiSidebarNativeProjectPathAction {
    pub(crate) fn from_str(value: &str) -> Option<Self> {
        match value {
            "copyRecentProjectPath" => Some(Self::CopyRecentProjectPath),
            "openRecentProjectInFinder" => Some(Self::OpenRecentProjectInFinder),
            "copyWorkspaceProjectPath" => Some(Self::CopyWorkspaceProjectPath),
            "openWorkspaceProjectInFinder" => Some(Self::OpenWorkspaceProjectInFinder),
            "openWorkspaceProjectInIde" => Some(Self::OpenWorkspaceProjectInIde),
            "openWorkspaceProjectInTarget" => Some(Self::OpenWorkspaceProjectInTarget),
            "openActiveWorkspaceProjectInFinder" => Some(Self::OpenActiveWorkspaceProjectInFinder),
            "openActiveWorkspaceProjectInVscode" => Some(Self::OpenActiveWorkspaceProjectInVscode),
            "openActiveWorkspaceProjectInZed" => Some(Self::OpenActiveWorkspaceProjectInZed),
            "openExistingPullRequestInBrowser" => Some(Self::OpenExistingPullRequestInBrowser),
            "openSidebarGitChangedFileInIde" => Some(Self::OpenSidebarGitChangedFileInIde),
            "revealSidebarGitChangedFile" => Some(Self::RevealSidebarGitChangedFile),
            "copyRemoteProjectPath" => Some(Self::CopyRemoteProjectPath),
            "openRemoteProjectTerminal" => Some(Self::OpenRemoteProjectTerminal),
            "openRemoteWorkspaceProjectInIde" => Some(Self::OpenRemoteWorkspaceProjectInIde),
            "openRemoteWorkspaceProjectInTarget" => Some(Self::OpenRemoteWorkspaceProjectInTarget),
            "openRemoteProjectInFiles" => Some(Self::OpenRemoteProjectInFiles),
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
            Self::OpenSidebarGitChangedFileInIde
                | Self::RevealSidebarGitChangedFile
                | Self::OpenRemoteSidebarGitChangedFileInIde
        )
    }

    pub(crate) fn is_remote_session_action(self) -> bool {
        matches!(self, Self::OpenRemoteSessionTerminal)
    }

    pub(crate) fn is_remote_project_action(self) -> bool {
        matches!(
            self,
            Self::CopyRemoteProjectPath
                | Self::OpenRemoteProjectTerminal
                | Self::OpenRemoteWorkspaceProjectInIde
                | Self::OpenRemoteWorkspaceProjectInTarget
                | Self::OpenRemoteProjectInFiles
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
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub(crate) struct GpuiStatusIndicatorSessionState {
    pub(crate) last_active_at: Option<String>,
    pub(crate) order: u64,
    /// An unanswered question card is waiting (the sidebar's pink dot).
    pub(crate) pending_question: bool,
    pub(crate) session_id: String,
    pub(crate) status: GpuiStatusIndicatorStatus,
    pub(crate) title: String,
}

/// Working / attention / question split the way the sidebar's section headers split them; what
/// the Ghostex Capture button shows.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) struct GpuiStatusSectionCounts {
    pub(crate) attention: u64,
    pub(crate) question: u64,
    pub(crate) working: u64,
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
    pub(crate) section_counts: GpuiStatusSectionCounts,
    pub(crate) working_count: u64,
}

pub(crate) fn gpui_sidebar_session_status_indicators_from_json(
    text: &str,
) -> Result<GpuiSidebarSessionStatusIndicatorsState, ()> {
    /*
    CDXC:StatusPet 2026-06-26-04:38:
    Status indicator parser coverage must keep the GPUI bridge fixed and privacy-safe: accept only version/type, enum counts, menu-bar visibility, bounded project/session rows with ids/order/titles, and optional bounded image data URLs for project notification icons. Reject generic action names, paths, external URLs, command text, stdout/stderr, tokens, terminal content, oversized arrays, and unknown keys before app state is updated.

    CDXC:StatusPet 2026-06-27-20:11:
    The standalone GPUI floating session indicator was removed. The status
    bridge still feeds the menu bar item and attention
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
            "sectionCounts",
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
        section_counts: gpui_status_section_counts_field(object)?,
        working_count: gpui_status_count_field(object, "workingCount")?,
    })
}

fn gpui_status_section_counts_field(
    object: &serde_json::Map<String, serde_json::Value>,
) -> Result<GpuiStatusSectionCounts, ()> {
    let counts = object
        .get("sectionCounts")
        .and_then(serde_json::Value::as_object)
        .ok_or(())?;
    reject_unexpected_gpui_status_keys(counts, &["attention", "question", "working"])?;
    Ok(GpuiStatusSectionCounts {
        attention: gpui_status_count_field(counts, "attention")?,
        question: gpui_status_count_field(counts, "question")?,
        working: gpui_status_count_field(counts, "working")?,
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
