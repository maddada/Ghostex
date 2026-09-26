//! A session row's Switch Account flyout (`sessionAccounts`) and the account-switch progress it
//! reports while a pick is in flight.
//!
//! CDXC:AgentProviders 2026-09-21 WHY:
//! A port of two closures that worked as one: `createNativeAccountMenuController` (one generation
//! for every row, so only the newest command's answer reaches the flyout) and the per-session
//! account-switch transport `requestSessionAccounts` kept in the runtime
//! (`createAccountSwitchTransport`), which remembers the LAST answer for each session whatever the
//! menu did with it. That memory is what lets a pick name the account's provider, email and
//! indicator before the daemon answers, so an answer the menu drops as overtaken is still kept
//! here, and a pick that is overtaken still clears its progress when it lands. The transport map
//! had no other caller in the sidebar's runtime, so this copy is the only one.
//!
//! CDXC:AgentProviders 2026-09-07 DECISION:
//! Show the selected provider and account email during a switch, starting before the request and
//! clearing on success or failure.
//!
//! SEE-ALSO: the deleted sidebar page's `accounts.ts`, the runtime's `account-switch.ts` and
//! `requestSessionAccounts` (deleted 2026-09-25, in git history),
//! apps/desktop/src/app/gx_store/sidebar_accounts.rs.

use std::collections::BTreeMap;

use serde_json::{json, Map, Value};

use crate::keys::{MachineId, SessionKey};

use super::data::AccountsState;
use super::items::{session_account_page, session_error_page};
use super::rpc::{session_accounts_target, AccountsTarget, SESSION_COMPUTER_UNAVAILABLE};
use super::{AccountMenuStep, AccountsRequest, RequestContext};

/// `{ type: 'sessionAccounts', sessionId, action, accountId? }`.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SessionAccountsCommand {
    pub session_id: String,
    pub action: String,
    pub account_id: Option<String>,
}

impl SessionAccountsCommand {
    pub fn from_json(command: &Value) -> Option<Self> {
        if command.get("type").and_then(Value::as_str) != Some("sessionAccounts") {
            return None;
        }
        Some(Self {
            session_id: command.get("sessionId")?.as_str()?.to_string(),
            action: command.get("action")?.as_str()?.to_string(),
            account_id: command
                .get("accountId")
                .and_then(Value::as_str)
                .map(str::to_string),
        })
    }
}

/// What a flyout request carries back to its answer.
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct SessionPending {
    pub(crate) generation: u64,
    pub(crate) command: SessionAccountsCommand,
    pub(crate) session: SessionKey,
    /// The pick posted a progress before the call, so it clears it when the call lands.
    pub(crate) switching: bool,
}

/// The closure state of the flyout controller and of the per-session transports.
#[derive(Clone, Debug, Default)]
pub struct SessionAccounts {
    generation: u64,
    /// Each session's newest answer, keyed by the sidebar session id.
    last_answer: BTreeMap<String, AccountsState>,
}

impl SessionAccounts {
    pub(crate) fn command(
        &mut self,
        command: SessionAccountsCommand,
        hide_account_emails: bool,
    ) -> Vec<AccountMenuStep> {
        self.generation += 1;
        let Some(session) = session_accounts_target(&command.session_id) else {
            // The transport cannot be built, so the request throws before any call.
            return vec![AccountMenuStep::Publish {
                owner_id: command.session_id.clone(),
                items: session_error_page(
                    &command.session_id,
                    SESSION_COMPUTER_UNAVAILABLE,
                    hide_account_emails,
                ),
                close: false,
            }];
        };
        let select = command.action == "select";
        let mut params = Map::new();
        if select {
            params.insert("operation".to_string(), json!("select"));
            if let Some(account_id) = &command.account_id {
                params.insert("accountId".to_string(), json!(account_id));
            }
        } else {
            params.insert("operation".to_string(), json!("session"));
            if command.action == "retry" {
                params.insert("refresh".to_string(), json!(true));
            }
        }
        params.insert("projectId".to_string(), json!(session.project_id));
        params.insert("sessionId".to_string(), json!(session.session_id));
        let mut steps = Vec::new();
        let memory = self.last_answer.get(&command.session_id);
        let progress = select
            .then(|| {
                let account_id = command.account_id.as_deref()?;
                memory?
                    .accounts
                    .iter()
                    .find(|account| account.id.as_deref() == Some(account_id))
            })
            .flatten()
            .map(|account| {
                let mut progress = Map::new();
                if let Some(provider) = &account.provider {
                    progress.insert("provider".to_string(), Value::String(provider.clone()));
                }
                // `account.email || account.name`.
                let email = account
                    .email
                    .clone()
                    .filter(|email| !email.is_empty())
                    .unwrap_or_else(|| account.name.clone());
                progress.insert("email".to_string(), Value::String(email));
                // `account.indicator || account.selector`, left out when both are missing.
                if let Some(indicator) = account
                    .indicator
                    .clone()
                    .filter(|indicator| !indicator.is_empty())
                    .or_else(|| account.selector.clone())
                {
                    progress.insert("indicator".to_string(), Value::String(indicator));
                }
                Value::Object(progress)
            });
        // `accounts?.session?.accountId !== params.accountId`.
        let switching = progress.is_some()
            && memory.and_then(AccountsState::session_account_id) != command.account_id.as_deref();
        if switching {
            steps.push(AccountMenuStep::SwitchProgress {
                session: session.clone(),
                progress,
            });
        }
        steps.push(AccountMenuStep::Request(AccountsRequest {
            target: match &session.machine {
                MachineId::Local => AccountsTarget::Local,
                MachineId::Remote(machine) => AccountsTarget::Remote(machine.clone()),
            },
            params: Value::Object(params),
            context: RequestContext::Session(SessionPending {
                generation: self.generation,
                command,
                session,
                switching,
            }),
        }));
        steps
    }

    /// The answer to a request this state issued. `working` is whether the row reads as working
    /// NOW. Returns the steps and whether the menu dropped the answer as overtaken (the memory
    /// and the progress clear happen either way).
    pub(crate) fn answer(
        &mut self,
        pending: SessionPending,
        result: Result<AccountsState, String>,
        working: bool,
        hide_account_emails: bool,
    ) -> (Vec<AccountMenuStep>, bool) {
        let session_id = pending.command.session_id.clone();
        if let Ok(data) = &result {
            self.last_answer.insert(session_id.clone(), data.clone());
        }
        let mut steps = Vec::new();
        if pending.switching {
            steps.push(AccountMenuStep::SwitchProgress {
                session: pending.session.clone(),
                progress: None,
            });
        }
        if pending.generation != self.generation {
            return (steps, true);
        }
        let items = match &result {
            Ok(_) if pending.command.action == "select" => {
                steps.push(AccountMenuStep::Publish {
                    owner_id: session_id,
                    items: Vec::new(),
                    close: true,
                });
                return (steps, false);
            }
            Ok(data) => session_account_page(&session_id, data, working, hide_account_emails),
            Err(error) => session_error_page(&session_id, error, hide_account_emails),
        };
        steps.push(AccountMenuStep::Publish {
            owner_id: session_id,
            items,
            close: false,
        });
        (steps, false)
    }

    /// How many sessions the transport remembers an answer for.
    pub fn remembered_sessions(&self) -> usize {
        self.last_answer.len()
    }
}
