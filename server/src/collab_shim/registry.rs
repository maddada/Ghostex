//! The live shares the Collab component told gxserver about (`setShares`), in memory only: what
//! lets a guest's replayed read through, for that share's one chat. Empty while the component
//! isn't running, so every `gxshare_` token is refused then.
//!
//! A share token opens only [`GUEST_READ_ENDPOINTS`] for its own `projectId`/`sessionId`, and
//! that chat's stream on `/api/events?stream=sessionChat`. Everything a guest writes is answered
//! by the component itself, never here.

use std::sync::{OnceLock, PoisonError, RwLock};

use ghostex_gx_protocol::collab_component::ShareScope;
use sha2::{Digest as _, Sha256};
use subtle::ConstantTimeEq;

/// What a share token may call, always for its own chat only.
///
/// CDXC:Collab 2026-11-07 WHY: `readSessionChatImage` takes a file path, so a guest could read any picture on the owner's computer through it; a guest's read is answered only for a path the shared chat itself shows (`session_files_links::session_shows_path`) or one that share's guests uploaded. `readVisualPageThumbnail` stays out.
pub(crate) const GUEST_READ_ENDPOINTS: &[&str] = &[
    "/api/readSessionChat",
    "/api/readSessionChatImage",
    "/api/readSessionChatResource",
    // A page the shared chat published or an HTML file it links to (visual_pages/guest.rs).
    "/api/collabGuestPage",
];

struct Entry {
    token_hash: [u8; 32],
    scope: ShareScope,
}

fn entries() -> &'static RwLock<Vec<Entry>> {
    static ENTRIES: OnceLock<RwLock<Vec<Entry>>> = OnceLock::new();
    ENTRIES.get_or_init(|| RwLock::new(Vec::new()))
}

fn hash(token: &str) -> [u8; 32] {
    Sha256::digest(token.as_bytes()).into()
}

/// Replaces the live shares. Only well-formed `gxshare_` tokens are kept.
pub(crate) fn set(scopes: Vec<ShareScope>) {
    let next = scopes
        .into_iter()
        .filter(|scope| scope.token.starts_with("gxshare_") && scope.token.len() >= 40)
        .map(|scope| Entry {
            token_hash: hash(&scope.token),
            scope: ShareScope {
                token: String::new(),
                ..scope
            },
        })
        .collect();
    *entries().write().unwrap_or_else(PoisonError::into_inner) = next;
}

pub(crate) fn clear() {
    set(Vec::new());
}

/// The live share a token belongs to.
pub(crate) fn authorize_token(token: &str) -> Option<ShareScope> {
    if !token.starts_with("gxshare_") {
        return None;
    }
    let wanted = hash(token);
    entries()
        .read()
        .unwrap_or_else(PoisonError::into_inner)
        .iter()
        .find(|entry| bool::from(entry.token_hash.ct_eq(&wanted)))
        .map(|entry| entry.scope.clone())
}

/// The live share of a chat, if it has one.
pub(crate) fn for_chat(project_id: &str, session_id: &str) -> Option<ShareScope> {
    entries()
        .read()
        .unwrap_or_else(PoisonError::into_inner)
        .iter()
        .find(|entry| entry.scope.project_id == project_id && entry.scope.session_id == session_id)
        .map(|entry| entry.scope.clone())
}

/// The share as it is now (its guests and pictures change while a token is in use).
pub(crate) fn current(share_id: &str) -> Option<ShareScope> {
    entries()
        .read()
        .unwrap_or_else(PoisonError::into_inner)
        .iter()
        .find(|entry| entry.scope.share_id == share_id)
        .map(|entry| entry.scope.clone())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn only_live_share_tokens_open_their_chat() {
        let _lock = crate::collab_shim::test_lock();
        let token = format!("gxshare_{}", "x".repeat(43));
        set(vec![ShareScope {
            share_id: "share_1".into(),
            project_id: "p".into(),
            session_id: "s".into(),
            token: token.clone(),
            viewer_ids: vec!["guest_1".into()],
            images: vec![],
        }]);
        let scope = authorize_token(&token).expect("live");
        assert_eq!(scope.session_id, "s");
        assert!(scope.token.is_empty(), "the token itself is never kept");
        assert!(authorize_token(&format!("gxshare_{}", "y".repeat(43))).is_none());
        assert!(authorize_token("not-a-share").is_none());
        assert!(for_chat("p", "s").is_some());
        clear();
        assert!(authorize_token(&token).is_none());
    }
}
