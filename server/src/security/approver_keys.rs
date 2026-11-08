//! Approver keys: the public halves of the biometric-locked keys that sign approvals, one per
//! approving device (the desktop app on each computer, each paired phone), and the signature
//! check.

use std::{
    collections::HashMap,
    sync::{Arc, RwLock},
};

use base64::{engine::general_purpose::STANDARD, Engine};
use ghostex_gx_protocol::approval::{ApproverKey, ApproverKeyAlgorithm};
use sha2::{Digest, Sha256};

/// Why a signature or a key was refused.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ApproverKeyError {
    UnknownKey,
    UnsupportedAlgorithm(String),
    BadPublicKey,
    BadSignatureEncoding,
    SignatureMismatch,
}

impl std::fmt::Display for ApproverKeyError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::UnknownKey => f.write_str("This approver key isn't registered here."),
            Self::UnsupportedAlgorithm(name) => {
                write!(f, "Approver keys of type {name} aren't supported.")
            }
            Self::BadPublicKey => f.write_str("The approver key's public key can't be read."),
            Self::BadSignatureEncoding => f.write_str("The signature isn't valid base64."),
            Self::SignatureMismatch => f.write_str("The signature doesn't match this request."),
        }
    }
}

/// The key id of a public key: hex of the first 16 bytes of its SHA-256.
pub fn approver_key_id(public_key: &[u8]) -> String {
    hex::encode(&Sha256::digest(public_key)[..16])
}

/// The key id `key` gets once enrolled (from its public key, whatever `key.key_id` says).
pub fn key_id_of(key: &ApproverKey) -> Result<String, ApproverKeyError> {
    STANDARD
        .decode(&key.public_key)
        .map(|bytes| approver_key_id(&bytes))
        .map_err(|_| ApproverKeyError::BadPublicKey)
}

/// Checks `signature` (base64) over `payload` with `key`.
pub fn verify_approval_signature(
    key: &ApproverKey,
    payload: &[u8],
    signature_b64: &str,
) -> Result<(), ApproverKeyError> {
    use ring::signature::{UnparsedPublicKey, ECDSA_P256_SHA256_ASN1, RSA_PKCS1_2048_8192_SHA256};

    let public_key = STANDARD
        .decode(&key.public_key)
        .map_err(|_| ApproverKeyError::BadPublicKey)?;
    let signature = STANDARD
        .decode(signature_b64.trim())
        .map_err(|_| ApproverKeyError::BadSignatureEncoding)?;
    let result = match &key.algorithm {
        ApproverKeyAlgorithm::EcdsaP256Sha256 => {
            UnparsedPublicKey::new(&ECDSA_P256_SHA256_ASN1, &public_key).verify(payload, &signature)
        }
        ApproverKeyAlgorithm::RsaPkcs1Sha256 => {
            UnparsedPublicKey::new(&RSA_PKCS1_2048_8192_SHA256, &public_key)
                .verify(payload, &signature)
        }
        ApproverKeyAlgorithm::Other(name) => {
            return Err(ApproverKeyError::UnsupportedAlgorithm(name.clone()))
        }
    };
    result.map_err(|_| ApproverKeyError::SignatureMismatch)
}

/// The registered approver keys, by key id.
///
/// Phase 1 keeps them in memory. Phase 2 persists them in the state database, signed with the
/// computer's integrity key where the platform has one, and enforces the enrollment rules in
/// `ai/security-tokens-and-approvals.md` (the first desktop key at first start, every later key
/// through an approved [`ghostex_gx_protocol::approval::SensitiveAction::AddApproverKey`]).
#[derive(Clone, Default)]
pub struct ApproverKeys {
    keys: Arc<RwLock<HashMap<String, ApproverKey>>>,
}

impl ApproverKeys {
    pub fn list(&self) -> Vec<ApproverKey> {
        let mut keys: Vec<_> = self
            .keys
            .read()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
            .values()
            .cloned()
            .collect();
        keys.sort_by_key(|key| key.added_at_ms);
        keys
    }

    pub fn get(&self, key_id: &str) -> Option<ApproverKey> {
        self.keys
            .read()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
            .get(key_id)
            .cloned()
    }

    pub fn is_empty(&self) -> bool {
        self.keys
            .read()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
            .is_empty()
    }

    /// Adds `key` after checking its public key parses; its `key_id` is recomputed here so a
    /// caller can't pick one.
    pub fn insert(&self, mut key: ApproverKey) -> Result<ApproverKey, ApproverKeyError> {
        let public_key = STANDARD
            .decode(&key.public_key)
            .map_err(|_| ApproverKeyError::BadPublicKey)?;
        match &key.algorithm {
            ApproverKeyAlgorithm::EcdsaP256Sha256 => {
                // The uncompressed X9.63 point: 0x04, then X and Y.
                if public_key.len() != 65 || public_key[0] != 0x04 {
                    return Err(ApproverKeyError::BadPublicKey);
                }
            }
            ApproverKeyAlgorithm::RsaPkcs1Sha256 => {
                if public_key.len() < 256 {
                    return Err(ApproverKeyError::BadPublicKey);
                }
            }
            ApproverKeyAlgorithm::Other(name) => {
                return Err(ApproverKeyError::UnsupportedAlgorithm(name.clone()))
            }
        }
        key.key_id = approver_key_id(&public_key);
        self.keys
            .write()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
            .insert(key.key_id.clone(), key.clone());
        Ok(key)
    }

    /// Puts back keys loaded from disk, each checked again as [`Self::insert`] does; a key
    /// that no longer parses is dropped.
    pub fn restore(&self, keys: Vec<ApproverKey>) {
        for key in keys {
            let _ = self.insert(key);
        }
    }

    /// Removing a key is always allowed for the owner's devices: it only closes a door.
    pub fn remove(&self, key_id: &str) -> Option<ApproverKey> {
        self.keys
            .write()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
            .remove(key_id)
    }
}

#[cfg(test)]
pub(crate) mod test_keys {
    //! A software P-256 key standing in for a device's hardware key in tests.

    use super::*;
    use ghostex_gx_protocol::approval::ApproverPresence;
    use ring::{
        rand::SystemRandom,
        signature::{EcdsaKeyPair, KeyPair, ECDSA_P256_SHA256_ASN1_SIGNING},
    };

    pub(crate) struct TestApprover {
        pair: EcdsaKeyPair,
        pub(crate) key: ApproverKey,
    }

    impl TestApprover {
        pub(crate) fn new(label: &str) -> Self {
            let rng = SystemRandom::new();
            let pkcs8 = EcdsaKeyPair::generate_pkcs8(&ECDSA_P256_SHA256_ASN1_SIGNING, &rng)
                .expect("generate key");
            let pair =
                EcdsaKeyPair::from_pkcs8(&ECDSA_P256_SHA256_ASN1_SIGNING, pkcs8.as_ref(), &rng)
                    .expect("parse key");
            let public_key = pair.public_key().as_ref().to_vec();
            let key = ApproverKey {
                key_id: approver_key_id(&public_key),
                device_id: format!("dev-{label}"),
                label: label.to_string(),
                platform: "macos".into(),
                algorithm: ApproverKeyAlgorithm::EcdsaP256Sha256,
                public_key: STANDARD.encode(&public_key),
                presence: ApproverPresence::TouchId,
                added_at_ms: 0,
            };
            Self { pair, key }
        }

        pub(crate) fn sign(&self, payload: &[u8]) -> String {
            let signature = self.pair.sign(&SystemRandom::new(), payload).expect("sign");
            STANDARD.encode(signature.as_ref())
        }
    }
}

#[cfg(test)]
mod tests {
    use super::test_keys::TestApprover;
    use super::*;

    #[test]
    fn p256_signature_verifies_and_a_changed_payload_does_not() {
        let approver = TestApprover::new("mac");
        let signature = approver.sign(b"payload");
        assert_eq!(
            verify_approval_signature(&approver.key, b"payload", &signature),
            Ok(())
        );
        assert_eq!(
            verify_approval_signature(&approver.key, b"payload2", &signature),
            Err(ApproverKeyError::SignatureMismatch)
        );
    }

    #[test]
    fn insert_recomputes_the_key_id_and_rejects_bad_keys() {
        let keys = ApproverKeys::default();
        let mut key = TestApprover::new("mac").key;
        let expected = key.key_id.clone();
        key.key_id = "chosen-by-caller".into();
        assert_eq!(keys.insert(key.clone()).unwrap().key_id, expected);
        assert!(keys.get(&expected).is_some());

        key.public_key = STANDARD.encode([4_u8; 10]);
        assert_eq!(keys.insert(key), Err(ApproverKeyError::BadPublicKey));
    }
}
