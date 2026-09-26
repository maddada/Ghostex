//! Family f's user actions: the panels, transcript search, the terminal tail, the subagent viewer
//! and Save to Markdown.
//!
//! Panel folds, transcript search, terminal-tail reads and the subagent viewer are pure view
//! state: none of them clears a send error, which is why nothing here touches
//! [`crate::CoreState`].

use serde_json::{json, Value};

use crate::action::{ActionKind, UserAction};
use crate::effect::Effect;
use crate::extras::subagent_target::SubagentTarget;
use crate::extras::{panels, save_markdown, search, subagent};
use crate::state::{ChatContext, ChatState};
use crate::wire::ChatRpcMethod;

/// Handles one action family f owns.
///
/// `next_request_id` hands out the core's own monotonic ids; the default [`handle`] passes a
/// counter family a owns, so the same inputs always produce the same ids.
pub fn handle_with_ids(
    state: &mut ChatState,
    action: &UserAction,
    context: &ChatContext,
    mut next_request_id: impl FnMut() -> u64,
) -> Vec<Effect> {
    match action.kind {
        ActionKind::ToggleAgentFleet => {
            state.extras.panels.fleet_open = Some(flag(action, "open"));
            Vec::new()
        }
        ActionKind::ToggleAgentTasks => {
            let collapsed = !flag(action, "open");
            state.extras.panels.tasks_collapsed = collapsed;
            vec![panels::write_tasks_collapsed(collapsed)]
        }
        ActionKind::ToggleAgentTasksCompleted => {
            state.extras.panels.tasks_show_completed = flag(action, "expanded");
            Vec::new()
        }

        ActionKind::SearchOpen => {
            search::open(&mut state.extras.search);
            Vec::new()
        }
        ActionKind::SearchClose => {
            search::close(&mut state.extras.search);
            Vec::new()
        }
        ActionKind::SearchQuery => {
            search::set_query(&mut state.extras.search, text(action, "query"));
            Vec::new()
        }
        ActionKind::SearchNext => {
            search::move_by(&mut state.extras.search, 1);
            Vec::new()
        }
        ActionKind::SearchPrevious => {
            search::move_by(&mut state.extras.search, -1);
            Vec::new()
        }

        ActionKind::TerminalTailHover => {
            let request_id = next_request_id();
            state.extras.terminal_tail.hover_request = Some(request_id);
            vec![read_terminal_tail(request_id)]
        }
        ActionKind::TerminalTailToggle => {
            let tail = &mut state.extras.terminal_tail;
            if tail.notice_open {
                tail.notice_open = false;
                return Vec::new();
            }
            // Re-read on every expand: the whole point is the CURRENT screen, and by the time a
            // user re-opens it they have usually just tried something.
            tail.notice_open = true;
            tail.notice_loading = true;
            tail.notice_error = None;
            let request_id = next_request_id();
            tail.notice_request = Some(request_id);
            vec![read_terminal_tail(request_id)]
        }

        ActionKind::OpenSubagent => {
            let Some(selector) = label(action, "selector") else {
                return Vec::new();
            };
            let target = SubagentTarget {
                name: label(action, "name").unwrap_or_else(|| selector.clone()),
                agent_type: label(action, "agentType"),
                task: label(action, "task"),
                model: label(action, "model"),
                effort: label(action, "effort"),
                selector,
            };
            subagent::open(&mut state.extras.subagent, target, next_request_id())
        }
        ActionKind::SubagentBack => subagent::back(&mut state.extras.subagent, next_request_id()),
        ActionKind::SubagentClose => subagent::close(&mut state.extras.subagent, next_request_id()),
        ActionKind::SubagentRetry => {
            subagent::request(&mut state.extras.subagent, false, next_request_id())
        }
        ActionKind::SubagentLoadEarlier => {
            subagent::request(&mut state.extras.subagent, true, next_request_id())
        }

        ActionKind::MarkdownSaveOpen => save_markdown::open(
            &mut state.extras.save_markdown,
            state.core.title.as_deref().unwrap_or_default(),
            text(action, "markdown"),
            context,
            next_request_id(),
        ),
        ActionKind::MarkdownSaveFolder => {
            save_markdown::set_folder(&mut state.extras.save_markdown, text(action, "value"));
            Vec::new()
        }
        ActionKind::MarkdownSaveName => {
            save_markdown::set_file_name(&mut state.extras.save_markdown, text(action, "value"));
            Vec::new()
        }
        ActionKind::MarkdownSaveCancel => {
            save_markdown::close(&mut state.extras.save_markdown);
            Vec::new()
        }
        ActionKind::MarkdownSaveSubmit => {
            save_markdown::submit(&mut state.extras.save_markdown, next_request_id())
        }

        _ => Vec::new(),
    }
}

/// Handles one action family f owns, with ids drawn from the core's one allocator.
///
/// The counter is taken out of [`crate::CoreState`] for the call and written back afterwards,
/// which is how a `&mut ChatState` and an id source live in the same expression. It is the same
/// counter every other family draws from, so [`crate::Event::RpcSettled`] routes by id alone with
/// no chance of two families claiming the same one.
pub fn handle(state: &mut ChatState, action: &UserAction, context: &ChatContext) -> Vec<Effect> {
    let mut next = state.core.next_request_id;
    let effects = handle_with_ids(state, action, context, || {
        next += 1;
        next
    });
    state.core.next_request_id = next;
    effects
}

/// `POST /api/readSessionTerminalTail`.
fn read_terminal_tail(request_id: u64) -> Effect {
    Effect::SendRpc {
        request_id,
        method: ChatRpcMethod::ReadSessionTerminalTail,
        params: Box::new(json!({})),
    }
}

/// `command.<name> === true`, which is how every one of these folds is sent.
fn flag(action: &UserAction, name: &str) -> bool {
    action.param(name) == Some(&Value::Bool(true))
}

/// `command.<name> ?? ''`.
fn text<'a>(action: &'a UserAction, name: &str) -> &'a str {
    action
        .param(name)
        .and_then(Value::as_str)
        .unwrap_or_default()
}

/// `label`: a non-blank string, kept untrimmed the way the viewer's own reader does.
fn label(action: &UserAction, name: &str) -> Option<String> {
    let value = action.param(name)?.as_str()?;
    (!crate::extras::agent_tasks::js_trim(value).is_empty()).then(|| value.to_string())
}
