//! The rows of the two account pages, as data.
//!
//! Ported from the deleted sidebar page's `agent-launcher.ts` (the launcher's account page, frozen
//! in the since-deleted `tooling/gx-core/sidebar-page-frozen/`) and `accounts.ts` (a session's
//! Switch Account flyout).

use crate::sidebar_menu::{colored_agent_logo, LauncherAgent, MenuCommand, MenuItem};

use super::data::AccountsState;
use super::usage::{account_usage_detail, mask_account_text};

/// `format`: the account text as Settings wants it shown.
pub(crate) fn account_text(value: &str, hide_account_emails: bool) -> String {
    if hide_account_emails {
        mask_account_text(value)
    } else {
        value.to_string()
    }
}

fn logo(provider: Option<&str>) -> Option<String> {
    provider.and_then(colored_agent_logo).map(str::to_string)
}

fn disabled(label: String) -> MenuItem {
    MenuItem {
        label: Some(label),
        disabled: true,
        ..MenuItem::default()
    }
}

/// The page's first row, which returns to the agent list.
pub(crate) fn launcher_back(group_id: &str, agent: &LauncherAgent) -> MenuItem {
    MenuItem {
        label: Some(agent.name.clone()),
        icon: Some("chevron-left".to_string()),
        agent_launcher: true,
        keep_open: true,
        command: Some(MenuCommand::agent_accounts(group_id, "root", None)),
        ..MenuItem::default()
    }
}

/// The hint the launcher shows while the first list is read.
pub(crate) fn launcher_reading(back: MenuItem) -> Vec<MenuItem> {
    vec![back, disabled("Reading accounts…".to_string())]
}

/// The launcher's account page for one agent: its provider's registered accounts, then the
/// failure pair, then the empty-state block when a list was read and holds none.
pub(crate) fn launcher_account_page(
    group_id: &str,
    agent: &LauncherAgent,
    provider: &str,
    data: Option<&AccountsState>,
    error: Option<&str>,
    hide_account_emails: bool,
) -> Vec<MenuItem> {
    let mut items = vec![launcher_back(group_id, agent)];
    let accounts: Vec<_> = data
        .map(|data| data.registered_for(Some(provider)).collect())
        .unwrap_or_default();
    let default_id = data.and_then(|data| data.default_accounts.get(provider));
    for account in &accounts {
        items.push(MenuItem {
            label: Some(account_text(&account.name, hide_account_emails)),
            detail: Some(account_usage_detail(account, provider)),
            // `account.id === data?.defaultAccounts[provider]`.
            suffix: (account.id.as_ref() == default_id).then(|| "· Default".to_string()),
            agent_icon: Some(provider.to_string()),
            image_data_url: logo(Some(provider)),
            disabled: account.status.as_deref() != Some("ready"),
            command: Some(MenuCommand::agent_run_as(
                group_id,
                &agent.agent_id,
                account.id.as_deref(),
            )),
            ..MenuItem::default()
        });
    }
    if let Some(error) = error {
        items.push(disabled(account_text(error, hide_account_emails)));
        items.push(MenuItem {
            label: Some("Try Again".to_string()),
            keep_open: true,
            command: Some(MenuCommand::agent_accounts(
                group_id,
                "retry",
                Some(&agent.agent_id),
            )),
            ..MenuItem::default()
        });
    }
    if data.is_some() && accounts.is_empty() {
        items.push(MenuItem {
            label: Some("Current CLI Login".to_string()),
            agent_icon: Some(provider.to_string()),
            image_data_url: logo(Some(provider)),
            command: Some(MenuCommand::project_action(
                group_id,
                "agent",
                Some(&agent.agent_id),
            )),
            ..MenuItem::default()
        });
        items.push(disabled(
            "Uses your existing CLI sign-in. No account switcher needed.".to_string(),
        ));
        items.push(MenuItem::separator());
        items.push(disabled(
            "Add your account to see usage and reset times in Ghostex.".to_string(),
        ));
        items.push(MenuItem {
            label: Some("Add Account".to_string()),
            command: Some(MenuCommand::sidebar_action("accounts")),
            ..MenuItem::default()
        });
    }
    items
}

/// A session's Switch Account flyout from a read list: the accounts of the session's provider,
/// the current one ticked, none pickable while the session works.
pub(crate) fn session_account_page(
    session_id: &str,
    data: &AccountsState,
    working: bool,
    hide_account_emails: bool,
) -> Vec<MenuItem> {
    let provider = data
        .session
        .as_ref()
        .and_then(|session| session.provider.as_deref());
    let current = data.session_account_id();
    let mut items: Vec<MenuItem> = data
        .registered_for(provider)
        .map(|account| {
            let is_current = account.id.is_some() && account.id.as_deref() == current;
            MenuItem {
                label: Some(account_text(&account.name, hide_account_emails)),
                agent_icon: account.provider.clone(),
                image_data_url: logo(account.provider.as_deref()),
                checked: is_current,
                disabled: working || is_current || account.status.as_deref() != Some("ready"),
                keep_open: true,
                command: Some(MenuCommand::session_accounts(
                    session_id,
                    "select",
                    account.id.as_deref(),
                )),
                ..MenuItem::default()
            }
        })
        .collect();
    if items.is_empty() {
        items.push(disabled("No saved accounts.".to_string()));
    }
    items.extend(session_page_tail());
    items
}

/// The flyout after a failed read or pick: the reason, then Try Again.
pub(crate) fn session_error_page(
    session_id: &str,
    error: &str,
    hide_account_emails: bool,
) -> Vec<MenuItem> {
    let mut items = vec![
        disabled(account_text(error, hide_account_emails)),
        MenuItem {
            label: Some("Try Again".to_string()),
            keep_open: true,
            command: Some(MenuCommand::session_accounts(session_id, "retry", None)),
            ..MenuItem::default()
        },
    ];
    items.extend(session_page_tail());
    items
}

fn session_page_tail() -> [MenuItem; 2] {
    [
        MenuItem::separator(),
        MenuItem {
            label: Some("Manage Accounts".to_string()),
            icon: Some("settings".to_string()),
            command: Some(MenuCommand::sidebar_action("accounts")),
            ..MenuItem::default()
        },
    ]
}
