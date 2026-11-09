//! Preview fixtures for Settings › Dictation and Settings › Voice Agent: a scripted
//! `/api/readDictateSettings` (Hex ready, Hex not installed, or OpenAI with no key),
//! `/api/readVoiceAgent` and an empty Dictation history.
//!
//! States: `dictation` (Hex ready), `dictation-hex-missing` (Hex not installed), `dictation-openai` (OpenAI picked, its key row under the engine),
//! `dictation-xai` (xAI picked, no key), `dictation-hex-stopped` (Hex installed but not running),
//! `dictation-hex-nomodel` (Hex running with no model ready), `dictation-install` (Hex
//! missing; Install Hex plays a stubbed installer, then Open Hex works), `dictation-install-confirm`,
//! `-asking`, `-running`, `-failed` and `-nobrew` (each step of it), `voice-agent` (the Voice Agent page on xAI with its saved key).
use serde_json::{Value, json};
use std::sync::atomic::{AtomicBool, Ordering};

/// The stubbed installer finished: Hex is installed, and not running until Open Hex.
static INSTALLED: AtomicBool = AtomicBool::new(false);
/// Open Hex was pressed: Hex runs.
static OPENED: AtomicBool = AtomicBool::new(false);

pub(super) fn owns(state: &str) -> bool {
    state.starts_with("dictation") || state == "voice-agent"
}

pub(super) fn open_message(state: &str) -> Option<Value> {
    if state == "voice-agent" {
        return Some(json!({ "initialTab": "voiceAgent" }));
    }
    owns(state).then(|| json!({ "initialTab": "dictation" }))
}

fn hex(state: &str) -> Value {
    // The install states start without Hex; the stubbed installer and Open Hex move them on.
    let state = if state.starts_with("dictation-install") {
        if OPENED.load(Ordering::SeqCst) {
            "dictation"
        } else if INSTALLED.load(Ordering::SeqCst) {
            "dictation-hex-stopped"
        } else {
            "dictation-hex-missing"
        }
    } else {
        state
    };
    match state {
        "dictation-hex-missing" => json!({
            "available": true, "state": "notInstalled", "version": null, "olderVersion": null,
            "models": [], "model": null, "ready": false,
            "problem": { "code": "hexNotInstalled", "message": "Hex 3 isn't installed on this computer. Install it to transcribe with Hex." },
            "downloadUrl": "https://github.com/anomalyco/hex#downloads",
        }),
        "dictation-hex-stopped" => json!({
            "available": true, "state": "notRunning", "version": null, "olderVersion": null,
            "models": [], "model": null, "ready": false, "canOpen": true,
            "problem": { "code": "hexNotRunning", "message": "Hex isn't running on this computer. Open Hex." },
            "downloadUrl": "https://github.com/anomalyco/hex#downloads",
        }),
        "dictation-hex-nomodel" => json!({
            "available": true, "state": "ready", "version": "3.3.0", "olderVersion": null,
            "models": [], "model": null, "ready": false, "canOpen": true,
            "problem": { "code": "hexNoModels", "message": "Hex has no model ready. Open Hex and download a model there." },
            "downloadUrl": "https://github.com/anomalyco/hex#downloads",
        }),
        _ => json!({
            "available": true, "state": "ready", "version": "3.3.0", "olderVersion": null,
            "models": [
                { "id": "parakeet_v3", "name": "Parakeet v3", "languages": ["en"], "supportsLanguageDetection": false },
                { "id": "whisper_turbo", "name": "Whisper turbo", "languages": ["*"], "supportsLanguageDetection": true }
            ],
            "model": "parakeet_v3", "ready": true, "problem": null,
            "downloadUrl": "https://github.com/anomalyco/hex#downloads",
        }),
    }
}

fn dictate_settings(state: &str) -> Value {
    let engine = match state {
        "dictation-openai" => "openai",
        "dictation-xai" => "xai",
        _ => "hex",
    };
    let hex = hex(state);
    let engine_view = match engine {
        "openai" => json!({
            "engine": "openai", "label": "OpenAI gpt-4o-transcribe", "ready": false,
            "problem": { "code": "noKey", "message": "Add your OpenAI API key in Settings." },
        }),
        "xai" => json!({
            "engine": "xai", "label": "xAI Grok", "ready": false,
            "problem": { "code": "noKey", "message": "Add your xAI API key in Settings." },
        }),
        _ => json!({
            "engine": "hex", "label": "Hex, Parakeet v3", "ready": hex["ready"],
            "problem": hex["problem"],
        }),
    };
    json!({
        "settings": {
            "engine": engine, "hexModel": "parakeet_v3", "language": "auto",
        },
        "engine": engine_view,
        "hex": hex,
        "keys": { "xai": { "configured": true, "unreadable": false }, "openai": { "configured": false, "unreadable": false } },
        "languages": [{ "code": "auto", "label": "Auto-detect" }, { "code": "en", "label": "English" }],
    })
}

/// The stubbed app host: what the real one would send back for a Settings message, as `(delay in
/// milliseconds, payload)`. No installer runs in the preview; Install Hex only plays the steps.
pub(super) fn script(state: &str, message: &Value) -> Vec<(u64, Value)> {
    if !owns(state) {
        return Vec::new();
    }
    let step = |phase: &str| json!({ "type": "hexInstallState", "phase": phase });
    match message.get("type").and_then(Value::as_str) {
        Some("hexInstallRun") => {
            INSTALLED.store(true, Ordering::SeqCst);
            vec![
                (0, step("asking")),
                (1200, step("installing")),
                (4200, step("done")),
            ]
        }
        Some("hexOpen") => {
            OPENED.store(true, Ordering::SeqCst);
            Vec::new()
        }
        _ => Vec::new(),
    }
}

/// The scripted daemon for these states; `None` for paths it does not know.
pub(super) fn rpc(state: &str, path: &str, _params: &Value) -> Option<Result<Value, String>> {
    Some(Ok(match path {
        "/api/readDictateSettings" | "/api/setDictateSettings" | "/api/deleteOldSpeechModels" => {
            dictate_settings(state)
        }
        "/api/readVoiceAgent" => json!({
            "keys": {
                "xai": { "configured": true, "unreadable": false },
                "openai": { "configured": false, "unreadable": false }
            },
            "memory": {
                "endedAt": "2026-11-07T18:40:00Z",
                "gist": "You went through the release branch and asked to follow up on the phone build."
            },
            "memorySummaryPending": false,
        }),
        "/api/listDictations" => json!({ "dictations": [], "count": 0, "totalBytes": 0 }),
        _ => return None,
    }))
}
