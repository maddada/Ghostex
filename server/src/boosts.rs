//! Boosts on gxserver: the prompts the user edited, the user's own Boosts, whether Boosts are on,
//! the one-time callout, `/api/boosts`, and the block each Boost's prompt adds under a message on
//! its way to the agent.
//!
//! Ghostex's Boosts (names, icons, the prompts Ghostex ships) are gx-chat-core's (`extras::boosts`),
//! so every client and this server read one list. Everything the user changes is kept here, for
//! every chat and every client of this computer (the desktop, the web build and the phone all reach
//! it through `/api/boosts`), in `<state>/boosts.json`: an edited prompt per Boost until Reset to
//! default (Settings > Boosts, or `ghostex visual boosts reset <id>`), the user's own Boosts in
//! their order, and the on/off switch.
//!
//! CDXC:Boosts 2026-11-20 WHY:
//! The prompt is added here, after the draft check, as review comments are: gxserver sends only the exact text of the saved draft revision (the pills), and every send and queued prompt passes here on its way to the agent. It goes in the `<ghostex-system>` block every chat already hides, so phones whose chat core predates Boosts still show only what the user wrote.
//! SEE-ALSO: packages/gx-chat-core/src/extras/boosts.rs (the user's decisions, the list, the block),
//! server/src/session_chat_queue_runtime/send.rs (where it is added),
//! server/src/ghostex_cli/visual_guide.rs (the help each prompt points to),
//! apps/desktop/src/app/window/settings_modal/tabs/boosts.rs (Settings > Boosts).

use std::path::PathBuf;

use ghostex_gx_chat_core::extras::boosts::{
    self as core, Boost, CustomBoost, BOOSTS, CUSTOM_ICONS,
};
use serde_json::{json, Map, Value};

use crate::domain::DomainStateError;
use crate::paths::GxserverPaths;

/// `/api/boosts`. Every answer is the whole state: `{on, hintSeen, boosts: [{id, label,
/// category, description, prompt, edited, default}], custom: [{id, label, icon, prompt}], icons}`.
/// `op`: `list` (the default), `set` (`id`, `prompt`: a Boost's prompt, Ghostex's or the user's),
/// `reset` (`id`), `hintSeen`, `setOn` (`on`), `addCustom` (`label`, `icon`, `prompt`; answers the
/// new one's id as `added`), `updateCustom` (`id`, any of `label`, `icon`, `prompt`),
/// `deleteCustom` (`id`) and `moveCustom` (`id`, `to`: its new index).
pub(crate) const BOOSTS_ENDPOINT: &str = "/api/boosts";

/// What is saved.
#[derive(Default)]
struct Saved {
    prompts: Map<String, Value>,
    custom: Vec<CustomBoost>,
    off: bool,
    hint_seen: bool,
    /// The number the next user's Boost's id ends with, so a deleted id is never reused.
    next: u64,
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
    let custom: Vec<CustomBoost> = value
        .get("custom")
        .cloned()
        .and_then(|custom| serde_json::from_value(custom).ok())
        .unwrap_or_default();
    Saved {
        prompts: value
            .get("prompts")
            .and_then(Value::as_object)
            .cloned()
            .unwrap_or_default(),
        next: value
            .get("next")
            .and_then(Value::as_u64)
            .unwrap_or(custom.len() as u64 + 1),
        custom,
        off: value.get("on") == Some(&Value::Bool(false)),
        hint_seen: value.get("hintSeen") == Some(&Value::Bool(true)),
    }
}

fn write(paths: &GxserverPaths, saved: &Saved) -> std::io::Result<()> {
    let text = serde_json::to_string_pretty(&json!({
        "prompts": saved.prompts,
        "custom": saved.custom,
        "on": !saved.off,
        "hintSeen": saved.hint_seen,
        "next": saved.next,
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

/// The edited prompt of Ghostex's `boost`, if the user edited it.
fn edited<'a>(saved: &'a Saved, boost: &Boost) -> Option<&'a str> {
    saved
        .prompts
        .get(boost.id)
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|prompt| !prompt.is_empty())
}

/// The prompt Boost `id` sends now: Ghostex's (or its edit), or the user's own; `None` for a
/// Boost that is gone.
fn prompt_of(saved: &Saved, id: &str) -> Option<String> {
    if let Some(boost) = core::boost(id) {
        return Some(edited(saved, boost).unwrap_or(boost.prompt).to_string());
    }
    saved
        .custom
        .iter()
        .find(|custom| custom.id == id)
        .map(|custom| custom.prompt.clone())
}

/// The text the agent gets for `text`: the message, then its Boosts' prompts. A message with no
/// Boost comes back as it is.
pub(crate) fn with_boosts(paths: &GxserverPaths, text: &str) -> String {
    if !text.contains("](boost:") {
        return text.to_string();
    }
    let saved = read(paths);
    core::with_boosts(text, |id| prompt_of(&saved, id))
}

/// Boosts on this computer for the anonymous daily snapshot: whether they are on, how many of the
/// user's own Boosts there are and how many of Ghostex's prompts the user edited. Counts only:
/// never a label or a prompt (CDXC:Telemetry 2026-11-20 in telemetry/settings_snapshot.rs).
pub(crate) fn telemetry_summary(
    paths: &GxserverPaths,
) -> crate::telemetry::settings_snapshot::BoostsReport {
    let saved = read(paths);
    crate::telemetry::settings_snapshot::BoostsReport {
        on: !saved.off,
        custom: saved.custom.len(),
        edited: BOOSTS
            .iter()
            .filter(|boost| edited(&saved, boost).is_some())
            .count(),
    }
}

/// The Boosts a message carries, for the anonymous `boosts.used` event: Ghostex's own by id, and
/// how many others (the user's own, or one since deleted), which are never named.
pub(crate) fn used_in(text: &str) -> (Vec<&'static str>, usize) {
    if !text.contains("](boost:") {
        return (Vec::new(), 0);
    }
    let mut built_in = Vec::new();
    let mut other = 0;
    for (id, _) in core::boosts_in(text) {
        match core::boost(&id) {
            Some(boost) => built_in.push(boost.id),
            None => other += 1,
        }
    }
    (built_in, other)
}

fn custom_description(prompt: &str) -> String {
    const MAX: usize = 110;
    let flat = prompt.split_whitespace().collect::<Vec<_>>().join(" ");
    if flat.chars().count() <= MAX {
        return flat;
    }
    let cut: String = flat.chars().take(MAX).collect();
    let cut = cut.rsplit_once(' ').map_or(cut.as_str(), |(head, _)| head);
    format!("{}\u{2026}", cut.trim_end_matches([',', '.', ';', ':']))
}

/// Enabled Boosts formatted for Live page `*` mention menus: Ghostex's built-in Boosts followed
/// by the user's custom Boosts, empty when Boosts are turned off in Settings.
pub(crate) fn mention_choices(paths: &GxserverPaths) -> Vec<Value> {
    let saved = read(paths);
    if saved.off {
        return Vec::new();
    }
    BOOSTS
        .iter()
        .map(|boost| {
            json!({
                "id": boost.id,
                "label": boost.label,
                "description": boost.description,
                "section": core::tab_label_of(boost.id),
                "token": core::boost_link(boost.id, boost.label),
            })
        })
        .chain(saved.custom.iter().map(|boost| {
            json!({
                "id": boost.id,
                "label": boost.label,
                "description": custom_description(&boost.prompt),
                "section": "Custom",
                "token": core::boost_link(&boost.id, &boost.label),
            })
        }))
        .collect()
}

fn listing(saved: &Saved) -> Value {
    json!({
        "on": !saved.off,
        "hintSeen": saved.hint_seen,
        "boosts": BOOSTS.iter().map(|boost| json!({
            "id": boost.id,
            "label": boost.label,
            "category": boost.category,
            "iconPath": boost.icon,
            "description": boost.description,
            "prompt": edited(saved, boost).unwrap_or(boost.prompt),
            "edited": edited(saved, boost).is_some(),
            "default": boost.prompt,
        })).collect::<Vec<_>>(),
        "custom": saved.custom,
        "icons": CUSTOM_ICONS,
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
/// The most of a user's Boost's name kept.
const LABEL_MAX_CHARS: usize = 40;

fn text<'a>(params: &'a Map<String, Value>, key: &str) -> Option<&'a str> {
    params.get(key).and_then(Value::as_str).map(str::trim)
}

fn checked_prompt(prompt: &str) -> Result<String, DomainStateError> {
    if prompt.is_empty() {
        return Err(bad("Say what the agent is told."));
    }
    if prompt.chars().count() > PROMPT_MAX_CHARS {
        return Err(bad(format!(
            "A Boost's prompt can be at most {PROMPT_MAX_CHARS} characters."
        )));
    }
    Ok(prompt.to_string())
}

fn checked_label(label: &str) -> Result<String, DomainStateError> {
    let label = label.split_whitespace().collect::<Vec<_>>().join(" ");
    if label.is_empty() {
        return Err(bad("Give the Boost a name."));
    }
    Ok(label.chars().take(LABEL_MAX_CHARS).collect())
}

fn checked_icon(icon: Option<&str>) -> String {
    icon.filter(|icon| CUSTOM_ICONS.contains(icon))
        .unwrap_or(CUSTOM_ICONS[0])
        .to_string()
}

fn custom_index(saved: &Saved, params: &Map<String, Value>) -> Result<usize, DomainStateError> {
    let id = text(params, "id").unwrap_or_default();
    saved
        .custom
        .iter()
        .position(|custom| custom.id == id)
        .ok_or_else(|| bad(format!("You have no Boost called \"{id}\".")))
}

/// One `/api/boosts` call.
pub(crate) fn handle(
    paths: &GxserverPaths,
    params: &Map<String, Value>,
) -> Result<Value, DomainStateError> {
    let op = text(params, "op").unwrap_or("list");
    let mut saved = read(paths);
    let mut added = None;
    match op {
        "list" => return Ok(listing(&saved)),
        "set" => {
            let id = text(params, "id").unwrap_or_default();
            let prompt = checked_prompt(text(params, "prompt").unwrap_or_default())?;
            if let Some(boost) = core::boost(id) {
                if prompt == boost.prompt {
                    saved.prompts.remove(boost.id);
                } else {
                    saved
                        .prompts
                        .insert(boost.id.to_string(), Value::String(prompt));
                }
            } else {
                let index = custom_index(&saved, params)?;
                saved.custom[index].prompt = prompt;
            }
        }
        "reset" => {
            let id = text(params, "id").unwrap_or_default();
            let boost = core::boost(id).ok_or_else(|| {
                bad(format!(
                    "\"{id}\" is not one of Ghostex's Boosts, so it has nothing to reset to."
                ))
            })?;
            saved.prompts.remove(boost.id);
        }
        "hintSeen" => saved.hint_seen = true,
        "setOn" => {
            saved.off = params.get("on") != Some(&Value::Bool(true));
            // Turning Boosts off is a choice about them: the callout has nothing to point at.
            saved.hint_seen = true;
        }
        "addCustom" => {
            let boost = CustomBoost {
                id: format!("custom-{}", saved.next.max(1)),
                label: checked_label(text(params, "label").unwrap_or_default())?,
                icon: checked_icon(text(params, "icon")),
                prompt: checked_prompt(text(params, "prompt").unwrap_or_default())?,
            };
            saved.next = saved.next.max(1) + 1;
            added = Some(boost.id.clone());
            saved.custom.push(boost);
        }
        "updateCustom" => {
            let index = custom_index(&saved, params)?;
            if let Some(label) = text(params, "label") {
                saved.custom[index].label = checked_label(label)?;
            }
            if params.contains_key("icon") {
                saved.custom[index].icon = checked_icon(text(params, "icon"));
            }
            if let Some(prompt) = text(params, "prompt") {
                saved.custom[index].prompt = checked_prompt(prompt)?;
            }
        }
        "deleteCustom" => {
            let index = custom_index(&saved, params)?;
            saved.custom.remove(index);
        }
        "moveCustom" => {
            let index = custom_index(&saved, params)?;
            let to = params
                .get("to")
                .and_then(Value::as_u64)
                .map_or(index, |to| (to as usize).min(saved.custom.len() - 1));
            let boost = saved.custom.remove(index);
            saved.custom.insert(to, boost);
        }
        other => return Err(bad(format!("Unknown Boosts op: {other}"))),
    }
    write(paths, &saved).map_err(|error| DomainStateError {
        code: "internalError",
        message: format!("Ghostex could not save the Boosts: {error}"),
    })?;
    let mut answer = listing(&saved);
    if let Some(id) = added {
        answer["added"] = json!(id);
    }
    Ok(answer)
}
