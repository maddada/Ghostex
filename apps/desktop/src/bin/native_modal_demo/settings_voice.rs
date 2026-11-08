//! Preview fixtures for Settings › Dictation and Settings › Voice Agent: a scripted
//! `/api/readDictateSettings` (Hex ready, Hex not installed with the notice for a computer that
//! used the retired local engine, or OpenAI with no key), `/api/readVoiceAgent` and an empty
//! Dictation history.
//!
//! States: `dictation` (Hex ready), `dictation-retired` (the one-time notice, Hex not installed,
//! old models to delete), `dictation-openai` (OpenAI picked, its key row under the engine),
//! `voice-agent` (the Voice Agent page on xAI with its saved key).
use serde_json::{Value, json};

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
    match state {
        "dictation-retired" => json!({
            "available": true, "state": "notInstalled", "version": null, "olderVersion": null,
            "models": [], "model": null, "ready": false,
            "problem": { "code": "hexNotInstalled", "message": "Hex 3 isn't installed on this computer. Install it to transcribe with Hex." },
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
    let engine = if state == "dictation-openai" {
        "openai"
    } else {
        "hex"
    };
    let hex = hex(state);
    let engine_view = match engine {
        "openai" => json!({
            "engine": "openai", "label": "OpenAI gpt-4o-transcribe", "ready": false, "polish": true,
            "problem": { "code": "noKey", "message": "Add your OpenAI API key in Settings." },
        }),
        _ => json!({
            "engine": "hex", "label": "Hex, Parakeet v3", "ready": hex["ready"], "polish": true,
            "problem": hex["problem"],
        }),
    };
    let retired = state == "dictation-retired";
    json!({
        "settings": {
            "engine": engine, "hexModel": "parakeet_v3", "language": "auto",
            "dictionary": [{ "word": "gxserver", "aliases": ["GX server"] }],
            "addProjectNames": true, "removeFillers": true,
            "polish": { "enabled": true, "model": "claude-sonnet-5-5", "prompt": null },
        },
        "engine": engine_view,
        "hex": hex,
        "keys": { "xai": { "configured": true, "unreadable": false }, "openai": { "configured": false, "unreadable": false } },
        "localRetired": if retired {
            json!({ "message": "Ghostex no longer has its own speech model. Hex, a free dictation app that runs on your computer, is now the recommended way to dictate, and Ghostex works with it directly. Install Hex to keep dictating offline; your phone's dictation goes through this computer, so it uses Hex too." })
        } else {
            Value::Null
        },
        "oldModels": if retired {
            json!({ "folder": "~/Library/Application Support/Ghostex/gxserver/speech-models", "bytes": 723_000_000u64 })
        } else {
            Value::Null
        },
        "projectWords": ["Ghostex"],
        "polishAvailable": true,
        "languages": [{ "code": "auto", "label": "Auto-detect" }, { "code": "en", "label": "English" }],
    })
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
