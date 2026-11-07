//! Ghostex Collab: share a chat through a link that opens it in a browser. The owner turns it on
//! in Settings > Collab and presses Share in a chat's header; gxserver keeps the share
//! (`shares.rs`), the choices (`config.rs`) and the end-to-end lock (`tunnel.rs`), and the relay
//! connector (server/src/server/collab_connector.rs) carries guests from share.ghostex.dev to this
//! chat. The owner's calls are `/api/collab` (server/src/server/collab_http.rs).
//!
//! SEE-ALSO: server/share-relay (the relay), apps/gpui-web/www/src/collab/ (the guest page),
//! packages/gx-chat-core/src/extras/collab.rs (the chat's guest rows, typing line and cards),
//! apps/desktop/src/app/render/workarea_header/share_button.rs (Share), apps/desktop/src/app/window/settings_modal/tabs/collab.rs (Settings > Collab).

pub mod config;
pub mod shares;
pub mod tunnel;

use std::sync::OnceLock;

pub use config::{apply_patch, read_config, update_config, CollabConfig, GuestMode};
pub use shares::insert_chat_collab;

/// Ghostex's own share relay, beside the Easy Connect relay (relay-eu.ghostex.dev).
pub const GHOSTEX_SHARE_RELAY_URL: &str = "https://share.ghostex.dev";

fn changes() -> &'static tokio::sync::Notify {
    static CHANGES: OnceLock<tokio::sync::Notify> = OnceLock::new();
    CHANGES.get_or_init(tokio::sync::Notify::new)
}

/// Wakes the relay connector: a share, a viewer or the switch changed.
pub fn notify_changed() {
    changes().notify_one();
}

pub async fn changed() {
    changes().notified().await;
}
