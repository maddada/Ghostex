pub use super::sidebar_bridge_manifest::AppModalHostBridgeSurface;
use anyhow::Result;
use gpui::{Bounds, Pixels};
use std::path::PathBuf;
use std::rc::Rc;
use std::sync::Arc;

pub fn prepare_application() {}

pub fn initialize(_cx: &gpui::App) -> Result<()> {
    /*
    CDXC:CefRuntime 2026-06-14-12:06:
    GPUI is macOS-first today, but the app structure must make Linux and Windows CEF support a platform backend decision instead of mixing platform checks into UI code. Builds for OSes without a cef platform adapter fail explicitly until their CEF child-window implementations are added.
    */
    anyhow::bail!("GPUI CEF has no platform adapter for this OS")
}

pub fn shutdown() {}

pub fn focus_native_view(_native_view: *mut std::ffi::c_void) {}

pub fn focus_gpui_root_view(_native_view: *mut std::ffi::c_void) {}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum BrowserPopupPlacement {
    Selected,
    Background,
}

pub type BrowserPopupOpenHandler = Rc<dyn Fn(String, BrowserPopupPlacement)>;

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

pub type BrowserPageMetadataHandler = Rc<dyn Fn(BrowserPageMetadataEvent)>;

/// CDXC:Onboarding 2026-08-18: API mirror of the CEF
/// main-frame load-end callback used by bridge-less third-party surfaces.
pub type PageLoadEndHandler = Rc<dyn Fn()>;

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

pub struct BrowserMediaAccessRequest {
    requesting_origin: String,
    kinds: BrowserMediaAccessKinds,
}

impl BrowserMediaAccessRequest {
    pub fn requesting_origin(&self) -> &str {
        &self.requesting_origin
    }

    pub fn kinds(&self) -> BrowserMediaAccessKinds {
        self.kinds
    }

    pub fn allow(self, _granted: BrowserMediaAccessKinds) {}

    pub fn deny(self) {}
}

pub type BrowserMediaAccessHandler = Rc<dyn Fn(BrowserMediaAccessRequest)>;

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
    ResourcesSnapshotRequest(String),
}

pub type SidebarBridgeEventHandler = Rc<dyn Fn(SidebarBridgeEvent)>;

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ProjectWorkareaBridgeEvent {
    ProjectBeadsRequest(String),
    ProjectBoardRequest(String),
    ProjectBoardImageRequest(String),
    ManageFilesRequest(String),
}

pub type ProjectWorkareaBridgeEventHandler = Rc<dyn Fn(ProjectWorkareaBridgeEvent)>;

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum AppModalHostBridgeEvent {
    Message(String),
    NativeHostMessage(String),
}

pub type AppModalHostBridgeEventHandler = Rc<dyn Fn(AppModalHostBridgeEvent)>;

#[derive(Clone, Debug)]
pub struct ManageDocsResourceScope;

/// CDXC:Docs 2026-08-09: parity with the CEF scope, whose mounted
/// Docs roots and their allowed relative roots are resolved together, lazily,
/// off the main thread.
type ManageDocsLocalRootResolver =
    Arc<dyn Fn() -> Option<Vec<ManageDocsResourceRoot>> + Send + Sync>;

/// CDXC:Docs 2026-08-09: parity with the CEF scope's mount record.
#[derive(Clone)]
pub struct ManageDocsResourceRoot {
    pub allowed_relative_roots: Vec<String>,
    pub mount_segment: String,
    pub path: PathBuf,
}

/// Parity with the CEF scope's in-memory/remote resource loader.
type ManageDocsRemoteResourceLoader = Arc<dyn Fn(&str) -> Option<Vec<u8>> + Send + Sync>;

impl ManageDocsResourceScope {
    pub fn new(_resolve_root: ManageDocsLocalRootResolver) -> Self {
        Self
    }

    pub fn new_remote(_loader: ManageDocsRemoteResourceLoader) -> Self {
        Self
    }
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

pub struct CefBrowser;

impl CefBrowser {
    pub fn new(
        _parent_native_view: *mut std::ffi::c_void,
        _url: &str,
        _profile: &str,
        _background_color: u32,
        _uses_system_page_appearance: bool,
        _trusted_clipboard_origin: Option<String>,
        _popup_open_handler: Option<BrowserPopupOpenHandler>,
        _page_metadata_handler: Option<BrowserPageMetadataHandler>,
        _media_access_handler: Option<BrowserMediaAccessHandler>,
        _sidebar_runtime_settings: Option<SidebarRuntimeSettingsSnapshot>,
        _sidebar_gxserver_bootstrap: Option<SidebarGxserverBootstrap>,
        _sidebar_bridge_event_handler: Option<SidebarBridgeEventHandler>,
        _project_workarea_bridge_event_handler: Option<ProjectWorkareaBridgeEventHandler>,
        _manage_docs_resource_scope: Option<ManageDocsResourceScope>,
        _app_modal_host_bridge_surface: Option<AppModalHostBridgeSurface>,
        _app_modal_host_bridge_event_handler: Option<AppModalHostBridgeEventHandler>,
        _page_load_end_handler: Option<PageLoadEndHandler>,
    ) -> Self {
        Self
    }

    pub fn set_bounds(&self, _bounds: Bounds<Pixels>, _scale_factor: f32) {}

    pub fn set_visible(&self, _visible: bool) {}

    pub fn order_front(&self) {}

    pub fn identifier(&self) -> i32 {
        0
    }

    pub fn focus(&self) {
        /*
        CDXC:Browser 2026-06-23-12:48:
        Non-macOS CEF is still an explicit unsupported backend, but the stub must keep the same public Browser runtime API as macOS so shared GPUI source can express focus handoff without platform-specific UI branches. This no-op does not create a fallback browser, synthetic focus, logging, persistence, or native hit routing.
        */
    }

    pub fn blur(&self) {}

    pub fn load_url(&self, _url: &str) {}

    pub fn select_all(&self) {}

    pub fn send_fullscreen_toggle_key(&self) {
        /*
        CDXC:Onboarding 2026-08-18:
        API mirror of the macOS/Windows/Linux host-side "f" key press that puts
        the tutorial video player in fullscreen. This no-op must not synthesize
        input, inject JavaScript, or pretend a CEF renderer exists.
        */
    }

    pub fn execute_java_script_in_main_frame(&self, _script: &str) -> bool {
        false
    }

    pub fn refresh_sidebar_runtime_settings(
        &self,
        _runtime_settings: SidebarRuntimeSettingsSnapshot,
    ) {
    }

    pub fn refresh_sidebar_gxserver_bootstrap(
        &self,
        _gxserver_bootstrap: Option<SidebarGxserverBootstrap>,
    ) {
        /*
        CDXC:ServerDaemon 2026-06-24-11:17:
        Non-macOS CEF remains explicitly unsupported, but its Rust API mirrors the macOS sidebar gxserver bootstrap refresh surface so shared GPUI code can compile when platform backends are added. This no-op must not create fallback gxserver data, expose tokens, log, persist, or pretend a CEF renderer exists.
        */
    }

    pub fn refresh_session_chat_gxserver_bootstrap(
        &self,
        _gxserver_bootstrap: Option<SidebarGxserverBootstrap>,
    ) {
        /*
        CDXC:SessionChat 2026-07-31:
        API mirror of the macOS Session Chat bootstrap refresh; same no-op
        rules as the sidebar bootstrap stub above.
        */
    }

    pub fn can_go_back(&self) -> bool {
        false
    }

    pub fn go_back(&self) {}

    pub fn can_go_forward(&self) -> bool {
        false
    }

    pub fn go_forward(&self) {}

    pub fn reload(&self) {}

    pub fn stop_load(&self) {}

    pub fn find_text(&self, _search_text: &str, _forward: bool, _find_next: bool) {}

    pub fn stop_finding(&self, _clear_selection: bool) {}

    pub fn zoom_level(&self) -> f64 {
        0.0
    }

    pub fn zoom_in(&self) {}

    pub fn zoom_out(&self) {}

    pub fn reset_zoom(&self) {}

    pub fn toggle_dev_tools(&self) {}
}
