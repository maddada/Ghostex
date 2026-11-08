//! Ghostex Collab, the component: share a chat through a link that opens it in a browser.
//!
//! This is all of Collab's working code. The app ships none of it (GitHub #495, Q1730): gxserver
//! downloads this component only when the owner installs Collab in Settings › Extensions with
//! their signed approval, starts it as a sidecar, and talks to it over the local channel in
//! `packages/gx-protocol/src/collab_component.rs`.
//!
//! - `shares.rs`: each share's key, route, code, expiry, mode, guests and held messages.
//! - `config.rs`: what a new share starts with.
//! - `owner.rs`: the owner's calls (`/api/collab`, forwarded by gxserver), with the gate brokered by
//!   gxserver.
//! - `connector.rs`: the outgoing link to the share relay, the end-to-end lock (`tunnel.rs`) and
//!   the guest page pin (`page_pin.rs`); guest reads are replayed to gxserver with the share's
//!   token, guest writes are answered in `guest.rs`.
//! - `sync.rs`: what gxserver is told (the live shares, each shared chat's guests and messages).
//! - `host.rs`: the calls into gxserver.
//! - `serve.rs`: the sidecar's loopback listener and its start.
//!
//! SEE-ALSO: server/src/collab_shim (gxserver's side), server/share-relay (the relay),
//! apps/gpui-web/www/src/collab/ (the guest page), packages/gx-chat-core/src/extras/collab.rs (the
//! chat's guest rows, typing line and cards).

pub mod audit;
pub mod config;
pub mod connector;
pub mod guest;
pub mod host;
pub mod http;
pub mod owner;
pub mod page_pin;
pub mod serve;
pub mod shares;
pub mod sync;
pub mod tunnel;

use std::path::PathBuf;
use std::sync::OnceLock;

/// Ghostex's own share relay, beside the Easy Connect relay (relay-eu.ghostex.dev).
pub const GHOSTEX_SHARE_RELAY_URL: &str = "https://share.ghostex.dev";

static DATA_DIR: OnceLock<PathBuf> = OnceLock::new();

/// Sets the component's data folder (from gxserver's hello); first call wins.
pub fn set_data_dir(dir: PathBuf) {
    let _ = DATA_DIR.set(dir);
}

/// Where Collab keeps its choices, shares, held messages and log: the folder gxserver named, or
/// one private folder per test run.
pub fn data_dir() -> PathBuf {
    DATA_DIR
        .get_or_init(|| {
            #[cfg(test)]
            {
                static HOME: OnceLock<tempfile::TempDir> = OnceLock::new();
                HOME.get_or_init(|| tempfile::tempdir().expect("collab test home"))
                    .path()
                    .to_path_buf()
            }
            #[cfg(not(test))]
            {
                std::env::temp_dir().join("ghostex-collab-unset")
            }
        })
        .clone()
}

fn changes() -> &'static tokio::sync::Notify {
    static CHANGES: OnceLock<tokio::sync::Notify> = OnceLock::new();
    CHANGES.get_or_init(tokio::sync::Notify::new)
}

/// Wakes the relay connector and the sync to gxserver: a share, a viewer or a choice changed.
pub fn notify_changed() {
    changes().notify_waiters();
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

/// What the agent reads: the guest's words, labelled with their name so it knows they did not
/// come from the owner.
pub fn labelled_guest_text(name: &str, text: &str) -> String {
    format!("{name} (a guest in Ghostex Collab) says:\n{text}")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn picture_references_come_out_of_the_words() {
        let (words, paths) = split_image_references("look [Image #1](/a/b.png) here [Image #x](y)");
        assert_eq!(words, "look  here [Image #x](y)");
        assert_eq!(paths, vec!["/a/b.png".to_string()]);
    }
}

#[cfg(test)]
mod tests_owner_guest;
