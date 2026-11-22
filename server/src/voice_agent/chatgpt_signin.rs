//! "Use my ChatGPT sign-in" for OpenAI voice: the access token Codex keeps in its `auth.json` after
//! a ChatGPT sign-in, read fresh each time a key is needed, instead of a pasted API key.
//!
//! CDXC:VoiceMode 2026-11-16 DECISION:
//! GitHub #629 asked for "Use my ChatGPT sign-in" for OpenAI voice, and the user picked it. Measured 2026-11-16 with a ChatGPT sign-in on this computer: the token opens `wss://api.openai.com/v1/realtime` (`session.created`) and gets a reply, even for an API organization with no credits left; `/v1/chat/completions` and `/v1/audio/transcriptions` answered 429 "You have no credits remaining", so the call memory summary and OpenAI transcription draw on the API organization's credits as with a pasted key.
//! CDXC:VoiceMode 2026-11-16 WHY:
//! Ghostex only reads the token and never refreshes it. Codex rotates the refresh token on every refresh, so a refresh by Ghostex would leave Codex holding a used one and sign Codex out. Codex refreshes the sign-in itself when it runs (its access tokens last about ten days), so an expired token asks the user to run Codex once.

use std::path::PathBuf;

use base64::{engine::general_purpose::URL_SAFE_NO_PAD, Engine};
use serde_json::Value;

use crate::paths::GxserverPaths;

/// Codex's `auth.json`: in `CODEX_HOME` when gxserver has it set, else the default Codex folder.
fn auth_path(paths: &GxserverPaths) -> PathBuf {
    ghostex_paths::agent_homes::codex_home(&paths.home_dir, true).join("auth.json")
}

fn read_auth(paths: &GxserverPaths) -> Option<Value> {
    serde_json::from_slice(&std::fs::read(auth_path(paths)).ok()?).ok()
}

/// The ChatGPT access token of an `auth.json` document.
fn access_token(auth: &Value) -> Option<&str> {
    auth.pointer("/tokens/access_token")
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|token| !token.is_empty())
}

/// Whether Codex is signed in with ChatGPT on this computer (one small file read, for Settings).
pub(super) fn available(paths: &GxserverPaths) -> bool {
    read_auth(paths).is_some_and(|auth| access_token(&auth).is_some())
}

/// When the token stops working (`exp`, Unix seconds); the claims are read, not verified.
fn expires_at(token: &str) -> Option<i64> {
    let payload = token.split('.').nth(1)?;
    let claims: Value =
        serde_json::from_slice(&URL_SAFE_NO_PAD.decode(payload.trim_end_matches('=')).ok()?)
            .ok()?;
    claims.get("exp")?.as_i64()
}

pub(super) const EXPIRED: &str = "Your ChatGPT sign-in in Codex has expired. Run Codex once so it signs in again, then try again.";

/// The current ChatGPT access token, read now. Blocking (one small file).
pub(super) fn read_token(paths: &GxserverPaths) -> Result<String, String> {
    let auth = read_auth(paths).ok_or_else(|| {
        "Codex isn't signed in with ChatGPT on this computer. Sign in to Codex, or paste an OpenAI API key in Settings > Voice.".to_string()
    })?;
    let token = access_token(&auth).ok_or_else(|| {
        "Codex is signed in with an API key, not ChatGPT. Paste an OpenAI API key in Settings > Voice instead.".to_string()
    })?;
    if expires_at(token).is_some_and(|exp| exp <= chrono::Utc::now().timestamp() + 60) {
        return Err(EXPIRED.to_string());
    }
    Ok(token.to_string())
}
