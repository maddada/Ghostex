//! Who a chat send is from, decided by gxserver from the caller rather than from the text.
//!
//! CDXC:Security 2026-11-19 DECISION (Fable security reviews of 38bfa3647 and cff1806a3):
//! Only the user's own callers (the desktop app, the web build and phones over SSH, which use the
//! token file, and paired phones) put unmarked text into another session's chat, where it reads as
//! the user's own; what they send stays byte for byte as it was.
//! - An agent's own session token that writes into another session's chat
//!   (`/api/sendSessionChatMessage`, `/api/queueSessionChatPrompt`,
//!   `/api/updateSessionChatQueuedPrompt`, `/api/sendSessionMessage`) keeps its text only when it
//!   carries the exact "Message from another agent" block, at its start or end, naming that same
//!   session; anything else (no block, a block in a code fence, a forged block naming another
//!   agent) gets gxserver's own block for the session, first for a `/` or `!` body so it never
//!   runs as a command. An agent message to another computer is signed by the relay before it
//!   leaves (connected_computers/relay.rs `marked_message_body`, which refuses a body it cannot
//!   read as a message rather than pass it on unsigned); the other computer passes it on as it
//!   came.
//! - The "Sent from the terminal" / "Sent via Grok Bot" line and the security log entry
//!   (`chat.sendAsUser`) are added only when the CLI passes `via` (`ghostex agents send
//!   --as-user`), from the token file, and never to text that carries a `draftVersion`. For a chat
//!   on another computer the relay adds them, since that computer drops `via`.
//! - A "W7 Q12: …" answer settles a coordinator's ask, and a message's quotes of earlier messages
//!   are added (chat_message_numbers.rs), only when one of the user's own callers sent that text:
//!   the record is kept in gxserver's database (`session_chat_user_sends`), written when the send
//!   or queued prompt is accepted (never for a replay; withdrawn when it fails), so a restart or a
//!   queued prompt that goes out later still finds it, and an edit by the user records the new
//!   text.
//!
//! CDXC:Security 2026-11-20 DECISION (Fable's review of 809e881f4):
//! User (Q4256 A): anything holding one computer's token file or a paired phone counts as the user
//! on the other computer too, the rule each computer already applies to its own chats. The relay
//! marks such a caller's message `userSend` (`mark_relayed_send`) and strips the field from
//! everyone else's; the receiving computer honours it only from a computer whose grant the desktop
//! app issued (`owner_issued`, and only the owner session or the token file may issue, register,
//! unregister or revoke grants, connected_computers/http.rs), and drops it from every other
//! caller's message before any handler sees it. Held messages (`holdSessionChatSend`) are never
//! marked: that route does not record its caller and a session token may call it.
//! SEE-ALSO: server/src/ghostex_cli/agents/identity.rs `sender` (`--as-user`),
//! server/src/session_chat_queue_runtime/send.rs `settle_work_replies`.

use serde_json::{json, Map, Value};

use crate::coordinators::{place_agent_message_block, split_agent_message, AGENT_MESSAGE_OPENER};
use crate::paths::GxserverPaths;
use crate::security::Principal;

/// The label under a message the user sent from their own terminal with `--as-user`.
pub(crate) const FROM_TERMINAL: &str = "Sent from the terminal";
/// The label under a message the user had Grok Bot send from this computer.
pub(crate) const VIA_GROK_BOT: &str = "Sent via Grok Bot";

/// The routes whose `text` goes into a session's chat or terminal as a message.
pub(crate) const MESSAGE_ROUTES: [&str; 4] = [
    "/api/sendSessionChatMessage",
    "/api/queueSessionChatPrompt",
    "/api/updateSessionChatQueuedPrompt",
    "/api/sendSessionMessage",
];

/// How long a user's send stays recognised for settling answers.
const USER_SEND_TTL_MS: i64 = 7 * 24 * 60 * 60 * 1000;

fn user_sends_table(db: &rusqlite::Connection) -> rusqlite::Result<()> {
    db.execute_batch(
        "CREATE TABLE IF NOT EXISTS session_chat_user_sends (
           projectId TEXT NOT NULL, sessionId TEXT NOT NULL, text TEXT NOT NULL, atMs INTEGER NOT NULL
         );
         CREATE INDEX IF NOT EXISTS session_chat_user_sends_session
           ON session_chat_user_sends(projectId, sessionId);",
    )
}

/// Records that one of the user's own callers sent `text` to the session.
pub(crate) fn note_user_send(
    paths: &GxserverPaths,
    project_id: &str,
    session_id: &str,
    text: &str,
) {
    let text = text.trim();
    if text.is_empty() {
        return;
    }
    let Ok(db) = crate::storage::open_gxserver_database(paths) else {
        return;
    };
    let now = chrono::Utc::now().timestamp_millis();
    let _ = user_sends_table(&db).and_then(|()| {
        db.execute(
            "DELETE FROM session_chat_user_sends WHERE atMs < ?1",
            [now - USER_SEND_TTL_MS],
        )?;
        db.execute(
            "INSERT INTO session_chat_user_sends (projectId, sessionId, text, atMs) VALUES (?1, ?2, ?3, ?4)",
            rusqlite::params![project_id, session_id, text, now],
        )
    });
}

/// Whether one of the user's own callers sent `text` to the session, without forgetting it: a
/// message's quotes of earlier messages go only with the user's own sends
/// (chat_message_numbers.rs), never a Collab guest's or another agent's.
pub(crate) fn is_user_send(
    paths: &GxserverPaths,
    project_id: &str,
    session_id: &str,
    text: &str,
) -> bool {
    let Ok(db) = crate::storage::open_gxserver_database(paths) else {
        return false;
    };
    let now = chrono::Utc::now().timestamp_millis();
    user_sends_table(&db).is_ok()
        && db
            .query_row(
                "SELECT 1 FROM session_chat_user_sends
                 WHERE projectId = ?1 AND sessionId = ?2 AND text = ?3 AND atMs >= ?4 LIMIT 1",
                rusqlite::params![project_id, session_id, text.trim(), now - USER_SEND_TTL_MS],
                |_| Ok(()),
            )
            .is_ok()
}

/// Whether one of the user's own callers sent `text` to the session (and forgets that one send).
pub(crate) fn take_user_send(
    paths: &GxserverPaths,
    project_id: &str,
    session_id: &str,
    text: &str,
) -> bool {
    let Ok(db) = crate::storage::open_gxserver_database(paths) else {
        return false;
    };
    let now = chrono::Utc::now().timestamp_millis();
    user_sends_table(&db)
        .and_then(|()| {
            db.execute(
                "DELETE FROM session_chat_user_sends WHERE rowid = (
                   SELECT rowid FROM session_chat_user_sends
                   WHERE projectId = ?1 AND sessionId = ?2 AND text = ?3 AND atMs >= ?4 LIMIT 1)",
                rusqlite::params![project_id, session_id, text.trim(), now - USER_SEND_TTL_MS],
            )
        })
        .is_ok_and(|removed| removed > 0)
}

fn param<'a>(params: &'a Map<String, Value>, key: &str) -> &'a str {
    params
        .get(key)
        .and_then(Value::as_str)
        .map(str::trim)
        .unwrap_or_default()
}

/// Whether `text` carries the exact sender block, at its start or end, naming `own_ref`.
pub(crate) fn signed_by(text: &str, own_ref: &str) -> bool {
    split_agent_message(text).is_some_and(|parts| parts.reply_to == own_ref)
}

/// This computer's record of the session `project_id`/`session_id` (`Null` when it has none).
fn read_session(
    paths: &GxserverPaths,
    server_id: &str,
    project_id: &str,
    session_id: &str,
) -> Value {
    crate::storage::open_gxserver_database(paths)
        .ok()
        .and_then(|db| {
            crate::domain::DomainRepository::new(&db, server_id)
                .get_session(project_id, session_id)
                .ok()
                .flatten()
        })
        .unwrap_or(Value::Null)
}

/// gxserver's block for the agent session `session` of this computer.
fn session_block(session: &Value, server_id: &str, project_id: &str, session_id: &str) -> String {
    let field = |key: &str| {
        session
            .get(key)
            .and_then(Value::as_str)
            .map(|value| value.replace('\n', " "))
            .unwrap_or_default()
    };
    let or = |value: String, fallback: &str| {
        if value.trim().is_empty() {
            fallback.to_string()
        } else {
            value
        }
    };
    let agent_session_id = session
        .pointer("/runtimeSettings/agentSessionId")
        .and_then(Value::as_str)
        .unwrap_or("unavailable");
    format!(
        "{AGENT_MESSAGE_OPENER}\nAgent: {}\nSession: {}\nSession ID: {session_id}\nAgent ID: {}\nAgent Session ID: {agent_session_id}\nReply to: {}",
        or(field("agentId"), "agent"),
        or(field("title"), "an agent session"),
        or(field("agentId"), "unavailable"),
        crate::ids::create_global_session_ref(server_id, project_id, session_id),
    )
}

/// Gives an agent session's message gxserver's block unless it already carries its own exact one.
/// A `draftVersion` is the composer's, never an agent's, so it is dropped with the change.
pub(crate) fn sign_agent_message(
    paths: &GxserverPaths,
    server_id: &str,
    project_id: &str,
    session_id: &str,
    params: &mut Map<String, Value>,
) {
    let Some(text) = params
        .get("text")
        .and_then(Value::as_str)
        .map(str::to_string)
    else {
        return;
    };
    if text.trim().is_empty() {
        return;
    }
    let own_ref = crate::ids::create_global_session_ref(server_id, project_id, session_id);
    if signed_by(&text, &own_ref) {
        return;
    }
    let session = read_session(paths, server_id, project_id, session_id);
    // A plain (non-agent) terminal pane holds a session token too: the person at it sends as the
    // user, as the raw verbs always did there (CDXC:Coordinators 2026-10-24 in outbound.rs).
    if session
        .get("agentId")
        .and_then(Value::as_str)
        .is_none_or(|agent| agent.trim().is_empty())
    {
        return;
    }
    params.remove("draftVersion");
    let block = session_block(&session, server_id, project_id, session_id);
    params.insert(
        "text".to_string(),
        json!(place_agent_message_block(&block, &text)),
    );
}

/// Applies the rules above to a message route's body before it is handled. `via` is honoured only
/// from the token file and never passed on.
pub(crate) fn prepare(
    paths: &GxserverPaths,
    server_id: &str,
    route: &str,
    caller: &Principal,
    body: &mut Value,
) {
    let Some(params) = body.get_mut("params").and_then(Value::as_object_mut) else {
        return;
    };
    let via = params
        .remove("via")
        .and_then(|value| value.as_str().map(str::to_string));
    // The relay's mark never reaches a handler, whoever sent it; it is read below only from a
    // computer the desktop app connected (`owner_issued`).
    let relayed_user = params.remove(RELAYED_USER_SEND) == Some(Value::Bool(true));
    // Only this function sets the accepted-send mark; a caller's own copy is dropped.
    params.remove(USER_SEND_MARK);
    let target_project = param(params, "projectId").to_string();
    let target_session = param(params, "sessionId").to_string();
    match caller {
        Principal::Session {
            project_id,
            session_id,
            ..
        } => {
            if *project_id != target_project || *session_id != target_session {
                sign_agent_message(paths, server_id, project_id, session_id, params);
            }
        }
        Principal::Owner { .. } | Principal::Device { .. } | Principal::LocalAgent => {
            let Some(text) = params
                .get("text")
                .and_then(Value::as_str)
                .map(str::to_string)
            else {
                return;
            };
            if text.trim().is_empty() {
                return;
            }
            let text =
                label_as_user_send(server_id, caller, via.as_deref(), params).unwrap_or(text);
            // Recorded once the send is accepted (`note_accepted_user_send`); an edit by the user
            // records its new text, which the scheduler's later send settles by.
            if route != "/api/sendSessionMessage" && !text.trim().is_empty() {
                params.insert(USER_SEND_MARK.to_string(), Value::Bool(true));
            }
        }
        // Another of the user's computers relays a message one of the user's own callers sent
        // there with `userSend` (connected_computers/relay.rs `mark_relayed_send`); its agents'
        // messages come signed and without it. Recorded as the user's, so the message's quotes
        // of earlier messages work in chats on this computer too.
        Principal::Peer { server_id, .. } => {
            let from_user = relayed_user && crate::connected_computers::owner_issued(server_id);
            let text = param(params, "text").to_string();
            // Not a security check (a forger simply leaves the block out; the mark itself is what
            // is trusted): it keeps an honest agent's message that its own CLI signed but sent
            // through the token file (Empryo has no session variables) from counting as the
            // user's. The same line test as `settle_work_replies` in
            // session_chat_queue_runtime/send.rs, so a user's words that only mention the phrase
            // still count.
            let signed_by_an_agent = text
                .lines()
                .any(|line| line.trim() == crate::coordinators::AGENT_MESSAGE_OPENER);
            if from_user
                && !text.trim().is_empty()
                && !signed_by_an_agent
                && route != "/api/sendSessionMessage"
            {
                params.insert(USER_SEND_MARK.to_string(), Value::Bool(true));
            }
        }
        // Anything else was marked by the computer it came from before it came (see above).
        _ => {}
    }
}

/// The mark `prepare` puts on a message one of the user's own callers sent (or that a computer the
/// desktop app connected relayed as theirs), for the handler to record once it accepts the send
/// (`note_accepted_user_send`). Any copy a caller sends is dropped by `prepare`.
pub(crate) const USER_SEND_MARK: &str = "ghostexUserSend";

/// Records a message `prepare` marked as the user's, when its handler accepts it: before the send
/// types it (its quotes of earlier messages check the record), and never for a replay the send
/// ledger answers. Answers what to withdraw (`withdraw_user_send`) if the send then fails.
pub(crate) fn note_accepted_user_send(
    paths: &GxserverPaths,
    body: &Value,
) -> Option<(String, String, String)> {
    let params = body.get("params")?.as_object()?;
    if params.get(USER_SEND_MARK) != Some(&Value::Bool(true)) {
        return None;
    }
    let (project_id, session_id, text) = (
        param(params, "projectId").to_string(),
        param(params, "sessionId").to_string(),
        param(params, "text").to_string(),
    );
    if text.trim().is_empty() {
        return None;
    }
    note_user_send(paths, &project_id, &session_id, &text);
    Some((project_id, session_id, text))
}

/// Forgets a user send recorded for a send that was then refused or failed.
pub(crate) fn withdraw_user_send(paths: &GxserverPaths, noted: Option<(String, String, String)>) {
    if let Some((project_id, session_id, text)) = noted {
        take_user_send(paths, &project_id, &session_id, &text);
    }
}

/// `ghostex agents send --as-user`'s label and security log entry: only it passes `via`, from the
/// token file, and never with a `draftVersion`; the app's, the web build's and a phone's sends stay
/// exactly as they were. Labels `params`' text ("Sent from the terminal", "Sent via Grok Bot") and
/// answers it, or `None` when nothing applies. `server_id` is the computer whose chat it goes to:
/// this one, or the one a relay passes it on to (connected_computers/relay.rs), which drops `via`.
pub(crate) fn label_as_user_send(
    server_id: &str,
    caller: &Principal,
    via: Option<&str>,
    params: &mut Map<String, Value>,
) -> Option<String> {
    let label = match via {
        Some("grokBot") => VIA_GROK_BOT,
        Some("terminal") => FROM_TERMINAL,
        _ => return None,
    };
    if !matches!(caller, Principal::LocalAgent) || params.contains_key("draftVersion") {
        return None;
    }
    let text = param(params, "text").to_string();
    if text.trim().is_empty() {
        return None;
    }
    let target = crate::ids::create_global_session_ref(
        server_id,
        param(params, "projectId"),
        param(params, "sessionId"),
    );
    crate::audit::record(
        crate::audit::Action::new(
            "chat.sendAsUser",
            format!(
                "Sent a message into {target}'s chat as you ({})",
                label.to_lowercase()
            ),
        )
        .target(target.clone()),
        crate::audit::Actor::current(),
        crate::audit::Outcome::Done,
        None,
    );
    // A `/` or `!` command from the user's own terminal still runs as one.
    let labelled = if text.trim_start().starts_with(['/', '!']) {
        text
    } else {
        format!("{}\n\n{label}", text.trim_end())
    };
    params.insert("text".to_string(), json!(labelled));
    Some(labelled)
}

/// The flag a relay sets on a message one of the user's own callers sent through it to another
/// computer (`mark_relayed_send`); only that computer's gxserver can set it, since only a peer's
/// message is read for it.
pub(crate) const RELAYED_USER_SEND: &str = "userSend";

/// Marks a message route's body before this computer relays it to another: `userSend` when one of
/// the user's own callers sent it, never for anyone else (an agent's session, which is signed
/// instead, cannot set it either).
pub(crate) fn mark_relayed_send(caller: &Principal, params: &mut Map<String, Value>) {
    params.remove(RELAYED_USER_SEND);
    if is_users_caller(caller) {
        params.insert(RELAYED_USER_SEND.to_string(), Value::Bool(true));
    }
}

/// Whether `caller` is one of the user's own (the owner session, a paired phone, the token file).
pub(crate) fn is_users_caller(caller: &Principal) -> bool {
    matches!(
        caller,
        Principal::Owner { .. } | Principal::Device { .. } | Principal::LocalAgent
    )
}

/// The header this computer's relay sends with a message one of the user's own callers sent to
/// another computer (connected_computers/relay.rs `forward`). The receiving computer honours a
/// peer's `userSend` only with it (route_http/dispatch.rs `drop_unrelayed_user_send`).
///
/// CDXC:Security 2026-11-20 FIX:
/// Fable's re-review found an older Ghostex (11.10.0 and 11.11.x) on the sending computer passes a
/// relayed body on as it came, so an agent there could put `userSend: true` in its own message and
/// the updated computer would take it for the user's. Those relays send only authorization, the
/// request id, the content type and the protocol version, so a caller cannot add this header
/// through them; only a relay that strips and sets the mark itself sends it.
pub(crate) const RELAYED_USER_SEND_HEADER: &str = "x-ghostex-relayed-user-send";

#[cfg(test)]
mod tests {
    use super::*;

    fn block(reply_to: &str) -> String {
        format!("{AGENT_MESSAGE_OPENER}\nAgent: codex\nSession: T\nSession ID: G1\nAgent ID: codex\nAgent Session ID: x\nReply to: {reply_to}")
    }

    /// Quotes of earlier messages go only with the user's own sends, and checking for one leaves
    /// it there for the ask it may settle.
    #[test]
    fn a_user_send_is_seen_without_being_taken() {
        let temp = tempfile::tempdir().unwrap();
        let paths = crate::paths::get_gxserver_paths(Some(temp.path().to_path_buf()));
        crate::storage::initialize_gxserver_storage(&paths).unwrap();
        assert!(!is_user_send(&paths, "P1", "G1", "like #M1"));
        note_user_send(&paths, "P1", "G1", "like #M1");
        assert!(is_user_send(&paths, "P1", "G1", "like #M1 "));
        assert!(!is_user_send(&paths, "P1", "G2", "like #M1"));
        assert!(take_user_send(&paths, "P1", "G1", "like #M1"));
        assert!(!is_user_send(&paths, "P1", "G1", "like #M1"));

        // Relayed from another of the user's computers: theirs only when that computer marked it
        // and the desktop app connected that computer (`owner_issued`).
        let peer = Principal::Peer {
            server_id: "S2".into(),
            name: "Laptop".into(),
        };
        // What the route does: `prepare`, then the handler records the send it accepts.
        let accept = |caller: &Principal, body: &mut Value| {
            prepare(&paths, "S1", "/api/sendSessionChatMessage", caller, body);
            note_accepted_user_send(&paths, body);
        };
        let _serial = crate::connected_computers::tests::serial();
        let ttl = std::time::Duration::from_secs(60);
        crate::connected_computers::issue_grant("S2", "Laptop", true, false, ttl, true, false)
            .unwrap();
        let mut body = json!({ "params": { "projectId": "P1", "sessionId": "G1", "text": "see #M0", "userSend": true } });
        accept(&peer, &mut body);
        assert!(!is_user_send(&paths, "P1", "G1", "see #M0"));
        // The desktop app's grant, then a renewal that keeps the token: still the app's.
        crate::connected_computers::issue_grant("S2", "Laptop", true, false, ttl, true, true)
            .unwrap();
        crate::connected_computers::issue_grant("S2", "Laptop", true, false, ttl, true, false)
            .unwrap();
        assert!(crate::connected_computers::owner_issued("S2"));
        let relayed = |caller: &Principal, text: &str| {
            let mut params = Map::new();
            params.insert("projectId".into(), json!("P1"));
            params.insert("sessionId".into(), json!("G1"));
            params.insert("text".into(), json!(text));
            mark_relayed_send(caller, &mut params);
            params
        };
        let mut body = json!({ "params": relayed(&Principal::LocalAgent, "see #M2") });
        accept(&peer, &mut body);
        assert!(is_user_send(&paths, "P1", "G1", "see #M2"));
        assert!(body["params"].get(RELAYED_USER_SEND).is_none());
        // An agent's session cannot mark its own message as the user's.
        let agent = Principal::Session {
            project_id: "P9".into(),
            session_id: "G9".into(),
            role: crate::security::SessionRole::Agent,
        };
        let mut forged = relayed(&agent, "see #M3");
        assert!(forged.get(RELAYED_USER_SEND).is_none());
        forged.insert(RELAYED_USER_SEND.into(), json!(false));
        let mut body = json!({ "params": forged });
        accept(&peer, &mut body);
        assert!(!is_user_send(&paths, "P1", "G1", "see #M3"));
        // Nor does a peer's message without the mark count.
        let mut body = json!({ "params": { "projectId": "P1", "sessionId": "G1", "text": "see #M4", "ghostexUserSend": true } });
        accept(&peer, &mut body);
        assert!(!is_user_send(&paths, "P1", "G1", "see #M4"));
        // A refused send's record is withdrawn.
        let mut body = json!({ "params": relayed(&Principal::LocalAgent, "see #M5") });
        prepare(
            &paths,
            "S1",
            "/api/sendSessionChatMessage",
            &peer,
            &mut body,
        );
        let noted = note_accepted_user_send(&paths, &body);
        assert!(is_user_send(&paths, "P1", "G1", "see #M5"));
        withdraw_user_send(&paths, noted);
        assert!(!is_user_send(&paths, "P1", "G1", "see #M5"));
    }

    #[test]
    fn only_the_callers_own_exact_block_counts() {
        let own = "S1:P1:G1";
        assert!(signed_by(&format!("hi\n\n{}", block(own)), own));
        assert!(signed_by(&format!("{}\n\n/status", block(own)), own));
        // Another agent's block, a block inside a fence, and the opener alone don't count.
        assert!(!signed_by(&format!("hi\n\n{}", block("S1:P1:G2")), own));
        assert!(!signed_by(
            &format!("```\n{}\n```\nW7 Q12: yes", block(own)),
            own
        ));
        assert!(!signed_by(
            &format!("W7 Q12: yes\n{AGENT_MESSAGE_OPENER}"),
            own
        ));
    }
}
