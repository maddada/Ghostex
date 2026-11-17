//! The notes gxserver adds under a chat message on its way to the agent, so the chat can tell the
//! message the user sent from the turn the agent recorded.
//!
//! CDXC:SessionChat 2026-11-06 WHY:
//! The user, on a coordinator mid-turn: "i just sent this message and i see it appear twice in ur
//! chat transcript in the gpui app pls fix". gxserver had added the playbook note ("Ghostex was
//! updated and your playbook changed: run `ghostex coordinator guide` ...") under the message
//! (server/src/session_chat_queue_runtime/send.rs, CDXC:Coordinators 2026-11-05), so Claude
//! recorded the message plus the note, while the optimistic echo held only what the user typed.
//! The echo matcher compares whole texts, so the echo never matched its turn and stayed under it as
//! a second copy, with the queued echo's buttons. Mid-turn or at a turn's end makes no difference:
//! both record the noted text. gxserver writes every such note through [`with_note`], and the
//! matcher reads every recorded user turn through [`without_appended_notes`], so the two cannot
//! drift: a new note goes into [`APPENDED_NOTES`] and is matched on every client (desktop, web
//! and the phone's WebAssembly core) with no other change.

use ghostex_gx_protocol::{ChatBlock, ChatMessage, ChatRole};

/// Under a coordinator's next message after its shipped guide changed (a Ghostex update, or the
/// Marked handovers setting, which picks the guide's reply format). It names no change: the guide
/// says what holds now. Every client, the phone apps already out included, recognises this text.
pub const PLAYBOOK_CHANGED_BY_UPDATE_NOTE: &str = "Ghostex was updated and your playbook changed: run `ghostex coordinator guide` now and follow it from here on.";

/// The update note of 2026-11-10 and 11.3.0, which said work items replace a coordinator's status
/// sections (no longer true while Marked handovers is on), still recognised under older messages.
const PLAYBOOK_CHANGED_BY_UPDATE_NOTE_NAMING_WORK_ITEMS: &str = "Ghostex was updated and your playbook changed: work items and their cards replace the board and your replies' status sections. Run `ghostex coordinator guide` now and follow it from here on.";

/// Under a coordinator's next message after its project's (or all projects') playbook changed.
pub const PLAYBOOK_CHANGED_NOTE: &str =
    "Your playbook changed: run `ghostex coordinator guide` now and follow it from here on.";

/// Under a coordinator's next message after its context was compacted, for the agents whose hooks
/// cannot hand it its role again (coordinators/role.rs `coordinator_role_again` covers Claude and
/// Codex).
pub const CONTEXT_COMPACTED_NOTE: &str =
    "Your context was compacted: run `ghostex coordinator guide` now and follow it from here on.";

/// Every fixed note gxserver may add under a message the user sent from the chat (the drift
/// reminder, built from its slips, is recognised by [`is_drift_reminder`]).
pub const APPENDED_NOTES: [&str; 4] = [
    PLAYBOOK_CHANGED_BY_UPDATE_NOTE,
    PLAYBOOK_CHANGED_NOTE,
    PLAYBOOK_CHANGED_BY_UPDATE_NOTE_NAMING_WORK_ITEMS,
    CONTEXT_COMPACTED_NOTE,
];

/// The slips a coordinator's reply can drift into, in the order its reminder names them
/// (server/src/coordinators/drift.rs).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DriftSlip {
    /// It asked the user a question in its reply's text.
    AskedInText,
    /// It did the work itself (edited the project's files, ran builds or tests) and started no
    /// thread.
    WorkedItself,
    /// Its reply has no marked sections while Marked handovers asks coordinators for them.
    Unmarked,
}

impl DriftSlip {
    pub const ALL: [DriftSlip; 3] = [Self::AskedInText, Self::WorkedItself, Self::Unmarked];

    fn words(self) -> &'static str {
        match self {
            Self::AskedInText => "ask that as a form or an item ask, not in plain text",
            Self::WorkedItself => "hand real work to a thread with start-thread",
            Self::Unmarked => "end with the marked sections",
        }
    }
}

const DRIFT_REMINDER_OPENING: &str = "Reminder: ";

/// The one-line reminder under a coordinator's next message naming its `slips` ("Reminder: ask
/// that as a form or an item ask, not in plain text; end with the marked sections."), `None` for
/// none.
pub fn drift_reminder(slips: &[DriftSlip]) -> Option<String> {
    let words: Vec<&str> = DriftSlip::ALL
        .into_iter()
        .filter(|slip| slips.contains(slip))
        .map(DriftSlip::words)
        .collect();
    (!words.is_empty()).then(|| format!("{DRIFT_REMINDER_OPENING}{}.", words.join("; ")))
}

/// Whether `paragraph` is a drift reminder gxserver wrote, for any set of slips.
fn is_drift_reminder(paragraph: &str) -> bool {
    let Some(body) = paragraph
        .strip_prefix(DRIFT_REMINDER_OPENING)
        .and_then(|rest| rest.strip_suffix('.'))
    else {
        return false;
    };
    let mut parts = body.split("; ").peekable();
    let mut order = DriftSlip::ALL.iter().map(|slip| slip.words());
    parts.peek().is_some() && parts.all(|part| order.by_ref().any(|words| words == part))
}

/// The opening and closing words of the note under a user's message to a coordinator that lists
/// the questions still waiting on the user (server/src/session_chat_queue_runtime/send.rs
/// `open_questions_note`); its middle names each by its title with the tags the agent knows it by.
///
/// CDXC:Coordinators 2026-11-16 DECISION:
/// User, on issue #627 (an answer typed in the normal chat box left the ask open, so he clicked a
/// wrong answer to clear it), picked "Tell the coordinator what's still open": when he sends a
/// message while questions wait on him, the coordinator is told which, by title, and closes any
/// the message answered. The note is for the agent only; the chat hides it like the other notes.
const OPEN_QUESTIONS_OPENING: &str = "Still open for the user: ";
const OPEN_QUESTIONS_CLOSING: &str = " If this message answers any of them, close it: `ghostex coordinator item ask <W7> --done <Q12>` for a work item's question, \"withdraw\" in a ```questions block for a form's or page's.";
/// The most questions the note names; the rest are counted ("and 3 more").
const OPEN_QUESTIONS_NAMED: usize = 10;

/// The note listing `open` (each a title and the tags the agent knows it by, "W7 Q12" or "Q12 to
/// Q14"), `None` for none. Titles are kept to one line of at most 80 characters.
pub fn open_questions_note(open: &[(String, String)]) -> Option<String> {
    if open.is_empty() {
        return None;
    }
    let mut named: Vec<String> = open
        .iter()
        .take(OPEN_QUESTIONS_NAMED)
        .map(|(title, tags)| {
            let title: String = title
                .split_whitespace()
                .collect::<Vec<_>>()
                .join(" ")
                .replace(['\u{201c}', '\u{201d}'], "\"");
            let title = if title.chars().count() > 80 {
                format!("{}\u{2026}", title.chars().take(79).collect::<String>())
            } else {
                title
            };
            match (title.is_empty(), tags.trim().is_empty()) {
                (true, _) => tags.trim().to_string(),
                (false, true) => format!("\u{201c}{title}\u{201d}"),
                (false, false) => format!("\u{201c}{title}\u{201d} ({})", tags.trim()),
            }
        })
        .collect();
    if open.len() > OPEN_QUESTIONS_NAMED {
        named.push(format!("and {} more", open.len() - OPEN_QUESTIONS_NAMED));
    }
    Some(format!(
        "{OPEN_QUESTIONS_OPENING}{}.{OPEN_QUESTIONS_CLOSING}",
        named.join("; ")
    ))
}

/// The opening of the note that tells an agent which of its questions the user dismissed (or, for
/// one with nothing to answer, marked read) since its last note, sent with the next message it
/// gets anyway instead of a message of its own (server/src/session_chat_dismissed.rs, where the
/// user's decision lives).
const DISMISSED_OPENING: &str = "Dismissed by the user since your last message, don't ask again: ";

/// The note naming `dismissed` (each as the agent knows it: "\"Release plan\" (Q12 to Q14)",
/// "W7 Q12 \"Which branch?\""), `None` for none.
pub fn dismissed_note(dismissed: &[String]) -> Option<String> {
    let named: Vec<&str> = dismissed
        .iter()
        .map(|name| name.trim())
        .filter(|name| !name.is_empty())
        .collect();
    (!named.is_empty()).then(|| format!("{DISMISSED_OPENING}{}.", named.join("; ")))
}

/// Whether `paragraph` is an open questions note gxserver wrote ([`open_questions_note`]).
fn is_open_questions_note(paragraph: &str) -> bool {
    !paragraph.contains('\n')
        && paragraph.starts_with(OPEN_QUESTIONS_OPENING)
        && paragraph.ends_with(OPEN_QUESTIONS_CLOSING)
}

/// `rest` without the note gxserver added last: the fixed notes, a drift reminder, or the open
/// questions note.
fn strip_last_note(rest: &str) -> Option<&str> {
    let drift = rest
        .rsplit_once('\n')
        .filter(|(_, last)| is_drift_reminder(last) || is_open_questions_note(last))
        .map(|(head, _)| &rest[..head.len() + 1]);
    let head = APPENDED_NOTES
        .iter()
        .find_map(|note| rest.strip_suffix(note))
        .or(drift)?;
    let body = head.trim_end();
    // The note must start its own line, after some text.
    (head.len() > body.len() && head[body.len()..].contains('\n') && !body.is_empty())
        .then_some(body)
}

/// `text` with `note` under it, a blank line apart: the shape gxserver typed before 11.8
/// (`with_system_notes` now). Kept so such rows in transcripts are still read without it.
pub fn with_note(text: &str, note: &str) -> String {
    format!("{}\n\n{note}", text.trim_end())
}

/// The block that carries Ghostex's own notes to the agent, after what the user wrote and apart
/// from it: the playbook note, the drift reminder, the compaction note and the open questions.
///
/// CDXC:Coordinators 2026-11-16 DECISION:
/// User, of "Still open for the user: …" drawn in his own bubble: "This is a system message. mark
/// it as such and don't show these ghostex system messages in the chat view at all like this!
/// also shouldn't be attached to a user's message should be sent on its own". Every note goes in
/// one `<ghostex-system>` block, marked as Ghostex's and never part of the user's words, in the
/// same submit (a second submit would land mid-turn as another user message on Claude Code and
/// Codex alike, and could be lost or reordered by their queues). Every chat drops the block from
/// the row it draws, wherever in the text it sits (`without_notes`).
pub const SYSTEM_OPEN: &str = "<ghostex-system>";
pub const SYSTEM_CLOSE: &str = "</ghostex-system>";

/// Ghostex's `notes` as a message of their own: one `<ghostex-system>` block, sent to the agent
/// right after the user's message as a separate submit; `None` for none.
///
/// CDXC:Coordinators 2026-11-16 DECISION:
/// User (Q3557): the notes may show in the terminal, never in the chat view, and go "as their own
/// separate message, not inside the user's message". The chat drops a user row that holds nothing
/// but such a block ([`is_system_only`]).
pub fn system_message(notes: &[&str]) -> Option<String> {
    if notes.iter().all(|note| note.trim().is_empty()) {
        return None;
    }
    let mut notes = notes.to_vec();
    notes.push(NOTE_ONLY_REPLY);
    let block = with_system_notes("", &notes);
    let block = block.trim();
    (!block.is_empty()).then(|| block.to_string())
}

/// The last line of a notes message: a note that reaches an idle agent opens a turn of its own,
/// which the chat does not draw, so its answer should cost as little as it can.
const NOTE_ONLY_REPLY: &str = "This message is from Ghostex, not the user, and needs no answer of its own: act on it if it asks something of you, and if there is nothing to do, reply with only \"ok\".";

/// Whether `message` is a user row that holds only Ghostex's notes: no words of the user's, no
/// picture. No chat draws it.
pub fn is_system_only(message: &ChatMessage) -> bool {
    message.role == ChatRole::User
        && !message.blocks.is_empty()
        && message.blocks.iter().all(|block| match block {
            ChatBlock::Text { text } => {
                text.contains(SYSTEM_OPEN) && without_notes(text).trim().is_empty()
            }
            _ => false,
        })
}

/// The transcript as the chat draws it: without Ghostex's notes sent as messages of their own, and
/// without the replies of a turn such a note opened.
///
/// A note typed while the agent works is taken into the running turn: Claude Code records it as a
/// queued command right after a tool's result (or, where tool rows are left out, right after the
/// user's message), and the reply after it is the reply to the user's message, so only the note
/// goes; when in doubt nothing else is hidden. A note that reaches an agent whose turn already ended (a reply
/// with no tool call ends before the note lands) opens a turn of its own, after a finished reply:
/// that turn is Ghostex's, so it goes whole, up to the next message the user wrote.
///
/// CDXC:Coordinators 2026-11-16 DECISION:
/// User picked "A. Separate message, and hide a reply only when the note opened its own turn"
/// (Q3561), with "make sure in case 1 (note joins the user's turn) the real reply is never
/// hidden". The note asks the agent to answer with only "ok" when it has nothing to do there.
pub fn without_note_turns(rows: Vec<crate::state::ChatRow>) -> Vec<crate::state::ChatRow> {
    without_note_turns_after(rows, &mut NoteTurns::default())
}

/// Where [`without_note_turns`] stood after the rows it last read, so rows pushed after them can
/// be read on their own (`session/composition.rs` `refresh_boundaried`).
#[derive(Clone, Debug, Default, PartialEq)]
pub struct NoteTurns {
    /// The role of the last row read that was not a system row.
    previous: Option<ChatRole>,
    /// Whether that row is in a turn a note opened.
    in_note_turn: bool,
}

/// [`without_note_turns`] for `rows` coming right after the ones `turns` read.
///
/// CDXC:Coordinators 2026-11-16 WHY:
/// The user still saw the open questions note as his own bubble, `<ghostex-system>` tags and all,
/// on a coordinator that was mid-turn. The note was a system-only row, but the live chat builds its
/// transcript in `refresh_boundaried`, whose whole rebuild and whose rows-pushed-at-the-end path
/// never dropped notes; only the rarely used rebuild in `boundaried_transcript` did. Both now
/// read every row through here, the pushed rows from where the last read stopped.
pub fn without_note_turns_after(
    rows: Vec<crate::state::ChatRow>,
    turns: &mut NoteTurns,
) -> Vec<crate::state::ChatRow> {
    if !turns.in_note_turn && !rows.iter().any(|row| is_system_only(row)) {
        if let Some(last) = rows.iter().rev().find(|row| row.role != ChatRole::System) {
            turns.previous = Some(last.role.clone());
        }
        return rows;
    }
    let mut kept = Vec::with_capacity(rows.len());
    for row in rows {
        if row.role == ChatRole::System {
            if !turns.in_note_turn {
                kept.push(row);
            }
            continue;
        }
        if is_system_only(&row) {
            // A note still waiting in the agent's queue says nothing about where it lands.
            // Only right after the agent's own reply did its turn end before the note: after the
            // user's message or a tool's result (a read that leaves tool rows out shows the note
            // right after the message), or with nothing loaded before it, the note joined a
            // running turn, and nothing is hidden.
            if !row.queued {
                turns.in_note_turn = matches!(
                    turns.previous,
                    Some(ChatRole::Assistant | ChatRole::Reasoning)
                );
            }
            turns.previous = Some(row.role.clone());
            continue;
        }
        if turns.in_note_turn && row.role == ChatRole::User {
            // The user's own words end the note's turn, wherever they land.
            turns.in_note_turn = false;
        }
        turns.previous = Some(row.role.clone());
        if !turns.in_note_turn {
            kept.push(row);
        }
    }
    kept
}

/// `text` with Ghostex's `notes` after it in one `<ghostex-system>` block; `text` alone for none.
pub fn with_system_notes(text: &str, notes: &[&str]) -> String {
    let notes: Vec<&str> = notes
        .iter()
        .map(|note| note.trim())
        .filter(|note| !note.is_empty())
        .collect();
    if notes.is_empty() {
        return text.to_string();
    }
    format!(
        "{}\n\n{SYSTEM_OPEN}\nGhostex system note, not written by the user:\n{}\n{SYSTEM_CLOSE}",
        text.trim_end(),
        notes.join("\n\n")
    )
}

/// `text` without its `<ghostex-system>` blocks, or `None` when it has none.
fn without_system_blocks(text: &str) -> Option<String> {
    if !text.contains(SYSTEM_OPEN) {
        return None;
    }
    let mut kept = String::with_capacity(text.len());
    let mut rest = text;
    while let Some(open) = rest.find(SYSTEM_OPEN) {
        let Some(close) = rest[open..].find(SYSTEM_CLOSE) else {
            break;
        };
        kept.push_str(rest[..open].trim_end());
        rest = &rest[open + close + SYSTEM_CLOSE.len()..];
    }
    kept.push_str(rest);
    Some(kept.trim_end().to_string())
}

/// `text` without anything gxserver added for the agent: its `<ghostex-system>` blocks, and the
/// bare notes of older versions under it (`without_appended_notes`). What the user sent.
pub fn without_notes(text: &str) -> String {
    match without_system_blocks(text) {
        Some(plain) => without_appended_notes(&plain).to_string(),
        None => without_appended_notes(text).to_string(),
    }
}

/// `text` without the notes gxserver added under it: what the user sent.
///
/// A note counts only as the last paragraph of a longer text, so a message that is nothing but a
/// note's wording (the user pasting it) stays as it is.
pub fn without_appended_notes(text: &str) -> &str {
    let mut rest = text.trim_end();
    loop {
        match strip_last_note(rest) {
            Some(body) => rest = body,
            None => return rest,
        }
    }
}

/// The opening and closing words of the reminder `ghostex agents send` adds under a coordinator's
/// message to one of its threads (server/src/coordinators/rename.rs `thread_title_reminder`),
/// whose middle names the thread's current title.
const THREAD_TITLE_REMINDER_START: &str = "Your title is \"";
const THREAD_TITLE_REMINDER_END: &str = "If it answers that the user renamed you, keep that name.";
/// The line that opens the sender block every message between agents carries.
const AGENT_MESSAGE_OPENER: &str = "\n\nMessage from another agent\n";

fn is_thread_title_reminder(paragraph: &str) -> bool {
    !paragraph.contains('\n')
        && paragraph.starts_with(THREAD_TITLE_REMINDER_START)
        && paragraph.ends_with(THREAD_TITLE_REMINDER_END)
        && paragraph.contains("rename-self")
}

/// `text` without the thread title reminder: the reminder is the body's last paragraph, so it
/// ends the text or sits right above the sender block, and some text comes before it.
fn without_thread_title_reminder(text: &str) -> Option<String> {
    let (body, block) = match text.rfind(AGENT_MESSAGE_OPENER) {
        Some(at) => text.split_at(at),
        None => (text, ""),
    };
    let body = body.trim_end();
    let (head, last) = body.rsplit_once("\n\n")?;
    (is_thread_title_reminder(last) && !head.trim().is_empty())
        .then(|| format!("{}{block}", head.trim_end()))
}

/// CDXC:SessionChat 2026-11-09 WHY:
/// The user, on a coordinator's chat that showed "Hi" plus "Ghostex was updated and your
/// playbook changed: run `ghostex coordinator guide` now ...": "please fam when you post the thing
/// about ghostex being updated and need to read coordinator playbook again don't mention this in
/// the ui of the chat view". The notes are for the agent, which still gets them (they are typed in
/// and recorded), but every chat draws, copies, edits and searches a user row through the core's
/// message list, so the core keeps each row there as the user (or the coordinator) wrote it: no
/// playbook note and no thread title reminder, with nothing drawn in their place.
///
/// `message` as the chat shows it, or `None` when it carries no added note.
pub fn shown_message(message: &ChatMessage) -> Option<ChatMessage> {
    // A message of nothing but Ghostex's notes stays whole: the transcript drops it, and the turn
    // it opened, where it can tell the two apart (`without_note_turns`).
    if message.role != ChatRole::User || is_system_only(message) {
        return None;
    }
    let mut changed = false;
    let blocks = message
        .blocks
        .iter()
        .map(|block| match block {
            ChatBlock::Text { text } => {
                let stripped = without_notes(text);
                let plain = if stripped.len() != text.trim_end().len() {
                    stripped
                } else {
                    text.clone()
                };
                match without_thread_title_reminder(&plain) {
                    Some(stripped) => {
                        changed = true;
                        ChatBlock::Text { text: stripped }
                    }
                    None => {
                        changed |= plain != *text;
                        ChatBlock::Text { text: plain }
                    }
                }
            }
            other => other.clone(),
        })
        .collect();
    changed.then(|| ChatMessage {
        blocks,
        ..message.clone()
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn strips_a_note_gxserver_added() {
        let sent = "please add a spinner";
        assert_eq!(
            without_appended_notes(&with_note(sent, PLAYBOOK_CHANGED_BY_UPDATE_NOTE)),
            sent
        );
        assert_eq!(
            without_appended_notes(&with_note(sent, PLAYBOOK_CHANGED_NOTE)),
            sent
        );
    }

    #[test]
    fn keeps_a_message_that_only_quotes_a_note() {
        assert_eq!(
            without_appended_notes(PLAYBOOK_CHANGED_NOTE),
            PLAYBOOK_CHANGED_NOTE
        );
        let inline = format!("what does {PLAYBOOK_CHANGED_NOTE}");
        assert_eq!(without_appended_notes(&inline), inline);
    }
}
