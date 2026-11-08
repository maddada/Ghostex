//! The desktop app's keys for GitHub #495: the **owner key** that opens the app's owner session
//! with gxserver, and the **approver key** that signs approvals, both P-256, both kept in the OS
//! key store, never in a file or a session's environment.
//!
//! - macOS: the login Keychain (an item only the Ghostex app that created it reads without the
//!   user's keychain password); approvals ask Touch ID or the Mac password (LocalAuthentication).
//! - Windows: Credential Manager; approvals ask Windows Hello (`UserConsentVerifier`).
//! - Linux: the Secret Service; approvals ask the system password through polkit.
//!
//! On Windows and Linux any program running as the user can read these key stores, which Help
//! says plainly (user decision 1A of #495). The approver key can only be loaded through
//! [`unlock_approver_key`], which runs the presence prompt first.

use base64::{engine::general_purpose::STANDARD, Engine};
use ring::{
    rand::SystemRandom,
    signature::{EcdsaKeyPair, KeyPair, ECDSA_P256_SHA256_ASN1_SIGNING},
};

mod platform;

/// Which of the desktop's two keys.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum KeyRole {
    /// Opens the owner session; used without a prompt at every start.
    Owner,
    /// Signs approvals; only after the presence prompt.
    Approver,
}

impl KeyRole {
    /// The key store entry's name.
    pub fn account(self) -> &'static str {
        match self {
            Self::Owner => "owner-key",
            Self::Approver => "approver-key",
        }
    }
}

/// What proved the user was there, as gxserver's `ApproverPresence` names it.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Presence {
    TouchId,
    MacPassword,
    WindowsHello,
    Polkit,
}

impl Presence {
    pub fn wire_name(self) -> &'static str {
        match self {
            Self::TouchId => "touchId",
            Self::MacPassword => "macPassword",
            Self::WindowsHello => "windowsHello",
            Self::Polkit => "polkit",
        }
    }
}

/// Why a key or the presence prompt failed, in words for a toast.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum KeyError {
    /// The user closed or failed the prompt.
    Cancelled,
    /// This computer has no way to prove presence (no Touch ID, password, Windows Hello or
    /// polkit agent).
    PresenceUnavailable(String),
    KeyStore(String),
    BadKey,
}

impl std::fmt::Display for KeyError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Cancelled => f.write_str("You didn't approve it."),
            Self::PresenceUnavailable(detail) => write!(
                f,
                "This computer can't confirm it's you ({detail}). Approve it on your phone."
            ),
            Self::KeyStore(detail) => {
                write!(f, "Ghostex couldn't reach the system key store: {detail}")
            }
            Self::BadKey => f.write_str("The saved Ghostex key can't be read."),
        }
    }
}

impl std::error::Error for KeyError {}

/// A P-256 key loaded from the key store.
pub struct DeviceKey {
    pair: EcdsaKeyPair,
}

impl DeviceKey {
    fn from_pkcs8(pkcs8: &[u8]) -> Result<Self, KeyError> {
        EcdsaKeyPair::from_pkcs8(&ECDSA_P256_SHA256_ASN1_SIGNING, pkcs8, &SystemRandom::new())
            .map(|pair| Self { pair })
            .map_err(|_| KeyError::BadKey)
    }

    fn generate() -> Result<(Self, Vec<u8>), KeyError> {
        let rng = SystemRandom::new();
        let pkcs8 = EcdsaKeyPair::generate_pkcs8(&ECDSA_P256_SHA256_ASN1_SIGNING, &rng)
            .map_err(|_| KeyError::BadKey)?;
        let key = Self::from_pkcs8(pkcs8.as_ref())?;
        Ok((key, pkcs8.as_ref().to_vec()))
    }

    /// The 65-byte uncompressed point, base64 standard (gxserver's `publicKey`).
    pub fn public_key_b64(&self) -> String {
        STANDARD.encode(self.pair.public_key().as_ref())
    }

    /// An ASN.1 DER ECDSA signature over `payload`, base64 standard.
    pub fn sign_b64(&self, payload: &[u8]) -> Result<String, KeyError> {
        self.pair
            .sign(&SystemRandom::new(), payload)
            .map(|signature| STANDARD.encode(signature.as_ref()))
            .map_err(|_| KeyError::BadKey)
    }
}

/// The owner key, made and saved the first time.
pub fn load_or_create_owner_key() -> Result<DeviceKey, KeyError> {
    load_or_create(KeyRole::Owner)
}

/// The approver key, made and saved the first time, after the presence prompt shows `reason`
/// (the approval card's summary). Returns what proved the user was there.
pub fn unlock_approver_key(reason: &str) -> Result<(DeviceKey, Presence), KeyError> {
    let presence = platform::confirm_presence(reason)?;
    Ok((load_or_create(KeyRole::Approver)?, presence))
}

/// The approver key's public half without a prompt (for showing which key is enrolled); `None`
/// when it was never made.
pub fn approver_public_key() -> Result<Option<String>, KeyError> {
    platform::read_secret(KeyRole::Approver.account())?
        .map(|pkcs8| DeviceKey::from_pkcs8(&pkcs8).map(|key| key.public_key_b64()))
        .transpose()
}

/// `macos`, `windows` or `linux`, as gxserver's `ApproverKey.platform` names it.
pub fn platform_name() -> &'static str {
    if cfg!(target_os = "macos") {
        "macos"
    } else if cfg!(target_os = "windows") {
        "windows"
    } else {
        "linux"
    }
}

/// What the presence prompt on this platform is called, for an approver key's `presence` before
/// the first prompt ran.
pub fn default_presence() -> Presence {
    if cfg!(target_os = "macos") {
        Presence::TouchId
    } else if cfg!(target_os = "windows") {
        Presence::WindowsHello
    } else {
        Presence::Polkit
    }
}

fn load_or_create(role: KeyRole) -> Result<DeviceKey, KeyError> {
    if let Some(pkcs8) = platform::read_secret(role.account())? {
        return DeviceKey::from_pkcs8(&pkcs8);
    }
    let (key, pkcs8) = DeviceKey::generate()?;
    platform::write_secret(role.account(), &pkcs8)?;
    // Read it back, so a key store that dropped the write never leaves two keys in play.
    match platform::read_secret(role.account())? {
        Some(saved) if saved == pkcs8 => Ok(key),
        _ => Err(KeyError::KeyStore("the new key wasn't saved".to_string())),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use ring::signature::{UnparsedPublicKey, ECDSA_P256_SHA256_ASN1};

    #[test]
    fn a_generated_key_signs_what_gxserver_verifies() {
        let (key, pkcs8) = DeviceKey::generate().unwrap();
        let again = DeviceKey::from_pkcs8(&pkcs8).unwrap();
        assert_eq!(key.public_key_b64(), again.public_key_b64());
        let public = STANDARD.decode(key.public_key_b64()).unwrap();
        assert_eq!(public.len(), 65);
        let signature = STANDARD.decode(key.sign_b64(b"payload").unwrap()).unwrap();
        UnparsedPublicKey::new(&ECDSA_P256_SHA256_ASN1, &public)
            .verify(b"payload", &signature)
            .unwrap();
    }
}
