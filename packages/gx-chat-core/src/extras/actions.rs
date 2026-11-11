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
        ActionKind::SessionDetailsVisible
        | ActionKind::SessionDetailsRefresh
        | ActionKind::SessionDetailsSection
        | ActionKind::SessionDetailsList
        | ActionKind::SessionDetailsStop
        | ActionKind::SessionDetailsOpen => {
            crate::extras::session_details::actions::handle(state, action, next_request_id)
        }
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
            // `step` is how many "Show more" presses the list has had; 0 is the short form. An
            // older client sends `expanded` instead.
            state.extras.panels.tasks_done_step = action
                .param("step")
                .and_then(Value::as_u64)
                .map_or(u32::from(flag(action, "expanded")), |step| {
                    step.min(u32::MAX as u64) as u32
                });
            Vec::new()
        }
        ActionKind::ToggleBackgroundTasks => {
            state.extras.panels.background_tasks_open = flag(action, "open");
            Vec::new()
        }
        ActionKind::ToggleCoordinatorThreadsMore => {
            let panels = &mut state.extras.panels;
            let step = action.param("step").and_then(Value::as_u64).unwrap_or(0);
            crate::extras::coordinator_threads::set_step(panels, step.min(u32::MAX as u64) as u32);
            crate::extras::coordinator_threads::read_all_threads(
                panels,
                state.session.coordinator_threads.as_ref(),
                next_request_id,
            )
            .into_iter()
            .collect()
        }
        ActionKind::ToggleCoordinatorBoardGroup => {
            crate::extras::coordinator_board::toggle(
                &mut state.extras.panels.board_toggled,
                text(action, "group"),
            );
            Vec::new()
        }
        ActionKind::ShowCoordinatorBoardGroup => {
            crate::extras::coordinator_board::show_group(state, text(action, "group"))
        }
        ActionKind::InsertBoardReference => {
            crate::extras::coordinator_board::insert_reference(&action.params)
        }
        ActionKind::HideCoordinatorIntro => {
            crate::extras::coordinator_intro::hide(state, next_request_id())
        }
        ActionKind::ToggleAutopilot => crate::extras::autopilot::toggle(state, next_request_id()),
        ActionKind::SetCollabGuest => crate::extras::collab::set_guest(state, action),
        ActionKind::ResolveCollabMessage => {
            crate::extras::collab::resolve(state, action, next_request_id())
        }
        ActionKind::UseCoordinatorIntroExample => {
            crate::extras::coordinator_intro::use_example(&action.params)
        }
        // A row of the In progress list shows its line of work in the Work panel, the chat staying
        // where it is (`open_questions::show`).
        // A page row in the dock asks how many of the page's questions are answered.
        ActionKind::LoadPageQuestionCounts => {
            crate::extras::work_cards::load_page_counts(state, text(action, "pageId"))
        }
        ActionKind::JumpToWorkCard => {
            crate::extras::coordinator_board::focus_row(state, text(action, "key"))
        }
        // A work ask snoozed from its card: out of the dock and the counts until its end
        // (server/src/coordinators/work.rs `snooze`).
        ActionKind::SnoozeWorkAsk => {
            let request_id = next_request_id();
            state.extras.panels.board_change_requests.insert(request_id);
            let key = text(action, "key").to_string();
            let hours = action.param("hours").and_then(Value::as_u64).unwrap_or(1);
            let until = context.now_millis() + hours as i64 * 3_600_000;
            state.extras.panels.work_snoozed.insert(key, until);
            state.core.request_publish();
            vec![crate::effect::Effect::SendRpc {
                request_id,
                method: crate::wire::ChatRpcMethod::CoordinatorBoard,
                params: Box::new(serde_json::json!({
                    "projectId": state.identity.project_id,
                    "sessionId": state.identity.session_id,
                    "op": "snooze",
                    "item": text(action, "item"),
                    "ask": text(action, "ask"),
                    "hours": hours,
                })),
            }]
        }
        // A dock row's "Show in chat": scrolls to the reply of the turn that asked or changed it,
        // only because the reader pressed it (work_cards.rs `show_in_chat`).
        ActionKind::ShowInChat => {
            if crate::questions::open_questions::jump_to_row(
                state,
                context,
                text(action, "messageId"),
                text(action, "target"),
                None,
            ) {
                Vec::new()
            } else {
                vec![crate::effect::Effect::Toast {
                    level: "info".to_string(),
                    message: "That turn isn't loaded in this chat any more.".to_string(),
                }]
            }
        }
        // The dock hidden (the phone's x; the computer's header collapses it), and shown again
        // (the phone's Work pill, the computer's header, or something new).
        ActionKind::HideWorkDock => {
            let keys = crate::extras::work_cards::dock(state, context)["rows"]
                .as_array()
                .into_iter()
                .flatten()
                .map(|row| row["key"].as_str().unwrap_or_default().to_string())
                .collect();
            state.extras.panels.work_dock_hidden = Some(keys);
            state.core.request_publish();
            Vec::new()
        }
        ActionKind::ShowWorkDock => {
            state.extras.panels.work_dock_hidden = None;
            state.core.request_publish();
            Vec::new()
        }
        // A finished-work card leaves the dock (work_cards.rs `dock`).
        ActionKind::DismissWorkDockCard => {
            state
                .extras
                .panels
                .work_dock_dismissed
                .insert(text(action, "key").to_string());
            state.core.request_publish();
            Vec::new()
        }
        ActionKind::CoordinatorBoardAnswer => {
            crate::extras::coordinator_board::answer(state, &action.params, next_request_id())
                .into_iter()
                .collect()
        }
        ActionKind::CoordinatorBoardChange => {
            crate::extras::coordinator_board::change(state, &action.params, next_request_id())
                .into_iter()
                .collect()
        }
        ActionKind::CoordinatorBoardMore => {
            crate::extras::coordinator_board::toggle(
                &mut state.extras.panels.board_expanded,
                text(action, "group"),
            );
            Vec::new()
        }
        // gxserver resumes a closed thread first; the host's focus follows its answer.
        ActionKind::OpenCoordinatorThread => crate::extras::coordinator_threads::open_thread(
            &mut state.extras.panels,
            text(action, "serverId"),
            text(action, "projectId"),
            text(action, "sessionId"),
            next_request_id(),
        )
        .into_iter()
        .collect(),

        ActionKind::OpenSideChat => {
            crate::extras::side_chats::open(text(action, "projectId"), text(action, "sessionId"))
                .into_iter()
                .collect()
        }
        ActionKind::BringBackSideChat | ActionKind::DeleteSideChat => {
            let target = action
                .param("sessionId")
                .and_then(Value::as_str)
                .map(str::to_string);
            let call = if action.kind == ActionKind::BringBackSideChat {
                crate::extras::side_chats::SideChatCall::BringBack(target)
            } else {
                crate::extras::side_chats::SideChatCall::Delete(target)
            };
            vec![crate::extras::side_chats::call(
                &mut state.extras.panels,
                call,
                next_request_id(),
            )]
        }

        ActionKind::AnswerRelayedApproval => {
            crate::extras::relayed_approvals::answer(state, action, next_request_id())
                .into_iter()
                .collect()
        }
        ActionKind::AnswerSecretRequest => {
            crate::extras::secret_requests::answer(state, action, next_request_id())
                .into_iter()
                .collect()
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
        ActionKind::SearchNext => search::navigate(state, 1, context),
        ActionKind::SearchPrevious => search::navigate(state, -1, context),
        ActionKind::SearchFolds => {
            search::set_folds(&mut state.extras.search, &action.params);
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

        ActionKind::CopyReport => {
            crate::extras::report::request(&mut state.extras.terminal_tail, next_request_id())
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
pub(crate) fn read_terminal_tail(request_id: u64) -> Effect {
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
