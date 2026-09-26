//! Family f's part of the document: the panels, the working strip, the terminal tail, the subagent
//! viewer, transcript search, the Save to Markdown sheet, the empty state and the loading stage.

use ghostex_gx_protocol::Tri;
use serde_json::Value;

use crate::document::{Document, NewSessionWelcome};
use crate::extras::welcome::{
    empty_state_copy, new_session_welcome_title, shows_new_session_welcome, welcome_agent_icon,
    welcome_agent_name,
};
use crate::extras::{panels, save_markdown, search, subagent, terminal_tail, working_strip};
use crate::state::{ChatContext, ChatState};

/// Writes family f's keys into `into`.
///
/// `agentFleet`, `agentTasks` and `terminalActivity` are folded by family a and cleared by any
/// frame that can carry them and does not; the strips and panels here are their projections and
/// are never gated on `working`, because subagents outlive the turn that spawned them.
pub fn document(state: &ChatState, context: &ChatContext, into: &mut Document) {
    let extras = &state.extras;
    into.terminal_activity = tri(state.session.terminal_activity.clone());
    into.agent_fleet = tri(state.session.agent_fleet.clone());
    into.agent_tasks = tri(state.session.agent_tasks.clone());

    let (fleet_strip, tasks_panel, _) = panels::project(state, context);
    into.agent_fleet_strip = Tri::Value(fleet_strip);
    into.agent_tasks_panel = Tri::Value(tasks_panel);

    let working = state.session.server_working || state.session.external_working;
    into.working_strip = working_strip::working_strip(state, context, working);
    into.terminal_tail = terminal_tail::project(&extras.terminal_tail);
    into.subagent = Tri::Value(subagent::project(state));
    into.transcript_search = Tri::Value(search::project(&extras.search));
    into.save_markdown = Tri::Value(save_markdown::project(&extras.save_markdown));

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
    let show_welcome = shows_new_session_welcome(view_kind)
        || (view_kind == "loading" && state.session.available_agents.is_some());
    into.new_session_welcome = show_welcome.then(|| {
        let agent_name = welcome_agent_name(state.session.agent.as_deref());
        NewSessionWelcome {
            title: new_session_welcome_title(agent_name.as_deref()),
            icon: welcome_agent_icon(state.session.agent.as_deref(), None),
            // The welcome drops its headline once a notice or question card takes the space below
            // it; both cards are family c's, so the flag reads their published state.
            show_title: !bottom_card_visible(into),
            agent_name,
        }
    });
    into.loading_stage = (view_kind == "loading" && !show_welcome).then(|| extras.loading_stage.clone());
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
