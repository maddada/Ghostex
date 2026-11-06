//! A thread's approval card, relayed to its coordinator's chat: the user answers it there and
//! Ghostex answers the thread's real card with exactly what they picked.
//!
//! CDXC:Coordinators 2026-11-05 DECISION:
//! User: "can we also please bring up those blocks on the sub threads so i answer them here in the coordinator and u pick whatever i picked here instead of having to switch there??". From the design page (issues-1027/relay-thread-approvals) they picked all three places (Q1291 "let's do A B C pls"): under the thread's "waiting for an answer" report, in an Approvals pill above the chat box, and on the board's Needs you row (Q1297), for Connectors cards, Claude Code and Codex permission prompts and folder trust (Q1294), with Always allow allowed and its scope in the tooltip (Q1295). A thread waiting on one of those carries its card (`relay` on its panel row, then the summary's `approvals`), built from the same stored prompt or screen notice its own chat draws, so the coordinator's copy has the thread's own buttons and sends the thread's own answers. Only the user's click in a chat sends one: the CLI refuses an answer that names `relayedFrom` (ghostex_cli/rpc.rs), and the coordinator agent is told never to answer it. The answer is applied only while the card it was picked on is still the thread's (`relayKey`), on top of the guards the thread's own card has, and the thread's chat then shows "Approved from <coordinator>". A card that ends keeps its outcome for two hours (Q1293: "Answered in the thread", expired, or moved on), so a report that arrives late reads as settled.
//! SEE-ALSO: server/src/coordinators/panel.rs (rows and summary), server/src/session_chat_send/answer_http.rs (the relayed lane), server/src/server/coordinator_runtime.rs (the report re-check), packages/gx-chat-core/src/extras/relayed_approvals.rs (the card).

use std::collections::HashMap;
use std::sync::{Mutex, OnceLock, PoisonError};

use serde_json::{json, Map, Value};

use super::records::SessionKey;
use crate::agents::session_chat_prompt_setting;
use crate::connectors::{connector_card_outcome, ConnectorCardAnswer, CONNECTOR_CARD_LABELS};
use crate::domain::DomainStateError;
use crate::paths::GxserverPaths;
use crate::session_chat_interactive::{
    parse_stored_session_chat_prompt, SessionChatInteractivePrompt,
};

/// How long an ended card stays in the coordinator's chat with its outcome.
const SETTLED_KEEP_MS: i64 = 2 * 60 * 60 * 1000;
/// Ended cards kept per thread.
const SETTLED_PER_THREAD: usize = 3;
/// How long the thread's chat shows "Approved from <coordinator>".
const AUDIT_SHOW_MS: i64 = 12 * 60 * 60 * 1000;
/// Audit lines kept per thread.
const AUDIT_PER_THREAD: usize = 10;
const AUDIT_FILE: &str = "coordinator-relayed-answers.json";

/// The answer kinds a relayed card sends: a Connectors card's pick, a hook approval's Allow or
/// Deny, a permission prompt's or folder-trust question's row (plain or in a live dialog), and
/// Trust and Remember.
const RELAYED_KINDS: [&str; 5] = [
    "question",
    "approval",
    "terminalChoice",
    "terminalDialog",
    "trustAndRemember",
];

fn now_ms() -> i64 {
    chrono::Utc::now().timestamp_millis()
}

fn iso(ms: i64) -> String {
    chrono::DateTime::from_timestamp_millis(ms)
        .unwrap_or_default()
        .to_rfc3339_opts(chrono::SecondsFormat::Millis, true)
}

fn short_hash(text: &str) -> String {
    use sha2::{Digest, Sha256};
    hex::encode(Sha256::digest(text.as_bytes()))
        .chars()
        .take(16)
        .collect()
}

/// The card a waiting thread relays: a Connectors card, a hook approval (with the permission
/// prompt its screen shows, when the chat read one), a permission prompt read off the screen, or
/// a folder-trust question. `None` for anything else, which stays the coordinator's to answer.
///
/// The key names this one card: the stored prompt while there is one, so a screen reading coming
/// and going does not make it a new card; the notice's `detectedAt` otherwise, which is new for
/// every new prompt on the screen.
pub fn relay_card(session: &Value) -> Option<Value> {
    let stored_text = session_chat_prompt_setting(session);
    let stored = stored_text
        .as_deref()
        .and_then(parse_stored_session_chat_prompt);
    let notice = super::state::screen_wait_notice(session);
    let notice_kind = notice
        .as_ref()
        .and_then(|notice| notice.get("kind"))
        .and_then(Value::as_str)
        .unwrap_or_default()
        .to_string();
    let (kind, key) = match &stored {
        Some(prompt) if crate::connectors::is_connector_card(prompt) => (
            "connector",
            short_hash(stored_text.as_deref().unwrap_or_default()),
        ),
        Some(SessionChatInteractivePrompt::Approval { .. }) => (
            "permission",
            short_hash(stored_text.as_deref().unwrap_or_default()),
        ),
        _ => {
            let kind = match notice_kind.as_str() {
                "permissionPrompt" => "permission",
                "trustPrompt" => "trust",
                _ => return None,
            };
            let detected_at = notice
                .as_ref()
                .and_then(|notice| notice.get("detectedAt"))
                .and_then(Value::as_str)
                .unwrap_or_default();
            (kind, short_hash(&format!("{notice_kind}:{detected_at}")))
        }
    };
    let mut card = json!({ "key": key, "kind": kind });
    // The stored prompt goes as its chat frame carries it (`prompt`); a trust question has none.
    if kind != "trust" {
        if let Some(prompt) = stored_text
            .as_deref()
            .and_then(|text| serde_json::from_str::<Value>(text).ok())
        {
            card["prompt"] = prompt;
        }
    }
    // A Connectors card is the stored prompt alone; the others draw the screen's rows when it
    // shows the prompt (a trust question has nothing else).
    if kind != "connector" && matches!(notice_kind.as_str(), "permissionPrompt" | "trustPrompt") {
        if let Some(notice) = notice {
            card["notice"] = notice;
        }
    }
    if kind == "connector" {
        card["labels"] = json!(CONNECTOR_CARD_LABELS);
    }
    Some(card)
}

/// One answer the user sent from a coordinator's chat, until the card it answered ends.
#[derive(Clone, Debug)]
struct AnsweredHere {
    label: String,
    coordinator_title: String,
    at_ms: i64,
}

#[derive(Clone, Debug)]
struct LiveCard {
    key: String,
    card: Value,
    asked_ms: i64,
}

#[derive(Default)]
struct ThreadRelay {
    live: Option<LiveCard>,
    /// Ended cards, newest last.
    settled: Vec<(i64, Value)>,
    /// Answers sent from the coordinator's chat, by card key.
    answered_here: HashMap<String, AnsweredHere>,
}

fn relays() -> &'static Mutex<HashMap<SessionKey, ThreadRelay>> {
    static RELAYS: OnceLock<Mutex<HashMap<SessionKey, ThreadRelay>>> = OnceLock::new();
    RELAYS.get_or_init(|| Mutex::new(HashMap::new()))
}

/// How the card `live` of thread `thread` ended. `replaced`: the thread now shows another card.
fn ended(relay: &mut ThreadRelay, live: &LiveCard, replaced: bool, now: i64) -> Value {
    let mut card = live.card.clone();
    card["settledAt"] = json!(iso(now));
    if let Some(answered) = relay.answered_here.remove(&live.key) {
        card["status"] = json!("answeredHere");
        card["answer"] = json!(answered.label);
        card["answeredFrom"] = json!(answered.coordinator_title);
        card["settledAt"] = json!(iso(answered.at_ms));
        return card;
    }
    let connector_outcome = (live.card["kind"] == "connector")
        .then(|| live.card["prompt"]["toolUseId"].as_str())
        .flatten()
        .and_then(connector_card_outcome);
    let (status, answer) = match connector_outcome {
        Some(ConnectorCardAnswer::Approve) => ("answeredInThread", CONNECTOR_CARD_LABELS[0]),
        Some(ConnectorCardAnswer::Always) => ("answeredInThread", CONNECTOR_CARD_LABELS[1]),
        Some(ConnectorCardAnswer::Deny) => ("answeredInThread", CONNECTOR_CARD_LABELS[2]),
        Some(ConnectorCardAnswer::Expired) => ("expired", ""),
        Some(ConnectorCardAnswer::Cancelled | ConnectorCardAnswer::Restarting) => ("movedOn", ""),
        None if replaced => ("movedOn", ""),
        None => ("answeredInThread", ""),
    };
    card["status"] = json!(status);
    if !answer.is_empty() {
        card["answer"] = json!(answer);
    }
    card
}

/// Follows every listed thread's card from one tick to the next and returns the coordinator's
/// cards: the open ones first (oldest asked first), then the ones that ended in the last two
/// hours (newest first). Takes each row's `relay` off it, so a frame carries each card once.
pub fn track_approvals(rows: &mut [Value]) -> Value {
    let now = now_ms();
    let mut relays = relays().lock().unwrap_or_else(PoisonError::into_inner);
    let mut open = Vec::new();
    let mut settled = Vec::new();
    for row in rows.iter_mut() {
        // Threads on another computer are answered there.
        if row["serverId"].as_str().is_some_and(|id| !id.is_empty()) {
            continue;
        }
        let Some(object) = row.as_object_mut() else {
            continue;
        };
        let current = object.remove("relay");
        let thread: SessionKey = (
            object
                .get("projectId")
                .and_then(Value::as_str)
                .unwrap_or_default()
                .to_string(),
            object
                .get("sessionId")
                .and_then(Value::as_str)
                .unwrap_or_default()
                .to_string(),
        );
        let identity = json!({
            "projectId": thread.0,
            "sessionId": thread.1,
            "title": object.get("title").cloned().unwrap_or(Value::Null),
            "globalRef": object.get("globalRef").cloned().unwrap_or(Value::Null),
            "agent": object.get("agent").cloned().unwrap_or(Value::Null),
        });
        let relay = relays.entry(thread.clone()).or_default();
        let current_key = current
            .as_ref()
            .and_then(|card| card["key"].as_str())
            .map(str::to_string);
        if let Some(live) = relay.live.take() {
            if Some(&live.key) == current_key.as_ref() {
                relay.live = Some(live);
            } else {
                let card = ended(relay, &live, current_key.is_some(), now);
                relay.settled.push((now, card));
                let excess = relay.settled.len().saturating_sub(SETTLED_PER_THREAD);
                relay.settled.drain(..excess);
            }
        }
        if let (Some(card), Some(key)) = (current, current_key) {
            let live = relay.live.get_or_insert_with(|| LiveCard {
                key,
                card: Value::Null,
                asked_ms: now,
            });
            let mut card = card;
            if let (Some(card), Some(identity)) = (card.as_object_mut(), identity.as_object()) {
                card.extend(identity.clone());
            }
            card["status"] = json!("open");
            card["askedAt"] = json!(iso(live.asked_ms));
            live.card = card.clone();
            open.push((live.asked_ms, card));
        }
        relay.settled.retain(|(at, _)| now - at < SETTLED_KEEP_MS);
        settled.extend(relay.settled.iter().cloned());
        // Answers recorded for a card that never showed again go with it.
        relay
            .answered_here
            .retain(|_, answered| now - answered.at_ms < SETTLED_KEEP_MS);
    }
    open.sort_by_key(|(asked, _)| *asked);
    settled.sort_by_key(|(at, _)| std::cmp::Reverse(*at));
    Value::Array(
        open.into_iter()
            .chain(settled)
            .map(|(_, card)| card)
            .collect(),
    )
}

/// The thread's open card, when its key is `key`.
fn live_card(thread: &SessionKey, key: &str) -> Option<Value> {
    relays()
        .lock()
        .unwrap_or_else(PoisonError::into_inner)
        .get(thread)
        .and_then(|relay| relay.live.as_ref())
        .filter(|live| live.key == key)
        .map(|live| live.card.clone())
}

/// What a relayed answer names: the coordinator chat it was sent from and the card it answers.
#[derive(Clone, Debug)]
pub struct RelayedAnswer {
    pub thread: SessionKey,
    pub coordinator: SessionKey,
    pub key: String,
    pub label: String,
    coordinator_title: String,
    question: String,
}

fn refused(message: &str) -> DomainStateError {
    DomainStateError {
        code: "invalidState",
        message: message.to_string(),
    }
}

/// `answerSessionChatPrompt` with `relayedFrom`: the answer is for a coordinator's thread, picked
/// on its relayed card. `None` when the answer is no relayed one. Checks that the thread belongs
/// to that coordinator, that the card it was picked on is still the thread's, and that the answer
/// is one a card sends; then marks the card answered from the coordinator until [`settle_answer`]
/// says how the answer went. Blocking.
pub fn begin_relayed_answer(
    db: &rusqlite::Connection,
    repository: &crate::domain::DomainRepository<'_>,
    thread: &SessionKey,
    params: &Map<String, Value>,
) -> Option<Result<RelayedAnswer, DomainStateError>> {
    let from = params.get("relayedFrom")?;
    Some((|| {
        let text = |key: &str| {
            from.get(key)
                .and_then(Value::as_str)
                .map(str::trim)
                .unwrap_or_default()
                .to_string()
        };
        let coordinator = (text("projectId"), text("sessionId"));
        let key = text("key");
        if coordinator.0.is_empty() || coordinator.1.is_empty() || key.is_empty() {
            return Err(DomainStateError::bad_request(
                "A relayed answer names its coordinator and card.",
            ));
        }
        let kind = params
            .get("kind")
            .and_then(Value::as_str)
            .unwrap_or_default();
        if !RELAYED_KINDS.contains(&kind) {
            return Err(DomainStateError::bad_request(
                "A relayed card only answers an approval.",
            ));
        }
        let record = super::records::read_thread(db, &thread.0, &thread.1)?;
        if record.is_none_or(|record| record.coordinator_key() != coordinator) {
            return Err(refused(
                "That session is not one of this coordinator's threads.",
            ));
        }
        let Some(card) = live_card(thread, &key) else {
            return Err(refused(
                "That approval changed before your answer arrived. Look at the card again.",
            ));
        };
        let coordinator_title = repository
            .get_session(&coordinator.0, &coordinator.1)
            .ok()
            .flatten()
            .map(|session| super::endpoint::session_title_of(&session))
            .filter(|title| !title.trim().is_empty())
            .unwrap_or_else(|| "its coordinator".to_string());
        let label = from
            .get("label")
            .and_then(Value::as_str)
            .map(str::trim)
            .filter(|label| !label.is_empty())
            .unwrap_or("Answered")
            .chars()
            .take(120)
            .collect::<String>();
        let answer = RelayedAnswer {
            thread: thread.clone(),
            coordinator,
            key: key.clone(),
            label: label.clone(),
            coordinator_title: coordinator_title.clone(),
            question: card_question(&card),
        };
        relays()
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .entry(thread.clone())
            .or_default()
            .answered_here
            .insert(
                key,
                AnsweredHere {
                    label,
                    coordinator_title,
                    at_ms: now_ms(),
                },
            );
        Ok(answer)
    })())
}

/// An answer the CLI sent from inside an agent session (`agentCaller`, ghostex_cli/rpc.rs) for a
/// thread whose card waits on the user in its coordinator's chat: refused, whatever it answers,
/// so the coordinator cannot answer the thread's card around the relay. Answers to anything else
/// (an agent driving a test session, a thread's own question) go on as before.
pub fn refuse_agent_answer(
    thread: &SessionKey,
    params: &Map<String, Value>,
) -> Option<DomainStateError> {
    params.get("agentCaller")?;
    let kind = params
        .get("kind")
        .and_then(Value::as_str)
        .unwrap_or_default();
    if !RELAYED_KINDS.contains(&kind) {
        return None;
    }
    let relays = relays().lock().unwrap_or_else(PoisonError::into_inner);
    relays.get(thread)?.live.as_ref()?;
    Some(refused(
        "Only the user can answer this thread's approval: it is waiting on its card in the coordinator's chat. Tell the user it is there; never answer it for them.",
    ))
}

/// What the card asked, in a line, for the thread's audit line.
fn card_question(card: &Value) -> String {
    let notice = &card["notice"];
    let from_notice = notice["permissionRequest"]["subject"]
        .as_str()
        .or_else(|| notice["title"].as_str());
    let from_prompt = card["prompt"]["questions"][0]["question"]
        .as_str()
        .map(|question| question.split("\n\n").next().unwrap_or_default())
        .or_else(|| card["prompt"]["summary"].as_str())
        .or_else(|| card["prompt"]["tool"].as_str());
    from_notice
        .or(from_prompt)
        .unwrap_or_default()
        .trim()
        .chars()
        .take(200)
        .collect()
}

/// Ends a relayed answer: a refused one is forgotten, so the card stays open; an applied one
/// writes the thread's audit line ("Approved from Main Coordinator") and republishes its chat.
pub(crate) fn settle_answer(
    state: &crate::server::AppState,
    answer: &RelayedAnswer,
    applied: bool,
) {
    if !applied {
        if let Some(relay) = relays()
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .get_mut(&answer.thread)
        {
            relay.answered_here.remove(&answer.key);
        }
        return;
    }
    record_audit(
        &state.paths,
        &answer.thread,
        json!({
            "at": iso(now_ms()),
            "label": answer.label,
            "from": answer.coordinator_title,
            "fromProjectId": answer.coordinator.0,
            "fromSessionId": answer.coordinator.1,
            "question": answer.question,
        }),
    );
    crate::server::republish_session_chat(state, &answer.thread.0, &answer.thread.1);
}

fn audit() -> &'static Mutex<Option<HashMap<String, Vec<Value>>>> {
    static AUDIT: OnceLock<Mutex<Option<HashMap<String, Vec<Value>>>>> = OnceLock::new();
    AUDIT.get_or_init(|| Mutex::new(None))
}

fn audit_key(thread: &SessionKey) -> String {
    format!("{}:{}", thread.0, thread.1)
}

/// Reads the audit lines a previous gxserver wrote, once; frames show nothing until then.
pub fn load_audit(paths: &GxserverPaths) {
    let mut audit = audit().lock().unwrap_or_else(PoisonError::into_inner);
    if audit.is_some() {
        return;
    }
    let stored = std::fs::read_to_string(paths.app_state_dir.join(AUDIT_FILE))
        .ok()
        .and_then(|text| serde_json::from_str::<HashMap<String, Vec<Value>>>(&text).ok())
        .unwrap_or_default();
    *audit = Some(stored);
}

fn record_audit(paths: &GxserverPaths, thread: &SessionKey, line: Value) {
    load_audit(paths);
    let snapshot = {
        let mut audit = audit().lock().unwrap_or_else(PoisonError::into_inner);
        let lines = audit.get_or_insert_with(HashMap::new);
        let entry = lines.entry(audit_key(thread)).or_default();
        entry.push(line);
        let excess = entry.len().saturating_sub(AUDIT_PER_THREAD);
        entry.drain(..excess);
        // Threads whose last line is long gone are dropped from the file.
        let now = now_ms();
        lines.retain(|_, entries| {
            entries.last().is_some_and(|line| {
                line["at"]
                    .as_str()
                    .and_then(|at| chrono::DateTime::parse_from_rfc3339(at).ok())
                    .is_some_and(|at| now - at.timestamp_millis() < 7 * 24 * 60 * 60 * 1000)
            })
        });
        serde_json::to_string(lines).unwrap_or_default()
    };
    let _ = std::fs::create_dir_all(&paths.app_state_dir);
    let _ = std::fs::write(paths.app_state_dir.join(AUDIT_FILE), snapshot);
}

/// Adds `relayedAnswers` to a thread's chat frame: the answers its coordinator's chat sent in the
/// last 12 hours, oldest first, which the thread's chat shows where its card was.
pub fn insert_relayed_answers(frame: &mut Map<String, Value>, project_id: &str, session_id: &str) {
    let audit = audit().lock().unwrap_or_else(PoisonError::into_inner);
    let Some(lines) = audit.as_ref().and_then(|audit| {
        audit.get(&audit_key(&(
            project_id.to_string(),
            session_id.to_string(),
        )))
    }) else {
        return;
    };
    let now = now_ms();
    let recent: Vec<Value> = lines
        .iter()
        .filter(|line| {
            line["at"]
                .as_str()
                .and_then(|at| chrono::DateTime::parse_from_rfc3339(at).ok())
                .is_some_and(|at| now - at.timestamp_millis() < AUDIT_SHOW_MS)
        })
        .cloned()
        .collect();
    // Sent even when every line is old, so an open chat drops the last one.
    frame.insert("relayedAnswers".to_string(), Value::Array(recent));
}

/// The waiting report's last paragraph when the thread's card is relayed to the coordinator's
/// chat: the user answers it there, and the coordinator never does.
pub const RELAYED_REPORT_NOTE: &str = "This approval card is also in the user's chat with you: under this report, in the Approvals pill above the chat box and on the board. Tell the user it is waiting there for them. Never answer it yourself, and don't ask them about it again once the card says it was answered.";

#[cfg(test)]
mod tests {
    use super::*;

    fn session_with_prompt(prompt: &Value) -> Value {
        json!({
            "projectId": "P1",
            "sessionId": "G2",
            "runtimeSettings": {
                "agentActivity": { "sessionChatPrompt": prompt.to_string() }
            }
        })
    }

    fn row(session: &str, relay: Option<Value>) -> Value {
        let mut row = json!({
            "projectId": "P1",
            "sessionId": session,
            "title": "Outdated deps audit",
            "globalRef": format!("S1:P1:{session}"),
        });
        if let Some(relay) = relay {
            row["relay"] = relay;
        }
        row
    }

    #[test]
    fn a_connector_card_and_a_hook_approval_are_relayed_and_a_question_is_not() {
        let connector = json!({
            "kind": "question",
            "questions": [{"question": "Allow this agent to run X through Composio?", "multiSelect": false, "options": [{"label": "Approve"}, {"label": "Always allow"}, {"label": "Deny"}]}],
            "toolUseId": "connectorcard:abc",
        });
        let card = relay_card(&session_with_prompt(&connector)).expect("a connector card relays");
        assert_eq!(card["kind"], "connector");
        assert_eq!(card["prompt"]["toolUseId"], "connectorcard:abc");
        assert_eq!(card["labels"][1], "Always allow");

        let approval = json!({"kind": "approval", "tool": "Bash", "summary": "rm -rf tmp"});
        let card = relay_card(&session_with_prompt(&approval)).expect("an approval relays");
        assert_eq!(card["kind"], "permission");
        assert_eq!(card["prompt"]["summary"], "rm -rf tmp");

        let question = json!({
            "kind": "question",
            "questions": [{"question": "Which layout?", "multiSelect": false, "options": [{"label": "A"}]}],
            "toolUseId": "toolu_1",
        });
        assert!(relay_card(&session_with_prompt(&question)).is_none());
    }

    #[test]
    fn a_card_answered_here_ends_as_answered_here_and_one_that_vanished_as_answered_in_thread() {
        let relay = json!({"key": "k1", "kind": "permission", "prompt": {"kind": "approval", "tool": "Bash"}});
        let mut rows = vec![
            row("G-track-1", Some(relay.clone())),
            row(
                "G-track-2",
                Some(json!({"key": "k2", "kind": "permission"})),
            ),
        ];
        let cards = track_approvals(&mut rows);
        assert!(
            rows[0].get("relay").is_none(),
            "the row hands its card over"
        );
        let open: Vec<_> = cards
            .as_array()
            .unwrap()
            .iter()
            .filter(|card| card["status"] == "open")
            .collect();
        assert_eq!(open.len(), 2);
        assert_eq!(open[0]["title"], "Outdated deps audit");

        relays()
            .lock()
            .unwrap()
            .get_mut(&("P1".to_string(), "G-track-1".to_string()))
            .unwrap()
            .answered_here
            .insert(
                "k1".to_string(),
                AnsweredHere {
                    label: "Yes".to_string(),
                    coordinator_title: "Main Coordinator".to_string(),
                    at_ms: now_ms(),
                },
            );
        let mut rows = vec![row("G-track-1", None), row("G-track-2", None)];
        let cards = track_approvals(&mut rows);
        let by_thread = |session: &str| {
            cards
                .as_array()
                .unwrap()
                .iter()
                .find(|card| card["sessionId"] == session)
                .cloned()
                .unwrap()
        };
        let here = by_thread("G-track-1");
        assert_eq!(here["status"], "answeredHere");
        assert_eq!(here["answer"], "Yes");
        assert_eq!(here["answeredFrom"], "Main Coordinator");
        assert_eq!(by_thread("G-track-2")["status"], "answeredInThread");
    }

    #[test]
    fn an_agents_answer_to_a_relayed_card_is_refused_and_a_persons_is_not() {
        let mut rows = vec![row(
            "G-track-4",
            Some(json!({"key": "k4", "kind": "permission"})),
        )];
        track_approvals(&mut rows);
        let thread = ("P1".to_string(), "G-track-4".to_string());
        let agent =
            json!({"kind": "approval", "approvalSend": "1", "agentCaller": "G-coordinator"});
        assert!(refuse_agent_answer(&thread, agent.as_object().unwrap()).is_some());
        let person = json!({"kind": "approval", "approvalSend": "1"});
        assert!(refuse_agent_answer(&thread, person.as_object().unwrap()).is_none());
        let elsewhere = ("P1".to_string(), "G-test-drive".to_string());
        assert!(refuse_agent_answer(&elsewhere, agent.as_object().unwrap()).is_none());
    }

    #[test]
    fn a_card_replaced_by_another_moved_on() {
        let mut rows = vec![row("G-track-3", Some(json!({"key": "a", "kind": "trust"})))];
        track_approvals(&mut rows);
        let mut rows = vec![row("G-track-3", Some(json!({"key": "b", "kind": "trust"})))];
        let cards = track_approvals(&mut rows);
        let cards = cards.as_array().unwrap();
        assert_eq!(cards[0]["status"], "open");
        assert_eq!(cards[0]["key"], "b");
        assert!(cards
            .iter()
            .any(|card| card["key"] == "a" && card["status"] == "movedOn"));
    }
}
