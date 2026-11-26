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
//!
//! A test copy launched with `GHOSTEX_OWNER_KEYS_STORE=file` keeps them in files inside its own
//! `GHOSTEX_HOME` instead and never calls the OS key store or the presence prompt (`store.rs`).

use base64::{engine::general_purpose::STANDARD, Engine};
use ring::{
    rand::SystemRandom,
    signature::{EcdsaKeyPair, KeyPair, ECDSA_P256_SHA256_ASN1_SIGNING},
};

mod platform;
mod store;

use store::Store;
pub use store::{FILE_STORE, FILE_STORE_FOLDER, STORE_VARIABLE};

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
    /// The Windows account password, where Windows Hello isn't set up.
    WindowsPassword,
    Polkit,
}

impl Presence {
    pub fn wire_name(self) -> &'static str {
        match self {
            Self::TouchId => "touchId",
            Self::MacPassword => "macPassword",
            Self::WindowsHello => "windowsHello",
            Self::WindowsPassword => "windowsPassword",
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

/// Whether this process keeps its keys in the file store (a test copy started with
/// `GHOSTEX_OWNER_KEYS_STORE=file` and its own `GHOSTEX_HOME`). Such keys have no presence prompt
/// and any program of the user can read them, so the app says so when it hands them to gxserver.
pub fn uses_file_store() -> bool {
    matches!(Store::current(), Ok(Store::File(_)))
}

/// Whether this process was started as a test copy's or test server's: `GHOSTEX_OWNER_KEYS_STORE=file`
/// together with a valid absolute `GHOSTEX_HOME`, the same test [`uses_file_store`] makes (without
/// looking at the disk). gxserver takes a key that declares the file store only then; a real
/// install's gxserver has no `GHOSTEX_HOME`, so the variable alone never qualifies it (review
/// round 2, R2-1).
pub fn opted_in_to_file_store() -> bool {
    matches!(
        Store::from_environment(
            std::env::var_os(STORE_VARIABLE),
            std::env::var_os("GHOSTEX_HOME"),
            |_| false,
        ),
        Ok(Store::File(_))
    )
}

/// Why no key can be used at all in this process (a bad `GHOSTEX_OWNER_KEYS_STORE`, or a test
/// copy's home started without it), in words for a warning; `None` when the store is usable.
pub fn store_problem() -> Option<String> {
    Store::current().err().map(|error| error.to_string())
}

/// The owner key, made and saved the first time.
pub fn load_or_create_owner_key() -> Result<DeviceKey, KeyError> {
    load_or_create(KeyRole::Owner)
}

/*
CDXC:Security 2026-11-16 FIX:
Review B1 of the owner-only door: with one owner key for every computer, whatever answered on a connected computer's tunnel could hand the app this computer's server id and a challenge from here, and the app's signature opened an owner session here. Each connected computer now gets an owner key of its own (`scope` names it), enrolled only there, so a signature the app makes for one computer opens nothing anywhere else. This computer keeps [`load_or_create_owner_key`].
*/
/// A random value made once and kept in the key store, never sent anywhere: the app mixes it into
/// the names it gives connected computers' keys and sessions (review round 2, R2-1), so those names
/// can't be worked out or lined up from outside.
pub fn local_salt() -> Result<Vec<u8>, KeyError> {
    const ACCOUNT: &str = "owner-key-salt";
    let store = Store::current()?;
    if let Some(salt) = store.read_secret(ACCOUNT)? {
        return Ok(salt);
    }
    let mut salt = vec![0_u8; 32];
    ring::rand::SecureRandom::fill(&SystemRandom::new(), &mut salt)
        .map_err(|_| KeyError::KeyStore("no random numbers".to_string()))?;
    store.write_secret(ACCOUNT, &salt)?;
    match store.read_secret(ACCOUNT)? {
        Some(saved) => Ok(saved),
        None => Err(KeyError::KeyStore("the salt wasn't saved".to_string())),
    }
}

/// The owner key for one connected computer (`scope`: a short name for it, letters, digits and
/// `-` only), made and saved the first time.
pub fn load_or_create_scoped_owner_key(scope: &str) -> Result<DeviceKey, KeyError> {
    if scope.is_empty() || !scope.chars().all(|c| c.is_ascii_alphanumeric() || c == '-') {
        return Err(KeyError::KeyStore("invalid key name".to_string()));
    }
    load_or_create_account(&format!("{}-{scope}", KeyRole::Owner.account()))
}

/// The owner key kept for one connected computer (as [`load_or_create_scoped_owner_key`] names
/// it), without making one: `None` when there is none.
pub fn load_scoped_owner_key(scope: &str) -> Result<Option<DeviceKey>, KeyError> {
    if scope.is_empty() || !scope.chars().all(|c| c.is_ascii_alphanumeric() || c == '-') {
        return Err(KeyError::KeyStore("invalid key name".to_string()));
    }
    Store::current()?
        .read_secret(&format!("{}-{scope}", KeyRole::Owner.account()))?
        .map(|pkcs8| DeviceKey::from_pkcs8(&pkcs8))
        .transpose()
}

/// The approver key, made and saved the first time, after the presence prompt shows `reason`
/// (the approval card's summary). Returns what proved the user was there.
pub fn unlock_approver_key(reason: &str) -> Result<(DeviceKey, Presence), KeyError> {
    let presence = Store::current()?.confirm_presence(reason)?;
    Ok((load_or_create(KeyRole::Approver)?, presence))
}

/// The system's presence prompt alone, showing `reason`: for a change the app makes itself that
/// needs the owner there (a security-relaxing setting saved in the Settings window).
pub fn confirm_presence(reason: &str) -> Result<Presence, KeyError> {
    Store::current()?.confirm_presence(reason)
}

/// The approver key's public half without a prompt (for showing which key is enrolled); `None`
/// when it was never made.
pub fn approver_public_key() -> Result<Option<String>, KeyError> {
    Store::current()?
        .read_secret(KeyRole::Approver.account())?
        .map(|pkcs8| DeviceKey::from_pkcs8(&pkcs8).map(|key| key.public_key_b64()))
        .transpose()
}

/// The approver key's public half, made and saved first when it was never made, without a prompt:
/// the app sends it with its owner key, so the one approval that lets it act as the owner on a
/// connected computer also lets it answer requests there. Signing with it still needs
/// [`unlock_approver_key`].
pub fn approver_public_key_or_create() -> Result<String, KeyError> {
    Ok(load_or_create(KeyRole::Approver)?.public_key_b64())
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
    load_or_create_account(role.account())
}

fn load_or_create_account(account: &str) -> Result<DeviceKey, KeyError> {
    let store = Store::current()?;
    if let Some(pkcs8) = store.read_secret(account)? {
        return DeviceKey::from_pkcs8(&pkcs8);
    }
    let (key, pkcs8) = DeviceKey::generate()?;
    store.write_secret(account, &pkcs8)?;
    // Read it back, so a key store that dropped the write never leaves two keys in play.
    match store.read_secret(account)? {
        Some(saved) if saved == pkcs8 => Ok(key),
        _ => Err(KeyError::KeyStore("the new key wasn't saved".to_string())),
    }
}
