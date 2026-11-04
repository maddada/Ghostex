//! Family f's minimap rail, one of its two frame channels; the other, the subagent viewer's own
//! transcript, is [`crate::extras::subagent_rows`].
//!
//! Both ride beside the main transcript so opening the subagent viewer never redraws the main
//! list. Both are also the two channels the desktop host can drop today
//! (`docs/2026-09-21/rust-chat/SEAM.md` section 5, item 1): the Rust host must include them in its
//! change gate.
//!
//! CDXC:SessionChat 2026-09-18 WHY:
//! React read the minimap's previews straight off the turns it had already rendered, but the
//! native rail gets them in the document, and a working session publishes a frame a second. The
//! whole row list keeps its identity while nothing changed, so an unchanged rail ships no bytes.

use serde_json::Value;

use crate::document::MinimapMarker;
use crate::extras::minimap_rail::{
    geometry, message_preview, minimap_preview, minimap_visible, MinimapMarkerRow,
};
use crate::state::{ChatContext, ChatState};
use crate::transcript::jsstr::js_trim;
use crate::transcript::page_answers::split_page_answers;
use crate::transcript::prose::prose_markdown;
use crate::transcript::raw_html::escape_raw_html;
use crate::transcript::turns::is_agent_conversation;
use ghostex_gx_protocol::chat::ChatMessage;

/// The minimap rail, shipped whole and only when it changed.
pub fn markers(state: &ChatState, _context: &ChatContext) -> Vec<MinimapMarker> {
    state
        .extras
        .minimap
        .iter()
        .map(|marker| serde_json::to_value(marker).unwrap_or(Value::Null))
        .collect()
}

/// `NativeChatMinimap.project` over the typed turns family b projects.
///
/// The same rule as [`project_minimap`], reading `ChatMessage` rather than raw JSON so the
/// transcript pass does not have to serialize every turn to ask for its preview. This is the one
/// the core calls; the JSON form has had no caller since the parity fixtures were deleted.
pub fn project_minimap_turns(
    turns: &[(
        &ghostex_gx_protocol::chat::ChatMessage,
        Option<&ghostex_gx_protocol::chat::ChatMessage>,
    )],
    item_index: &[(String, usize)],
) -> Vec<MinimapMarkerRow> {
    let rows = minimap_rows(turns, item_index);
    if !minimap_shows(rows.len()) {
        return Vec::new();
    }
    rows
}

/// Whether a rail of `turn_count` dashes draws at all.
pub fn minimap_shows(turn_count: usize) -> bool {
    minimap_visible(turn_count, geometry().minimum_turns)
}

/// Whether a turn's opening message gets a dash: the user typed it themselves.
///
/// CDXC:SessionChat 2026-11-02 DECISION:
/// User: "please don't show the question answers and the incoming messages as lines in the scroll points on the right side of chat". The rail lists only messages the user wrote: a message that is nothing but a page or ```questions form's answers envelope, and a message from another agent or a Ghostex thread report pasted in as the user's turn ([`is_agent_conversation`]), get no dash. Answers sent with the user's own words still count, and their preview shows those words.
pub fn is_own_prompt(message: &ChatMessage) -> bool {
    if is_agent_conversation(message) {
        return false;
    }
    match split_page_answers(&prose_markdown(&message.blocks)) {
        Some((_, rest)) => !js_trim(&rest).is_empty(),
        None => true,
    }
}

/// The rows of [`project_minimap_turns`] for some of the turns, whether or not the rail shows.
/// Turns the user did not open themselves ([`is_own_prompt`]) get no row.
pub fn minimap_rows(
    turns: &[(
        &ghostex_gx_protocol::chat::ChatMessage,
        Option<&ghostex_gx_protocol::chat::ChatMessage>,
    )],
    item_index: &[(String, usize)],
) -> Vec<MinimapMarkerRow> {
    let geometry = geometry();
    turns
        .iter()
        .filter(|(user, _)| is_own_prompt(user))
        .map(|(user, reply)| {
            let id = user.id.clone();
            let (prompt, prompt_lines) =
                message_preview(Some(user), geometry.preview_lines, geometry.preview_limit);
            let reply_lines = reply_line_budget(geometry.preview_lines, prompt_lines);
            MinimapMarkerRow {
                item: item_index
                    .iter()
                    .find(|(known, _)| *known == id)
                    .map(|(_, index)| *index)
                    .unwrap_or(0),
                prompt: if prompt.is_empty() {
                    "User message".to_string()
                } else {
                    escape_raw_html(&prompt)
                },
                reply: escape_raw_html(
                    &message_preview(*reply, reply_lines, geometry.preview_limit).0,
                ),
                id,
            }
        })
        .collect()
}

/// `NativeChatMinimap.project`: one row per genuine user prompt, pointing at the transcript row
/// that renders it.
///
/// `turns` is `(user message, reply message or none)` in transcript order and `item_index` maps a
/// user message id to the row that draws it; both come from family b's projection.
pub fn project_minimap(
    turns: &[(Value, Option<Value>)],
    item_index: &[(String, usize)],
) -> Vec<MinimapMarkerRow> {
    let geometry = geometry();
    if !minimap_visible(turns.len(), geometry.minimum_turns) {
        return Vec::new();
    }
    turns
        .iter()
        .map(|(user, reply)| {
            let id = user
                .get("id")
                .and_then(Value::as_str)
                .unwrap_or_default()
                .to_string();
            let (prompt, prompt_lines) =
                minimap_preview(Some(user), geometry.preview_lines, geometry.preview_limit);
            let reply_lines = reply_line_budget(geometry.preview_lines, prompt_lines);
            MinimapMarkerRow {
                item: item_index
                    .iter()
                    .find(|(known, _)| *known == id)
                    .map(|(_, index)| *index)
                    .unwrap_or(0),
                prompt: if prompt.is_empty() {
                    "User message".to_string()
                } else {
                    escape_raw_html(&prompt)
                },
                reply: escape_raw_html(
                    &minimap_preview(reply.as_ref(), reply_lines, geometry.preview_limit).0,
                ),
                id,
            }
        })
        .collect()
}

/// The lines the reply may take on the hover card: whatever the prompt left, after the blank line
/// that separates the two. An empty prompt still shows its one "User message" line.
fn reply_line_budget(card_lines: usize, prompt_lines: usize) -> usize {
    card_lines.saturating_sub(prompt_lines.max(1) + 1)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn row(id: &str, role: &str, text: &str) -> ChatMessage {
        serde_json::from_value(serde_json::json!({
            "id": id,
            "role": role,
            "source": "transcript",
            "blocks": [{"type": "text", "text": text}],
        }))
        .unwrap()
    }

    const ANSWERS: &str = "<ghostex-page-answers page=\"0123456789abcdef0123456789abcdef\" title=\"Open questions\">\n{\"Q1\":\"Yes\"}\n</ghostex-page-answers>";

    fn ids(rows: &[MinimapMarkerRow]) -> Vec<&str> {
        rows.iter().map(|row| row.id.as_str()).collect()
    }

    #[test]
    fn answers_and_agent_messages_get_no_dash() {
        let typed = row("u1", "user", "Fix the sidebar");
        let answers = row("u2", "user", ANSWERS);
        let brief = row(
            "u3",
            "user",
            "Task: fix the sidebar\n\nMessage from another agent\nAgent: Claude\nSession: Thread\nSession ID: G1\nAgent ID: claude\nAgent Session ID: x\nReply to: S1:P1:G1",
        );
        let report = row(
            "u4",
            "user",
            "Ghostex thread report: finished its turn. Its final message:\n\nDone.",
        );
        let again = row("u5", "user", "Now the chat");
        let turns: Vec<(&ChatMessage, Option<&ChatMessage>)> =
            [&typed, &answers, &brief, &report, &again]
                .into_iter()
                .map(|user| (user, None))
                .collect();
        assert_eq!(ids(&project_minimap_turns(&turns, &[])), ["u1", "u5"]);
    }

    #[test]
    fn answers_with_the_users_words_keep_a_dash_that_previews_the_words() {
        let typed = row("u1", "user", "Fix the sidebar");
        let mixed = row(
            "u2",
            "user",
            &format!("{ANSWERS}\n\nAlso keep the colours."),
        );
        let turns = [(&typed, None), (&mixed, None)];
        let rows = project_minimap_turns(&turns, &[]);
        assert_eq!(ids(&rows), ["u1", "u2"]);
        assert_eq!(rows[1].prompt, "Also keep the colours.");
    }

    #[test]
    fn the_rail_counts_dashes_not_turns() {
        let typed = row("u1", "user", "Fix the sidebar");
        let answers = row("u2", "user", ANSWERS);
        let turns = [(&typed, None), (&answers, None)];
        assert!(project_minimap_turns(&turns, &[]).is_empty());
    }
}
