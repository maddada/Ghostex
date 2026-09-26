//! Family a's part of the document: the session facts, the transcript's readiness, and the
//! pagination cursor.
//!
//! It runs first in [`crate::document::assemble`], so it may write any key it owns; the other five
//! families then write theirs over a document that already has this one's.

use ghostex_gx_protocol::Tri;

use crate::document::Document;
use crate::session::view_state::select_view_state;
use crate::session::working::{is_working, publish_status, transcript_working};
use crate::state::{ChatContext, ChatState};

/// Writes family a's keys into `into`.
pub fn document(state: &ChatState, context: &ChatContext, into: &mut Document) {
    let session = &state.session;

    // A locally accepted send owns the working presentation immediately. It stays pending until
    // the authoritative transcript advances past that user turn, bridging the gap before host or
    // server activity arrives.
    let working = is_working(state);

    // `ChatCore::republish` composes the list into `state.messages.composed` before any document
    // is assembled, and only the row count is read here; composing it again per assembly (twice
    // per event, over the whole transcript) was most of a long chat's per-tick cost.
    let composed_rows = state.messages.composed.len();
    let status = publish_status(&session.server_status, working, session.error.is_some());

    into.view = select_view_state(&status, composed_rows, session.error.as_deref());
    into.status = status.as_str().to_string();
    into.working = working;
    into.transcript_working = transcript_working(state);
    into.session_working = session.session_activity_working || session.external_working;
    into.error = tri_string(session.error.clone());
    into.lifecycle = match &session.lifecycle {
        Some(lifecycle) => serde_json::to_value(lifecycle)
            .map(Tri::Value)
            .unwrap_or(Tri::Null),
        None => Tri::Null,
    };
    into.agent = tri_string(session.agent.clone());
    into.agent_session_id = tri_string(session.agent_session_id.clone());
    into.session_agent_id = tri_string(session.session_agent_id.clone());
    into.screen_probed = session.screen_probed;
    into.retired_async_question_ids = Tri::Value(session.retired_async_question_ids.clone());
    into.has_more = state.messages.has_more;
    into.earlier_page_cursor = state.messages.before_offset as i64;
    into.loading_earlier = state.messages.loading_earlier;
    // Omitted, not `null`: the TypeScript wrote `operationError` straight from a
    // `string | undefined`, so `JSON.stringify` dropped the key whenever there was no refusal. Its
    // `operationErrorCode` sibling was written as `?? null` and is therefore always present.
    into.operation_error = match state.core.operation_error.clone() {
        Some(message) => Tri::Value(message),
        None => Tri::Absent,
    };
    into.operation_error_code = state.core.operation_error_code.clone();
    let _ = context;
}

/// `None` is a key present with `null`, which is what the TypeScript producer writes for every one
/// of these.
fn tri_string(value: Option<String>) -> Tri<String> {
    match value {
        Some(value) => Tri::Value(value),
        None => Tri::Null,
    }
}
