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
use crate::state::{ChatContext, ChatState};

/// Handles one context meter, context editor or context details action.
pub fn handle(state: &mut ChatState, action: &UserAction, context: &ChatContext) -> Vec<Effect> {
    match action.kind {
        ActionKind::ContextCompact => {
            // `if (!chat.working) await chat.send('/compact')`. Sending is family d's; the
            // refusal while the agent is busy is drawn by the meter's `compactDisabled`.
            Vec::new()
        }
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
