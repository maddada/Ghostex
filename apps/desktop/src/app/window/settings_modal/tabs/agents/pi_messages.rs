//! The Messages group of OMP's and Pi's row panels in Settings > Agents, also shown for every custom
//! agent that works like one of them: Messages sent while it works (Steer or After yield). One
//! setting for all of them; gxserver passes it with each message it hands a busy OMP or Pi
//! (server/src/session_chat_queue_runtime/pi_channel_send.rs, where the user's decision is).

use super::super::super::catalog::{module, settings_catalog};
use super::super::super::fields::{RowSpec, SELECT_WIDTH, reset_key, setting_row, settings_select};
use super::super::super::palette::SettingsPalette;
use super::super::super::search::TabSearch;
use super::AgentsTab;
use super::model::AgentButton;
use gpui::{AnyElement, Context, SharedString, Window};
use serde_json::json;

const MESSAGES_WHILE_WORKING: &str = "piMessagesWhileWorking";

/// OMP, Pi, or a custom agent that works like one of them.
pub(super) fn is_pi_like(agent: &AgentButton) -> bool {
    let pi_like = |id: &str| matches!(id, "pi" | "omp");
    (agent.is_default && pi_like(&agent.agent_id)) || agent.icon.as_deref().is_some_and(pi_like)
}

/// Whether a search found the group's row, which opens the panels that hold it.
pub(super) fn search_opens_pi(search: &TabSearch) -> bool {
    let section = search.section("agentList");
    section.is_searching && section.visible_keys.contains(MESSAGES_WHILE_WORKING)
}

impl AgentsTab {
    /// The group's row, in the panel's own row shape (`panel_child`).
    pub(super) fn pi_messages_row(
        &mut self,
        p: &SettingsPalette,
        agent_id: &str,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let values = self.store.read(cx).values();
        let options =
            settings_catalog().options(module::SETTINGS, "PI_MESSAGES_WHILE_WORKING_OPTIONS");
        let allowed: Vec<String> = options.iter().map(|option| option.value.clone()).collect();
        let current = values.choice(MESSAGES_WHILE_WORKING, &allowed);
        // Each panel draws its own copy of the row, so its element ids carry the agent.
        let select = settings_select(
            self,
            p,
            SharedString::from(format!("agent-pi-messages-{agent_id}")),
            &options,
            &current,
            Some(SELECT_WIDTH),
            false,
            None,
            |page: &mut Self, value, _window, cx| {
                page.save(MESSAGES_WHILE_WORKING, json!(value), cx)
            },
            window,
            cx,
        );
        setting_row(
            p,
            SharedString::from(format!("agent-pi-messages-row-{agent_id}")),
            RowSpec::new("Messages sent while it works")
                .description(
                    "Steer hands your message to the agent at its next step, as Enter does in its own terminal. After yield waits until it finishes its turn. One setting for OMP, Pi and every agent that works like them.",
                )
                .keyed(&values, MESSAGES_WHILE_WORKING),
            Some(reset_key::<Self>(MESSAGES_WHILE_WORKING)),
            select,
            cx,
        )
    }
}
