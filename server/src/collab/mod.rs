//! Ghostex Collab: share a chat through a link that opens it in a browser. The owner turns it on
//! in Settings > Collab and presses Share in a chat's header; gxserver keeps the share
//! (`shares.rs`), the choices (`config.rs`) and the end-to-end lock (`tunnel.rs`), and the relay
//! connector (server/src/server/collab_connector.rs) carries guests from share.ghostex.dev to this
//! chat. The owner's calls are `/api/collab` (server/src/server/collab_http.rs).
//!
//! SEE-ALSO: server/share-relay (the relay), apps/gpui-web/www/src/collab/ (the guest page),
//! packages/gx-chat-core/src/extras/collab.rs (the chat's guest rows, typing line and cards),
//! apps/desktop/src/app/render/workarea_header/share_button.rs (Share), apps/desktop/src/app/window/settings_modal/tabs/collab.rs (Settings > Collab).

pub mod audit;
pub mod config;
pub mod page_pin;
pub mod shares;
pub mod tunnel;

use std::sync::OnceLock;

pub use config::{apply_patch, read_config, update_config, CollabConfig, GuestMode};
pub use shares::insert_chat_collab;

/// Where Collab keeps its files: gxserver's state folder, or one private folder per test run, so
/// tests never read or end the real shares.
pub(crate) fn state_paths() -> crate::paths::GxserverPaths {
    #[cfg(test)]
    {
        static HOME: OnceLock<tempfile::TempDir> = OnceLock::new();
        let home = HOME.get_or_init(|| tempfile::tempdir().expect("collab test home"));
        crate::paths::get_gxserver_paths(Some(home.path().to_path_buf()))
    }
    #[cfg(not(test))]
    crate::paths::get_gxserver_paths(None)
}

/// The 11.0.1 move (GitHub #495): an 11.0.0 `collab.json` or share file ends every share and
/// turns Collab off, once. Run at gxserver start, before anything reads a share.
pub fn migrate_at_start(paths: &crate::paths::GxserverPaths) {
    if config::migrate(paths, &[shares::SHARES_FILE, shares::HELD_FILE]) {
        eprintln!("[collab] 11.1.0: ended the shares made before Collab's security update and turned Collab off");
    }
}

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

/// The chat box's picture references (`[Image #1](/path/to/picture.png)`, what a paste writes)
/// taken out of `text`: the words left, and the pictures' paths in order.
pub fn split_image_references(text: &str) -> (String, Vec<String>) {
    let mut words = String::new();
    let mut paths = Vec::new();
    let mut rest = text;
    while let Some(start) = rest.find("[Image #") {
        let after = &rest[start + "[Image #".len()..];
        let digits = after.chars().take_while(char::is_ascii_digit).count();
        let tail = &after[digits..];
        match (digits > 0 && tail.starts_with("]("))
            .then(|| tail[2..].find(')'))
            .flatten()
        {
            Some(close) => {
                words.push_str(&rest[..start]);
                paths.push(tail[2..2 + close].to_string());
                rest = &tail[2 + close + 1..];
            }
            None => {
                words.push_str(&rest[..start + 1]);
                rest = &rest[start + 1..];
            }
        }
    }
    words.push_str(rest);
    (words.trim().to_string(), paths)
}
