//! The owner key and owner sessions: the desktop app proves its owner key (kept in the OS key
//! store, never in a file or a session's environment) by signing a one-time challenge, and gets
//! a `gxo_` owner session it holds only in memory.

use std::{
    collections::HashMap,
    sync::{Arc, Mutex},
};

use base64::{engine::general_purpose::URL_SAFE_NO_PAD, Engine};
use ghostex_gx_protocol::approval::{
    owner_session_signing_payload, ApproverKey, OpenOwnerSessionResult, OwnerChallenge,
};
use rand::Rng;

use super::{verify_approval_signature, ApproverKeyError, SecurityEvent, SecurityState};

/// How long a challenge may be answered.
pub const OWNER_CHALLENGE_TTL_MS: u64 = 60 * 1000;

#[derive(Clone, Default)]
pub struct OwnerChallenges {
    by_value: Arc<Mutex<HashMap<String, u64>>>,
}

impl OwnerChallenges {
    pub fn issue(&self, now_ms: u64) -> OwnerChallenge {
        let mut random = [0_u8; 32];
        rand::rng().fill_bytes(&mut random);
        let challenge = URL_SAFE_NO_PAD.encode(random);
        let expires_at_ms = now_ms + OWNER_CHALLENGE_TTL_MS;
        let mut map = self.lock();
        map.retain(|_, expires| *expires > now_ms);
        map.insert(challenge.clone(), expires_at_ms);
        OwnerChallenge {
            challenge,
            expires_at_ms,
        }
    }

    /// Single use: a challenge is gone once taken, answered right or wrong.
    pub fn take(&self, challenge: &str, now_ms: u64) -> bool {
        self.lock()
            .remove(challenge)
            .is_some_and(|expires| expires > now_ms)
    }

    fn lock(&self) -> std::sync::MutexGuard<'_, HashMap<String, u64>> {
        self.by_value
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
    }
}

/// Why an owner key or session was refused.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum OwnerError {
    /// An owner key is already enrolled and this is another one.
    AlreadyEnrolled,
    Key(ApproverKeyError),
    BadChallenge,
    Save(String),
}

impl std::fmt::Display for OwnerError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::AlreadyEnrolled => f.write_str(
                "Another owner key is already enrolled on this computer. Add this one from a device you already trust.",
            ),
            Self::Key(error) => error.fmt(f),
            Self::BadChallenge => f.write_str("That challenge expired or was already used."),
            Self::Save(message) => write!(f, "Ghostex could not save its keys: {message}"),
        }
    }
}

impl SecurityState {
    /*
    CDXC:Security 2026-11-08 DECISION:
    User (#495, answer 1A): trust the first key enrolled at first start; Help says plainly that Windows and Linux are weaker because programs running as the user can read their key stores. So the first owner key is accepted from any local caller while none is enrolled (the desktop enrolls its key the first time it starts on 11.0.1), the same key enrolling again is a no-op, and every other owner key needs an approved `addApproverKey` (handled by the route).
    */
    pub fn enroll_first_owner_key(
        &self,
        mut key: ApproverKey,
        now_ms: u64,
    ) -> Result<ApproverKey, OwnerError> {
        key.added_at_ms = now_ms;
        let existing = self.owner_keys.list();
        if !existing.is_empty() {
            let candidate = super::approver_keys::key_id_of(&key).map_err(OwnerError::Key)?;
            return existing
                .into_iter()
                .find(|known| known.key_id == candidate)
                .ok_or(OwnerError::AlreadyEnrolled);
        }
        self.add_owner_key(key)
    }

    /// Adds an owner key without the first-key rule; the caller checked an approval.
    pub fn add_owner_key(&self, key: ApproverKey) -> Result<ApproverKey, OwnerError> {
        let key = self.owner_keys.insert(key).map_err(OwnerError::Key)?;
        self.save()
            .map_err(|error| OwnerError::Save(error.to_string()))?;
        Ok(key)
    }

    pub fn owner_challenge(&self, now_ms: u64) -> OwnerChallenge {
        self.owner_challenges.issue(now_ms)
    }

    /// Checks the owner key's signature over the challenge and mints an owner session.
    pub fn open_owner_session(
        &self,
        key_id: &str,
        challenge: &str,
        signature_b64: &str,
        now_ms: u64,
    ) -> Result<OpenOwnerSessionResult, OwnerError> {
        if !self.owner_challenges.take(challenge, now_ms) {
            return Err(OwnerError::BadChallenge);
        }
        let key = self
            .owner_keys
            .get(key_id)
            .ok_or(OwnerError::Key(ApproverKeyError::UnknownKey))?;
        let payload = owner_session_signing_payload(&self.server_id, challenge, key_id)
            .map_err(|_| OwnerError::BadChallenge)?;
        verify_approval_signature(&key, &payload, signature_b64).map_err(OwnerError::Key)?;
        let token = self.session_tokens.mint_owner(&key.device_id);
        self.save()
            .map_err(|error| OwnerError::Save(error.to_string()))?;
        self.publish(SecurityEvent::SessionTokenMinted {
            principal: super::Principal::Owner {
                device_id: key.device_id.clone(),
            },
        });
        Ok(OpenOwnerSessionResult {
            token,
            device_id: key.device_id,
        })
    }
}
