// C4 light split: bridge/popup event taxonomy, dispatch-policy
// classification, the V8Handler impls, and the sidebar/project-workarea/
// native-host JS bridge install, update, send,
// and (de)serialization plumbing. Pure move out of `cef/shell.rs`; the only
// edit is the `pub(crate) ` prefix moved items need to stay callable from
// their siblings and from `shell` itself. See
// docs/2026-08-22/repo-restructure/SPLITS.md C4.
use super::*;

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
    CDXC:Browser 2026-06-23-12:48:
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
CDXC:Browser 2026-08-18:
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
pub enum ProjectWorkareaBridgeEvent {
    ProjectBeadsRequest(String),
    ProjectBoardRequest(String),
    ProjectBoardImageRequest(String),
    ManageFilesRequest(String),
    /// The page tried to navigate its own main frame somewhere else; the payload is the refused URL.
    RefusedPageNavigation(String),
}

pub type ProjectWorkareaBridgeEventHandler = StdRc<dyn Fn(ProjectWorkareaBridgeEvent)>;

/// Plain Rust shared with the native app, which names no CEF type; see
/// `app/helpers/web_bridge_types.rs`.
pub use crate::app::helpers::web_bridge_types::{
    BrowserPageContextMenu, BrowserPageContextMenuEntry, PageLoadEndHandler,
};

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ExtensionBridgeEvent {
    pub extension_id: String,
    pub payload: String,
}

pub type ExtensionBridgeEventHandler = StdRc<dyn Fn(ExtensionBridgeEvent)>;

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

pub enum BrowserPageMetadataEvent {
    HistoryRequested,
    FindRequested,
    /// The Annotate hotkey, pressed while the page had the keyboard.
    AnnotateRequested,
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
    /// A page context-menu copy (link/image address, or the image itself).
    CopyToClipboard(gpui::ClipboardItem),
    /// A right-click on the page, shown in Ghostex's own menu instead of CEF's.
    ContextMenuRequested(BrowserPageContextMenu),
}

pub type BrowserPageMetadataHandler = StdRc<dyn Fn(BrowserPageMetadataEvent)>;

/*
CDXC:Browser 2026-07-27:
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
    pub(crate) struct GhostexGpuiExtensionBridgeV8Handler;

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

pub(crate) fn send_project_workarea_bridge_process_message(
    process_message_name: &str,
    payload: &str,
) -> bool {
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

pub(crate) fn send_extension_bridge_process_message(payload: &str) -> bool {
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

pub(crate) fn set_v8_bool_return(retval: Option<&mut Option<V8Value>>, value: bool) {
    if let Some(retval) = retval {
        *retval = cef::v8_value_create_bool(if value { 1 } else { 0 });
    }
}
