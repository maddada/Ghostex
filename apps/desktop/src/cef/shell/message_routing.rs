// C4 light split: bridge/popup event taxonomy, dispatch-policy
// classification, the V8Handler impls, and the sidebar/project-workarea/
// app-modal-host/native-host/session-chat JS bridge install, update, send,
// and (de)serialization plumbing. Pure move out of `cef/shell.rs`; the only
// edit is the `pub(crate) ` prefix moved items need to stay callable from
// their siblings and from `shell` itself. See
// docs/2026-08-22/repo-restructure/SPLITS.md C4.
use super::*;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum SidebarBridgeEventKind {
    ActiveProjectContext,
    SourceWorkareaReadiness,
    BrowserWorkareaReadiness,
    ProjectWorkareaReadiness,
    ManageFileWorkareaOperationRequest,
    NativeProjectPathAction,
    NativeAppShotPrompt,
    SidebarCommandAction,
    SidebarCommandRunEnd,
    GhostexHotkeyAction,
    GxserverPresentationFocusState,
    CreateProjectAgent,
    CreateProjectTerminal,
    WorkspaceTerminalFocus,
    WorkspaceTerminalRenameCommand,
    WorkspaceTerminalEnter,
    WorkspaceTerminalLifecycleResult,
    SessionCompletionSound,
    SessionStatusIndicators,
    PetOverlayState,
    GlobalActions,
    TitlebarGitMenuState,
    OpenBrowserUrl,
    BrowserTabFocus,
    ProjectBoardConversationResponse,
}

impl SidebarBridgeEventKind {
    /*
    CDXC:GPUISidebarPassiveMouseFocus 2026-07-22:
    Almost every sidebar bridge function forwards to the app handler in
    main.rs. SidebarEditableFocus is the one exception: it is a native
    first-responder transfer for the sending browser itself, so the CEF
    boundary consumes it directly and it never becomes an app event.
    */
    pub(crate) fn forwarded_from(function_id: SidebarBridgeFunctionId) -> Option<Self> {
        Some(match function_id {
            SidebarBridgeFunctionId::SidebarEditableFocus => return None,
            SidebarBridgeFunctionId::ActiveProjectContext => Self::ActiveProjectContext,
            SidebarBridgeFunctionId::SourceWorkareaReadiness => Self::SourceWorkareaReadiness,
            SidebarBridgeFunctionId::BrowserWorkareaReadiness => Self::BrowserWorkareaReadiness,
            SidebarBridgeFunctionId::ProjectWorkareaReadiness => Self::ProjectWorkareaReadiness,
            SidebarBridgeFunctionId::ManageFileWorkareaOperationRequest => {
                Self::ManageFileWorkareaOperationRequest
            }
            SidebarBridgeFunctionId::NativeProjectPathAction => Self::NativeProjectPathAction,
            SidebarBridgeFunctionId::NativeAppShotPrompt => Self::NativeAppShotPrompt,
            SidebarBridgeFunctionId::SidebarCommandAction => Self::SidebarCommandAction,
            SidebarBridgeFunctionId::SidebarCommandRunEnd => Self::SidebarCommandRunEnd,
            SidebarBridgeFunctionId::GhostexHotkeyAction => Self::GhostexHotkeyAction,
            SidebarBridgeFunctionId::GxserverPresentationFocusState => {
                Self::GxserverPresentationFocusState
            }
            SidebarBridgeFunctionId::CreateProjectAgent => Self::CreateProjectAgent,
            SidebarBridgeFunctionId::CreateProjectTerminal => Self::CreateProjectTerminal,
            SidebarBridgeFunctionId::WorkspaceTerminalFocus => Self::WorkspaceTerminalFocus,
            SidebarBridgeFunctionId::WorkspaceTerminalRenameCommand => {
                Self::WorkspaceTerminalRenameCommand
            }
            SidebarBridgeFunctionId::WorkspaceTerminalEnter => Self::WorkspaceTerminalEnter,
            SidebarBridgeFunctionId::WorkspaceTerminalLifecycleResult => {
                Self::WorkspaceTerminalLifecycleResult
            }
            SidebarBridgeFunctionId::SessionCompletionSound => Self::SessionCompletionSound,
            SidebarBridgeFunctionId::SessionStatusIndicators => Self::SessionStatusIndicators,
            SidebarBridgeFunctionId::PetOverlayState => Self::PetOverlayState,
            SidebarBridgeFunctionId::GlobalActions => Self::GlobalActions,
            SidebarBridgeFunctionId::TitlebarGitMenuState => Self::TitlebarGitMenuState,
            SidebarBridgeFunctionId::OpenBrowserUrl => Self::OpenBrowserUrl,
            SidebarBridgeFunctionId::BrowserTabFocus => Self::BrowserTabFocus,
            SidebarBridgeFunctionId::ProjectBoardConversationResponse => {
                Self::ProjectBoardConversationResponse
            }
        })
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum ProjectWorkareaBridgeEventKind {
    ProjectBeadsRequest,
    ProjectBoardRequest,
    ProjectBoardImageRequest,
    ManageFilesRequest,
}

impl From<ProjectWorkareaBridgeFunctionId> for ProjectWorkareaBridgeEventKind {
    fn from(function_id: ProjectWorkareaBridgeFunctionId) -> Self {
        match function_id {
            ProjectWorkareaBridgeFunctionId::ProjectBeadsRequest => Self::ProjectBeadsRequest,
            ProjectWorkareaBridgeFunctionId::ProjectBoardRequest => Self::ProjectBoardRequest,
            ProjectWorkareaBridgeFunctionId::ProjectBoardImageRequest => {
                Self::ProjectBoardImageRequest
            }
            ProjectWorkareaBridgeFunctionId::ManageFilesRequest => Self::ManageFilesRequest,
        }
    }
}

#[derive(Clone, Copy, PartialEq, Eq)]
pub(crate) enum BrowserPopupDispatchPolicy {
    DispatchShellOpen,
    HandleWithoutDispatch,
}

impl BrowserPopupDispatchPolicy {
    /*
    CDXC:GPUIBrowserRuntimePolicy 2026-06-23-12:48:
    The CEF backend must mirror the shell popup policy before crossing into GPUI app state. Non-empty target URLs dispatch the shell-owned Browser tab path; empty targets are handled inside CEF with no shell callback, no address-only tab, no content transfer fallback, no filesystem/browser-store access, and no URL/title/page logging.
    */
    pub(crate) fn for_target_url(target_url: &str) -> Self {
        if target_url.trim().is_empty() {
            Self::HandleWithoutDispatch
        } else {
            Self::DispatchShellOpen
        }
    }

    pub(crate) fn dispatches_shell_open(self) -> bool {
        matches!(self, Self::DispatchShellOpen)
    }
}

pub(crate) fn browser_popup_target_url_for_shell(target_url: Option<&CefString>) -> Option<String> {
    let requested_url = target_url.map(CefString::to_string).unwrap_or_default();
    BrowserPopupDispatchPolicy::for_target_url(&requested_url)
        .dispatches_shell_open()
        .then_some(requested_url)
}

/*
CDXC:GPUIBrowserLinkNewTab 2026-08-18:
Middle-click and Cmd/Ctrl-click link opens never reach OnBeforePopup: Chromium
routes them through RequestHandler::OnOpenURLFromTab with the disposition the
gesture asked for. Map exactly the new-browser dispositions onto the existing
shell popup path so they become Browser tabs, and leave every same-tab or
non-navigational disposition to CEF's default handling.
*/
pub(crate) fn browser_popup_placement_for_disposition(
    disposition: WindowOpenDisposition,
) -> Option<BrowserPopupPlacement> {
    match disposition {
        WindowOpenDisposition::NEW_BACKGROUND_TAB => Some(BrowserPopupPlacement::Background),
        WindowOpenDisposition::NEW_FOREGROUND_TAB
        | WindowOpenDisposition::NEW_WINDOW
        | WindowOpenDisposition::NEW_POPUP => Some(BrowserPopupPlacement::Selected),
        _ => None,
    }
}

/// Path markers that identify app-bundled first-party CEF entries in each
/// OS packaging layout; dev builds always serve from `dist/sidebar`.
#[cfg(target_os = "macos")]
pub(crate) const FIRST_PARTY_CEF_ENTRY_PATH_MARKERS: [&str; 2] =
    ["/Contents/Resources/sidebar/", "/dist/sidebar/"];
// Windows and Linux share the bundle-less flat layout: the sidebar ships at
// dist/sidebar beside the executable (see build-windows-app.ps1 /
// build-linux-app.sh).
#[cfg(any(target_os = "windows", target_os = "linux"))]
pub(crate) const FIRST_PARTY_CEF_ENTRY_PATH_MARKERS: [&str; 2] = ["/resources/sidebar/", "/dist/sidebar/"];

pub(crate) fn is_gpui_first_party_cef_entry_url(url: &str, entry_file_name: &str) -> bool {
    let Some(base) = url.split(['?', '#']).next() else {
        return false;
    };
    base.starts_with("file://")
        && base.ends_with(&format!("/{entry_file_name}"))
        && FIRST_PARTY_CEF_ENTRY_PATH_MARKERS
            .iter()
            .any(|marker| base.contains(marker))
}

pub(crate) fn app_modal_host_bridge_surface_for_frame_url(url: &str) -> Option<AppModalHostBridgeSurface> {
    APP_MODAL_HOST_BRIDGE_SURFACE_SPECS
        .iter()
        .find(|spec| is_gpui_first_party_cef_entry_url(url, spec.entry_file_name))
        .map(|spec| spec.surface)
}
/// Where a CEF-requested link open should land in the Browser tab strip.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum BrowserPopupPlacement {
    /// Select the new tab: `window.open`, `target=_blank`, and the
    /// context-menu "Open Link in New Window"/"New Tab" rows.
    Selected,
    /// Append the new tab without leaving the current page: middle-click and
    /// Cmd/Ctrl-click link opens, matching every desktop browser.
    Background,
}

pub type BrowserPopupOpenHandler = StdRc<dyn Fn(String, BrowserPopupPlacement)>;

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum SidebarBridgeEvent {
    ActiveProjectContext(String),
    SourceWorkareaReadiness(String),
    BrowserWorkareaReadiness(String),
    ProjectWorkareaReadiness(String),
    ManageFileWorkareaOperationRequest(String),
    NativeProjectPathAction(String),
    NativeAppShotPrompt(String),
    SidebarCommandAction(String),
    SidebarCommandRunEnd(String),
    GhostexHotkeyAction(String),
    GxserverPresentationFocusState(String),
    CreateProjectAgent(String),
    CreateProjectTerminal(String),
    WorkspaceTerminalFocus(String),
    WorkspaceTerminalRenameCommand(String),
    WorkspaceTerminalEnter(String),
    WorkspaceTerminalLifecycleResult(String),
    SessionCompletionSound(String),
    SessionStatusIndicators(String),
    PetOverlayState(String),
    GlobalActions(String),
    TitlebarGitMenuState(String),
    OpenBrowserUrl(String),
    BrowserTabFocus(String),
    ProjectBoardConversationResponse(String),
}

pub type SidebarBridgeEventHandler = StdRc<dyn Fn(SidebarBridgeEvent)>;

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ProjectWorkareaBridgeEvent {
    ProjectBeadsRequest(String),
    ProjectBoardRequest(String),
    ProjectBoardImageRequest(String),
    ManageFilesRequest(String),
}

pub type ProjectWorkareaBridgeEventHandler = StdRc<dyn Fn(ProjectWorkareaBridgeEvent)>;

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum AppModalHostBridgeEvent {
    Message(String),
    NativeHostMessage(String),
}

pub type AppModalHostBridgeEventHandler = StdRc<dyn Fn(AppModalHostBridgeEvent)>;

impl SidebarBridgeEventKind {
    pub(crate) fn with_payload(self, payload: String) -> SidebarBridgeEvent {
        match self {
            Self::ActiveProjectContext => SidebarBridgeEvent::ActiveProjectContext(payload),
            Self::SourceWorkareaReadiness => SidebarBridgeEvent::SourceWorkareaReadiness(payload),
            Self::BrowserWorkareaReadiness => SidebarBridgeEvent::BrowserWorkareaReadiness(payload),
            Self::ProjectWorkareaReadiness => SidebarBridgeEvent::ProjectWorkareaReadiness(payload),
            Self::ManageFileWorkareaOperationRequest => {
                SidebarBridgeEvent::ManageFileWorkareaOperationRequest(payload)
            }
            Self::NativeProjectPathAction => SidebarBridgeEvent::NativeProjectPathAction(payload),
            Self::NativeAppShotPrompt => SidebarBridgeEvent::NativeAppShotPrompt(payload),
            Self::SidebarCommandAction => SidebarBridgeEvent::SidebarCommandAction(payload),
            Self::SidebarCommandRunEnd => SidebarBridgeEvent::SidebarCommandRunEnd(payload),
            Self::GhostexHotkeyAction => SidebarBridgeEvent::GhostexHotkeyAction(payload),
            Self::GxserverPresentationFocusState => {
                SidebarBridgeEvent::GxserverPresentationFocusState(payload)
            }
            Self::CreateProjectAgent => SidebarBridgeEvent::CreateProjectAgent(payload),
            Self::CreateProjectTerminal => SidebarBridgeEvent::CreateProjectTerminal(payload),
            Self::WorkspaceTerminalFocus => SidebarBridgeEvent::WorkspaceTerminalFocus(payload),
            Self::WorkspaceTerminalRenameCommand => {
                SidebarBridgeEvent::WorkspaceTerminalRenameCommand(payload)
            }
            Self::WorkspaceTerminalEnter => SidebarBridgeEvent::WorkspaceTerminalEnter(payload),
            Self::WorkspaceTerminalLifecycleResult => {
                SidebarBridgeEvent::WorkspaceTerminalLifecycleResult(payload)
            }
            Self::SessionCompletionSound => SidebarBridgeEvent::SessionCompletionSound(payload),
            Self::SessionStatusIndicators => SidebarBridgeEvent::SessionStatusIndicators(payload),
            Self::PetOverlayState => SidebarBridgeEvent::PetOverlayState(payload),
            Self::GlobalActions => SidebarBridgeEvent::GlobalActions(payload),
            Self::TitlebarGitMenuState => SidebarBridgeEvent::TitlebarGitMenuState(payload),
            Self::OpenBrowserUrl => SidebarBridgeEvent::OpenBrowserUrl(payload),
            Self::BrowserTabFocus => SidebarBridgeEvent::BrowserTabFocus(payload),
            Self::ProjectBoardConversationResponse => {
                SidebarBridgeEvent::ProjectBoardConversationResponse(payload)
            }
        }
    }
}

pub(crate) fn sidebar_bridge_event_kind_for_process_message(
    process_message_name: &str,
) -> Option<SidebarBridgeEventKind> {
    sidebar_bridge_function_spec_for_process_message(process_message_name)
        .and_then(|spec| SidebarBridgeEventKind::forwarded_from(spec.id))
}

pub(crate) fn sidebar_bridge_installed_for_handler(handler_present: bool) -> bool {
    handler_present
}

impl ProjectWorkareaBridgeEventKind {
    pub(crate) fn with_payload(self, payload: String) -> ProjectWorkareaBridgeEvent {
        match self {
            Self::ProjectBeadsRequest => ProjectWorkareaBridgeEvent::ProjectBeadsRequest(payload),
            Self::ProjectBoardRequest => ProjectWorkareaBridgeEvent::ProjectBoardRequest(payload),
            Self::ProjectBoardImageRequest => {
                ProjectWorkareaBridgeEvent::ProjectBoardImageRequest(payload)
            }
            Self::ManageFilesRequest => ProjectWorkareaBridgeEvent::ManageFilesRequest(payload),
        }
    }
}

pub(crate) fn project_workarea_bridge_event_kind_for_process_message(
    process_message_name: &str,
) -> Option<ProjectWorkareaBridgeEventKind> {
    project_workarea_bridge_function_spec_for_process_message(process_message_name)
        .map(|spec| ProjectWorkareaBridgeEventKind::from(spec.id))
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct SidebarRuntimeSettingsSnapshot {
    pub debugging_mode: bool,
    pub show_beta_features: bool,
    pub saved_settings_json: String,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SidebarGxserverBootstrap {
    pub base_url: String,
    pub auth_token: String,
    pub protocol_version: i32,
    pub client_id: String,
    pub initial_active_project_id: Option<String>,
    pub focused_session_id: Option<String>,
    pub visible_session_ids: Vec<String>,
}

pub enum BrowserPageMetadataEvent {
    AddressChanged(String),
    CloseRequested,
    FaviconUrlChanged(Option<String>),
    FindResult {
        match_count: i32,
        active_match_ordinal: i32,
        final_update: bool,
    },
    LoadingStateChanged {
        is_loading: bool,
        can_go_back: bool,
        can_go_forward: bool,
    },
    TitleChanged(String),
}

pub type BrowserPageMetadataHandler = StdRc<dyn Fn(BrowserPageMetadataEvent)>;

/*
CDXC:GPUITutorialVideoFullscreen 2026-08-18:
Third-party surfaces that carry no Ghostex bridge (today only the tutorial
video modal, which loads the YouTube watch page as its top-level document) can
still need a host-side action once their page is really on screen. This
callback reports main-frame load-end for exactly those surfaces; it carries no
page data (no URL, title, or content), only the "this browser finished loading
its main frame" edge.
*/
pub type PageLoadEndHandler = StdRc<dyn Fn()>;

/*
CDXC:GPUIBrowserMediaPermissions 2026-07-27:
Alloy-style CEF denies every `getUserMedia()` call outright when the client
installs no permission handler, so Browser panes reported "permission denied"
without ever asking the user. Device microphone/camera requests are forwarded
to the GPUI shell instead, which renders the in-pane permission prompt and
answers through the responder below. Desktop capture (`getDisplayMedia`) keeps
CEF's default deny: it needs a source picker plus macOS Screen Recording
consent that this surface does not implement, so it is never silently granted
along with a microphone/camera decision.
*/
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct BrowserMediaAccessKinds {
    pub microphone: bool,
    pub camera: bool,
}

impl BrowserMediaAccessKinds {
    pub fn is_empty(self) -> bool {
        !self.microphone && !self.camera
    }

    pub fn intersection(self, other: Self) -> Self {
        Self {
            microphone: self.microphone && other.microphone,
            camera: self.camera && other.camera,
        }
    }
}

/// A pending CEF media-device permission request. The CEF request stays open
/// until exactly one of `allow`/`deny` runs, so dropping an unanswered request
/// cancels it instead of leaving the page's `getUserMedia()` promise hanging.
pub struct BrowserMediaAccessRequest {
    pub(crate) requesting_origin: String,
    pub(crate) kinds: BrowserMediaAccessKinds,
    pub(crate) callback: Option<MediaAccessCallback>,
}

impl BrowserMediaAccessRequest {
    pub fn requesting_origin(&self) -> &str {
        &self.requesting_origin
    }

    pub fn kinds(&self) -> BrowserMediaAccessKinds {
        self.kinds
    }

    /// Grants the intersection of `granted` and the originally requested
    /// devices; anything the page did not ask for stays denied.
    pub fn allow(mut self, granted: BrowserMediaAccessKinds) {
        let granted = self.kinds.intersection(granted);
        let mut allowed_permissions = MediaAccessPermissionTypes::NONE.get_raw() as u32;
        if granted.microphone {
            allowed_permissions |=
                MediaAccessPermissionTypes::DEVICE_AUDIO_CAPTURE.get_raw() as u32;
        }
        if granted.camera {
            allowed_permissions |=
                MediaAccessPermissionTypes::DEVICE_VIDEO_CAPTURE.get_raw() as u32;
        }
        if let Some(callback) = self.callback.take() {
            callback.cont(allowed_permissions as _);
        }
    }

    pub fn deny(mut self) {
        if let Some(callback) = self.callback.take() {
            callback.cont(MediaAccessPermissionTypes::NONE.get_raw() as _);
        }
    }
}

impl Drop for BrowserMediaAccessRequest {
    fn drop(&mut self) {
        if let Some(callback) = self.callback.take() {
            callback.cancel();
        }
    }
}

pub type BrowserMediaAccessHandler = StdRc<dyn Fn(BrowserMediaAccessRequest)>;
wrap_v8_handler! {
    pub(crate) struct GhostexGpuiProjectWorkareaBridgeV8Handler;

    impl V8Handler {
        fn execute(
            &self,
            name: Option<&CefString>,
            _object: Option<&mut V8Value>,
            arguments: Option<&[Option<V8Value>]>,
            retval: Option<&mut Option<V8Value>>,
            _exception: Option<&mut CefString>,
        ) -> c_int {
            let name = name.map(CefString::to_string);
            let Some(spec) = name
                .as_deref()
                .and_then(project_workarea_bridge_function_spec_for_js_function)
            else {
                return 0;
            };

            let payload = arguments
                .and_then(|arguments| arguments.first())
                .and_then(Option::as_ref)
                .filter(|argument| argument.is_string() != 0)
                .map(|argument| CefString::from(&argument.string_value()).to_string());
            let Some(payload) = payload else {
                set_v8_bool_return(retval, false);
                return 1;
            };

            let sent =
                send_project_workarea_bridge_process_message(spec.process_message_name, &payload);
            set_v8_bool_return(retval, sent);
            1
        }
    }
}

wrap_v8_handler! {
    pub(crate) struct GhostexGpuiAppModalHostBridgeV8Handler;

    impl V8Handler {
        fn execute(
            &self,
            name: Option<&CefString>,
            _object: Option<&mut V8Value>,
            arguments: Option<&[Option<V8Value>]>,
            retval: Option<&mut Option<V8Value>>,
            _exception: Option<&mut CefString>,
        ) -> c_int {
            let name = name.map(CefString::to_string);
            if name.as_deref() != Some(WEBKIT_POST_MESSAGE_JS_FUNCTION) {
                return 0;
            }

            let payload = arguments
                .and_then(|arguments| arguments.first())
                .and_then(Option::as_ref)
                .and_then(app_modal_host_payload_from_v8_value);
            let Some(payload) = payload else {
                set_v8_bool_return(retval, false);
                return 1;
            };

            let sent = send_app_modal_host_bridge_process_message(&payload);
            set_v8_bool_return(retval, sent);
            1
        }
    }
}

wrap_v8_handler! {
    pub(crate) struct GhostexGpuiNativeHostBridgeV8Handler;

    impl V8Handler {
        fn execute(
            &self,
            name: Option<&CefString>,
            _object: Option<&mut V8Value>,
            arguments: Option<&[Option<V8Value>]>,
            retval: Option<&mut Option<V8Value>>,
            _exception: Option<&mut CefString>,
        ) -> c_int {
            let name = name.map(CefString::to_string);
            if name.as_deref() != Some(WEBKIT_POST_MESSAGE_JS_FUNCTION) {
                return 0;
            }

            let payload = arguments
                .and_then(|arguments| arguments.first())
                .and_then(Option::as_ref)
                .and_then(app_modal_host_payload_from_v8_value);
            let Some(payload) = payload else {
                set_v8_bool_return(retval, false);
                return 1;
            };

            let sent = send_native_host_bridge_process_message(&payload);
            set_v8_bool_return(retval, sent);
            1
        }
    }
}

wrap_v8_handler! {
    pub(crate) struct GhostexGpuiSidebarBridgeV8Handler;

    impl V8Handler {
        fn execute(
            &self,
            name: Option<&CefString>,
            _object: Option<&mut V8Value>,
            arguments: Option<&[Option<V8Value>]>,
            retval: Option<&mut Option<V8Value>>,
            _exception: Option<&mut CefString>,
        ) -> c_int {
            let name = name.map(CefString::to_string);
            let Some(spec) = name
                .as_deref()
                .and_then(sidebar_bridge_function_spec_for_js_function)
            else {
                return 0;
            };

            let payload = arguments
                .and_then(|arguments| arguments.first())
                .and_then(Option::as_ref)
                .filter(|argument| argument.is_string() != 0)
                .map(|argument| CefString::from(&argument.string_value()).to_string());
            let Some(payload) = payload else {
                set_v8_bool_return(retval, false);
                return 1;
            };

            let sent = send_sidebar_bridge_process_message(spec.process_message_name, &payload);
            set_v8_bool_return(retval, sent);
            1
        }
    }
}

pub(crate) fn install_sidebar_project_context_v8_bridge(
    context: Option<&mut cef::V8Context>,
    runtime_settings: SidebarRuntimeSettingsSnapshot,
    gxserver_bootstrap: Option<SidebarGxserverBootstrap>,
) {
    /*
    CDXC:GPUIProjectSidebarBridge 2026-06-23-18:29:
    The renderer-side sidebar bridge exposes only fixed typed string-payload functions for active-project context, Source readiness, Browser readiness, project-workarea readiness, Manage operation requests, sidebar-native side-effect requests, gxserver focus-state hints, and workspace terminal focus and rename requests, plus `window.ghostexGpui.runtimeSettings` with strict debuggingMode/showBetaFeatures booleans and the saved shared Settings object. It does not expose generic message names, event buses, filesystem/project detection, trusted file paths, URL/title inspection, arbitrary logging, persistence, or fallback project inference.

    CDXC:GPUIProjectSidebarBridge 2026-06-23-06:57:
    Initial install publishes runtime settings through an already-registered `window.ghostexGpui.onRuntimeSettingsChanged(settings)` callback because the sidebar runtime can mount before CEF's load-end install message. If install wins the race, the runtime reads the installed object directly. Later refreshes use a second private browser-to-renderer CEF message with the same callback contract. This keeps ordinary Browser tabs out of the sidebar bridge and avoids a generic event/settings bus.

    CDXC:GPUISettingsSidebarHandoff 2026-06-24-11:22:
    The runtimeSettings object also carries the saved shared Settings object for the sidebar renderer to normalize with the shared TypeScript schema. This remains a narrow sidebar-owned handoff: the CEF boundary accepts only the serialized object already read by GPUI, parses it into one V8 object property, and does not expose a generic settings API, persistence hook, logging path, URL/title state, command text, tokens, or fallback project inference.

    CDXC:GPUISidebarGxserverBootstrap 2026-06-24-11:17:
    The same sidebar-only private install message may set `window.ghostexGpui.gxserverBootstrap` from real local gxserver facts: loopback base URL, bearer token, protocol version, stable client id, and optional gxserver ids only when app state already owns them. Do not derive ids from paths, titles, fixtures, shell placeholders, Browser tabs, terminal state, logs, persistence, or fallback project detection.

    CDXC:GPUISidebarGxserverFocusState 2026-06-24-21:07:
    The focus-state bridge is a fixed sidebar-only string payload used to return React-owned gxserver presentation session ids to Rust for bootstrap replay. It must remain separate from native path actions and must not carry paths, titles, command text, terminal contents, tokens, daemon response bodies, or renderer-derived labels.

    CDXC:GPUIWorkspaceSessionFocus 2026-06-26-06:08:
    The workspace terminal focus bridge is fixed-function and sidebar-only. It may carry only the gxserver project/session ids React already focused so Rust can select or materialize the matching Agents tab from gxserver attach metadata; it must not accept labels, commands, paths, terminal contents, daemon responses, or generic terminal IPC.

    CDXC:GPUIWorkspaceRenameCommand 2026-06-27-02:27:
    Workspace terminal rename parity adds one fixed sidebar-only bridge function for the already-trimmed rename title plus gxserver project/session ids. CEF still exposes no generic terminal-text sender, command bus, cwd/path authority, logging path, renderer-selected target surface, or fallback terminal IPC.

    CDXC:GPUICommandPane 2026-06-24-23:17:
    The sidebar command-action bridge is a fixed-function handoff for the shared SidebarApp `runSidebarCommand` message. It may carry the selected gxserver HUD action fields to app Rust, but it must not expose generic IPC, filesystem/project discovery, logs, persistence, terminal content, stdout/stderr, or renderer-side execution authority.

    CDXC:GPUICommandPane 2026-06-26-00:05:
    Terminal Actions may include the terminal-only `closeTerminalOnExit` boolean in the fixed command-action JSON so GPUI can match macOS close-on-exit behavior. Browser Actions must not use that flag, and CEF still forwards only the bounded sidebar payload string for Rust-side strict parsing.

    CDXC:GPUIAppShots 2026-06-25-23:28:
    The App Shot prompt bridge is fixed-function and sidebar-only. It may carry only the validated gxserver presentation session id and the already formatted app-owned prompt string; screenshot paths are not accepted as separate authority and the bridge is not a generic terminal text IPC.

    CDXC:GPUIAppShots 2026-06-26-04:27:
    Remote App Shot insertion still uses this fixed bridge shape. Renderer code may identify only the existing machine-scoped remote session row; Rust owns mounted-surface verification and must not receive SSH details, paths, URLs, tokens, commands, output, or terminal content.

    CDXC:GPUIStatusPetOverlay 2026-06-26-04:38:
    Status indicator and pet overlay updates are fixed sidebar-only functions on this namespace. They may carry bounded enum/count/boolean/size/pet-id/project-id/session-id/order/title fields for GPUI-owned presentation and click routing, but no generic native-event bus, menu-bar status-item emulation, paths, URLs, commands, stdout/stderr, tokens, or terminal content.
    */
    let Some(context) = context else {
        return;
    };
    let Some(global) = context.global() else {
        return;
    };

    let namespace_key = CefString::from(SIDEBAR_PROJECT_CONTEXT_JS_NAMESPACE);
    let mut namespace = global
        .value_bykey(Some(&namespace_key))
        .filter(|value| value.is_object() != 0)
        .or_else(|| cef::v8_value_create_object(None, None));
    let Some(namespace) = namespace.as_mut() else {
        return;
    };

    for spec in SIDEBAR_BRIDGE_FUNCTION_SPECS {
        let mut handler = GhostexGpuiSidebarBridgeV8Handler::new();
        let function_name = CefString::from(spec.js_function_name);
        let mut function = cef::v8_value_create_function(Some(&function_name), Some(&mut handler));
        let Some(function) = function.as_mut() else {
            return;
        };

        namespace.set_value_bykey(
            Some(&function_name),
            Some(function),
            V8Propertyattribute::default(),
        );
    }
    let runtime_settings_object =
        install_sidebar_runtime_settings_v8_object(context, namespace, runtime_settings);
    let _ = install_sidebar_gxserver_bootstrap_v8_object(namespace, gxserver_bootstrap);
    global.set_value_bykey(
        Some(&namespace_key),
        Some(namespace),
        V8Propertyattribute::default(),
    );
    if let Some(runtime_settings_object) = runtime_settings_object {
        notify_sidebar_runtime_settings_changed(context, namespace, runtime_settings_object);
    }
}

pub(crate) fn update_sidebar_runtime_settings_v8_bridge(
    context: Option<&mut cef::V8Context>,
    runtime_settings: SidebarRuntimeSettingsSnapshot,
) {
    let Some(context) = context else {
        return;
    };
    let Some(global) = context.global() else {
        return;
    };
    let namespace_key = CefString::from(SIDEBAR_PROJECT_CONTEXT_JS_NAMESPACE);
    let mut namespace = global
        .value_bykey(Some(&namespace_key))
        .filter(|value| value.is_object() != 0);
    let Some(namespace) = namespace.as_mut() else {
        return;
    };
    for spec in SIDEBAR_BRIDGE_FUNCTION_SPECS {
        let function_key = CefString::from(spec.js_function_name);
        if namespace
            .value_bykey(Some(&function_key))
            .filter(|value| value.is_function() != 0)
            .is_none()
        {
            return;
        }
    }

    let Some(runtime_settings_object) =
        install_sidebar_runtime_settings_v8_object(context, namespace, runtime_settings)
    else {
        return;
    };
    notify_sidebar_runtime_settings_changed(context, namespace, runtime_settings_object);
}

pub(crate) fn update_sidebar_gxserver_bootstrap_v8_bridge(
    context: Option<&mut cef::V8Context>,
    gxserver_bootstrap: Option<SidebarGxserverBootstrap>,
) {
    let Some(context) = context else {
        return;
    };
    let Some(global) = context.global() else {
        return;
    };
    let namespace_key = CefString::from(SIDEBAR_PROJECT_CONTEXT_JS_NAMESPACE);
    let mut namespace = global
        .value_bykey(Some(&namespace_key))
        .filter(|value| value.is_object() != 0);
    let Some(namespace) = namespace.as_mut() else {
        return;
    };
    for spec in SIDEBAR_BRIDGE_FUNCTION_SPECS {
        let function_key = CefString::from(spec.js_function_name);
        if namespace
            .value_bykey(Some(&function_key))
            .filter(|value| value.is_function() != 0)
            .is_none()
        {
            return;
        }
    }

    /*
    CDXC:GPUISidebarGxserverBootstrap 2026-06-24-11:17:
    Post-load gxserver bootstrap refresh is a narrow sidebar bridge update, not a generic host event bus or JavaScript injection channel. It can replace only `window.ghostexGpui.gxserverBootstrap` and call the fixed optional `onGxserverBootstrapChanged(bootstrap)` callback, so token availability changes reach the React runtime while Browser/workarea/modal CEF clients remain outside the token path.
    */
    let Some(bootstrap_object) =
        install_sidebar_gxserver_bootstrap_v8_object(namespace, gxserver_bootstrap)
    else {
        return;
    };
    notify_sidebar_gxserver_bootstrap_changed(context, namespace, bootstrap_object);
}

pub(crate) fn install_session_chat_gxserver_bootstrap_v8_bridge(
    context: Option<&mut cef::V8Context>,
    gxserver_bootstrap: Option<SidebarGxserverBootstrap>,
) {
    let Some(context) = context else {
        return;
    };
    let Some(global) = context.global() else {
        return;
    };
    let namespace_key = CefString::from(SIDEBAR_PROJECT_CONTEXT_JS_NAMESPACE);
    let mut namespace = global
        .value_bykey(Some(&namespace_key))
        .filter(|value| value.is_object() != 0)
        .or_else(|| cef::v8_value_create_object(None, None));
    let Some(namespace) = namespace.as_mut() else {
        return;
    };
    let Some(bootstrap_object) =
        install_sidebar_gxserver_bootstrap_v8_object(namespace, gxserver_bootstrap)
    else {
        return;
    };
    global.set_value_bykey(
        Some(&namespace_key),
        Some(namespace),
        V8Propertyattribute::default(),
    );
    notify_sidebar_gxserver_bootstrap_changed(context, namespace, bootstrap_object);
}

pub(crate) fn install_project_workarea_v8_bridge(
    context: Option<&mut cef::V8Context>,
    manage_docs_resource_base_url: Option<&str>,
) {
    let Some(context) = context else {
        return;
    };
    let Some(global) = context.global() else {
        return;
    };

    let namespace_key = CefString::from(SIDEBAR_PROJECT_CONTEXT_JS_NAMESPACE);
    let mut namespace = global
        .value_bykey(Some(&namespace_key))
        .filter(|value| value.is_object() != 0)
        .or_else(|| cef::v8_value_create_object(None, None));
    let Some(namespace) = namespace.as_mut() else {
        return;
    };

    for spec in PROJECT_WORKAREA_BRIDGE_FUNCTION_SPECS {
        let mut handler = GhostexGpuiProjectWorkareaBridgeV8Handler::new();
        let function_name = CefString::from(spec.js_function_name);
        let mut function = cef::v8_value_create_function(Some(&function_name), Some(&mut handler));
        let Some(function) = function.as_mut() else {
            return;
        };

        namespace.set_value_bykey(
            Some(&function_name),
            Some(function),
            V8Propertyattribute::default(),
        );
    }

    if let Some(base_url) = manage_docs_resource_base_url {
        set_v8_string_property(
            namespace,
            PROJECT_WORKAREA_MANAGE_DOCS_RESOURCE_BASE_URL_JS_FIELD,
            base_url,
        );
    }

    global.set_value_bykey(
        Some(&namespace_key),
        Some(namespace),
        V8Propertyattribute::default(),
    );
}

pub(crate) fn install_app_modal_host_v8_bridge(
    context: Option<&mut cef::V8Context>,
    surface: AppModalHostBridgeSurface,
) {
    let Some(context) = context else {
        return;
    };
    let Some(global) = context.global() else {
        return;
    };

    if surface.exposes_native_window_identity() {
        let _ = set_v8_string_property(
            &global,
            APP_MODAL_HOST_SURFACE_JS_FIELD,
            APP_MODAL_HOST_SURFACE_VALUE,
        );
        let _ =
            set_v8_string_property(&global, APP_MODAL_HOST_ID_JS_FIELD, APP_MODAL_HOST_ID_VALUE);
    }

    let Some(mut webkit) = v8_object_property_or_new(&global, WEBKIT_JS_OBJECT) else {
        return;
    };
    let Some(mut message_handlers) =
        v8_object_property_or_new(&webkit, WEBKIT_MESSAGE_HANDLERS_JS_OBJECT)
    else {
        return;
    };
    let Some(mut app_modal_host) = cef::v8_value_create_object(None, None) else {
        return;
    };

    let mut handler = GhostexGpuiAppModalHostBridgeV8Handler::new();
    let function_name = CefString::from(WEBKIT_POST_MESSAGE_JS_FUNCTION);
    let mut post_message =
        match cef::v8_value_create_function(Some(&function_name), Some(&mut handler)) {
            Some(function) => function,
            None => return,
        };
    app_modal_host.set_value_bykey(
        Some(&function_name),
        Some(&mut post_message),
        V8Propertyattribute::default(),
    );

    let app_modal_host_key = CefString::from(WEBKIT_APP_MODAL_HOST_MESSAGE_HANDLER_JS_OBJECT);
    message_handlers.set_value_bykey(
        Some(&app_modal_host_key),
        Some(&mut app_modal_host),
        V8Propertyattribute::default(),
    );

    if matches!(
        surface,
        AppModalHostBridgeSurface::Sidebar | AppModalHostBridgeSurface::Titlebar
    ) {
        let Some(mut native_host) = cef::v8_value_create_object(None, None) else {
            return;
        };
        let mut handler = GhostexGpuiNativeHostBridgeV8Handler::new();
        let function_name = CefString::from(WEBKIT_POST_MESSAGE_JS_FUNCTION);
        let mut post_message =
            match cef::v8_value_create_function(Some(&function_name), Some(&mut handler)) {
                Some(function) => function,
                None => return,
            };
        native_host.set_value_bykey(
            Some(&function_name),
            Some(&mut post_message),
            V8Propertyattribute::default(),
        );

        let native_host_key = CefString::from(WEBKIT_NATIVE_HOST_MESSAGE_HANDLER_JS_OBJECT);
        message_handlers.set_value_bykey(
            Some(&native_host_key),
            Some(&mut native_host),
            V8Propertyattribute::default(),
        );
    }

    let message_handlers_key = CefString::from(WEBKIT_MESSAGE_HANDLERS_JS_OBJECT);
    webkit.set_value_bykey(
        Some(&message_handlers_key),
        Some(&mut message_handlers),
        V8Propertyattribute::default(),
    );

    let webkit_key = CefString::from(WEBKIT_JS_OBJECT);
    global.set_value_bykey(
        Some(&webkit_key),
        Some(&mut webkit),
        V8Propertyattribute::default(),
    );
}

pub(crate) fn v8_object_property_or_new(parent: &V8Value, key: &str) -> Option<V8Value> {
    let key = CefString::from(key);
    parent
        .value_bykey(Some(&key))
        .filter(|value| value.is_object() != 0)
        .or_else(|| cef::v8_value_create_object(None, None))
}

pub(crate) fn set_v8_string_property(parent: &V8Value, key: &str, value: &str) -> bool {
    let key = CefString::from(key);
    let value = CefString::from(value);
    let Some(mut value) = cef::v8_value_create_string(Some(&value)) else {
        return false;
    };
    parent.set_value_bykey(Some(&key), Some(&mut value), V8Propertyattribute::default()) != 0
}

pub(crate) fn app_modal_host_payload_from_v8_value(value: &V8Value) -> Option<String> {
    if value.is_string() != 0 {
        return Some(CefString::from(&value.string_value()).to_string());
    }

    let Some(context) = cef::v8_context_get_current_context() else {
        return None;
    };
    let Some(global) = context.global() else {
        return None;
    };
    let json_key = CefString::from("JSON");
    let mut json = global
        .value_bykey(Some(&json_key))
        .filter(|value| value.is_object() != 0)?;
    let stringify_key = CefString::from("stringify");
    let stringify = json
        .value_bykey(Some(&stringify_key))
        .filter(|value| value.is_function() != 0)?;
    let argument = value.clone();
    let result = stringify.execute_function(Some(&mut json), Some(&[Some(argument)]))?;
    if result.is_string() == 0 {
        return None;
    }
    Some(CefString::from(&result.string_value()).to_string())
}

pub(crate) fn send_sidebar_install_process_message(
    frame: &mut Frame,
    runtime_settings: SidebarRuntimeSettingsSnapshot,
    gxserver_bootstrap: Option<SidebarGxserverBootstrap>,
) {
    let mut message = match cef::process_message_create(Some(&CefString::from(
        SIDEBAR_PROJECT_CONTEXT_INSTALL_MESSAGE_NAME,
    ))) {
        Some(message) => message,
        None => return,
    };
    attach_sidebar_runtime_settings_to_process_message(&mut message, runtime_settings);
    attach_sidebar_gxserver_bootstrap_to_process_message(
        &mut message,
        SIDEBAR_RUNTIME_SETTINGS_ARGUMENT_COUNT,
        gxserver_bootstrap.as_ref(),
    );
    frame.send_process_message(ProcessId::RENDERER, Some(&mut message));
}

pub(crate) fn send_sidebar_runtime_settings_process_message(
    frame: &mut Frame,
    message_name: &str,
    runtime_settings: SidebarRuntimeSettingsSnapshot,
) {
    let mut message = match cef::process_message_create(Some(&CefString::from(message_name))) {
        Some(message) => message,
        None => return,
    };
    attach_sidebar_runtime_settings_to_process_message(&mut message, runtime_settings);
    frame.send_process_message(ProcessId::RENDERER, Some(&mut message));
}

pub(crate) fn send_sidebar_gxserver_bootstrap_process_message(
    frame: &mut Frame,
    gxserver_bootstrap: Option<SidebarGxserverBootstrap>,
) {
    let mut message = match cef::process_message_create(Some(&CefString::from(
        SIDEBAR_GXSERVER_BOOTSTRAP_UPDATE_MESSAGE_NAME,
    ))) {
        Some(message) => message,
        None => return,
    };
    attach_sidebar_gxserver_bootstrap_to_process_message(
        &mut message,
        0,
        gxserver_bootstrap.as_ref(),
    );
    frame.send_process_message(ProcessId::RENDERER, Some(&mut message));
}

pub(crate) fn send_session_chat_gxserver_bootstrap_process_message(
    frame: &mut Frame,
    gxserver_bootstrap: Option<SidebarGxserverBootstrap>,
) {
    let mut message = match cef::process_message_create(Some(&CefString::from(
        SESSION_CHAT_GXSERVER_BOOTSTRAP_MESSAGE_NAME,
    ))) {
        Some(message) => message,
        None => return,
    };
    attach_sidebar_gxserver_bootstrap_to_process_message(
        &mut message,
        0,
        gxserver_bootstrap.as_ref(),
    );
    frame.send_process_message(ProcessId::RENDERER, Some(&mut message));
}

pub(crate) fn attach_sidebar_runtime_settings_to_process_message(
    message: &mut ProcessMessage,
    runtime_settings: SidebarRuntimeSettingsSnapshot,
) {
    let Some(arguments) = message.argument_list() else {
        return;
    };
    arguments.set_size(SIDEBAR_RUNTIME_SETTINGS_ARGUMENT_COUNT);
    arguments.set_bool(
        SIDEBAR_RUNTIME_SETTINGS_DEBUGGING_MODE_ARGUMENT_INDEX,
        bool_to_cef_int(runtime_settings.debugging_mode),
    );
    arguments.set_bool(
        SIDEBAR_RUNTIME_SETTINGS_SHOW_BETA_FEATURES_ARGUMENT_INDEX,
        bool_to_cef_int(runtime_settings.show_beta_features),
    );
    arguments.set_string(
        SIDEBAR_RUNTIME_SETTINGS_SAVED_SETTINGS_JSON_ARGUMENT_INDEX,
        Some(&CefString::from(bounded_sidebar_saved_settings_json(
            &runtime_settings.saved_settings_json,
        ))),
    );
}

pub(crate) fn attach_sidebar_gxserver_bootstrap_to_process_message(
    message: &mut ProcessMessage,
    offset: usize,
    gxserver_bootstrap: Option<&SidebarGxserverBootstrap>,
) {
    let Some(arguments) = message.argument_list() else {
        return;
    };
    let Some(gxserver_bootstrap) = gxserver_bootstrap else {
        arguments.set_size(offset + 1);
        arguments.set_bool(
            offset + SIDEBAR_GXSERVER_BOOTSTRAP_PRESENT_ARGUMENT_INDEX,
            bool_to_cef_int(false),
        );
        return;
    };

    let visible_session_count = gxserver_bootstrap.visible_session_ids.len();
    arguments.set_size(
        offset
            + SIDEBAR_GXSERVER_BOOTSTRAP_ARGUMENT_COUNT_WITHOUT_VISIBLE_IDS
            + visible_session_count,
    );
    arguments.set_bool(
        offset + SIDEBAR_GXSERVER_BOOTSTRAP_PRESENT_ARGUMENT_INDEX,
        bool_to_cef_int(true),
    );
    arguments.set_string(
        offset + SIDEBAR_GXSERVER_BOOTSTRAP_BASE_URL_ARGUMENT_INDEX,
        Some(&CefString::from(gxserver_bootstrap.base_url.as_str())),
    );
    arguments.set_string(
        offset + SIDEBAR_GXSERVER_BOOTSTRAP_AUTH_TOKEN_ARGUMENT_INDEX,
        Some(&CefString::from(gxserver_bootstrap.auth_token.as_str())),
    );
    arguments.set_int(
        offset + SIDEBAR_GXSERVER_BOOTSTRAP_PROTOCOL_VERSION_ARGUMENT_INDEX,
        gxserver_bootstrap.protocol_version,
    );
    arguments.set_string(
        offset + SIDEBAR_GXSERVER_BOOTSTRAP_CLIENT_ID_ARGUMENT_INDEX,
        Some(&CefString::from(gxserver_bootstrap.client_id.as_str())),
    );
    arguments.set_string(
        offset + SIDEBAR_GXSERVER_BOOTSTRAP_INITIAL_ACTIVE_PROJECT_ID_ARGUMENT_INDEX,
        Some(&CefString::from(
            gxserver_bootstrap
                .initial_active_project_id
                .as_deref()
                .unwrap_or(""),
        )),
    );
    arguments.set_string(
        offset + SIDEBAR_GXSERVER_BOOTSTRAP_FOCUSED_SESSION_ID_ARGUMENT_INDEX,
        Some(&CefString::from(
            gxserver_bootstrap
                .focused_session_id
                .as_deref()
                .unwrap_or(""),
        )),
    );
    arguments.set_int(
        offset + SIDEBAR_GXSERVER_BOOTSTRAP_VISIBLE_SESSION_COUNT_ARGUMENT_INDEX,
        visible_session_count as c_int,
    );
    for (index, session_id) in gxserver_bootstrap.visible_session_ids.iter().enumerate() {
        arguments.set_string(
            offset + SIDEBAR_GXSERVER_BOOTSTRAP_ARGUMENT_COUNT_WITHOUT_VISIBLE_IDS + index,
            Some(&CefString::from(session_id.as_str())),
        );
    }
}

pub(crate) fn sidebar_runtime_settings_from_install_message(
    message: &mut ProcessMessage,
) -> SidebarRuntimeSettingsSnapshot {
    let Some(arguments) = message.argument_list() else {
        return SidebarRuntimeSettingsSnapshot::default();
    };
    if arguments.size() < SIDEBAR_RUNTIME_SETTINGS_ARGUMENT_COUNT {
        return SidebarRuntimeSettingsSnapshot::default();
    }
    if arguments.get_type(SIDEBAR_RUNTIME_SETTINGS_DEBUGGING_MODE_ARGUMENT_INDEX) != ValueType::BOOL
        || arguments.get_type(SIDEBAR_RUNTIME_SETTINGS_SHOW_BETA_FEATURES_ARGUMENT_INDEX)
            != ValueType::BOOL
    {
        return SidebarRuntimeSettingsSnapshot::default();
    }

    SidebarRuntimeSettingsSnapshot {
        debugging_mode: arguments.bool(SIDEBAR_RUNTIME_SETTINGS_DEBUGGING_MODE_ARGUMENT_INDEX) != 0,
        show_beta_features: arguments
            .bool(SIDEBAR_RUNTIME_SETTINGS_SHOW_BETA_FEATURES_ARGUMENT_INDEX)
            != 0,
        saved_settings_json: sidebar_saved_settings_json_from_arguments(&arguments),
    }
}

pub(crate) fn sidebar_gxserver_bootstrap_from_process_message(
    message: &mut ProcessMessage,
    offset: usize,
) -> Option<SidebarGxserverBootstrap> {
    let arguments = message.argument_list()?;
    if arguments.size() <= offset
        || arguments.get_type(offset + SIDEBAR_GXSERVER_BOOTSTRAP_PRESENT_ARGUMENT_INDEX)
            != ValueType::BOOL
        || arguments.bool(offset + SIDEBAR_GXSERVER_BOOTSTRAP_PRESENT_ARGUMENT_INDEX) == 0
    {
        return None;
    }
    if arguments.size() < offset + SIDEBAR_GXSERVER_BOOTSTRAP_ARGUMENT_COUNT_WITHOUT_VISIBLE_IDS {
        return None;
    }
    for index in [
        SIDEBAR_GXSERVER_BOOTSTRAP_BASE_URL_ARGUMENT_INDEX,
        SIDEBAR_GXSERVER_BOOTSTRAP_AUTH_TOKEN_ARGUMENT_INDEX,
        SIDEBAR_GXSERVER_BOOTSTRAP_CLIENT_ID_ARGUMENT_INDEX,
        SIDEBAR_GXSERVER_BOOTSTRAP_INITIAL_ACTIVE_PROJECT_ID_ARGUMENT_INDEX,
        SIDEBAR_GXSERVER_BOOTSTRAP_FOCUSED_SESSION_ID_ARGUMENT_INDEX,
    ] {
        if arguments.get_type(offset + index) != ValueType::STRING {
            return None;
        }
    }
    if arguments.get_type(offset + SIDEBAR_GXSERVER_BOOTSTRAP_PROTOCOL_VERSION_ARGUMENT_INDEX)
        != ValueType::INT
        || arguments
            .get_type(offset + SIDEBAR_GXSERVER_BOOTSTRAP_VISIBLE_SESSION_COUNT_ARGUMENT_INDEX)
            != ValueType::INT
    {
        return None;
    }

    let visible_session_count =
        arguments.int(offset + SIDEBAR_GXSERVER_BOOTSTRAP_VISIBLE_SESSION_COUNT_ARGUMENT_INDEX);
    if visible_session_count < 0 {
        return None;
    }
    let visible_session_count = visible_session_count as usize;
    if arguments.size()
        < offset
            + SIDEBAR_GXSERVER_BOOTSTRAP_ARGUMENT_COUNT_WITHOUT_VISIBLE_IDS
            + visible_session_count
    {
        return None;
    }
    let mut visible_session_ids = Vec::with_capacity(visible_session_count);
    for index in 0..visible_session_count {
        let argument_index =
            offset + SIDEBAR_GXSERVER_BOOTSTRAP_ARGUMENT_COUNT_WITHOUT_VISIBLE_IDS + index;
        if arguments.get_type(argument_index) != ValueType::STRING {
            return None;
        }
        let value = CefString::from(&arguments.string(argument_index)).to_string();
        if !value.trim().is_empty() {
            visible_session_ids.push(value);
        }
    }

    Some(SidebarGxserverBootstrap {
        base_url: CefString::from(
            &arguments.string(offset + SIDEBAR_GXSERVER_BOOTSTRAP_BASE_URL_ARGUMENT_INDEX),
        )
        .to_string(),
        auth_token: CefString::from(
            &arguments.string(offset + SIDEBAR_GXSERVER_BOOTSTRAP_AUTH_TOKEN_ARGUMENT_INDEX),
        )
        .to_string(),
        protocol_version: arguments
            .int(offset + SIDEBAR_GXSERVER_BOOTSTRAP_PROTOCOL_VERSION_ARGUMENT_INDEX),
        client_id: CefString::from(
            &arguments.string(offset + SIDEBAR_GXSERVER_BOOTSTRAP_CLIENT_ID_ARGUMENT_INDEX),
        )
        .to_string(),
        initial_active_project_id: non_empty_cef_argument_string(
            &arguments,
            offset + SIDEBAR_GXSERVER_BOOTSTRAP_INITIAL_ACTIVE_PROJECT_ID_ARGUMENT_INDEX,
        ),
        focused_session_id: non_empty_cef_argument_string(
            &arguments,
            offset + SIDEBAR_GXSERVER_BOOTSTRAP_FOCUSED_SESSION_ID_ARGUMENT_INDEX,
        ),
        visible_session_ids,
    })
}

pub(crate) fn non_empty_cef_argument_string(arguments: &cef::ListValue, index: usize) -> Option<String> {
    let value = CefString::from(&arguments.string(index)).to_string();
    (!value.trim().is_empty()).then_some(value)
}

pub(crate) fn install_sidebar_runtime_settings_v8_object(
    context: &mut cef::V8Context,
    namespace: &mut V8Value,
    runtime_settings: SidebarRuntimeSettingsSnapshot,
) -> Option<V8Value> {
    let Some(mut runtime_settings_object) = cef::v8_value_create_object(None, None) else {
        return None;
    };
    set_v8_bool_property(
        &mut runtime_settings_object,
        SIDEBAR_RUNTIME_SETTINGS_DEBUGGING_MODE_JS_FIELD,
        runtime_settings.debugging_mode,
    );
    set_v8_bool_property(
        &mut runtime_settings_object,
        SIDEBAR_RUNTIME_SETTINGS_SHOW_BETA_FEATURES_JS_FIELD,
        runtime_settings.show_beta_features,
    );
    if let Some(mut settings_object) =
        parse_sidebar_json_v8_object(context, &runtime_settings.saved_settings_json)
    {
        let settings_key = CefString::from(SIDEBAR_RUNTIME_SETTINGS_SAVED_SETTINGS_JS_FIELD);
        runtime_settings_object.set_value_bykey(
            Some(&settings_key),
            Some(&mut settings_object),
            V8Propertyattribute::default(),
        );
    }
    let runtime_settings_key = CefString::from(SIDEBAR_RUNTIME_SETTINGS_JS_OBJECT);
    namespace.set_value_bykey(
        Some(&runtime_settings_key),
        Some(&mut runtime_settings_object),
        V8Propertyattribute::default(),
    );
    Some(runtime_settings_object)
}

pub(crate) fn sidebar_saved_settings_json_from_arguments(arguments: &cef::ListValue) -> String {
    if arguments.size() <= SIDEBAR_RUNTIME_SETTINGS_SAVED_SETTINGS_JSON_ARGUMENT_INDEX
        || arguments.get_type(SIDEBAR_RUNTIME_SETTINGS_SAVED_SETTINGS_JSON_ARGUMENT_INDEX)
            != ValueType::STRING
    {
        return String::new();
    }
    let value = CefString::from(
        &arguments.string(SIDEBAR_RUNTIME_SETTINGS_SAVED_SETTINGS_JSON_ARGUMENT_INDEX),
    )
    .to_string();
    bounded_sidebar_saved_settings_json(&value).to_string()
}

pub(crate) fn bounded_sidebar_saved_settings_json(value: &str) -> &str {
    if value.chars().count() > SIDEBAR_RUNTIME_SETTINGS_SAVED_SETTINGS_JSON_MAX_CHARS {
        return "";
    }
    value
}

pub(crate) fn parse_sidebar_json_v8_object(context: &mut cef::V8Context, json_text: &str) -> Option<V8Value> {
    if json_text.trim().is_empty() {
        return None;
    }
    let global = context.global()?;
    let json_key = CefString::from("JSON");
    let mut json = global
        .value_bykey(Some(&json_key))
        .filter(|value| value.is_object() != 0)?;
    let parse_key = CefString::from("parse");
    let parse = json
        .value_bykey(Some(&parse_key))
        .filter(|value| value.is_function() != 0)?;
    let settings_json = CefString::from(json_text);
    let settings_json_value = cef::v8_value_create_string(Some(&settings_json))?;
    let result = parse.execute_function(Some(&mut json), Some(&[Some(settings_json_value)]))?;
    (result.is_object() != 0).then_some(result)
}

pub(crate) fn install_sidebar_gxserver_bootstrap_v8_object(
    namespace: &mut V8Value,
    gxserver_bootstrap: Option<SidebarGxserverBootstrap>,
) -> Option<V8Value> {
    let Some(mut bootstrap_object) = cef::v8_value_create_object(None, None) else {
        return None;
    };
    if let Some(gxserver_bootstrap) = gxserver_bootstrap {
        set_v8_string_property(
            &bootstrap_object,
            SIDEBAR_GXSERVER_BOOTSTRAP_BASE_URL_JS_FIELD,
            &gxserver_bootstrap.base_url,
        );
        set_v8_string_property(
            &bootstrap_object,
            SIDEBAR_GXSERVER_BOOTSTRAP_AUTH_TOKEN_JS_FIELD,
            &gxserver_bootstrap.auth_token,
        );
        set_v8_int_property(
            &mut bootstrap_object,
            SIDEBAR_GXSERVER_BOOTSTRAP_PROTOCOL_VERSION_JS_FIELD,
            gxserver_bootstrap.protocol_version,
        );
        set_v8_string_property(
            &bootstrap_object,
            SIDEBAR_GXSERVER_BOOTSTRAP_CLIENT_ID_JS_FIELD,
            &gxserver_bootstrap.client_id,
        );
        if let Some(initial_active_project_id) = gxserver_bootstrap.initial_active_project_id {
            set_v8_string_property(
                &bootstrap_object,
                SIDEBAR_GXSERVER_BOOTSTRAP_INITIAL_ACTIVE_PROJECT_ID_JS_FIELD,
                &initial_active_project_id,
            );
        }
        if let Some(focused_session_id) = gxserver_bootstrap.focused_session_id {
            set_v8_string_property(
                &bootstrap_object,
                SIDEBAR_GXSERVER_BOOTSTRAP_FOCUSED_SESSION_ID_JS_FIELD,
                &focused_session_id,
            );
        }
        if !gxserver_bootstrap.visible_session_ids.is_empty() {
            set_v8_string_array_property(
                &mut bootstrap_object,
                SIDEBAR_GXSERVER_BOOTSTRAP_VISIBLE_SESSION_IDS_JS_FIELD,
                &gxserver_bootstrap.visible_session_ids,
            );
        }
    }

    let bootstrap_key = CefString::from(SIDEBAR_GXSERVER_BOOTSTRAP_JS_OBJECT);
    namespace.set_value_bykey(
        Some(&bootstrap_key),
        Some(&mut bootstrap_object),
        V8Propertyattribute::default(),
    );
    Some(bootstrap_object)
}

pub(crate) fn notify_sidebar_runtime_settings_changed(
    context: &mut cef::V8Context,
    namespace: &mut V8Value,
    runtime_settings_object: V8Value,
) {
    let callback_key = CefString::from(SIDEBAR_RUNTIME_SETTINGS_CHANGED_JS_CALLBACK);
    let Some(callback) = namespace
        .value_bykey(Some(&callback_key))
        .filter(|value| value.is_function() != 0)
    else {
        return;
    };
    let arguments = [Some(runtime_settings_object)];
    callback.execute_function_with_context(Some(context), Some(namespace), Some(&arguments));
}

pub(crate) fn notify_sidebar_gxserver_bootstrap_changed(
    context: &mut cef::V8Context,
    namespace: &mut V8Value,
    bootstrap_object: V8Value,
) {
    let callback_key = CefString::from(SIDEBAR_GXSERVER_BOOTSTRAP_CHANGED_JS_CALLBACK);
    let Some(callback) = namespace
        .value_bykey(Some(&callback_key))
        .filter(|value| value.is_function() != 0)
    else {
        return;
    };
    let arguments = [Some(bootstrap_object)];
    callback.execute_function_with_context(Some(context), Some(namespace), Some(&arguments));
}

pub(crate) fn set_v8_bool_property(object: &mut V8Value, key: &str, value: bool) {
    let key = CefString::from(key);
    let mut value = cef::v8_value_create_bool(bool_to_cef_int(value));
    object.set_value_bykey(Some(&key), value.as_mut(), V8Propertyattribute::default());
}

pub(crate) fn set_v8_int_property(object: &mut V8Value, key: &str, value: i32) {
    let key = CefString::from(key);
    let mut value = cef::v8_value_create_int(value);
    object.set_value_bykey(Some(&key), value.as_mut(), V8Propertyattribute::default());
}

pub(crate) fn set_v8_string_array_property(object: &mut V8Value, key: &str, values: &[String]) {
    let Some(mut array) = cef::v8_value_create_array(values.len() as c_int) else {
        return;
    };
    for (index, value) in values.iter().enumerate() {
        let value = CefString::from(value.as_str());
        let Some(mut value) = cef::v8_value_create_string(Some(&value)) else {
            return;
        };
        array.set_value_byindex(index as c_int, Some(&mut value));
    }
    let key = CefString::from(key);
    object.set_value_bykey(Some(&key), Some(&mut array), V8Propertyattribute::default());
}

pub(crate) fn bool_to_cef_int(value: bool) -> c_int {
    if value { 1 } else { 0 }
}

pub(crate) fn send_sidebar_bridge_process_message(process_message_name: &str, payload: &str) -> bool {
    if sidebar_bridge_event_kind_for_process_message(process_message_name).is_none() {
        return false;
    }
    if payload.chars().count() > SIDEBAR_BRIDGE_PAYLOAD_MAX_CHARS {
        return false;
    }

    let Some(context) = cef::v8_context_get_current_context() else {
        return false;
    };
    let Some(frame) = context.frame() else {
        return false;
    };
    let mut message =
        match cef::process_message_create(Some(&CefString::from(process_message_name))) {
            Some(message) => message,
            None => return false,
        };
    let Some(arguments) = message.argument_list() else {
        return false;
    };
    arguments.set_size(1);
    arguments.set_string(0, Some(&CefString::from(payload)));
    frame.send_process_message(ProcessId::BROWSER, Some(&mut message));
    true
}

pub(crate) fn send_project_workarea_bridge_process_message(process_message_name: &str, payload: &str) -> bool {
    if project_workarea_bridge_event_kind_for_process_message(process_message_name).is_none() {
        return false;
    }
    if payload.chars().count() > PROJECT_WORKAREA_BRIDGE_PAYLOAD_MAX_CHARS {
        return false;
    }

    let Some(context) = cef::v8_context_get_current_context() else {
        return false;
    };
    let Some(frame) = context.frame() else {
        return false;
    };
    let mut message =
        match cef::process_message_create(Some(&CefString::from(process_message_name))) {
            Some(message) => message,
            None => return false,
        };
    let Some(arguments) = message.argument_list() else {
        return false;
    };
    arguments.set_size(1);
    arguments.set_string(0, Some(&CefString::from(payload)));
    frame.send_process_message(ProcessId::BROWSER, Some(&mut message));
    true
}

pub(crate) fn send_app_modal_host_bridge_process_message(payload: &str) -> bool {
    if payload.chars().count() > APP_MODAL_HOST_BRIDGE_PAYLOAD_MAX_CHARS {
        return false;
    }

    let Some(context) = cef::v8_context_get_current_context() else {
        return false;
    };
    let Some(frame) = context.frame() else {
        return false;
    };
    let mut message = match cef::process_message_create(Some(&CefString::from(
        APP_MODAL_HOST_BRIDGE_PROCESS_MESSAGE_NAME,
    ))) {
        Some(message) => message,
        None => return false,
    };
    let Some(arguments) = message.argument_list() else {
        return false;
    };
    arguments.set_size(1);
    arguments.set_string(0, Some(&CefString::from(payload)));
    frame.send_process_message(ProcessId::BROWSER, Some(&mut message));
    true
}

pub(crate) fn send_native_host_bridge_process_message(payload: &str) -> bool {
    if payload.chars().count() > NATIVE_HOST_BRIDGE_PAYLOAD_MAX_CHARS {
        return false;
    }

    let Some(context) = cef::v8_context_get_current_context() else {
        return false;
    };
    let Some(frame) = context.frame() else {
        return false;
    };
    let mut message = match cef::process_message_create(Some(&CefString::from(
        NATIVE_HOST_BRIDGE_PROCESS_MESSAGE_NAME,
    ))) {
        Some(message) => message,
        None => return false,
    };
    let Some(arguments) = message.argument_list() else {
        return false;
    };
    arguments.set_size(1);
    arguments.set_string(0, Some(&CefString::from(payload)));
    frame.send_process_message(ProcessId::BROWSER, Some(&mut message));
    true
}

pub(crate) fn set_v8_bool_return(retval: Option<&mut Option<V8Value>>, value: bool) {
    if let Some(retval) = retval {
        *retval = cef::v8_value_create_bool(if value { 1 } else { 0 });
    }
}
