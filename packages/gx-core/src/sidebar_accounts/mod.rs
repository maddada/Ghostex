//! The sidebar's two account pages: the agent launcher's (`agentAccounts`) and a session row's
//! Switch Account flyout (`sessionAccounts`).
//!
//! CDXC:AgentProviders 2026-09-21 WHY:
//! These were the last menu pages only the sidebar page could answer, because they fetch an
//! account list and the answer arrives later. The pages, the closure state behind them (the
//! launcher's cached list and both generations) and the per-session account-switch memory are
//! here as data and transitions; the host performs the calls (`Request`), applies the pages to the
//! panel the menu already opened for them (`Publish`), starts a launch (`Launch`) and shows the
//! switch progress (`SwitchProgress`), and hands each answer back with the context its request
//! carried. Nothing here reads a clock or performs I/O.
//!
//! SEE-ALSO: apps/desktop/src/app/gx_store/sidebar_accounts.rs (the host). The parity gate,
//! `tooling/gx-core/account-menu-parity.ts`, was deleted with the TypeScript.

mod data;
mod items;
mod launcher;
mod rpc;
mod session;
mod usage;

use serde_json::Value;

use crate::keys::SessionKey;
use crate::sidebar_menu::{MenuHost, MenuItem};
use crate::sidebar_view::SidebarView;

pub use data::{
    AccountSession, AccountUsageWindow, AccountsState, AgentAccount, ResetCredits,
    INVALID_ACCOUNTS_ANSWER,
};
pub use launcher::{LauncherAccounts, LauncherCommand};
pub use rpc::{
    agent_accounts_http_answer, group_accounts_target, session_accounts_target, AccountsTarget,
    AGENT_ACCOUNTS_PATH, SESSION_COMPUTER_UNAVAILABLE,
};
pub use session::{SessionAccounts, SessionAccountsCommand};
pub use usage::{
    account_headline_windows, account_usage_detail, account_usage_label, is_five_hour_window,
    is_weekly_window, js_round, mask_account_text,
};

/// One thing the host does for an account page.
#[derive(Clone, Debug, PartialEq)]
pub enum AccountMenuStep {
    /// `{ kind: 'menu', ownerId, items, close }`: replace the panel this owner opened, or close
    /// the menu. The host applies it only while that owner's panel is still on screen.
    Publish {
        owner_id: String,
        items: Vec<MenuItem>,
        close: bool,
    },
    /// One `/api/agentAccounts` call. Its answer goes back through [`SidebarAccountMenus::answer`]
    /// with this request, whatever it is.
    Request(AccountsRequest),
    /// `runNativeProjectAction({ type: 'projectAction', action: 'agent', groupId, agentId,
    /// accountId })`: the store's own launcher run (`plan_agent_run`).
    Launch {
        group_id: String,
        agent_id: String,
        account_id: Option<String>,
    },
    /// `accountSwitchProgress` for this session: the account being switched to, or `None` once
    /// the pick has landed.
    SwitchProgress {
        session: SessionKey,
        progress: Option<Value>,
    },
}

/// An account call and what its answer needs to be applied.
#[derive(Clone, Debug, PartialEq)]
pub struct AccountsRequest {
    pub target: AccountsTarget,
    /// The request body, `{ operation, ... }`.
    pub params: Value,
    context: RequestContext,
}

impl AccountsRequest {
    /// The row a flyout request is for, which the host reads the working state of when the answer
    /// arrives. `None` for a launcher request.
    pub fn session_id(&self) -> Option<&str> {
        match &self.context {
            RequestContext::Session(pending) => Some(pending.command.session_id.as_str()),
            RequestContext::Launcher(_) => None,
        }
    }

    /// Which page asked: `agentAccounts` or `sessionAccounts`.
    pub fn page(&self) -> &'static str {
        match &self.context {
            RequestContext::Launcher(_) => "agentAccounts",
            RequestContext::Session(_) => "sessionAccounts",
        }
    }
}

#[derive(Clone, Debug, PartialEq)]
enum RequestContext {
    Launcher(launcher::LauncherPending),
    Session(session::SessionPending),
}

/// What the host knows when it builds a page.
pub struct AccountMenuHost<'a> {
    /// The agents and the primary agent the launcher lists.
    pub menu: &'a MenuHost,
    /// Settings' Hide account emails.
    pub hide_account_emails: bool,
    /// Whether the flyout's row is working, read when its answer arrives. Ignored by the launcher.
    pub session_working: bool,
}

/// What an answer did.
#[derive(Clone, Debug, PartialEq)]
pub struct AccountAnswer {
    pub steps: Vec<AccountMenuStep>,
    /// A newer command on the same page overtook the request, so its page was not published.
    pub overtaken: bool,
}

/// Both pages' state, one per app run.
#[derive(Clone, Debug, Default)]
pub struct SidebarAccountMenus {
    pub launcher: LauncherAccounts,
    pub sessions: SessionAccounts,
}

impl SidebarAccountMenus {
    /// Whether this renderer command is one of the two pages', without reading anything.
    pub fn owns_command(command: &Value) -> bool {
        matches!(
            command.get("type").and_then(Value::as_str),
            Some("agentAccounts" | "sessionAccounts")
        )
    }

    /// The steps for a command. `None` for a command of the two types that names no group or
    /// session, or no action.
    pub fn command(
        &mut self,
        command: &Value,
        host: &AccountMenuHost<'_>,
    ) -> Option<Vec<AccountMenuStep>> {
        if let Some(command) = LauncherCommand::from_json(command) {
            return Some(
                self.launcher
                    .command(command, host.menu, host.hide_account_emails),
            );
        }
        let command = SessionAccountsCommand::from_json(command)?;
        Some(self.sessions.command(command, host.hide_account_emails))
    }

    /// Applies the answer to `request`: the daemon's result (see [`agent_accounts_http_answer`]
    /// for a local call) or the text of its failure.
    pub fn answer(
        &mut self,
        request: AccountsRequest,
        result: Result<Value, String>,
        host: &AccountMenuHost<'_>,
    ) -> AccountAnswer {
        let result = result.and_then(|value| AccountsState::from_json(&value));
        match request.context {
            RequestContext::Launcher(pending) => {
                match self
                    .launcher
                    .answer(pending, result, host.menu, host.hide_account_emails)
                {
                    Some(steps) => AccountAnswer {
                        steps,
                        overtaken: false,
                    },
                    None => AccountAnswer {
                        steps: Vec::new(),
                        overtaken: true,
                    },
                }
            }
            RequestContext::Session(pending) => {
                let (steps, overtaken) = self.sessions.answer(
                    pending,
                    result,
                    host.session_working,
                    host.hide_account_emails,
                );
                AccountAnswer { steps, overtaken }
            }
        }
    }
}

/// `sidebarStore.getState().sessionsById[sessionId]?.activity === 'working'`, read from the drawn
/// list: the flyout opens on a drawn row.
pub fn account_session_working(view: &SidebarView, session_id: &str) -> bool {
    view.groups
        .iter()
        .flat_map(|group| group.core.sessions.iter())
        .find(|session| session.row.sidebar_session_id == session_id)
        .is_some_and(|session| session.row.activity == "working")
}
