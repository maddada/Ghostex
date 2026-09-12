#![cfg_attr(target_os = "windows", windows_subsystem = "windows")]

use cef::rc::Rc as _;
use cef::{
    App, CefString, Frame, ImplApp, ImplBrowser as _, ImplDomnode as _, ImplFrame as _,
    ImplListValue as _, ImplProcessMessage as _, ImplRenderProcessHandler, ImplV8Context as _,
    ImplV8Handler, ImplV8Value as _, ProcessId, RenderProcessHandler, V8Handler,
    V8Propertyattribute, V8Value, ValueType, WrapApp, WrapRenderProcessHandler, WrapV8Handler,
    wrap_app, wrap_render_process_handler, wrap_v8_handler,
};
#[path = "ghostex_gpui_cef_helper/v8.rs"]
mod v8;
use v8::*;
#[path = "../cef/sidebar_bridge_manifest.rs"]
mod sidebar_bridge_manifest;
use sidebar_bridge_manifest::{
    APP_MODAL_HOST_BRIDGE_PAYLOAD_MAX_CHARS, APP_MODAL_HOST_BRIDGE_PROCESS_MESSAGE_NAME,
    APP_MODAL_HOST_BRIDGE_SURFACE_SPECS, APP_MODAL_HOST_ID_JS_FIELD, APP_MODAL_HOST_ID_VALUE,
    APP_MODAL_HOST_SURFACE_JS_FIELD, APP_MODAL_HOST_SURFACE_VALUE, AppModalHostBridgeSurface,
    EXTENSION_BRIDGE_INSTALL_MESSAGE_NAME, EXTENSION_BRIDGE_PAYLOAD_MAX_CHARS,
    EXTENSION_BRIDGE_PROCESS_MESSAGE_NAME, EXTENSION_BRIDGE_RUNTIME_SHIM,
    NATIVE_HOST_BRIDGE_PROCESS_MESSAGE_NAME, PROJECT_WORKAREA_BRIDGE_FUNCTION_SPECS,
    PROJECT_WORKAREA_BRIDGE_INSTALL_MESSAGE_NAME, PROJECT_WORKAREA_BRIDGE_PAYLOAD_MAX_CHARS,
    PROJECT_WORKAREA_MANAGE_DOCS_RESOURCE_BASE_URL,
    PROJECT_WORKAREA_MANAGE_DOCS_RESOURCE_BASE_URL_JS_FIELD, SIDEBAR_BRIDGE_FUNCTION_SPECS,
    SIDEBAR_BRIDGE_PAYLOAD_MAX_CHARS, SIDEBAR_EDITABLE_FOCUS_PROCESS_MESSAGE_NAME,
    SIDEBAR_PROJECT_CONTEXT_JS_NAMESPACE, SidebarBridgeFunctionId,
    WEBKIT_APP_MODAL_HOST_MESSAGE_HANDLER_JS_OBJECT,
    WEBKIT_EXTENSION_HOST_MESSAGE_HANDLER_JS_OBJECT, WEBKIT_JS_OBJECT,
    WEBKIT_MESSAGE_HANDLERS_JS_OBJECT, WEBKIT_NATIVE_HOST_MESSAGE_HANDLER_JS_OBJECT,
    WEBKIT_POST_MESSAGE_JS_FUNCTION, project_workarea_bridge_function_spec_for_js_function,
    project_workarea_bridge_function_spec_for_process_message,
    sidebar_bridge_function_spec_for_js_function, sidebar_bridge_function_spec_for_process_message,
};
use std::{cell::RefCell, collections::HashMap, os::raw::c_int};
#[cfg(target_os = "macos")]
use std::{
    ffi::CString,
    os::unix::ffi::OsStrExt as _,
    path::{Path, PathBuf},
};

#[cfg(target_os = "macos")]
const CEF_FRAMEWORK_EXECUTABLE_ENV: &str = "GHOSTEX_CEF_FRAMEWORK_EXECUTABLE";
#[cfg(any(target_os = "windows", target_os = "linux"))]
const CEF_RUNTIME_DIR_ENV: &str = "GHOSTEX_CEF_DIR";
#[cfg(target_os = "macos")]
const CEF_FRAMEWORK_EXECUTABLE_RELATIVE_PATH: &str =
    "Chromium Embedded Framework.framework/Chromium Embedded Framework";

const SIDEBAR_PROJECT_CONTEXT_INSTALL_MESSAGE_NAME: &str =
    "ghostex.gpui.sidebar.installActiveProjectContextBridge";
const SIDEBAR_RUNTIME_SETTINGS_UPDATE_MESSAGE_NAME: &str =
    "ghostex.gpui.sidebar.runtimeSettingsChanged";
const SIDEBAR_GXSERVER_BOOTSTRAP_UPDATE_MESSAGE_NAME: &str =
    "ghostex.gpui.sidebar.gxserverBootstrapChanged";
/*
CDXC:SessionChat 2026-07-31:
Session Chat surfaces receive only the gxserver bootstrap through this
dedicated message; unlike the sidebar bootstrap-update path it must not
require the installed sidebar post-function bridge, because chat.html never
gets one. Keep in sync with the macOS renderer bridge in cef/shell.rs.
*/
const SESSION_CHAT_ACTIVATE_MESSAGE_NAME: &str = "ghostex.gpui.sessionChat.activate";
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

#[derive(Clone, Debug, Default, PartialEq, Eq)]
struct SidebarRuntimeSettingsSnapshot {
    debugging_mode: bool,
    show_beta_features: bool,
    saved_settings_json: String,
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct SidebarGxserverBootstrap {
    base_url: String,
    auth_token: String,
    protocol_version: i32,
    client_id: String,
    initial_active_project_id: Option<String>,
    focused_session_id: Option<String>,
    visible_session_ids: Vec<String>,
}

/*
CDXC:ContextMenus 2026-09-11 WHY:
The sidebar focus grant is one merged state, not two independent signals. Editable focus comes from Blink's focused-node callback; "a context menu is open" comes from the page through the fixed editableFocus bridge function, forwarded by the desktop runtime from the portal's opened/closed notifications.
An earlier version marked the menu element and inferred the grant from which DOM node was focused. That released the grant whenever a focused menu item was unmounted (a submenu switch) or focus landed in a sibling flyout portal (Copy Details, Tag as, Switch Account), and it released-then-regranted when one menu closed as another opened, which the page saw as a window blur that dismissed the new menu.
Merging both inputs here means the grant only changes when the combined answer changes, so DOM focus can move anywhere inside the sidebar while a menu is open.
SEE-ALSO: packages/core-ui/sidebar-context-menu-portal.tsx, apps/desktop/sidebar/gxserver-runtime/sessions-and-focus.ts, apps/desktop/src/cef/shell/native_view.rs.
*/
#[derive(Clone, Copy, Default)]
struct SidebarFocusState {
    context_menu_open: bool,
    editable_focused: bool,
    granted: bool,
}

thread_local! {
    static SIDEBAR_FOCUS_STATE_BY_BROWSER_ID: RefCell<HashMap<c_int, SidebarFocusState>> = RefCell::new(HashMap::new());
}

fn update_sidebar_focus_grant(
    browser: &mut cef::Browser,
    update: impl FnOnce(&mut SidebarFocusState),
) {
    let browser_id = browser.identifier();
    let transition = SIDEBAR_FOCUS_STATE_BY_BROWSER_ID.with(|states| {
        let mut states = states.borrow_mut();
        let state = states.entry(browser_id).or_default();
        update(state);
        let granted = state.editable_focused || state.context_menu_open;
        if state.granted == granted {
            return None;
        }
        state.granted = granted;
        Some(granted)
    });
    let Some(granted) = transition else {
        return;
    };
    let Some(main_frame) = browser.main_frame() else {
        return;
    };
    let mut message = match cef::process_message_create(Some(&CefString::from(
        SIDEBAR_EDITABLE_FOCUS_PROCESS_MESSAGE_NAME,
    ))) {
        Some(message) => message,
        None => return,
    };
    let Some(arguments) = message.argument_list() else {
        return;
    };
    arguments.set_size(1);
    arguments.set_string(
        0,
        Some(&CefString::from(if granted {
            "focused"
        } else {
            "blurred"
        })),
    );
    main_frame.send_process_message(ProcessId::BROWSER, Some(&mut message));
}

fn main() {
    let args = cef::args::Args::new();

    #[cfg(target_os = "macos")]
    let _loader = load_macos_cef_framework();
    #[cfg(any(target_os = "windows", target_os = "linux"))]
    verify_external_cef_runtime_dir();

    let _ = cef::api_hash(cef::sys::CEF_API_VERSION_LAST, 0);
    let mut app = GhostexGpuiCefHelperApp::new();
    cef::execute_process(
        Some(args.as_main_args()),
        Some(&mut app),
        std::ptr::null_mut(),
    );
}

#[cfg(any(target_os = "windows", target_os = "linux"))]
fn verify_external_cef_runtime_dir() {
    let runtime_dir = std::env::var_os(CEF_RUNTIME_DIR_ENV)
        .map(std::path::PathBuf::from)
        .expect("verified helper CEF runtime directory is not configured");
    #[cfg(target_os = "windows")]
    let library = runtime_dir.join("libcef.dll");
    #[cfg(target_os = "linux")]
    let library = runtime_dir.join("libcef.so");
    assert!(
        library.is_file(),
        "verified helper CEF runtime is missing {}",
        library.display()
    );
}

#[cfg(target_os = "macos")]
struct MacosCefFrameworkLoader {
    path: PathBuf,
}

#[cfg(target_os = "macos")]
impl Drop for MacosCefFrameworkLoader {
    fn drop(&mut self) {
        if cef::unload_library() != 1 {
            eprintln!(
                "could not unload helper CEF framework {}",
                self.path.display()
            );
        }
    }
}

#[cfg(target_os = "macos")]
fn load_macos_cef_framework() -> MacosCefFrameworkLoader {
    let executable = std::env::current_exe().expect("failed to resolve helper executable path");
    let path = resolve_macos_cef_framework_executable(&executable)
        .expect("failed to resolve helper CEF framework path");
    let c_path = CString::new(path.as_os_str().as_bytes())
        .expect("helper CEF framework path contains an embedded NUL byte");
    assert_eq!(
        unsafe { cef::load_library(Some(&*c_path.as_ptr().cast())) },
        1,
        "failed to load CEF framework for helper from {}",
        path.display()
    );
    MacosCefFrameworkLoader { path }
}

#[cfg(target_os = "macos")]
fn resolve_macos_cef_framework_executable(executable: &Path) -> Result<PathBuf, String> {
    let executable_dir = executable
        .parent()
        .ok_or_else(|| "helper executable has no parent directory".to_string())?;
    let bundled = executable_dir
        .join("../../..")
        .join(CEF_FRAMEWORK_EXECUTABLE_RELATIVE_PATH);
    if bundled.is_file() {
        return bundled
            .canonicalize()
            .map_err(|error| format!("could not resolve bundled helper CEF: {error}"));
    }
    let configured = std::env::var_os(CEF_FRAMEWORK_EXECUTABLE_ENV)
        .filter(|value| !value.is_empty())
        .map(PathBuf::from)
        .ok_or_else(|| {
            "verified external helper CEF framework path is not configured".to_string()
        })?;
    if !configured.is_file() {
        return Err(format!(
            "verified external helper CEF framework is missing at {}",
            configured.display()
        ));
    }
    configured
        .canonicalize()
        .map_err(|error| format!("could not resolve external helper CEF: {error}"))
}

wrap_app! {
    struct GhostexGpuiCefHelperApp;

    impl App {
        fn render_process_handler(&self) -> Option<RenderProcessHandler> {
            Some(GhostexGpuiRenderProcessHandler::new())
        }
    }
}

wrap_render_process_handler! {
    struct GhostexGpuiRenderProcessHandler;

    impl RenderProcessHandler {
        fn on_browser_destroyed(&self, browser: Option<&mut cef::Browser>) {
            if let Some(browser_id) = browser.as_ref().map(|browser| browser.identifier()) {
                SIDEBAR_FOCUS_STATE_BY_BROWSER_ID
                    .with(|states| states.borrow_mut().remove(&browser_id));
            }
        }

        fn on_focused_node_changed(
            &self,
            browser: Option<&mut cef::Browser>,
            _frame: Option<&mut Frame>,
            node: Option<&mut cef::Domnode>,
        ) {
            /*
            CDXC:FocusRouting 2026-07-22:
            The sidebar surface is mouse-focus passive, so its document is
            usually NOT natively focused. Chromium defers DOM focus/blur
            events in unfocused documents (element.focus() moves activeElement
            silently), which made a page-side focusin watcher blind exactly
            when the grant was needed — a rename input mounted with autoFocus
            could never request the keyboard. Blink's focused-node callback
            fires on every activeElement change regardless of native focus
            state, so the renderer is the one reliable authority: report
            editable-focus transitions for the sidebar surface only, as the
            same fixed bounded editableFocus message the browser process
            already consumes as an app-owned focus grant/release.
            */
            let Some(browser) = browser else {
                return;
            };
            let is_sidebar = browser.main_frame().is_some_and(|frame| {
                let frame_url = CefString::from(&frame.url()).to_string();
                app_modal_host_bridge_surface_for_frame_url(&frame_url)
                    == Some(AppModalHostBridgeSurface::Sidebar)
            });
            if !is_sidebar {
                return;
            }
            let editable_focused = node.is_some_and(|node| node.is_editable() != 0);
            update_sidebar_focus_grant(browser, |state| state.editable_focused = editable_focused);
        }

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
            CDXC:CefRuntime 2026-06-29-14:45:
            Helper-backed renderers must install the same surface-scoped app-modal shim as the macOS CEF path by reading allowed surfaces from the shared Rust manifest. Browser tabs and project workareas still do not receive this bridge.
            */
            install_app_modal_host_v8_bridge(Some(&mut *context), surface);
        }

        fn on_process_message_received(
            &self,
            _browser: Option<&mut cef::Browser>,
            frame: Option<&mut Frame>,
            source_process: ProcessId,
            message: Option<&mut cef::ProcessMessage>,
        ) -> std::os::raw::c_int {
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
            let is_session_chat_activation_message = message_name == SESSION_CHAT_ACTIVATE_MESSAGE_NAME;
            let is_session_chat_gxserver_bootstrap_message =
                message_name == SESSION_CHAT_GXSERVER_BOOTSTRAP_MESSAGE_NAME;
            let is_project_workarea_install_message =
                message_name == PROJECT_WORKAREA_BRIDGE_INSTALL_MESSAGE_NAME;
            let is_extension_bridge_install_message =
                message_name == EXTENSION_BRIDGE_INSTALL_MESSAGE_NAME;
            if !is_install_message
                && !is_runtime_settings_update
                && !is_gxserver_bootstrap_update
                && !is_session_chat_gxserver_bootstrap_message
                && !is_session_chat_activation_message
                && !is_project_workarea_install_message
                && !is_extension_bridge_install_message
            {
                return 0;
            }
            let Some(frame) = frame else {
                return 1;
            };
            if frame.is_main() == 0 {
                return 1;
            }
            if (is_session_chat_activation_message || is_session_chat_gxserver_bootstrap_message)
                && app_modal_host_bridge_surface_for_frame_url(&CefString::from(&frame.url()).to_string()).is_none()
            {
                return 1;
            }
            let Some(mut context) = frame.v8_context() else {
                return 1;
            };
            if context.enter() == 0 {
                return 1;
            }
            if is_extension_bridge_install_message {
                install_extension_v8_bridge(Some(&mut context));
            } else if is_project_workarea_install_message {
                /*
                CDXC:Docs 2026-08-07:
                Mirror cef/shell.rs exactly: the install message optionally
                carries the Manage Docs resource base URL, and the helper must
                install it too or Docs HTML documents lose their <base> and
                every relative image/stylesheet/script request breaks.
                */
                let manage_docs_resource_base_url = message
                    .argument_list()
                    .filter(|arguments| {
                        arguments.size() == 1 && arguments.get_type(0) == ValueType::STRING
                    })
                    .map(|arguments| CefString::from(&arguments.string(0)).to_string())
                    .filter(|value| value == PROJECT_WORKAREA_MANAGE_DOCS_RESOURCE_BASE_URL);
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
            } else if is_session_chat_activation_message {
                let activation = message.argument_list()
                    .filter(|arguments| arguments.size() == 1 && arguments.get_type(0) == ValueType::STRING)
                    .map(|arguments| CefString::from(&arguments.string(0)).to_string());
                if let Some(activation) = activation {
                    install_session_chat_activation_v8_bridge(&mut context, &activation);
                }
            } else if is_session_chat_gxserver_bootstrap_message {
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
            if is_extension_bridge_install_message {
                frame.execute_java_script(
                    Some(&CefString::from(EXTENSION_BRIDGE_RUNTIME_SHIM)),
                    Some(&CefString::from("ghostex://gpui/extension-bridge")),
                    1,
                );
            }
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
        ) -> std::os::raw::c_int {
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
        ) -> std::os::raw::c_int {
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
    struct GhostexGpuiExtensionBridgeV8Handler;

    impl V8Handler {
        fn execute(
            &self,
            name: Option<&CefString>,
            _object: Option<&mut V8Value>,
            arguments: Option<&[Option<V8Value>]>,
            retval: Option<&mut Option<V8Value>>,
            _exception: Option<&mut CefString>,
        ) -> std::os::raw::c_int {
            let name = name.map(CefString::to_string);
            if name.as_deref() != Some(WEBKIT_POST_MESSAGE_JS_FUNCTION) {
                return 0;
            }
            let payload = arguments
                .and_then(|arguments| arguments.first())
                .and_then(Option::as_ref)
                .filter(|argument| argument.is_string() != 0)
                .map(|argument| CefString::from(&argument.string_value()).to_string());
            let Some(payload) = payload else {
                set_v8_bool_return(retval, false);
                return 1;
            };
            let sent = send_extension_bridge_process_message(&payload);
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
        ) -> std::os::raw::c_int {
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
        ) -> std::os::raw::c_int {
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

            if spec.id == SidebarBridgeFunctionId::SidebarEditableFocus {
                // The page declares "a sidebar context menu is open" here. It is merged with
                // Blink's editable-focus state in update_sidebar_focus_grant rather than
                // forwarded as-is, so the two inputs can never send contradictory grants.
                let context_menu_open = match payload.as_str() {
                    "focused" => true,
                    "blurred" => false,
                    _ => {
                        set_v8_bool_return(retval, false);
                        return 1;
                    }
                };
                let handled = cef::v8_context_get_current_context()
                    .and_then(|context| context.browser())
                    .map(|mut browser| {
                        update_sidebar_focus_grant(&mut browser, |state| {
                            state.context_menu_open = context_menu_open;
                        });
                    })
                    .is_some();
                set_v8_bool_return(retval, handled);
                return 1;
            }

            let sent = send_sidebar_bridge_process_message(spec.process_message_name, &payload);
            set_v8_bool_return(retval, sent);
            1
        }
    }
}

fn is_gpui_first_party_cef_entry_url(url: &str, entry_file_name: &str) -> bool {
    let Some(base) = url.split(['?', '#']).next() else {
        return false;
    };
    base.starts_with("file://")
        && base.ends_with(&format!("/{entry_file_name}"))
        && (base.contains("/dist/sidebar/")
            || if cfg!(target_os = "macos") {
                base.contains("/Contents/Resources/sidebar/")
            } else {
                base.contains("/resources/sidebar/")
            })
}

fn app_modal_host_bridge_surface_for_frame_url(url: &str) -> Option<AppModalHostBridgeSurface> {
    APP_MODAL_HOST_BRIDGE_SURFACE_SPECS
        .iter()
        .find(|spec| is_gpui_first_party_cef_entry_url(url, spec.entry_file_name))
        .map(|spec| spec.surface)
}

fn v8_object_property_or_new(parent: &V8Value, key: &str) -> Option<V8Value> {
    let key = CefString::from(key);
    parent
        .value_bykey(Some(&key))
        .filter(|value| value.is_object() != 0)
        .or_else(|| cef::v8_value_create_object(None, None))
}

fn app_modal_host_payload_from_v8_value(value: &V8Value) -> Option<String> {
    if value.is_string() != 0 {
        return Some(CefString::from(&value.string_value()).to_string());
    }

    let context = cef::v8_context_get_current_context()?;
    let global = context.global()?;
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

fn sidebar_runtime_settings_from_install_message(
    message: &mut cef::ProcessMessage,
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

fn sidebar_gxserver_bootstrap_from_process_message(
    message: &mut cef::ProcessMessage,
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

fn set_v8_string_property(parent: &V8Value, key: &str, value: &str) -> bool {
    let key = CefString::from(key);
    let value = CefString::from(value);
    let Some(mut value) = cef::v8_value_create_string(Some(&value)) else {
        return false;
    };
    parent.set_value_bykey(Some(&key), Some(&mut value), V8Propertyattribute::default()) != 0
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

fn send_sidebar_bridge_process_message(process_message_name: &str, payload: &str) -> bool {
    if sidebar_bridge_function_spec_for_process_message(process_message_name).is_none() {
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
    if project_workarea_bridge_function_spec_for_process_message(process_message_name).is_none() {
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

fn send_extension_bridge_process_message(payload: &str) -> bool {
    if payload.chars().count() > EXTENSION_BRIDGE_PAYLOAD_MAX_CHARS {
        return false;
    }
    let Some(context) = cef::v8_context_get_current_context() else {
        return false;
    };
    let Some(frame) = context.frame() else {
        return false;
    };
    let mut message = match cef::process_message_create(Some(&CefString::from(
        EXTENSION_BRIDGE_PROCESS_MESSAGE_NAME,
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
        *retval = cef::v8_value_create_bool(bool_to_cef_int(value));
    }
}

fn bool_to_cef_int(value: bool) -> c_int {
    if value { 1 } else { 0 }
}
