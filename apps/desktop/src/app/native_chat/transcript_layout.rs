use serde::Deserialize;
use std::sync::LazyLock;

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub(super) struct TranscriptLayout {
    pub end_padding: f32,
    /// The space every transcript row ends with, which is also the space between two one-line
    /// cards that follow each other anywhere in the transcript (`SINGLE_LINE_CARD.gap_above`).
    pub row_gap: f32,
}

/// How far the lone scroll-to-bottom pill sits above the transcript's bottom edge (working_card.rs).
///
/// CDXC:SessionChat 2026-10-19 WHY: the list used to reserve this space under its last row too (React's message-scroller-content did), which with the working card in its own row left 40px between the last row and the card; the user asked to "remove the extra space we have here please", so the last row ends with the row gap alone (the DECISION in packages/gx-chat-core/src/extras/working_card.rs).
pub(super) static LAYOUT: LazyLock<TranscriptLayout> = LazyLock::new(|| {
    serde_json::from_str(include_str!(
        "../../../../../packages/gx-chat-core/visual/transcript-layout.json"
    ))
    .expect("shared transcript layout")
});
