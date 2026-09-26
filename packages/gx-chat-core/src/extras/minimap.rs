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
    let geometry = geometry();
    if !minimap_visible(turns.len(), geometry.minimum_turns) {
        return Vec::new();
    }
    turns
        .iter()
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
                    prompt
                },
                reply: message_preview(*reply, reply_lines, geometry.preview_limit).0,
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
                    prompt
                },
                reply: minimap_preview(reply.as_ref(), reply_lines, geometry.preview_limit).0,
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
