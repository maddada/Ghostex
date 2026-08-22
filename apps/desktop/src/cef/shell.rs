pub use super::sidebar_bridge_manifest::AppModalHostBridgeSurface;
use super::sidebar_bridge_manifest::{
    APP_MODAL_HOST_BRIDGE_PAYLOAD_MAX_CHARS, APP_MODAL_HOST_BRIDGE_PROCESS_MESSAGE_NAME,
    APP_MODAL_HOST_BRIDGE_SURFACE_SPECS, APP_MODAL_HOST_ID_JS_FIELD, APP_MODAL_HOST_ID_VALUE,
    APP_MODAL_HOST_SURFACE_JS_FIELD, APP_MODAL_HOST_SURFACE_VALUE,
    NATIVE_HOST_BRIDGE_PAYLOAD_MAX_CHARS, NATIVE_HOST_BRIDGE_PROCESS_MESSAGE_NAME,
    PROJECT_WORKAREA_BRIDGE_FUNCTION_SPECS, PROJECT_WORKAREA_BRIDGE_INSTALL_MESSAGE_NAME,
    PROJECT_WORKAREA_BRIDGE_PAYLOAD_MAX_CHARS, PROJECT_WORKAREA_MANAGE_DOCS_RESOURCE_BASE_URL,
    PROJECT_WORKAREA_MANAGE_DOCS_RESOURCE_BASE_URL_JS_FIELD, ProjectWorkareaBridgeFunctionId,
    SIDEBAR_BRIDGE_FUNCTION_SPECS, SIDEBAR_BRIDGE_PAYLOAD_MAX_CHARS,
    SIDEBAR_EDITABLE_FOCUS_PROCESS_MESSAGE_NAME, SIDEBAR_PROJECT_CONTEXT_JS_NAMESPACE,
    SidebarBridgeFunctionId, WEBKIT_APP_MODAL_HOST_MESSAGE_HANDLER_JS_OBJECT, WEBKIT_JS_OBJECT,
    WEBKIT_MESSAGE_HANDLERS_JS_OBJECT, WEBKIT_NATIVE_HOST_MESSAGE_HANDLER_JS_OBJECT,
    WEBKIT_POST_MESSAGE_JS_FUNCTION, project_workarea_bridge_function_spec_for_js_function,
    project_workarea_bridge_function_spec_for_process_message,
    sidebar_bridge_function_spec_for_js_function, sidebar_bridge_function_spec_for_process_message,
};
use crate::support_logs::{self, GpuiSupportLog};
use anyhow::{Context as _, Result};
use cef::rc::Rc as _;
use cef::wrapper::resource_manager::{get_mime_type, get_url_without_query_or_fragment};
use cef::{
    App, BrowserProcessHandler, BrowserSettings, Callback, CefString, Client, CommandLine,
    ContentSettingTypes, ContentSettingValues, ContextMenuHandler, ContextMenuParams,
    DictionaryValue, DisplayHandler, EventFlags, FindHandler, FocusHandler, FocusSource, Frame,
    ImplApp, ImplBrowser as _, ImplBrowserHost as _, ImplBrowserProcessHandler, ImplClient,
    ImplCommandLine as _, ImplContextMenuHandler, ImplContextMenuParams as _,
    ImplDictionaryValue as _, ImplDisplayHandler, ImplFindHandler, ImplFocusHandler,
    ImplFrame as _, ImplLifeSpanHandler, ImplListValue as _, ImplLoadHandler,
    ImplMediaAccessCallback as _, ImplMenuModel as _, ImplPermissionHandler,
    ImplPermissionPromptCallback as _, ImplProcessMessage as _, ImplRenderProcessHandler,
    ImplRequest as _, ImplRequestContext as _, ImplRequestHandler, ImplResourceHandler,
    ImplResourceRequestHandler, ImplResponse as _, ImplStreamReader as _, ImplTask,
    ImplV8Context as _, ImplV8Handler, ImplV8Value as _, KeyboardHandler, LifeSpanHandler,
    LoadHandler, MediaAccessCallback, MediaAccessPermissionTypes, MenuModel, PermissionHandler,
    PermissionPromptCallback, PermissionRequestResult, PermissionRequestTypes, PopupFeatures,
    ProcessId, ProcessMessage, RenderProcessHandler, Request, RequestHandler, ResourceHandler,
    ResourceReadCallback, ResourceRequestHandler, Response, ReturnValue, State, StreamReader, Task,
    TerminationStatus, ThreadId, UnresponsiveProcessCallback, V8Handler, V8Propertyattribute,
    V8Value, ValueType, WindowInfo, WindowOpenDisposition, WrapApp, WrapBrowserProcessHandler,
    WrapClient, WrapContextMenuHandler, WrapDisplayHandler, WrapFindHandler, WrapFocusHandler,
    WrapLifeSpanHandler, WrapLoadHandler, WrapPermissionHandler, WrapRenderProcessHandler,
    WrapRequestHandler, WrapResourceHandler, WrapResourceRequestHandler, WrapTask, WrapV8Handler,
    ZoomCommand, post_task, stream_reader_create_for_file, string_multimap_alloc,
    string_multimap_append, wrap_app, wrap_browser_process_handler, wrap_client,
    wrap_context_menu_handler, wrap_display_handler, wrap_find_handler, wrap_focus_handler,
    wrap_life_span_handler, wrap_load_handler, wrap_permission_handler,
    wrap_render_process_handler, wrap_request_handler, wrap_resource_handler,
    wrap_resource_request_handler, wrap_task, wrap_v8_handler,
};
#[cfg(target_os = "windows")]
use cef::{
    ImplKeyboardHandler, KeyEvent, KeyEventType, WrapKeyboardHandler, wrap_keyboard_handler,
};
use gpui::{Bounds, Pixels};
use percent_encoding::percent_decode_str;
use std::{
    cell::{Cell, RefCell},
    collections::{HashMap, HashSet},
    ffi::{c_int, c_void},
    path::PathBuf,
    rc::Rc as StdRc,
    sync::{
        Arc, Mutex, OnceLock,
        atomic::{AtomicBool, AtomicUsize, Ordering},
    },
    time::Instant,
};

fn cef_resize_diagnostics_enabled() -> bool {
    static ENABLED: OnceLock<bool> = OnceLock::new();
    *ENABLED.get_or_init(|| std::env::var_os("GHOSTEX_GPUI_CEF_RESIZE_DIAGNOSTICS").is_some())
}

/*
CDXC:GPUICefPlatformSeam 2026-07-04:
This module owns every platform-independent piece of the windowed-CEF
backend: runtime init/shutdown ordering, the app/client/bridge handler
machinery, and the CefBrowser wrapper. Truly per-OS behavior (framework
loading, message-pump scheduling into the native run loop, child-view
frame/visibility/focus, child WindowInfo construction) lives behind the
`super::platform` seam (cef/macos.rs, cef/windows.rs, or cef/linux_x11.rs).
Shared code treats native child-view handles as opaque `*mut c_void`; only
the platform module converts them to an NSView*, HWND, or X11 window id.
*/
use super::platform;

struct CefRuntimeState {
    _platform: platform::PlatformCefRuntime,
    _app: cef::App,
}

static CEF_RUNTIME: OnceLock<Mutex<Option<CefRuntimeState>>> = OnceLock::new();
static CEF_CONTEXT_INITIALIZED: AtomicBool = AtomicBool::new(false);
static CEF_SHUTDOWN_IN_PROGRESS: AtomicBool = AtomicBool::new(false);
const SIDEBAR_PROJECT_CONTEXT_INSTALL_MESSAGE_NAME: &str =
    "ghostex.gpui.sidebar.installActiveProjectContextBridge";
const SIDEBAR_RUNTIME_SETTINGS_UPDATE_MESSAGE_NAME: &str =
    "ghostex.gpui.sidebar.runtimeSettingsChanged";
const SIDEBAR_GXSERVER_BOOTSTRAP_UPDATE_MESSAGE_NAME: &str =
    "ghostex.gpui.sidebar.gxserverBootstrapChanged";
/*
CDXC:GPUISessionChatSurface 2026-07-31:
The Session Chat pane surface needs only the gxserver bootstrap
(baseUrl/token/protocolVersion), never the sidebar post-function bridge. The
sidebar bootstrap-update path deliberately refuses pages without the full
installed sidebar bridge, so chat surfaces use this dedicated message that
installs exactly `window.ghostexGpui.gxserverBootstrap` on the bundled
chat.html renderer.
*/
const SESSION_CHAT_GXSERVER_BOOTSTRAP_MESSAGE_NAME: &str =
    "ghostex.gpui.sessionChat.gxserverBootstrap";
const SIDEBAR_RUNTIME_SETTINGS_JS_OBJECT: &str = "runtimeSettings";
const SIDEBAR_RUNTIME_SETTINGS_CHANGED_JS_CALLBACK: &str = "onRuntimeSettingsChanged";
const SIDEBAR_RUNTIME_SETTINGS_DEBUGGING_MODE_JS_FIELD: &str = "debuggingMode";
const SIDEBAR_RUNTIME_SETTINGS_SHOW_BETA_FEATURES_JS_FIELD: &str = "showBetaFeatures";
const SIDEBAR_RUNTIME_SETTINGS_SAVED_SETTINGS_JS_FIELD: &str = "settings";
const SIDEBAR_GXSERVER_BOOTSTRAP_JS_OBJECT: &str = "gxserverBootstrap";
const SIDEBAR_GXSERVER_BOOTSTRAP_CHANGED_JS_CALLBACK: &str = "onGxserverBootstrapChanged";
const SIDEBAR_GXSERVER_BOOTSTRAP_BASE_URL_JS_FIELD: &str = "baseUrl";
const SIDEBAR_GXSERVER_BOOTSTRAP_AUTH_TOKEN_JS_FIELD: &str = "authToken";
const SIDEBAR_GXSERVER_BOOTSTRAP_PROTOCOL_VERSION_JS_FIELD: &str = "protocolVersion";
const SIDEBAR_GXSERVER_BOOTSTRAP_CLIENT_ID_JS_FIELD: &str = "clientId";
const SIDEBAR_GXSERVER_BOOTSTRAP_INITIAL_ACTIVE_PROJECT_ID_JS_FIELD: &str =
    "initialActiveProjectId";
const SIDEBAR_GXSERVER_BOOTSTRAP_FOCUSED_SESSION_ID_JS_FIELD: &str = "focusedSessionId";
const SIDEBAR_GXSERVER_BOOTSTRAP_VISIBLE_SESSION_IDS_JS_FIELD: &str = "visibleSessionIds";
const CEF_BROWSER_PAGE_BACKGROUND_COLOR: u32 = 0xFFFF_FFFF;
const CEF_CONTEXT_MENU_INSPECT_ELEMENT_COMMAND_ID: c_int = 26_001;
// Stable Chromium content-context commands used by the production macOS CEF
// host (cef_command_ids.h).
const CEF_CONTEXT_MENU_OPEN_LINK_NEW_TAB_COMMAND_ID: c_int = 50_100;
const CEF_CONTEXT_MENU_OPEN_LINK_NEW_WINDOW_COMMAND_ID: c_int = 50_101;
const SIDEBAR_RUNTIME_SETTINGS_DEBUGGING_MODE_ARGUMENT_INDEX: usize = 0;
const SIDEBAR_RUNTIME_SETTINGS_SHOW_BETA_FEATURES_ARGUMENT_INDEX: usize = 1;
const SIDEBAR_RUNTIME_SETTINGS_SAVED_SETTINGS_JSON_ARGUMENT_INDEX: usize = 2;
const SIDEBAR_RUNTIME_SETTINGS_ARGUMENT_COUNT: usize = 3;
const SIDEBAR_RUNTIME_SETTINGS_SAVED_SETTINGS_JSON_MAX_CHARS: usize = 1024 * 1024;
const SIDEBAR_GXSERVER_BOOTSTRAP_PRESENT_ARGUMENT_INDEX: usize = 0;
const SIDEBAR_GXSERVER_BOOTSTRAP_BASE_URL_ARGUMENT_INDEX: usize = 1;
const SIDEBAR_GXSERVER_BOOTSTRAP_AUTH_TOKEN_ARGUMENT_INDEX: usize = 2;
const SIDEBAR_GXSERVER_BOOTSTRAP_PROTOCOL_VERSION_ARGUMENT_INDEX: usize = 3;
const SIDEBAR_GXSERVER_BOOTSTRAP_CLIENT_ID_ARGUMENT_INDEX: usize = 4;
const SIDEBAR_GXSERVER_BOOTSTRAP_INITIAL_ACTIVE_PROJECT_ID_ARGUMENT_INDEX: usize = 5;
const SIDEBAR_GXSERVER_BOOTSTRAP_FOCUSED_SESSION_ID_ARGUMENT_INDEX: usize = 6;
const SIDEBAR_GXSERVER_BOOTSTRAP_VISIBLE_SESSION_COUNT_ARGUMENT_INDEX: usize = 7;
const SIDEBAR_GXSERVER_BOOTSTRAP_ARGUMENT_COUNT_WITHOUT_VISIBLE_IDS: usize = 8;
const BROWSER_APP_OWNED_SCRIPT_URL: &str = "ghostex://gpui/browser-feedback";

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum SidebarBridgeEventKind {
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
    fn forwarded_from(function_id: SidebarBridgeFunctionId) -> Option<Self> {
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
enum ProjectWorkareaBridgeEventKind {
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
enum BrowserPopupDispatchPolicy {
    DispatchShellOpen,
    HandleWithoutDispatch,
}

impl BrowserPopupDispatchPolicy {
    /*
    CDXC:GPUIBrowserRuntimePolicy 2026-06-23-12:48:
    The CEF backend must mirror the shell popup policy before crossing into GPUI app state. Non-empty target URLs dispatch the shell-owned Browser tab path; empty targets are handled inside CEF with no shell callback, no address-only tab, no content transfer fallback, no filesystem/browser-store access, and no URL/title/page logging.
    */
    fn for_target_url(target_url: &str) -> Self {
        if target_url.trim().is_empty() {
            Self::HandleWithoutDispatch
        } else {
            Self::DispatchShellOpen
        }
    }

    fn dispatches_shell_open(self) -> bool {
        matches!(self, Self::DispatchShellOpen)
    }
}

fn browser_popup_target_url_for_shell(target_url: Option<&CefString>) -> Option<String> {
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
fn browser_popup_placement_for_disposition(
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
const FIRST_PARTY_CEF_ENTRY_PATH_MARKERS: [&str; 2] =
    ["/Contents/Resources/sidebar/", "/dist/sidebar/"];
// Windows and Linux share the bundle-less flat layout: the sidebar ships at
// dist/sidebar beside the executable (see build-windows-app.ps1 /
// build-linux-app.sh).
#[cfg(any(target_os = "windows", target_os = "linux"))]
const FIRST_PARTY_CEF_ENTRY_PATH_MARKERS: [&str; 2] = ["/resources/sidebar/", "/dist/sidebar/"];

fn is_gpui_first_party_cef_entry_url(url: &str, entry_file_name: &str) -> bool {
    let Some(base) = url.split(['?', '#']).next() else {
        return false;
    };
    base.starts_with("file://")
        && base.ends_with(&format!("/{entry_file_name}"))
        && FIRST_PARTY_CEF_ENTRY_PATH_MARKERS
            .iter()
            .any(|marker| base.contains(marker))
}

fn app_modal_host_bridge_surface_for_frame_url(url: &str) -> Option<AppModalHostBridgeSurface> {
    APP_MODAL_HOST_BRIDGE_SURFACE_SPECS
        .iter()
        .find(|spec| is_gpui_first_party_cef_entry_url(url, spec.entry_file_name))
        .map(|spec| spec.surface)
}

thread_local! {
    static CEF_BROWSERS_BY_NATIVE_VIEW: RefCell<HashMap<usize, cef::Browser>> = RefCell::new(HashMap::new());
    static KEYBOARD_ZOOM_CEF_NATIVE_VIEWS: RefCell<HashSet<usize>> = RefCell::new(HashSet::new());
    static CEF_GLOBAL_REQUEST_CONTEXT: RefCell<Option<cef::RequestContext>> = const { RefCell::new(None) };
    static CEF_REQUEST_CONTEXTS_BY_PROFILE: RefCell<HashMap<String, cef::RequestContext>> = RefCell::new(HashMap::new());
    static APP_MODAL_HOST_BRIDGE_SURFACES_BY_BROWSER_ID: RefCell<HashMap<c_int, AppModalHostBridgeSurface>> = RefCell::new(HashMap::new());
    // Native views the app has explicitly hidden via CefBrowser::set_visible.
    // The focus handler consults this so a hidden surface can never take
    // native keyboard focus (see GhostexGpuiCefFocusHandler).
    static HIDDEN_CEF_NATIVE_VIEWS: RefCell<HashSet<usize>> = RefCell::new(HashSet::new());
    static SYSTEM_PAGE_APPEARANCE_CEF_NATIVE_VIEWS: RefCell<HashSet<usize>> = RefCell::new(HashSet::new());
    static PAGE_APPEARANCE_DEVTOOLS_MESSAGE_ID: Cell<c_int> = const { Cell::new(0) };
}

// AppKit grants and Chromium focus callbacks can run on different native
// threads. Keep the one process-wide active CEF identity outside the
// thread-local browser registries so a GPUI-root handoff is immediately
// visible to the focus guard on whichever thread CEF invokes it.
static ACTIVE_CEF_NATIVE_VIEW: AtomicUsize = AtomicUsize::new(0);

/*
CDXC:GPUIWindowsSidebarEditableFocus 2026-07-25:
Windows Chromium can report the final focus transfer into a newly mounted
sidebar input as NAVIGATION even though the app already authorized that exact
editable node through the fixed sidebar bridge. Keep that narrow grant
separate from general active-CEF tracking: renderer focus requests remain
unable to claim another surface, while the granted sidebar browser may finish
moving focus from its wrapper HWND into Chromium's keyboard widget.
*/
static SIDEBAR_EDITABLE_FOCUS_NATIVE_VIEW: AtomicUsize = AtomicUsize::new(0);

fn active_cef_native_view() -> Option<usize> {
    match ACTIVE_CEF_NATIVE_VIEW.load(Ordering::Acquire) {
        0 => None,
        native_view => Some(native_view),
    }
}

fn set_active_cef_native_view(native_view: usize) {
    ACTIVE_CEF_NATIVE_VIEW.store(native_view, Ordering::Release);
}

fn set_cef_native_view_hidden(native_view: *mut c_void, hidden: bool) {
    if native_view.is_null() {
        return;
    }
    HIDDEN_CEF_NATIVE_VIEWS.with(|views| {
        if hidden {
            views.borrow_mut().insert(native_view as usize);
        } else {
            views.borrow_mut().remove(&(native_view as usize));
        }
    });
}

fn cef_native_view_is_hidden(native_view: *mut c_void) -> bool {
    if native_view.is_null() {
        return false;
    }
    HIDDEN_CEF_NATIVE_VIEWS.with(|views| views.borrow().contains(&(native_view as usize)))
}

pub fn prepare_application() {
    platform::prepare_application();
}

#[cfg(target_os = "macos")]
pub fn refresh_application_menu_hooks() {
    platform::install_application_hooks();
}

pub fn focus_native_view(native_view: *mut c_void) {
    platform::focus_native_view(native_view);
}

pub fn focus_gpui_root_view(native_view: *mut c_void) {
    platform::focus_gpui_root_view(native_view);
}

#[cfg(target_os = "windows")]
pub fn gpui_root_view_has_native_focus(native_view: *mut c_void) -> bool {
    platform::native_view_has_direct_focus(native_view)
}

#[cfg(target_os = "macos")]
pub fn install_first_responder_observer(native_view: *mut c_void) {
    platform::install_first_responder_observer(native_view);
}

#[cfg(target_os = "macos")]
pub fn set_sidebar_pointer_tracking_view(native_view: *mut c_void) {
    /*
    CDXC:GPUISidebarPointerTracking 2026-08-02:
    Registers the sidebar CEF child view with the AppKit sendEvent observer so
    Rust learns when the pointer crosses the sidebar's frame and when a
    mouse-down lands outside it (sticky-hover reset + context-menu dismissal).
    */
    platform::set_sidebar_pointer_tracking_view(native_view);
}

/*
CDXC:GPUISidebarPointerTracking 2026-08-20:
`data-native-pointer-inside` is only cheap because the AppKit observer keeps a
cache of what the page was last told and skips redundant writes on the
mouse-moved path. Window activation changes have to move through that same
cache instead of writing the page directly, or the cache and the page disagree
and the next real crossing is dropped as redundant.
*/
#[cfg(target_os = "macos")]
pub fn report_sidebar_pointer_outside() {
    platform::report_sidebar_pointer_outside();
}

#[cfg(target_os = "macos")]
pub fn refresh_sidebar_pointer_inside() {
    platform::refresh_sidebar_pointer_inside();
}

#[cfg(target_os = "macos")]
pub fn native_view_contains_responder(
    root_native_view: *mut c_void,
    responder: *mut c_void,
) -> bool {
    platform::native_view_contains_responder(root_native_view, responder)
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
    fn with_payload(self, payload: String) -> SidebarBridgeEvent {
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

fn sidebar_bridge_event_kind_for_process_message(
    process_message_name: &str,
) -> Option<SidebarBridgeEventKind> {
    sidebar_bridge_function_spec_for_process_message(process_message_name)
        .and_then(|spec| SidebarBridgeEventKind::forwarded_from(spec.id))
}

fn sidebar_bridge_installed_for_handler(handler_present: bool) -> bool {
    handler_present
}

impl ProjectWorkareaBridgeEventKind {
    fn with_payload(self, payload: String) -> ProjectWorkareaBridgeEvent {
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

fn project_workarea_bridge_event_kind_for_process_message(
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
    requesting_origin: String,
    kinds: BrowserMediaAccessKinds,
    callback: Option<MediaAccessCallback>,
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

pub fn initialize(cx: &gpui::App) -> Result<()> {
    let state = CEF_RUNTIME.get_or_init(|| Mutex::new(None));
    let mut state = state
        .lock()
        .expect("CEF runtime mutex should not be poisoned");
    if state.is_some() {
        return Ok(());
    }
    CEF_SHUTDOWN_IN_PROGRESS.store(false, Ordering::Release);

    let args = cef::args::Args::new();
    let platform_runtime = platform::load_cef_runtime()?;

    let _ = cef::api_hash(cef::sys::CEF_API_VERSION_LAST, 0);
    platform::install_application_hooks();

    let mut app = GhostexGpuiCefApp::new();
    let process_exit_code = cef::execute_process(
        Some(args.as_main_args()),
        Some(&mut app),
        std::ptr::null_mut(),
    );
    if process_exit_code >= 0 {
        std::process::exit(process_exit_code);
    }

    let root_cache_path = cef_root_cache_path()?;
    /*
    CDXC:GPUIPrivacyAudit 2026-06-23-13:18:
    The built-in Default Browser profile and first-party app UI use the app-owned persistent global CEF store, while generated Browser profiles remain memory-backed. Keep CEF file logging disabled and Chromium runtime data out of support-bundle logs.
    */
    let mut settings = cef::Settings {
        no_sandbox: 1,
        external_message_pump: 1,
        cache_path: cef::CefString::from(root_cache_path.to_string_lossy().as_ref()),
        root_cache_path: cef::CefString::from(root_cache_path.to_string_lossy().as_ref()),
        persist_session_cookies: 1,
        log_severity: cef::LogSeverity::DISABLE,
        remote_debugging_port: remote_debugging_port(),
        ..Default::default()
    };
    platform::apply_platform_settings(&mut settings);

    /*
    CDXC:GPUICefRuntime 2026-06-14-15:25:
    The GPUI shell must use Tauri's cef-rs binding path instead of the earlier GhostexCEFBridge.mm browser wrapper. Initialize CEF through cef-rs, keep GPUI as the AppKit loop owner, and scope profile data to the GPUI app so the React sidebar and main browser share a stable Chromium runtime without production-host coupling.

    CDXC:GPUICefMessagePump 2026-06-14-16:29:
    GPUI runs a blocking NSApplication loop, so the cef-rs port must use CEF's external_message_pump together with BrowserProcessHandler::on_schedule_message_pump_work. CEF schedules each pump step and the AppKit shim executes it on the main queue, avoiding the Chromium run-loop observer trap caused by an unconditional timer.

    CDXC:GPUICefMessagePump 2026-06-14-16:54:
    CEF can call on_schedule_message_pump_work during cef::initialize before the first browser is created. Install the GPUI pump gate before initialization so those startup callbacks reach the main queue instead of leaving Chromium partially initialized with only helper processes alive.
    */
    platform::install_message_pump(cx);
    let initialized = cef::initialize(
        Some(args.as_main_args()),
        Some(&settings),
        Some(&mut app),
        std::ptr::null_mut(),
    );
    if initialized != 1 {
        platform::invalidate_message_pump();
        #[cfg(target_os = "windows")]
        if cef::get_exit_code() == cef::Resultcode::NORMAL_EXIT_AUTO_DE_ELEVATED.get_raw() {
            std::process::exit(0);
        }
        anyhow::bail!("CEF initialization returned false");
    }

    *state = Some(CefRuntimeState {
        _platform: platform_runtime,
        _app: app,
    });
    Ok(())
}

pub fn context_initialized() -> bool {
    CEF_CONTEXT_INITIALIZED.load(Ordering::Acquire)
}

wrap_app! {
    struct GhostexGpuiCefApp;

    impl App {
        fn browser_process_handler(&self) -> Option<BrowserProcessHandler> {
            Some(GhostexGpuiBrowserProcessHandler::new())
        }

        fn render_process_handler(&self) -> Option<RenderProcessHandler> {
            Some(GhostexGpuiRenderProcessHandler::new())
        }

        fn on_before_command_line_processing(
            &self,
            _process_type: Option<&CefString>,
            command_line: Option<&mut CommandLine>,
        ) {
            /*
            CDXC:GPUICefCommandLine 2026-06-14-17:00:
            The GPUI shell is a local app and must not block CEF startup on macOS Keychain prompts or locks. Match production Ghostex's CEF switch set by using Chromium's mock keychain and keeping browser subprocesses foreground-capable for embedded child views.
            */
            if let Some(command_line) = command_line {
                command_line.append_switch(Some(&CefString::from("use-mock-keychain")));
                command_line.append_switch(Some(&CefString::from("enable-fullscreen")));
                command_line.append_switch(Some(&CefString::from("allow-insecure-localhost")));
                command_line.append_switch_with_value(
                    Some(&CefString::from("remote-allow-origins")),
                    Some(&CefString::from("*")),
                );
                // Per-OS Chromium switches (e.g. Linux forcing Ozone onto
                // X11 to match the app-wide X11 embedding constraint) stay
                // behind the platform seam like every other OS-specific
                // decision.
                platform::append_platform_command_line_switches(command_line);
            }
        }
    }
}

wrap_browser_process_handler! {
    struct GhostexGpuiBrowserProcessHandler;

    impl BrowserProcessHandler {
        fn on_context_initialized(&self) {
            /*
            CEF's global request context and synchronous browser creation are
            valid only after this callback. cef::initialize returning does not
            imply that Chromium has reached that point, especially with a new
            profile on first launch.
            */
            CEF_CONTEXT_INITIALIZED.store(true, Ordering::Release);
        }

        fn on_before_child_process_launch(&self, command_line: Option<&mut CommandLine>) {
            if let Some(command_line) = command_line {
                command_line.append_switch(Some(&CefString::from("disable-background-mode")));
                command_line.append_switch(Some(&CefString::from(
                    "disable-backgrounding-occluded-windows",
                )));
            }
        }

        fn on_schedule_message_pump_work(&self, delay_ms: i64) {
            platform::schedule_message_pump_work(delay_ms);
        }
    }
}

/*
CDXC:GPUICefNativeFocus 2026-07-09:
Renderer-initiated focus requests (page JS focus()/re-render focus recovery)
must never move AppKit first-responder to a CEF child view: the shared
sidebar re-renders on every gxserver presentation delta, and without this
handler each render pulled key focus away from the active terminal for a few
milliseconds (dropped keystrokes; occasionally permanently until a click).
Native focus for CEF surfaces is exclusively app-owned: user mouse-down via
the AppKit focus subclass, or explicit Rust `focus_native_view` +
`host.set_focus` calls, both of which arrive here as FOCUS_SOURCE_SYSTEM and
stay allowed. Canceling NAVIGATION-source requests does not affect the
page's internal DOM focus, only the native first-responder transfer.
*/
wrap_focus_handler! {
    struct GhostexGpuiCefFocusHandler;

    impl FocusHandler {
        fn on_set_focus(
            &self,
            browser: Option<&mut cef::Browser>,
            source: FocusSource,
        ) -> c_int {
            /*
            CDXC:GPUICefNativeFocus 2026-07-10:
            NAVIGATION-source cancellation alone proved insufficient: a hidden
            titlebar-host page (Tips stays alive after close) requested native
            focus every ~30s through its keep-awake poll, and Chromium
            delivered that renderer-driven request as FOCUS_SOURCE_SYSTEM —
            stranding keyboard focus on an invisible surface until the next
            terminal click. A surface the app has hidden has no focus claim
            from any source, so cancel those requests outright.

            CDXC:GPUICefNativeFocus 2026-07-10-14:30:
            The hidden-only SYSTEM guard was still insufficient: visible
            background surfaces (a non-active project-workarea page plus the
            sidebar, each running the shared app's blur-recovery focus())
            stole first responder from each other as allowed SYSTEM requests,
            producing a millisecond-cadence focus war that saturated the main
            thread (2026-07-10 beach ball). Every app-owned grant — user
            mouse-down via the focus subclass, Rust `focus_native_view` +
            `host.set_focus`, and the edit-command responder shim — makes the
            CEF view first responder *before* Chromium raises OnSetFocus, so
            the app-owned arbitration rule is: a CEF surface may complete a
            native focus transfer only if it already contains the window's
            first responder. Requests from any surface that does not own the
            responder are renderer-initiated steals and are canceled
            regardless of source or visibility.
            */
            let (native_view, browser_id) = browser
                .map(|browser| {
                    (
                        browser
                            .host()
                            .map(|host| platform::native_view_ptr(host.window_handle())),
                        Some(browser.identifier()),
                    )
                })
                .unwrap_or((None, None));
            let hidden = native_view.is_some_and(cef_native_view_is_hidden);
            let responder_outside = native_view
                .is_some_and(|native_view| !platform::native_view_owns_first_responder(native_view));
            /*
            CDXC:GPUICefExplicitNativeFocusOwnership 2026-07-15:
            `native_view_owns_first_responder` cannot prove a SYSTEM request is
            app-owned: Chromium moves its NSView into first-responder position
            before invoking OnSetFocus, so a renderer `focus()` call satisfies
            that check while stealing input from a GPUI terminal. The existing
            active-native-view registry is the non-circular authority. AppKit
            CEF mouseDown and explicit Rust `focus_native_view` calls mark the
            exact browser root before requesting focus; returning focus to the
            GPUI root clears the registry. Reject every request that lacks that
            explicit current ownership, even if Chromium already changed the
            responder underneath us.
            */
            let explicitly_active = native_view
                .is_some_and(|native_view| {
                    active_cef_native_view() == Some(native_view as usize)
                        || (cfg!(target_os = "windows")
                            && platform::native_view_owns_first_responder(native_view))
                });
            let sidebar_editable_focus_granted = native_view.is_some_and(|native_view| {
                SIDEBAR_EDITABLE_FOCUS_NATIVE_VIEW.load(Ordering::Acquire)
                    == native_view as usize
            });
            #[cfg(target_os = "windows")]
            let cancel = hidden
                || (source == FocusSource::NAVIGATION && !sidebar_editable_focus_granted);
            #[cfg(not(target_os = "windows"))]
            let cancel = hidden
                || responder_outside
                || !explicitly_active
                || source == FocusSource::NAVIGATION;
            #[cfg(target_os = "windows")]
            if !cancel {
                if let Some(native_view) = native_view {
                    set_active_cef_native_view(native_view as usize);
                }
            }
            crate::support_logs::append(
                crate::support_logs::GpuiSupportLog::TerminalFocus,
                "gpui.terminalFocus.cefNativeFocusRequest",
                serde_json::json!({
                    "browserId": browser_id,
                    "source": format!("{source:?}"),
                    "surfaceHidden": hidden,
                    "responderOutside": responder_outside,
                    "explicitlyActive": explicitly_active,
                    "sidebarEditableFocusGranted": sidebar_editable_focus_granted,
                    "canceled": cancel,
                }),
            );
            cancel as c_int
        }
    }
}

/*
CDXC:GPUICefPaneZoomShortcutsWindows 2026-08-12:
Windowed CEF owns keyboard focus in its Chromium child HWND on Windows, so
GPUI's Ctrl+=, Ctrl+-, and Ctrl+0 bindings cannot observe those keystrokes.
Install this handler only on Browser, main project-workarea, and Session Chat
clients (the same surfaces registered for macOS keyboard zoom) and consume the
Windows primary-modifier chord through Chromium's browser-host zoom API.
Sidebar, modal, titlebar, companion, and DevTools clients deliberately receive
no handler. The macOS AppKit responder path remains unchanged.
*/
#[cfg(target_os = "windows")]
wrap_keyboard_handler! {
    struct GhostexGpuiWindowsZoomKeyboardHandler;

    impl KeyboardHandler {
        fn on_pre_key_event(
            &self,
            browser: Option<&mut cef::Browser>,
            event: Option<&KeyEvent>,
            _os_event: Option<&mut cef::sys::MSG>,
            _is_keyboard_shortcut: Option<&mut c_int>,
        ) -> c_int {
            const VK_0: c_int = 0x30;
            const VK_OEM_PLUS: c_int = 0xBB;
            const VK_OEM_MINUS: c_int = 0xBD;
            const CONTROL_DOWN: u32 =
                cef::sys::cef_event_flags_t::EVENTFLAG_CONTROL_DOWN.0 as u32;
            const ALT_DOWN: u32 = cef::sys::cef_event_flags_t::EVENTFLAG_ALT_DOWN.0 as u32;
            const COMMAND_DOWN: u32 =
                cef::sys::cef_event_flags_t::EVENTFLAG_COMMAND_DOWN.0 as u32;

            let Some(event) = event else {
                return 0;
            };
            if event.type_ != KeyEventType::RAWKEYDOWN
                || event.modifiers & CONTROL_DOWN == 0
                || event.modifiers & (ALT_DOWN | COMMAND_DOWN) != 0
            {
                return 0;
            }
            let command = match event.windows_key_code {
                VK_OEM_PLUS => ZoomCommand::IN,
                VK_OEM_MINUS => ZoomCommand::OUT,
                VK_0 => ZoomCommand::RESET,
                _ => return 0,
            };
            let Some(host) = browser.and_then(|browser| browser.host()) else {
                return 0;
            };
            host.zoom(command);
            1
        }
    }
}

#[cfg(target_os = "windows")]
fn keyboard_zoom_handler(enabled: bool) -> Option<KeyboardHandler> {
    enabled.then(GhostexGpuiWindowsZoomKeyboardHandler::new)
}

#[cfg(not(target_os = "windows"))]
fn keyboard_zoom_handler(_enabled: bool) -> Option<KeyboardHandler> {
    None
}

/*
CDXC:GPUIManageHtmlResources 2026-07-14:
Manage renders authored HTML through srcdoc, whose default base is the bundled
manage.html file. Give only the Manage CEF client a synthetic HTTPS resource
origin so normal browser URL resolution can load sibling CSS, JavaScript,
images, CSS url() values, and module imports. The provider resolves files on
CEF's blocking-file thread, canonicalizes both ends, and serves only paths
inside the configured Docs roots; ordinary Browser/sidebar/workarea clients
never receive this request handler or the project path.
*/
const MANAGE_DOCS_RESOURCE_BASE_URL: &str = PROJECT_WORKAREA_MANAGE_DOCS_RESOURCE_BASE_URL;

type ManageDocsRemoteResourceLoader = Arc<dyn Fn(&str) -> Option<Vec<u8>> + Send + Sync>;

/*
CDXC:DocsRootDirectory 2026-08-09:
The local Docs root is a configurable folder now, and resolving it reads the
project's Docs directory from the daemon. That resolution must not run on the
main thread while a CEF surface is being created, so the scope carries a
resolver that runs on the same blocking-capable worker sequence as the file
open, memoized so one document's images cost one lookup.

CDXC:DocsRootAdditive 2026-08-09: Docs serves TWO roots — the project's own and
the mounted Docs directory — so the resolver answers with every mount, each
carrying the path segment that addresses it and the relative roots a resource
may live under inside it. Which mounts exist and what they allow both come out
of the same daemon lookup, so neither can be answered before it.
*/
type ManageDocsLocalRootResolver =
    Arc<dyn Fn() -> Option<Vec<ManageDocsResourceRoot>> + Send + Sync>;

/// One mounted Docs root as the resource scope sees it: the reserved first path
/// segment that addresses it (empty for the project root, which owns bare
/// paths), the root itself, and the relative roots inside it a resource may
/// live under. An empty relative root means the whole tree.
#[derive(Clone)]
pub struct ManageDocsResourceRoot {
    pub allowed_relative_roots: Vec<String>,
    pub mount_segment: String,
    pub path: PathBuf,
}

#[derive(Clone)]
enum ManageDocsResourceSource {
    Local {
        resolve_root: ManageDocsLocalRootResolver,
        /*
        CDXC:DocsRootDirectory 2026-08-10:
        Memoize only a successful resolution. The lookup reads the project's
        Docs directory from the daemon, so it can answer `None` for a reason
        that passes — daemon not reachable yet, project row not loaded. Sealing
        that first answer would leave every image and stylesheet in the document
        broken until the surface is recreated, with nothing shown to say why.
        */
        resolved_root: Arc<Mutex<Option<Vec<ManageDocsResourceRoot>>>>,
    },
    Remote {
        loader: ManageDocsRemoteResourceLoader,
    },
}

#[derive(Clone)]
pub struct ManageDocsResourceScope {
    source: ManageDocsResourceSource,
}

/*
CDXC:GPUIAppServedResource 2026-08-19:
The synthetic origin this scope serves is not Docs-specific: it is simply the
one http(s) origin the app can hand to a CEF document that must not be a
file:// URL. The first-launch tutorial player page uses it because YouTube's
embed player answers "Error 153 - Video player configuration error" when the
embedding document has no real origin.
*/
pub fn app_served_resource_url(relative_path: &str) -> String {
    format!("{MANAGE_DOCS_RESOURCE_BASE_URL}{relative_path}")
}

impl ManageDocsResourceScope {
    pub fn new(resolve_root: ManageDocsLocalRootResolver) -> Self {
        Self {
            source: ManageDocsResourceSource::Local {
                resolve_root,
                resolved_root: Arc::new(Mutex::new(None)),
            },
        }
    }

    pub fn new_remote(loader: ManageDocsRemoteResourceLoader) -> Self {
        Self {
            source: ManageDocsResourceSource::Remote { loader },
        }
    }

    pub fn base_url(&self) -> &'static str {
        MANAGE_DOCS_RESOURCE_BASE_URL
    }

    fn request_handler(&self) -> RequestHandler {
        GhostexManageDocsRequestHandler::new(self.source.clone())
    }
}

/*
CDXC:GPUIManageHtmlResources 2026-08-07:
Serve Docs resources straight from a CEF resource handler instead of the cef
wrapper's ResourceManager. That wrapper re-locks its own manager mutex while
already holding it (ResourceManager::send_request -> ResourceManagerRequest::
send_request), so the very first Docs subresource permanently wedged the
browser-process IO thread and froze every CEF pane in the app. We need no
provider ordering or async continuation here, so the direct handler is both
correct and simpler: CEF calls `open`/`read` on a blocking-capable worker
sequence, never the IO thread, which is exactly where the file open, the
remote fetch, and the reads belong.
*/
fn manage_docs_resource_relative_path(url: &str) -> Option<String> {
    let encoded_relative_path = url.strip_prefix(MANAGE_DOCS_RESOURCE_BASE_URL)?;
    let encoded_relative_path = get_url_without_query_or_fragment(encoded_relative_path);
    let relative_path = percent_decode_str(encoded_relative_path)
        .decode_utf8()
        .ok()?;
    if relative_path.is_empty()
        || relative_path.contains(['\0', '\\'])
        || relative_path.starts_with('/')
    {
        return None;
    }
    if relative_path
        .split('/')
        .any(|component| component.is_empty() || component == "." || component == "..")
    {
        return None;
    }
    Some(relative_path.to_string())
}

/// Opens a Docs resource. Runs on a CEF worker sequence, never the IO thread.
fn open_manage_docs_resource(
    source: &ManageDocsResourceSource,
    relative_path: &str,
) -> Option<ManageDocsResourceBody> {
    match source {
        ManageDocsResourceSource::Local {
            resolve_root,
            resolved_root,
        } => {
            /*
            CDXC:DocsRootAdditive 2026-08-09:
            The requested path names its own root through the reserved mount
            segment, exactly as the Docs bridge routes it, so an image beside a
            note in the mounted Docs directory resolves there and a path can
            never be resolved against the root it did not name.
            */
            let mounts = {
                let mut resolved = resolved_root.lock().ok()?;
                if resolved.is_none() {
                    *resolved = resolve_root();
                }
                resolved.clone()?
            };
            // A named mount claims its own segment first; the project root owns
            // every path no mount claimed.
            let (mount, relative_path) = mounts
                .iter()
                .filter(|mount| !mount.mount_segment.is_empty())
                .find_map(|mount| {
                    relative_path
                        .strip_prefix(&format!("{}/", mount.mount_segment))
                        .map(|inner| (mount, inner))
                })
                .or_else(|| {
                    mounts
                        .iter()
                        .find(|mount| mount.mount_segment.is_empty())
                        .map(|mount| (mount, relative_path))
                })?;
            let root = std::fs::canonicalize(&mount.path).ok()?;
            let candidate = std::fs::canonicalize(
                relative_path
                    .split('/')
                    .fold(root.clone(), |path, component| path.join(component)),
            )
            .ok()?;
            if !candidate.is_file() || !candidate.starts_with(&root) {
                return None;
            }
            let allowed = mount.allowed_relative_roots.iter().any(|relative_root| {
                let allowed_root = root.join(relative_root);
                std::fs::canonicalize(allowed_root)
                    .ok()
                    .is_some_and(|allowed_root| {
                        allowed_root.starts_with(&root) && candidate.starts_with(allowed_root)
                    })
            });
            if !allowed {
                return None;
            }
            let file_name = candidate.to_string_lossy();
            let stream = stream_reader_create_for_file(Some(&CefString::from(file_name.as_ref())))?;
            Some(ManageDocsResourceBody::Stream(stream))
        }
        ManageDocsResourceSource::Remote { loader } => {
            let data = loader(relative_path)?;
            Some(ManageDocsResourceBody::Buffer { data, offset: 0 })
        }
    }
}

enum ManageDocsResourceBody {
    /// Local files stream from disk so a large Docs asset is never buffered whole.
    Stream(StreamReader),
    Buffer {
        data: Vec<u8>,
        offset: usize,
    },
}

impl ManageDocsResourceBody {
    fn response_length(&self) -> i64 {
        match self {
            Self::Stream(_) => -1,
            Self::Buffer { data, .. } => data.len() as i64,
        }
    }

    fn read(&mut self, data_out: *mut u8, bytes_to_read: usize) -> usize {
        match self {
            Self::Stream(stream) => stream.read(data_out, 1, bytes_to_read),
            Self::Buffer { data, offset } => {
                let available = data.len().saturating_sub(*offset);
                let count = available.min(bytes_to_read);
                if count > 0 {
                    // `data_out` is CEF's buffer, guaranteed to hold `bytes_to_read`.
                    unsafe {
                        std::ptr::copy_nonoverlapping(data.as_ptr().add(*offset), data_out, count);
                    }
                    *offset += count;
                }
                count
            }
        }
    }
}

wrap_resource_handler! {
    struct GhostexManageDocsResourceHandler {
        source: ManageDocsResourceSource,
        relative_path: String,
        body: Arc<Mutex<Option<ManageDocsResourceBody>>>,
    }

    impl ResourceHandler {
        fn open(
            &self,
            _request: Option<&mut Request>,
            handle_request: Option<&mut c_int>,
            _callback: Option<&mut Callback>,
        ) -> c_int {
            // Handled synchronously on this worker sequence; blocking here is
            // the documented contract for `open`, unlike the IO thread. The
            // file open and the remote fetch below both depend on that.
            if let Some(handle_request) = handle_request {
                *handle_request = 1;
            }
            let Some(opened) = open_manage_docs_resource(&self.source, &self.relative_path) else {
                // Outside the Docs roots or unreadable: cancel the request.
                return 0;
            };
            let Ok(mut body) = self.body.lock() else {
                return 0;
            };
            *body = Some(opened);
            1
        }

        fn response_headers(
            &self,
            response: Option<&mut Response>,
            response_length: Option<&mut i64>,
            _redirect_url: Option<&mut CefString>,
        ) {
            let Some(response) = response else {
                return;
            };
            response.set_status(200);
            response.set_status_text(Some(&CefString::from("OK")));
            response.set_mime_type(Some(&CefString::from(
                get_mime_type(&self.relative_path).as_str(),
            )));
            let mut headers = string_multimap_alloc();
            if let Some(headers) = headers.as_mut() {
                string_multimap_append(
                    Some(headers),
                    Some(&CefString::from("Access-Control-Allow-Origin")),
                    Some(&CefString::from("*")),
                );
                string_multimap_append(
                    Some(headers),
                    Some(&CefString::from("Cache-Control")),
                    Some(&CefString::from("no-store")),
                );
                response.set_header_map(Some(headers));
            }
            if let Some(response_length) = response_length {
                *response_length = self
                    .body
                    .lock()
                    .ok()
                    .and_then(|body| body.as_ref().map(ManageDocsResourceBody::response_length))
                    .unwrap_or(-1);
            }
        }

        #[allow(clippy::not_unsafe_ptr_arg_deref)]
        fn read(
            &self,
            data_out: *mut u8,
            bytes_to_read: c_int,
            bytes_read: Option<&mut c_int>,
            _callback: Option<&mut ResourceReadCallback>,
        ) -> c_int {
            if bytes_to_read < 1 {
                return 0;
            }
            let Some(bytes_read) = bytes_read else {
                return 0;
            };
            let Ok(mut body) = self.body.lock() else {
                return 0;
            };
            let Some(body) = body.as_mut() else {
                *bytes_read = 0;
                return 0;
            };

            // Fill the buffer until it is full or the source reports EOF.
            *bytes_read = 0;
            loop {
                let data_out = unsafe { data_out.add(*bytes_read as usize) };
                let read = body.read(data_out, (bytes_to_read - *bytes_read) as usize);
                *bytes_read += read as c_int;
                if read == 0 || *bytes_read >= bytes_to_read {
                    break;
                }
            }

            // Returning 0 with no bytes read signals the end of the response.
            if *bytes_read > 0 { 1 } else { 0 }
        }
    }
}

wrap_resource_request_handler! {
    struct GhostexManageDocsResourceRequestHandler {
        source: ManageDocsResourceSource,
    }

    impl ResourceRequestHandler {
        /*
        CDXC:GPUIManageHtmlResources 2026-08-08:
        CEF consults on_before_resource_load BEFORE resource_handler, and the
        generated cef-rs binding's inherited default returns
        ReturnValue::default() == RV_CANCEL. Without this explicit CONTINUE
        override, every Docs subresource request was aborted
        (net::ERR_ABORTED, canceled) before the resource handler was ever
        queried, so no image/CSS/JS in rendered HTML Docs could load.
        */
        fn on_before_resource_load(
            &self,
            _browser: Option<&mut cef::Browser>,
            _frame: Option<&mut Frame>,
            _request: Option<&mut Request>,
            _callback: Option<&mut Callback>,
        ) -> ReturnValue {
            ReturnValue::CONTINUE
        }

        fn resource_handler(
            &self,
            _browser: Option<&mut cef::Browser>,
            _frame: Option<&mut Frame>,
            request: Option<&mut Request>,
        ) -> Option<ResourceHandler> {
            let request_url = CefString::from(&request?.url()).to_string();
            let relative_path = manage_docs_resource_relative_path(&request_url)?;
            Some(GhostexManageDocsResourceHandler::new(
                self.source.clone(),
                relative_path,
                Arc::new(Mutex::new(None)),
            ))
        }
    }
}

wrap_request_handler! {
    struct GhostexGpuiSidebarRendererRequestHandler;

    impl RequestHandler {
        fn on_render_view_ready(&self, browser: Option<&mut cef::Browser>) {
            append_sidebar_renderer_lifecycle(
                "gpui.sidebar.rendererReady",
                browser,
                serde_json::json!({}),
            );
        }

        fn on_render_process_unresponsive(
            &self,
            browser: Option<&mut cef::Browser>,
            _callback: Option<&mut UnresponsiveProcessCallback>,
        ) -> c_int {
            append_sidebar_renderer_lifecycle(
                "gpui.sidebar.rendererUnresponsive",
                browser,
                serde_json::json!({}),
            );
            0
        }

        fn on_render_process_responsive(&self, browser: Option<&mut cef::Browser>) {
            append_sidebar_renderer_lifecycle(
                "gpui.sidebar.rendererResponsive",
                browser,
                serde_json::json!({}),
            );
        }

        fn on_render_process_terminated(
            &self,
            browser: Option<&mut cef::Browser>,
            status: TerminationStatus,
            error_code: c_int,
            error_string: Option<&CefString>,
        ) {
            append_sidebar_renderer_lifecycle(
                "gpui.sidebar.rendererTerminated",
                browser,
                serde_json::json!({
                    "cefCode": error_code,
                    "cefText": error_string.map(CefString::to_string),
                    "terminationKind": cef_termination_kind(status),
                    "terminationRaw": status.get_raw(),
                }),
            );
        }
    }
}

fn cef_termination_kind(status: TerminationStatus) -> &'static str {
    match status {
        TerminationStatus::ABNORMAL_TERMINATION => "abnormalTermination",
        TerminationStatus::PROCESS_WAS_KILLED => "processWasKilled",
        TerminationStatus::PROCESS_CRASHED => "processCrashed",
        TerminationStatus::PROCESS_OOM => "processOutOfMemory",
        TerminationStatus::LAUNCH_FAILED => "launchFailed",
        TerminationStatus::INTEGRITY_FAILURE => "integrityFailure",
        _ => "unknown",
    }
}

fn append_sidebar_renderer_lifecycle(
    event: &str,
    browser: Option<&mut cef::Browser>,
    mut details: serde_json::Value,
) {
    if let Some(details) = details.as_object_mut() {
        details.insert(
            "browserId".to_string(),
            browser
                .map(|browser| serde_json::Value::from(browser.identifier()))
                .unwrap_or(serde_json::Value::Null),
        );
        details.insert(
            "cefContextInitialized".to_string(),
            serde_json::Value::Bool(CEF_CONTEXT_INITIALIZED.load(Ordering::Acquire)),
        );
        details.insert(
            "runtimeShutdownStarted".to_string(),
            serde_json::Value::Bool(CEF_SHUTDOWN_IN_PROGRESS.load(Ordering::Acquire)),
        );
    }
    support_logs::append(GpuiSupportLog::SidebarRenderer, event, details);
}

wrap_request_handler! {
    struct GhostexGpuiBrowserRequestHandler {
        popup_open_handler: BrowserPopupOpenHandler,
    }

    impl RequestHandler {
        fn on_open_urlfrom_tab(
            &self,
            _browser: Option<&mut cef::Browser>,
            _frame: Option<&mut Frame>,
            target_url: Option<&CefString>,
            target_disposition: WindowOpenDisposition,
            _user_gesture: c_int,
        ) -> c_int {
            /*
            CDXC:GPUIBrowserLinkNewTab 2026-08-18:
            Chromium reports middle-click and Cmd/Ctrl-click link opens here,
            not through OnBeforePopup, so Browser panes need this callback to
            keep those gestures inside the GPUI Browser workspace. Forward only
            the requested target URL to the same shell tab model the popup path
            uses and return handled so Chromium creates no separate browser.
            Dispositions that are not a new browser (same-tab navigation,
            save-to-disk, ignored actions) stay on CEF's default path.

            Empty targets mirror the popup policy
            (CDXC:GPUIBrowserPopups 2026-06-23-11:43): handled here with no
            shell dispatch, because there is no transferable URL and no
            fallback transfer path.
            */
            let Some(placement) = browser_popup_placement_for_disposition(target_disposition) else {
                return 0;
            };
            if let Some(requested_url) = browser_popup_target_url_for_shell(target_url) {
                (self.popup_open_handler)(requested_url, placement);
            }
            1
        }
    }
}

wrap_request_handler! {
    struct GhostexManageDocsRequestHandler {
        source: ManageDocsResourceSource,
    }

    impl RequestHandler {
        fn resource_request_handler(
            &self,
            _browser: Option<&mut cef::Browser>,
            _frame: Option<&mut Frame>,
            request: Option<&mut Request>,
            _is_navigation: c_int,
            _is_download: c_int,
            _request_initiator: Option<&CefString>,
            _disable_default_handling: Option<&mut c_int>,
        ) -> Option<ResourceRequestHandler> {
            let request_url = request
                .map(|request| CefString::from(&request.url()).to_string())
                .unwrap_or_default();
            request_url.starts_with(MANAGE_DOCS_RESOURCE_BASE_URL).then(|| {
                GhostexManageDocsResourceRequestHandler::new(self.source.clone())
            })
        }
    }
}

wrap_client! {
    struct GhostexGpuiCefClient {
        life_span_handler: Option<LifeSpanHandler>,
        context_menu_handler: Option<ContextMenuHandler>,
        display_handler: Option<DisplayHandler>,
        find_handler: Option<FindHandler>,
        load_handler: Option<LoadHandler>,
        sidebar_bridge_event_handler: Option<SidebarBridgeEventHandler>,
        project_workarea_bridge_event_handler: Option<ProjectWorkareaBridgeEventHandler>,
        app_modal_host_bridge_event_handler: Option<AppModalHostBridgeEventHandler>,
        request_handler: Option<RequestHandler>,
        permission_handler: Option<PermissionHandler>,
        focus_handler: Option<FocusHandler>,
        keyboard_handler: Option<KeyboardHandler>,
    }

    impl Client {
        fn focus_handler(&self) -> Option<FocusHandler> {
            self.focus_handler.clone()
        }

        fn keyboard_handler(&self) -> Option<KeyboardHandler> {
            self.keyboard_handler.clone()
        }

        fn life_span_handler(&self) -> Option<LifeSpanHandler> {
            self.life_span_handler.clone()
        }

        fn context_menu_handler(&self) -> Option<ContextMenuHandler> {
            self.context_menu_handler.clone()
        }

        fn display_handler(&self) -> Option<DisplayHandler> {
            self.display_handler.clone()
        }

        fn find_handler(&self) -> Option<FindHandler> {
            self.find_handler.clone()
        }

        fn load_handler(&self) -> Option<LoadHandler> {
            self.load_handler.clone()
        }

        fn permission_handler(&self) -> Option<PermissionHandler> {
            self.permission_handler.clone()
        }

        fn request_handler(&self) -> Option<RequestHandler> {
            self.request_handler.clone()
        }

        fn on_process_message_received(
            &self,
            browser: Option<&mut cef::Browser>,
            frame: Option<&mut Frame>,
            source_process: ProcessId,
            message: Option<&mut ProcessMessage>,
        ) -> c_int {
            /*
            CDXC:GPUIProjectSidebarBridge 2026-06-23-18:29:
            The GPUI sidebar bridge may carry only the allowlisted typed sidebar events from `window.ghostexGpui`, each as one bounded string payload. Ordinary Browser CEF surfaces construct clients without this handler, and CEF only classifies the private event kind; strict JSON parsing and stale/private-shape rejection stay in the GPUI app stores with no logging or persistence at this boundary.

            CDXC:GPUISidebarProjectPathActions 2026-06-24-14:18:
            Sidebar-native project path actions use the same fixed-function CEF bridge as project-context/readiness events. CEF forwards only a bounded string from the bundled sidebar main frame; Rust app code must parse the small action/project-id JSON and resolve project paths through gxserver, not from renderer-provided absolute path data.

            CDXC:GPUISidebarGit 2026-06-24-15:43:
            Existing-PR browser open and changed-file IDE open are still sidebar-only native side effects on this fixed bridge. CEF does not trust or inspect URLs or paths; app-side Rust must re-query gxserver and treat any file path as a relative candidate only.

            CDXC:GPUICommandPane 2026-06-24-23:17:
            Sidebar command actions use their own fixed sidebar bridge function so the shared SidebarApp and command palette can ask GPUI to run the gxserver-projected action through Rust-owned Browser or command-pane paths. CEF still forwards only one bounded string from the sidebar main frame and does not log, persist, inspect, or execute command text.

            CDXC:GPUIAppShots 2026-06-25-23:28:
            App Shot prompt insertion uses its own fixed sidebar bridge function. CEF forwards only one bounded JSON string from the bundled sidebar; app-side Rust must parse the gxserver presentation session id and prompt, then verify the exact mounted Agents surface before writing terminal bytes.

            CDXC:GPUIAppShots 2026-06-26-04:27:
            The same bridge may carry a machine-scoped remote presentation session id for App Shots, but CEF remains a string forwarder only; Rust must decline unless the exact remote attach Agents terminal is already mounted.

            CDXC:GPUIStatusPetOverlay 2026-06-26-04:38:
            GPUI status indicators and pet overlay state use their own fixed sidebar bridge functions. CEF forwards only bounded first-party strings; app-side Rust must strictly parse counts/settings/candidate ids and never treat renderer paths, URLs, command text, terminal output, tokens, or generic message names as presentation authority.
            */
            if source_process != ProcessId::RENDERER {
                return 0;
            }

            let Some(message) = message else {
                return 0;
            };
            let message_name = CefString::from(&message.name()).to_string();
            let sidebar_event_kind = sidebar_bridge_event_kind_for_process_message(&message_name);
            let is_sidebar_editable_focus_message =
                message_name == SIDEBAR_EDITABLE_FOCUS_PROCESS_MESSAGE_NAME;
            let project_workarea_event_kind =
                project_workarea_bridge_event_kind_for_process_message(&message_name);
            let is_app_modal_host_message =
                message_name == APP_MODAL_HOST_BRIDGE_PROCESS_MESSAGE_NAME;
            let is_native_host_message = message_name == NATIVE_HOST_BRIDGE_PROCESS_MESSAGE_NAME;
            if sidebar_event_kind.is_none()
                && !is_sidebar_editable_focus_message
                && project_workarea_event_kind.is_none()
                && !is_app_modal_host_message
                && !is_native_host_message
            {
                return 0;
            }
            if frame.map(|frame| frame.is_main() == 0).unwrap_or(true) {
                return 1;
            }

            let Some(arguments) = message.argument_list() else {
                return 1;
            };
            if arguments.size() != 1 || arguments.get_type(0) != ValueType::STRING {
                return 1;
            }

            let payload = CefString::from(&arguments.string(0)).to_string();
            if is_sidebar_editable_focus_message {
                /*
                CDXC:GPUISidebarPassiveMouseFocus 2026-07-22:
                The shared sidebar surface is mouse-focus passive: clicking its
                background never moves AppKit first responder away from the
                active terminal. The only way the sidebar may take keyboard
                focus is this fixed bridge message, sent when its page focuses
                a real editable element (search, rename). It is consumed here
                as a native focus transfer for the sending browser; it carries
                no app data and never reaches the app event handler.
                */
                handle_sidebar_editable_focus(browser, &payload);
                return 1;
            }
            if let Some(event_kind) = sidebar_event_kind {
                let Some(handler) = self.sidebar_bridge_event_handler.clone() else {
                    return 0;
                };
                if payload.chars().count() > SIDEBAR_BRIDGE_PAYLOAD_MAX_CHARS {
                    return 1;
                }

                handler(event_kind.with_payload(payload));
                return 1;
            }

            if let Some(event_kind) = project_workarea_event_kind {
                let Some(handler) = self.project_workarea_bridge_event_handler.clone() else {
                    return 0;
                };
                /*
                CDXC:GPUIProjectWorkareaCefBridge 2026-06-24-11:03:
                Project-workarea CEF process messages are fixed-function and main-frame-only like the sidebar bridge, but their payload budget is larger because Manage save requests carry bounded file contents. The CEF boundary forwards only in-memory strings to the app handler and does not log, persist, inspect URL/title state, expose generic IPC, or create a WKWebView/WebKit path.
                */
                if payload.chars().count() > PROJECT_WORKAREA_BRIDGE_PAYLOAD_MAX_CHARS {
                    return 1;
                }

                handler(event_kind.with_payload(payload));
                return 1;
            }

            if is_app_modal_host_message {
                let Some(handler) = self.app_modal_host_bridge_event_handler.clone() else {
                    return 0;
                };
                /*
                CDXC:GPUITitlebarAppModalHost 2026-06-24-10:42:
                The GPUI app-modal host and titlebar Tips panel reuse the macOS React bridge shape, but CEF forwards each message as a single bounded JSON string from first-party bundled pages only. Keep this main-frame-only and handler-scoped so Browser tabs, workarea pages, logs, persistence, raw URLs, page titles, and generic IPC never receive app-modal payloads.
                */
                if payload.chars().count() > APP_MODAL_HOST_BRIDGE_PAYLOAD_MAX_CHARS {
                    return 1;
                }

                handler(AppModalHostBridgeEvent::Message(payload));
                return 1;
            }

            if is_native_host_message {
                let Some(handler) = self.app_modal_host_bridge_event_handler.clone() else {
                    return 0;
                };
                /*
                CDXC:GPUITitlebarNativeHost 2026-07-08:
                The bundled titlebar-host Resources document uses macOS's `ghostexNativeHost` bridge for process sampling and titlebar actions. CEF forwards only a bounded main-frame JSON string from first-party modal/sidebar/titlebar surfaces and tags it as native-host; app-side Rust owns the fixed process allowlist and action validation.
                */
                if payload.chars().count() > NATIVE_HOST_BRIDGE_PAYLOAD_MAX_CHARS {
                    return 1;
                }

                handler(AppModalHostBridgeEvent::NativeHostMessage(payload));
                return 1;
            }

            0
        }
    }
}

wrap_load_handler! {
    struct GhostexGpuiBrowserPageLoadHandler {
        page_metadata_handler: BrowserPageMetadataHandler,
    }

    impl LoadHandler {
        fn on_loading_state_change(
            &self,
            _browser: Option<&mut cef::Browser>,
            is_loading: c_int,
            can_go_back: c_int,
            can_go_forward: c_int,
        ) {
            (self.page_metadata_handler)(BrowserPageMetadataEvent::LoadingStateChanged {
                is_loading: is_loading != 0,
                can_go_back: can_go_back != 0,
                can_go_forward: can_go_forward != 0,
            });
        }
    }
}

wrap_load_handler! {
    struct GhostexGpuiPageLoadEndHandler {
        load_end_handler: PageLoadEndHandler,
    }

    impl LoadHandler {
        fn on_load_end(
            &self,
            _browser: Option<&mut cef::Browser>,
            frame: Option<&mut Frame>,
            _http_status_code: c_int,
        ) {
            let Some(frame) = frame else {
                return;
            };
            if frame.is_main() == 0 {
                return;
            }

            /*
            CDXC:GPUITutorialVideoFullscreen 2026-08-18:
            Report only the main-frame load-end edge to the app; sub-frames
            (ads, player iframes) must not retrigger the host action.
            */
            (self.load_end_handler)();
        }
    }
}

wrap_load_handler! {
    struct GhostexGpuiSidebarProjectContextLoadHandler {
        runtime_settings: SidebarRuntimeSettingsSnapshot,
        gxserver_bootstrap: Option<SidebarGxserverBootstrap>,
    }

    impl LoadHandler {
        fn on_load_end(
            &self,
            _browser: Option<&mut cef::Browser>,
            frame: Option<&mut Frame>,
            _http_status_code: c_int,
        ) {
            let Some(frame) = frame else {
                return;
            };
            if frame.is_main() == 0 {
                return;
            }

            /*
            CDXC:GPUIProjectSidebarBridge 2026-06-24-11:17:
            Install renderer-side `window.ghostexGpui` only for sidebar CEF clients with fixed allowlisted post functions, strict debug/beta booleans, saved shared Settings, and the real gxserver bootstrap when the local token helper can construct it. The private install message may carry the loopback base URL, bearer token, protocol version, stable client id, and only explicit gxserver ids from app state; ordinary Browser, workarea, and modal CEF clients never attach this load handler or receive the bootstrap.

            CDXC:GPUISettingsSidebarHandoff 2026-06-24-11:22:
            The same sidebar-only runtime message must carry the saved shared Settings object so the mounted React SidebarApp can normalize real user preferences instead of booting from hardcoded GPUI defaults plus debug/beta flags. Keep this as a bounded first-party CEF payload scoped to the sidebar renderer; Browser, workarea, and modal-host clients must not receive it.
            */
            send_sidebar_install_process_message(
                frame,
                self.runtime_settings.clone(),
                self.gxserver_bootstrap.clone(),
            );
        }
    }
}

wrap_load_handler! {
    struct GhostexGpuiSessionChatGxserverBootstrapLoadHandler {
        gxserver_bootstrap: Option<SidebarGxserverBootstrap>,
    }

    impl LoadHandler {
        fn on_load_end(
            &self,
            _browser: Option<&mut cef::Browser>,
            frame: Option<&mut Frame>,
            _http_status_code: c_int,
        ) {
            let Some(frame) = frame else {
                return;
            };
            if frame.is_main() == 0 {
                return;
            }

            /*
            CDXC:GPUISessionChatSurface 2026-07-31:
            Session Chat CEF clients receive only the gxserver bootstrap so the
            bundled chat.html page can call the session-chat endpoints and open
            /api/events directly, matching the sidebar's loopback token scope.
            No sidebar post functions, runtime settings, or workarea bridges are
            installed for this surface, and ordinary Browser/workarea/modal
            clients never attach this load handler. The page polls for the
            installed object, so load-end delivery cannot strand it.
            */
            send_session_chat_gxserver_bootstrap_process_message(
                frame,
                self.gxserver_bootstrap.clone(),
            );
        }
    }
}

wrap_load_handler! {
    struct GhostexGpuiProjectWorkareaBridgeLoadHandler {
        manage_docs_resource_base_url: Option<String>,
    }

    impl LoadHandler {
        fn on_load_end(
            &self,
            _browser: Option<&mut cef::Browser>,
            frame: Option<&mut Frame>,
            _http_status_code: c_int,
        ) {
            let Some(frame) = frame else {
                return;
            };
            if frame.is_main() == 0 {
                return;
            }

            /*
            CDXC:GPUIProjectWorkareaCefBridge 2026-06-24-11:03:
            Project workarea CEF clients install only the Kanban/Manage fixed bridge functions after the first-party CEF entry loads. Sidebar and ordinary Browser clients do not receive this handler, keeping project file/board messages out of generic Browser tabs and avoiding WKWebView/WebKit compatibility at the native runtime layer.
            */
            let mut message =
                match cef::process_message_create(Some(&CefString::from(
                    PROJECT_WORKAREA_BRIDGE_INSTALL_MESSAGE_NAME,
                ))) {
                    Some(message) => message,
                    None => return,
                };
            if let Some(arguments) = message.argument_list() {
                if let Some(base_url) = self.manage_docs_resource_base_url.as_deref() {
                    arguments.set_size(1);
                    arguments.set_string(0, Some(&CefString::from(base_url)));
                }
            }
            frame.send_process_message(ProcessId::RENDERER, Some(&mut message));
        }
    }
}

wrap_render_process_handler! {
    struct GhostexGpuiRenderProcessHandler;

    impl RenderProcessHandler {
        fn on_context_created(
            &self,
            _browser: Option<&mut cef::Browser>,
            frame: Option<&mut Frame>,
            context: Option<&mut cef::V8Context>,
        ) {
            let Some(frame) = frame else {
                return;
            };
            if frame.is_main() == 0 {
                return;
            }
            let frame_url = CefString::from(&frame.url()).to_string();
            let surface = app_modal_host_bridge_surface_for_frame_url(&frame_url);
            let Some(surface) = surface else {
                return;
            };
            let Some(context) = context else {
                return;
            };
            /*
            CDXC:GPUITitlebarAppModalHost 2026-06-24-11:09:
            Install the CEF-compatible `window.webkit.messageHandlers.ghostexAppModalHost` shim at V8 context creation for only bundled modal-host.html, titlebar-host.html, and sidebar index.html entries. Install `ghostexNativeHost` for titlebar-host and the first-party sidebar so either surface can invoke Rust's fixed, validated native actions, including gxserver lifecycle controls. The shared React modal host posts `ready` during mount, the titlebar panels post dropdown/process messages during hydration, and the shared sidebar can emit Settings/Hotkeys/Command Palette opens after hydration, so waiting for load-end would race real presentation. Only modal-host.html receives the native-window identity fields; Browser tabs, project workareas, arbitrary pages, raw URLs, titles, logs, persistence, and generic IPC do not receive these bridges.

            CDXC:GPUILoggingRemoval 2026-06-28-17:06:
            App-modal CEF setup keeps only the functional host message bridge. Do not emit lifecycle diagnostic IPC or renderer logging events from bridge installation while GPUI logging is intentionally removed.
            */
            install_app_modal_host_v8_bridge(Some(&mut *context), surface);
        }

        fn on_process_message_received(
            &self,
            _browser: Option<&mut cef::Browser>,
            frame: Option<&mut Frame>,
            source_process: ProcessId,
            message: Option<&mut ProcessMessage>,
        ) -> c_int {
            if source_process != ProcessId::BROWSER {
                return 0;
            }
            let Some(message) = message else {
                return 0;
            };
            let message_name = CefString::from(&message.name()).to_string();
            let is_install_message = message_name == SIDEBAR_PROJECT_CONTEXT_INSTALL_MESSAGE_NAME;
            let is_runtime_settings_update =
                message_name == SIDEBAR_RUNTIME_SETTINGS_UPDATE_MESSAGE_NAME;
            let is_gxserver_bootstrap_update =
                message_name == SIDEBAR_GXSERVER_BOOTSTRAP_UPDATE_MESSAGE_NAME;
            let is_session_chat_gxserver_bootstrap_message =
                message_name == SESSION_CHAT_GXSERVER_BOOTSTRAP_MESSAGE_NAME;
            let is_project_workarea_install_message =
                message_name == PROJECT_WORKAREA_BRIDGE_INSTALL_MESSAGE_NAME;
            if !is_install_message
                && !is_runtime_settings_update
                && !is_gxserver_bootstrap_update
                && !is_session_chat_gxserver_bootstrap_message
                && !is_project_workarea_install_message
            {
                return 0;
            }
            let Some(frame) = frame else {
                return 1;
            };
            if frame.is_main() == 0 {
                return 1;
            }
            let Some(mut context) = frame.v8_context() else {
                return 1;
            };
            if context.enter() == 0 {
                return 1;
            }
            if is_project_workarea_install_message {
                let manage_docs_resource_base_url = message
                    .argument_list()
                    .filter(|arguments| {
                        arguments.size() == 1 && arguments.get_type(0) == ValueType::STRING
                    })
                    .map(|arguments| CefString::from(&arguments.string(0)).to_string())
                    .filter(|value| value == MANAGE_DOCS_RESOURCE_BASE_URL);
                install_project_workarea_v8_bridge(
                    Some(&mut context),
                    manage_docs_resource_base_url.as_deref(),
                );
            } else if is_install_message {
                let runtime_settings = sidebar_runtime_settings_from_install_message(message);
                let gxserver_bootstrap = sidebar_gxserver_bootstrap_from_process_message(
                    message,
                    SIDEBAR_RUNTIME_SETTINGS_ARGUMENT_COUNT,
                );
                install_sidebar_project_context_v8_bridge(
                    Some(&mut context),
                    runtime_settings,
                    gxserver_bootstrap,
                );
            } else if is_runtime_settings_update {
                let runtime_settings = sidebar_runtime_settings_from_install_message(message);
                update_sidebar_runtime_settings_v8_bridge(Some(&mut context), runtime_settings);
            } else if is_session_chat_gxserver_bootstrap_message {
                /*
                CDXC:GPUISessionChatSurface 2026-07-31:
                Session Chat bootstrap install creates the ghostexGpui
                namespace when missing and sets only the gxserverBootstrap
                object plus the fixed changed callback; it must not install
                sidebar post functions or relax the sidebar update path's
                installed-bridge integrity gate.
                */
                let gxserver_bootstrap = sidebar_gxserver_bootstrap_from_process_message(message, 0);
                install_session_chat_gxserver_bootstrap_v8_bridge(
                    Some(&mut context),
                    gxserver_bootstrap,
                );
            } else {
                let gxserver_bootstrap = sidebar_gxserver_bootstrap_from_process_message(message, 0);
                update_sidebar_gxserver_bootstrap_v8_bridge(
                    Some(&mut context),
                    gxserver_bootstrap,
                );
            }
            context.exit();
            1
        }
    }
}

wrap_v8_handler! {
    struct GhostexGpuiProjectWorkareaBridgeV8Handler;

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
    struct GhostexGpuiAppModalHostBridgeV8Handler;

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
    struct GhostexGpuiNativeHostBridgeV8Handler;

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
    struct GhostexGpuiSidebarBridgeV8Handler;

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

fn install_sidebar_project_context_v8_bridge(
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

fn update_sidebar_runtime_settings_v8_bridge(
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

fn update_sidebar_gxserver_bootstrap_v8_bridge(
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

fn install_session_chat_gxserver_bootstrap_v8_bridge(
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

fn install_project_workarea_v8_bridge(
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

fn install_app_modal_host_v8_bridge(
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

fn v8_object_property_or_new(parent: &V8Value, key: &str) -> Option<V8Value> {
    let key = CefString::from(key);
    parent
        .value_bykey(Some(&key))
        .filter(|value| value.is_object() != 0)
        .or_else(|| cef::v8_value_create_object(None, None))
}

fn set_v8_string_property(parent: &V8Value, key: &str, value: &str) -> bool {
    let key = CefString::from(key);
    let value = CefString::from(value);
    let Some(mut value) = cef::v8_value_create_string(Some(&value)) else {
        return false;
    };
    parent.set_value_bykey(Some(&key), Some(&mut value), V8Propertyattribute::default()) != 0
}

fn app_modal_host_payload_from_v8_value(value: &V8Value) -> Option<String> {
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

fn send_sidebar_install_process_message(
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

fn send_sidebar_runtime_settings_process_message(
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

fn send_sidebar_gxserver_bootstrap_process_message(
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

fn send_session_chat_gxserver_bootstrap_process_message(
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

fn attach_sidebar_runtime_settings_to_process_message(
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

fn attach_sidebar_gxserver_bootstrap_to_process_message(
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

fn sidebar_runtime_settings_from_install_message(
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

fn sidebar_gxserver_bootstrap_from_process_message(
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

fn non_empty_cef_argument_string(arguments: &cef::ListValue, index: usize) -> Option<String> {
    let value = CefString::from(&arguments.string(index)).to_string();
    (!value.trim().is_empty()).then_some(value)
}

fn install_sidebar_runtime_settings_v8_object(
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

fn sidebar_saved_settings_json_from_arguments(arguments: &cef::ListValue) -> String {
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

fn bounded_sidebar_saved_settings_json(value: &str) -> &str {
    if value.chars().count() > SIDEBAR_RUNTIME_SETTINGS_SAVED_SETTINGS_JSON_MAX_CHARS {
        return "";
    }
    value
}

fn parse_sidebar_json_v8_object(context: &mut cef::V8Context, json_text: &str) -> Option<V8Value> {
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

fn install_sidebar_gxserver_bootstrap_v8_object(
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

fn notify_sidebar_runtime_settings_changed(
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

fn notify_sidebar_gxserver_bootstrap_changed(
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

fn set_v8_bool_property(object: &mut V8Value, key: &str, value: bool) {
    let key = CefString::from(key);
    let mut value = cef::v8_value_create_bool(bool_to_cef_int(value));
    object.set_value_bykey(Some(&key), value.as_mut(), V8Propertyattribute::default());
}

fn set_v8_int_property(object: &mut V8Value, key: &str, value: i32) {
    let key = CefString::from(key);
    let mut value = cef::v8_value_create_int(value);
    object.set_value_bykey(Some(&key), value.as_mut(), V8Propertyattribute::default());
}

fn set_v8_string_array_property(object: &mut V8Value, key: &str, values: &[String]) {
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

fn bool_to_cef_int(value: bool) -> c_int {
    if value { 1 } else { 0 }
}

fn send_sidebar_bridge_process_message(process_message_name: &str, payload: &str) -> bool {
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

fn send_project_workarea_bridge_process_message(process_message_name: &str, payload: &str) -> bool {
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

fn send_app_modal_host_bridge_process_message(payload: &str) -> bool {
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

fn send_native_host_bridge_process_message(payload: &str) -> bool {
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

fn set_v8_bool_return(retval: Option<&mut Option<V8Value>>, value: bool) {
    if let Some(retval) = retval {
        *retval = cef::v8_value_create_bool(if value { 1 } else { 0 });
    }
}

fn show_browser_dev_tools(
    browser: Option<&mut cef::Browser>,
    inspect_element_at: Option<&cef::Point>,
) -> bool {
    let Some(browser) = browser else {
        return false;
    };
    let Some(host) = browser.host() else {
        return false;
    };
    let window_info = cef::WindowInfo {
        window_name: cef::CefString::from("Chromium DevTools"),
        ..Default::default()
    };
    let browser_settings = cef::BrowserSettings::default();
    let mut devtools_client = Some(GhostexGpuiCefClient::new(
        Some(GhostexGpuiLifeSpanHandler::new(None, None, true)),
        None,
        None,
        None,
        None,
        None,
        None,
        None,
        None,
        None,
        Some(GhostexGpuiCefFocusHandler::new()),
        None,
    ));
    host.show_dev_tools(
        Some(&window_info),
        devtools_client.as_mut(),
        Some(&browser_settings),
        inspect_element_at,
    );
    true
}

wrap_task! {
    struct GhostexRegisterDevToolsNativeView {
        browser: cef::Browser,
    }

    impl Task {
        fn execute(&self) {
            let Some(host) = self.browser.host() else {
                return;
            };
            let native_view = platform::native_view_ptr(host.window_handle());
            platform::prepare_native_view_for_focus(native_view);
            register_native_view_browser(native_view, &self.browser, false, false);
            /*
            CDXC:GPUICefDevToolsFocus 2026-07-15:
            OnAfterCreated precedes native DevTools window attachment on macOS,
            so its host handle can still be null. A CEF UI task runs after that
            creation callback, at which point the final native root can be
            registered before the explicit OS/Chromium focus grant. This keeps
            Copy/Paste on DevTools' real responder chain without broad routing.
            */
            #[cfg(target_os = "macos")]
            platform::activate_native_view_window(native_view);
            platform::focus_native_view(native_view);
            host.set_focus(1);
            crate::support_logs::append(
                crate::support_logs::GpuiSupportLog::TerminalFocus,
                "gpui.cef.nativeViewBrowserRegistered",
                serde_json::json!({
                    "browserId": self.browser.identifier(),
                    "isPopup": self.browser.is_popup() != 0,
                    "nativeViewWasNull": native_view.is_null(),
                    "explicitFocusGranted": !native_view.is_null(),
                }),
            );
        }
    }
}

wrap_context_menu_handler! {
    struct GhostexGpuiContextMenuHandler {
        popup_open_handler: Option<BrowserPopupOpenHandler>,
    }

    impl ContextMenuHandler {
        fn on_before_context_menu(
            &self,
            _browser: Option<&mut cef::Browser>,
            _frame: Option<&mut Frame>,
            _params: Option<&mut ContextMenuParams>,
            model: Option<&mut MenuModel>,
        ) {
            let Some(model) = model else {
                return;
            };
            /*
            CDXC:GPUICefContextMenuParity 2026-07-10:
            Match the production macOS CEF browser menu by preserving CEF's
            normal page/edit/link commands and appending one real Inspect
            Element command. This remains Chromium-owned menu UI and does not
            add GPUI overlays, hit-test routing, or page-content logging.
            */
            if model.count() > 0 {
                model.add_separator();
            }
            model.add_item(
                CEF_CONTEXT_MENU_INSPECT_ELEMENT_COMMAND_ID,
                Some(&CefString::from("Inspect Element")),
            );
        }

        fn on_context_menu_command(
            &self,
            browser: Option<&mut cef::Browser>,
            _frame: Option<&mut Frame>,
            params: Option<&mut ContextMenuParams>,
            command_id: c_int,
            _event_flags: EventFlags,
        ) -> c_int {
            if command_id == CEF_CONTEXT_MENU_INSPECT_ELEMENT_COMMAND_ID {
                let inspect_point = params.as_deref().map(|params| cef::Point {
                    x: params.xcoord(),
                    y: params.ycoord(),
                });
                return show_browser_dev_tools(browser, inspect_point.as_ref()) as c_int;
            }

            if !matches!(
                command_id,
                CEF_CONTEXT_MENU_OPEN_LINK_NEW_TAB_COMMAND_ID
                    | CEF_CONTEXT_MENU_OPEN_LINK_NEW_WINDOW_COMMAND_ID
            ) {
                return 0;
            }
            let (Some(popup_open_handler), Some(params)) =
                (self.popup_open_handler.as_ref(), params)
            else {
                return 0;
            };
            let unfiltered = params.unfiltered_link_url();
            let mut requested_url = CefString::from(&unfiltered).to_string();
            if requested_url.trim().is_empty() {
                let filtered = params.link_url();
                requested_url = CefString::from(&filtered).to_string();
            }
            let requested_url = requested_url.trim();
            if requested_url.is_empty() {
                return 0;
            }
            popup_open_handler(requested_url.to_string(), BrowserPopupPlacement::Selected);
            1
        }
    }
}

wrap_find_handler! {
    struct GhostexGpuiFindHandler {
        page_metadata_handler: BrowserPageMetadataHandler,
    }

    impl FindHandler {
        fn on_find_result(
            &self,
            _browser: Option<&mut cef::Browser>,
            _identifier: c_int,
            match_count: c_int,
            _selection_rect: Option<&cef::Rect>,
            active_match_ordinal: c_int,
            final_update: c_int,
        ) {
            (self.page_metadata_handler)(BrowserPageMetadataEvent::FindResult {
                match_count,
                active_match_ordinal,
                final_update: final_update != 0,
            });
        }
    }
}

wrap_life_span_handler! {
    struct GhostexGpuiLifeSpanHandler {
        popup_open_handler: Option<BrowserPopupOpenHandler>,
        page_metadata_handler: Option<BrowserPageMetadataHandler>,
        register_created_native_view: bool,
    }

    impl LifeSpanHandler {
        fn on_after_created(&self, browser: Option<&mut cef::Browser>) {
            if !self.register_created_native_view {
                return;
            }
            let Some(browser) = browser else {
                return;
            };
            let mut task = GhostexRegisterDevToolsNativeView::new(browser.clone());
            post_task(ThreadId::UI, Some(&mut task));
        }

        fn do_close(&self, _browser: Option<&mut cef::Browser>) -> c_int {
            /*
            CDXC:GPUIResourcesTitlebar 2026-07-09:
            All GPUI CEF browsers are child NSViews inside app-owned GPUI
            windows. CEF's default DoClose flow (returning 0) sends a native
            close to the browser's top-level host window, so dropping any
            short-lived browser (e.g. the fresh-per-open titlebar Resources
            panel) closed the MAIN window and the quit-on-last-window hook
            then terminated the whole app. Return handled: browser teardown
            is fully owned by `CefBrowser::drop`, and the host GPUI window
            must never receive a close from CEF.

            CDXC:GPUIBrowserAgentClose 2026-08-21:
            DevTools Target.closeTarget and /json/close enter through this
            same CEF close request. Browser panes must hand that request back
            to the GPUI tab model before returning handled; otherwise CEF
            accepts the request but the app-owned pane remains. App-initiated
            closes may report this during teardown too, and the model close
            path deliberately treats that as a no-op.
            */
            if let Some(handler) = self.page_metadata_handler.as_ref() {
                handler(BrowserPageMetadataEvent::CloseRequested);
            }
            1
        }

        fn on_before_close(&self, browser: Option<&mut cef::Browser>) {
            /*
            CDXC:GPUICefTeardownRegistry 2026-07-11:
            The main-thread native-view registries (CEF_BROWSERS_BY_NATIVE_VIEW,
            HIDDEN_CEF_NATIVE_VIEWS, ACTIVE_CEF_NATIVE_VIEW) were cleaned up
            only by `CefBrowser::drop`, so a browser torn down by CEF itself
            (renderer crash, Chromium-destroyed window) left dangling entries.
            ACTIVE_CEF_NATIVE_VIEW is set on every mouseDown and later
            dereferenced as an NSView pointer by
            select_all_for_active_native_view, so a stale entry is a
            use-after-free. on_before_close is CEF's last callback before the
            browser window is destroyed and runs on the CEF UI thread, which
            is the main thread under the external message pump — the same
            thread that owns these thread_local registries.
            unregister_native_view_browser is idempotent, so the Drop path
            may run it again for app-initiated closes.
            */
            let Some(host) = browser.and_then(|browser| browser.host()) else {
                return;
            };
            unregister_native_view_browser(platform::native_view_ptr(host.window_handle()));
        }

        fn on_before_popup(
            &self,
            _browser: Option<&mut cef::Browser>,
            _frame: Option<&mut Frame>,
            _popup_id: c_int,
            target_url: Option<&CefString>,
            _target_frame_name: Option<&CefString>,
            _target_disposition: WindowOpenDisposition,
            _user_gesture: c_int,
            _popup_features: Option<&PopupFeatures>,
            _window_info: Option<&mut WindowInfo>,
            _client: Option<&mut Option<Client>>,
            _settings: Option<&mut BrowserSettings>,
            _extra_info: Option<&mut Option<DictionaryValue>>,
            no_javascript_access: Option<&mut c_int>,
        ) -> c_int {
            /*
            CDXC:GPUIBrowserPopups 2026-06-22-07:14:
            Browser-mode target=_blank and window.open requests must stay inside the GPUI Browser workspace. Intercept CEF popup creation through cef-rs LifeSpanHandler, forward only the requested target URL to the shell tab model, and return handled so Chromium does not create a separate native CEF window.

            CDXC:GPUIBrowserPopups 2026-06-23-11:43:
            Match native macOS CEF popup policy: empty target URLs are handled here without dispatching a shell popup callback because there is no transferable URL/content and no fallback transfer path. Non-empty targets remain shell-owned Browser tab requests.
            */
            if let Some(no_javascript_access) = no_javascript_access {
                *no_javascript_access = 1;
            }

            if let (Some(popup_open_handler), Some(requested_url)) = (
                self.popup_open_handler.as_ref(),
                browser_popup_target_url_for_shell(target_url),
            ) {
                (popup_open_handler)(requested_url, BrowserPopupPlacement::Selected);
            }
            1
        }
    }
}

wrap_display_handler! {
    struct GhostexGpuiDisplayHandler {
        page_metadata_handler: BrowserPageMetadataHandler,
        suppress_initial_about_blank: Cell<bool>,
    }

    impl DisplayHandler {
        fn on_address_change(
            &self,
            _browser: Option<&mut cef::Browser>,
            frame: Option<&mut Frame>,
            url: Option<&CefString>,
        ) {
            /*
            CDXC:GPUIBrowserMetadata 2026-06-22-07:23:
            Browser-tab URL state must be driven by CEF's DisplayHandler rather than synthetic shell guesses. Forward only main-frame address changes to the GPUI tab model, where raw runtime URLs can update the active address field while persistence remains guarded by the existing sanitizer.
            */
            if let Some(frame) = frame
                && frame.is_main() == 0
            {
                return;
            }

            let url = url.map(CefString::to_string).unwrap_or_default();
            if self.suppress_initial_about_blank.get() {
                if url.eq_ignore_ascii_case("about:blank") {
                    return;
                }
                self.suppress_initial_about_blank.set(false);
            }
            (self.page_metadata_handler)(BrowserPageMetadataEvent::AddressChanged(url));
        }

        fn on_title_change(&self, _browser: Option<&mut cef::Browser>, title: Option<&CefString>) {
            /*
            CDXC:GPUIBrowserMetadata 2026-06-22-07:23:
            Page titles may contain user-owned content, so CEF title callbacks may update only runtime tab-strip presentation. The GPUI shell-state writer must continue deriving restored titles from sanitized URLs instead of storing raw page titles.
            */
            let title = title.map(CefString::to_string).unwrap_or_default();
            (self.page_metadata_handler)(BrowserPageMetadataEvent::TitleChanged(title));
        }

        fn on_favicon_urlchange(
            &self,
            _browser: Option<&mut cef::Browser>,
            icon_urls: Option<&mut cef::CefStringList>,
        ) {
            /*
            CDXC:GPUIBrowserFavicons 2026-06-22-09:11:
            CEF favicon URL callbacks are runtime browser metadata only. Forward a single representative non-empty URL so GPUI browser chrome and sidebar sessions can show favicon presence, but keep bitmap download/cache and shell-state persistence of favicon URLs out of this slice.
            */
            let representative_url = icon_urls.and_then(|icon_urls| {
                // `CefStringList::clone` changes a mutable borrowed list into a
                // non-iterable immutable wrapper in cef-rs. Move the callback's
                // borrowed wrapper out instead so the URLs CEF supplied remain
                // visible to the iterator for the lifetime of this callback.
                let icon_urls = std::mem::take(icon_urls);
                icon_urls.into_iter().find_map(|url| {
                    let url = url.trim().to_string();
                    if url.is_empty() { None } else { Some(url) }
                })
            });
            (self.page_metadata_handler)(BrowserPageMetadataEvent::FaviconUrlChanged(
                representative_url,
            ));
        }
    }
}

wrap_permission_handler! {
    struct GhostexGpuiPermissionHandler {
        allow_first_party_loopback_requests: bool,
        trusted_clipboard_origin: Option<String>,
        media_access_handler: Option<BrowserMediaAccessHandler>,
    }

    impl PermissionHandler {
        fn on_request_media_access_permission(
            &self,
            _browser: Option<&mut cef::Browser>,
            _frame: Option<&mut Frame>,
            requesting_origin: Option<&CefString>,
            requested_permissions: u32,
            callback: Option<&mut MediaAccessCallback>,
        ) -> c_int {
            /*
            CDXC:GPUIBrowserMediaPermissions 2026-07-27:
            Only device microphone/camera requests are answered by the shell;
            desktop capture bits keep CEF's default deny so a mixed request can
            never grant screen capture as a side effect of a microphone
            decision. Surfaces without a media handler (sidebar and editor)
            also keep default handling.
            */
            let Some(handler) = self.media_access_handler.clone() else {
                return 0;
            };
            let kinds = BrowserMediaAccessKinds {
                microphone: requested_permissions
                    & MediaAccessPermissionTypes::DEVICE_AUDIO_CAPTURE.get_raw() as u32
                    != 0,
                camera: requested_permissions
                    & MediaAccessPermissionTypes::DEVICE_VIDEO_CAPTURE.get_raw() as u32
                    != 0,
            };
            if kinds.is_empty() {
                return 0;
            }
            let Some(callback) = callback else {
                return 0;
            };
            handler(BrowserMediaAccessRequest {
                requesting_origin: requesting_origin
                    .map(CefString::to_string)
                    .unwrap_or_default(),
                kinds,
                callback: Some(callback.clone()),
            });
            1
        }

        fn on_show_permission_prompt(
            &self,
            _browser: Option<&mut cef::Browser>,
            _prompt_id: u64,
            requesting_origin: Option<&CefString>,
            requested_permissions: u32,
            callback: Option<&mut PermissionPromptCallback>,
        ) -> c_int {
            /*
            CDXC:GPUIWindowsLoopbackPermission 2026-08-04:
            Current Windows CEF asks for LOCAL_NETWORK_ACCESS, LOCAL_NETWORK,
            or LOOPBACK_NETWORK before a bundled file:// app surface may call
            the authenticated loopback gxserver API. Alloy has no permission
            UI for these hidden first-party surfaces, so leaving the prompt to
            default handling strands fetch (and therefore sleeping-session
            wake) indefinitely.
            Accept only a pure local-network request on surfaces that were
            explicitly constructed with the sidebar gxserver bridge/bootstrap;
            Browser, editor, project-workarea, and modal surfaces keep their
            existing permission behavior.
            */
            let local_network_permissions =
                PermissionRequestTypes::LOCAL_NETWORK_ACCESS.get_raw() as u32
                    | PermissionRequestTypes::LOCAL_NETWORK.get_raw() as u32
                    | PermissionRequestTypes::LOOPBACK_NETWORK.get_raw() as u32;
            if self.allow_first_party_loopback_requests
                && requested_permissions & local_network_permissions != 0
                && requested_permissions & !local_network_permissions == 0
            {
                let Some(callback) = callback else {
                    return 0;
                };
                crate::support_logs::append(
                    crate::support_logs::GpuiSupportLog::TerminalFocus,
                    "gpui.cef.firstPartyLoopbackPermissionAccepted",
                    serde_json::json!({
                        "requestedPermissions": requested_permissions,
                    }),
                );
                callback.cont(PermissionRequestResult::ACCEPT);
                return 1;
            }
            /*
            macOS `GhostexCEFBrowserClient::OnShowPermissionPrompt` parity: only
            clipboard prompts are decided here (anything else keeps CEF's
            default handling), and clipboard is granted only when the request
            carries no other permission bits and the requesting origin matches
            this surface's trusted code-server origin. Embedded VS Code runs in
            CEF Alloy, whose default permission handling ignores clipboard
            prompts, so without this the code-server clipboard silently fails.
            */
            let Some(trusted_clipboard_origin) = self.trusted_clipboard_origin.as_deref() else {
                return 0;
            };
            let clipboard_permission = PermissionRequestTypes::CLIPBOARD.get_raw() as u32;
            if requested_permissions & clipboard_permission == 0 {
                return 0;
            }
            let Some(callback) = callback else {
                return 0;
            };
            let requesting_origin = requesting_origin
                .map(CefString::to_string)
                .unwrap_or_default();
            let unsupported_permissions = requested_permissions & !clipboard_permission;
            let should_accept = unsupported_permissions == 0
                && cef_origins_match(&requesting_origin, trusted_clipboard_origin);
            callback.cont(if should_accept {
                PermissionRequestResult::ACCEPT
            } else {
                PermissionRequestResult::DENY
            });
            1
        }
    }
}

fn cef_normalized_origin(value: &str) -> Option<String> {
    // Mirrors macOS `GhostexCEFNormalizedOrigin`: lowercased scheme://host with
    // the explicit port, defaulting http/https ports so "http://127.0.0.1:80"
    // and "http://127.0.0.1" compare equal; hostless/invalid values are None.
    let (scheme, rest) = value.split_once("://")?;
    let scheme = scheme.to_ascii_lowercase();
    let mut authority = rest.split(['/', '?', '#']).next().unwrap_or_default();
    if let Some((_, host)) = authority.rsplit_once('@') {
        authority = host;
    }
    let (host, explicit_port) = if let Some(bracket_end) = authority.rfind(']') {
        let (host, remainder) = authority.split_at(bracket_end + 1);
        (host, remainder.strip_prefix(':'))
    } else if let Some((host, port)) = authority.rsplit_once(':') {
        (host, Some(port))
    } else {
        (authority, None)
    };
    if scheme.is_empty() || host.is_empty() {
        return None;
    }
    let host = host.to_ascii_lowercase();
    let port = match explicit_port {
        Some(port) => port.parse::<u32>().ok()?,
        None => match scheme.as_str() {
            "http" => 80,
            "https" => 443,
            _ => return Some(format!("{scheme}://{host}")),
        },
    };
    Some(format!("{scheme}://{host}:{port}"))
}

fn cef_origins_match(lhs: &str, rhs: &str) -> bool {
    match (cef_normalized_origin(lhs), cef_normalized_origin(rhs)) {
        (Some(lhs), Some(rhs)) => lhs == rhs,
        _ => false,
    }
}

#[allow(dead_code)]
pub fn shutdown() {
    let Some(state) = CEF_RUNTIME.get() else {
        return;
    };
    let mut state = state
        .lock()
        .expect("CEF runtime mutex should not be poisoned");
    if state.is_none() {
        return;
    }
    CEF_SHUTDOWN_IN_PROGRESS.store(true, Ordering::Release);
    state.take();
    CEF_REQUEST_CONTEXTS_BY_PROFILE.with(|contexts| {
        contexts.borrow_mut().clear();
    });
    CEF_GLOBAL_REQUEST_CONTEXT.with(|context| {
        context.borrow_mut().take();
    });
    platform::invalidate_message_pump();
    cef::shutdown();
    CEF_CONTEXT_INITIALIZED.store(false, Ordering::Release);
}

fn apply_browser_page_appearance(browser: &cef::Browser) {
    let Some(host) = browser.host() else {
        return;
    };
    /*
    CDXC:GPUIBrowserPageAppearanceParity 2026-07-10:
    Public Browser tabs match the production macOS CEF host: the page receives
    the current system prefers-color-scheme while Chromium's unspecified
    document canvas stays Chrome-like white. This is renderer state only; it
    does not persist per-origin appearance, inject page CSS, or add a fallback
    rendering path.
    */
    let mut media_params = match cef::dictionary_value_create() {
        Some(params) => params,
        None => return,
    };
    media_params.set_string(Some(&CefString::from("media")), Some(&CefString::from("")));
    let mut features = match cef::list_value_create() {
        Some(features) => features,
        None => return,
    };
    let mut feature = match cef::dictionary_value_create() {
        Some(feature) => feature,
        None => return,
    };
    feature.set_string(
        Some(&CefString::from("name")),
        Some(&CefString::from("prefers-color-scheme")),
    );
    feature.set_string(
        Some(&CefString::from("value")),
        Some(&CefString::from(
            if platform::system_uses_dark_page_appearance() {
                "dark"
            } else {
                "light"
            },
        )),
    );
    features.set_dictionary(0, Some(&mut feature));
    media_params.set_list(Some(&CefString::from("features")), Some(&mut features));
    host.execute_dev_tools_method(
        next_page_appearance_devtools_message_id(),
        Some(&CefString::from("Emulation.setEmulatedMedia")),
        Some(&mut media_params),
    );

    let mut background_params = match cef::dictionary_value_create() {
        Some(params) => params,
        None => return,
    };
    let mut color = match cef::dictionary_value_create() {
        Some(color) => color,
        None => return,
    };
    for (key, value) in [("r", 255), ("g", 255), ("b", 255)] {
        color.set_int(Some(&CefString::from(key)), value);
    }
    color.set_double(Some(&CefString::from("a")), 1.0);
    background_params.set_dictionary(Some(&CefString::from("color")), Some(&mut color));
    host.execute_dev_tools_method(
        next_page_appearance_devtools_message_id(),
        Some(&CefString::from(
            "Emulation.setDefaultBackgroundColorOverride",
        )),
        Some(&mut background_params),
    );
}

fn next_page_appearance_devtools_message_id() -> c_int {
    PAGE_APPEARANCE_DEVTOOLS_MESSAGE_ID.with(|message_id| {
        let next = message_id.get().checked_add(1).unwrap_or(1);
        message_id.set(next);
        next
    })
}

pub struct CefBrowser {
    browser: RefCell<cef::Browser>,
    _client: Option<cef::Client>,
    _request_context: cef::RequestContext,
    last_bounds: RefCell<Option<(f32, f32, f32, f32, f32)>>,
    last_visible: Cell<Option<bool>>,
    uses_system_page_appearance: bool,
}

impl CefBrowser {
    pub fn new(
        parent_native_view: *mut c_void,
        url: &str,
        profile: &str,
        background_color: u32,
        uses_system_page_appearance: bool,
        trusted_clipboard_origin: Option<String>,
        popup_open_handler: Option<BrowserPopupOpenHandler>,
        page_metadata_handler: Option<BrowserPageMetadataHandler>,
        media_access_handler: Option<BrowserMediaAccessHandler>,
        sidebar_runtime_settings: Option<SidebarRuntimeSettingsSnapshot>,
        sidebar_gxserver_bootstrap: Option<SidebarGxserverBootstrap>,
        sidebar_bridge_event_handler: Option<SidebarBridgeEventHandler>,
        project_workarea_bridge_event_handler: Option<ProjectWorkareaBridgeEventHandler>,
        manage_docs_resource_scope: Option<ManageDocsResourceScope>,
        app_modal_host_bridge_surface: Option<AppModalHostBridgeSurface>,
        app_modal_host_bridge_event_handler: Option<AppModalHostBridgeEventHandler>,
        page_load_end_handler: Option<PageLoadEndHandler>,
    ) -> Result<Self, String> {
        let keyboard_zoom_enabled = page_metadata_handler.is_some()
            || project_workarea_bridge_event_handler.is_some()
            || app_modal_host_bridge_surface == Some(AppModalHostBridgeSurface::SessionChat);
        /*
        CDXC:GPUICefBrowserCreateFallible 2026-07-11:
        CreateBrowserSync returns null when the per-profile request context's
        asynchronous initialization has not completed yet (the same race the
        app-ui profiles dodge via the pre-initialized global context — see
        CDXC:GPUIAppUiPersistence 2026-07-09). This used to be an `.expect`
        that hard-crashed the whole app (five "failed to create cef-rs child
        browser" aborts on 2026-07-10, all from fresh browser
        profile contexts). Creation is now fallible; ensure-style callers skip
        the surface for this pass and naturally create it on their next
        reconcile once the context finishes initializing.
        */
        let initial_bounds = cef::Rect {
            x: 0,
            y: 0,
            width: 1,
            height: 1,
        };
        let window_info = platform::child_window_info(parent_native_view, &initial_bounds);
        /*
        macOS `createBrowserIfNeeded` trusted-clipboard parity: only surfaces
        constructed with a trusted clipboard origin (the code-server editor)
        enable JavaScript clipboard access, pre-grant Chromium's clipboard
        read/write content setting for that exact origin, and install the
        permission-prompt handler. Ordinary Browser panes keep CEF defaults.
        */
        let trusted_clipboard_origin = trusted_clipboard_origin
            .as_deref()
            .and_then(cef_normalized_origin);
        let allow_first_party_loopback_requests =
            sidebar_bridge_installed_for_handler(sidebar_bridge_event_handler.is_some())
                || sidebar_gxserver_bootstrap.is_some();
        let mut browser_settings = cef::BrowserSettings::default();
        if trusted_clipboard_origin.is_some() {
            browser_settings.javascript_access_clipboard = State::ENABLED;
            browser_settings.javascript_dom_paste = State::ENABLED;
        }
        let requested_url = url.to_string();
        if let Some(expected_surface) = app_modal_host_bridge_surface
            && app_modal_host_bridge_surface_for_frame_url(&requested_url) != Some(expected_surface)
        {
            return Err("app-modal CEF surface does not match its first-party entry URL".into());
        }
        let creation_url = if uses_system_page_appearance {
            "about:blank"
        } else {
            requested_url.as_str()
        };
        let creation_url = cef::CefString::from(creation_url);
        browser_settings.background_color = if uses_system_page_appearance {
            CEF_BROWSER_PAGE_BACKGROUND_COLOR
        } else {
            background_color
        };
        /*
        CDXC:GPUIBrowserMediaPermissions 2026-07-27:
        The permission handler now serves independent surfaces: the
        code-server clipboard grant (trusted origin only) and Browser-pane
        microphone/camera prompts, plus bundled sidebar/session-chat loopback
        access. Install it when any is in play, and keep the decisions
        independent inside the handler.
        */
        let permission_handler = (allow_first_party_loopback_requests
            || trusted_clipboard_origin.is_some()
            || media_access_handler.is_some())
        .then(|| {
            GhostexGpuiPermissionHandler::new(
                allow_first_party_loopback_requests,
                trusted_clipboard_origin.clone(),
                media_access_handler,
            )
        });
        let context_menu_handler = GhostexGpuiContextMenuHandler::new(popup_open_handler.clone());
        let display_handler = page_metadata_handler.as_ref().map(|handler| {
            GhostexGpuiDisplayHandler::new(handler.clone(), Cell::new(uses_system_page_appearance))
        });
        let find_handler = page_metadata_handler
            .as_ref()
            .map(|handler| GhostexGpuiFindHandler::new(handler.clone()));
        let manage_docs_resource_base_url = manage_docs_resource_scope
            .as_ref()
            .map(|scope| scope.base_url().to_string());
        let is_shared_sidebar_surface =
            sidebar_bridge_installed_for_handler(sidebar_bridge_event_handler.is_some());
        let request_handler = manage_docs_resource_scope
            .as_ref()
            .map(ManageDocsResourceScope::request_handler)
            .or_else(|| {
                is_shared_sidebar_surface.then(GhostexGpuiSidebarRendererRequestHandler::new)
            })
            .or_else(|| {
                // Browser panes are the only surface with a shell popup path,
                // so they are the only ones that turn middle-click and
                // Cmd/Ctrl-click link opens into Browser tabs.
                popup_open_handler
                    .clone()
                    .map(GhostexGpuiBrowserRequestHandler::new)
            });
        let browser_lifecycle_handler = page_metadata_handler.clone();
        let load_handler = if let Some(page_load_end_handler) = page_load_end_handler {
            /*
            CDXC:GPUITutorialVideoFullscreen 2026-08-18:
            Only bridge-less third-party surfaces (the tutorial video modal)
            pass this handler, so it can never displace the sidebar,
            session-chat, workarea, or Browser load handlers below.
            */
            Some(GhostexGpuiPageLoadEndHandler::new(page_load_end_handler))
        } else if sidebar_bridge_installed_for_handler(sidebar_bridge_event_handler.is_some()) {
            Some(GhostexGpuiSidebarProjectContextLoadHandler::new(
                sidebar_runtime_settings.unwrap_or_default(),
                sidebar_gxserver_bootstrap,
            ))
        } else if sidebar_gxserver_bootstrap.is_some() {
            /*
            CDXC:GPUISessionChatSurface 2026-07-31:
            A bootstrap without the sidebar bridge handler identifies the
            per-session Session Chat surface: it gets only the bootstrap
            install message so the bundled chat page can reach the local
            gxserver, while Browser, workarea, and modal clients keep passing
            no bootstrap at all.
            */
            Some(GhostexGpuiSessionChatGxserverBootstrapLoadHandler::new(
                sidebar_gxserver_bootstrap,
            ))
        } else if project_workarea_bridge_event_handler.is_some() {
            Some(GhostexGpuiProjectWorkareaBridgeLoadHandler::new(
                manage_docs_resource_base_url,
            ))
        } else {
            page_metadata_handler.map(GhostexGpuiBrowserPageLoadHandler::new)
        };
        // Every GPUI CEF browser needs the client's life-span handler so
        // DoClose is always handled and CEF can never close the host GPUI
        // window when a browser is dropped.
        let mut client = Some(GhostexGpuiCefClient::new(
            Some(GhostexGpuiLifeSpanHandler::new(
                popup_open_handler,
                browser_lifecycle_handler,
                false,
            )),
            Some(context_menu_handler),
            display_handler,
            find_handler,
            load_handler,
            sidebar_bridge_event_handler,
            project_workarea_bridge_event_handler,
            app_modal_host_bridge_event_handler,
            request_handler,
            permission_handler,
            Some(GhostexGpuiCefFocusHandler::new()),
            keyboard_zoom_handler(keyboard_zoom_enabled),
        ));
        let mut request_context = cef_request_context_for_profile(profile)
            .map_err(|error| format!("failed to create GPUI CEF request context: {error}"))?;
        if let Some(origin) = trusted_clipboard_origin.as_deref() {
            let origin = CefString::from(origin);
            request_context.set_content_setting(
                Some(&origin),
                Some(&origin),
                ContentSettingTypes::CLIPBOARD_READ_WRITE,
                ContentSettingValues::ALLOW,
            );
        }
        let browser = cef::browser_host_create_browser_sync(
            Some(&window_info),
            client.as_mut(),
            Some(&creation_url),
            Some(&browser_settings),
            None,
            Some(&mut request_context),
        )
        .ok_or_else(|| {
            "cef-rs child browser creation returned null (request context still initializing)"
                .to_string()
        })?;
        if let Some(host) = browser.host() {
            let native_view = platform::native_view_ptr(host.window_handle());
            platform::prepare_native_view_for_focus(native_view);
            /*
            CDXC:GPUISidebarPassiveMouseFocus 2026-07-22:
            The shared sidebar is chrome, not a work surface: clicking its
            background must never pull the keyboard away from the active
            terminal/pane. Mark exactly this surface mouse-focus passive so
            the AppKit focus subclass stops claiming first responder on its
            mouse-downs; keyboard focus arrives only through the fixed
            editable-focus bridge grant when the page focuses a text input.
            */
            if is_shared_sidebar_surface {
                platform::set_native_view_mouse_focus_passive(native_view, true);
            }
            register_native_view_browser(
                native_view,
                &browser,
                uses_system_page_appearance,
                keyboard_zoom_enabled,
            );
        }
        if uses_system_page_appearance {
            apply_browser_page_appearance(&browser);
            if let Some(frame) = browser.main_frame() {
                frame.load_url(Some(&CefString::from(requested_url.as_str())));
            }
        }

        Ok(Self {
            browser: RefCell::new(browser),
            _client: client,
            _request_context: request_context,
            last_bounds: RefCell::new(None),
            last_visible: Cell::new(None),
            uses_system_page_appearance,
        })
    }

    pub fn identifier(&self) -> i32 {
        self.browser.borrow().identifier()
    }

    pub fn set_bounds(&self, bounds: Bounds<Pixels>, scale_factor: f32) {
        /*
        `scale_factor` is the GPUI window's logical-to-physical ratio at the
        call site. AppKit children are positioned in points and Win32 queries
        per-window DPI itself, but X11 has no per-window scale query at all,
        so the only correct source for the Linux adapter is the value GPUI
        already computed for the parent window.
        */
        let x = bounds.origin.x.as_f32();
        let y = bounds.origin.y.as_f32();
        let width = bounds.size.width.as_f32().max(0.0);
        let height = bounds.size.height.as_f32().max(0.0);
        let raw_bounds = (x, y, width, height, scale_factor);
        {
            let mut last_bounds = self.last_bounds.borrow_mut();
            if last_bounds.as_ref() == Some(&raw_bounds) {
                return;
            }
            *last_bounds = Some(raw_bounds);
        }

        let rect = cef::Rect {
            x: x.round() as i32,
            y: y.round() as i32,
            width: width.round() as i32,
            height: height.round() as i32,
        };

        let browser = self.browser.borrow();
        let Some(host) = browser.host() else {
            return;
        };
        let native_view = platform::native_view_ptr(host.window_handle());
        /*
        CDXC:GPUICefNativeViewFrame 2026-06-14-15:25:
        Match Tauri's CEF child-view model: cef-rs owns the browser host while a thin platform adapter positions the native child view inside the GPUI-owned parent. The adapter respects the parent's coordinate/scale conventions (flipped NSView points on macOS, DPI-scaled physical pixels on Windows) so CEF never overlaps GPUI chrome or sibling surfaces.

        GPUI layout can place a surface on a half logical pixel. Preserve that
        raw rectangle through the platform seam: AppKit can position child
        views in fractional points, while the Windows and X11 adapters round
        only after converting to physical pixels. Rounding here shifted a
        half-point Browser origin by one backing pixel on Retina displays and
        exposed the surface background as a vertical seam.
        */
        let started_at = Instant::now();
        platform::set_native_view_frame(
            native_view,
            x as f64,
            y as f64,
            width as f64,
            height as f64,
            scale_factor,
        );
        let frame_elapsed = started_at.elapsed();
        let resize_started_at = Instant::now();
        host.was_resized();
        let resize_elapsed = resize_started_at.elapsed();
        if cef_resize_diagnostics_enabled() {
            platform::log_resize_diagnostic(
                browser.identifier(),
                rect.width,
                rect.height,
                frame_elapsed.as_micros() as u64,
                resize_elapsed.as_micros() as u64,
                started_at.elapsed().as_micros() as u64,
            );
        }
    }

    #[cfg(target_os = "macos")]
    pub fn native_view(&self) -> Option<*mut c_void> {
        let browser = self.browser.borrow();
        browser
            .host()
            .map(|host| platform::native_view_ptr(host.window_handle()))
    }

    pub fn set_visible(&self, visible: bool) {
        if self.last_visible.get() == Some(visible) {
            return;
        }
        if !visible {
            self.blur();
        }

        let browser = self.browser.borrow();
        if visible && self.uses_system_page_appearance {
            apply_browser_page_appearance(&browser);
        }
        let Some(host) = browser.host() else {
            return;
        };
        let native_view = platform::native_view_ptr(host.window_handle());
        set_cef_native_view_hidden(native_view, !visible);
        platform::set_native_view_visible(native_view, visible);
        self.last_visible.set(Some(visible));
    }

    pub fn order_front(&self) {
        /*
        CDXC:GPUITitlebarDropdownZOrder 2026-07-09:
        Native child views stack in creation order, and terminal host views
        keep being appended as sessions mount. Reused overlay CEF surfaces
        (titlebar dropdown panels) must re-assert their top sibling position
        when shown, or they reappear underneath newer terminal views. Only
        intentional overlay surfaces may call this; normal laid-out surfaces
        rely on non-overlapping frames instead of z-order.
        */
        let browser = self.browser.borrow();
        let Some(host) = browser.host() else {
            return;
        };
        platform::order_native_view_front(platform::native_view_ptr(host.window_handle()));
    }

    pub fn focus(&self) {
        let browser = self.browser.borrow();
        let Some(host) = browser.host() else {
            return;
        };
        /*
        CDXC:GPUICefFocusRouting 2026-06-14-16:31:
        Web-page text fields inside CEF must regain both native focus ownership (AppKit first responder / Win32 keyboard focus) and Chromium browser focus after GPUI chrome has been focused. Without this handoff, command shortcuts such as Cmd+A can stay routed to GPUI instead of selecting text in the active page input.
        */
        platform::focus_native_view(platform::native_view_ptr(host.window_handle()));
        host.set_focus(1);
    }

    pub fn blur(&self) {
        let browser = self.browser.borrow();
        let Some(host) = browser.host() else {
            return;
        };
        /*
        CDXC:GPUIBrowserLifecycle 2026-06-23-11:32:
        Hiding a GPUI Browser CEF child view for sleep, mode switch, or tab drag must also release Chromium focus and runtime active-view bookkeeping so hidden pages cannot keep command-dispatch ownership. This is a narrow native-view boundary blur; it does not destroy the CEF browser, change layout, persist data, log content, or synthesize native hit routing.
        */
        let native_view = platform::native_view_ptr(host.window_handle());
        host.set_focus(0);
        clear_active_native_view_if_matching(native_view);
    }

    pub fn select_all(&self) {
        self.focus();
        let browser = self.browser.borrow();
        select_all_in_browser(&browser);
    }

    /*
    CDXC:GPUITutorialVideoFullscreen 2026-08-18:
    The tutorial modal loads the YouTube watch page as its own top-level CEF
    document, so the app cannot put its player in fullscreen from injected
    JavaScript: Chromium's Fullscreen API requires a transient user
    activation, and app-owned `execute_java_script` runs without one (the
    `requestFullscreen()` promise is rejected outright). Sending the key
    through the browser host instead feeds Chromium's real input pipeline, so
    the page sees a trusted keydown with user activation and runs its own "f"
    shortcut. "f" toggles, so callers must send this exactly once per loaded
    page. This carries no page data and does not persist or log anything.
    */
    pub fn send_fullscreen_toggle_key(&self) {
        // Windows virtual key code for F; Chromium derives DOM `code`/`key`
        // from this plus the platform-native code below.
        const VK_F: c_int = 0x46;
        #[cfg(target_os = "macos")]
        const NATIVE_F_KEY_CODE: c_int = 3; // kVK_ANSI_F
        #[cfg(target_os = "linux")]
        const NATIVE_F_KEY_CODE: c_int = 41; // X11 keycode for F
        #[cfg(not(any(target_os = "macos", target_os = "linux")))]
        const NATIVE_F_KEY_CODE: c_int = 0;

        let browser = self.browser.borrow();
        let Some(host) = browser.host() else {
            return;
        };
        let mut event = cef::KeyEvent {
            size: std::mem::size_of::<cef::sys::cef_key_event_t>(),
            type_: cef::KeyEventType::RAWKEYDOWN,
            modifiers: 0,
            windows_key_code: VK_F,
            native_key_code: NATIVE_F_KEY_CODE,
            is_system_key: 0,
            character: b'f' as u16,
            unmodified_character: b'f' as u16,
            focus_on_editable_field: 0,
        };
        host.send_key_event(Some(&event));
        // CEF's char event carries the produced character in the key code.
        event.type_ = cef::KeyEventType::CHAR;
        event.windows_key_code = b'f' as c_int;
        host.send_key_event(Some(&event));
        event.type_ = cef::KeyEventType::KEYUP;
        event.windows_key_code = VK_F;
        host.send_key_event(Some(&event));
    }

    pub fn load_url(&self, url: &str) {
        let browser = self.browser.borrow();
        if let Some(frame) = browser.main_frame() {
            frame.load_url(Some(&cef::CefString::from(url)));
        }
    }

    pub fn execute_java_script_in_main_frame(&self, script: &str) -> bool {
        let browser = self.browser.borrow();
        let Some(frame) = browser.main_frame() else {
            return false;
        };
        /*
        CDXC:GPUIBrowserFeedback 2026-06-23-11:04:
        GPUI Browser feedback tools now use CEF's normal main-frame JavaScript execution path for app-owned injection scripts. Pass a synthetic script URL and return only main-frame availability so this backend does not log page URLs, titles, script bodies, user content, JS errors, cookies, tokens, paths, command text, or terminal content.

        CDXC:GPUICefAppOwnedScriptFocus 2026-07-15:
        App-owned renderer notifications are sideband state delivery, not an
        input-focus action. Executing one must preserve the current GPUI,
        terminal, or CEF responder; callers that represent an explicit user
        focus action invoke `focus` separately. Focusing here caused sidebar
        attention/session-selection notifications to steal keyboard ownership
        synchronously during terminal mouse-down handling.
        */
        frame.execute_java_script(
            Some(&cef::CefString::from(script)),
            Some(&cef::CefString::from(BROWSER_APP_OWNED_SCRIPT_URL)),
            1,
        );
        true
    }

    pub fn refresh_sidebar_runtime_settings(
        &self,
        runtime_settings: SidebarRuntimeSettingsSnapshot,
    ) {
        let browser = self.browser.borrow();
        let Some(mut frame) = browser.main_frame() else {
            return;
        };
        send_sidebar_runtime_settings_process_message(
            &mut frame,
            SIDEBAR_RUNTIME_SETTINGS_UPDATE_MESSAGE_NAME,
            runtime_settings,
        );
    }

    pub fn refresh_sidebar_gxserver_bootstrap(
        &self,
        gxserver_bootstrap: Option<SidebarGxserverBootstrap>,
    ) {
        let browser = self.browser.borrow();
        let Some(mut frame) = browser.main_frame() else {
            return;
        };
        send_sidebar_gxserver_bootstrap_process_message(&mut frame, gxserver_bootstrap);
    }

    pub fn refresh_session_chat_gxserver_bootstrap(
        &self,
        gxserver_bootstrap: Option<SidebarGxserverBootstrap>,
    ) {
        /*
        CDXC:GPUISessionChatSurface 2026-07-31:
        Session Chat surfaces refresh through their dedicated bootstrap
        message because the sidebar update path refuses pages without the
        installed sidebar bridge. Same scope rules as the sidebar refresh:
        app-owned snapshot only, main frame only, never logged or persisted.
        */
        let browser = self.browser.borrow();
        let Some(mut frame) = browser.main_frame() else {
            return;
        };
        send_session_chat_gxserver_bootstrap_process_message(&mut frame, gxserver_bootstrap);
    }

    pub fn can_go_back(&self) -> bool {
        self.browser.borrow().can_go_back() != 0
    }

    pub fn go_back(&self) {
        if !self.can_go_back() {
            return;
        }
        self.focus();
        self.browser.borrow().go_back();
    }

    pub fn can_go_forward(&self) -> bool {
        self.browser.borrow().can_go_forward() != 0
    }

    pub fn go_forward(&self) {
        if !self.can_go_forward() {
            return;
        }
        self.focus();
        self.browser.borrow().go_forward();
    }

    pub fn reload(&self) {
        self.focus();
        self.browser.borrow().reload();
    }

    pub fn stop_load(&self) {
        self.focus();
        self.browser.borrow().stop_load();
    }

    pub fn find_text(&self, search_text: &str, forward: bool, find_next: bool) {
        let search_text = search_text.trim();
        if search_text.is_empty() {
            self.stop_finding(true);
            return;
        }
        let browser = self.browser.borrow();
        let Some(host) = browser.host() else {
            return;
        };
        host.find(
            Some(&CefString::from(search_text)),
            forward as c_int,
            0,
            find_next as c_int,
        );
    }

    pub fn stop_finding(&self, clear_selection: bool) {
        let browser = self.browser.borrow();
        let Some(host) = browser.host() else {
            return;
        };
        host.stop_finding(clear_selection as c_int);
    }

    pub fn zoom_level(&self) -> f64 {
        let browser = self.browser.borrow();
        let Some(host) = browser.host() else {
            return 0.0;
        };
        host.zoom_level()
    }

    pub fn zoom_in(&self) {
        let browser = self.browser.borrow();
        let Some(host) = browser.host() else {
            return;
        };
        host.zoom(ZoomCommand::IN);
    }

    pub fn zoom_out(&self) {
        let browser = self.browser.borrow();
        let Some(host) = browser.host() else {
            return;
        };
        host.zoom(ZoomCommand::OUT);
    }

    pub fn reset_zoom(&self) {
        let browser = self.browser.borrow();
        let Some(host) = browser.host() else {
            return;
        };
        /*
        CDXC:GPUIBrowserToolbar 2026-06-22-11:59:
        Zoom reset in the GPUI browser toolbar must use Chromium's browser-host zoom level, matching native CEF behavior and avoiding CSS, JavaScript, overlay, or fallback scaling.
        */
        host.zoom(ZoomCommand::RESET);
    }

    pub fn toggle_dev_tools(&self) {
        let mut browser = self.browser.borrow_mut();
        let Some(host) = browser.host() else {
            return;
        };
        /*
        CDXC:GPUIBrowserToolbar 2026-06-22-11:50:
        Browser toolbar DevTools is a real CEF host action in GPUI. Toggle the browser's associated DevTools surface through CEF itself so the toolbar action is not a silent placeholder and no GPUI overlay, hidden hit region, or synthetic coordinate routing is introduced.
        */
        if host.has_dev_tools() != 0 {
            host.close_dev_tools();
            return;
        }
        show_browser_dev_tools(Some(&mut browser), None);
    }
}

impl Drop for CefBrowser {
    fn drop(&mut self) {
        if let Some(host) = self.browser.borrow().host() {
            let native_view = platform::native_view_ptr(host.window_handle());
            unregister_native_view_browser(native_view);
            platform::release_native_view(native_view);
            host.close_browser(1);
            /*
            CDXC:GPUICefDropPumpReentrancy 2026-07-11:
            CefBrowser drops happen inside gpui entity updates while the
            AppCell is borrowed. Pumping cef::do_message_loop_work() inline
            here ran arbitrary Chromium tasks and CEF handler callbacks
            synchronously in that borrowed context (a handler touching the
            app re-borrows the AppCell and panics), could nest CEF's message
            loop work if the drop itself ran from a scheduled pump step
            (which CEF forbids, and the ObjC-side re-entrancy guard cannot
            see direct calls), and added unbounded main-thread latency to
            the update. Nothing requires the close to complete within drop:
            close_browser(1) only queues the teardown, so ask the external
            message pump (the same scheduling entry
            BrowserProcessHandler::on_schedule_message_pump_work uses) to
            run soon and let CEF process the close on later runloop turns.
            */
            platform::schedule_message_pump_work(0);
        }
    }
}

fn cef_root_cache_path() -> Result<PathBuf> {
    /*
    CDXC:GPUIPrivacyAudit 2026-06-23-13:18:
    The explicit CEF root cache path prevents Chromium from falling back to its platform default user-data folder. The built-in Default Browser profile and first-party app-UI surfaces use the durable global context, while generated Browser profiles remain memory-backed.
    */
    let os_default_root = Some(crate::shared_settings::ghostex_storage_paths().cef_cache_dir());
    let path = std::env::var_os("GHOSTEX_GPUI_CEF_CACHE_DIR")
        .map(PathBuf::from)
        .or(os_default_root)
        .unwrap_or_else(|| std::env::temp_dir().join("ghostex-gpui/cef"));
    std::fs::create_dir_all(&path).context("failed to create GPUI CEF root cache directory")?;
    Ok(path)
}

fn cef_request_context_for_profile(profile: &str) -> Result<cef::RequestContext> {
    /*
    CDXC:GPUIBrowserProfilePersistence 2026-07-16:
    Browser profile ids are app-global rather than project- or tab-scoped. The
    built-in Default profile uses CEF's pre-initialized durable global context,
    so ordinary logins survive app restarts and are visible from every
    Default-profile tab/project. Generated profiles remain separate and
    memory-backed.

    CDXC:GPUIAppUiPersistence 2026-07-09-03:40:
    First-party app-UI surfaces (sidebar, app modal, titlebar panels, project workareas) need durable localStorage for UI state (collapse state, Show more/less, project order), matching how the macOS sidebar WKWebViews use the persistent default WKWebsiteDataStore. They and the built-in Default Browser profile use CEF's global persistent request context, which is initialized with the runtime before synchronous browser creation. Creating a new disk-backed request context here races its asynchronous initialization and causes CreateBrowserSync to return null during app startup. Generated Browser profiles stay memory-backed.
    */
    let profile_segment = cef_profile_cache_segment(profile)
        .unwrap_or("default")
        .to_string();
    if cef_profile_is_app_ui(&profile_segment) || profile_segment == "default" {
        return CEF_GLOBAL_REQUEST_CONTEXT.with(|cached| {
            if let Some(context) = cached.borrow().as_ref() {
                return Ok(context.clone());
            }
            let context = cef::request_context_get_global_context()
                .context("failed to access GPUI CEF global persistent request context")?;
            *cached.borrow_mut() = Some(context.clone());
            Ok(context)
        });
    }
    CEF_REQUEST_CONTEXTS_BY_PROFILE.with(|contexts| {
        if let Some(context) = contexts.borrow().get(&profile_segment) {
            return Ok(context.clone());
        }

        let settings = cef::RequestContextSettings {
            persist_session_cookies: 0,
            ..Default::default()
        };
        let context = cef::request_context_create_context(Some(&settings), None)
            .context("failed to create GPUI CEF profile request context")?;
        contexts
            .borrow_mut()
            .insert(profile_segment, context.clone());
        Ok(context)
    })
}

fn cef_profile_is_app_ui(profile_segment: &str) -> bool {
    matches!(
        profile_segment,
        "gpui-sidebar" | "app-modal" | "session-chat"
    ) || profile_segment.starts_with("titlebar-")
        || profile_segment.starts_with("project-workarea-")
}

fn cef_profile_cache_segment(profile: &str) -> Option<&str> {
    let profile = profile.trim();
    if profile.is_empty() || profile.len() > 64 {
        return None;
    }
    if !profile
        .bytes()
        .next()
        .is_some_and(|byte| byte.is_ascii_alphanumeric())
        || !profile
            .bytes()
            .last()
            .is_some_and(|byte| byte.is_ascii_alphanumeric())
    {
        return None;
    }
    profile
        .bytes()
        .all(|byte| matches!(byte, b'a'..=b'z' | b'0'..=b'9' | b'-'))
        .then_some(profile)
}

fn remote_debugging_port() -> i32 {
    // Tooling (browser-use MCP, macOS app scripts) sets the shared
    // GHOSTEX_CEF_REMOTE_DEBUGGING_PORT; the GPUI-specific name stays as a
    // more-specific override so side-by-side runs can split ports. The
    // default 9334 stays inside the tooling's 9333-9343 scan range.
    [
        "GHOSTEX_GPUI_CEF_REMOTE_DEBUGGING_PORT",
        "GHOSTEX_CEF_REMOTE_DEBUGGING_PORT",
    ]
    .iter()
    .find_map(|name| {
        std::env::var(name)
            .ok()
            .and_then(|value| value.parse::<i32>().ok())
            .filter(|port| *port > 0)
    })
    .unwrap_or(9334)
}

fn register_native_view_browser(
    native_view: *mut c_void,
    browser: &cef::Browser,
    uses_system_page_appearance: bool,
    keyboard_zoom_enabled: bool,
) {
    if native_view.is_null() {
        return;
    }

    CEF_BROWSERS_BY_NATIVE_VIEW.with(|browsers| {
        browsers
            .borrow_mut()
            .insert(native_view as usize, browser.clone());
    });
    if keyboard_zoom_enabled {
        KEYBOARD_ZOOM_CEF_NATIVE_VIEWS.with(|views| {
            views.borrow_mut().insert(native_view as usize);
        });
    }
    if uses_system_page_appearance {
        SYSTEM_PAGE_APPEARANCE_CEF_NATIVE_VIEWS.with(|views| {
            views.borrow_mut().insert(native_view as usize);
        });
    }
}

fn unregister_native_view_browser(native_view: *mut c_void) {
    if native_view.is_null() {
        return;
    }

    CEF_BROWSERS_BY_NATIVE_VIEW.with(|browsers| {
        browsers.borrow_mut().remove(&(native_view as usize));
    });
    KEYBOARD_ZOOM_CEF_NATIVE_VIEWS.with(|views| {
        views.borrow_mut().remove(&(native_view as usize));
    });
    SYSTEM_PAGE_APPEARANCE_CEF_NATIVE_VIEWS.with(|views| {
        views.borrow_mut().remove(&(native_view as usize));
    });
    set_cef_native_view_hidden(native_view, false);
    clear_active_native_view_if_matching(native_view);
    let _ = SIDEBAR_EDITABLE_FOCUS_NATIVE_VIEW.compare_exchange(
        native_view as usize,
        0,
        Ordering::AcqRel,
        Ordering::Acquire,
    );
}

pub(super) fn refresh_system_page_appearance_for_native_view(native_view: *mut c_void) -> c_int {
    if native_view.is_null()
        || !SYSTEM_PAGE_APPEARANCE_CEF_NATIVE_VIEWS
            .with(|views| views.borrow().contains(&(native_view as usize)))
    {
        return 0;
    }
    let browser = CEF_BROWSERS_BY_NATIVE_VIEW
        .with(|browsers| browsers.borrow().get(&(native_view as usize)).cloned());
    let Some(browser) = browser else {
        return 0;
    };
    apply_browser_page_appearance(&browser);
    1
}

fn clear_active_native_view_if_matching(native_view: *mut c_void) {
    if native_view.is_null() {
        return;
    }

    let _ = ACTIVE_CEF_NATIVE_VIEW.compare_exchange(
        native_view as usize,
        0,
        Ordering::AcqRel,
        Ordering::Acquire,
    );
}

pub(super) fn clear_active_native_view() {
    ACTIVE_CEF_NATIVE_VIEW.store(0, Ordering::Release);
}

fn select_all_in_browser(browser: &cef::Browser) -> bool {
    if let Some(frame) = browser.focused_frame().or_else(|| browser.main_frame()) {
        frame.select_all();
        true
    } else {
        false
    }
}

/*
CDXC:GPUICefEditCommands 2026-07-09:
Cut/Copy/Paste join Select All as bridged edit commands because GPUI's
window-level key dispatch consumes Cmd-chords before AppKit can deliver
them to CEF child views, so settings, modal-host, sidebar, and browser
pages never receive the standard clipboard shortcuts. The raw values are
the ABI contract with the AppKit shim (GpuiCefAppKitHooks.m); both sides
must stay in sync.
*/
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum CefEditCommand {
    Cut,
    Copy,
    Paste,
}

impl CefEditCommand {
    pub(super) fn from_raw(raw: c_int) -> Option<Self> {
        match raw {
            1 => Some(Self::Cut),
            2 => Some(Self::Copy),
            3 => Some(Self::Paste),
            _ => None,
        }
    }
}

/*
CDXC:GPUICefPaneZoomShortcuts 2026-07-14:
The AppKit CEF responder subclass forwards only the standard page-zoom
commands for Browser, main project-workarea, and Session Chat native views.
The raw values are the narrow ABI contract with GpuiCefAppKitHooks.m. Sidebar,
modal, titlebar, and companion CEF views are deliberately absent from the
keyboard zoom registry even though they share the browser registry used by
editing.
*/
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum CefZoomCommand {
    In,
    Out,
    Reset,
}

impl CefZoomCommand {
    pub(super) fn from_raw(raw: c_int) -> Option<Self> {
        match raw {
            1 => Some(Self::In),
            2 => Some(Self::Out),
            3 => Some(Self::Reset),
            _ => None,
        }
    }
}

fn edit_command_in_browser(browser: &cef::Browser, command: CefEditCommand) -> bool {
    let Some(frame) = browser.focused_frame().or_else(|| browser.main_frame()) else {
        return false;
    };
    match command {
        CefEditCommand::Cut => frame.cut(),
        CefEditCommand::Copy => frame.copy(),
        CefEditCommand::Paste => frame.paste(),
    }
    true
}

/*
CDXC:GPUICefPlatformSeam 2026-07-04:
The select-all/active-view helpers stay in shared code because the registry
they consult is shared, but the entry points that reach them are per-OS:
macOS exports them to the AppKit responder-chain shim (cef/macos.rs), while
Windows/Chromium routes Ctrl+A to the focused browser HWND natively and
needs no external dispatch hook.
*/
pub(super) fn select_all_for_native_view(native_view: *mut c_void) -> c_int {
    if native_view.is_null() {
        return 0;
    }

    let browser = CEF_BROWSERS_BY_NATIVE_VIEW
        .with(|browsers| browsers.borrow().get(&(native_view as usize)).cloned());
    let Some(browser) = browser else {
        return 0;
    };

    set_active_cef_native_view(native_view as usize);

    /*
    CDXC:GPUICefEditCommands 2026-08-18:
    Taking native focus here is only for the case where GPUI chrome still owns
    the first responder while the page holds the caret. When Chromium already
    owns the keyboard, re-running the focus handoff walks the responder out of
    the render widget and back, and Blink reports that round trip to the page
    as a blur/focus pair — which commits and closes any field that saves on
    blur, such as the sidebar group rename. Select All must never disturb the
    focus that already exists.
    */
    if let Some(host) = browser.host() {
        let host_view = platform::native_view_ptr(host.window_handle());
        if !platform::native_view_owns_first_responder(host_view) {
            platform::focus_native_view(host_view);
            host.set_focus(1);
        }
    }

    if select_all_in_browser(&browser) {
        1
    } else {
        0
    }
}

pub(super) fn select_all_for_active_native_view() -> c_int {
    let native_view = active_cef_native_view();
    let Some(native_view) = native_view else {
        return 0;
    };
    select_all_for_native_view(native_view as *mut c_void)
}

/*
CDXC:GPUICefEditCommands 2026-07-09:
Unlike Select All, clipboard commands are destructive to shared clipboard
state, so the AppKit shim resolves the target by walking the key window's
actual first responder instead of the last-active CEF view registry; a
stale active view (e.g. after clicking into a native Ghostty terminal)
must never receive a mirrored Cmd+C/X/V.
*/
pub(super) fn edit_command_for_native_view(
    native_view: *mut c_void,
    command: CefEditCommand,
) -> c_int {
    if native_view.is_null() {
        crate::support_logs::append(
            crate::support_logs::GpuiSupportLog::TerminalFocus,
            "gpui.cef.editCommandNativeViewRoute",
            serde_json::json!({
                "command": format!("{command:?}"),
                "matchedBrowser": false,
                "nativeViewWasNull": true,
            }),
        );
        return 0;
    }

    let browser = CEF_BROWSERS_BY_NATIVE_VIEW
        .with(|browsers| browsers.borrow().get(&(native_view as usize)).cloned());
    let Some(browser) = browser else {
        return 0;
    };

    set_active_cef_native_view(native_view as usize);

    if let Some(host) = browser.host() {
        host.set_focus(1);
    }

    let handled = edit_command_in_browser(&browser, command);
    crate::support_logs::append(
        crate::support_logs::GpuiSupportLog::TerminalFocus,
        "gpui.cef.editCommandNativeViewRoute",
        serde_json::json!({
            "command": format!("{command:?}"),
            "matchedBrowser": true,
            "browserId": browser.identifier(),
            "isPopup": browser.is_popup() != 0,
            "handled": handled,
        }),
    );
    if handled { 1 } else { 0 }
}

pub(super) fn zoom_command_for_native_view(
    native_view: *mut c_void,
    command: CefZoomCommand,
) -> c_int {
    if native_view.is_null()
        || !KEYBOARD_ZOOM_CEF_NATIVE_VIEWS
            .with(|views| views.borrow().contains(&(native_view as usize)))
    {
        return 0;
    }

    let browser = CEF_BROWSERS_BY_NATIVE_VIEW
        .with(|browsers| browsers.borrow().get(&(native_view as usize)).cloned());
    let Some(browser) = browser else {
        return 0;
    };
    let Some(host) = browser.host() else {
        return 0;
    };

    set_active_cef_native_view(native_view as usize);
    host.set_focus(1);
    match command {
        CefZoomCommand::In => host.zoom(ZoomCommand::IN),
        CefZoomCommand::Out => host.zoom(ZoomCommand::OUT),
        CefZoomCommand::Reset => host.zoom(ZoomCommand::RESET),
    }
    1
}

/*
CDXC:GPUISidebarPassiveMouseFocus 2026-07-22:
App-owned focus grant/release for the mouse-focus-passive sidebar surface.
"focused" repeats the exact sequence every sanctioned grant already uses
(mark the explicit active view, then native first responder, then Chromium
focus) so the OnSetFocus arbitration recognizes it as app-owned. "blurred"
releases only if the sidebar actually owns the first responder, handing the
keyboard back to the GPUI root so the previously focused terminal types
again without requiring a click.
*/
fn handle_sidebar_editable_focus(browser: Option<&mut cef::Browser>, payload: &str) {
    let focused = match payload {
        "focused" => true,
        "blurred" => false,
        _ => return,
    };
    let Some(host) = browser.and_then(|browser| browser.host()) else {
        return;
    };
    let native_view = platform::native_view_ptr(host.window_handle());
    if native_view.is_null() {
        return;
    }

    let owned_first_responder = platform::native_view_owns_first_responder(native_view);
    if focused {
        SIDEBAR_EDITABLE_FOCUS_NATIVE_VIEW.store(native_view as usize, Ordering::Release);
        set_active_cef_native_view(native_view as usize);
        platform::set_native_view_passive_focus_grant(native_view, true);
        platform::focus_native_view(native_view);
        host.set_focus(1);
    } else {
        let _ = SIDEBAR_EDITABLE_FOCUS_NATIVE_VIEW.compare_exchange(
            native_view as usize,
            0,
            Ordering::AcqRel,
            Ordering::Acquire,
        );
        platform::set_native_view_passive_focus_grant(native_view, false);
        if owned_first_responder {
            host.set_focus(0);
            platform::return_focus_to_gpui_root(native_view);
        }
    }
    crate::support_logs::append(
        crate::support_logs::GpuiSupportLog::TerminalFocus,
        "gpui.cef.sidebarEditableFocus",
        serde_json::json!({
            "focused": focused,
            "ownedFirstResponder": owned_first_responder,
        }),
    );
}

pub(super) fn mark_native_view_focused(native_view: *mut c_void) -> c_int {
    if native_view.is_null() {
        return 0;
    }

    let is_cef_view = CEF_BROWSERS_BY_NATIVE_VIEW
        .with(|browsers| browsers.borrow().contains_key(&(native_view as usize)));
    if !is_cef_view {
        return 0;
    }

    set_active_cef_native_view(native_view as usize);
    1
}

#[cfg(target_os = "macos")]
#[allow(clippy::too_many_arguments)]
pub(super) fn log_native_mouse_down(
    native_view: *mut c_void,
    event_window_x: f64,
    event_window_y: f64,
    frame_window_x: f64,
    frame_window_y: f64,
    frame_width: f64,
    frame_height: f64,
    parent_bounds_width: f64,
    parent_bounds_height: f64,
    hidden: bool,
    responder_class: String,
) {
    let browser = CEF_BROWSERS_BY_NATIVE_VIEW
        .with(|browsers| browsers.borrow().get(&(native_view as usize)).cloned());
    let event_inside_native_frame = event_window_x >= frame_window_x
        && event_window_y >= frame_window_y
        && event_window_x < frame_window_x + frame_width
        && event_window_y < frame_window_y + frame_height;
    crate::support_logs::append(
        crate::support_logs::GpuiSupportLog::TerminalFocus,
        "gpui.cef.nativeMouseDown",
        serde_json::json!({
            "browserId": browser.as_ref().map(cef::Browser::identifier),
            "isPopup": browser.as_ref().is_some_and(|browser| browser.is_popup() != 0),
            "eventWindow": [event_window_x, event_window_y],
            "nativeFrameWindow": [frame_window_x, frame_window_y, frame_width, frame_height],
            "parentBounds": [parent_bounds_width, parent_bounds_height],
            "eventInsideNativeFrame": event_inside_native_frame,
            "hidden": hidden,
            "responderClass": responder_class,
        }),
    );
}

pub(super) fn clear_active_native_view_registry() {
    clear_active_native_view();
}
