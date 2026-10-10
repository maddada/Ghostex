use anyhow::Result;
use gpui::{Bounds, Pixels};
use std::rc::Rc;

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
    HistoryRequested,
    FindRequested,
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
}

pub type BrowserPageMetadataHandler = Rc<dyn Fn(BrowserPageMetadataEvent)>;

/// Plain Rust shared with the native app; see `app/helpers/web_bridge_types.rs`.
pub use crate::app::helpers::web_bridge_types::PageLoadEndHandler;

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

pub enum BrowserSiteRequest {
    OpenExternalApp(BrowserExternalAppRequest),
    LocalNetworkAccess(BrowserLocalNetworkAccessRequest),
}

pub struct BrowserExternalAppRequest {
    pub url: String,
    pub scheme: String,
    pub origin: String,
}

pub struct BrowserLocalNetworkAccessRequest {
    origin: String,
    local_network: bool,
    profile: String,
}

impl BrowserLocalNetworkAccessRequest {
    pub fn origin(&self) -> &str {
        &self.origin
    }

    pub fn profile(&self) -> &str {
        &self.profile
    }

    pub fn includes_local_network(&self) -> bool {
        self.local_network
    }

    pub fn page_gone(&mut self) -> impl std::future::Future<Output = ()> + 'static {
        std::future::pending()
    }

    pub fn answer(self, _allow: bool) {}
}

pub fn forget_local_network_access_answer(_profile: &str, _origin: &str) -> bool {
    false
}

pub type BrowserSiteRequestHandler = Rc<dyn Fn(BrowserSiteRequest)>;

pub fn set_browser_site_request_handler(_handler: BrowserSiteRequestHandler) {}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ProjectWorkareaBridgeEvent {
    ProjectBeadsRequest(String),
    ProjectBoardRequest(String),
    ProjectBoardImageRequest(String),
    ManageFilesRequest(String),
    /// The page tried to navigate its own main frame somewhere else; the payload is the refused URL.
    RefusedPageNavigation(String),
}

pub type ProjectWorkareaBridgeEventHandler = Rc<dyn Fn(ProjectWorkareaBridgeEvent)>;

/// The Docs resource scope is plain Rust shared with the native Files view; see
/// `app/helpers/manage_docs_resources.rs`.
pub use crate::app::helpers::manage_docs_resources::ManageDocsResourceScope;

/// API mirror of `shell::PageKeepAwake`; without a runtime there is no page to keep.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum PageKeepAwake {
    Unknown,
    Keep,
    Release,
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
        _project_workarea_bridge_event_handler: Option<ProjectWorkareaBridgeEventHandler>,
        _manage_docs_resource_scope: Option<ManageDocsResourceScope>,
        _page_load_end_handler: Option<PageLoadEndHandler>,
    ) -> Self {
        Self
    }

    pub fn set_bounds(&self, _bounds: Bounds<Pixels>, _scale_factor: f32) {}

    pub fn set_visible(&self, _visible: bool) {}

    pub fn bounds_differ(&self, _bounds: Bounds<Pixels>, _scale_factor: f32) -> bool {
        false
    }

    pub fn set_motion_hidden(&self, _hidden: bool, _fade_in: std::time::Duration) {}

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

    pub fn page_keep_awake(&self) -> PageKeepAwake {
        PageKeepAwake::Release
    }

    pub fn ask_page_keep_awake(&self) {}

    pub fn load_url(&self, _url: &str) {}

    pub fn select_all(&self) {}

    pub fn execute_java_script_in_main_frame(&self, _script: &str) -> bool {
        false
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
