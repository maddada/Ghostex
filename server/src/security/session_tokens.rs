//! Per-session capability tokens (`gxs_`) and the desktop's owner sessions (`gxo_`).
//!
//! Only the SHA-256 of a token is kept, like the paired phones' `gxd_` tokens (`auth.rs`).

use std::{
    collections::HashMap,
    sync::{Arc, RwLock},
};

use base64::{engine::general_purpose::URL_SAFE_NO_PAD, Engine};
use rand::Rng;
use sha2::{Digest, Sha256};

use super::{Principal, SessionRole};

/// Prefix of a session's own token.
pub const SESSION_TOKEN_PREFIX: &str = "gxs_";
/// Prefix of the desktop app's in-memory owner session.
pub const OWNER_TOKEN_PREFIX: &str = "gxo_";
/// The environment variable a session's processes read their token from (Phase 2 sets it in
/// `zmx/env.rs` and stops handing sessions `GHOSTEX_GXSERVER_AUTH_TOKEN_FILE`).
pub const SESSION_TOKEN_ENV: &str = "GHOSTEX_SESSION_TOKEN";

/// Tokens by hash, each with the principal it stands for.
///
/// Phase 1 keeps them in memory. Phase 2 persists session tokens' hashes in the state database
/// (zmx sessions outlive a gxserver restart and keep their environment), deletes them with the
/// session, and never persists owner sessions.
#[derive(Clone, Default)]
pub struct SessionTokens {
    by_hash: Arc<RwLock<HashMap<String, Principal>>>,
}

impl SessionTokens {
    /// A fresh token for session `session_id` of `project_id`. Any earlier token of the same
    /// session stops working.
    pub fn mint_session(&self, project_id: &str, session_id: &str, role: SessionRole) -> String {
        self.revoke_session(session_id);
        let token = mint(SESSION_TOKEN_PREFIX);
        self.write().insert(
            hash_token(&token),
            Principal::Session {
                project_id: project_id.to_string(),
                session_id: session_id.to_string(),
                role,
            },
        );
        token
    }

    /// An owner session for the desktop app on `device_id`, after it proved the owner key.
    pub fn mint_owner(&self, device_id: &str) -> String {
        let token = mint(OWNER_TOKEN_PREFIX);
        self.write().insert(
            hash_token(&token),
            Principal::Owner {
                device_id: device_id.to_string(),
            },
        );
        token
    }

    pub fn resolve(&self, token: &str) -> Option<Principal> {
        if !token.starts_with(SESSION_TOKEN_PREFIX) && !token.starts_with(OWNER_TOKEN_PREFIX) {
            return None;
        }
        self.by_hash
            .read()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
            .get(&hash_token(token))
            .cloned()
    }

    /// Revokes every token of `session_id`; returns the principals it removed.
    pub fn revoke_session(&self, session_id: &str) -> Vec<Principal> {
        let mut map = self.write();
        let mut removed = Vec::new();
        map.retain(|_, principal| match principal {
            Principal::Session { session_id: id, .. } if id == session_id => {
                removed.push(principal.clone());
                false
            }
            _ => true,
        });
        removed
    }

    pub fn revoke_token(&self, token: &str) -> Option<Principal> {
        self.write().remove(&hash_token(token))
    }

    fn write(&self) -> std::sync::RwLockWriteGuard<'_, HashMap<String, Principal>> {
        self.by_hash
            .write()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
    }
}

fn mint(prefix: &str) -> String {
    let mut random = [0_u8; 32];
    rand::rng().fill_bytes(&mut random);
    format!("{prefix}{}", URL_SAFE_NO_PAD.encode(random))
}

fn hash_token(token: &str) -> String {
    hex::encode(Sha256::digest(token.as_bytes()))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_session_token_resolves_until_its_session_ends() {
        let tokens = SessionTokens::default();
        let first = tokens.mint_session("p", "s", SessionRole::Coordinator);
        assert!(matches!(
            tokens.resolve(&first),
            Some(Principal::Session {
                role: SessionRole::Coordinator,
                ..
            })
        ));
        let renewed = tokens.mint_session("p", "s", SessionRole::Coordinator);
        assert!(tokens.resolve(&first).is_none());
        assert_eq!(tokens.revoke_session("s").len(), 1);
        assert!(tokens.resolve(&renewed).is_none());
    }

    #[test]
    fn foreign_prefixes_never_resolve() {
        let tokens = SessionTokens::default();
        let owner = tokens.mint_owner("mac");
        assert!(tokens.resolve(&owner).is_some());
        assert!(tokens.resolve(&owner.replacen("gxo_", "gxd_", 1)).is_none());
    }
}
