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

/// One row of a Browser page's right-click menu (`BrowserPageContextMenu`).
pub enum BrowserPageContextMenuEntry {
    Item {
        /// CEF's command id for the row (`cef_menu_id_t`, or one of Ghostex's own ids).
        command_id: i32,
        label: String,
        enabled: bool,
    },
    Separator,
}

/// CEF's Back and Forward command ids (`cef_menu_id_t`'s MENU_ID_BACK and MENU_ID_FORWARD).
pub const PAGE_MENU_BACK_COMMAND_ID: i32 = 100;
pub const PAGE_MENU_FORWARD_COMMAND_ID: i32 = 101;

/// A right-click on a Browser page, for Ghostex's own menu to show: the rows to offer, where the
/// click landed in the page's view (logical pixels from its top-left corner), and what runs the
/// chosen row's command on that page.
pub struct BrowserPageContextMenu {
    pub x: f32,
    pub y: f32,
    pub entries: Vec<BrowserPageContextMenuEntry>,
    pub run: Rc<dyn Fn(i32)>,
}

impl BrowserPageContextMenu {
    /// The menu without Back and Forward, for a page that never navigates (the floating page
    /// window opens its links elsewhere), where they would only ever show disabled.
    pub fn without_history(mut self) -> Self {
        self.entries.retain(|entry| {
            !matches!(
                entry,
                BrowserPageContextMenuEntry::Item { command_id, .. }
                    if *command_id == PAGE_MENU_BACK_COMMAND_ID
                        || *command_id == PAGE_MENU_FORWARD_COMMAND_ID
            )
        });
        while matches!(self.entries.first(), Some(BrowserPageContextMenuEntry::Separator)) {
            self.entries.remove(0);
        }
        self
    }
}
