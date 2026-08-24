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

// C4 light split: the modules below hold the bulk of what used to be
// this file's content; see docs/2026-08-22/repo-restructure/SPLITS.md C4
// for the cluster map. `pub(crate) use` re-exports keep every existing
// `shell::name` and `super::shell::name` path (cef/mod.rs's `pub use
// shell::*`, cef/macos.rs, cef/windows.rs, cef/linux_x11.rs) resolving
// unchanged.
mod browser;
mod client;
mod lifecycle;
mod message_routing;
mod native_view;
mod request_handling;

pub(crate) use browser::*;
pub(crate) use client::*;
pub(crate) use lifecycle::*;
pub(crate) use message_routing::*;
pub(crate) use native_view::*;
pub(crate) use request_handling::*;
