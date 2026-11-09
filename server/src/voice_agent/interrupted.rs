//! Calls that were open when gxserver stopped, kept for a few minutes so the device that comes back
//! to one (a phone reattaching with its `conversationId`) gets a new call with the same settings
//! instead of "That voice conversation has ended".
//!
//! CDXC:VoiceMode 2026-11-10 WHY: The user's iPhone voice calls kept failing while the computer's Ghostex restarted and the phone's link reconnected. A phone (iPhone 11.2.1 included, whose voice code can't change over the air) reattaches for two minutes with only the `conversationId`, and a restarted gxserver no longer knew it, so the call ended. The restarted gxserver now opens a fresh call with the interrupted one's provider, call mode and session focus, says so on screen, and the phone carries on: its native code takes the new `conversationId` from `ready`. A call with a GBot is not restarted (its chat is opened by `gbot_call/`, which needs the bot the phone no longer sends).

use std::fs;
use std::path::PathBuf;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use serde::{Deserialize, Serialize};

use crate::paths::GxserverPaths;

/// How long after the stop a device may come back and get a new call.
const KEEP_FOR: Duration = Duration::from_secs(5 * 60);

/// What a new call needs to stand in for an interrupted one.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(super) struct Interrupted {
    pub(super) provider: String,
    pub(super) push_to_talk: bool,
    /// `<projectId>:<sessionId>` for a call about one session.
    pub(super) focus: Option<String>,
    pub(super) client_remote: bool,
    pub(super) ended_at_ms: u64,
}

fn folder(paths: &GxserverPaths) -> PathBuf {
    paths.root_dir.join("voice-agent").join("interrupted")
}

/// Conversation ids are uuids; anything else is never a file name here.
fn path(paths: &GxserverPaths, id: &str) -> Option<PathBuf> {
    let safe = !id.is_empty()
        && id.len() <= 64
        && id.chars().all(|c| c.is_ascii_alphanumeric() || c == '-');
    safe.then(|| folder(paths).join(format!("{id}.json")))
}

fn now_ms() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|elapsed| elapsed.as_millis() as u64)
        .unwrap_or(0)
}

/// Keeps the interrupted call `id` (written while gxserver stops, so it is small and synchronous).
pub(super) fn remember(paths: &GxserverPaths, id: &str, mut call: Interrupted) {
    let Some(path) = path(paths, id) else { return };
    call.ended_at_ms = now_ms();
    let _ = fs::create_dir_all(folder(paths));
    if let Ok(body) = serde_json::to_vec(&call) {
        let _ = fs::write(path, body);
    }
}

/// Takes the interrupted call `id` if it stopped within [`KEEP_FOR`]; forgets it either way, and
/// clears out other old ones.
pub(super) fn take(paths: &GxserverPaths, id: &str) -> Option<Interrupted> {
    let now = now_ms();
    let fresh =
        |call: &Interrupted| now.saturating_sub(call.ended_at_ms) <= KEEP_FOR.as_millis() as u64;
    if let Ok(entries) = fs::read_dir(folder(paths)) {
        for entry in entries.flatten() {
            let stale = fs::read(entry.path())
                .ok()
                .and_then(|body| serde_json::from_slice::<Interrupted>(&body).ok())
                .is_none_or(|call| !fresh(&call));
            if stale {
                let _ = fs::remove_file(entry.path());
            }
        }
    }
    let path = path(paths, id)?;
    let call = fs::read(&path)
        .ok()
        .and_then(|body| serde_json::from_slice::<Interrupted>(&body).ok());
    let _ = fs::remove_file(&path);
    call.filter(fresh)
}
