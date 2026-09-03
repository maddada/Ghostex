use std::{fs, io, path::Path};

use anyhow::{bail, Context, Result};
use axum::http::{header, HeaderMap};
use base64::{engine::general_purpose::URL_SAFE_NO_PAD, Engine};
use rand::RngCore;
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
    rand::thread_rng().fill_bytes(&mut random);
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

pub fn is_expected_gxserver_auth_token(provided_token: &str, expected_token: &str) -> bool {
    provided_token.len() == expected_token.len()
        && provided_token
            .as_bytes()
            .ct_eq(expected_token.as_bytes())
            .into()
}

fn bearer_token(headers: &HeaderMap) -> Option<&str> {
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
