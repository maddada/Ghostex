//! The `modelHoldCard` document key: one card above the composer while a model change the agent
//! refused for good holds messages behind it.

use std::sync::Arc;

use ghostex_gx_protocol::Tri;
use serde_json::{json, Value};

use crate::menus::picker::projection::ModelMenuContext;
use crate::state::ChatState;

/// What releases the held messages, in the card's footer.
pub const MODEL_HOLD_WAY_OUT: &str = "Pick a model again; held messages send once it applies.";

/// The sentence a failed model change is explained with.
///
/// CDXC:SessionChat 2026-10-30 SEE-ALSO: gxserver writes the same sentence as the sidebar's red-dot reason (`note_model_selection_failure` in server/src/session_chat_model_selection_alert.rs calls this), so the card and the tooltip read alike.
pub fn model_selection_failure_reason(model: &str, effort: &str, message: &str) -> String {
    match (model.is_empty(), effort.is_empty()) {
        (true, true) => format!("The chat option change failed: {message}"),
        (false, true) => format!("The change to {model} failed: {message}"),
        (true, false) => format!("The change to {effort} failed: {message}"),
        (false, false) => format!("The change to {model} {effort} failed: {message}"),
    }
}

/// What the card is built from, read in place: the failed pick's model, effort and reason, and how
/// many queued messages wait behind it.
struct HoldInputs<'a> {
    model: &'a str,
    effort: &'a str,
    message: &'a str,
    held: usize,
}

/// The inputs while a failed pick holds at least one queued message, else `None`.
fn hold_inputs(state: &ChatState) -> Option<HoldInputs<'_>> {
    let Tri::Value(pending) = &state.session.pending_model_selection else {
        return None;
    };
    let text = |key: &str| pending.get(key).and_then(Value::as_str).unwrap_or_default();
    if text("state") != "failed" {
        return None;
    }
    let held = state
        .session
        .queue_prompts
        .as_deref()
        .unwrap_or_default()
        .iter()
        .filter(|prompt| prompt.get("state").and_then(Value::as_str) == Some("queued"))
        .count();
    (held > 0).then(|| HoldInputs {
        model: text("model"),
        effort: text("effort"),
        message: text("errorMessage"),
        held,
    })
}

/// The last card built and what it was built from.
///
/// CDXC:SessionChat 2026-10-30 WHY: Every event of a retained chat assembles the document once to test whether anything changed (an unchanged timer tick stops there), and again when it publishes, and building the card's sentence and object on each assembly cost about 25 allocations while nothing changed. The picker's settle keeps the card here and the document shares it while the inputs are the same, as it already does for the provider catalogs (`menus/picker/inputs.rs`); a state the settle has not seen yet still gets a freshly built card.
#[derive(Clone, Debug, PartialEq)]
pub struct ModelHoldMemo {
    model: String,
    effort: String,
    message: String,
    held: usize,
    card: Arc<Value>,
}

impl ModelHoldMemo {
    fn matches(&self, inputs: &HoldInputs<'_>) -> bool {
        self.held == inputs.held
            && self.model == inputs.model
            && self.effort == inputs.effort
            && self.message == inputs.message
    }

    fn build(inputs: &HoldInputs<'_>) -> Self {
        Self {
            model: inputs.model.to_string(),
            effort: inputs.effort.to_string(),
            message: inputs.message.to_string(),
            held: inputs.held,
            card: Arc::new(card(inputs)),
        }
    }
}

fn card(inputs: &HoldInputs<'_>) -> Value {
    let detail = if inputs.message.trim().is_empty() {
        String::new()
    } else {
        model_selection_failure_reason(inputs.model, inputs.effort, inputs.message)
    };
    let title = if inputs.held == 1 {
        "1 message is held".to_string()
    } else {
        format!("{} messages are held", inputs.held)
    };
    json!({
        "tone": "warning",
        "title": title,
        "detail": detail,
        "note": MODEL_HOLD_WAY_OUT,
        "action": { "id": "pickModel", "label": "Pick model" },
        "heldCount": inputs.held,
    })
}

/// Keeps `PickersState::model_hold` current, rebuilding the card only when what it is built from
/// changed. Run by the picker's settle on every event.
pub fn remember_model_hold(state: &mut ChatState) {
    let next = match (hold_inputs(state), &state.pickers.model_hold) {
        (Some(inputs), Some(memo)) if memo.matches(&inputs) => return,
        (None, None) => return,
        (Some(inputs), _) => Some(ModelHoldMemo::build(&inputs)),
        (None, Some(_)) => None,
    };
    state.pickers.model_hold = next;
}

/// The card, or `None` while nothing is held behind a failed change.
///
/// CDXC:SessionChat 2026-10-30 WHY: A model change picked in the chat can fail for good (the agent refused it, an OMP compaction was cancelled or failed, or three tries went unconfirmed), and the messages behind it then waited with only a spinner while the reason sat in the sidebar's tooltip and the model menu. One card counts the held messages (the queued rows of Ghostex's queue, which its scheduler delivers only after a pick applies; a message's own Send now still sends it on the current model), gives the reason and opens the model menu with Pick model, so it is drawn only where that menu exists: until the agent's model list is available, the sidebar's mark alone explains the hold. A change still retrying draws nothing, as in the model menu (the 2026-09-18 decision in menus/option_menus.rs). It has no Dismiss: hiding it would be a way to clear the hold the 2026-09-27 decision in server/src/session_chat_model_selection_alert.rs does not have, so it leaves only when a pick applies or the messages leave.
/// SEE-ALSO: apps/desktop/src/app/native_chat/model_hold_card.rs and apps/mobile/app/src/chat/native/cards/ModelHoldCard.tsx draw it.
pub fn model_hold_card(state: &ChatState, menu: Option<&ModelMenuContext>) -> Option<Arc<Value>> {
    menu?;
    let inputs = hold_inputs(state)?;
    Some(match &state.pickers.model_hold {
        Some(memo) if memo.matches(&inputs) => Arc::clone(&memo.card),
        _ => Arc::new(card(&inputs)),
    })
}
