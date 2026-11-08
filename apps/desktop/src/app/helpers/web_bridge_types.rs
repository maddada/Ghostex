//! Plain-Rust shapes that native code shares with the web-page bridges: the settings snapshot and
//! gxserver bootstrap a page receives, the app-modal host's message event, and the load-end
//! callback. They live outside `cef` so the native app (Settings, chat, the store) names no CEF
//! type while the web runtime is optional; `cef` re-exports them for its bridges.
//! SEE-ALSO: apps/desktop/src/cef/shell/message_routing.rs and apps/desktop/src/cef/unsupported.rs
//! (the re-exports), app/helpers/web_runtime.rs (CDXC:CefRuntime 2026-09-28).

use std::rc::Rc;

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct SidebarRuntimeSettingsSnapshot {
    pub debugging_mode: bool,
    /// All Automations is on (Settings > Extensions > Experimental).
    pub all_automations_enabled: bool,
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

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum AppModalHostBridgeEvent {
    Message(String),
}

pub type AppModalHostBridgeEventHandler = Rc<dyn Fn(AppModalHostBridgeEvent)>;

/*
CDXC:Onboarding 2026-08-18:
Third-party surfaces that carry no Ghostex bridge (today only the tutorial
video modal, which loads the YouTube watch page as its top-level document) can
still need a host-side action once their page is really on screen. This
callback reports main-frame load-end for exactly those surfaces; it carries no
page data (no URL, title, or content), only the "this browser finished loading
its main frame" edge.
*/
pub type PageLoadEndHandler = Rc<dyn Fn()>;
