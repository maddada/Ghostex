//! Per-session capability tokens (`gxs_`) and the desktop's owner sessions (`gxo_`).

use std::{
    collections::HashMap,
    sync::{Arc, RwLock},
};

use base64::{engine::general_purpose::URL_SAFE_NO_PAD, Engine};
use rand::Rng;
use sha2::{Digest, Sha256};

use super::{persist::PersistedSessionToken, Principal, SessionRole};

/// Prefix of a session's own token.
pub const SESSION_TOKEN_PREFIX: &str = "gxs_";
/// Prefix of the desktop app's in-memory owner session.
pub const OWNER_TOKEN_PREFIX: &str = "gxo_";
/// The environment variable a session's processes read their own token from. The `ghostex` CLI
/// prefers it over the token file.
pub const SESSION_TOKEN_ENV: &str = "GHOSTEX_SESSION_TOKEN";

#[derive(Default)]
struct Tokens {
    /// SHA-256 of every live token → who it stands for (lookups never compare raw tokens).
    by_hash: HashMap<String, Principal>,
    /// `<project id>:<session id>` → its token, so a session started again (or attached through a script that
    /// may start it) gets the same token its running processes already hold.
    session_token: HashMap<String, String>,
}

/*
CDXC:Security 2026-11-08 WHY:
A session's token must be issued again unchanged: zmx daemons outlive gxserver restarts and attach scripts start a session only if it is missing, so minting a new token each time would revoke the one the running agent holds. The session's token is therefore kept (in memory and in the 0600 `auth/security.json`) until the session ends. That file is as readable to a same-user program as the token file was, the documented limit of #495; what a session token adds over the token file is the session's identity and its revocation, not secrecy from other programs of the user.
*/
#[derive(Clone, Default)]
pub struct SessionTokens {
    inner: Arc<RwLock<Tokens>>,
}

impl SessionTokens {
    /// The token of session `session_id`: the one it already has, with its role brought up to
    /// date, or a new one. `true` when it is new.
    pub fn issue_session(
        &self,
        project_id: &str,
        session_id: &str,
        role: SessionRole,
    ) -> (String, bool) {
        let principal = Principal::Session {
            project_id: project_id.to_string(),
            session_id: session_id.to_string(),
            role,
        };
        let key = session_key(project_id, session_id);
        let mut tokens = self.write();
        if let Some(token) = tokens.session_token.get(&key).cloned() {
            tokens.by_hash.insert(hash_token(&token), principal);
            return (token, false);
        }
        let token = mint(SESSION_TOKEN_PREFIX);
        tokens.by_hash.insert(hash_token(&token), principal);
        tokens.session_token.insert(key, token.clone());
        (token, true)
    }

    /// A new token for the session, revoking its earlier one.
    pub fn mint_session(&self, project_id: &str, session_id: &str, role: SessionRole) -> String {
        self.revoke_session(project_id, session_id);
        self.issue_session(project_id, session_id, role).0
    }

    /// An owner session for the desktop app on `device_id`, after it proved the owner key.
    pub fn mint_owner(&self, device_id: &str) -> String {
        let token = mint(OWNER_TOKEN_PREFIX);
        self.write().by_hash.insert(
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
        self.inner
            .read()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
            .by_hash
            .get(&hash_token(token))
            .cloned()
    }

    /// Revokes the session's token; returns the principal it removed.
    pub fn revoke_session(&self, project_id: &str, session_id: &str) -> Vec<Principal> {
        let mut tokens = self.write();
        let Some(token) = tokens
            .session_token
            .remove(&session_key(project_id, session_id))
        else {
            return Vec::new();
        };
        tokens
            .by_hash
            .remove(&hash_token(&token))
            .into_iter()
            .collect()
    }

    pub fn revoke_token(&self, token: &str) -> Option<Principal> {
        let mut tokens = self.write();
        let principal = tokens.by_hash.remove(&hash_token(token))?;
        if let Principal::Session {
            project_id,
            session_id,
            ..
        } = &principal
        {
            tokens
                .session_token
                .remove(&session_key(project_id, session_id));
        }
        Some(principal)
    }

    /// The session tokens to save (owner sessions are never written).
    pub fn snapshot(&self) -> Vec<PersistedSessionToken> {
        let tokens = self
            .inner
            .read()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        let mut saved: Vec<_> = tokens
            .session_token
            .iter()
            .filter_map(|(_, token)| match tokens.by_hash.get(&hash_token(token)) {
                Some(Principal::Session {
                    project_id,
                    session_id,
                    role,
                }) => Some(PersistedSessionToken {
                    token: token.clone(),
                    project_id: project_id.clone(),
                    session_id: session_id.clone(),
                    role: *role,
                }),
                _ => None,
            })
            .collect();
        saved.sort_by(|a, b| a.session_id.cmp(&b.session_id));
        saved
    }

    /// Puts back the session tokens saved before a restart.
    pub fn restore(&self, saved: Vec<PersistedSessionToken>) {
        let mut tokens = self.write();
        for entry in saved {
            if !entry.token.starts_with(SESSION_TOKEN_PREFIX) {
                continue;
            }
            tokens.by_hash.insert(
                hash_token(&entry.token),
                Principal::Session {
                    project_id: entry.project_id.clone(),
                    session_id: entry.session_id.clone(),
                    role: entry.role,
                },
            );
            tokens.session_token.insert(
                session_key(&entry.project_id, &entry.session_id),
                entry.token,
            );
        }
    }

    /// The `(project id, session id)` of every session that holds a token.
    pub fn sessions(&self) -> Vec<(String, String)> {
        self.snapshot()
            .into_iter()
            .map(|entry| (entry.project_id, entry.session_id))
            .collect()
    }

    fn write(&self) -> std::sync::RwLockWriteGuard<'_, Tokens> {
        self.inner
            .write()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
    }
}

fn session_key(project_id: &str, session_id: &str) -> String {
    format!("{project_id}:{session_id}")
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
        assert!(tokens.revoke_session("other", "s").is_empty());
        assert_eq!(tokens.revoke_session("p", "s").len(), 1);
        assert!(tokens.resolve(&renewed).is_none());
    }

    #[test]
    fn issuing_again_keeps_the_running_session_token_and_survives_a_restart() {
        let tokens = SessionTokens::default();
        let (first, new) = tokens.issue_session("p", "s", SessionRole::Agent);
        assert!(new);
        let (again, new) = tokens.issue_session("p", "s", SessionRole::Thread);
        assert!(!new);
        assert_eq!(first, again);
        assert!(matches!(
            tokens.resolve(&first),
            Some(Principal::Session {
                role: SessionRole::Thread,
                ..
            })
        ));
        let restarted = SessionTokens::default();
        restarted.restore(tokens.snapshot());
        assert!(restarted.resolve(&first).is_some());
        assert_eq!(
            restarted.issue_session("p", "s", SessionRole::Thread).0,
            first
        );
    }

    #[test]
    fn foreign_prefixes_never_resolve() {
        let tokens = SessionTokens::default();
        let owner = tokens.mint_owner("mac");
        assert!(tokens.resolve(&owner).is_some());
        assert!(tokens.resolve(&owner.replacen("gxo_", "gxd_", 1)).is_none());
    }
}
