//! Family e's part of the document: the option pills, the model menu and picker, accounts, the
//! context meter and editor, and the fork branches.

use ghostex_gx_protocol::Tri;
use serde_json::{json, Value};

use crate::document::{AccountStatus, Document};
use crate::menus::controls::{account_providers, account_switch_status};
use crate::menus::native_accounts::{native_account_panel, native_account_switch_card};
use crate::menus::options::{accounts_state, compute_native_chat_options};
use crate::state::{ChatContext, ChatState};

/// Writes family e's keys into `into`.
///
/// `selectedOptions` is folded by family a, which merges by evidence priority and `detectedAt`;
/// publishing it is family e's, because the pills decide what a weaker capture may overwrite.
pub fn document(state: &ChatState, context: &ChatContext, into: &mut Document) {
    let now_ms = context.now_millis();
    let options = compute_native_chat_options(state, context);

    into.selected_options = nullable(state.session.selected_options.clone());
    into.available_agents = nullable(state.session.available_agents.clone());
    into.switchable_agents = nullable(state.session.switchable_agents.clone());
    into.run_location = match crate::menus::run_location::row(state) {
        Some(row) => Tri::Value(row),
        None => Tri::Absent,
    };
    into.worktree_tray = match crate::menus::draft_worktree::tray(state) {
        Some(tray) => Tri::Value(tray),
        None => Tri::Absent,
    };
    into.option_labels = Tri::Value(to_value(&options.option_labels));
    into.option_menus = Tri::Value(to_value(&options.option_menus));
    into.session_options = Tri::Value(to_value(&options.session_options));
    into.option_dispatch_id = state.menus.option_dispatch_id.clone();
    into.model_provider = match options.model_provider {
        Some(provider) => Tri::Value(provider.to_string()),
        None => Tri::Absent,
    };

    let providers = account_providers(state);
    let accounts = accounts_state(state);
    let status = account_switch_status(state, now_ms);
    into.accounts = match &state.menus.accounts {
        Some(value) => Tri::Value(value.clone()),
        None => Tri::Absent,
    };
    into.account_error = match &state.menus.account_error {
        Some(error) => Tri::Value(error.clone()),
        None => Tri::Absent,
    };
    into.account_switch = nullable(state.session.account_switch.value().cloned());
    let mut panel_context = context.clone();
    if let Some(clock) = state.menus.panel_clock_ms {
        panel_context.now_ms = clock;
    }
    into.account_panel = Tri::Value(match providers.panel {
        None => Value::Null,
        Some(_) => to_value(&native_account_panel(
            accounts.as_ref(),
            state.menus.account_error.as_deref(),
            state.menus.accounts_busy,
            state.core.hide_account_emails,
            &panel_context,
        )),
    });
    into.account_switch_card = Tri::Value(match &status.visible {
        None => Value::Null,
        Some(progress) => native_account_switch_card(
            progress,
            accounts.as_ref(),
            state.menus.account_error.as_deref(),
            crate::menus::controls::switch_ready(state),
            state.menus.accounts_busy,
            state.core.hide_account_emails,
            status.now_ms,
        )
        .map(|card| to_value(&card))
        .unwrap_or(Value::Null),
    });
    into.account_status = AccountStatus {
        visible: status.visible.as_ref().map(|_| {
            state
                .session
                .account_switch
                .value()
                .cloned()
                .unwrap_or(Value::Null)
        }),
        now: status.now_ms,
        busy: status.busy,
        extra: serde_json::Map::new(),
    };

    // One build of the menu inputs for both halves: each half building its own recomputed the
    // options and swept every provider's model catalog again, twice per document.
    let inputs = crate::menus::picker::inputs::menu_inputs_from(state, &options);
    crate::menus::picker::document::document_with(state, context, &inputs, into);
    crate::menus::context::document::document_with_icon(
        state,
        context,
        inputs.agent_icon.clone(),
        into,
    );
}

/// A field the TypeScript hook normalizes to `null` when it has no value: present either way.
fn nullable(value: Option<Value>) -> Tri<Value> {
    match value {
        Some(value) => Tri::Value(value),
        None => Tri::Null,
    }
}

/// Serializes one of family e's own types. Every one of them is plain data, so this cannot fail;
/// a failure would be a programming error rather than an input, and `null` is what the renderer
/// draws as "nothing here".
fn to_value<T: serde::Serialize>(value: &T) -> Value {
    serde_json::to_value(value).unwrap_or(json!(null))
}
