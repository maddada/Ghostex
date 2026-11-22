//! The secret vault: AES-256-GCM sealing with a master key in the macOS Keychain (a 0600 key file
//! elsewhere), ported from Open Dot's `vault.ts`. Connectors seal their sign-ins with it (the
//! Composio tokens of each workspace); plaintext only leaves this module to call that provider.
//! Nothing here logs, and no error message carries a secret.
//!
//! CDXC:Connectors 2026-10-15 WHY:
//! A Composio sign-in reaches the user's mail, chat and files, so it is sealed with a Keychain key instead of sitting in a plain 0600 file the way the Linear key does.

use std::collections::HashMap;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::sync::{Mutex, OnceLock};

use aes_gcm::aead::{Aead, KeyInit};
use aes_gcm::Aes256Gcm;
use base64::engine::general_purpose::STANDARD as BASE64;
use base64::Engine as _;
use rand::Rng;

use crate::paths::GxserverPaths;

/// The Keychain item that holds the master key (service, account `master`).
const KEYCHAIN_SERVICE: &str = "ghostex-vault";
const KEYCHAIN_ACCOUNT: &str = "master";
const SECURITY_TOOL: &str = "/usr/bin/security";
/// `security` exits with errSecItemNotFound's code when the item does not exist yet.
const SECURITY_ITEM_NOT_FOUND_EXIT: i32 = 44;

#[derive(Clone, Copy)]
struct MasterKey {
    bytes: [u8; 32],
    backend: &'static str,
}

/// The master key per gxserver root, read once per process.
fn key_cache() -> &'static Mutex<HashMap<PathBuf, MasterKey>> {
    static CACHE: OnceLock<Mutex<HashMap<PathBuf, MasterKey>>> = OnceLock::new();
    CACHE.get_or_init(|| Mutex::new(HashMap::new()))
}

fn key_file(paths: &GxserverPaths) -> PathBuf {
    paths.auth_dir.join("vault.key")
}

/// Where the master key lives, for Settings: `"keychain"` or `"file"`.
pub(crate) fn vault_backend(paths: &GxserverPaths) -> &'static str {
    if let Some(key) = key_cache()
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
        .get(&paths.root_dir)
    {
        return key.backend;
    }
    // Not loaded yet: report where `master_key` will look first, without creating anything
    // (Settings polls this, and it must not spawn `security` every few seconds).
    if key_file(paths).is_file() || !cfg!(target_os = "macos") {
        "file"
    } else {
        "keychain"
    }
}

/// Encrypts any secret with the vault key: `<iv b64>.<tag b64>.<data b64>` (Open Dot's format).
pub(crate) fn seal(paths: &GxserverPaths, plain: &str) -> Result<String, String> {
    let key = master_key(paths)?;
    let cipher = Aes256Gcm::new_from_slice(&key.bytes)
        .map_err(|_| "The vault key is not valid.".to_string())?;
    let mut iv = [0u8; 12];
    rand::rng().fill_bytes(&mut iv);
    let nonce = aes_gcm::aead::Nonce::<Aes256Gcm>::try_from(&iv[..])
        .map_err(|_| "Couldn't seal the secret.".to_string())?;
    let mut sealed = cipher
        .encrypt(&nonce, plain.as_bytes())
        .map_err(|_| "Couldn't seal the secret.".to_string())?;
    // aes-gcm appends the 16-byte tag; Open Dot's format stores it as its own part.
    let tag = sealed.split_off(sealed.len() - 16);
    Ok(format!(
        "{}.{}.{}",
        BASE64.encode(iv),
        BASE64.encode(tag),
        BASE64.encode(sealed)
    ))
}

/// Why a sealed secret didn't open.
pub(crate) enum UnsealError {
    /// It never opens with this computer's vault key: it is damaged, or the vault key changed since
    /// it was sealed. Saving it again is the only fix; callers word that for their own secret.
    Unreadable,
    /// The vault key itself couldn't be loaded (a locked Keychain, for example), so the secret
    /// may still open later; the text says what to do.
    Vault(String),
}

pub(crate) fn unseal(paths: &GxserverPaths, sealed: &str) -> Result<String, UnsealError> {
    let mut parts = sealed.trim().split('.');
    let (Some(iv), Some(tag), Some(data), None) =
        (parts.next(), parts.next(), parts.next(), parts.next())
    else {
        return Err(UnsealError::Unreadable);
    };
    let unreadable = |_| UnsealError::Unreadable;
    let iv = BASE64.decode(iv).map_err(unreadable)?;
    let tag = BASE64.decode(tag).map_err(unreadable)?;
    let mut data = BASE64.decode(data).map_err(unreadable)?;
    if iv.len() != 12 || tag.len() != 16 {
        return Err(UnsealError::Unreadable);
    }
    let key = master_key(paths).map_err(UnsealError::Vault)?;
    let cipher = Aes256Gcm::new_from_slice(&key.bytes).map_err(|_| UnsealError::Unreadable)?;
    let nonce = aes_gcm::aead::Nonce::<Aes256Gcm>::try_from(&iv[..])
        .map_err(|_| UnsealError::Unreadable)?;
    data.extend_from_slice(&tag);
    let plain = cipher
        .decrypt(&nonce, data.as_slice())
        .map_err(|_| UnsealError::Unreadable)?;
    String::from_utf8(plain).map_err(|_| UnsealError::Unreadable)
}

/// The cached master key, loading (and on first use creating) it.
///
/// CDXC:Connectors 2026-10-15 WHY:
/// An existing key file wins over the Keychain, and the Keychain key is only created when `security` reports the item missing (exit 44), never after another failure such as a locked keychain. Open Dot created a fresh key whenever the lookup failed, which silently orphans every secret sealed with the old key; mixing a file key and a Keychain key would do the same.
fn master_key(paths: &GxserverPaths) -> Result<MasterKey, String> {
    let mut cache = key_cache()
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    if let Some(key) = cache.get(&paths.root_dir) {
        return Ok(*key);
    }
    let file = key_file(paths);
    let key = if file.is_file() {
        MasterKey {
            bytes: read_key_file(&file)?,
            backend: "file",
        }
    } else if cfg!(target_os = "macos") {
        match keychain_master_key()? {
            Some(bytes) => MasterKey {
                bytes,
                backend: "keychain",
            },
            None => MasterKey {
                bytes: create_key_file(&file)?,
                backend: "file",
            },
        }
    } else {
        MasterKey {
            bytes: create_key_file(&file)?,
            backend: "file",
        }
    };
    cache.insert(paths.root_dir.clone(), key);
    Ok(key)
}

/// The Keychain key: read it, or create it when it does not exist. `Ok(None)` means the Keychain
/// cannot store it here, so the caller uses a key file instead.
fn keychain_master_key() -> Result<Option<[u8; 32]>, String> {
    match keychain_find()? {
        KeychainLookup::Found(hex) => {
            return parse_hex_key(&hex)
                .map(Some)
                .ok_or_else(|| "The vault key in the Keychain is damaged.".to_string());
        }
        KeychainLookup::Missing => {}
    }
    let mut bytes = [0u8; 32];
    rand::rng().fill_bytes(&mut bytes);
    if !keychain_add(&hex_encode(&bytes)) {
        return Ok(None);
    }
    // Read it back so the key in use is the one the Keychain really holds.
    match keychain_find()? {
        KeychainLookup::Found(hex) => Ok(parse_hex_key(&hex)),
        KeychainLookup::Missing => Ok(None),
    }
}

enum KeychainLookup {
    Found(String),
    Missing,
}

fn keychain_find() -> Result<KeychainLookup, String> {
    let output = std::process::Command::new(SECURITY_TOOL)
        .args([
            "find-generic-password",
            "-s",
            KEYCHAIN_SERVICE,
            "-a",
            KEYCHAIN_ACCOUNT,
            "-w",
        ])
        .stdin(std::process::Stdio::null())
        .output()
        .map_err(|error| format!("Couldn't reach the Keychain: {error}"))?;
    if output.status.success() {
        return Ok(KeychainLookup::Found(
            String::from_utf8_lossy(&output.stdout).trim().to_string(),
        ));
    }
    if output.status.code() == Some(SECURITY_ITEM_NOT_FOUND_EXIT) {
        return Ok(KeychainLookup::Missing);
    }
    Err(format!(
        "Couldn't read the Ghostex vault key from the Keychain ({}). Unlock the Keychain and try again.",
        security_failure_detail(&output.status, &output.stderr)
    ))
}

/// CDXC:ServerDaemon 2026-10-25 WHY:
/// A gxserver started from an SSH login runs outside the user's GUI security session, where `security` fails without writing anything; the error read "Keychain ()" and said nothing about why. The exit status (or the signal that ended the tool) always goes in.
fn security_failure_detail(status: &std::process::ExitStatus, stderr: &[u8]) -> String {
    let ended = match status.code() {
        Some(code) => format!("security exited with status {code}"),
        None => {
            #[cfg(unix)]
            {
                use std::os::unix::process::ExitStatusExt as _;
                match status.signal() {
                    Some(signal) => format!("security was ended by signal {signal}"),
                    None => "security ended without an exit status".to_string(),
                }
            }
            #[cfg(not(unix))]
            {
                "security ended without an exit status".to_string()
            }
        }
    };
    let stderr = String::from_utf8_lossy(stderr);
    match stderr.trim() {
        "" => format!("{ended} and printed nothing"),
        message => format!("{ended}: {message}"),
    }
}

/// Adds the key through `security -i` so the hex key goes over stdin instead of the command line,
/// where any process listing could read it.
fn keychain_add(hex: &str) -> bool {
    let Ok(mut child) = std::process::Command::new(SECURITY_TOOL)
        .arg("-i")
        .stdin(std::process::Stdio::piped())
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .spawn()
    else {
        return false;
    };
    let command =
        format!("add-generic-password -U -s {KEYCHAIN_SERVICE} -a {KEYCHAIN_ACCOUNT} -w {hex}\n");
    let wrote = child
        .stdin
        .take()
        .map(|mut stdin| stdin.write_all(command.as_bytes()).is_ok())
        .unwrap_or(false);
    let exited = child.wait().map(|status| status.success()).unwrap_or(false);
    wrote && exited
}

fn read_key_file(file: &Path) -> Result<[u8; 32], String> {
    let text = std::fs::read_to_string(file)
        .map_err(|error| format!("Couldn't read the vault key file: {error}"))?;
    parse_hex_key(text.trim()).ok_or_else(|| "The vault key file is damaged.".to_string())
}

fn create_key_file(file: &Path) -> Result<[u8; 32], String> {
    if let Some(parent) = file.parent() {
        std::fs::create_dir_all(parent)
            .map_err(|error| format!("Couldn't create the vault folder: {error}"))?;
    }
    let mut bytes = [0u8; 32];
    rand::rng().fill_bytes(&mut bytes);
    let mut options = std::fs::OpenOptions::new();
    options.write(true).create_new(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.mode(0o600);
    }
    match options.open(file) {
        Ok(mut handle) => {
            handle
                .write_all(hex_encode(&bytes).as_bytes())
                .map_err(|error| format!("Couldn't write the vault key file: {error}"))?;
            Ok(bytes)
        }
        // Another thread or process created it first: use theirs.
        Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => read_key_file(file),
        Err(error) => Err(format!("Couldn't create the vault key file: {error}")),
    }
}

fn hex_encode(bytes: &[u8]) -> String {
    bytes.iter().map(|byte| format!("{byte:02x}")).collect()
}

fn parse_hex_key(hex: &str) -> Option<[u8; 32]> {
    if hex.len() != 64 || !hex.is_ascii() {
        return None;
    }
    let mut bytes = [0u8; 32];
    for (index, byte) in bytes.iter_mut().enumerate() {
        *byte = u8::from_str_radix(&hex[index * 2..index * 2 + 2], 16).ok()?;
    }
    Some(bytes)
}

#[cfg(all(test, unix))]
mod security_failure_detail_tests {
    use super::security_failure_detail;
    use std::os::unix::process::ExitStatusExt as _;
    use std::process::ExitStatus;

    #[test]
    fn a_silent_failure_still_names_its_exit_status() {
        assert_eq!(
            security_failure_detail(&ExitStatus::from_raw(51 << 8), b""),
            "security exited with status 51 and printed nothing"
        );
    }

    #[test]
    fn the_tool_output_follows_the_status() {
        assert_eq!(
            security_failure_detail(
                &ExitStatus::from_raw(36 << 8),
                b"security: SecKeychainSearchCopyNext: User interaction is not allowed.\n"
            ),
            "security exited with status 36: security: SecKeychainSearchCopyNext: User interaction is not allowed."
        );
    }

    #[test]
    fn a_signal_is_named() {
        assert_eq!(
            security_failure_detail(&ExitStatus::from_raw(9), b""),
            "security was ended by signal 9 and printed nothing"
        );
    }
}
