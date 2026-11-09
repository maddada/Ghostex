//! `/api/gbots` ops for a GBot's API: `api` (is it on, its key's label and use, the address and
//! the command to paste), `apiOn` (`label?`), `apiNewKey` (`label?`) and `apiOff`, each with `bot`.
//!
//! CDXC:GBots 2026-11-09 DECISION:
//! Coordinator brief (M11), on the user's behalf: the GBot API is "off by default, per GBot; only the user can turn it on, mint a key or turn it off (an owner-presence approval like other security-relaxing changes, and a security-log entry each time). The key is shown once and stored hashed; New key replaces it." So `apiOn` and `apiNewKey` are refused to every agent (Chief of Staff included) and ask your approval from your own computer, phone or terminal (server/src/server/security_gates.rs); `apiOff` is yours too but never asks, because closing something never does (`SensitiveAction`). Each writes a security-log entry. The key is in the answer of `apiOn` and `apiNewKey` only.

use rusqlite::Connection;
use serde_json::{json, Map, Value};

use super::keys::{add_key, key_summary, live_key, revoke, BotKey, LABEL_MAX_CHARS};
use crate::domain::DomainStateError;
use crate::gbots::record::{bot_summary, require_bot, Bot};
use crate::security::Principal;
use crate::server::AppState;

pub(crate) fn handles(op: &str) -> bool {
    matches!(
        op,
        "api" | "apiOn" | "apiNewKey" | "apiOff" | "apiShareMemory"
    )
}

/// The ops that open a GBot to apps, which ask your approval first (sharing memory only when it
/// turns it on).
pub(crate) fn opens_api(op: &str) -> bool {
    matches!(op, "apiOn" | "apiNewKey" | "apiShareMemory")
}

fn text<'a>(params: &'a Map<String, Value>, key: &str) -> Option<&'a str> {
    params
        .get(key)
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty())
}

fn forbidden(message: &str) -> DomainStateError {
    DomainStateError {
        code: "forbidden",
        message: message.to_string(),
    }
}

/// The address an app on this computer posts a message to.
pub(crate) fn messages_url(state: &AppState, bot_id: &str) -> String {
    format!(
        "http://127.0.0.1:{}{}{bot_id}/messages",
        state.config.listeners.local.port,
        super::inbound::BOTS_ROUTE_PREFIX
    )
}

/// The command to paste, with `key` (or a stand-in when the key is not shown).
fn command(url: &str, key: Option<&str>) -> String {
    format!(
        "curl -s {url} \\\n  -H 'Authorization: Bearer {}' \\\n  -H 'Content-Type: application/json' \\\n  -d '{{\"message\": \"Hello\"}}'",
        key.unwrap_or("<your GBot API key>")
    )
}

fn status(state: &AppState, bot: &Bot, key: Option<&BotKey>, shown: Option<&str>) -> Value {
    let url = messages_url(state, &bot.bot_id);
    let mut answer = json!({
        "bot": bot_summary(bot),
        "api": {
            "on": key.is_some(),
            "key": key.map(key_summary),
            "url": url,
            "command": command(&url, shown),
            "reach": "Apps and scripts on this computer, and devices that already reach it over SSH (a paired phone, another computer of yours). Ghostex opens no port for it.",
        },
    });
    if let Some(shown) = shown {
        answer["key"] = json!(shown);
        answer["message"] = json!(format!(
            "Copy the key now: Ghostex keeps only its fingerprint and won't show it again. Apps send {} messages with it.",
            bot.name
        ));
    }
    answer
}

/// The security-log entry for each change. `/api/gbots` runs its ops off the request's own task,
/// so the approval a change was let through with is named from the call (`approvalId`) rather
/// than read back from the request.
fn log(caller: &Principal, kind: &str, summary: String, bot: &Bot, params: &Map<String, Value>) {
    let approval = text(params, ghostex_gx_protocol::approval::APPROVAL_ID_PARAM).map(|id| {
        crate::audit::ApprovalRef {
            id: id.to_string(),
            signer: None,
        }
    });
    crate::audit::record(
        crate::audit::Action::new(kind, summary).target(bot.bot_id.clone()),
        crate::audit::Actor::from(caller),
        crate::audit::Outcome::Done,
        approval,
    );
}

pub(crate) fn run(
    state: &AppState,
    db: &Connection,
    caller: &Principal,
    is_user: bool,
    op: &str,
    params: &Map<String, Value>,
) -> Result<Value, DomainStateError> {
    let bot = require_bot(
        db,
        text(params, "bot").ok_or_else(|| DomainStateError::bad_request("Pass bot."))?,
    )?;
    if op == "api" {
        let key = live_key(db, &bot.bot_id)?;
        return Ok(status(state, &bot, key.as_ref(), None));
    }
    if !is_user {
        return Err(forbidden(
            "Only the user turns a GBot's API on or off and makes its key, from GBot Settings > Bot API or their own terminal.",
        ));
    }
    if matches!(op, "apiOn" | "apiNewKey") && text(params, "keyNonce").is_none() {
        return Err(DomainStateError::bad_request(
            "Send a one-time keyNonce with the call: the key is shown only to the call whose approval asked with it. Update Ghostex or the ghostex CLI.",
        ));
    }
    let label: String = text(params, "label")
        .unwrap_or_default()
        .chars()
        .take(LABEL_MAX_CHARS)
        .collect();
    match op {
        "apiOn" => {
            super::chat::check_agent(state, db, &bot)?;
            if live_key(db, &bot.bot_id)?.is_some() {
                return Err(DomainStateError::bad_request(format!(
                    "{}'s API is already on. New key replaces its key; Turn off stops it.",
                    bot.name
                )));
            }
            let (row, key) = add_key(db, &bot.bot_id, &label, "user", false)?;
            // New chats start with the "Messages from apps" part of the brief (brief.rs).
            crate::gbots::brief::refresh_instruction_files(&state.paths, db, &bot);
            log(
                caller,
                "gbots.apiOn",
                format!("Turned on the GBot API for {}", bot.name),
                &bot,
                params,
            );
            Ok(status(state, &bot, Some(&row), Some(&key)))
        }
        "apiNewKey" => {
            let old = revoke(db, &bot.bot_id)?;
            let label = if label.is_empty() {
                old.as_ref()
                    .map(|old| old.label.clone())
                    .unwrap_or_default()
            } else {
                label
            };
            let share = old.as_ref().is_some_and(|old| old.share_memory);
            let (row, key) = add_key(db, &bot.bot_id, &label, "user", share)?;
            // New chats start with the "Messages from apps" part of the brief (brief.rs).
            crate::gbots::brief::refresh_instruction_files(&state.paths, db, &bot);
            log(
                caller,
                "gbots.apiNewKey",
                format!(
                    "Made a new GBot API key for {} (the old one stopped working)",
                    bot.name
                ),
                &bot,
                params,
            );
            Ok(status(state, &bot, Some(&row), Some(&key)))
        }
        "apiOff" => {
            let revoked = revoke(db, &bot.bot_id)?;
            if revoked.is_some() {
                crate::gbots::brief::refresh_instruction_files(&state.paths, db, &bot);
                log(
                    caller,
                    "gbots.apiOff",
                    format!("Turned off the GBot API for {}", bot.name),
                    &bot,
                    params,
                );
            }
            let mut answer = status(state, &bot, None, None);
            answer["message"] = json!(if revoked.is_some() {
                format!("{}'s API is off; its key no longer works.", bot.name)
            } else {
                format!("{}'s API was already off.", bot.name)
            });
            Ok(answer)
        }
        "apiShareMemory" => {
            let on = params.get("on").and_then(Value::as_bool) == Some(true);
            if live_key(db, &bot.bot_id)?.is_none() {
                return Err(DomainStateError::bad_request(format!(
                    "{}'s API is off. Turn it on first.",
                    bot.name
                )));
            }
            super::keys::set_share_memory(db, &bot.bot_id, on)?;
            crate::gbots::brief::refresh_instruction_files(&state.paths, db, &bot);
            log(
                caller,
                "gbots.apiShareMemory",
                format!(
                    "{} {}'s memory with apps through the GBot API",
                    if on { "Shared" } else { "Stopped sharing" },
                    bot.name
                ),
                &bot,
                params,
            );
            let key = live_key(db, &bot.bot_id)?;
            Ok(status(state, &bot, key.as_ref(), None))
        }
        other => Err(DomainStateError::bad_request(format!(
            "Unknown GBots op {other}: use api, apiOn, apiNewKey, apiOff or apiShareMemory."
        ))),
    }
}

/// What an approval for `apiOn`, `apiNewKey` or `apiShareMemory` names: the GBot (when it
/// exists; an unknown GBot is refused by the op without asking) and its routines' names.
pub(crate) struct ApprovalSubject {
    pub bot_id: String,
    pub name: String,
    pub routines: Vec<String>,
    /// Its key's chats read its memory already, or will once New key keeps that.
    pub shares_memory: bool,
}

pub(crate) fn approval_subject(
    state: &AppState,
    params: &Map<String, Value>,
) -> Option<ApprovalSubject> {
    let reference = text(params, "bot")?;
    let db = crate::storage::open_gxserver_database(&state.paths).ok()?;
    let bot = crate::gbots::record::find_bot(&db, reference).ok()??;
    let routines = crate::automations::bot_routines(&db, &bot.bot_id)
        .unwrap_or_default()
        .iter()
        .filter_map(|routine| routine["name"].as_str().map(str::to_string))
        .collect();
    let shares_memory = live_key(&db, &bot.bot_id)
        .ok()
        .flatten()
        .is_some_and(|key| key.share_memory);
    Some(ApprovalSubject {
        bot_id: bot.bot_id,
        name: bot.name,
        routines,
        shares_memory,
    })
}
