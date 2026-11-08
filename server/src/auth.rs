use std::{
    collections::HashMap,
    fs, io,
    path::Path,
    sync::{Arc, RwLock},
};

use anyhow::{bail, Context, Result};
use axum::http::{header, HeaderMap};
use base64::{engine::general_purpose::URL_SAFE_NO_PAD, Engine};
use rand::Rng;
use sha2::{Digest, Sha256};
use subtle::ConstantTimeEq;

use crate::paths::GxserverPaths;

const TOKEN_BYTES: usize = 32;
const TOKEN_FILE_MODE: u32 = 0o600;
const AUTH_DIR_MODE: u32 = 0o700;

#[derive(Clone, Debug)]
pub struct GxserverAuthState {
    pub token: String,
}

/*
CDXC:ServerApi 2026-06-14-20:37:
The gxserver auth contract stores a 32-byte base64url token in the resolved Ghostex state directory; the directory is 0700, the token file is 0600, and comparisons are constant-time.

CDXC:ServerApi 2026-06-22-04:10:
Authorization parsing must mirror Node's `authorization.split(" ")` behavior in TypeScript gxserver: use the first space-delimited token after `Bearer`, ignore later segments, and treat an empty second segment as missing auth.
*/
pub fn ensure_gxserver_auth_token(paths: &GxserverPaths) -> Result<GxserverAuthState> {
    fs::create_dir_all(&paths.auth_dir).with_context(|| "create gxserver auth directory")?;
    chmod_if_supported(&paths.auth_dir, AUTH_DIR_MODE)?;

    if let Some(existing) = read_gxserver_auth_token(paths)? {
        chmod_if_supported(&paths.auth_token_file, TOKEN_FILE_MODE)?;
        return Ok(existing);
    }

    let mut random = [0_u8; TOKEN_BYTES];
    rand::rng().fill_bytes(&mut random);
    let token = URL_SAFE_NO_PAD.encode(random);
    write_new_file_with_mode(
        &paths.auth_token_file,
        format!("{token}\n").as_bytes(),
        TOKEN_FILE_MODE,
    )
    .with_context(|| "write gxserver auth token")?;
    Ok(GxserverAuthState { token })
}

pub fn read_gxserver_auth_token(paths: &GxserverPaths) -> Result<Option<GxserverAuthState>> {
    match fs::read_to_string(&paths.auth_token_file) {
        Ok(text) => {
            let token = text.trim().to_string();
            if !is_valid_auth_token(&token) {
                bail!(
                    "Invalid gxserver auth token file at {}.",
                    paths.auth_token_file.display()
                );
            }
            Ok(Some(GxserverAuthState { token }))
        }
        Err(error) if error.kind() == io::ErrorKind::NotFound => Ok(None),
        Err(error) => Err(error).with_context(|| "read gxserver auth token"),
    }
}

pub fn is_authorized_headers(headers: &HeaderMap, expected_token: &str) -> bool {
    let Some(provided) = bearer_token(headers) else {
        return false;
    };
    is_expected_gxserver_auth_token(provided, expected_token)
}

/// The main token in the `Authorization` header, a paired phone's own token, or the desktop's
/// owner session.
pub fn is_authorized_client_headers(
    headers: &HeaderMap,
    expected_token: &str,
    device_tokens: &DeviceApiTokens,
    owner_sessions: &crate::security::SessionTokens,
) -> bool {
    bearer_token(headers).is_some_and(|provided| {
        is_authorized_client_token(provided, expected_token, device_tokens, owner_sessions)
    })
}

/*
CDXC:Security 2026-11-08 WHY:
After #495 phase 2 the desktop sends its `gxo_` owner session instead of the token file on every call to this computer's gxserver, its sidebar store's `/api/events` socket included. The sockets accepted only the token file and phone tokens, so the store's socket was refused (401) as soon as the owner session opened and the sidebar lost every local project and session while remote ones stayed. The owner session holds more than the token file, so every socket the token file opens opens for it too. A session's own `gxs_` token is still refused here, as before.
*/
/// The main token, a paired phone's own token, or the desktop's owner session (`gxo_`).
pub fn is_authorized_client_token(
    provided_token: &str,
    expected_token: &str,
    device_tokens: &DeviceApiTokens,
    owner_sessions: &crate::security::SessionTokens,
) -> bool {
    is_expected_gxserver_auth_token(provided_token, expected_token)
        || device_tokens.accepts(provided_token)
        || owner_sessions.is_owner_session(provided_token)
}

/*
CDXC:Security 2026-11-08 WHY:
The `ghostex` CLI inside a Ghostex session (an agent's or the user's own terminal) sends that session's `gxs_` token, not the token file (`ghostex_cli/rpc.rs`), so `ghostex voice` and `ghostex browser mcp` were refused on `/api/voice` and the `/api/cefDevtools` pages. Those two take a session token as well: it is agent level, as the token file that opened them before is. The other sockets (events, terminal, browserTcp, the DevTools host) have no client inside a session and keep refusing it.
*/
/// [`is_authorized_client_headers`], or an open session's own `gxs_` token.
pub fn is_authorized_session_client_headers(
    headers: &HeaderMap,
    expected_token: &str,
    device_tokens: &DeviceApiTokens,
    session_tokens: &crate::security::SessionTokens,
) -> bool {
    bearer_token(headers).is_some_and(|provided| {
        is_authorized_session_client_token(provided, expected_token, device_tokens, session_tokens)
    })
}

/// [`is_authorized_client_token`], or an open session's own `gxs_` token.
pub fn is_authorized_session_client_token(
    provided_token: &str,
    expected_token: &str,
    device_tokens: &DeviceApiTokens,
    session_tokens: &crate::security::SessionTokens,
) -> bool {
    is_authorized_client_token(
        provided_token,
        expected_token,
        device_tokens,
        session_tokens,
    ) || session_tokens.is_session_token(provided_token)
}

/// Prefix of the API tokens `/api/pairDevice` gives each paired phone.
pub const DEVICE_API_TOKEN_PREFIX: &str = "gxd_";

/*
CDXC:RemotePairing 2026-10-25 DECISION:
User (security audit, Q4 "per-device tokens and a tunnel allow-list"): each phone paired through Easy Connect gets its own API token, and removing the phone from Paired devices revokes it. Before this every phone read the computer's main token over SSH and kept it, so a removed phone (or anyone who copied its token) could still call gxserver through the Easy Connect address. Only the token's SHA-256 is stored, next to the device row; this set mirrors those hashes so a request is checked without a database read, and it is refreshed on every pair and remove. A device token opens the same API the main token does: what it adds is that it can be taken away on its own.
SEE-ALSO: server/src/remote_access/pair_device.rs (minting), server/src/remote_access/paired_devices.rs (revoking), apps/mobile/app/src/chat/gpui/endpoint.ts (the phone's use).
*/
#[derive(Clone, Default)]
pub struct DeviceApiTokens {
    entries: Arc<RwLock<HashMap<String, DeviceTokenEntry>>>,
}

/*
CDXC:RemotePairing 2026-11-08 DECISION:
User (GitHub #495, Q1721): pairing v2 binds each phone's token to the device key it paired with. The token is issued only after the owner approved that key, names its device (so the caller is `Principal::Device` with the real device id), and lapses after `DEVICE_API_TOKEN_TTL` unless the device key signs a renewal (`/api/renewDeviceToken`), so a copied token stops working on its own. Tokens of phones paired before 11.0.1 are not loaded at all.
*/
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct DeviceTokenEntry {
    /// The paired device the token belongs to; `None` only in tests that load bare hashes.
    pub device_id: Option<String>,
    /// Milliseconds since the epoch after which the token no longer opens anything.
    pub expires_at_ms: Option<u64>,
}

/// How long a device token works before the device key must renew it.
pub const DEVICE_API_TOKEN_TTL: chrono::Duration = chrono::Duration::days(30);

impl DeviceApiTokens {
    pub fn new(hashes: impl IntoIterator<Item = String>) -> Self {
        let tokens = Self::default();
        tokens.replace(hashes);
        tokens
    }

    /// Loads bare hashes that never lapse and name no device (tests and old fixtures).
    pub fn replace(&self, hashes: impl IntoIterator<Item = String>) {
        self.replace_entries(
            hashes
                .into_iter()
                .map(|hash| (hash, DeviceTokenEntry::default())),
        );
    }

    pub fn replace_entries(&self, entries: impl IntoIterator<Item = (String, DeviceTokenEntry)>) {
        let mut map = self
            .entries
            .write()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        *map = entries.into_iter().collect();
    }

    pub fn accepts(&self, provided_token: &str) -> bool {
        self.entry_for(provided_token).is_some()
    }

    /// The live entry for `provided_token`, if it is a known device token that has not lapsed.
    pub fn entry_for(&self, provided_token: &str) -> Option<DeviceTokenEntry> {
        if !provided_token.starts_with(DEVICE_API_TOKEN_PREFIX) {
            return None;
        }
        let hash = hash_device_api_token(provided_token);
        let now_ms = chrono::Utc::now().timestamp_millis().max(0) as u64;
        self.entries
            .read()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
            .get(&hash)
            .filter(|entry| entry.expires_at_ms.is_none_or(|expires| now_ms < expires))
            .cloned()
    }
}

/// A fresh device token: the prefix and 32 random bytes in base64url.
pub fn mint_device_api_token() -> String {
    let mut random = [0_u8; TOKEN_BYTES];
    rand::rng().fill_bytes(&mut random);
    format!(
        "{DEVICE_API_TOKEN_PREFIX}{}",
        URL_SAFE_NO_PAD.encode(random)
    )
}

pub fn hash_device_api_token(token: &str) -> String {
    hex::encode(Sha256::digest(token.as_bytes()))
}

pub fn is_expected_gxserver_auth_token(provided_token: &str, expected_token: &str) -> bool {
    provided_token.len() == expected_token.len()
        && provided_token
            .as_bytes()
            .ct_eq(expected_token.as_bytes())
            .into()
}

pub fn bearer_token(headers: &HeaderMap) -> Option<&str> {
    let value = headers.get(header::AUTHORIZATION)?.to_str().ok()?;
    let mut parts = value.split(' ');
    let scheme = parts.next()?;
    let token = parts.next()?;
    if scheme.eq_ignore_ascii_case("bearer") && !token.is_empty() {
        Some(token)
    } else {
        None
    }
}

fn is_valid_auth_token(token: &str) -> bool {
    token.len() >= 32
        && token
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || byte == b'_' || byte == b'-')
}

#[cfg(unix)]
fn chmod_if_supported(path: &Path, mode: u32) -> Result<()> {
    use std::os::unix::fs::PermissionsExt;
    fs::set_permissions(path, fs::Permissions::from_mode(mode))
        .with_context(|| format!("chmod {}", path.display()))
}

#[cfg(not(unix))]
fn chmod_if_supported(_path: &Path, _mode: u32) -> Result<()> {
    Ok(())
}

fn write_new_file_with_mode(path: &Path, bytes: &[u8], mode: u32) -> Result<()> {
    #[cfg(unix)]
    {
        use std::io::Write;
        use std::os::unix::fs::OpenOptionsExt;

        let mut file = fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .mode(mode)
            .open(path)?;
        file.write_all(bytes)?;
        chmod_if_supported(path, mode)?;
        Ok(())
    }
    #[cfg(not(unix))]
    {
        fs::write(path, bytes)?;
        let _ = mode;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::paths::get_gxserver_paths;

    #[test]
    fn a_device_token_names_its_device_and_stops_working_when_it_lapses() {
        let tokens = DeviceApiTokens::default();
        let live = mint_device_api_token();
        let lapsed = mint_device_api_token();
        let now_ms = chrono::Utc::now().timestamp_millis() as u64;
        tokens.replace_entries([
            (
                hash_device_api_token(&live),
                DeviceTokenEntry {
                    device_id: Some("d1".into()),
                    expires_at_ms: Some(now_ms + 60_000),
                },
            ),
            (
                hash_device_api_token(&lapsed),
                DeviceTokenEntry {
                    device_id: Some("d2".into()),
                    expires_at_ms: Some(now_ms - 1),
                },
            ),
        ]);
        assert_eq!(
            tokens.entry_for(&live).and_then(|entry| entry.device_id),
            Some("d1".to_string())
        );
        assert!(!tokens.accepts(&lapsed));
        assert!(!tokens.accepts("not-a-device-token"));
    }

    #[test]
    fn authorization_bearer_parsing_matches_typescript_split() {
        let mut headers = HeaderMap::new();
        headers.insert(
            header::AUTHORIZATION,
            "Bearer expected extra".parse().unwrap(),
        );
        assert!(is_authorized_headers(&headers, "expected"));

        headers.insert(header::AUTHORIZATION, "Bearer  expected".parse().unwrap());
        assert!(!is_authorized_headers(&headers, "expected"));

        headers.insert(header::AUTHORIZATION, "Bearer expected".parse().unwrap());
        assert!(is_authorized_headers(&headers, "expected"));
    }

    #[test]
    fn auth_token_is_generated_once_at_gxserver_auth_path() {
        let temp = tempfile::tempdir().expect("tempdir");
        let paths = get_gxserver_paths(Some(temp.path().to_path_buf()));

        let first = ensure_gxserver_auth_token(&paths).expect("first token");
        let second = ensure_gxserver_auth_token(&paths).expect("second token");
        let read = read_gxserver_auth_token(&paths)
            .expect("read token")
            .expect("token");

        assert_eq!(
            paths.auth_token_file,
            temp.path()
                .join(".ghostex")
                .join("state")
                .join("gxserver")
                .join("auth")
                .join("token")
        );
        assert_eq!(second.token, first.token);
        assert_eq!(read.token, first.token);
        assert!(is_valid_auth_token(&first.token));
        assert!(!first.token.contains('='));
    }

    #[cfg(unix)]
    #[test]
    fn auth_token_permissions_are_strict() {
        use std::os::unix::fs::PermissionsExt;

        let temp = tempfile::tempdir().expect("tempdir");
        let paths = get_gxserver_paths(Some(temp.path().to_path_buf()));

        ensure_gxserver_auth_token(&paths).expect("token");
        assert_eq!(
            fs::metadata(&paths.auth_token_file)
                .expect("token metadata")
                .permissions()
                .mode()
                & 0o777,
            TOKEN_FILE_MODE
        );
        assert_eq!(
            fs::metadata(&paths.auth_dir)
                .expect("auth dir metadata")
                .permissions()
                .mode()
                & 0o777,
            AUTH_DIR_MODE
        );

        fs::set_permissions(&paths.auth_token_file, fs::Permissions::from_mode(0o644))
            .expect("loosen token");
        ensure_gxserver_auth_token(&paths).expect("token reused");
        assert_eq!(
            fs::metadata(&paths.auth_token_file)
                .expect("token metadata")
                .permissions()
                .mode()
                & 0o777,
            TOKEN_FILE_MODE
        );
    }
}
