//! How the document is put together: one call per port family, in a fixed order.
//!
//! Family a runs first and writes the session facts every other family reads; the other five then
//! write their own keys over that. No family writes a key another one owns, so each family's keys
//! can be changed and checked on their own.
//!
//! The order below is not a dependency chain: every family reads [`crate::state::ChatState`], not
//! the half-built document. It is fixed only so two runs of the same state produce the same bytes.

use crate::document::{Document, MinimapMarker, RowDetails, TranscriptItem};
use crate::state::{ChatContext, ChatState};
use crate::{composer, extras, menus, questions, session, transcript};

/// The whole document for this state.
pub fn assemble(state: &ChatState, context: &ChatContext) -> Document {
    let mut document = Document::default();
    session::document(state, context, &mut document);
    transcript::document(state, context, &mut document);
    questions::document(state, context, &mut document);
    composer::document(state, context, &mut document);
    menus::document(state, context, &mut document);
    extras::document(state, context, &mut document);
    document
}

/// The parts of a frame that do not ride inside the document.
///
/// They are produced whole here; turning them into splices against what the host last saw is
/// family a's, in [`crate::ChatCore`].
#[derive(Clone, Debug, Default, PartialEq)]
pub struct FrameParts {
    /// The main transcript list. Family b.
    pub items: Vec<TranscriptItem>,
    /// The open subagent's transcript. Family f.
    pub subagent_items: Vec<TranscriptItem>,
    /// The minimap rail. Family f.
    pub minimap: Vec<MinimapMarker>,
    /// Details for the rows the renderer draws open. Family b.
    pub row_details: RowDetails,
    /// One identity per entry of `items`, compared beside the value when the splice is cut.
    ///
    /// `take` compares items by object identity, and a completed turn's item is a new object
    /// whenever its `deferred` is a new object with the same value
    /// ([`crate::state::MessagesState::deferred_objects`]); every other field is reused by value.
    /// Zero for an item with no such reference.
    pub item_identities: Vec<u64>,
}

/// The object identity of one transcript item, beside its value.
///
/// `reuseItems` keeps an item only when every field is the same object. Two fields can be a new
/// object with the same value: a completed turn's `deferred` (the user message's own
/// `deferredWork`) and a PLACEHOLDER row, which is cached per message object until its backfill
/// batch runs. A fully projected row keeps its object across arrivals (the projection cache
/// compares by value), so it contributes nothing.
fn item_identity(state: &ChatState, item: &TranscriptItem) -> u64 {
    let placeholder = |message: &crate::document::ProjectedMessage| -> u64 {
        if message.get("pending").and_then(serde_json::Value::as_bool) != Some(true) {
            return 0;
        }
        message
            .get("id")
            .and_then(serde_json::Value::as_str)
            .and_then(|id| state.messages.message_objects.get(id))
            .copied()
            .unwrap_or_default()
    };
    // Order-sensitive, so two items whose parts moved in opposite directions still differ.
    let mix = |seed: u64, part: u64| seed.wrapping_mul(0x0100_0000_01b3).wrapping_add(part);
    match item {
        TranscriptItem::Message { message } => placeholder(message),
        TranscriptItem::Summary {
            user,
            final_message,
            work,
            ..
        } => work.iter().fold(
            mix(
                placeholder(user),
                final_message.as_ref().map_or(0, placeholder),
            ),
            |seed, row| mix(seed, placeholder(row)),
        ),
        TranscriptItem::CompletedWork {
            id,
            deferred,
            work,
            artifacts,
            final_message,
            ..
        } => {
            let deferred = deferred
                .as_ref()
                .and_then(|_| state.messages.deferred_objects.get(id))
                .copied()
                .unwrap_or_default();
            work.iter()
                .chain(artifacts.iter())
                .chain(final_message.iter())
                .fold(deferred, |seed, row| mix(seed, placeholder(row)))
        }
        TranscriptItem::Unknown(_) => 0,
    }
}

/// Everything a frame carries beside the document.
pub fn frame_parts(state: &ChatState, context: &ChatContext) -> FrameParts {
    let items = transcript::rows(state, context);
    let item_identities = items
        .iter()
        .map(|item| item_identity(state, item))
        .collect();
    FrameParts {
        items,
        item_identities,
        subagent_items: extras::subagent_rows(state, context),
        minimap: extras::markers(state, context),
        row_details: transcript::row_details(state, context),
    }
}
