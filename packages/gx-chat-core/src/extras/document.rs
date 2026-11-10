//! Family f's part of the document: the panels, the working strip, the terminal tail, the subagent
//! viewer, transcript search, the Save to Markdown sheet, the empty state and the loading stage.

use ghostex_gx_protocol::Tri;
use serde_json::Value;

use crate::document::{Document, NewSessionWelcome};
use crate::extras::welcome::{
    empty_state_copy, loading_notice, new_session_welcome_title, shows_new_session_welcome,
    welcome_agent_icon, welcome_agent_name,
};
use crate::extras::{
    chat_pills, coordinator_threads, panels, relayed_approvals, save_markdown, search,
    secret_requests, side_chats, subagent, terminal_tail, working_card, working_strip,
};
use crate::state::{ChatContext, ChatState};

/// Writes family f's keys into `into`.
///
/// `agentFleet`, `agentTasks` and `terminalActivity` are folded by family a and cleared by any
/// frame that can carry them and does not; the strips and panels here are their projections and
/// are never gated on `working`, because subagents outlive the turn that spawned them.
pub fn document(state: &ChatState, context: &ChatContext, into: &mut Document) {
    let board = crate::document::SharedValue::from(crate::extras::coordinator_board::project(
        state, context,
    ));
    document_with_board(state, context, into, board);
}

/// [`document`] with the coordinator's board already projected (`coordinator_board::project_kept`).
pub fn document_with_board(
    state: &ChatState,
    context: &ChatContext,
    into: &mut Document,
    board: crate::document::SharedValue,
) {
    let extras = &state.extras;
    into.terminal_activity = tri(state.session.terminal_activity.clone());
    into.agent_fleet = tri(state.session.agent_fleet.clone());
    into.agent_tasks = tri(state.session.agent_tasks.clone());

    let (fleet_strip, tasks_panel, _) = panels::project(state, context);
    let threads_panel = coordinator_threads::project(
        state.session.coordinator_threads.as_ref(),
        extras.panels.threads_step,
        extras.panels.threads_all.as_ref().map(|(_, rows)| rows),
    );

    let working = crate::session::working::presents_working(state);
    into.working_strip = working_strip::working_strip(state, context, working);
    // Background tasks are the Background pill's menu, not a card on the working strip.
    let background = into
        .working_strip
        .presentation
        .as_mut()
        .and_then(Value::as_object_mut)
        .and_then(|presentation| presentation.remove("backgroundTasks"))
        .unwrap_or(Value::Null);
    if !background.is_null() {
        into.working_strip.presentation = None;
    }
    // The Session details panel on screen lists the tasks, so the Tasks pill steps aside.
    let tasks_in_session_details =
        crate::extras::session_details::settle::enabled(state) && extras.session_details.visible;
    into.chat_pills = chat_pills::chat_pills(
        &into.open_questions,
        &threads_panel,
        &tasks_panel,
        &fleet_strip,
        &background,
        tasks_in_session_details,
    );
    // The Board pill leads the core's pills, ahead of Resources (the renderers' ? pill sits
    // before it).
    //
    // CDXC:Coordinators 2026-11-06 DECISION:
    // User, of the strip reading "Resources 605 · Board · Workers · Stashed": "i asked u to swap the
    // placement of board and resources pills here, pls do it". Order: ?, Board, Resources, Workers.
    // This is the one place the order is set, so the desktop, the web build and the phone follow.
    // CDXC:Coordinators 2026-11-10 DECISION:
    // User, of a coordinator chat showing the same state six times (a card and an In progress row
    // under the reply, a status line above the box, the Work pill and the panel): "This is a mess
    // man. we need something more cohesive. wtf is this?", then "Maybe we don't actually need the
    // In progress card inside the chat transcript since we have it in the work sidebar. let's have
    // it as a pill instead." The Work pill is the one place for the counts, in words while the row
    // has room ("Work · 4 need you · 7 in progress", `pill`); above the box only the cards he must
    // act on (`work_cards::dock`); under a reply one quiet line.
    if let Some(pill) = crate::extras::coordinator_board::pill(&board) {
        into.chat_pills.insert(0, pill);
        // Blocked, before the Board pill (just after the renderers' ? pill), while anything is
        // blocked.
        if let Some(blocked) = crate::extras::coordinator_board::blocked_pill(&board) {
            into.chat_pills.insert(0, blocked);
        }
    }
    into.coordinator_board = Tri::Value(board);
    // Autopilot's switch, after the coordinator's pills (a session's own while Autopilot is on).
    into.chat_pills
        .extend(crate::extras::autopilot::pill(state));
    // The phone's Share pill (the desktop has the header's Share button).
    into.chat_pills.extend(crate::extras::collab::pill(state));
    // The coordinator tips' pill closes the core's pills (before Stashed), on a coordinator only.
    let intro = crate::extras::coordinator_intro::project(state);
    into.chat_pills
        .extend(crate::extras::coordinator_intro::pill(&intro));
    into.coordinator_intro = Tri::Value(intro);
    let side_chats_panel = side_chats::project(state.session.side_chats.as_ref());
    if let Some(pill) = side_chats::pill(&side_chats_panel) {
        into.chat_pills.insert(0, pill);
    }
    // The Approvals pill leads the strip while a thread's relayed card waits on the user.
    let approvals = relayed_approvals::project(state);
    if let Some(pill) = relayed_approvals::pill(&approvals) {
        into.chat_pills.insert(0, pill);
    }
    into.relayed_approvals = Tri::Value(approvals);
    into.chat_pills.extend(chat_pills::stashed_pill(
        state.composer.chrome.stashed_prompt_count(),
    ));
    into.view_chips = chat_pills::view_chips(crate::transcript::view_level::view_level(
        &state.transcript_view,
    ));
    into.view_chips
        .extend(crate::transcript::quiet_view::view_chip(state));
    into.working_card = working_card::working_card(state, &into.working_strip, &into.chat_pills);
    into.agent_fleet_strip = Tri::Value(fleet_strip);
    into.agent_tasks_panel = Tri::Value(tasks_panel);
    into.coordinator_threads_panel = Tri::Value(threads_panel);
    into.side_chats_panel = Tri::Value(side_chats_panel);
    into.secret_requests = Tri::Value(secret_requests::project(state));
    into.collab = Tri::Value(crate::extras::collab::project(state));
    // An approval answered in this chat leaves its "Approved" / "Denied" line after the
    // coordinator's (questions/pending_approval.rs), so the newest answer is the one drawn.
    let mut answer_lines = relayed_approvals::audit_lines(state);
    if let (Some(lines), Some(settled)) = (
        answer_lines.as_array_mut(),
        state.questions.settled_approval.as_ref(),
    ) {
        lines.push(settled.clone());
    }
    into.relayed_answer_lines = Tri::Value(answer_lines);
    into.side_chat_of = Tri::Value(side_chats::bar(
        state.session.side_chat_of.as_ref(),
        &state.identity.project_id,
        &state.transcript_view.session_directory,
    ));
    into.background_tasks_panel = Tri::Value(background);
    into.terminal_tail = terminal_tail::project(&extras.terminal_tail);
    into.subagent = Tri::Value(subagent::project(state));
    into.transcript_search = Tri::Value(search::project(&extras.search));
    into.save_markdown = Tri::Value(save_markdown::project(&extras.save_markdown));
    into.session_details = crate::extras::session_details::document::document(state, context);

    // The copy comes from the view, not the agent status, as React picked it.
    let view_kind = into.view.kind.as_str();
    into.empty_state = empty_state_copy(
        if view_kind == "ready" {
            "empty"
        } else {
            view_kind
        },
        state.session.agent.as_deref(),
    );
    /*
    The new-session welcome, projected for the native chat the same way React rendered it: a
    `starting` or `empty` transcript greets the user with the agent mark and headline instead of
    falling through to the `emptyState` loading copy.
    */
    // A sleeping Cloud Box's session is not a new one: with nothing cached it says why the
    // conversation is not shown, instead of greeting (`session/cloud_box.rs`).
    let box_asleep = state.session.cloud_box_asleep;
    if box_asleep {
        into.empty_state = crate::session::cloud_box::asleep_empty_state();
    }
    let show_welcome = !box_asleep
        && (shows_new_session_welcome(view_kind)
            || (view_kind == "loading" && state.session.available_agents.is_some()));
    into.new_session_welcome = show_welcome.then(|| {
        let agent_name = welcome_agent_name(state.session.agent.as_deref());
        NewSessionWelcome {
            // CDXC:Coordinators 2026-09-30 WHY: a new coordinator's first screen says what it is for; the generic "What should we build with Claude?" hides that this agent hands the work to threads.
            title: if coordinator_threads::is_coordinator(
                state.session.coordinator_threads.as_ref(),
            ) {
                crate::extras::welcome::wrap_new_session_welcome_title(
                    "What should this coordinator work on?",
                )
            } else {
                new_session_welcome_title(agent_name.as_deref())
            },
            icon: welcome_agent_icon(state.session.agent.as_deref(), None),
            // The welcome drops its headline once a notice or question card takes the space below
            // it; both cards are family c's, so the flag reads their published state.
            show_title: !bottom_card_visible(into),
            agent_name,
        }
    });
    into.loading_stage =
        (view_kind == "loading" && !show_welcome).then(|| extras.loading_stage.clone());
    // CDXC:SessionChat 2026-10-08 in `session/constants.rs`: while Ghostex's service is not
    // answering, the hold draws the skeleton rows and says nothing; the core is retrying.
    let unreachable = state.messages.unreachable_since_ms.is_some();
    into.transcript_skeleton = into.loading_stage.is_some() && unreachable;
    into.loading_notice = into
        .loading_stage
        .as_deref()
        .filter(|_| !unreachable)
        .and_then(loading_notice);
}

/// A notice or a question card is on screen, which is what the welcome gives its headline up for.
fn bottom_card_visible(document: &Document) -> bool {
    document.notice_visible || document.question_card.visible
}

/// `undefined` versus `null`: an absent value is a key the producer left out.
fn tri(value: Option<Value>) -> Tri<Value> {
    match value {
        Some(value) => Tri::Value(value),
        None => Tri::Null,
    }
}
