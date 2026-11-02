//! Side chats in the view panel: their tabs in the strip (`tabs.rs`, `render.rs`), the chat each
//! tab hosts (`host.rs`), and the chat host actions that open or close one. A side chat is a forked
//! session hidden from the sidebar and from the Agents workspace, so it has no Agents tab and no
//! `TerminalSessionId` there; its chat view lives in `GhostexGpuiApp::side_panel_chats` instead.
//!
//! SEE-ALSO: apps/desktop/src/app/model/view_tabs.rs (`ViewStripTabKey::SideChat`, the saved form),
//! packages/gx-chat-core/src/extras/side_chats.rs (`openSideChat`, `sideChatDeleted`),
//! apps/gpui-web/src/app/chat_host.rs (the web page's side chat panel).

mod host;
mod render;
mod tabs;

pub(crate) use host::SidePanelChat;
