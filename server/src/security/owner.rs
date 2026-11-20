//! The owner key and owner sessions: the desktop app proves its owner key (kept in the OS key
//! store, never in a file or a session's environment) by signing a one-time challenge, and gets
//! a `gxo_` owner session it holds only in memory.

use std::{
    collections::HashMap,
    sync::{Arc, Mutex},
};

use base64::{engine::general_purpose::URL_SAFE_NO_PAD, Engine};
use ghostex_gx_protocol::approval::{
    owner_key_approval_signing_payload, owner_session_signing_payload, ApprovalRequest,
    ApprovalStatus, ApproverKey, OpenOwnerSessionResult, OwnerChallenge, SensitiveAction,
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
    /// `/api/approveWithOwnerKey` named a request that isn't a waiting ask for a new owner key.
    NotOwnerKeyRequest,
    /// The key already let a new owner key in here with `/api/approveWithOwnerKey`.
    Retired,
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
            Self::Retired => f.write_str(
                "That owner key already let its newer key in on this computer, so it signs nothing here any more.",
            ),
            Self::NotOwnerKeyRequest => f.write_str(
                "That request isn't a waiting ask to add an owner key, so an owner key can't approve it this way.",
            ),
        }
    }
}

/*
CDXC:Security 2026-11-20 FIX:
The user's Mac, renamed to "Macbook 16", was still named "Mohamads-MacBook-Pro-2" in M7-Desktop's "approve it in Ghostex on …", the name its key was enrolled under weeks before: a key's label was written once, at enrollment. The app now sends the computer's name each time its key opens a session, and the label follows it once the signature checks out (only the key's holder can rename it; it is a display name and grants nothing). A phone's label is left alone.
*/
/// The label an app sent with its owner session (`OpenOwnerSessionParams::label`), cleaned for
/// showing: one line, no control characters, at most 80 characters. `None` when nothing is left.
pub fn current_key_label(label: &str) -> Option<String> {
    let cleaned: String = label
        .chars()
        .map(|c| if c.is_control() { ' ' } else { c })
        .collect::<String>()
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ");
    let cleaned: String = cleaned.chars().take(80).collect();
    let cleaned = cleaned.trim().to_string();
    (!cleaned.is_empty()).then_some(cleaned)
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

    /*
    CDXC:Security 2026-11-17 FIX:
    User (after the review of the m7-arch unlock): "Old/pre-16-Nov owner keys get door-only sessions on new gxservers, so they can never open a full owner session." The scope was never part of what the key signs, so a signature relayed from another computer without `scope` opened a full owner session (every capability, no end) wherever the key was enrolled without one (every key from an app before 16 Nov, like this app's old shared key). Every owner session is now a door session: pairing codes, approvals and paired devices only, lapsing after `DOOR_SESSION_TTL_MS`. Apps since 16 Nov ask for nothing else; an app from before (11.2.2 and earlier) gets the door session too, and its other calls fall back to the token file. `scope` is still read for older callers and changes nothing.
    */
    /// Checks the owner key's signature over the challenge and mints a door session (the only
    /// kind of owner session since 17 Nov, above). `label`: the computer's name now, as its app
    /// sends it (CDXC:Security 2026-11-20 at [`current_key_label`]).
    pub fn open_owner_session(
        &self,
        key_id: &str,
        challenge: &str,
        signature_b64: &str,
        label: Option<&str>,
        now_ms: u64,
    ) -> Result<OpenOwnerSessionResult, OwnerError> {
        if !self.owner_challenges.take(challenge, now_ms) {
            return Err(OwnerError::BadChallenge);
        }
        let key = self
            .owner_keys
            .get(key_id)
            .ok_or(OwnerError::Key(ApproverKeyError::UnknownKey))?;
        if self.is_retired_owner_key(key_id) {
            return Err(OwnerError::Retired);
        }
        let payload = owner_session_signing_payload(&self.server_id, challenge, key_id)
            .map_err(|_| OwnerError::BadChallenge)?;
        verify_approval_signature(&key, &payload, signature_b64).map_err(OwnerError::Key)?;
        if let Some(label) = label.and_then(current_key_label) {
            if label != key.label && !matches!(key.platform.as_str(), "ios" | "android") {
                let mut renamed = key.clone();
                renamed.label = label;
                let _ = self.owner_keys.insert(renamed);
            }
        }
        // Door sessions only (CDXC:Security 2026-11-17 above; review M2 in security/policy.rs).
        let until = now_ms + super::DOOR_SESSION_TTL_MS;
        let token = self.session_tokens.mint_door(&key.device_id, until, now_ms);
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
            expires_at_ms: Some(until),
        })
    }

    /// `/api/approveWithOwnerKey` (CDXC:Security 2026-11-17 in packages/gx-protocol/src/approval.rs):
    /// checks that `request` waits to add a new owner key, that `new_key` is that key exactly as
    /// the request sent it and door-only (its parameters' digest), and that owner key `key_id`,
    /// enrolled here, signed this server id, that request's id and nonce, and that new key.
    /// Returns the signing key; the new key is then door-only.
    pub fn check_owner_key_approval(
        &self,
        request: &ApprovalRequest,
        key_id: &str,
        signature_b64: &str,
        new_key: &ApproverKey,
    ) -> Result<ApproverKey, OwnerError> {
        let signer = self
            .owner_keys
            .get(key_id)
            .ok_or(OwnerError::Key(ApproverKeyError::UnknownKey))?;
        if self.is_retired_owner_key(key_id) {
            return Err(OwnerError::Retired);
        }
        let Some(new_key_id) = request.target.as_deref().filter(|_| {
            request.action == SensitiveAction::AddApproverKey
                && request.status == ApprovalStatus::Pending
        }) else {
            return Err(OwnerError::NotOwnerKeyRequest);
        };
        let payload = owner_key_approval_signing_payload(
            &self.server_id,
            &request.id,
            &request.nonce,
            new_key_id,
        )
        .map_err(|_| OwnerError::NotOwnerKeyRequest)?;
        verify_approval_signature(&signer, &payload, signature_b64).map_err(OwnerError::Key)?;
        // The key the request asked to add, door-only, exactly (the same parameters
        // `enroll_key` made the request with).
        let asked = serde_json::json!({
            "key": serde_json::to_value(new_key).map_err(|_| OwnerError::NotOwnerKeyRequest)?,
            "doorOnly": true,
        });
        if super::key_id_of(new_key).ok().as_deref() != Some(new_key_id)
            || super::params_digest(&asked) != request.params_digest
        {
            return Err(OwnerError::NotOwnerKeyRequest);
        }
        self.mark_door_only_key(new_key_id);
        Ok(signer)
    }

    /*
    CDXC:Security 2026-11-17 FIX:
    Second review of the m7-arch unlock (finding 1), user: "In approveWithOwnerKey, on success retire the signing old key on that gxserver (it can't sign there again), so the window closes at each computer's first real unlock." A key that let its newer key in here opens no session and approves nothing here again. It stays in the owner key list, so this computer still has an owner device and the token file never gets the door back; `securityStatus` no longer lists it, so the app doesn't try it again.
    */
    /*
    CDXC:Security 2026-11-17 FIX:
    Third review of the m7-arch unlock. Finding 2, user: "Never retire (or refuse approveWithOwnerKey for) a key that this gxserver's own app enrolled (firstKeySecret / note_app_owner_key_request), so a cloned Mac isn't locked out of its own computer." Migration Assistant copies the login Keychain, so a Mac cloned from this one holds the same old key as its own app's key on its own gxserver; retiring it there would lock that app out. A key enrolled with the one-time value this computer's app handed gxserver at start is noted (`note_app_owner_key`, kept in `security.json`) and is never retired: the approval still lets the other Mac's new key in. Finding 3: retiring also ends that key's door sessions at once, instead of letting them run their 12 hours.
    */
    /// Owner key `key` signs nothing here again (above), and its door sessions end, unless this
    /// computer's own app enrolled it. Whether it was retired.
    pub fn retire_owner_key(&self, key: &ApproverKey) -> bool {
        if self.is_app_owner_key(&key.key_id) {
            return false;
        }
        self.retired_owner_keys
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
            .insert(key.key_id.clone());
        self.session_tokens.drop_door_sessions_of(&key.device_id);
        let _ = self.save();
        true
    }

    /// This computer's own app enrolled owner key `key_id` (it carried this start's one-time value).
    pub fn note_app_owner_key(&self, key_id: &str) {
        let added = self
            .app_owner_keys
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
            .insert(key_id.to_string());
        if added {
            let _ = self.save();
        }
    }

    /// Whether this computer's own app enrolled owner key `key_id` (above).
    pub fn is_app_owner_key(&self, key_id: &str) -> bool {
        self.app_owner_keys
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
            .contains(key_id)
    }

    /// Whether owner key `key_id` was retired here (above).
    pub fn is_retired_owner_key(&self, key_id: &str) -> bool {
        self.retired_owner_keys
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
            .contains(key_id)
    }

    /// The owner keys `securityStatus` lists: every enrolled one but the retired.
    pub fn listed_owner_keys(&self) -> Vec<ApproverKey> {
        self.owner_keys
            .list()
            .into_iter()
            .filter(|key| !self.is_retired_owner_key(&key.key_id))
            .collect()
    }
}
