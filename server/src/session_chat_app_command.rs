/*
CDXC:SessionChatAppCommands 2026-08-23:
Slash commands GHOSTEX types into the agent, not the user.

Several flows write a command straight into the session's pty without the chat
composer ever being involved: the first-prompt auto-title job and the rename
modal's "Generate Name" stage `/rename <title>` (Pi `/name`, Hermes Agent
`/title`), and a fresh fork submits a provisional `Fork: <old title>` the same
way. Chat is a transcript projection, so what it shows afterwards depends
entirely on whether the CLI happens to record the command:

  * Claude Code writes a `local_command` row for everything it intercepts, so
    the send lands in the transcript and chat already renders it.
  * Codex records NOTHING for an intercepted command. The conversation simply
    did not move, and a session that renamed itself mid-thread looked like the
    chat had dropped whatever the user was doing.

So the app records what IT sent. This is deliberately a short-lived
ACKNOWLEDGEMENT, not an archive entry: the point is "Ghostex just did this",
which stops being worth a row once the agent's own record shows up (the client
drops ours when it finds the matching transcript envelope) or once enough time
has passed that nobody is still wondering. That also keeps the two agents from
disagreeing about history — nothing here is ever persisted, so a reload shows
the transcript and only the transcript.

The store is keyed by (project, session) and swept lazily on read, the same
shape as the terminal-notice watchdog map in session_chat_notice.rs.
*/

use std::collections::HashMap;
use std::sync::{Mutex, OnceLock};
use std::time::{Duration, Instant};

use serde_json::{json, Map, Value};

/*
Long enough to cover a rename the user was not looking at (the fork rename
fires four seconds after startup, the auto-title job after a model round trip),
short enough that a row cannot outlive anyone's memory of what caused it. The
client retires it earlier whenever the agent records the command itself.
*/
const APP_COMMAND_TTL: Duration = Duration::from_secs(300);

/// A session cannot plausibly be app-renamed more often than this; the cap only
/// exists so a runaway caller cannot grow the map without bound.
const APP_COMMAND_LIMIT: usize = 8;

#[derive(Clone, Debug)]
pub struct SessionChatAppCommand {
    /// Stable within a session, so a client can key rows without re-deriving
    /// identity from the text (two `/rename` sends can carry the same title).
    pub id: String,
    /// Verbatim command text as written to the pty, e.g. `/rename Fix parser`.
    pub command: String,
    /// RFC3339 millis, for display ordering only.
    pub sent_at: String,
    recorded: Instant,
}

impl SessionChatAppCommand {
    pub fn to_value(&self) -> Value {
        let mut map = Map::new();
        map.insert("id".to_string(), json!(self.id));
        map.insert("command".to_string(), json!(self.command));
        map.insert("sentAt".to_string(), json!(self.sent_at));
        Value::Object(map)
    }
}

type AppCommandStore = Mutex<HashMap<(String, String), Vec<SessionChatAppCommand>>>;

fn store() -> &'static AppCommandStore {
    static STORE: OnceLock<AppCommandStore> = OnceLock::new();
    STORE.get_or_init(|| Mutex::new(HashMap::new()))
}

fn prune(rows: &mut Vec<SessionChatAppCommand>, now: Instant) {
    rows.retain(|row| now.duration_since(row.recorded) < APP_COMMAND_TTL);
    if rows.len() > APP_COMMAND_LIMIT {
        rows.drain(..rows.len() - APP_COMMAND_LIMIT);
    }
}

/*
Call this at the point the command STRING is built, next to the dispatch that
writes it — not from inside the zmx send path. Ghostex also writes Ctrl+U/Ctrl+Y
draft-kill bytes and bare `\r` submits through that same path, and none of those
are commands the user needs told about.
*/
pub fn record_session_chat_app_command(project_id: &str, session_id: &str, command: &str) {
    let command = command.trim();
    if command.is_empty() {
        return;
    }
    let now = Instant::now();
    let sent_at = chrono::Utc::now().to_rfc3339_opts(chrono::SecondsFormat::Millis, true);
    let Ok(mut guard) = store().lock() else {
        return;
    };
    let rows = guard
        .entry((project_id.to_string(), session_id.to_string()))
        .or_default();
    rows.push(SessionChatAppCommand {
        id: format!("{sent_at}-{}", rows.len()),
        command: command.to_string(),
        sent_at,
        recorded: now,
    });
    prune(rows, now);
}

/// Live rows for a session, oldest first. Sweeps expired entries on the way out.
pub fn session_chat_app_commands(project_id: &str, session_id: &str) -> Vec<SessionChatAppCommand> {
    let now = Instant::now();
    let Ok(mut guard) = store().lock() else {
        return Vec::new();
    };
    let key = (project_id.to_string(), session_id.to_string());
    let Some(rows) = guard.get_mut(&key) else {
        return Vec::new();
    };
    prune(rows, now);
    if rows.is_empty() {
        guard.remove(&key);
        return Vec::new();
    }
    rows.clone()
}

/*
Stamped onto read results and onto every frame that can carry live state. Unlike
`terminalNotice`, an omitted field does NOT mean "cleared": these rows retire on
their own schedule and on the client's dedupe, so a frame that has nothing to add
simply says nothing rather than racing the client into dropping a row it should
still be showing.
*/
pub fn insert_session_chat_app_commands(
    frame: &mut Map<String, Value>,
    project_id: &str,
    session_id: &str,
) {
    let rows = session_chat_app_commands(project_id, session_id);
    if rows.is_empty() {
        return;
    }
    frame.insert(
        "appCommands".to_string(),
        Value::Array(rows.iter().map(SessionChatAppCommand::to_value).collect()),
    );
}

/*
What the 500ms long-poll fingerprint hashes. Ids alone: the text cannot change
under a fixed id, and this must stay allocation-cheap and I/O-free like every
other term in that hash.
*/
pub fn session_chat_app_commands_identity(project_id: &str, session_id: &str) -> String {
    session_chat_app_commands(project_id, session_id)
        .into_iter()
        .map(|row| row.id)
        .collect::<Vec<_>>()
        .join("\u{1f}")
}
