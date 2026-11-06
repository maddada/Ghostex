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

/// Under a coordinator's next message after a Ghostex update changed its shipped playbook.
pub const PLAYBOOK_CHANGED_BY_UPDATE_NOTE: &str = "Ghostex was updated and your playbook changed: run `ghostex coordinator guide` now and follow it from here on.";

/// Under a coordinator's next message after its project's (or all projects') playbook changed.
pub const PLAYBOOK_CHANGED_NOTE: &str =
    "Your playbook changed: run `ghostex coordinator guide` now and follow it from here on.";

/// Every note gxserver may add under a message the user sent from the chat.
pub const APPENDED_NOTES: [&str; 2] = [PLAYBOOK_CHANGED_BY_UPDATE_NOTE, PLAYBOOK_CHANGED_NOTE];

/// `text` with `note` under it, a blank line apart: the shape gxserver types.
pub fn with_note(text: &str, note: &str) -> String {
    format!("{}\n\n{note}", text.trim_end())
}

/// `text` without the notes gxserver added under it: what the user sent.
///
/// A note counts only as the last paragraph of a longer text, so a message that is nothing but a
/// note's wording (the user pasting it) stays as it is.
pub fn without_appended_notes(text: &str) -> &str {
    let mut rest = text.trim_end();
    loop {
        let stripped = APPENDED_NOTES.iter().find_map(|note| {
            let head = rest.strip_suffix(note)?;
            let body = head.trim_end();
            // The note must start its own line, after some text.
            (head.len() > body.len() && head[body.len()..].contains('\n') && !body.is_empty())
                .then_some(body)
        });
        match stripped {
            Some(body) => rest = body,
            None => return rest,
        }
    }
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
