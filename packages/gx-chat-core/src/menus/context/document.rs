//! Family e2's context keys: the meter and the row editor.
//!
//! Port of the `contextEditor` and `contextMeter` slices of `publish` in
//! `packages/shared/session-chat-controller/native-host.ts`.

use ghostex_gx_protocol::Tri;
use serde_json::Value;

use crate::document::Document;
use crate::menus::context::editor::context_editor_projection;
use crate::menus::context::meter::{compute_context_meter, ContextMeterInput};
use crate::menus::context::status::DetectedOptions;
use crate::menus::context::usage::mask_account_text;
use crate::menus::picker::inputs::menu_inputs;
use crate::state::{ChatContext, ChatState};

/// Writes family e2's context keys into `into`.
pub fn document(state: &ChatState, context: &ChatContext, into: &mut Document) {
    document_with_icon(state, context, menu_inputs(state, context).agent_icon, into);
}

/// [`document`] with the agent icon family e's document already read off this frame's options.
pub fn document_with_icon(
    state: &ChatState,
    context: &ChatContext,
    agent_icon: Option<String>,
    into: &mut Document,
) {
    let pickers = &state.pickers;
    let detected: Option<DetectedOptions> = state
        .session
        .selected_options
        .as_ref()
        .and_then(|value| serde_json::from_value(value.clone()).ok());
    // `accounts?.accounts.find((account) => account.id === accounts.session?.accountId)`. Family
    // e1 folds the raw answer; the context rows read more of an account than e1's menu view keeps
    // (the session count, the usage snapshot's age, every window's own reset), so the row is read
    // from the answer itself rather than from that view.
    let account = session_account(state.menus.accounts.as_ref());
    let input = ContextMeterInput {
        icon: agent_icon.as_deref(),
        selected_options: detected.as_ref(),
        account: account.as_ref(),
        agent_session_id: state.session.agent_session_id.clone(),
        // `chat.availableAgents !== null`: nothing has reached the agent yet.
        draft: state.session.available_agents.is_some(),
        title: state.core.title.clone(),
        // Family a writes `working` first (`document::assemble` runs a, b, c, d, e, f), so this is
        // the same derived flag the Compact button reads in the TypeScript.
        working: into.working,
        hide_account_emails: state.core.hide_account_emails,
    };
    let agent = crate::menus::context::ContextDetailsAgent::from_icon(input.icon);
    // The rows count down from the meter's own latched clock, not the event's.
    let mut meter_context = context.clone();
    if let Some(latched) = state.menus.meter_now_ms {
        meter_context.now_ms = latched;
    }
    // The pins of the session's computer when this host follows it (`computer_pins.rs`).
    let preferences = pickers
        .context
        .computer
        .preferences(agent, pickers.context.preferences.get(agent));
    let result = compute_context_meter(&input, &preferences, &meter_context);
    into.context_meter = match result.meter {
        Value::Null => Tri::Null,
        meter => Tri::Value(meter),
    };

    let hide = state.core.hide_account_emails;
    into.context_editor = match context_editor_projection(
        pickers.context.editor.as_ref(),
        &result.status,
        Some(&result.session),
        context,
        move |text| {
            if hide {
                mask_account_text(text)
            } else {
                text.to_string()
            }
        },
    ) {
        Value::Null => Tri::Null,
        editor => Tri::Value(editor),
    };
}

/// The session's saved account, read out of the answer family e1 folded.
fn session_account(accounts: Option<&Value>) -> Option<crate::menus::context::AgentAccount> {
    let accounts = accounts?;
    let account_id = accounts
        .get("session")
        .and_then(|session| session.get("accountId"))
        .and_then(Value::as_str)?;
    let entry = accounts
        .get("accounts")
        .and_then(Value::as_array)?
        .iter()
        .find(|account| account.get("id").and_then(Value::as_str) == Some(account_id))?;
    serde_json::from_value(entry.clone()).ok()
}
