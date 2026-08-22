// C1 wave-3 extraction: the keep-awake, remote/gxserver connection, project-context, and source-code-server runtime value types moved verbatim out of main.rs (pure
// move, no logic changes; items made pub(crate) so main.rs and sibling
// modules can still reach them). See docs/2026-08-22/repo-restructure/SPLITS.md C1.
#![allow(dead_code)]

use crate::*;


#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum GpuiKeepAwakeRuntimeSource {
    Manual,
    Automatic,
}


#[cfg(target_os = "macos")]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum GpuiKeepAwakeLidSleepAction {
    Enable { install_if_needed: bool },
    Disable,
    Heartbeat,
}


pub(crate) struct GpuiKeepAwakeRuntime {
    pub(crate) runtime_id: u64,
    pub(crate) duration_minutes: shared_settings::SharedKeepAwakeDurationMinutes,
    pub(crate) source: GpuiKeepAwakeRuntimeSource,
    pub(crate) started_at: Instant,
    pub(crate) fire_at: Option<Instant>,
    #[cfg(target_os = "macos")]
    pub(crate) child: std::process::Child,
    #[cfg(target_os = "macos")]
    pub(crate) lid_sleep_prevention_enabled: bool,
    #[cfg(target_os = "macos")]
    pub(crate) lid_sleep_prevention_install_attempted: bool,
    #[cfg(target_os = "macos")]
    pub(crate) lid_sleep_prevention_update_in_flight: bool,
    #[cfg(target_os = "macos")]
    pub(crate) lid_sleep_prevention_warning_sent: bool,
    #[cfg(target_os = "macos")]
    pub(crate) lid_sleep_prevention_last_refresh_at: Option<Instant>,
}


/*
CDXC:GPUIRemoteNewTerminal 2026-08-20:
A remote machine runs the gxserver package this app installed for it, so it can
implement fewer operations than the app that is driving it. The authenticated
`/api/health/server` probe this connection already performs carries the daemon's
own capability inventory, so record the bounded selectors remote requests have to
choose between instead of sending a newer selector and losing the whole action to
a 400. Keep this to fixed capability names: no daemon paths, ports, tokens, or
response text may be retained here.
*/
#[derive(Clone, Copy, Debug, Default)]
pub(crate) struct GpuiRemoteGxserverCapabilities {
    pub(crate) code_server_prompt_editor: bool,
}


pub(crate) struct GpuiRemoteGxserverConnection {
    pub(crate) _base_url: String,
    pub(crate) capabilities: GpuiRemoteGxserverCapabilities,
    pub(crate) code_server_component_platform: Option<String>,
    pub(crate) execution_target: GpuiRemoteExecutionTarget,
    pub(crate) local_port: u16,
    pub(crate) presentation_stream_cancel: Option<Arc<AtomicBool>>,
    pub(crate) presentation_stream_generation: Option<u64>,
    pub(crate) token: String,
    pub(crate) child: Child,
    pub(crate) health_check_failures: u8,
}


impl GpuiRemoteGxserverConnection {
    pub(crate) fn request_target(&self) -> GpuiRemoteGxserverRequestTarget {
        GpuiRemoteGxserverRequestTarget {
            capabilities: self.capabilities,
            code_server_component_platform: self.code_server_component_platform.clone(),
            execution_target: self.execution_target.clone(),
            local_port: self.local_port,
            token: self.token.clone(),
        }
    }

    pub(crate) fn terminate(&mut self) {
        if let Some(cancel) = self.presentation_stream_cancel.as_ref() {
            cancel.store(true, Ordering::SeqCst);
        }
        let _ = self.child.kill();
    }
}


#[derive(Clone)]
pub(crate) struct GpuiRemoteGxserverRequestTarget {
    pub(crate) capabilities: GpuiRemoteGxserverCapabilities,
    pub(crate) code_server_component_platform: Option<String>,
    pub(crate) execution_target: GpuiRemoteExecutionTarget,
    pub(crate) local_port: u16,
    pub(crate) token: String,
}


pub(crate) enum GpuiRemoteGxserverPresentationStreamMessage {
    Event(serde_json::Value),
    Failed,
}


#[derive(Clone)]
pub(crate) struct GpuiRemoteRepositoryCloneRequest {
    pub(crate) job_id: String,
    pub(crate) remote_machine_id: Option<String>,
    pub(crate) toast_id: String,
}


#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum GpuiProjectContext {
    Project,
    QuickProjectless,
}


impl GpuiProjectContext {
    pub(crate) fn from_project_is_quick_bridge(project_is_quick: bool) -> Self {
        if project_is_quick {
            Self::QuickProjectless
        } else {
            Self::Project
        }
    }

    pub(crate) fn from_env_bridge() -> Self {
        let value = env::var(GPUI_PROJECT_IS_QUICK_ENV).ok();
        gpui_project_context_from_env_bridge_value(value.as_deref())
    }

    pub(crate) fn has_project_scoped_workareas(self) -> bool {
        matches!(self, Self::Project)
    }
}


/*
CDXC:GPUIProjectSnapshot 2026-06-24-07:41:
The live sidebar active-project snapshot is strict instead of a pre-bridge placeholder. The snapshot carries active project id, display name, Quick/projectless state, project-scoped availability, the allowlisted in-memory project path, and identity-only Source, Kanban, Automate, and gated Manage surface ids from explicit sidebar/native project-editor state without inventing .git, path, fixture, workspace-name, or fallback project detection. Browser runtime state plus Source/Kanban/Automate/Manage runtime URL, CEF, and file-bridge facts stay outside the snapshot; real workarea surfaces are created only through direct runtime gates after snapshot acceptance.

CDXC:GPUIProjectSnapshot 2026-06-22-18:14:
Project display names and project paths are private runtime facts. `in_memory_project_path` is accepted only from the future allowlisted sidebar contract, is not normalized or probed on disk, and must not be serialized by GPUI shell-state persistence or emitted in logs; durable shell state may only store privacy-boundary booleans/count-like facts unless a later requirement explicitly adds a sanitized field.

CDXC:GPUIProjectSnapshotContract 2026-06-22-18:14:
The staged sidebar message contract is deliberately narrow: version 1, type `ghostex.gpui.sidebar.activeProjectContext`, and one `activeProject` object with explicit allowlisted fields. Reject non-object JSON, malformed booleans/strings, unknown keys, unsupported versions, unexpected message types, and Quick/projectless payloads that still carry project ids, paths, project-scoped surface ids, or enabled project-only workareas.

CDXC:GPUIProjectSidebarBridge 2026-06-23-06:53:
Active-project change semantics need deterministic duplicate handling: valid payloads are accepted, but only snapshots that differ from the stored runtime snapshot may replace it or trigger titlebar label, mode-availability coercion, and render notification work.

CDXC:GPUISourceWorkarea 2026-06-23-12:16:
Source mounting may only use explicit active-project and Source surface identity from the sidebar snapshot. The active-project payload still does not carry runtime Source instantiation data, so GPUI must keep Source on the existing placeholder path instead of deriving readiness, URLs, paths, .git, labels, fixtures, filesystem probes, or localhost constants.

CDXC:GPUISourceWorkarea 2026-06-23-12:25:
Normal sidebar project payloads may now carry the explicit Source workarea identity from the sidebar/native project-editor id. Missing or malformed Source identity still blocks without deriving ids or readiness from paths, titles, fixtures, probes, group ids, URLs, or localhost constants; runtime Source instantiation remains outside the active-project snapshot.

CDXC:GPUISourceWorkarea 2026-06-24-07:41:
Source identity is not runtime authority. The app-owned code-server owner must turn the snapshot into the only Source runtime URL gate; this boundary must not treat raw URLs, localhost values, paths, filesystem probes, sidebar readiness messages, or placeholder shell state as readiness, mount permission, or placeholder replacement.

CDXC:GPUISourceRuntime 2026-06-24-23:17:
GPUI Source runtime authority now starts after this snapshot boundary: the snapshot may supply only explicit project identity, Source workarea identity, and in-memory project path; the app-owned runtime owner turns that into the macOS-compatible code-server folder URL only at the visible Source startup edge.

CDXC:GPUISourceWorkarea 2026-06-23-14:36:
Source sleep/wake evidence must preserve explicit Source runtime identity while keeping code-server launch state separate from shell lifecycle. Shell sleep/wake may only toggle the placeholder lifecycle; it must not synthesize Source readiness, mount CEF/code-server, persist private ids/paths/URLs, or reset companion and command-pane shell state.

CDXC:GPUIProjectSnapshotContract 2026-06-23-15:18:
The active-project snapshot accepts identity-only Source, Kanban, Automate, and gated Manage surface ids, but Browser surface identity is not part of the snapshot. Browser availability may still gate the titlebar; any `browserWorkareaId` field must be rejected instead of stored as speculative identity.

CDXC:GPUIProjectWorkareaRuntimeCleanup 2026-06-29-00:02:
Browser active-project readiness no longer has a GPUI proof store. Keep `browserWorkareaId` rejected in `surfaceIds`; Source/Kanban/Automate/Manage readiness messages are compatibility no-ops, and real workarea surfaces depend on direct runtime URL plus CEF surface ownership.
*/
#[allow(dead_code)]
#[derive(Clone, PartialEq, Eq)]
pub(crate) struct GpuiProjectId(pub(crate) String);


#[allow(dead_code)]
#[derive(Clone, PartialEq, Eq)]
pub(crate) struct GpuiProjectSurfaceIds {
    pub(crate) source_workarea_id: Option<String>,
    pub(crate) kanban_board_id: Option<String>,
    pub(crate) automate_board_id: Option<String>,
    pub(crate) manage_workspace_id: Option<String>,
}


#[allow(dead_code)]
impl GpuiProjectSurfaceIds {
    pub(crate) fn has_any(&self) -> bool {
        self.source_workarea_id.is_some()
            || self.kanban_board_id.is_some()
            || self.automate_board_id.is_some()
            || self.manage_workspace_id.is_some()
    }

    pub(crate) fn has_ids_for_unavailable_features(
        &self,
        feature_availability: GpuiProjectScopedFeatureAvailability,
    ) -> bool {
        /*
        CDXC:GPUIProjectSnapshotContract 2026-06-23-13:01:
        Surface ids must agree with explicit workarea availability. The sidebar may send Kanban and Automate identity only for project contexts. Docs identity is accepted independently of the old beta/debug titlebar gate so the Rust titlebar can expose Docs consistently while still relying on the direct runtime URL gate before any CEF surface is used.
        */
        (!feature_availability.source && self.source_workarea_id.is_some())
            || (!feature_availability.kanban && self.kanban_board_id.is_some())
            || (!feature_availability.automate && self.automate_board_id.is_some())
    }
}


#[allow(dead_code)]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct GpuiProjectScopedFeatureAvailability {
    pub(crate) source: bool,
    pub(crate) browser: bool,
    pub(crate) kanban: bool,
    pub(crate) automate: bool,
    pub(crate) manage: bool,
}


#[allow(dead_code)]
impl GpuiProjectScopedFeatureAvailability {
    pub(crate) fn for_project_context(project_context: GpuiProjectContext) -> Self {
        let has_project = project_context.has_project_scoped_workareas();
        Self {
            source: true,
            browser: has_project,
            kanban: has_project,
            automate: has_project,
            manage: has_project,
        }
    }

    pub(crate) fn is_quick_projectless_compatible(self) -> bool {
        self.source && !self.browser && !self.kanban && !self.automate && !self.manage
    }

    pub(crate) fn shell_state_privacy_boundary_json(self) -> serde_json::Value {
        serde_json::json!({
            "source": self.source,
            "browser": self.browser,
            "kanban": self.kanban,
            "automate": self.automate,
            "manage": self.manage,
        })
    }
}


#[allow(dead_code)]
#[derive(Clone, PartialEq, Eq)]
pub(crate) struct GpuiProjectSnapshot {
    pub(crate) active_project_id: Option<GpuiProjectId>,
    pub(crate) selection_owner_project_id: Option<GpuiProjectId>,
    pub(crate) display_name: String,
    pub(crate) project_icon_data_url: Option<String>,
    pub(crate) in_memory_project_path: Option<PathBuf>,
    pub(crate) is_quick_projectless: bool,
    pub(crate) feature_availability: GpuiProjectScopedFeatureAvailability,
    pub(crate) surface_ids: GpuiProjectSurfaceIds,
}


#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub(crate) struct GpuiGxserverPresentationFocusState {
    pub(crate) active_project_id: Option<String>,
    pub(crate) active_project_tab_sessions: Option<Vec<GpuiSidebarWorkspaceTabSession>>,
    pub(crate) focused_session_id: Option<String>,
    pub(crate) visible_session_ids: Vec<String>,
}


#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum GpuiProjectSwitchRequestKind {
    ActiveProjectContext,
    GxserverPresentationFocusState,
    WorkspaceTerminalFocus,
}


impl GpuiProjectSwitchRequestKind {
    pub(crate) fn breadcrumb_id(self) -> &'static str {
        match self {
            Self::ActiveProjectContext => "activeProjectContext",
            Self::GxserverPresentationFocusState => "gxserverPresentationFocusState",
            Self::WorkspaceTerminalFocus => "workspaceTerminalFocus",
        }
    }

    /// Only the two authoritative project snapshots perform the swap itself.
    /// The imperative session-focus requests merely have to land after it, so
    /// they never open a settle window of their own.
    pub(crate) fn opens_settle_window(self) -> bool {
        matches!(
            self,
            Self::ActiveProjectContext | Self::GxserverPresentationFocusState
        )
    }
}


/// A sidebar request collapsed into the trailing edge of a project switch.
/// `ActiveProjectContext` carries no payload because the parsed snapshot is
/// already stored in `latest_sidebar_project_snapshot`; replaying the marker
/// applies whatever the newest stored snapshot is.
#[derive(Clone, Debug)]
pub(crate) enum GpuiPendingProjectSwitchPayload {
    ActiveProjectContext,
    GxserverPresentationFocusState(GpuiGxserverPresentationFocusState),
    WorkspaceTerminalFocus(GpuiSidebarWorkspaceTerminalFocusMessage),
}


impl GpuiPendingProjectSwitchPayload {
    pub(crate) fn kind(&self) -> GpuiProjectSwitchRequestKind {
        match self {
            Self::ActiveProjectContext => GpuiProjectSwitchRequestKind::ActiveProjectContext,
            Self::GxserverPresentationFocusState(_) => {
                GpuiProjectSwitchRequestKind::GxserverPresentationFocusState
            }
            Self::WorkspaceTerminalFocus(_) => GpuiProjectSwitchRequestKind::WorkspaceTerminalFocus,
        }
    }
}


#[derive(Clone, Debug)]
pub(crate) struct GpuiPendingProjectSwitchRequest {
    pub(crate) target_project_id: Option<String>,
    pub(crate) payload: GpuiPendingProjectSwitchPayload,
}


#[allow(dead_code)]
impl GpuiProjectSnapshot {
    pub(crate) fn project_context(&self) -> GpuiProjectContext {
        if self.is_quick_projectless {
            GpuiProjectContext::QuickProjectless
        } else {
            GpuiProjectContext::Project
        }
    }

    pub(crate) fn titlebar_availability(&self) -> ProjectScopedWorkareaAvailability {
        ProjectScopedWorkareaAvailability::from_project_snapshot(self)
    }

    pub(crate) fn shell_state_privacy_boundary_json(&self) -> serde_json::Value {
        serde_json::json!({
            "hasActiveProject": self.active_project_id.is_some(),
            "isQuickProjectless": self.is_quick_projectless,
            "workareaAvailability": self.feature_availability.shell_state_privacy_boundary_json(),
        })
    }
}


#[allow(dead_code)]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum GpuiProjectSnapshotContractError {
    MalformedJson,
    ExpectedObject,
    UnexpectedKey,
    MissingField,
    MalformedField,
    UnexpectedVersion,
    UnexpectedMessageType,
    InconsistentProjectContext,
}


#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum GpuiGxserverPresentationFocusStateContractError {
    MalformedJson,
    ExpectedObject,
    UnexpectedKey,
    MissingField,
    MalformedField,
    UnexpectedVersion,
    UnexpectedMessageType,
}


#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum GpuiProjectSnapshotStoreResult {
    Changed,
    Unchanged,
}


#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct ProjectScopedWorkareaAvailability {
    pub(crate) project_context: GpuiProjectContext,
    pub(crate) project_features: GpuiProjectScopedFeatureAvailability,
    pub(crate) active_project_is_remote: bool,
}


impl ProjectScopedWorkareaAvailability {
    pub(crate) fn from_env_bridge() -> Self {
        Self::new(GpuiProjectContext::from_env_bridge())
    }

    pub(crate) fn new(project_context: GpuiProjectContext) -> Self {
        Self {
            project_context,
            project_features: GpuiProjectScopedFeatureAvailability::for_project_context(
                project_context,
            ),
            active_project_is_remote: false,
        }
    }

    #[allow(dead_code)]
    pub(crate) fn from_project_snapshot(snapshot: &GpuiProjectSnapshot) -> Self {
        Self {
            project_context: snapshot.project_context(),
            project_features: snapshot.feature_availability,
            active_project_is_remote: snapshot.active_project_id.as_ref().is_some_and(
                |project_id| {
                    gpui_remote_project_reference_from_project_id(project_id.0.as_str()).is_some()
                },
            ),
        }
    }

    pub(crate) fn titlebar_mode_available(self, mode: TitlebarMode) -> bool {
        /*
        CDXC:GPUIProjectRouting 2026-07-04-01:00:
        Titlebar availability mirrors macOS: Agents and Source are always selectable; Browser, Kanban, Automate, and Docs are visible for all contexts but selectable only for real project-scoped contexts. The old Docs/Manage debuggingMode plus showBetaFeatures visibility gate must not participate in switcher visibility, activation guards, restored active-mode coercion, or persisted active-mode fallback.

        CDXC:GPUTitlebarAvailability 2026-08-20:
        Source is the one exception: a machine-scoped remote project has no
        working Source runtime, so it is unavailable there.
        */
        match mode {
            TitlebarMode::Agents => true,
            TitlebarMode::Source => self.project_features.source && !self.active_project_is_remote,
            TitlebarMode::Browser | TitlebarMode::Kanban | TitlebarMode::Automate => {
                self.project_context.has_project_scoped_workareas()
                    && match mode {
                        TitlebarMode::Browser => self.project_features.browser,
                        TitlebarMode::Kanban => self.project_features.kanban,
                        TitlebarMode::Automate => self.project_features.automate,
                        _ => false,
                    }
            }
            TitlebarMode::Manage => self.project_context.has_project_scoped_workareas(),
        }
    }

    pub(crate) fn available_titlebar_mode_or_agents(self, mode: TitlebarMode) -> TitlebarMode {
        if self.titlebar_mode_available(mode) {
            mode
        } else {
            TitlebarMode::Agents
        }
    }

    pub(crate) fn titlebar_mode_switcher_items(self) -> Vec<TitlebarModeSwitcherItem> {
        let modes = vec![
            TitlebarMode::Agents,
            TitlebarMode::Source,
            TitlebarMode::Browser,
            TitlebarMode::Kanban,
            TitlebarMode::Automate,
            TitlebarMode::Manage,
        ];

        modes
            .into_iter()
            .map(|mode| {
                let is_available = self.titlebar_mode_available(mode);
                let disabled_reason = if is_available {
                    None
                } else if mode == TitlebarMode::Source && self.active_project_is_remote {
                    Some(TITLEBAR_REMOTE_SOURCE_DISABLED_REASON)
                } else if matches!(self.project_context, GpuiProjectContext::QuickProjectless)
                    && matches!(
                        mode,
                        TitlebarMode::Browser
                            | TitlebarMode::Kanban
                            | TitlebarMode::Automate
                            | TitlebarMode::Manage
                    ) {
                    Some(TITLEBAR_PROJECT_CONTEXT_DISABLED_REASON)
                } else {
                    None
                };
                TitlebarModeSwitcherItem {
                    mode,
                    is_available,
                    disabled_reason,
                }
            })
            .collect()
    }
}


/*
CDXC:GPUISourceRuntimeCleanup 2026-06-28-17:09:
Keep only the lean runtime value types needed to launch Source and create Source/Kanban/Automate/Manage CEF surfaces. These structs are process-local implementation state, not retired proof records, and must not grow JSON status APIs, persisted URL fields, private logging, fallback navigation, or placeholder-preflight evidence.
*/
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub(crate) enum ProjectWorkareaCefSurfaceSlotKey {
    Source,
    Kanban,
    Automate,
    Manage,
}


impl ProjectWorkareaCefSurfaceSlotKey {
    pub(crate) fn project_placeholder_slots() -> [Self; 4] {
        [Self::Source, Self::Kanban, Self::Automate, Self::Manage]
    }

    pub(crate) fn privacy_label(self) -> &'static str {
        match self {
            Self::Source => "source",
            Self::Kanban => "kanban",
            Self::Automate => "automate",
            Self::Manage => "manage",
        }
    }

    pub(crate) fn titlebar_mode(self) -> TitlebarMode {
        match self {
            Self::Source => TitlebarMode::Source,
            Self::Kanban => TitlebarMode::Kanban,
            Self::Automate => TitlebarMode::Automate,
            Self::Manage => TitlebarMode::Manage,
        }
    }

    pub(crate) fn cef_surface_id(self) -> String {
        format!("project-workarea-{}", self.privacy_label())
    }

    pub(crate) fn cef_profile_id(self) -> String {
        format!("project-workarea-{}", self.privacy_label())
    }
}


#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct ProjectWorkareaRealRuntimeUrl {
    pub(crate) value: String,
}


impl ProjectWorkareaRealRuntimeUrl {
    pub(crate) fn from_authorized_runtime_url(value: String) -> Option<Self> {
        let trimmed = value.trim();
        if trimmed.is_empty() || trimmed != value {
            return None;
        }

        let (scheme, rest) = trimmed.split_once("://")?;
        let scheme = scheme.to_ascii_lowercase();
        if !matches!(scheme.as_str(), "http" | "https" | "file") || rest.trim().is_empty() {
            return None;
        }

        Some(Self { value })
    }

    pub(crate) fn into_cef_url(self) -> String {
        self.value
    }
}


/*
CDXC:GPUIProjectWorkareaRuntimeCefSurfaces 2026-06-29-00:15:
Project workarea CEF ownership keeps only the process-local direct runtime URL identity beside the CefSurface so active-project changes can prune stale slot reuse. This identity is not a readiness/proof store, must not be serialized or logged, and must only be compared against `project_workarea_runtime_url_for_slot`.
*/
pub(crate) struct ProjectWorkareaRuntimeCefSurface {
    pub(crate) runtime_url: ProjectWorkareaRealRuntimeUrl,
    pub(crate) surface: Entity<CefSurface>,
}


impl ProjectWorkareaRuntimeCefSurface {
    pub(crate) fn matches_runtime_url(&self, runtime_url: &ProjectWorkareaRealRuntimeUrl) -> bool {
        self.runtime_url.eq(runtime_url)
    }
}


#[derive(Clone, PartialEq, Eq)]
pub(crate) struct SourceCodeServerRuntimeTarget {
    pub(crate) active_project_id: GpuiProjectId,
    pub(crate) source_workarea_id: String,
    pub(crate) project_path: PathBuf,
    pub(crate) endpoint: SourceCodeServerRuntimeEndpoint,
}


#[derive(Clone, PartialEq, Eq)]
pub(crate) enum SourceCodeServerRuntimeEndpoint {
    Local,
    Remote {
        component_platform: String,
        connection_generation: u64,
        execution_target: GpuiRemoteExecutionTarget,
        machine_config: GpuiRemoteMachineConfig,
        remote_machine_id: String,
    },
}


impl SourceCodeServerRuntimeTarget {
    pub(crate) fn component_platform(&self) -> Option<&str> {
        match &self.endpoint {
            SourceCodeServerRuntimeEndpoint::Local => None,
            SourceCodeServerRuntimeEndpoint::Remote {
                component_platform, ..
            } => Some(component_platform.as_str()),
        }
    }

    pub(crate) fn can_share_runtime_with(&self, other: &Self) -> bool {
        match (&self.endpoint, &other.endpoint) {
            (SourceCodeServerRuntimeEndpoint::Local, SourceCodeServerRuntimeEndpoint::Local) => {
                true
            }
            (
                SourceCodeServerRuntimeEndpoint::Remote {
                    connection_generation: left_generation,
                    remote_machine_id: left_machine,
                    ..
                },
                SourceCodeServerRuntimeEndpoint::Remote {
                    connection_generation: right_generation,
                    remote_machine_id: right_machine,
                    ..
                },
            ) => left_generation == right_generation && left_machine == right_machine,
            _ => false,
        }
    }
}


#[derive(Clone, Copy, PartialEq, Eq)]
pub(crate) enum RemotePromptEditorDeliveryTarget {
    #[cfg(target_os = "macos")]
    NativeTerminal(FocusedTerminalTextMountTarget),
    GpuiEngineTerminal {
        target: GpuiEngineTerminalEventTarget,
        runtime_session_id: AgentsTerminalRuntimeSessionId,
    },
    #[cfg(target_os = "macos")]
    NativeView(usize),
}


#[derive(Clone, PartialEq, Eq)]
pub(crate) struct PendingRemotePromptEditorRequest {
    pub(crate) shell_session_id: TerminalSessionId,
    pub(crate) remote_key: GpuiRemoteAttachSessionKey,
    pub(crate) connection_generation: u64,
    pub(crate) source_target: SourceCodeServerRuntimeTarget,
    pub(crate) source_runtime_generation: u64,
    pub(crate) delivery_target: RemotePromptEditorDeliveryTarget,
}


#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct SourceCodeServerRuntimeSettings {
    pub(crate) link_vscode_user_config: bool,
    pub(crate) use_vscode_insiders_user_config: bool,
    pub(crate) vscode_user_config_dir: String,
}


impl SourceCodeServerRuntimeSettings {
    pub(crate) fn from_shared_settings(settings: &shared_settings::SharedSidebarSettingsSnapshot) -> Self {
        let link_vscode_user_config = settings
            .object()
            .get("codeServerLinkVscodeUserConfig")
            .and_then(serde_json::Value::as_bool)
            .unwrap_or(false);
        let use_vscode_insiders_user_config = settings
            .object()
            .get("codeServerUseVscodeInsidersUserConfig")
            .and_then(serde_json::Value::as_bool)
            .unwrap_or(false);
        let app_name = if use_vscode_insiders_user_config {
            "Code - Insiders"
        } else {
            "Code"
        };
        Self {
            link_vscode_user_config,
            use_vscode_insiders_user_config,
            vscode_user_config_dir: gpui_path_string(
                &home_dir().join(format!("Library/Application Support/{app_name}/User")),
            ),
        }
    }

    pub(crate) fn from_sidebar_runtime_settings(settings: &cef::SidebarRuntimeSettingsSnapshot) -> Self {
        let object = serde_json::from_str::<serde_json::Value>(&settings.saved_settings_json)
            .ok()
            .and_then(|value| value.as_object().cloned())
            .unwrap_or_default();
        Self::from_shared_settings(
            &shared_settings::SharedSidebarSettingsSnapshot::from_object(object),
        )
    }

    pub(crate) fn linked_vscode_user_config_dir(&self) -> Option<&str> {
        self.link_vscode_user_config
            .then_some(self.vscode_user_config_dir.as_str())
    }
}


#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) enum SourceCodeServerRuntimeLaunchState {
    #[default]
    Idle,
    InstallRequired,
    Installing,
    Launching,
    Ready,
    Failed,
}


#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum SourceCodeServerRuntimeFailure {
    InstallDownload,
    InstallIntegrity,
    InstallOther,
    Launch,
}


impl SourceCodeServerRuntimeFailure {
    pub(crate) fn placeholder_message(self) -> &'static str {
        match self {
            Self::InstallDownload => {
                "The VS Code IDE component couldn’t be downloaded. Check your connection and try again."
            }
            Self::InstallIntegrity => {
                "The VS Code IDE component failed verification and was not installed. Try again."
            }
            Self::InstallOther => "The VS Code IDE component couldn’t be installed. Try again.",
            Self::Launch => "VS Code didn’t start in time.",
        }
    }
}


pub(crate) struct SourceCodeServerRuntimeOwner {
    pub(crate) child: Option<Child>,
    pub(crate) failure: Option<SourceCodeServerRuntimeFailure>,
    pub(crate) install_progress: Option<component_store::ComponentStoreProgressPhase>,
    pub(crate) started_at: Option<Instant>,
    pub(crate) generation: u64,
    pub(crate) state: SourceCodeServerRuntimeLaunchState,
    pub(crate) target: Option<SourceCodeServerRuntimeTarget>,
    pub(crate) settings: Option<SourceCodeServerRuntimeSettings>,
    pub(crate) runtime_origin: Option<String>,
    pub(crate) prompt_editor_ipc_ready: bool,
    pub(crate) pending_remote_prompt_editor_request: Option<PendingRemotePromptEditorRequest>,
}


impl SourceCodeServerRuntimeOwner {
    pub(crate) fn new() -> Self {
        Self {
            child: None,
            failure: None,
            install_progress: None,
            started_at: None,
            generation: 0,
            state: SourceCodeServerRuntimeLaunchState::Idle,
            target: None,
            settings: None,
            runtime_origin: None,
            prompt_editor_ipc_ready: false,
            pending_remote_prompt_editor_request: None,
        }
    }

    pub(crate) fn next_generation(&mut self) -> u64 {
        self.generation = self.generation.saturating_add(1);
        self.generation
    }

    pub(crate) fn queue_remote_prompt_editor_request(&mut self, request: PendingRemotePromptEditorRequest) {
        self.pending_remote_prompt_editor_request = Some(request);
    }

    pub(crate) fn cancel_remote_prompt_editor_request_for_shell_session(
        &mut self,
        shell_session_id: TerminalSessionId,
    ) {
        if self
            .pending_remote_prompt_editor_request
            .as_ref()
            .is_some_and(|request| request.shell_session_id == shell_session_id)
        {
            self.pending_remote_prompt_editor_request = None;
        }
    }

    pub(crate) fn owns_ready_remote_prompt_editor_ipc(
        &self,
        request: &PendingRemotePromptEditorRequest,
    ) -> bool {
        self.state == SourceCodeServerRuntimeLaunchState::Ready
            && self.generation == request.source_runtime_generation
            && self.target.as_ref() == Some(&request.source_target)
            && self.child.is_some()
            && self.prompt_editor_ipc_ready
            && self
                .runtime_origin
                .as_deref()
                .is_some_and(|origin| !origin.trim().is_empty())
            && request.source_target.active_project_id.0
                == gpui_remote_scoped_project_id(
                    request.remote_key.remote_machine_id.as_str(),
                    request.remote_key.project_id.as_str(),
                )
            && matches!(
                &request.source_target.endpoint,
                SourceCodeServerRuntimeEndpoint::Remote {
                    connection_generation,
                    remote_machine_id,
                    ..
                } if remote_machine_id == &request.remote_key.remote_machine_id
                    && *connection_generation == request.connection_generation
            )
    }

    pub(crate) fn runtime_url_for_target(
        &self,
        target: &SourceCodeServerRuntimeTarget,
    ) -> Option<ProjectWorkareaRealRuntimeUrl> {
        (self.state == SourceCodeServerRuntimeLaunchState::Ready
            && self.target.as_ref() == Some(target)
            && self.child.is_some()
            && self.prompt_editor_ipc_ready)
            .then(|| {
                source_code_server_runtime_url(
                    self.runtime_origin.as_deref()?,
                    target.project_path.as_path(),
                )
            })
            .flatten()
    }

    pub(crate) fn can_reuse_ready_process(
        &self,
        target: &SourceCodeServerRuntimeTarget,
        settings: &SourceCodeServerRuntimeSettings,
    ) -> bool {
        self.state == SourceCodeServerRuntimeLaunchState::Ready
            && self.settings.as_ref() == Some(settings)
            && self.child.is_some()
            && self.prompt_editor_ipc_ready
            && self.runtime_origin.is_some()
            && self
                .target
                .as_ref()
                .is_some_and(|current| current.can_share_runtime_with(target))
    }

    pub(crate) fn launching_matches(
        &self,
        target: &SourceCodeServerRuntimeTarget,
        settings: &SourceCodeServerRuntimeSettings,
    ) -> bool {
        self.state == SourceCodeServerRuntimeLaunchState::Launching
            && self.target.as_ref() == Some(target)
            && self.settings.as_ref() == Some(settings)
    }

    pub(crate) fn launching_can_share(
        &self,
        target: &SourceCodeServerRuntimeTarget,
        settings: &SourceCodeServerRuntimeSettings,
    ) -> bool {
        self.state == SourceCodeServerRuntimeLaunchState::Launching
            && self.settings.as_ref() == Some(settings)
            && self
                .target
                .as_ref()
                .is_some_and(|current| current.can_share_runtime_with(target))
    }

    pub(crate) fn child_is_within_startup_grace(&self) -> bool {
        self.child.is_some()
            && self
                .started_at
                .is_some_and(|started_at| started_at.elapsed() < SOURCE_CODE_SERVER_STARTUP_TIMEOUT)
    }

    pub(crate) fn set_launching(
        &mut self,
        target: SourceCodeServerRuntimeTarget,
        settings: SourceCodeServerRuntimeSettings,
        started_at: Instant,
    ) {
        self.pending_remote_prompt_editor_request = None;
        self.state = SourceCodeServerRuntimeLaunchState::Launching;
        self.failure = None;
        self.install_progress = None;
        self.target = Some(target);
        self.settings = Some(settings);
        self.started_at = Some(started_at);
        self.runtime_origin = None;
        self.prompt_editor_ipc_ready = false;
    }

    pub(crate) fn set_ready_target(
        &mut self,
        target: SourceCodeServerRuntimeTarget,
        settings: SourceCodeServerRuntimeSettings,
    ) {
        self.state = SourceCodeServerRuntimeLaunchState::Ready;
        self.failure = None;
        self.install_progress = None;
        self.target = Some(target);
        self.settings = Some(settings);
        if self.started_at.is_none() {
            self.started_at = Some(Instant::now());
        }
    }

    pub(crate) fn set_ready(
        &mut self,
        target: SourceCodeServerRuntimeTarget,
        settings: SourceCodeServerRuntimeSettings,
        child: Child,
        started_at: Instant,
        runtime_origin: String,
        prompt_editor_ipc_ready: bool,
    ) {
        self.replace_child(child);
        self.started_at = Some(started_at);
        self.runtime_origin = Some(runtime_origin);
        self.prompt_editor_ipc_ready = prompt_editor_ipc_ready;
        self.set_ready_target(target, settings);
    }

    pub(crate) fn set_failed(
        &mut self,
        target: SourceCodeServerRuntimeTarget,
        settings: SourceCodeServerRuntimeSettings,
        child: Option<Child>,
        started_at: Option<Instant>,
        failure: SourceCodeServerRuntimeFailure,
    ) {
        self.pending_remote_prompt_editor_request = None;
        if let Some(child) = child {
            self.replace_child(child);
        }
        self.started_at = started_at;
        self.state = SourceCodeServerRuntimeLaunchState::Failed;
        self.failure = Some(failure);
        self.install_progress = None;
        self.target = Some(target);
        self.settings = Some(settings);
        self.runtime_origin = None;
        self.prompt_editor_ipc_ready = false;
    }

    pub(crate) fn set_install_required(
        &mut self,
        target: SourceCodeServerRuntimeTarget,
        settings: SourceCodeServerRuntimeSettings,
    ) {
        self.pending_remote_prompt_editor_request = None;
        self.state = SourceCodeServerRuntimeLaunchState::InstallRequired;
        self.failure = None;
        self.install_progress = None;
        self.started_at = None;
        self.target = Some(target);
        self.settings = Some(settings);
        self.runtime_origin = None;
        self.prompt_editor_ipc_ready = false;
    }

    pub(crate) fn set_installing(
        &mut self,
        target: Option<SourceCodeServerRuntimeTarget>,
        settings: Option<SourceCodeServerRuntimeSettings>,
    ) {
        self.pending_remote_prompt_editor_request = None;
        self.state = SourceCodeServerRuntimeLaunchState::Installing;
        self.failure = None;
        self.install_progress = Some(component_store::ComponentStoreProgressPhase::Checking);
        self.started_at = Some(Instant::now());
        self.target = target;
        self.settings = settings;
        self.runtime_origin = None;
        self.prompt_editor_ipc_ready = false;
    }

    pub(crate) fn set_install_failed(
        &mut self,
        target: Option<SourceCodeServerRuntimeTarget>,
        settings: Option<SourceCodeServerRuntimeSettings>,
        failure: SourceCodeServerRuntimeFailure,
    ) {
        self.pending_remote_prompt_editor_request = None;
        self.state = SourceCodeServerRuntimeLaunchState::Failed;
        self.failure = Some(failure);
        self.install_progress = None;
        self.started_at = None;
        self.target = target;
        self.settings = settings;
        self.runtime_origin = None;
        self.prompt_editor_ipc_ready = false;
    }

    pub(crate) fn reset_after_install(&mut self) {
        self.pending_remote_prompt_editor_request = None;
        self.started_at = None;
        self.state = SourceCodeServerRuntimeLaunchState::Idle;
        self.failure = None;
        self.install_progress = None;
        self.target = None;
        self.settings = None;
        self.runtime_origin = None;
        self.prompt_editor_ipc_ready = false;
    }

    pub(crate) fn replace_child(&mut self, child: Child) {
        if let Some(mut previous_child) = self.child.take() {
            let _ = previous_child.kill();
            let _ = previous_child.wait();
        }
        self.child = Some(child);
    }

    pub(crate) fn refresh_child_exit(&mut self) -> bool {
        let Some(child) = self.child.as_mut() else {
            return false;
        };
        match child.try_wait() {
            Ok(Some(_)) | Err(_) => {
                self.pending_remote_prompt_editor_request = None;
                self.child = None;
                self.started_at = None;
                self.state = SourceCodeServerRuntimeLaunchState::Idle;
                self.failure = None;
                self.install_progress = None;
                self.runtime_origin = None;
                self.prompt_editor_ipc_ready = false;
                true
            }
            Ok(None) => false,
        }
    }

    pub(crate) fn stop(&mut self) -> bool {
        let had_state = self.child.is_some()
            || self.target.is_some()
            || self.state != SourceCodeServerRuntimeLaunchState::Idle;
        if let Some(mut child) = self.child.take() {
            let _ = child.kill();
            let _ = child.wait();
        }
        self.generation = self.generation.saturating_add(1);
        self.pending_remote_prompt_editor_request = None;
        self.started_at = None;
        self.state = SourceCodeServerRuntimeLaunchState::Idle;
        self.failure = None;
        self.install_progress = None;
        self.target = None;
        self.settings = None;
        self.runtime_origin = None;
        self.prompt_editor_ipc_ready = false;
        had_state
    }
}


pub(crate) struct SourceCodeServerRuntimeStartOutput {
    pub(crate) child: Child,
    pub(crate) runtime_origin: String,
    pub(crate) prompt_editor_ipc_ready: bool,
    pub(crate) started_at: Instant,
    pub(crate) http_runtime_ready: bool,
}


#[derive(Clone, Copy, PartialEq, Eq, Hash)]
pub(crate) struct AgentsTerminalRuntimeSessionId(pub(crate) u64);

