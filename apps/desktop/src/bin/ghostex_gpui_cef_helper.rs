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
    EXTENSION_BRIDGE_INSTALL_MESSAGE_NAME, EXTENSION_BRIDGE_PAYLOAD_MAX_CHARS,
    EXTENSION_BRIDGE_PROCESS_MESSAGE_NAME, EXTENSION_BRIDGE_RUNTIME_SHIM,
    PROJECT_WORKAREA_BRIDGE_FUNCTION_SPECS, PROJECT_WORKAREA_BRIDGE_INSTALL_MESSAGE_NAME,
    PROJECT_WORKAREA_BRIDGE_PAYLOAD_MAX_CHARS, PROJECT_WORKAREA_MANAGE_DOCS_RESOURCE_BASE_URL,
    PROJECT_WORKAREA_MANAGE_DOCS_RESOURCE_BASE_URL_JS_FIELD, SIDEBAR_PROJECT_CONTEXT_JS_NAMESPACE,
    WEBKIT_EXTENSION_HOST_MESSAGE_HANDLER_JS_OBJECT, WEBKIT_JS_OBJECT,
    WEBKIT_MESSAGE_HANDLERS_JS_OBJECT, WEBKIT_POST_MESSAGE_JS_FUNCTION,
    project_workarea_bridge_function_spec_for_js_function,
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

fn main() {
    let args = cef::args::Args::new();

    #[cfg(target_os = "macos")]
    let _loader = load_macos_cef_framework();
    #[cfg(any(target_os = "windows", target_os = "linux"))]
    verify_external_cef_runtime_dir();

    // A libcef from another CEF build refuses this API version, and `execute_process` would then
    // abort with SIGTRAP (CDXC:CefRuntime 2026-10-27 in cef/shell/lifecycle.rs).
    if cef::api_hash(cef::sys::CEF_API_VERSION_LAST, 0).is_null() {
        eprintln!(
            "ghostex-gpui-cef-helper: the loaded libcef does not support CEF {}, which this build needs",
            env!("GHOSTEX_CEF_VERSION")
        );
        std::process::exit(1);
    }
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
            let is_project_workarea_install_message =
                message_name == PROJECT_WORKAREA_BRIDGE_INSTALL_MESSAGE_NAME;
            let is_extension_bridge_install_message =
                message_name == EXTENSION_BRIDGE_INSTALL_MESSAGE_NAME;
            if !is_project_workarea_install_message
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
            let Some(mut context) = frame.v8_context() else {
                return 1;
            };
            if context.enter() == 0 {
                return 1;
            }
            if is_extension_bridge_install_message {
                install_extension_v8_bridge(Some(&mut context));
            } else {
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

fn set_v8_bool_return(retval: Option<&mut Option<V8Value>>, value: bool) {
    if let Some(retval) = retval {
        *retval = cef::v8_value_create_bool(bool_to_cef_int(value));
    }
}

fn bool_to_cef_int(value: bool) -> c_int {
    if value { 1 } else { 0 }
}
