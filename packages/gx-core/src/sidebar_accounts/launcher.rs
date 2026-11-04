//! The agent launcher's account pages (`agentAccounts`): the counts on the agent list, one agent's
//! account page, and a launch that waits for the list so it can start as the provider's default
//! account.
//!
//! CDXC:AgentLauncher 2026-09-21 WHY:
//! A port of `createNativeAgentLauncherController`, whose state lived in a closure: the list read
//! for the OWNER group (dropped when a `load` arrives or the owner changes) and a generation that
//! makes an answer overtaken by a newer command publish nothing and cache nothing. Both live here
//! now. The agent and its back row are the ones the command found when it ARRIVED; the agent list,
//! the primary agent and the email setting are read when the page is BUILT, which is when the
//! TypeScript read them too (after its `await`).
//!
//! Ported from the sidebar page's agent launcher (frozen in the deleted
//! `tooling/gx-core/sidebar-page-frozen/agent-launcher.ts`; its gate,
//! `tooling/gx-core/account-menu-parity.ts`, is deleted too).
//!
//! SEE-ALSO: apps/desktop/src/app/gx_store/sidebar_accounts.rs.

use serde_json::{json, Map, Value};

use crate::sidebar_menu::{
    agent_launcher_items_with_accounts, run_in_box_agents_page, run_in_box_locations_page,
    AgentChoicePlace, LauncherAgent, MenuHost,
};

use super::data::AccountsState;
use super::items::{launcher_account_page, launcher_back, launcher_reading};
use super::rpc::group_accounts_target;
use super::{AccountMenuStep, AccountsRequest, RequestContext};

/// `{ type: 'agentAccounts', groupId, action, agentId?, runLocation? }`.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct LauncherCommand {
    pub group_id: String,
    pub action: String,
    pub agent_id: Option<String>,
    /// The box location a Run in a Box agents page lists agents for.
    pub run_location: Option<String>,
}

impl LauncherCommand {
    pub fn from_json(command: &Value) -> Option<Self> {
        if command.get("type").and_then(Value::as_str) != Some("agentAccounts") {
            return None;
        }
        Some(Self {
            group_id: command.get("groupId")?.as_str()?.to_string(),
            action: command.get("action")?.as_str()?.to_string(),
            agent_id: command
                .get("agentId")
                .and_then(Value::as_str)
                .map(str::to_string),
            run_location: command
                .get("runLocation")
                .and_then(Value::as_str)
                .map(str::to_string),
        })
    }

    fn owner_id(&self) -> String {
        format!("group:{}", self.group_id)
    }
}

/// What a launcher request carries back to its answer.
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct LauncherPending {
    pub(crate) generation: u64,
    pub(crate) command: LauncherCommand,
    /// The agent the command named, as the agent list held it when the command arrived.
    pub(crate) agent: Option<LauncherAgent>,
}

/// The closure state of `createNativeAgentLauncherController`.
#[derive(Clone, Debug, Default)]
pub struct LauncherAccounts {
    data: Option<AccountsState>,
    owner: Option<String>,
    generation: u64,
}

impl LauncherAccounts {
    pub(crate) fn command(
        &mut self,
        command: LauncherCommand,
        host: &MenuHost,
        place: &AgentChoicePlace,
        hide_account_emails: bool,
    ) -> Vec<AccountMenuStep> {
        if self.owner.as_deref() != Some(command.group_id.as_str()) || command.action == "load" {
            self.owner = Some(command.group_id.clone());
            self.data = None;
        }
        self.generation += 1;
        // The Run in a Box pages read nothing from the daemon: the ready locations are the host's.
        match command.action.as_str() {
            "box" => {
                return vec![AccountMenuStep::Publish {
                    owner_id: command.owner_id(),
                    items: run_in_box_locations_page(&command.group_id, host),
                    close: false,
                }];
            }
            "boxAgents" => {
                return vec![AccountMenuStep::Publish {
                    owner_id: command.owner_id(),
                    items: run_in_box_agents_page(
                        &command.group_id,
                        host,
                        command.run_location.as_deref().unwrap_or_default(),
                    ),
                    close: false,
                }];
            }
            _ => {}
        }
        let agent = command.agent_id.as_deref().and_then(|agent_id| {
            host.agents
                .iter()
                .find(|agent| agent.agent_id == agent_id)
                .cloned()
        });
        let provider = agent.as_ref().and_then(agent_provider);
        if command.action == "launch" && provider.is_none() {
            if let Some(agent) = &agent {
                // An agent with no account switcher starts at once, signed in as whatever its CLI
                // is signed in as.
                return vec![
                    AccountMenuStep::Launch {
                        group_id: command.group_id.clone(),
                        agent_id: agent.agent_id.clone(),
                        account_id: None,
                    },
                    close(&command),
                ];
            }
        }
        if self.data.is_none() || command.action == "retry" {
            let mut steps = Vec::new();
            // The React launcher opens the account page at once and shows this hint until the
            // list arrives.
            if let (Some(agent), Some(_), "accounts") = (&agent, provider, command.action.as_str())
            {
                steps.push(AccountMenuStep::Publish {
                    owner_id: command.owner_id(),
                    items: launcher_reading(launcher_back(&command.group_id, agent)),
                    close: false,
                });
            }
            let mut params = Map::new();
            params.insert("operation".to_string(), json!("list"));
            if command.action == "retry" {
                params.insert("refresh".to_string(), json!(true));
            }
            // The project the launcher is for, so the list's default Claude account is the one a
            // launch there would use (its workspace's account, `CDXC:Workspaces` in gxserver's
            // accounts endpoint). The project id is the raw one of the machine the request goes to.
            if let Some(project) =
                crate::keys::ProjectKey::parse_sidebar_group_id(&command.group_id)
            {
                params.insert("projectId".to_string(), json!(project.project_id));
            }
            steps.push(AccountMenuStep::Request(AccountsRequest {
                target: group_accounts_target(&command.group_id),
                params: Value::Object(params),
                context: RequestContext::Launcher(LauncherPending {
                    generation: self.generation,
                    command,
                    agent,
                }),
            }));
            return steps;
        }
        self.finish(
            &command,
            agent.as_ref(),
            None,
            host,
            place,
            hide_account_emails,
        )
    }

    /// The answer to a request this state issued. Returns `None` when a newer command overtook it,
    /// which publishes nothing and keeps nothing, exactly as the TypeScript's generation check.
    pub(crate) fn answer(
        &mut self,
        pending: LauncherPending,
        result: Result<AccountsState, String>,
        host: &MenuHost,
        place: &AgentChoicePlace,
        hide_account_emails: bool,
    ) -> Option<Vec<AccountMenuStep>> {
        if pending.generation != self.generation {
            return None;
        }
        let error = match result {
            Ok(data) => {
                self.data = Some(data);
                None
            }
            Err(error) => Some(error),
        };
        Some(self.finish(
            &pending.command,
            pending.agent.as_ref(),
            error.as_deref(),
            host,
            place,
            hide_account_emails,
        ))
    }

    fn finish(
        &self,
        command: &LauncherCommand,
        agent: Option<&LauncherAgent>,
        error: Option<&str>,
        host: &MenuHost,
        place: &AgentChoicePlace,
        hide_account_emails: bool,
    ) -> Vec<AccountMenuStep> {
        let provider = agent.and_then(agent_provider);
        if let (true, Some(agent), Some(_), Some(_)) = (
            command.action == "launch",
            agent,
            provider,
            self.data.as_ref(),
        ) {
            // CDXC:Workspaces 2026-10-09 WHY:
            // The launcher's own launch sends no account: gxserver resolves the provider's rule
            // itself (`accounts::launch::apply_new_session`), and an `accountId` on a create now
            // means the user picked that account, so the workspace's Claude account can win over
            // the rule. Sending the rule's pick as `accountId` made every launch look picked.
            return vec![
                AccountMenuStep::Launch {
                    group_id: command.group_id.clone(),
                    agent_id: agent.agent_id.clone(),
                    account_id: None,
                },
                close(command),
            ];
        }
        let (Some(agent), Some(provider)) = (agent, provider) else {
            return vec![AccountMenuStep::Publish {
                owner_id: command.owner_id(),
                items: agent_launcher_items_with_accounts(
                    &command.group_id,
                    place,
                    host,
                    self.data.as_ref(),
                ),
                close: false,
            }];
        };
        vec![AccountMenuStep::Publish {
            owner_id: command.owner_id(),
            items: launcher_account_page(
                &command.group_id,
                agent,
                provider,
                self.data.as_ref(),
                error,
                hide_account_emails,
            ),
            close: false,
        }]
    }
}

fn agent_provider(agent: &LauncherAgent) -> Option<&'static str> {
    agent.account_provider()
}

/// `update([], true)`: the menu goes away once a launch is under way.
fn close(command: &LauncherCommand) -> AccountMenuStep {
    AccountMenuStep::Publish {
        owner_id: command.owner_id(),
        items: Vec::new(),
        close: true,
    }
}
