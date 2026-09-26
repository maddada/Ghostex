#![cfg_attr(target_os = "windows", windows_subsystem = "windows")]

use cef::rc::Rc as _;
use cef::{
    App, CefString, Frame, ImplApp, ImplFrame as _, ImplListValue as _, ImplProcessMessage as _,
    ImplRenderProcessHandler, ImplV8Context as _, ImplV8Handler, ImplV8Value as _, ProcessId,
    RenderProcessHandler, V8Handler, V8Propertyattribute, V8Value, ValueType, WrapApp,
    WrapRenderProcessHandler, WrapV8Handler, wrap_app, wrap_render_process_handler,
    wrap_v8_handler,
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
    PROJECT_WORKAREA_MANAGE_DOCS_RESOURCE_BASE_URL_JS_FIELD, SIDEBAR_PROJECT_CONTEXT_JS_NAMESPACE,
    WEBKIT_APP_MODAL_HOST_MESSAGE_HANDLER_JS_OBJECT,
    WEBKIT_EXTENSION_HOST_MESSAGE_HANDLER_JS_OBJECT, WEBKIT_JS_OBJECT,
    WEBKIT_MESSAGE_HANDLERS_JS_OBJECT, WEBKIT_NATIVE_HOST_MESSAGE_HANDLER_JS_OBJECT,
    WEBKIT_POST_MESSAGE_JS_FUNCTION, project_workarea_bridge_function_spec_for_js_function,
    project_workarea_bridge_function_spec_for_process_message,
};
use std::os::raw::c_int;
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

/*
CDXC:SessionChat 2026-09-21 WHY:
Bootstrap-only pages receive only the gxserver bootstrap through this
dedicated message. Keep in sync with the macOS renderer bridge in cef/shell.rs.
*/
const SESSION_CHAT_GXSERVER_BOOTSTRAP_MESSAGE_NAME: &str =
    "ghostex.gpui.sessionChat.gxserverBootstrap";
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
const SIDEBAR_GXSERVER_BOOTSTRAP_PRESENT_ARGUMENT_INDEX: usize = 0;
const SIDEBAR_GXSERVER_BOOTSTRAP_BASE_URL_ARGUMENT_INDEX: usize = 1;
const SIDEBAR_GXSERVER_BOOTSTRAP_AUTH_TOKEN_ARGUMENT_INDEX: usize = 2;
const SIDEBAR_GXSERVER_BOOTSTRAP_PROTOCOL_VERSION_ARGUMENT_INDEX: usize = 3;
const SIDEBAR_GXSERVER_BOOTSTRAP_CLIENT_ID_ARGUMENT_INDEX: usize = 4;
const SIDEBAR_GXSERVER_BOOTSTRAP_INITIAL_ACTIVE_PROJECT_ID_ARGUMENT_INDEX: usize = 5;
const SIDEBAR_GXSERVER_BOOTSTRAP_FOCUSED_SESSION_ID_ARGUMENT_INDEX: usize = 6;
const SIDEBAR_GXSERVER_BOOTSTRAP_VISIBLE_SESSION_COUNT_ARGUMENT_INDEX: usize = 7;
const SIDEBAR_GXSERVER_BOOTSTRAP_ARGUMENT_COUNT_WITHOUT_VISIBLE_IDS: usize = 8;

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
            let is_session_chat_gxserver_bootstrap_message =
                message_name == SESSION_CHAT_GXSERVER_BOOTSTRAP_MESSAGE_NAME;
            let is_project_workarea_install_message =
                message_name == PROJECT_WORKAREA_BRIDGE_INSTALL_MESSAGE_NAME;
            let is_extension_bridge_install_message =
                message_name == EXTENSION_BRIDGE_INSTALL_MESSAGE_NAME;
            if !is_session_chat_gxserver_bootstrap_message
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
            if is_session_chat_gxserver_bootstrap_message
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
            } else {
                let gxserver_bootstrap = sidebar_gxserver_bootstrap_from_process_message(message, 0);
                install_session_chat_gxserver_bootstrap_v8_bridge(
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
