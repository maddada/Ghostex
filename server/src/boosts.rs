//! Boosts on gxserver: the prompts the user edited, the one-time hint, `/api/boosts`, and the
//! block each Boost's prompt adds under a message on its way to the agent.
//!
//! The Boosts themselves (names, icons, the prompts Ghostex ships) are gx-chat-core's
//! (`extras::boosts`), so every client and this server read one list. An edited prompt is kept
//! here, per Boost, for every chat and every client of this computer (the desktop, the web build
//! and the phone all reach it through `/api/boosts`), in `<state>/boosts.json`. Reset to default in
//! the prompt editor, or `ghostex visual boosts reset <id>`, drops the edit.
//!
//! CDXC:Boosts 2026-11-20 WHY:
//! The prompt is added here, after the draft check, as review comments are: gxserver sends only the exact text of the saved draft revision (the pills), and every send and queued prompt passes here on its way to the agent. It goes in the `<ghostex-system>` block every chat already hides, so phones whose chat core predates Boosts still show only what the user wrote.
//! SEE-ALSO: packages/gx-chat-core/src/extras/boosts.rs (the user's decision, the list, the block),
//! server/src/session_chat_queue_runtime/send.rs (where it is added),
//! server/src/ghostex_cli/visual_guide.rs (the help each prompt points to).

use std::path::PathBuf;

use ghostex_gx_chat_core::extras::boosts::{self as core, Boost, BOOSTS};
use serde_json::{json, Map, Value};

use crate::domain::DomainStateError;
use crate::paths::GxserverPaths;

/// `/api/boosts`: `op` is `list` (the default), `set` (`id`, `prompt`), `reset` (`id`) or
/// `hintSeen`. Every answer is the list: `{boosts: [{id, label, description, prompt, edited,
/// default}], hintSeen}`.
pub(crate) const BOOSTS_ENDPOINT: &str = "/api/boosts";

/// What is saved: the edited prompts by Boost id, and whether the hint was seen.
#[derive(Default)]
struct Saved {
    prompts: Map<String, Value>,
    hint_seen: bool,
}

fn file(paths: &GxserverPaths) -> PathBuf {
    paths.app_state_dir.join("boosts.json")
}

fn read(paths: &GxserverPaths) -> Saved {
    let Some(value) = std::fs::read_to_string(file(paths))
        .ok()
        .and_then(|text| serde_json::from_str::<Value>(&text).ok())
    else {
        return Saved::default();
    };
    Saved {
        prompts: value
            .get("prompts")
            .and_then(Value::as_object)
            .cloned()
            .unwrap_or_default(),
        hint_seen: value.get("hintSeen") == Some(&Value::Bool(true)),
    }
}

fn write(paths: &GxserverPaths, saved: &Saved) -> std::io::Result<()> {
    let text = serde_json::to_string_pretty(&json!({
        "prompts": saved.prompts,
        "hintSeen": saved.hint_seen,
    }))
    .unwrap_or_default();
    let path = file(paths);
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    // Through a temporary file, so a crash never leaves half a file.
    let temporary = path.with_extension("json.tmp");
    std::fs::write(&temporary, text)?;
    std::fs::rename(&temporary, &path)
}

/// The edited prompt of `boost`, if the user edited it.
fn edited<'a>(saved: &'a Saved, boost: &Boost) -> Option<&'a str> {
    saved
        .prompts
        .get(boost.id)
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|prompt| !prompt.is_empty())
}

/// The text the agent gets for `text`: the message, then its Boosts' prompts (the edited ones where
/// the user edited them). A message with no Boost comes back as it is.
pub(crate) fn with_boosts(paths: &GxserverPaths, text: &str) -> String {
    if !text.contains("](boost:") {
        return text.to_string();
    }
    let saved = read(paths);
    core::with_boosts(text, |boost| {
        edited(&saved, boost).unwrap_or(boost.prompt).to_string()
    })
}

fn listing(saved: &Saved) -> Value {
    json!({
        "boosts": BOOSTS.iter().map(|boost| json!({
            "id": boost.id,
            "label": boost.label,
            "description": boost.description,
            "prompt": edited(saved, boost).unwrap_or(boost.prompt),
            "edited": edited(saved, boost).is_some(),
            "default": boost.prompt,
        })).collect::<Vec<_>>(),
        "hintSeen": saved.hint_seen,
    })
}

fn bad(message: impl Into<String>) -> DomainStateError {
    DomainStateError {
        code: "badRequest",
        message: message.into(),
    }
}

/// The most of a prompt kept: a Boost's prompt is a short instruction, not a document.
const PROMPT_MAX_CHARS: usize = 4_000;

/// One `/api/boosts` call.
pub(crate) fn handle(
    paths: &GxserverPaths,
    params: &Map<String, Value>,
) -> Result<Value, DomainStateError> {
    let op = params.get("op").and_then(Value::as_str).unwrap_or("list");
    let mut saved = read(paths);
    let boost_param = || {
        let id = params.get("id").and_then(Value::as_str).unwrap_or_default();
        core::boost(id).ok_or_else(|| {
            bad(format!(
                "No Boost is called \"{id}\". The Boosts are: {}.",
                BOOSTS
                    .iter()
                    .map(|boost| boost.id)
                    .collect::<Vec<_>>()
                    .join(", ")
            ))
        })
    };
    match op {
        "list" => return Ok(listing(&saved)),
        "set" => {
            let boost = boost_param()?;
            let prompt = params
                .get("prompt")
                .and_then(Value::as_str)
                .map(str::trim)
                .unwrap_or_default();
            if prompt.is_empty() {
                return Err(bad("Say the prompt, or reset it to go back to Ghostex's."));
            }
            if prompt.chars().count() > PROMPT_MAX_CHARS {
                return Err(bad(format!(
                    "A Boost's prompt can be at most {PROMPT_MAX_CHARS} characters."
                )));
            }
            if prompt == boost.prompt {
                saved.prompts.remove(boost.id);
            } else {
                saved
                    .prompts
                    .insert(boost.id.to_string(), Value::String(prompt.to_string()));
            }
        }
        "reset" => {
            let boost = boost_param()?;
            saved.prompts.remove(boost.id);
        }
        "hintSeen" => saved.hint_seen = true,
        other => return Err(bad(format!("Unknown Boosts op: {other}"))),
    }
    write(paths, &saved).map_err(|error| DomainStateError {
        code: "internalError",
        message: format!("Ghostex could not save the Boost: {error}"),
    })?;
    Ok(listing(&saved))
}
