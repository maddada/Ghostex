//! Calls with a GBot, the GBots side: the GBot's voice (`gbots_bots.voice`, migration 0071), the
//! `/api/gbots` `voice` and `voiceSet` ops, the chat a call talks to, and the line a finished call
//! leaves in the GBot's daily log. The call itself is the Voice agent's
//! (server/src/voice_agent/gbot_call/).
//!
//! CDXC:GBots 2026-11-09 DECISION:
//! User (plan piece 16, M14): a call with a GBot is the Voice agent "with the bot's persona in its prompt" and a `talk_to_bot` tool that "sends the spoken request to the bot's standing chat and reads its answer back"; "the call's turns land in the bot's chat and daily log". A call started from the GBot's row (or the phone's long press) talks to your standing chat with it, the one `ghostex gbots ask` from you hands work to, and opens one ("Calls") when there is none; a call started from one of its chats talks to that chat.
//! CDXC:GBots 2026-11-09 DECISION:
//! A call reaches a GBot whose chats run on this computer. One given another computer or a Cloud Box (computer/) is refused with a sentence saying so: a Voice agent call about one session is about a session of the computer it runs on (voice_agent/focus.rs).

use rusqlite::{Connection, OptionalExtension};
use serde_json::{json, Map, Value};

use super::record::{bot_summary, color_hex, require_bot, Bot};
use super::session::{bot_chats, create_bot_chat_with, ChatOpening, CHAT_KEY};
use crate::domain::{DomainRepository, DomainStateError};
use crate::paths::GxserverPaths;
use crate::server::AppState;
use crate::storage::open_gxserver_database;

/// The longest voice id kept (the providers' ids are short words).
const VOICE_MAX_CHARS: usize = 32;
/// Characters of the standing instructions the call's persona carries.
const SOUL_CHARS: usize = 1_500;
/// The title of the chat a call opens when you have no standing chat with the GBot.
const CALL_CHAT_TITLE: &str = "Calls";

fn sql(error: rusqlite::Error) -> DomainStateError {
    DomainStateError {
        code: "internalError",
        message: format!("SQLite GBots error: {error}"),
    }
}

/// The GBot's voice, `None` for the Voice agent's own.
pub(crate) fn read_voice(db: &Connection, bot_id: &str) -> Option<String> {
    db.query_row(
        "SELECT voice FROM gbots_bots WHERE botId = ?1",
        [bot_id],
        |row| row.get::<_, Option<String>>(0),
    )
    .optional()
    .ok()
    .flatten()
    .flatten()
    .filter(|voice| !voice.trim().is_empty())
}

fn write_voice(db: &Connection, bot_id: &str, voice: Option<&str>) -> Result<(), DomainStateError> {
    db.execute(
        "UPDATE gbots_bots SET voice = ?2, updatedAt = ?3 WHERE botId = ?1",
        rusqlite::params![bot_id, voice, crate::coordinators::now_iso()],
    )
    .map_err(sql)?;
    Ok(())
}

pub(super) fn handles(op: &str) -> bool {
    matches!(op, "voice" | "voiceSet")
}

/// `voice` (anyone here): the GBot's voice and the voices it can have. `voiceSet` (you and Chief
/// of Staff): `voice`, one of either provider's voices, or "" for the Voice agent's own.
pub(super) fn run(
    state: &AppState,
    db: &Connection,
    may_manage: bool,
    op: &str,
    params: &Map<String, Value>,
) -> Result<Value, DomainStateError> {
    let reference = params
        .get("bot")
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|bot| !bot.is_empty())
        .ok_or_else(|| DomainStateError::bad_request("Pass bot."))?;
    let bot = require_bot(db, reference)?;
    if op == "voiceSet" {
        if !may_manage {
            return Err(DomainStateError {
                code: "forbidden",
                message: "Only you and Chief of Staff can change a GBot's voice.".to_string(),
            });
        }
        let voice = params
            .get("voice")
            .and_then(Value::as_str)
            .map(|voice| voice.trim().to_ascii_lowercase())
            .unwrap_or_default();
        if voice.is_empty() {
            write_voice(db, &bot.bot_id, None)?;
        } else {
            let choices = crate::voice_agent::gbot_voice_choices(&state.paths);
            let known = voice.chars().count() <= VOICE_MAX_CHARS
                && ["xai", "openai"].iter().any(|provider| {
                    choices
                        .pointer(&format!("/voiceOptions/{provider}"))
                        .and_then(Value::as_array)
                        .is_some_and(|voices| voices.iter().any(|v| v.as_str() == Some(&voice)))
                });
            if !known {
                return Err(DomainStateError::bad_request(format!(
                    "{voice} isn't a voice of the Voice agent. Run `ghostex gbots voice {}` to see them.",
                    bot.name
                )));
            }
            write_voice(db, &bot.bot_id, Some(&voice))?;
        }
    }
    Ok(voice_view(&state.paths, db, &bot))
}

/// The GBot, its voice, the provider calls use and the voices it offers.
fn voice_view(paths: &GxserverPaths, db: &Connection, bot: &Bot) -> Value {
    let mut view = crate::voice_agent::gbot_voice_choices(paths);
    view["bot"] = bot_summary(bot);
    view["voice"] = json!(read_voice(db, &bot.bot_id));
    view
}

/// Who a call is with: what the Voice agent's persona prompt and the clients need.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct CallBot {
    pub bot_id: String,
    pub name: String,
    pub title: String,
    pub description: String,
    /// The start of its standing instructions, for the persona.
    pub soul: String,
    pub icon: String,
    pub color: String,
    pub color_hex: String,
    pub voice: Option<String>,
}

impl CallBot {
    fn of(db: &Connection, bot: &Bot) -> Self {
        Self {
            bot_id: bot.bot_id.clone(),
            name: bot.name.clone(),
            title: bot.title.clone(),
            description: bot.description.clone(),
            soul: bot.soul.chars().take(SOUL_CHARS).collect(),
            icon: bot.icon.clone(),
            color: bot.color.clone(),
            color_hex: color_hex(&bot.color).to_string(),
            voice: read_voice(db, &bot.bot_id),
        }
    }

    /// The `gbot` object of a call's `focus` frame.
    pub(crate) fn to_client(&self) -> Value {
        json!({
            "botId": self.bot_id,
            "name": self.name,
            "title": self.title,
            "icon": self.icon,
            "color": self.color,
            "colorHex": self.color_hex,
            "voice": self.voice,
        })
    }
}

/// A call's refusal: a code for the client and the sentence to show.
pub(crate) struct CallRefusal {
    pub code: &'static str,
    pub message: String,
}

fn refusal(code: &'static str, message: impl Into<String>) -> CallRefusal {
    CallRefusal {
        code,
        message: message.into(),
    }
}

/// Why a call with the GBot `reference` can't start (GBots off, no such GBot, its chats run on
/// another computer), checked before the Voice agent's key so nothing is opened or billed first.
pub(crate) fn call_refusal(state: &AppState, reference: &str) -> Option<CallRefusal> {
    if !super::gbots_on(&state.paths) {
        return Some(refusal("gbotsOff", super::gbots_off_message(&state.paths)));
    }
    let db = match open_gxserver_database(&state.paths) {
        Ok(db) => db,
        Err(error) => {
            return Some(refusal(
                "internalError",
                format!("SQLite gxserver state error: {error}"),
            ))
        }
    };
    match require_bot(&db, reference) {
        Err(error) => Some(refusal("notFound", error.message)),
        Ok(bot) if bot.computer != "this" => Some(elsewhere(&bot)),
        Ok(_) => None,
    }
}

fn elsewhere(bot: &Bot) -> CallRefusal {
    refusal(
        "gbotElsewhere",
        format!(
            "{}'s chats run on another computer, so a call can't reach it from here. Calls reach GBots whose chats run on this computer.",
            bot.name
        ),
    )
}

/// The GBot `reference` names and the chat (`projectId:sessionId`) a call with it talks to:
/// `chat` when it is one of its open chats, else your standing chat with it, opened (and woken)
/// as needed. Blocking.
pub(crate) fn call_chat(
    state: &AppState,
    reference: &str,
    chat: Option<&str>,
) -> Result<(CallBot, String), CallRefusal> {
    if !super::gbots_on(&state.paths) {
        return Err(refusal("gbotsOff", super::gbots_off_message(&state.paths)));
    }
    let db = open_gxserver_database(&state.paths).map_err(|error| {
        refusal(
            "internalError",
            format!("SQLite gxserver state error: {error}"),
        )
    })?;
    let repository = DomainRepository::new(&db, &state.metadata.server_id);
    let bot = require_bot(&db, reference).map_err(|error| refusal("notFound", error.message))?;
    if bot.computer != "this" {
        return Err(elsewhere(&bot));
    }
    let failed = |error: DomainStateError| refusal("internalError", error.message);
    let chats = bot_chats(&repository, &bot).map_err(failed)?;
    let picked = match chat.map(str::trim).filter(|chat| !chat.is_empty()) {
        Some(wanted) => {
            let session_id = wanted.rsplit(':').next().unwrap_or(wanted);
            Some(
                chats
                    .iter()
                    .find(|session| text(session, "sessionId") == session_id)
                    .cloned()
                    .ok_or_else(|| {
                        refusal(
                            "focusNotFound",
                            format!("That chat isn't an open chat of {}.", bot.name),
                        )
                    })?,
            )
        }
        None => chats
            .iter()
            .find(|session| {
                session
                    .pointer(&format!("/runtimeSettings/{CHAT_KEY}/from"))
                    .and_then(Value::as_str)
                    == Some("user")
                    && session
                        .pointer(&format!("/runtimeSettings/{CHAT_KEY}/standing"))
                        .and_then(Value::as_bool)
                        == Some(true)
            })
            .cloned(),
    };
    let session = match picked {
        Some(session) => {
            super::assign::wake_if_asleep(state, &session);
            session
        }
        None => {
            create_bot_chat_with(
                state,
                &db,
                &repository,
                &bot,
                &ChatOpening {
                    opened_by: "user",
                    purpose: Some("call"),
                    from: Some("user"),
                    standing: true,
                    title: Some(CALL_CHAT_TITLE),
                    ..ChatOpening::default()
                },
            )
            .map_err(failed)?
            .0
        }
    };
    let reference = format!(
        "{}:{}",
        text(&session, "projectId"),
        text(&session, "sessionId")
    );
    Ok((CallBot::of(&db, &bot), reference))
}

fn text<'a>(session: &'a Value, key: &str) -> &'a str {
    session.get(key).and_then(Value::as_str).unwrap_or_default()
}

/// A finished call's line in the GBot's daily log: how long it was and how many things you said
/// to the GBot (each is in the chat already).
pub(crate) fn log_call(
    paths: &GxserverPaths,
    bot_id: &str,
    chat_title: &str,
    seconds: u64,
    messages: u64,
) {
    let folder = super::folder::bot_folder(paths, bot_id);
    if !folder.exists() {
        return;
    }
    let length = call_length(seconds);
    let said = match messages {
        0 => "nothing sent to the chat".to_string(),
        1 => "1 message in the chat".to_string(),
        count => format!("{count} messages in the chat"),
    };
    let _ = super::memory::append_log_line(
        &folder,
        chat_title,
        &format!("Call with you, {length}, {said}."),
    );
}

/// How long a call was, in the log's words: whole minutes, never rounded up (a 99-second call is
/// "a minute").
fn call_length(seconds: u64) -> String {
    match seconds / 60 {
        0 => "under a minute".to_string(),
        1 => "a minute".to_string(),
        minutes => format!("{minutes} minutes"),
    }
}

#[cfg(test)]
mod tests {
    use super::call_length;

    #[test]
    fn a_call_s_length_is_never_rounded_up() {
        assert_eq!(call_length(30), "under a minute");
        assert_eq!(call_length(60), "a minute");
        assert_eq!(call_length(99), "a minute");
        assert_eq!(call_length(150), "2 minutes");
        assert_eq!(call_length(179), "2 minutes");
        assert_eq!(call_length(600), "10 minutes");
    }
}
