//! Messages between agents and the reader's answers fold into their finished turn's "Worked for"
//! row: the closed row counts them as pills ("2 messages", "1 answer") beside its tool calls and
//! files, and opening it shows each card as it is drawn elsewhere, above the work.
//!
//! What folds, for a finished turn (`completed-work`):
//! - the message that opened it when that is another agent's message (`interAgentMessage`, or the
//!   older `agentMessage` header) or answers alone (`pageAnswers` with no words of the reader's
//!   besides): moved from the row before the turn to the head of its `artifacts`;
//! - the same kinds of row among its `artifacts` (one that landed while the agent worked);
//! - its `sentMessages` ("Message to …") and its answered `questions`, and the reply's own, which
//!   move onto the turn so the reply no longer draws them.
//!
//! A folded row keeps its place in `artifacts` and carries `foldedIntoWork: true`; the turn carries
//! `cardsInFold: true`, which tells the renderers to draw the folded rows, then `questions`, then
//! `sentMessages`, at the top of the opened row instead of after it. Every walker over a turn's rows
//! (names, pictures, links, Find, the scroll anchor) reads them where it always did.
//!
//! CDXC:SessionChat 2026-11-10 DECISION: User, of a coordinator chat whose turn stood between "Message to …" and "You answered …" cards: "please add the answers/messages to and from under the Worked for X thingy at the top of it when i expand it basically. this is because we don't need to see them. we need a pill for each '2 messages' '1 answer'". Messages to and from other sessions and answer cards fold into their finished turn's "Worked for" row, counted as pills ("N messages" counts both directions, "N answers"), and show at the top of the opened row, before the work. It applies where the work is grouped: Tool call cards on under Agent work Default, or Agent work Reduced (whose line says the counts after its tool calls). With Tool call cards off, and under Agent work Quiet (which draws no row) or Summary, the cards stand on their own as before. Supersedes, for these cards only, the 2026-11-08 rule that sent-message cards and questions never fold into the work (`chat_look.rs`). A turn with no finished reply has no row, so its cards stay in view.
//!
//! CDXC:SessionChat 2026-11-10 SEE-ALSO: apps/desktop/src/app/native_chat/completed_work_row.rs and quiet_work.rs (desktop and web), apps/mobile/app/src/chat/native/transcript/TranscriptItemRow.tsx and quietWorkRows.tsx (phone) draw `cardsInFold`; `extras/transcript_search.rs` searches the folded rows only while the row is open; `grouped_work.rs` adds the counts up.

use serde_json::Value;

use crate::document::{ProjectedMessage, TranscriptItem};

/// The per-row mark of a folded row.
pub const FOLDED_INTO_WORK: &str = "foldedIntoWork";

/// What one folded row counts as.
enum Folded {
    Message,
    Answer,
}

/// What `row` folds as, if it folds: another agent's message, or answers with nothing else.
fn folds_as(row: &Value) -> Option<Folded> {
    if row["role"] != "user" {
        return None;
    }
    if row["interAgentMessage"].is_object() || row["agentMessage"].is_object() {
        return Some(Folded::Message);
    }
    let answers = &row["pageAnswers"];
    (answers.is_object() && answers["withText"] != true).then_some(Folded::Answer)
}

/// The sends a `sentMessages` list draws a card for.
fn drawn_sends(cards: &[Value]) -> usize {
    cards.iter().filter(|card| card["card"].is_object()).count()
}

/// `items` with each finished turn's messages and answers folded into its row (see the module).
pub fn fold_cards(items: Vec<TranscriptItem>) -> Vec<TranscriptItem> {
    let mut out: Vec<TranscriptItem> = Vec::with_capacity(items.len());
    for mut item in items {
        if let TranscriptItem::CompletedWork { id, .. } = &item {
            // The row that opened the turn comes right before it.
            let opener = match out.last() {
                Some(TranscriptItem::Message { message })
                    if message["id"].as_str() == Some(id.as_str())
                        && folds_as(message).is_some() =>
                {
                    match out.pop() {
                        Some(TranscriptItem::Message { message }) => Some(message),
                        _ => None,
                    }
                }
                _ => None,
            };
            fold_turn(&mut item, opener);
        }
        out.push(item);
    }
    out
}

/// One finished turn: takes in its opener, marks its folded rows, takes its reply's cards and
/// counts them all on its pills.
fn fold_turn(item: &mut TranscriptItem, opener: Option<ProjectedMessage>) {
    let TranscriptItem::CompletedWork {
        stats,
        expandable,
        questions,
        sent_messages,
        artifacts,
        final_message,
        tally,
        cards_in_fold,
        ..
    } = item
    else {
        return;
    };
    if let Some(opener) = opener {
        artifacts.insert(0, opener);
    }
    let mut messages = 0;
    let mut answers = 0;
    for row in artifacts.iter_mut() {
        let Some(kind) = folds_as(row) else {
            continue;
        };
        match kind {
            Folded::Message => messages += 1,
            Folded::Answer => answers += 1,
        }
        if let Value::Object(fields) = std::sync::Arc::make_mut(row) {
            fields.insert(FOLDED_INTO_WORK.into(), Value::Bool(true));
        }
    }
    // The reply's own cards join the turn's, so the reply draws only its words.
    if let Some(reply) = final_message.as_mut() {
        let has = |key: &str| reply[key].as_array().is_some_and(|list| !list.is_empty());
        if has("sentMessages") || has("questions") {
            if let Value::Object(fields) = std::sync::Arc::make_mut(reply) {
                for (key, list) in [
                    ("questions", &mut *questions),
                    ("sentMessages", &mut *sent_messages),
                ] {
                    if let Some(Value::Array(cards)) = fields.get_mut(key) {
                        list.append(cards);
                    }
                }
            }
        }
    }
    messages += drawn_sends(sent_messages);
    answers += questions.len();
    if messages + answers == 0 {
        return;
    }
    *cards_in_fold = true;
    *expandable = true;
    tally.messages = messages;
    tally.answers = answers;
    stats.extend(card_stats(messages, answers));
}

/// The pills for a row's folded cards: "2 messages", "1 answer", each only when there is one.
pub fn card_stats(messages: usize, answers: usize) -> Vec<Value> {
    let mut stats = Vec::new();
    if messages > 0 {
        stats.push(serde_json::json!({
            "kind": "messages",
            "label": format!("{messages} {}", if messages == 1 { "message" } else { "messages" }),
        }));
    }
    if answers > 0 {
        stats.push(serde_json::json!({
            "kind": "answers",
            "label": format!("{answers} {}", if answers == 1 { "answer" } else { "answers" }),
        }));
    }
    stats
}
