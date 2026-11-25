//! Where the Agents card's rows come from: gxserver's `agentRoster` (every agent, on and off,
//! with when it was last used), the switches flipped here that have not come back yet, the
//! dragged order, and the split between the list and the "More agents" grid.
use super::super::super::store::store_gxserver_rpc;
use super::AgentsTab;
use super::model::{
    AgentButton, agents_from_hud, merge_ids, reconcile_draft_ids, roster_from_hud_answer,
};
use gpui::Context;
use serde_json::{Value, json};
use std::time::Duration;

const ROSTER_TIMEOUT: Duration = Duration::from_secs(15);
/// How long after a switch write the roster is re-read to settle it.
const SETTLE_AFTER: Duration = Duration::from_millis(2500);

impl AgentsTab {
    /// Reads `readSidebarHud { includeAgentRoster }`. An older gxserver answers without the
    /// roster, and the page keeps showing `hud.agents` (every row on, no More agents).
    pub(super) fn roster_refresh(&mut self, cx: &mut Context<Self>) {
        self.roster_refresh_with(Vec::new(), cx);
    }

    /// `settle`: the agents of a write that had time to land; their switches show what gxserver
    /// stored afterwards (a write that failed snaps back).
    fn roster_refresh_with(&mut self, settle: Vec<String>, cx: &mut Context<Self>) {
        if !self.active || !self.cli_connected(cx) {
            return;
        }
        self.roster_seq += 1;
        let seq = self.roster_seq;
        self.roster_hud_signature = Some(self.hud_agents_signature(cx));
        let weak = cx.entity().downgrade();
        let store = self.store.clone();
        store_gxserver_rpc(
            &store,
            "/api/readSidebarHud",
            json!({ "includeAgentRoster": true }),
            ROSTER_TIMEOUT,
            move |result, cx| {
                let _ = weak.update(cx, |page, cx| {
                    if page.roster_seq != seq {
                        return;
                    }
                    if let Ok(answer) = result
                        && let Some(roster) = roster_from_hud_answer(&answer)
                    {
                        for agent_id in &settle {
                            page.pending_enabled.remove(agent_id);
                        }
                        page.pending_enabled.retain(|agent_id, enabled| {
                            roster
                                .iter()
                                .find(|agent| &agent.agent_id == agent_id)
                                .is_some_and(|agent| agent.enabled != *enabled)
                        });
                        page.roster = Some(roster);
                        if let Some(pins) = answer
                            .get("agentModelPins")
                            .and_then(Value::as_object)
                        {
                            page.agent_model_pins = pins
                                .iter()
                                .filter_map(|(family, pin)| {
                                    let model = pin.get("model").and_then(Value::as_str)?;
                                    (!ghostex_gx_chat_core::menus::catalog::is_placeholder_model_value(
                                        model,
                                    ))
                                    .then(|| (family.clone(), model.to_string()))
                                })
                                .collect();
                        }
                        cx.notify();
                    }
                });
            },
            cx,
        );
    }

    fn hud_agents_signature(&self, cx: &Context<Self>) -> String {
        self.store
            .read(cx)
            .hud()
            .and_then(|hud| hud.get("agents"))
            .map(Value::to_string)
            .unwrap_or_default()
    }

    /// Any agent write (here, in the launcher or on another client) changes `hud.agents`, so the
    /// roster is re-read whenever that changes.
    pub(super) fn roster_follow_hud(&mut self, cx: &mut Context<Self>) {
        if !self.active || self.roster_hud_signature.is_none() {
            return;
        }
        if self.roster_hud_signature.as_deref() != Some(self.hud_agents_signature(cx).as_str()) {
            self.roster_refresh(cx);
        }
    }

    /// Every agent in the shared order, with the switches flipped here applied.
    pub(super) fn all_agents(&self, cx: &Context<Self>) -> Vec<AgentButton> {
        let mut agents = match &self.roster {
            Some(roster) => roster.clone(),
            None => agents_from_hud(self.store.read(cx).hud()),
        };
        for agent in &mut agents {
            if let Some(enabled) = self.pending_enabled.get(&agent.agent_id) {
                agent.enabled = *enabled;
            }
        }
        agents
    }

    /// The rows of the list (on, used before, or custom) in display order, reconciling a dragged
    /// order with the synced roster first (`reconcileDraftIds` on every roster change).
    pub(super) fn ordered_agents(&mut self, cx: &Context<Self>) -> Vec<AgentButton> {
        let agents: Vec<AgentButton> = self
            .all_agents(cx)
            .into_iter()
            .filter(AgentButton::listed)
            .collect();
        let synced: Vec<String> = agents.iter().map(|agent| agent.agent_id.clone()).collect();
        if synced != self.synced_agent_ids {
            self.draft_agent_ids = reconcile_draft_ids(self.draft_agent_ids.as_deref(), &synced);
            self.synced_agent_ids = synced.clone();
        }
        let order = match &self.draft_agent_ids {
            Some(draft) => merge_ids(draft, &synced),
            None => synced,
        };
        order
            .iter()
            .filter_map(|id| agents.iter().find(|agent| &agent.agent_id == id).cloned())
            .collect()
    }

    /// The "More agents" grid: built-in agents that are off and were never used, the ones whose
    /// CLI is already on this computer first.
    pub(super) fn unlisted_agents(&self, cx: &Context<Self>) -> Vec<AgentButton> {
        let mut agents: Vec<AgentButton> = self
            .all_agents(cx)
            .into_iter()
            .filter(|agent| !agent.listed())
            .collect();
        agents.sort_by_key(|agent| !self.cli_found(&self.cli_agent_id(agent)));
        agents
    }

    /// The CLI an agent runs: a custom agent that works like a built-in runs that built-in's CLI.
    pub(super) fn cli_agent_id(&self, agent: &AgentButton) -> String {
        super::model::default_agent_by_icon(agent.icon.as_deref())
            .map(|default| default.agent_id.clone())
            .unwrap_or_else(|| agent.agent_id.clone())
    }

    /// The CLI check found the agent's executable on this computer.
    pub(super) fn cli_found(&self, cli_agent: &str) -> bool {
        self.cli
            .list
            .get(cli_agent)
            .is_some_and(|state| state.executable_path.is_some())
    }

    pub(super) fn move_agent(&mut self, from: usize, to: usize, cx: &mut Context<Self>) {
        let ids: Vec<String> = self
            .ordered_agents(cx)
            .into_iter()
            .map(|agent| agent.agent_id)
            .collect();
        let next = super::super::super::fields::move_index(&ids, from, to);
        self.draft_agent_ids = Some(next.clone());
        self.post(
            json!({ "agentIds": next, "requestId": super::model::reorder_request_id(), "type": "syncSidebarAgentOrder" }),
            cx,
        );
        cx.notify();
    }

    /// Turns agents on or off through gxserver (`setSidebarAgentsEnabled`): the row keeps its
    /// settings and its slot in the order.
    pub(super) fn set_agents_enabled(
        &mut self,
        agent_ids: Vec<String>,
        enabled: bool,
        cx: &mut Context<Self>,
    ) {
        if agent_ids.is_empty() {
            return;
        }
        for agent_id in &agent_ids {
            self.pending_enabled.insert(agent_id.clone(), enabled);
            if !enabled {
                self.turn_on.remove(agent_id);
            }
        }
        self.post(
            json!({ "agentIds": agent_ids.clone(), "enabled": enabled, "type": "setSidebarAgentsEnabled" }),
            cx,
        );
        cx.spawn(async move |this, cx| {
            cx.background_executor().timer(SETTLE_AFTER).await;
            let _ = this.update(cx, |page, cx| page.roster_refresh_with(agent_ids, cx));
        })
        .detach();
        cx.notify();
    }
}
