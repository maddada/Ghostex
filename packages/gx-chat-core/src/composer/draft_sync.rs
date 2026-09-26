//! Drafts across devices: the draft another client saved, offered to this composer, and this
//! composer's own draft pushed to gxserver while the user types.
//!
//! CDXC:Drafts 2026-09-25 DECISION:
//! User: "if have start on my pc then on this android app i don't see the draft text or Can press a
//! button to see it". A composer draft saved on one device is offered on every other device that
//! shows the same chat, through the "Another saved draft is available" bar with Use and Dismiss,
//! and every composer pushes its draft to gxserver once typing pauses instead of only when the
//! field loses focus. The offer was the React composer's `shouldOfferSessionChatDraft` effect
//! (`packages/core-ui/chat/session-chat-composer.tsx`), which neither the TypeScript native host
//! nor this core had ported: gxserver delivered the draft on every read and frame, the core folded
//! it into `draft.synced`, and no rule ever turned it into an offer, so no native chat (desktop,
//! web or phone) could show a draft typed elsewhere.

use serde_json::{json, Value};

use crate::composer::queue::{is_newer_draft_stamp, should_offer_draft, DraftVersion, SyncedDraft};
use crate::document::IncomingDraft;
use crate::effect::Effect;
use crate::jsnum::js_number_of;
use crate::state::{ChatContext, ChatState};
use crate::wire::ChatRpcMethod;

/// The timer that pushes the draft once typing pauses.
pub const DRAFT_PUSH_TIMER: &str = "composer.draftPush";

/// How long typing must pause before the draft is pushed.
///
/// CDXC:Drafts 2026-09-25 WHY:
/// React pushed after every rendered edit. The phone reaches gxserver through one SSH exec per
/// call, so a short pause stands in for "every edit" without an exec per keystroke; a blur still
/// pushes at once.
pub const DRAFT_PUSH_IDLE_MS: f64 = 1_000.0;

/// What the cross-device draft rules remember between events.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct DraftSyncState {
    /// The `session.synced_draft_revision` the offer rule last ran for, so it runs once per new
    /// synced draft, which is when the React effect on `[syncedDraft]` re-ran.
    pub seen_revision: u64,
    /// `lastHandledDraftAtRef`: the newest synced stamp this composer offered and had answered, or
    /// ignored because there was nothing to offer. An older or equal stamp is never offered again.
    pub last_handled_at: Option<String>,
    /// The stamp of the synced draft `incoming_draft` is offering, or `None` when the bar shows a
    /// terminal handoff's conflict (or nothing), which this rule neither withdraws nor answers.
    pub offered_at: Option<String>,
    /// The last edit, waiting for the pause, with the draft version it was typed under.
    pub pending_push: Option<(String, Value)>,
}

/// `editDraft`: the edit's identity becomes the local version the offer compares against, and the
/// push waits for typing to pause.
pub fn edited(state: &mut ChatState, text: &str, version: Option<&Value>, context: &ChatContext) {
    let Some(version) = version.filter(|version| version.is_object()) else {
        return;
    };
    if let Ok(parsed) = serde_json::from_value::<DraftVersion>(version.clone()) {
        state.composer.version = Some(parsed);
    }
    if !state.composer.boot_read || !state.composer.transport.set_draft {
        return;
    }
    state.composer.draft_sync.pending_push = Some((text.to_string(), version.clone()));
    state
        .core
        .timers
        .arm(DRAFT_PUSH_TIMER, context.now_ms, DRAFT_PUSH_IDLE_MS);
}

/// Drops the push waiting for the pause: a blur save, a send or a handoff carries the text itself.
pub fn cancel_pending_push(state: &mut ChatState) {
    state.composer.draft_sync.pending_push = None;
    state.core.timers.cancel(DRAFT_PUSH_TIMER);
}

/// `pushDraft`: one `setSessionChatDraft`, whose answer `settle_draft_push` folds back onto the
/// synced draft. Any push supersedes the one waiting for the pause.
pub fn push(state: &mut ChatState, content: &str, version: Value) -> Effect {
    cancel_pending_push(state);
    let request_id = state.core.allocate_request_id();
    state.composer.draft_pushes.push(request_id);
    Effect::SendRpc {
        request_id,
        method: ChatRpcMethod::SetSessionChatDraft,
        params: Box::new(json!({
            "clientId": state.identity.client_id,
            "content": content,
            "draftVersion": version,
        })),
    }
}

/// Use or Dismiss on the bar: the offered stamp is handled, so only a newer save offers again.
pub fn answered(state: &mut ChatState) {
    if let Some(at) = state.composer.draft_sync.offered_at.take() {
        state.composer.draft_sync.last_handled_at = Some(at);
    }
}

/// Family d's settle for the two rules: the push after a pause, and the offer of a new synced
/// draft.
pub fn settle(state: &mut ChatState) -> Vec<Effect> {
    let effects = push_after_pause(state);
    offer_synced_draft(state);
    effects
}

/// The pause timer fired: push the last edit unless something newer already carried it.
fn push_after_pause(state: &mut ChatState) -> Vec<Effect> {
    if !state.core.timer_fired(DRAFT_PUSH_TIMER) {
        return Vec::new();
    }
    let Some((text, version)) = state.composer.draft_sync.pending_push.take() else {
        return Vec::new();
    };
    // A send in flight pushes its own final revision before it delivers.
    if state.composer.submitting.is_some() || already_synced(state, &version) {
        return Vec::new();
    }
    vec![push(state, &text, version)]
}

/// gxserver already holds this identity at this revision or a later one.
fn already_synced(state: &ChatState, version: &Value) -> bool {
    let Some(synced) = state
        .session
        .synced_draft
        .as_ref()
        .and_then(|draft| draft.get("version"))
    else {
        return false;
    };
    let same_identity = synced.get("draftId").and_then(Value::as_str).is_some()
        && synced.get("draftId") == version.get("draftId");
    same_identity
        && js_number_of(synced.get("revision")).unwrap_or(0.0)
            >= js_number_of(version.get("revision")).unwrap_or(f64::INFINITY)
}

/// The React composer's effect on `[draftClientId, syncedDraft]`, from the boot read on: another
/// client's newer, non-empty draft that differs from what is typed here is offered, never written
/// over the field; anything else withdraws this rule's offer and counts as handled.
fn offer_synced_draft(state: &mut ChatState) {
    // Before the boot read the client id and the local draft identity are unknown, so every draft
    // would look foreign. The revision is left unseen, so the rule runs once the read lands.
    if !state.composer.boot_read {
        return;
    }
    let revision = state.session.synced_draft_revision;
    if state.composer.draft_sync.seen_revision == revision {
        return;
    }
    state.composer.draft_sync.seen_revision = revision;
    let Some(incoming) = state
        .session
        .synced_draft
        .as_ref()
        .and_then(|draft| serde_json::from_value::<SyncedDraft>(draft.clone()).ok())
    else {
        return;
    };
    // `if (pendingDraftTransfersRef.current > 0) return;`: a send or handoff is moving this
    // composer's text right now, so the field is not what the comparison below should read.
    if state.composer.submitting.is_some() {
        return;
    }
    let offer = should_offer_draft(
        Some(&incoming),
        &state.identity.client_id,
        state.composer.draft_sync.last_handled_at.as_deref(),
        &state.composer.text,
        state.composer.version.as_ref(),
    );
    if offer {
        state.composer.incoming_draft = Some(IncomingDraft {
            content: incoming.content,
            version: match incoming.version.map(serde_json::to_value) {
                Some(Ok(version)) => ghostex_gx_protocol::Tri::Value(version),
                _ => ghostex_gx_protocol::Tri::Absent,
            },
            extra: Default::default(),
        });
        state.composer.draft_sync.offered_at = Some(incoming.updated_at);
        return;
    }
    if state.composer.draft_sync.offered_at.take().is_some() {
        state.composer.incoming_draft = None;
    }
    if is_newer_draft_stamp(
        &incoming.updated_at,
        state.composer.draft_sync.last_handled_at.as_deref(),
    ) {
        state.composer.draft_sync.last_handled_at = Some(incoming.updated_at);
    }
}
