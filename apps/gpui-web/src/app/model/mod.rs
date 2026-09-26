//! The desktop's `model/` is mostly native workspace state; only the plain types the shared drawing code names are lifted out (see `extracted-items.txt`).
#[allow(dead_code, unused_imports)]
pub(crate) mod titlebar_panels {
    use crate::*;
    include!(concat!(env!("OUT_DIR"), "/titlebar_panels.rs"));
}
pub(crate) use titlebar_panels::*;
#[allow(dead_code)]
pub(crate) mod app_modal_kind;
/// The plain focus and interface types of the desktop's sidebar bridge messages.
#[allow(dead_code)]
pub(crate) mod sidebar_bridge_messages {
    include!(concat!(env!("OUT_DIR"), "/sidebar_bridge_messages.rs"));
}
pub(crate) use sidebar_bridge_messages::*;
/// The desktop keeps this key type in `model/local_workspace_attach.rs`; the page lifts it with the id checks (`helpers::ids`).
pub(crate) use crate::app::helpers::GpuiLocalWorkspaceSessionKey;

/// `model/local_workspace_attach.rs`'s conversion (a trait impl, which `build.rs` cannot lift by name).
impl From<&GpuiSidebarWorkspaceTerminalFocusMessage> for GpuiLocalWorkspaceSessionKey {
    fn from(message: &GpuiSidebarWorkspaceTerminalFocusMessage) -> Self {
        Self {
            project_id: message.project_id.clone(),
            session_id: message.session_id.clone(),
        }
    }
}
pub(crate) use app_modal_kind::*;
pub(crate) mod titlebar_mode;
pub(crate) use titlebar_mode::*;
#[allow(dead_code)]
pub(crate) mod agents_terminal_startup {
    include!(concat!(env!("OUT_DIR"), "/agents_terminal_startup.rs"));
}
pub(crate) use agents_terminal_startup::*;

/// The desktop reaches a remote machine's daemon through an SSH tunnel it owns. The browser build has no tunnels yet, so no value of this type is ever made; it exists for the shared chat files that carry one.
#[derive(Clone)]
pub(crate) struct GpuiRemoteGxserverRequestTarget {
    pub(crate) local_port: u16,
    pub(crate) token: String,
}

/// Only the identity counter of the desktop's chat page state, which the chat view uses for draft ids.
pub(crate) struct SessionChatPageState;

impl SessionChatPageState {
    pub(crate) fn next_identity() -> u64 {
        static NEXT: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(1);
        NEXT.fetch_add(1, std::sync::atomic::Ordering::Relaxed)
    }
}
#[allow(dead_code, unused_imports)]
pub(crate) mod hotkeys_and_palette {
    use crate::*;
    include!(concat!(env!("OUT_DIR"), "/hotkeys_and_palette.rs"));
}
pub(crate) use hotkeys_and_palette::*;

#[allow(dead_code, unused_imports)]
mod storybook {
    use crate::*;
    include!(concat!(env!("OUT_DIR"), "/storybook.rs"));
}

/// The desktop's project website providers are read from settings and installed extensions; the browser build offers no views, so no mode is ever a website.
pub(crate) struct WebsiteProvider {
    pub(crate) id: String,
}

impl WebsiteProvider {
    pub(crate) fn automatic(&self) -> bool {
        false
    }
}

impl TitlebarMode {
    pub(crate) fn website_provider(self) -> Option<&'static WebsiteProvider> {
        None
    }
}

/// The desktop's project id newtype, and the one field of its project snapshot the shared create files read (the active project, for a browser open). The page fills it from the store's focus.
#[allow(dead_code)]
pub(crate) mod runtime_state {
    include!(concat!(env!("OUT_DIR"), "/runtime_state.rs"));

    #[derive(Clone, Default, PartialEq, Eq)]
    pub(crate) struct GpuiProjectSnapshot {
        pub(crate) active_project_id: Option<GpuiProjectId>,
    }
}
pub(crate) use runtime_state::*;

/// An Action's run state, which Quick Access's Commands rows show. The page runs no Actions itself, so the map that holds them stays empty.
#[allow(dead_code)]
pub(crate) mod command_pane_action_run {
    include!(concat!(env!("OUT_DIR"), "/command_pane_action_run.rs"));
}
pub(crate) use command_pane_action_run::*;
