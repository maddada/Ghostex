pub use super::sidebar_bridge_manifest::ExtensionBridgeSurfaceSpec;
use super::sidebar_bridge_manifest::{
    EXTENSION_BRIDGE_INSTALL_MESSAGE_NAME, EXTENSION_BRIDGE_PAYLOAD_MAX_CHARS,
    EXTENSION_BRIDGE_PROCESS_MESSAGE_NAME, EXTENSION_BRIDGE_RUNTIME_SHIM,
    PROJECT_WORKAREA_BRIDGE_FUNCTION_SPECS, PROJECT_WORKAREA_BRIDGE_INSTALL_MESSAGE_NAME,
    PROJECT_WORKAREA_BRIDGE_PAYLOAD_MAX_CHARS, PROJECT_WORKAREA_MANAGE_DOCS_RESOURCE_BASE_URL,
    PROJECT_WORKAREA_MANAGE_DOCS_RESOURCE_BASE_URL_JS_FIELD, ProjectWorkareaBridgeFunctionId,
    SIDEBAR_PROJECT_CONTEXT_JS_NAMESPACE, WEBKIT_EXTENSION_HOST_MESSAGE_HANDLER_JS_OBJECT,
    WEBKIT_JS_OBJECT, WEBKIT_MESSAGE_HANDLERS_JS_OBJECT, WEBKIT_POST_MESSAGE_JS_FUNCTION,
    project_workarea_bridge_function_spec_for_js_function,
    project_workarea_bridge_function_spec_for_process_message,
};
use anyhow::{Context as _, Result};
use cef::rc::Rc as _;
use cef::wrapper::resource_manager::{get_mime_type, get_url_without_query_or_fragment};
use cef::{
    App, BeforeDownloadCallback, BrowserProcessHandler, BrowserSettings, Callback, CefString,
    Client, CommandLine, ContentSettingTypes, ContentSettingValues, ContextMenuHandler,
    ContextMenuMediaType, ContextMenuParams, DictionaryValue, DisplayHandler, DownloadHandler,
    DownloadImageCallback, DownloadItem, EventFlags, FindHandler, FocusHandler, FocusSource, Frame,
    ImplApp, ImplBeforeDownloadCallback as _, ImplBinaryValue as _, ImplBrowser as _,
    ImplBrowserHost as _, ImplBrowserProcessHandler, ImplClient, ImplCommandLine as _,
    ImplContextMenuHandler, ImplContextMenuParams as _, ImplDictionaryValue as _,
    ImplDisplayHandler, ImplDownloadHandler, ImplDownloadImageCallback, ImplFindHandler,
    ImplFocusHandler, ImplFrame as _, ImplImage as _, ImplLifeSpanHandler, ImplListValue as _,
    ImplLoadHandler, ImplMediaAccessCallback as _, ImplMenuModel as _, ImplPermissionHandler,
    ImplPermissionPromptCallback as _, ImplProcessMessage as _, ImplRenderProcessHandler,
    ImplRequest as _, ImplRequestContext as _, ImplRequestHandler, ImplResourceHandler,
    ImplResourceRequestHandler, ImplResponse as _, ImplStreamReader as _, ImplTask,
    ImplV8Context as _, ImplV8Handler, ImplV8Value as _, KeyboardHandler, LifeSpanHandler,
    LoadHandler, MediaAccessCallback, MediaAccessPermissionTypes, MenuModel, PermissionHandler,
    PermissionPromptCallback, PermissionRequestResult, PermissionRequestTypes, PopupFeatures,
    ProcessId, ProcessMessage, RenderProcessHandler, Request, RequestHandler, ResourceHandler,
    ResourceReadCallback, ResourceRequestHandler, Response, ReturnValue, State, StreamReader, Task,
    ThreadId, V8Handler, V8Propertyattribute, V8Value, ValueType, WindowInfo,
    WindowOpenDisposition, WrapApp, WrapBrowserProcessHandler, WrapClient, WrapContextMenuHandler,
    WrapDisplayHandler, WrapDownloadHandler, WrapDownloadImageCallback, WrapFindHandler,
    WrapFocusHandler, WrapLifeSpanHandler, WrapLoadHandler, WrapPermissionHandler,
    WrapRenderProcessHandler, WrapRequestHandler, WrapResourceHandler, WrapResourceRequestHandler,
    WrapTask, WrapV8Handler, ZoomCommand, post_task, stream_reader_create_for_file,
    string_multimap_alloc, string_multimap_append, wrap_app, wrap_browser_process_handler,
    wrap_client, wrap_context_menu_handler, wrap_display_handler, wrap_download_handler,
    wrap_download_image_callback, wrap_find_handler, wrap_focus_handler, wrap_life_span_handler,
    wrap_load_handler, wrap_permission_handler, wrap_render_process_handler, wrap_request_handler,
    wrap_resource_handler, wrap_resource_request_handler, wrap_task, wrap_v8_handler,
};
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
CDXC:CefRuntime 2026-07-04:
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
const CEF_CONTEXT_MENU_INSPECT_ELEMENT_COMMAND_ID: c_int = 26_001;
// Stable Chromium content-context commands used by the production macOS CEF
// host (cef_command_ids.h).
const CEF_CONTEXT_MENU_OPEN_LINK_NEW_TAB_COMMAND_ID: c_int = 50_100;
const CEF_CONTEXT_MENU_OPEN_LINK_NEW_WINDOW_COMMAND_ID: c_int = 50_101;
// App-owned Browser page commands, inside CEF's MENU_ID_USER_FIRST..LAST range.
const CEF_CONTEXT_MENU_APP_OPEN_LINK_NEW_TAB_COMMAND_ID: c_int = 26_501;
const CEF_CONTEXT_MENU_SAVE_LINK_AS_COMMAND_ID: c_int = 26_502;
const CEF_CONTEXT_MENU_COPY_LINK_ADDRESS_COMMAND_ID: c_int = 26_503;
const CEF_CONTEXT_MENU_OPEN_IMAGE_NEW_TAB_COMMAND_ID: c_int = 26_504;
const CEF_CONTEXT_MENU_SAVE_IMAGE_AS_COMMAND_ID: c_int = 26_505;
const CEF_CONTEXT_MENU_COPY_IMAGE_COMMAND_ID: c_int = 26_506;
const CEF_CONTEXT_MENU_COPY_IMAGE_ADDRESS_COMMAND_ID: c_int = 26_507;
const BROWSER_APP_OWNED_SCRIPT_URL: &str = "ghostex://gpui/browser-feedback";
thread_local! {
    static CEF_BROWSERS_BY_NATIVE_VIEW: RefCell<HashMap<usize, cef::Browser>> = RefCell::new(HashMap::new());
    static KEYBOARD_ZOOM_CEF_NATIVE_VIEWS: RefCell<HashSet<usize>> = RefCell::new(HashSet::new());
    static CEF_GLOBAL_REQUEST_CONTEXT: RefCell<Option<cef::RequestContext>> = const { RefCell::new(None) };
    static CEF_REQUEST_CONTEXTS_BY_PROFILE: RefCell<HashMap<String, cef::RequestContext>> = RefCell::new(HashMap::new());
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

// C4 light split: the modules below hold the bulk of what used to be
// this file's content; see docs/2026-08-22/repo-restructure/SPLITS.md C4
// for the cluster map. `pub(crate) use` re-exports keep every existing
// `shell::name` and `super::shell::name` path (cef/mod.rs's `pub use
// shell::*`, cef/macos.rs, cef/windows.rs, cef/linux_x11.rs) resolving
// unchanged.
mod browser;
mod browser_appearance;
mod browser_device_emulation;
mod browser_handlers;
mod browser_password_manager;
mod claude_cloud_page;
mod client;
mod devtools_relay;
mod lifecycle;
mod message_routing;
mod native_view;
mod page_bridge;
mod page_keep_awake;
mod page_screenshot;
mod remote_browser;
mod request_handling;
mod sign_in_popup;
mod site_permissions;
mod site_requests;
mod v8_bridges;
mod workspace_browser;

pub(crate) use browser::*;
pub(crate) use browser_appearance::*;
pub(crate) use browser_device_emulation::*;
pub(crate) use browser_handlers::*;
pub(crate) use browser_password_manager::*;
pub(crate) use claude_cloud_page::*;
pub(crate) use client::*;
pub(crate) use devtools_relay::*;
pub(crate) use lifecycle::*;
pub(crate) use message_routing::*;
pub(crate) use native_view::*;
pub(crate) use page_bridge::*;
pub(crate) use page_keep_awake::*;
pub(crate) use page_screenshot::*;
pub(crate) use remote_browser::*;
pub(crate) use request_handling::*;
pub(crate) use sign_in_popup::*;
pub(crate) use site_permissions::*;
pub(crate) use site_requests::*;
pub(crate) use v8_bridges::*;
pub(crate) use workspace_browser::*;
