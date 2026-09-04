#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum SidebarBridgeFunctionId {
    ActiveProjectContext,
    SourceWorkareaReadiness,
    BrowserWorkareaReadiness,
    ProjectWorkareaReadiness,
    ManageFileWorkareaOperationRequest,
    NativeProjectPathAction,
    NativeAppShotPrompt,
    SidebarCommandAction,
    SidebarCommandRunEnd,
    SidebarEditableFocus,
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
    ResourcesSnapshotRequest,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct SidebarBridgeFunctionSpec {
    #[allow(dead_code)]
    pub(crate) id: SidebarBridgeFunctionId,
    pub(crate) js_function_name: &'static str,
    pub(crate) process_message_name: &'static str,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum ProjectWorkareaBridgeFunctionId {
    ProjectBeadsRequest,
    ProjectBoardRequest,
    ProjectBoardImageRequest,
    ManageFilesRequest,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct ProjectWorkareaBridgeFunctionSpec {
    #[allow(dead_code)]
    pub(crate) id: ProjectWorkareaBridgeFunctionId,
    pub(crate) js_function_name: &'static str,
    pub(crate) process_message_name: &'static str,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum AppModalHostBridgeSurface {
    NativeWindow,
    Sidebar,
    Titlebar,
    SessionChat,
    FindPrompts,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct AppModalHostBridgeSurfaceSpec {
    pub(crate) surface: AppModalHostBridgeSurface,
    pub(crate) entry_file_name: &'static str,
    pub(crate) extra_info_value: &'static str,
    pub(crate) exposes_native_window_identity: bool,
}

const SIDEBAR_PROJECT_CONTEXT_PROCESS_MESSAGE_NAME: &str =
    "ghostex.gpui.sidebar.activeProjectContext";
const SIDEBAR_SOURCE_WORKAREA_READINESS_PROCESS_MESSAGE_NAME: &str =
    "ghostex.gpui.sidebar.sourceWorkareaReadiness";
const SIDEBAR_BROWSER_WORKAREA_READINESS_PROCESS_MESSAGE_NAME: &str =
    "ghostex.gpui.sidebar.browserWorkareaReadiness";
const SIDEBAR_PROJECT_WORKAREA_READINESS_PROCESS_MESSAGE_NAME: &str =
    "ghostex.gpui.sidebar.projectWorkareaReadiness";
const SIDEBAR_MANAGE_FILE_WORKAREA_OPERATION_REQUEST_PROCESS_MESSAGE_NAME: &str =
    "ghostex.gpui.sidebar.manageFileWorkareaOperationRequest";
const SIDEBAR_NATIVE_PROJECT_PATH_ACTION_PROCESS_MESSAGE_NAME: &str =
    "ghostex.gpui.sidebar.nativeProjectPathAction";
const SIDEBAR_NATIVE_APP_SHOT_PROMPT_PROCESS_MESSAGE_NAME: &str =
    "ghostex.gpui.sidebar.nativeAppShotPrompt";
const SIDEBAR_COMMAND_ACTION_PROCESS_MESSAGE_NAME: &str = "ghostex.gpui.sidebar.commandAction";
const SIDEBAR_COMMAND_RUN_END_PROCESS_MESSAGE_NAME: &str = "ghostex.gpui.sidebar.commandRunEnd";
pub(crate) const SIDEBAR_EDITABLE_FOCUS_PROCESS_MESSAGE_NAME: &str =
    "ghostex.gpui.sidebar.editableFocus";
const SIDEBAR_GHOSTEX_HOTKEY_ACTION_PROCESS_MESSAGE_NAME: &str =
    "ghostex.gpui.sidebar.ghostexHotkeyAction";
const SIDEBAR_GXSERVER_FOCUS_STATE_PROCESS_MESSAGE_NAME: &str =
    "ghostex.gpui.sidebar.gxserverPresentationFocusState";
const SIDEBAR_CREATE_PROJECT_AGENT_PROCESS_MESSAGE_NAME: &str =
    "ghostex.gpui.sidebar.createProjectAgent";
const SIDEBAR_CREATE_PROJECT_TERMINAL_PROCESS_MESSAGE_NAME: &str =
    "ghostex.gpui.sidebar.createProjectTerminal";
const SIDEBAR_WORKSPACE_TERMINAL_FOCUS_PROCESS_MESSAGE_NAME: &str =
    "ghostex.gpui.sidebar.workspaceTerminalFocus";
const SIDEBAR_WORKSPACE_TERMINAL_RENAME_COMMAND_PROCESS_MESSAGE_NAME: &str =
    "ghostex.gpui.sidebar.workspaceTerminalRenameCommand";
const SIDEBAR_WORKSPACE_TERMINAL_ENTER_PROCESS_MESSAGE_NAME: &str =
    "ghostex.gpui.sidebar.workspaceTerminalEnter";
const SIDEBAR_WORKSPACE_TERMINAL_LIFECYCLE_RESULT_PROCESS_MESSAGE_NAME: &str =
    "ghostex.gpui.sidebar.workspaceTerminalLifecycleResult";
const SIDEBAR_SESSION_COMPLETION_SOUND_PROCESS_MESSAGE_NAME: &str =
    "ghostex.gpui.sidebar.sessionCompletionSound";
const SIDEBAR_SESSION_STATUS_INDICATORS_PROCESS_MESSAGE_NAME: &str =
    "ghostex.gpui.sidebar.sessionStatusIndicators";
const SIDEBAR_PET_OVERLAY_STATE_PROCESS_MESSAGE_NAME: &str = "ghostex.gpui.sidebar.petOverlayState";
const SIDEBAR_GLOBAL_ACTIONS_PROCESS_MESSAGE_NAME: &str = "ghostex.gpui.sidebar.globalActions";
const SIDEBAR_TITLEBAR_GIT_MENU_STATE_PROCESS_MESSAGE_NAME: &str =
    "ghostex.gpui.sidebar.titlebarGitMenuState";
const SIDEBAR_OPEN_BROWSER_URL_PROCESS_MESSAGE_NAME: &str = "ghostex.gpui.sidebar.openBrowserUrl";
const SIDEBAR_BROWSER_TAB_FOCUS_PROCESS_MESSAGE_NAME: &str = "ghostex.gpui.sidebar.browserTabFocus";
const SIDEBAR_PROJECT_BOARD_CONVERSATION_RESPONSE_PROCESS_MESSAGE_NAME: &str =
    "ghostex.gpui.sidebar.projectBoardConversationResponse";
const SIDEBAR_RESOURCES_SNAPSHOT_REQUEST_PROCESS_MESSAGE_NAME: &str =
    "ghostex.gpui.sidebar.resourcesSnapshotRequest";

pub(crate) const SIDEBAR_PROJECT_CONTEXT_JS_NAMESPACE: &str = "ghostexGpui";
const SIDEBAR_PROJECT_CONTEXT_JS_FUNCTION: &str = "postActiveProjectContext";
const SIDEBAR_SOURCE_WORKAREA_READINESS_JS_FUNCTION: &str = "postSourceWorkareaReadiness";
const SIDEBAR_BROWSER_WORKAREA_READINESS_JS_FUNCTION: &str = "postBrowserWorkareaReadiness";
const SIDEBAR_PROJECT_WORKAREA_READINESS_JS_FUNCTION: &str = "postProjectWorkareaReadiness";
const SIDEBAR_MANAGE_FILE_WORKAREA_OPERATION_REQUEST_JS_FUNCTION: &str =
    "postManageFileWorkareaOperationRequest";
const SIDEBAR_NATIVE_PROJECT_PATH_ACTION_JS_FUNCTION: &str = "postNativeProjectPathAction";
const SIDEBAR_NATIVE_APP_SHOT_PROMPT_JS_FUNCTION: &str = "postNativeAppShotPromptToSession";
const SIDEBAR_COMMAND_ACTION_JS_FUNCTION: &str = "postSidebarCommandAction";
const SIDEBAR_COMMAND_RUN_END_JS_FUNCTION: &str = "postSidebarCommandRunEnd";
const SIDEBAR_EDITABLE_FOCUS_JS_FUNCTION: &str = "postSidebarEditableFocus";
const SIDEBAR_GHOSTEX_HOTKEY_ACTION_JS_FUNCTION: &str = "postGhostexHotkeyAction";
const SIDEBAR_GXSERVER_FOCUS_STATE_JS_FUNCTION: &str = "postGxserverPresentationFocusState";
const SIDEBAR_CREATE_PROJECT_AGENT_JS_FUNCTION: &str = "postCreateProjectAgent";
const SIDEBAR_CREATE_PROJECT_TERMINAL_JS_FUNCTION: &str = "postCreateProjectTerminal";
const SIDEBAR_WORKSPACE_TERMINAL_FOCUS_JS_FUNCTION: &str = "postWorkspaceTerminalFocus";
const SIDEBAR_WORKSPACE_TERMINAL_RENAME_COMMAND_JS_FUNCTION: &str =
    "postWorkspaceTerminalRenameCommand";
const SIDEBAR_WORKSPACE_TERMINAL_ENTER_JS_FUNCTION: &str = "postWorkspaceTerminalEnter";
const SIDEBAR_WORKSPACE_TERMINAL_LIFECYCLE_RESULT_JS_FUNCTION: &str =
    "postWorkspaceTerminalLifecycleResult";
const SIDEBAR_SESSION_COMPLETION_SOUND_JS_FUNCTION: &str = "postSessionCompletionSound";
const SIDEBAR_SESSION_STATUS_INDICATORS_JS_FUNCTION: &str = "postSessionStatusIndicators";
const SIDEBAR_PET_OVERLAY_STATE_JS_FUNCTION: &str = "postPetOverlayState";
const SIDEBAR_GLOBAL_ACTIONS_JS_FUNCTION: &str = "postGlobalActions";
const SIDEBAR_TITLEBAR_GIT_MENU_STATE_JS_FUNCTION: &str = "postTitlebarGitMenuState";
const SIDEBAR_OPEN_BROWSER_URL_JS_FUNCTION: &str = "postOpenBrowserUrl";
const SIDEBAR_BROWSER_TAB_FOCUS_JS_FUNCTION: &str = "postBrowserTabFocus";
const SIDEBAR_PROJECT_BOARD_CONVERSATION_RESPONSE_JS_FUNCTION: &str =
    "postProjectBoardConversationResponse";
const SIDEBAR_RESOURCES_SNAPSHOT_REQUEST_JS_FUNCTION: &str = "postResourcesSnapshotRequest";

/*
Must fit the largest sidebar-bridge message: the project board conversation
response, which the app bounds at
GPUI_SIDEBAR_PROJECT_BOARD_CONVERSATION_PAYLOAD_MAX_CHARS (256 KiB). At the
previous 32 KiB, a shared board with ~45+ conversation links produced a
response the bridge silently dropped, so every card showed
"No linked conversation yet" while the links were stored correctly.
*/
pub(crate) const SIDEBAR_BRIDGE_PAYLOAD_MAX_CHARS: usize = 256 * 1024;
pub(crate) const PROJECT_WORKAREA_BRIDGE_INSTALL_MESSAGE_NAME: &str =
    "ghostex.gpui.projectWorkarea.installBridge";
const PROJECT_WORKAREA_PROJECT_BEADS_REQUEST_PROCESS_MESSAGE_NAME: &str =
    "ghostex.gpui.projectWorkarea.projectBeadsRequest";
const PROJECT_WORKAREA_PROJECT_BOARD_REQUEST_PROCESS_MESSAGE_NAME: &str =
    "ghostex.gpui.projectWorkarea.projectBoardRequest";
const PROJECT_WORKAREA_PROJECT_BOARD_IMAGE_REQUEST_PROCESS_MESSAGE_NAME: &str =
    "ghostex.gpui.projectWorkarea.projectBoardImageRequest";
const PROJECT_WORKAREA_MANAGE_FILES_REQUEST_PROCESS_MESSAGE_NAME: &str =
    "ghostex.gpui.projectWorkarea.manageFilesRequest";
const PROJECT_WORKAREA_PROJECT_BEADS_REQUEST_JS_FUNCTION: &str = "postProjectBeadsRequest";
const PROJECT_WORKAREA_PROJECT_BOARD_REQUEST_JS_FUNCTION: &str = "postProjectBoardRequest";
const PROJECT_WORKAREA_PROJECT_BOARD_IMAGE_REQUEST_JS_FUNCTION: &str =
    "postProjectBoardImageRequest";
const PROJECT_WORKAREA_MANAGE_FILES_REQUEST_JS_FUNCTION: &str = "postManageFilesRequest";
pub(crate) const PROJECT_WORKAREA_BRIDGE_PAYLOAD_MAX_CHARS: usize = 3 * 1024 * 1024;
/*
CDXC:Docs 2026-08-07:
The Manage Docs synthetic resource origin and its JS field live in this shared
manifest so the browser process (cef/shell.rs) and the helper renderer install
the same `manageDocsResourceBaseUrl`. When the helper drifted and dropped the
install-message argument, the field stayed unset and every relative image,
stylesheet, and script in the Docs HTML viewer resolved against the bundled
manage.html file URL instead of the docs folder.
*/
pub(crate) const PROJECT_WORKAREA_MANAGE_DOCS_RESOURCE_BASE_URL: &str =
    "https://ghostex-docs.invalid/";
pub(crate) const PROJECT_WORKAREA_MANAGE_DOCS_RESOURCE_BASE_URL_JS_FIELD: &str =
    "manageDocsResourceBaseUrl";
pub(crate) const APP_MODAL_HOST_BRIDGE_PROCESS_MESSAGE_NAME: &str =
    "ghostex.gpui.appModalHost.message";
pub(crate) const APP_MODAL_HOST_BRIDGE_PAYLOAD_MAX_CHARS: usize = 1024 * 1024;
pub(crate) const EXTENSION_BRIDGE_INSTALL_MESSAGE_NAME: &str =
    "ghostex.gpui.extension.installBridge";
pub(crate) const EXTENSION_BRIDGE_PROCESS_MESSAGE_NAME: &str = "ghostex.gpui.extension.message";
pub(crate) const EXTENSION_BRIDGE_PAYLOAD_MAX_CHARS: usize = 4 * 1024 * 1024;
pub(crate) const WEBKIT_EXTENSION_HOST_MESSAGE_HANDLER_JS_OBJECT: &str = "ghostexExtensionHost";
#[allow(dead_code)]
pub(crate) const NATIVE_HOST_BRIDGE_PROCESS_MESSAGE_NAME: &str = "ghostex.gpui.nativeHost.message";
#[allow(dead_code)]
pub(crate) const NATIVE_HOST_BRIDGE_PAYLOAD_MAX_CHARS: usize = 1024 * 1024;
#[allow(dead_code)] // protocol manifest: the extra_info key is part of the CEF bridge contract even where Rust does not read it (matches the neighbouring allows)
pub(crate) const APP_MODAL_HOST_BRIDGE_SURFACE_EXTRA_INFO_KEY: &str =
    "ghostexGpuiAppModalHostSurface";
const APP_MODAL_HOST_BRIDGE_SURFACE_NATIVE_WINDOW: &str = "nativeWindow";
const APP_MODAL_HOST_BRIDGE_SURFACE_SIDEBAR: &str = "sidebar";
const APP_MODAL_HOST_BRIDGE_SURFACE_TITLEBAR: &str = "titlebar";
const APP_MODAL_HOST_BRIDGE_SURFACE_SESSION_CHAT: &str = "sessionChat";
const APP_MODAL_HOST_BRIDGE_SURFACE_FIND_PROMPTS: &str = "findPrompts";
pub(crate) const APP_MODAL_HOST_SURFACE_JS_FIELD: &str = "__ghostex_APP_MODAL_HOST_SURFACE__";
pub(crate) const APP_MODAL_HOST_ID_JS_FIELD: &str = "__ghostex_APP_MODAL_HOST_ID__";
pub(crate) const APP_MODAL_HOST_SURFACE_VALUE: &str = "nativeWindow";
pub(crate) const APP_MODAL_HOST_ID_VALUE: &str = "gpui";
pub(crate) const WEBKIT_JS_OBJECT: &str = "webkit";
pub(crate) const WEBKIT_MESSAGE_HANDLERS_JS_OBJECT: &str = "messageHandlers";
pub(crate) const WEBKIT_APP_MODAL_HOST_MESSAGE_HANDLER_JS_OBJECT: &str = "ghostexAppModalHost";
#[allow(dead_code)]
pub(crate) const WEBKIT_NATIVE_HOST_MESSAGE_HANDLER_JS_OBJECT: &str = "ghostexNativeHost";
pub(crate) const WEBKIT_POST_MESSAGE_JS_FUNCTION: &str = "postMessage";

/*
CDXC:Extensions 2026-08-28:
An extension surface spec pins the one origin+path prefix a given extension's
CEF surface is allowed to be, and whether that surface gets the extension
bridge. `new` builds the first-party case: gxserver's static `/ext/{id}/`
bundle or the extension's own loopback command server, both of which are
extension-shipped code and do get the bridge. `new_remote` builds a
`server.url` extension's surface: a third-party HTTPS origin nobody in this
process serves or verifies, so `bridge_enabled` is false and the browser never
sends the install message, never installs `ghostexExtensionHost` or
`window.ghostex`, drops inbound bridge messages, and refuses to dispatch
outbound context to the page. The origin still pins the surface so a remote
page cannot navigate itself into a bridged origin and inherit the bridge.
*/
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ExtensionBridgeSurfaceSpec {
    pub id: String,
    pub origin: String,
    pub path_prefix: String,
    pub bridge_enabled: bool,
}

impl ExtensionBridgeSurfaceSpec {
    pub fn new(id: String, origin: String, path_prefix: String) -> Result<Self, String> {
        let id = validated_extension_bridge_id(id)?;
        let origin = origin.trim_end_matches('/').to_string();
        if !is_loopback_http_origin(&origin) {
            return Err("extension bridge origin must be loopback HTTP".to_string());
        }
        if !path_prefix.starts_with('/')
            || path_prefix.contains('?')
            || path_prefix.contains('#')
            || path_prefix.split('/').any(|segment| segment == "..")
        {
            return Err("extension bridge path prefix is invalid".to_string());
        }
        Ok(Self {
            id,
            origin,
            path_prefix,
            bridge_enabled: true,
        })
    }

    pub fn new_remote(id: String, origin: String) -> Result<Self, String> {
        let id = validated_extension_bridge_id(id)?;
        let origin = origin.trim_end_matches('/').to_string();
        if !is_remote_https_origin(&origin) {
            return Err("extension remote surface origin must be HTTPS".to_string());
        }
        Ok(Self {
            id,
            origin,
            // The user navigates inside the remote site, so every path under
            // the pinned origin belongs to this surface.
            path_prefix: "/".to_string(),
            bridge_enabled: false,
        })
    }

    pub fn matches_url(&self, url: &str) -> bool {
        let Some(base) = url.split(['?', '#']).next() else {
            return false;
        };
        // Scheme and host are case-insensitive, and CEF hands back frame URLs
        // with both lowercased regardless of how the manifest spelled them.
        let Some((candidate_origin, path)) = base.split_at_checked(self.origin.len()) else {
            return false;
        };
        if !candidate_origin.eq_ignore_ascii_case(&self.origin) {
            return false;
        }
        let path = if path.is_empty() { "/" } else { path };
        path.starts_with(&self.path_prefix)
            && (self.path_prefix.ends_with('/')
                || path.len() == self.path_prefix.len()
                || path.as_bytes().get(self.path_prefix.len()) == Some(&b'/'))
    }
}

fn validated_extension_bridge_id(id: String) -> Result<String, String> {
    if id.is_empty()
        || !id
            .bytes()
            .all(|byte| byte.is_ascii_lowercase() || byte.is_ascii_digit() || byte == b'-')
    {
        return Err("extension bridge id must be kebab-case".to_string());
    }
    Ok(id)
}

fn is_loopback_http_origin(origin: &str) -> bool {
    let Some(port) = origin.strip_prefix("http://127.0.0.1:") else {
        return false;
    };
    port.parse::<u16>().is_ok_and(|port| port != 0)
}

fn is_remote_https_origin(origin: &str) -> bool {
    let Some(authority) = origin.strip_prefix("https://") else {
        return false;
    };
    !authority.is_empty()
        && !authority.contains(['/', '?', '#', '@', '\\', ' ', '\t', '\n', '\r', '"', '\''])
}

pub(crate) const EXTENSION_BRIDGE_RUNTIME_SHIM: &str = r#"
(() => {
  if (window.ghostex && window.ghostex.__bridgeVersion === 1) return;
  const host = window.webkit?.messageHandlers?.ghostexExtensionHost;
  if (!host || typeof host.postMessage !== 'function') return;
  let sequence = 0;
  const pending = new Map();
  const contextListeners = new Set();
  const call = (method, params = {}, onChunk) => new Promise((resolve, reject) => {
    const requestId = `${Date.now().toString(36)}-${(++sequence).toString(36)}`;
    pending.set(requestId, { resolve, reject, onChunk });
    if (host.postMessage(JSON.stringify({ requestId, method, params })) === false) {
      pending.delete(requestId);
      reject(Object.assign(new Error('Ghostex rejected the extension call'), { code: 'operationFailed' }));
    }
  });
  Object.defineProperty(window, '__ghostexExtensionBridgeReceive', {
    configurable: false,
    enumerable: false,
    value(raw) {
      const message = typeof raw === 'string' ? JSON.parse(raw) : raw;
      if (message?.event === 'contextChanged') {
        for (const listener of contextListeners) listener(message.value);
        return;
      }
      const request = pending.get(message?.requestId);
      if (!request) return;
      if (message.chunk !== undefined) {
        request.onChunk?.(message.chunk);
        return;
      }
      pending.delete(message.requestId);
      if (message.ok) request.resolve(message.result);
      else request.reject(Object.assign(new Error(message.error?.message || 'Ghostex extension call failed'), message.error));
    },
  });
  const api = {
    __bridgeVersion: 1,
    context: () => call('context'),
    onContextChange(callback) {
      contextListeners.add(callback);
      return () => contextListeners.delete(callback);
    },
    cli: (verb, args = []) => call('cli', { verb, args }),
    exec(command, options = {}) {
      const { stream, ...params } = options;
      return call('exec', { command, ...params }, typeof stream === 'function' ? stream : undefined);
    },
    settings: {
      get: () => call('settings.get'),
      set: (values) => call('settings.set', { values }),
    },
    storage: {
      get: (key) => call('storage.get', { key }),
      set: (key, value) => call('storage.set', { key, value }),
    },
    ui: {
      toast: (message) => call('ui.toast', { message }),
      close: () => call('ui.close'),
      setBadge: (lines) => call('ui.setBadge', { lines }),
    },
  };
  Object.defineProperty(window, 'ghostex', { configurable: false, enumerable: true, value: Object.freeze(api) });
})();
"#;

pub(crate) fn extension_bridge_context_changed_message(
    context: serde_json::Value,
) -> serde_json::Value {
    serde_json::json!({
        "event": "contextChanged",
        "value": context,
    })
}

/*
CDXC:CefRuntime 2026-06-28-23:24:
The sidebar CEF post-function allowlist must have one Rust manifest shared by main-process macOS CEF and the helper renderer, so packaged helper-backed sidebars cannot lose supported calls such as workspace terminal rename.

CDXC:CefRuntime 2026-06-29-14:45:
GPUI CEF bridge names, payload budgets, and allowed app-modal/project-workarea surfaces live in this Rust manifest so the macOS browser process and helper renderer consume one ownership point. Keep sidebar, project-workarea, and app-modal handlers surface-specific; this manifest is an allowlist, not a generic IPC bus.
*/
pub(crate) const SIDEBAR_BRIDGE_FUNCTION_SPECS: [SidebarBridgeFunctionSpec; 27] = [
    SidebarBridgeFunctionSpec {
        id: SidebarBridgeFunctionId::ActiveProjectContext,
        js_function_name: SIDEBAR_PROJECT_CONTEXT_JS_FUNCTION,
        process_message_name: SIDEBAR_PROJECT_CONTEXT_PROCESS_MESSAGE_NAME,
    },
    SidebarBridgeFunctionSpec {
        id: SidebarBridgeFunctionId::SourceWorkareaReadiness,
        js_function_name: SIDEBAR_SOURCE_WORKAREA_READINESS_JS_FUNCTION,
        process_message_name: SIDEBAR_SOURCE_WORKAREA_READINESS_PROCESS_MESSAGE_NAME,
    },
    SidebarBridgeFunctionSpec {
        id: SidebarBridgeFunctionId::BrowserWorkareaReadiness,
        js_function_name: SIDEBAR_BROWSER_WORKAREA_READINESS_JS_FUNCTION,
        process_message_name: SIDEBAR_BROWSER_WORKAREA_READINESS_PROCESS_MESSAGE_NAME,
    },
    SidebarBridgeFunctionSpec {
        id: SidebarBridgeFunctionId::ProjectWorkareaReadiness,
        js_function_name: SIDEBAR_PROJECT_WORKAREA_READINESS_JS_FUNCTION,
        process_message_name: SIDEBAR_PROJECT_WORKAREA_READINESS_PROCESS_MESSAGE_NAME,
    },
    SidebarBridgeFunctionSpec {
        id: SidebarBridgeFunctionId::ManageFileWorkareaOperationRequest,
        js_function_name: SIDEBAR_MANAGE_FILE_WORKAREA_OPERATION_REQUEST_JS_FUNCTION,
        process_message_name: SIDEBAR_MANAGE_FILE_WORKAREA_OPERATION_REQUEST_PROCESS_MESSAGE_NAME,
    },
    SidebarBridgeFunctionSpec {
        id: SidebarBridgeFunctionId::NativeProjectPathAction,
        js_function_name: SIDEBAR_NATIVE_PROJECT_PATH_ACTION_JS_FUNCTION,
        process_message_name: SIDEBAR_NATIVE_PROJECT_PATH_ACTION_PROCESS_MESSAGE_NAME,
    },
    SidebarBridgeFunctionSpec {
        id: SidebarBridgeFunctionId::NativeAppShotPrompt,
        js_function_name: SIDEBAR_NATIVE_APP_SHOT_PROMPT_JS_FUNCTION,
        process_message_name: SIDEBAR_NATIVE_APP_SHOT_PROMPT_PROCESS_MESSAGE_NAME,
    },
    SidebarBridgeFunctionSpec {
        id: SidebarBridgeFunctionId::SidebarCommandAction,
        js_function_name: SIDEBAR_COMMAND_ACTION_JS_FUNCTION,
        process_message_name: SIDEBAR_COMMAND_ACTION_PROCESS_MESSAGE_NAME,
    },
    SidebarBridgeFunctionSpec {
        id: SidebarBridgeFunctionId::SidebarCommandRunEnd,
        js_function_name: SIDEBAR_COMMAND_RUN_END_JS_FUNCTION,
        process_message_name: SIDEBAR_COMMAND_RUN_END_PROCESS_MESSAGE_NAME,
    },
    SidebarBridgeFunctionSpec {
        id: SidebarBridgeFunctionId::SidebarEditableFocus,
        js_function_name: SIDEBAR_EDITABLE_FOCUS_JS_FUNCTION,
        process_message_name: SIDEBAR_EDITABLE_FOCUS_PROCESS_MESSAGE_NAME,
    },
    SidebarBridgeFunctionSpec {
        id: SidebarBridgeFunctionId::GhostexHotkeyAction,
        js_function_name: SIDEBAR_GHOSTEX_HOTKEY_ACTION_JS_FUNCTION,
        process_message_name: SIDEBAR_GHOSTEX_HOTKEY_ACTION_PROCESS_MESSAGE_NAME,
    },
    SidebarBridgeFunctionSpec {
        id: SidebarBridgeFunctionId::GxserverPresentationFocusState,
        js_function_name: SIDEBAR_GXSERVER_FOCUS_STATE_JS_FUNCTION,
        process_message_name: SIDEBAR_GXSERVER_FOCUS_STATE_PROCESS_MESSAGE_NAME,
    },
    SidebarBridgeFunctionSpec {
        id: SidebarBridgeFunctionId::CreateProjectAgent,
        js_function_name: SIDEBAR_CREATE_PROJECT_AGENT_JS_FUNCTION,
        process_message_name: SIDEBAR_CREATE_PROJECT_AGENT_PROCESS_MESSAGE_NAME,
    },
    SidebarBridgeFunctionSpec {
        id: SidebarBridgeFunctionId::CreateProjectTerminal,
        js_function_name: SIDEBAR_CREATE_PROJECT_TERMINAL_JS_FUNCTION,
        process_message_name: SIDEBAR_CREATE_PROJECT_TERMINAL_PROCESS_MESSAGE_NAME,
    },
    SidebarBridgeFunctionSpec {
        id: SidebarBridgeFunctionId::WorkspaceTerminalFocus,
        js_function_name: SIDEBAR_WORKSPACE_TERMINAL_FOCUS_JS_FUNCTION,
        process_message_name: SIDEBAR_WORKSPACE_TERMINAL_FOCUS_PROCESS_MESSAGE_NAME,
    },
    SidebarBridgeFunctionSpec {
        id: SidebarBridgeFunctionId::WorkspaceTerminalRenameCommand,
        js_function_name: SIDEBAR_WORKSPACE_TERMINAL_RENAME_COMMAND_JS_FUNCTION,
        process_message_name: SIDEBAR_WORKSPACE_TERMINAL_RENAME_COMMAND_PROCESS_MESSAGE_NAME,
    },
    SidebarBridgeFunctionSpec {
        id: SidebarBridgeFunctionId::WorkspaceTerminalEnter,
        js_function_name: SIDEBAR_WORKSPACE_TERMINAL_ENTER_JS_FUNCTION,
        process_message_name: SIDEBAR_WORKSPACE_TERMINAL_ENTER_PROCESS_MESSAGE_NAME,
    },
    SidebarBridgeFunctionSpec {
        id: SidebarBridgeFunctionId::WorkspaceTerminalLifecycleResult,
        js_function_name: SIDEBAR_WORKSPACE_TERMINAL_LIFECYCLE_RESULT_JS_FUNCTION,
        process_message_name: SIDEBAR_WORKSPACE_TERMINAL_LIFECYCLE_RESULT_PROCESS_MESSAGE_NAME,
    },
    SidebarBridgeFunctionSpec {
        id: SidebarBridgeFunctionId::SessionCompletionSound,
        js_function_name: SIDEBAR_SESSION_COMPLETION_SOUND_JS_FUNCTION,
        process_message_name: SIDEBAR_SESSION_COMPLETION_SOUND_PROCESS_MESSAGE_NAME,
    },
    SidebarBridgeFunctionSpec {
        id: SidebarBridgeFunctionId::SessionStatusIndicators,
        js_function_name: SIDEBAR_SESSION_STATUS_INDICATORS_JS_FUNCTION,
        process_message_name: SIDEBAR_SESSION_STATUS_INDICATORS_PROCESS_MESSAGE_NAME,
    },
    SidebarBridgeFunctionSpec {
        id: SidebarBridgeFunctionId::PetOverlayState,
        js_function_name: SIDEBAR_PET_OVERLAY_STATE_JS_FUNCTION,
        process_message_name: SIDEBAR_PET_OVERLAY_STATE_PROCESS_MESSAGE_NAME,
    },
    SidebarBridgeFunctionSpec {
        id: SidebarBridgeFunctionId::GlobalActions,
        js_function_name: SIDEBAR_GLOBAL_ACTIONS_JS_FUNCTION,
        process_message_name: SIDEBAR_GLOBAL_ACTIONS_PROCESS_MESSAGE_NAME,
    },
    SidebarBridgeFunctionSpec {
        id: SidebarBridgeFunctionId::TitlebarGitMenuState,
        js_function_name: SIDEBAR_TITLEBAR_GIT_MENU_STATE_JS_FUNCTION,
        process_message_name: SIDEBAR_TITLEBAR_GIT_MENU_STATE_PROCESS_MESSAGE_NAME,
    },
    SidebarBridgeFunctionSpec {
        id: SidebarBridgeFunctionId::OpenBrowserUrl,
        js_function_name: SIDEBAR_OPEN_BROWSER_URL_JS_FUNCTION,
        process_message_name: SIDEBAR_OPEN_BROWSER_URL_PROCESS_MESSAGE_NAME,
    },
    SidebarBridgeFunctionSpec {
        id: SidebarBridgeFunctionId::BrowserTabFocus,
        js_function_name: SIDEBAR_BROWSER_TAB_FOCUS_JS_FUNCTION,
        process_message_name: SIDEBAR_BROWSER_TAB_FOCUS_PROCESS_MESSAGE_NAME,
    },
    SidebarBridgeFunctionSpec {
        id: SidebarBridgeFunctionId::ProjectBoardConversationResponse,
        js_function_name: SIDEBAR_PROJECT_BOARD_CONVERSATION_RESPONSE_JS_FUNCTION,
        process_message_name: SIDEBAR_PROJECT_BOARD_CONVERSATION_RESPONSE_PROCESS_MESSAGE_NAME,
    },
    SidebarBridgeFunctionSpec {
        id: SidebarBridgeFunctionId::ResourcesSnapshotRequest,
        js_function_name: SIDEBAR_RESOURCES_SNAPSHOT_REQUEST_JS_FUNCTION,
        process_message_name: SIDEBAR_RESOURCES_SNAPSHOT_REQUEST_PROCESS_MESSAGE_NAME,
    },
];

pub(crate) const PROJECT_WORKAREA_BRIDGE_FUNCTION_SPECS: [ProjectWorkareaBridgeFunctionSpec; 4] = [
    ProjectWorkareaBridgeFunctionSpec {
        id: ProjectWorkareaBridgeFunctionId::ProjectBeadsRequest,
        js_function_name: PROJECT_WORKAREA_PROJECT_BEADS_REQUEST_JS_FUNCTION,
        process_message_name: PROJECT_WORKAREA_PROJECT_BEADS_REQUEST_PROCESS_MESSAGE_NAME,
    },
    ProjectWorkareaBridgeFunctionSpec {
        id: ProjectWorkareaBridgeFunctionId::ProjectBoardRequest,
        js_function_name: PROJECT_WORKAREA_PROJECT_BOARD_REQUEST_JS_FUNCTION,
        process_message_name: PROJECT_WORKAREA_PROJECT_BOARD_REQUEST_PROCESS_MESSAGE_NAME,
    },
    ProjectWorkareaBridgeFunctionSpec {
        id: ProjectWorkareaBridgeFunctionId::ProjectBoardImageRequest,
        js_function_name: PROJECT_WORKAREA_PROJECT_BOARD_IMAGE_REQUEST_JS_FUNCTION,
        process_message_name: PROJECT_WORKAREA_PROJECT_BOARD_IMAGE_REQUEST_PROCESS_MESSAGE_NAME,
    },
    ProjectWorkareaBridgeFunctionSpec {
        id: ProjectWorkareaBridgeFunctionId::ManageFilesRequest,
        js_function_name: PROJECT_WORKAREA_MANAGE_FILES_REQUEST_JS_FUNCTION,
        process_message_name: PROJECT_WORKAREA_MANAGE_FILES_REQUEST_PROCESS_MESSAGE_NAME,
    },
];

pub(crate) const APP_MODAL_HOST_BRIDGE_SURFACE_SPECS: [AppModalHostBridgeSurfaceSpec; 5] = [
    AppModalHostBridgeSurfaceSpec {
        surface: AppModalHostBridgeSurface::NativeWindow,
        entry_file_name: "modal-host.html",
        extra_info_value: APP_MODAL_HOST_BRIDGE_SURFACE_NATIVE_WINDOW,
        exposes_native_window_identity: true,
    },
    AppModalHostBridgeSurfaceSpec {
        surface: AppModalHostBridgeSurface::Sidebar,
        entry_file_name: "index.html",
        extra_info_value: APP_MODAL_HOST_BRIDGE_SURFACE_SIDEBAR,
        exposes_native_window_identity: false,
    },
    AppModalHostBridgeSurfaceSpec {
        surface: AppModalHostBridgeSurface::Titlebar,
        entry_file_name: "titlebar-host.html",
        extra_info_value: APP_MODAL_HOST_BRIDGE_SURFACE_TITLEBAR,
        exposes_native_window_identity: false,
    },
    /*
    CDXC:SessionChat 2026-07-31:
    chat.html is the first-party per-session Session Chat pane surface. It is
    registered here so the renderer installs the bounded ghostexAppModalHost
    shim for the bundled entry only; it never receives the native-window
    identity fields, and its gxserver bootstrap arrives through the dedicated
    session-chat bootstrap process message, not the sidebar install path.
    */
    AppModalHostBridgeSurfaceSpec {
        surface: AppModalHostBridgeSurface::SessionChat,
        entry_file_name: "chat.html",
        extra_info_value: APP_MODAL_HOST_BRIDGE_SURFACE_SESSION_CHAT,
        exposes_native_window_identity: false,
    },
    /*
    CDXC:PromptSearch 2026-08-20:
    find.html is the Search by Prompt child-window page — the GUI for `gx f`.
    The renderer installs the bounded ghostexAppModalHost shim for this bundled
    entry so the page can post its focus/launch/close requests to Rust, and
    nothing else. It never receives the React modal-host identity fields, and
    it reuses the narrow gxserver bootstrap process message, which installs
    only `window.ghostexGpui.gxserverBootstrap`.
    */
    AppModalHostBridgeSurfaceSpec {
        surface: AppModalHostBridgeSurface::FindPrompts,
        entry_file_name: "find.html",
        extra_info_value: APP_MODAL_HOST_BRIDGE_SURFACE_FIND_PROMPTS,
        exposes_native_window_identity: false,
    },
];

impl AppModalHostBridgeSurface {
    #[allow(dead_code)]
    pub(crate) fn extra_info_value(self) -> &'static str {
        app_modal_host_bridge_surface_spec(self).extra_info_value
    }

    pub(crate) fn exposes_native_window_identity(self) -> bool {
        app_modal_host_bridge_surface_spec(self).exposes_native_window_identity
    }

    #[allow(dead_code)] // protocol manifest: reverse lookup of the CEF extra_info contract, kept alongside the forward mapping
    pub(crate) fn from_extra_info_value(value: &str) -> Option<Self> {
        APP_MODAL_HOST_BRIDGE_SURFACE_SPECS
            .iter()
            .find(|spec| spec.extra_info_value == value)
            .map(|spec| spec.surface)
    }
}

pub(crate) fn app_modal_host_bridge_surface_spec(
    surface: AppModalHostBridgeSurface,
) -> &'static AppModalHostBridgeSurfaceSpec {
    APP_MODAL_HOST_BRIDGE_SURFACE_SPECS
        .iter()
        .find(|spec| spec.surface == surface)
        .expect("app-modal host surface must be listed in bridge manifest")
}

pub(crate) fn sidebar_bridge_function_spec_for_js_function(
    function_name: &str,
) -> Option<&'static SidebarBridgeFunctionSpec> {
    SIDEBAR_BRIDGE_FUNCTION_SPECS
        .iter()
        .find(|spec| spec.js_function_name == function_name)
}

pub(crate) fn sidebar_bridge_function_spec_for_process_message(
    process_message_name: &str,
) -> Option<&'static SidebarBridgeFunctionSpec> {
    SIDEBAR_BRIDGE_FUNCTION_SPECS
        .iter()
        .find(|spec| spec.process_message_name == process_message_name)
}

pub(crate) fn project_workarea_bridge_function_spec_for_js_function(
    function_name: &str,
) -> Option<&'static ProjectWorkareaBridgeFunctionSpec> {
    PROJECT_WORKAREA_BRIDGE_FUNCTION_SPECS
        .iter()
        .find(|spec| spec.js_function_name == function_name)
}

pub(crate) fn project_workarea_bridge_function_spec_for_process_message(
    process_message_name: &str,
) -> Option<&'static ProjectWorkareaBridgeFunctionSpec> {
    PROJECT_WORKAREA_BRIDGE_FUNCTION_SPECS
        .iter()
        .find(|spec| spec.process_message_name == process_message_name)
}
