//! `/api/voiceAgentClip` `{conversationId, offset, chunk (base64), done?}`: a recorded turn for a
//! voice call whose client could not stream its microphone. The client records what was said while
//! Talk was held (Dictate's recorder), sends it in pieces after the release (under 1 MiB each, so the
//! ordinary body limit holds), and the last piece hands it to the call as the user's turn. Answers
//! `{received}`, and `{sent: true}` after `done`. The audio is 24 kHz mono PCM16; a WAV header at the
//! start is skipped.
//!
//! CDXC:VoiceMode 2026-11-10 WHY: On the user's iPhone 15 Pro Max (iOS 27.2, app 11.2.1) every voice call's microphone stopped about 120 ms after it started: switching voice processing on changes the audio layout, and the 11.2.1 native code answers by building a new engine that stops the same way. That code can only change in a new iPhone build, but its Dictate recorder (no voice processing) works, and the phone's JavaScript ships over the air. So a phone whose call microphone keeps stopping records each turn with that recorder and sends it here; phones that never call this keep streaming over `/api/voice` as before. `readVoiceAgent` says `clipTurns: true`.
//! SEE-ALSO: apps/mobile/app/src/voice/clipTalk.ts (the phone's side), `Conversation::clip_turn`.

use std::collections::HashMap;
use std::sync::{Mutex, OnceLock};
use std::time::{Duration, Instant};

use axum::http::StatusCode;
use base64::Engine as _;
use bytes::Bytes;
use serde_json::{json, Value};

use super::conversation::Input;
use super::registry;
use crate::domain::{read_domain_rpc_params, DomainStateError};
use crate::protocol::rpc_success;
use crate::server::{domain_error_response, routed_json, AppState, RoutedResponse};

/// Base64 of a piece may not exceed the 1 MiB JSON limit.
const MAX_PIECE_BYTES: usize = 640 * 1024;
/// Two minutes of audio: a turn longer than that is not a turn.
const MAX_CLIP_BYTES: usize = 2 * 60 * 48_000;
/// A clip whose last piece never came is forgotten after this.
const STALE_AFTER: Duration = Duration::from_secs(120);
const WAV_HEADER_BYTES: usize = 44;

struct Upload {
    bytes: Vec<u8>,
    touched: Instant,
}

fn uploads() -> &'static Mutex<HashMap<String, Upload>> {
    static UPLOADS: OnceLock<Mutex<HashMap<String, Upload>>> = OnceLock::new();
    UPLOADS.get_or_init(|| Mutex::new(HashMap::new()))
}

fn failure(code: &'static str, message: impl Into<String>) -> DomainStateError {
    DomainStateError {
        code,
        message: message.into(),
    }
}

/// Adds a piece at `offset`; answers how many bytes are held, or the whole clip once `done`.
fn add_piece(
    conversation_id: &str,
    offset: usize,
    piece: &[u8],
    done: bool,
) -> Result<(usize, Option<Vec<u8>>), DomainStateError> {
    let mut uploads = uploads()
        .lock()
        .unwrap_or_else(|poison| poison.into_inner());
    uploads.retain(|_, upload| upload.touched.elapsed() < STALE_AFTER);
    let upload = uploads
        .entry(conversation_id.to_string())
        .or_insert_with(|| Upload {
            bytes: Vec::new(),
            touched: Instant::now(),
        });
    // A new clip starts at 0; a piece sent again after a lost answer lands where it was.
    if offset == 0 {
        upload.bytes.clear();
    }
    if offset > upload.bytes.len() {
        return Ok((upload.bytes.len(), None));
    }
    upload.bytes.truncate(offset);
    if upload.bytes.len() + piece.len() > MAX_CLIP_BYTES {
        uploads.remove(conversation_id);
        return Err(failure("tooLong", "That turn is longer than two minutes."));
    }
    upload.bytes.extend_from_slice(piece);
    upload.touched = Instant::now();
    let received = upload.bytes.len();
    if !done {
        return Ok((received, None));
    }
    let mut clip = uploads
        .remove(conversation_id)
        .map(|upload| upload.bytes)
        .unwrap_or_default();
    if clip.len() >= WAV_HEADER_BYTES && clip.starts_with(b"RIFF") {
        clip.drain(..WAV_HEADER_BYTES);
    }
    clip.truncate(clip.len() - clip.len() % 2);
    Ok((received, Some(clip)))
}

async fn handle(body: &Value) -> Result<Value, DomainStateError> {
    let params = read_domain_rpc_params(body)?;
    let conversation_id = params
        .get("conversationId")
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|id| !id.is_empty())
        .ok_or_else(|| failure("invalidParams", "conversationId is required."))?
        .to_string();
    let offset = params.get("offset").and_then(Value::as_u64).unwrap_or(0) as usize;
    let piece = base64::engine::general_purpose::STANDARD
        .decode(
            params
                .get("chunk")
                .and_then(Value::as_str)
                .unwrap_or_default(),
        )
        .map_err(|_| failure("invalidParams", "chunk must be base64."))?;
    if piece.len() > MAX_PIECE_BYTES {
        return Err(failure(
            "invalidParams",
            format!("A piece may carry at most {MAX_PIECE_BYTES} bytes."),
        ));
    }
    let done = params.get("done").and_then(Value::as_bool) == Some(true);
    let Some(tx) = registry::sender(&conversation_id) else {
        return Err(failure(
            "notFound",
            "That voice conversation has ended. Start a new one.",
        ));
    };
    let (received, clip) = add_piece(&conversation_id, offset, &piece, done)?;
    let Some(clip) = clip else {
        return Ok(json!({"received": received}));
    };
    tx.send(Input::ClipTurn(Bytes::from(clip))).map_err(|_| {
        failure(
            "notFound",
            "That voice conversation has ended. Start a new one.",
        )
    })?;
    Ok(json!({"received": received, "sent": true}))
}

pub(crate) async fn handle_voice_agent_clip_http(
    _state: &AppState,
    endpoint_path: String,
    request_id: String,
    body: &Value,
) -> RoutedResponse {
    match handle(body).await {
        Ok(result) => routed_json(
            Some(endpoint_path),
            StatusCode::OK,
            rpc_success(request_id, result),
        ),
        Err(error) => domain_error_response(endpoint_path, request_id, error),
    }
}
