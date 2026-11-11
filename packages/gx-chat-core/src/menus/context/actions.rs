//! Family e2's context actions: the row editor's commands and Compact.
//!
//! Port of `nativeContextEditorCommand` (`packages/shared/session-chat-controller/
//! native-context-editor.ts`) and of the `contextCompact` arm of `native-host.ts`.

use serde_json::Value;

use crate::action::{ActionKind, UserAction};
use crate::effect::Effect;
use crate::menus::context::editor::{
    editor_row, reorder_context_details, toggle_context_detail_star, ContextEditorState,
};
use crate::menus::context::preferences::{
    context_preferences_key, normalize_preferences, recommended_preferences, serialize_preferences,
};
use crate::menus::context::rows::GroupId;
use crate::menus::context::status::ContextDetailsAgent;
use crate::menus::option_dispatch::DispatchStep;
use crate::state::{ChatContext, ChatState};

/// Handles one context meter, context editor or context details action.
pub fn handle(state: &mut ChatState, action: &UserAction, context: &ChatContext) -> Vec<Effect> {
    match action.kind {
        ActionKind::ContextCompact => compact(state, context),
        ActionKind::ContextEdit => {
            let agent = editor_agent(state, context);
            state.pickers.context.editor = Some(ContextEditorState {
                agent,
                draft: normalize_preferences(
                    serde_json::to_value(state.pickers.context.preferences.get(agent))
                        .ok()
                        .as_ref(),
                    agent,
                ),
                query: String::new(),
                saving: false,
                error: None,
            });
            Vec::new()
        }
        _ => editor_command(state, action, context),
    }
}

/// `if (!chat.working) await chat.send('/compact')`: Compact types `/compact` into the agent the
/// way a typed one goes, so its "Ran /compact" row shows at once and gives way to the agent's own
/// compaction card.
///
/// CDXC:SessionChat 2026-11-11 WHY:
/// GitHub #572: "Compact context" in the context meter's panel did nothing at all. The Rust port of
/// this arm left the send to family d and returned no effects, so the click closed the panel and
/// nothing reached the agent. It now goes through the option dispatch's command step (the seam
/// `/model` and `/effort` use to type into the agent), which takes the row back and puts
/// gxserver's refusal on the chat box's error line when the send fails. A click the core cannot
/// run says why there too instead of doing nothing: the button is greyed with this reason while
/// the agent works, but the click can still race a turn that just started.
fn compact(state: &mut ChatState, context: &ChatContext) -> Vec<Effect> {
    if crate::session::working::is_working(state) {
        state
            .core
            .fail("Compact is available once the agent is idle.", None);
        return Vec::new();
    }
    if state.menus.option_dispatch_id.is_some() {
        state.core.fail(
            "A setting change is still reaching the agent. Compact again once it lands.",
            None,
        );
        return Vec::new();
    }
    state.menus.option_dispatch_id = Some("contextCompact".to_string());
    crate::menus::dispatch_run::begin(
        state,
        context,
        vec![DispatchStep::Command {
            text: "/compact".to_string(),
        }],
        None,
    )
}

/// The rest of `nativeContextEditorCommand`, which is refused while a save is in flight.
fn editor_command(
    state: &mut ChatState,
    action: &UserAction,
    _context: &ChatContext,
) -> Vec<Effect> {
    let Some(editor) = state.pickers.context.editor.as_mut() else {
        return Vec::new();
    };
    if editor.saving {
        return Vec::new();
    }
    let row = editor_row(editor, action.param("id").and_then(Value::as_str));
    match action.kind {
        ActionKind::ContextCancel => {
            state.pickers.context.editor = None;
        }
        ActionKind::ContextQuery => {
            editor.query = action
                .param("query")
                .and_then(Value::as_str)
                .unwrap_or_default()
                .to_string();
        }
        ActionKind::ContextShown => {
            if let Some(row) = row {
                editor.draft.shown.insert(
                    row.id.to_string(),
                    action.param("shown") == Some(&Value::Bool(true)),
                );
            }
        }
        ActionKind::ContextStar => {
            if let Some(row) = row {
                editor.draft = toggle_context_detail_star(&editor.draft, &row, editor.agent);
            }
        }
        ActionKind::ContextReorder => {
            let group = action.param("group").and_then(Value::as_str);
            let target = match group {
                Some("starred") => Some(None),
                Some(group) => GroupId::from_wire(group).map(Some),
                None => None,
            };
            let from = action.param("from").and_then(Value::as_str);
            let to = action.param("to").and_then(Value::as_str);
            if let (Some(target), Some(from), Some(to)) = (target, from, to) {
                editor.draft =
                    reorder_context_details(&editor.draft, editor.agent, target, from, to);
            }
        }
        ActionKind::ContextReset => {
            editor.draft = recommended_preferences(editor.agent);
        }
        ActionKind::ContextSave => {
            editor.saving = true;
            editor.error = None;
            let agent = editor.agent;
            let draft = editor.draft.clone();
            return vec![Effect::WriteStorage {
                key: context_preferences_key(agent),
                value: Some(serialize_preferences(&draft)),
                durable: false,
            }];
        }
        _ => {}
    }
    Vec::new()
}

/// `chat.sessionOptions.catalog?.modelIcon === 'codex' ? 'codex' : 'claude'`.
fn editor_agent(state: &ChatState, context: &ChatContext) -> ContextDetailsAgent {
    ContextDetailsAgent::from_icon(
        crate::menus::picker::inputs::menu_inputs(state, context)
            .agent_icon
            .as_deref(),
    )
}
