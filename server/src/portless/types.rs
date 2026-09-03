use anyhow::{bail, Result};
use serde::{Deserialize, Serialize};
#[derive(Clone, Debug, Deserialize, PartialEq, Eq, Serialize)]
pub struct PortlessRoute {
    pub hostname: String,
    pub port: u16,
    pub pid: u32,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PortlessOwnedListener {
    pub project_id: String,
    pub session_id: String,
    pub zmx_name: String,
    pub worktree_parent_project_id: Option<String>,
    pub port: u16,
    pub pid: u32,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PortlessBackgroundRouteAction {
    ClearMirroredRoutes,
    MirrorDesiredRoutes,
    SkipRouteFileWrite,
}

impl PortlessBackgroundRouteAction {
    pub(crate) fn as_str(self) -> &'static str {
        match self {
            Self::ClearMirroredRoutes => "clearMirroredRoutes",
            Self::MirrorDesiredRoutes => "mirrorDesiredRoutes",
            Self::SkipRouteFileWrite => "skipRouteFileWrite",
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PortlessBackgroundStatus {
    Disabled,
    SetupActive,
    SetupFailed,
    SetupNeeded,
    SetupUnknown,
}

impl PortlessBackgroundStatus {
    pub(crate) fn as_str(self) -> &'static str {
        match self {
            Self::Disabled => "disabled",
            Self::SetupActive => "setupActive",
            Self::SetupFailed => "setupFailed",
            Self::SetupNeeded => "setupNeeded",
            Self::SetupUnknown => "setupUnknown",
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PortlessBackgroundSyncOutcome {
    pub action: PortlessBackgroundRouteAction,
    pub desired_route_count: usize,
    pub live_listener_count: usize,
    pub status: PortlessBackgroundStatus,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PortlessServiceClassification {
    Missing,
    GhostexActive,
    GhostexConfigMismatch,
    GhostexFailed,
    Standalone,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct PortlessServiceReachability {
    pub manager_running: Option<bool>,
    pub proxy_reachable: Option<bool>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PortlessServiceInspection {
    pub classification: PortlessServiceClassification,
    pub mismatch_count: usize,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PortlessLogErrorCode {
    BackgroundSyncFailed,
    BackgroundSyncTaskJoinFailed,
    StateUpdateDatabaseUnavailable,
    StateUpdateFailed,
}

impl PortlessLogErrorCode {
    pub(crate) fn as_str(self) -> &'static str {
        match self {
            Self::BackgroundSyncFailed => "backgroundSyncFailed",
            Self::BackgroundSyncTaskJoinFailed => "backgroundSyncTaskJoinFailed",
            Self::StateUpdateDatabaseUnavailable => "stateUpdateDatabaseUnavailable",
            Self::StateUpdateFailed => "stateUpdateFailed",
        }
    }
}

#[derive(Clone, Debug, Deserialize, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PortlessStatusPayload {
    pub actions: PortlessAdminActionSet,
    pub enabled: bool,
    pub protocol: PortlessProtocol,
    pub runtime_status: PortlessRuntimeStatus,
    pub setup_ownership: PortlessSetupOwnership,
    pub setup_status: PortlessSetupStatus,
    pub source_status: PortlessPayloadSourceStatus,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub updated_at: Option<String>,
}

/*
CDXC:Portless 2026-06-23-04:28:
Phase 16 makes Portless recovery state daemon-owned: protocol changes, admin
success/failure, retry, disable, and explicit service removal are persisted as
enum metadata so React can recover without reading Portless files or inventing
local fallback state.
*/
#[derive(Clone, Debug, Deserialize, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase", tag = "kind")]
pub enum PortlessStateUpdate {
    SetEnabled {
        enabled: bool,
    },
    SetProtocol {
        protocol: PortlessProtocol,
    },
    RecordAdminResult {
        action: PortlessAdminResultAction,
        ok: bool,
        #[serde(skip_serializing_if = "Option::is_none")]
        protocol: Option<PortlessProtocol>,
    },
}

#[derive(Clone, Copy, Debug, Deserialize, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum PortlessAdminResultAction {
    Install,
    Reconfigure,
    Remove,
    Retry,
}

impl PortlessAdminResultAction {
    pub(crate) fn as_str(self) -> &'static str {
        match self {
            Self::Install => "install",
            Self::Reconfigure => "reconfigure",
            Self::Remove => "remove",
            Self::Retry => "retry",
        }
    }
}

#[derive(Clone, Debug, Deserialize, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PortlessAdminActionSet {
    pub install: PortlessAdminActionAvailability,
    pub reconfigure: PortlessAdminActionAvailability,
    pub remove: PortlessAdminActionAvailability,
    pub retry: PortlessAdminActionAvailability,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PortlessAdminActionAvailability {
    pub available: bool,
    pub local_mac_only: bool,
    pub recommended: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub unavailable_reason: Option<PortlessAdminActionUnavailableReason>,
}

#[derive(Clone, Copy, Debug, Deserialize, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum PortlessAdminActionUnavailableReason {
    NativeAdminBridgeRequired,
    NotRecommended,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PortlessPresentationPayload {
    pub assigned_domains: Vec<PortlessAssignedDomain>,
    pub live_listener_count: usize,
    pub route_preview_status: PortlessRoutePreviewStatus,
    pub route_previews: Vec<PortlessRoutePreview>,
    pub status: PortlessStatusPayload,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PortlessAssignedDomain {
    pub hostname: String,
    pub kind: PortlessAssignedDomainKind,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub parent_project_id: Option<String>,
    pub project_id: String,
}

#[derive(Clone, Copy, Debug, Deserialize, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum PortlessAssignedDomainKind {
    Project,
    Worktree,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PortlessRoutePreview {
    pub hostname: String,
    pub kind: PortlessRoutePreviewKind,
    pub port: u16,
    pub project_id: String,
    pub protocol: PortlessProtocol,
    pub session_id: String,
}

#[derive(Clone, Copy, Debug, Deserialize, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum PortlessRoutePreviewKind {
    Additional,
    Primary,
}

#[derive(Clone, Copy, Debug, Deserialize, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum PortlessRoutePreviewStatus {
    Current,
    Disabled,
    Unavailable,
}

#[derive(Clone, Copy, Debug, Deserialize, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum PortlessPayloadSourceStatus {
    Current,
    Missing,
    Unavailable,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PortlessDomainIdentities {
    pub projects: Vec<PortlessProjectSlug>,
    pub worktrees: Vec<PortlessWorktreeDomainParts>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PortlessWorktreeDomainParts {
    pub parent_project_id: String,
    pub project_slug: String,
    pub worktree_project_id: String,
    pub worktree_key: String,
    pub worktree_slug: String,
    pub created_at: String,
    pub updated_at: String,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PortlessProjectSlug {
    pub project_id: String,
    pub slug: String,
    pub created_at: String,
    pub updated_at: String,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PortlessWorktreeSlug {
    pub project_id: String,
    pub worktree_key: String,
    pub slug: String,
    pub created_at: String,
    pub updated_at: String,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PortlessState {
    pub enabled: bool,
    pub protocol: PortlessProtocol,
    pub setup_ownership: PortlessSetupOwnership,
    pub setup_status: PortlessSetupStatus,
    pub runtime_status: PortlessRuntimeStatus,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PortlessStateRecord {
    pub state: PortlessState,
    pub created_at: String,
    pub updated_at: String,
}

#[derive(Clone, Copy, Debug, Deserialize, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum PortlessProtocol {
    Https,
    Http,
}

impl PortlessProtocol {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Https => "https",
            Self::Http => "http",
        }
    }

    pub(crate) fn from_storage(value: &str) -> Result<Self> {
        match value {
            "https" => Ok(Self::Https),
            "http" => Ok(Self::Http),
            _ => bail!("Invalid Portless protocol metadata value."),
        }
    }
}

#[derive(Clone, Copy, Debug, Deserialize, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum PortlessSetupOwnership {
    Unknown,
    Missing,
    Ghostex,
    Standalone,
}

impl PortlessSetupOwnership {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Unknown => "unknown",
            Self::Missing => "missing",
            Self::Ghostex => "ghostex",
            Self::Standalone => "standalone",
        }
    }

    pub(crate) fn from_storage(value: &str) -> Result<Self> {
        match value {
            "unknown" => Ok(Self::Unknown),
            "missing" => Ok(Self::Missing),
            "ghostex" => Ok(Self::Ghostex),
            "standalone" => Ok(Self::Standalone),
            _ => bail!("Invalid Portless setup ownership metadata value."),
        }
    }
}

#[derive(Clone, Copy, Debug, Deserialize, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum PortlessSetupStatus {
    Unknown,
    Needed,
    Active,
    Failed,
    Disabled,
    Postponed,
}

impl PortlessSetupStatus {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Unknown => "unknown",
            Self::Needed => "needed",
            Self::Active => "active",
            Self::Failed => "failed",
            Self::Disabled => "disabled",
            Self::Postponed => "postponed",
        }
    }

    pub(crate) fn from_storage(value: &str) -> Result<Self> {
        match value {
            "unknown" => Ok(Self::Unknown),
            "needed" => Ok(Self::Needed),
            "active" => Ok(Self::Active),
            "failed" => Ok(Self::Failed),
            "disabled" => Ok(Self::Disabled),
            "postponed" => Ok(Self::Postponed),
            _ => bail!("Invalid Portless setup status metadata value."),
        }
    }
}

#[derive(Clone, Copy, Debug, Deserialize, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum PortlessRuntimeStatus {
    Unknown,
    Inactive,
    Active,
    Failed,
}

impl PortlessRuntimeStatus {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Unknown => "unknown",
            Self::Inactive => "inactive",
            Self::Active => "active",
            Self::Failed => "failed",
        }
    }

    pub(crate) fn from_storage(value: &str) -> Result<Self> {
        match value {
            "unknown" => Ok(Self::Unknown),
            "inactive" => Ok(Self::Inactive),
            "active" => Ok(Self::Active),
            "failed" => Ok(Self::Failed),
            _ => bail!("Invalid Portless runtime status metadata value."),
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct PortlessRouteTarget {
    pub(crate) port: u16,
    pub(crate) pid: u32,
}
