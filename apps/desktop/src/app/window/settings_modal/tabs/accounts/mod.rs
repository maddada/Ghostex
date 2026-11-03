//! The Accounts page (settings-modal/tabs/accounts.tsx (deleted 2026-10-01) wrapping accounts/manager.tsx (deleted 2026-10-01)): the Accounts
//! section (Refresh accounts, Hide emails, the read state), then the account manager: a list of
//! each provider's saved accounts with its rules on the left (`list.rs`), the selected account, the
//! provider's rules or the Add account setup on the right (`editor.rs`, `rules.rs`, `manager.rs`),
//! the sign-in flows (`connect.rs`), Claude Swap / Codex Swap as quiet footer lines
//! (`helper_tools.rs`), and the Connect your accounts guide.
//!
//! CDXC:Settings 2026-09-28 WHY:
//! The page reads and changes accounts through the Settings host's gxserver call
//! (`/api/agentAccounts`) instead of the React page's `fetch` with the injected bootstrap, so it
//! works without CEF. Only this computer is offered, as the desktop's React page did
//! (`getAccountsConnections()` had one connection there).
mod claude_ai_wait;
pub(crate) mod client;
mod connect;
pub(crate) mod data;
mod editor;
mod guide;
mod helper_tools;
mod list;
mod manager;
mod preview;
mod remembered_form;
mod rules;
mod sign_in_watch;
pub(crate) mod widgets;

use super::super::fields::{
    FieldStates, RowSpec, SettingsPage, SizedButtonSize, SizedButtonVariant,
};
use super::super::fields::{
    settings_list_item, settings_section, settings_sized_button, toggle_field_with,
};
use super::super::model::SettingsTabId;
use super::super::page::{PageBlock, settings_page};
use super::super::rail::{rail_pages, render_no_matches};
use super::super::search::should_show_section;
use super::super::store::SettingsStore;
use client::AccountsClient;
use connect::FlowState;
use editor::EditorDraft;
use gpui::{
    AnyElement, AnyView, App, AppContext as _, Context, Entity, FocusHandle, IntoElement,
    ParentElement as _, Render, SharedString, Task, Window, div,
};
use helper_tools::HelperToolsState;
use manager::SetupDraft;
use serde_json::{Value, json};
use std::collections::{HashMap, HashSet};
use std::time::Duration;

/// Creates the Accounts page view.
///
/// CDXC:Settings 2026-09-09 DECISION:
/// User: account management has its own Accounts page in Settings, replacing the Accounts section under Agents. Claude uses cswap and Codex uses xswap.
pub(crate) fn accounts_tab_view(
    store: &Entity<SettingsStore>,
    window: &mut Window,
    cx: &mut App,
) -> AnyView {
    cx.new(|cx| AccountsTab::new(store.clone(), window, cx))
        .into()
}

/// `setInterval(poll, 2000)` of the manager's setup watch.
const SETUP_POLL: Duration = Duration::from_secs(2);

pub(crate) struct AccountsTab {
    pub(crate) store: Entity<SettingsStore>,
    fields: FieldStates,
    pub(crate) client: Entity<AccountsClient>,
    /// The provider whose Connection guide is open.
    pub(crate) guide: Option<String>,
    pub(crate) guide_focus: FocusHandle,
    /// What the right-hand pane shows; `None` until the user picks, which shows the first account.
    pub(crate) pane: Option<Pane>,
    /// The account whose sign-in just finished: its pane says Connected for a few seconds.
    pub(crate) connected: Option<(String, Task<()>)>,
    /// The sign-in in progress (`pendingJob`).
    pub(crate) pending_job: Option<Value>,
    completed_job: String,
    setup_poll: Option<Task<()>>,
    pub(crate) helpers: HelperToolsState,
    pub(crate) editors: HashMap<String, EditorDraft>,
    pub(crate) setups: HashMap<String, SetupDraft>,
    pub(crate) flows: HashMap<String, FlowState>,
    /// The command whose copy button shows its check (`CopyCommand`'s `copied`).
    pub(crate) copied: Option<(String, Task<()>)>,
    was_active: bool,
    preview_applied: bool,
    /// The inputs masked as passwords (`sync_masked`).
    pub(crate) masked_inputs: HashMap<SharedString, bool>,
    /// The editor inputs that save when they lose focus or take Enter (`commit_on_blur`).
    pub(crate) commit_inputs: HashSet<SharedString>,
    /// A Sign in to claude.ai is under way and the page re-reads it (`claude_ai_wait`).
    claude_ai_polling: bool,
    /// The account a Reconnect outside the page asked to sign in, until the accounts are read.
    pending_reconnect: Option<String>,
}

/// What the account manager's right-hand pane shows.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum Pane {
    /// A saved account (its id).
    Account(String),
    /// A provider's rules for new sessions and limits.
    Rules(&'static str),
    /// A provider's Add account setup.
    Add(&'static str),
}

impl super::HoldsUnsavedInput for AccountsTab {
    /// An Add account setup, a Remove confirmation or an Uninstall confirmation is open (account
    /// edits save as they are made).
    fn holds_unsaved_input(&self, _cx: &gpui::App) -> bool {
        matches!(self.pane, Some(Pane::Add(_)))
            || self.editors.values().any(|draft| draft.remove)
            || self.helpers.confirm_uninstall.is_some()
    }
}

impl SettingsPage for AccountsTab {
    fn settings_store(&self) -> &Entity<SettingsStore> {
        &self.store
    }

    fn field_states(&mut self) -> &mut FieldStates {
        &mut self.fields
    }
}

impl AccountsTab {
    pub(crate) fn new(
        store: Entity<SettingsStore>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Self {
        // The page view outlives a switch to another page, which is when it closes and reopens.
        cx.observe(&store, |page, _, cx| {
            page.sync_active(cx);
            cx.notify();
        })
        .detach();
        // CDXC:Settings 2026-09-08 DECISION: Refresh accounts every time the Accounts page opens and show loading on the Refresh accounts button itself.
        let client = cx.new(|cx| AccountsClient::new(store.clone(), true, cx));
        cx.observe(&client, |_, _, cx| cx.notify()).detach();
        // Settings closing drops the page with a sign-in still running in the browser.
        cx.on_release(|page: &mut Self, cx| page.hand_off_sign_ins(cx))
            .detach();
        cx.observe_window_activation(window, |page: &mut Self, window, cx| {
            if window.is_window_active() {
                page.client
                    .update(cx, |client, cx| client.refresh_on_focus(cx));
            }
        })
        .detach();
        let mut page = Self {
            store,
            fields: FieldStates::default(),
            client,
            guide: None,
            guide_focus: cx.focus_handle(),
            pane: None,
            connected: None,
            pending_job: None,
            completed_job: String::new(),
            setup_poll: None,
            helpers: HelperToolsState::default(),
            editors: HashMap::new(),
            setups: HashMap::new(),
            flows: HashMap::new(),
            copied: None,
            was_active: false,
            preview_applied: false,
            masked_inputs: HashMap::new(),
            commit_inputs: HashSet::new(),
            claude_ai_polling: false,
            pending_reconnect: None,
        };
        page.restore_open_form(cx);
        // An account toast's Open Accounts opens on that agent's accounts and rules.
        if let Some(provider) = page
            .store
            .read(cx)
            .request()
            .initial_accounts_provider
            .as_deref()
            .and_then(|provider| data::PROVIDERS.into_iter().find(|known| *known == provider))
        {
            page.pane = Some(Pane::Rules(provider));
        }
        // A Reconnect in the usage panel or the chat's Accounts & limits panel opens here on that
        // account with its sign-in started.
        if let Some(account_id) = page
            .store
            .read(cx)
            .request()
            .initial_accounts_reconnect
            .clone()
        {
            page.pane = Some(Pane::Account(account_id.clone()));
            page.pending_reconnect = Some(account_id);
        }
        // Back to Settings after a claude.ai sign-in returns to that account.
        if let Some(account_id) = claude_ai_wait::take_reopen_account() {
            page.pane = Some(Pane::Account(account_id));
        }
        page
    }

    /// Once the accounts are read, starts the sign-in a Reconnect outside the page asked for. An
    /// account that needs none (signed in again meanwhile) or is gone just shows.
    fn start_pending_reconnect(&mut self, cx: &mut Context<Self>) {
        let Some(id) = self.pending_reconnect.as_ref() else {
            return;
        };
        let Some(data) = self.client.read(cx).data.as_ref() else {
            return;
        };
        let account = data
            .accounts()
            .into_iter()
            .find(|account| account.id() == *id && account.needs_sign_in());
        self.pending_reconnect = None;
        if let Some(account) = account {
            self.start_reconnect(&account, cx);
        }
    }

    pub(crate) fn hide_emails(&self, cx: &App) -> bool {
        self.store.read(cx).bool("hideAccountEmails")
    }

    /// The page opened (read the accounts, watch sign-ins, read the helper tools) or closed.
    fn sync_active(&mut self, cx: &mut Context<Self>) {
        let active = self.store.read(cx).active_tab() == SettingsTabId::Accounts;
        if active == self.was_active {
            return;
        }
        self.was_active = active;
        self.client
            .update(cx, |client, cx| client.set_active(active, cx));
        if !active {
            self.hand_off_sign_ins(cx);
            self.setup_poll = None;
            self.helpers.poll = None;
            for flow in self.flows.values_mut() {
                flow.poll = None;
            }
            return;
        }
        sign_in_watch::stop();
        if !self.client.read(cx).connected(cx) {
            return;
        }
        self.poll_setup_jobs(cx);
        self.setup_poll = Some(cx.spawn(async move |this, cx| {
            loop {
                cx.background_executor().timer(SETUP_POLL).await;
                if this
                    .update(cx, |page, cx| page.poll_setup_jobs(cx))
                    .is_err()
                {
                    break;
                }
            }
        }));
        self.load_helper_tools(false, false, cx);
    }

    /// The manager's watch: the sign-in in progress, and a finished one highlights and opens its
    /// account.
    ///
    /// CDXC:AgentProviders 2026-09-08 DECISION:
    /// Finishing login reopens Settings at Accounts, even if the user left Settings while the browser was open. The account is already registered before this completion is announced.
    /// This watch runs while the Accounts page is open; a sign-in still running when the page closes is watched by `sign_in_watch`, which reopens Settings at Accounts when it completes.
    fn poll_setup_jobs(&mut self, cx: &mut Context<Self>) {
        let this = cx.weak_entity();
        let params = json!({ "operation": "setupStatus", "owner": client::ACCOUNT_SETUP_OWNER });
        client::AccountsClient::call(
            &self.client.clone(),
            params,
            move |result, cx| {
                let Ok(state) = result else {
                    return;
                };
                let jobs: Vec<Value> = state["setupJobs"].as_array().cloned().unwrap_or_default();
                let _ = this.update(cx, |page, cx| {
                    page.pending_job = jobs
                        .iter()
                        .filter(|job| {
                            !matches!(job["status"].as_str(), Some("complete" | "failed"))
                        })
                        .next_back()
                        .cloned();
                    let complete = jobs
                        .iter()
                        .rfind(|job| job["status"] == "complete")
                        .cloned();
                    if let Some(complete) = complete
                        && let Some(account_id) =
                            complete["accountId"].as_str().filter(|id| !id.is_empty())
                        && complete["id"].as_str() != Some(page.completed_job.as_str())
                    {
                        page.completed_job =
                            complete["id"].as_str().unwrap_or_default().to_string();
                        page.guide = None;
                        page.select_pane(Pane::Account(account_id.to_string()), cx);
                        page.mark_connected(account_id.to_string(), cx);
                    }
                    cx.notify();
                });
            },
            cx,
        );
    }

    /// The page is leaving with sign-ins still running: watch them from outside the page
    /// (`sign_in_watch`).
    fn hand_off_sign_ins(&self, cx: &mut App) {
        let mut job_ids: Vec<String> = self
            .flows
            .values()
            .filter_map(|flow| flow.job.as_ref())
            .chain(self.pending_job.as_ref())
            .filter(|job| !matches!(job["status"].as_str(), Some("complete" | "failed")))
            .filter_map(|job| job["id"].as_str().map(str::to_string))
            .collect();
        job_ids.sort();
        job_ids.dedup();
        if !job_ids.is_empty() && self.client.read(cx).connected(cx) {
            sign_in_watch::watch(self.store.read(cx).host(), job_ids, cx);
        }
    }

    /// `request(params)` of the manager: a mutation whose answer is the new state.
    pub(crate) fn account_request(
        &mut self,
        params: Value,
        done: Option<Box<dyn FnOnce(bool, &mut App)>>,
        cx: &mut Context<Self>,
    ) {
        self.client
            .update(cx, |client, cx| client.request(params, done, cx));
    }

    /// `showAccountFlowToast`.
    pub(crate) fn flow_toast(&self, title: &str, description: &str, cx: &mut App) {
        self.store
            .update(cx, |store, cx| store.toast("info", title, description, cx));
    }

    /// The preview binary's state: the rows and dialogs no open message reaches.
    fn apply_preview_state(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.preview_applied {
            return;
        }
        let Some(state) = self.store.read(cx).request().preview_state.clone() else {
            self.preview_applied = true;
            return;
        };
        if self.client.read(cx).data.is_none()
            && !matches!(state.as_str(), "accounts-error" | "accounts-loading")
        {
            return;
        }
        self.preview_applied = true;
        self.apply_preview_pane(&state, window, cx);
        cx.notify();
    }

    /// Shows `pane`. Leaving an account saves what its focused field still holds and forgets its
    /// draft, so returning starts fresh; a finished sign-in's flow is dropped too.
    pub(crate) fn select_pane(&mut self, pane: Pane, cx: &mut Context<Self>) {
        if self.pane.as_ref() == Some(&pane) {
            return;
        }
        if let Some(Pane::Account(id)) = self.pane.clone() {
            self.commit_editor(&id, cx);
            self.editors.remove(&id);
            let key = format!("reconnect:{id}");
            if self
                .flows
                .get(&key)
                .and_then(|flow| flow.job.as_ref())
                .is_none_or(|job| !connect::job_active(job))
                && self.flows.get(&key).is_none_or(|flow| !flow.starting)
            {
                self.flows.remove(&key);
            }
        }
        if let Pane::Add(provider) = &pane {
            // CDXC:AgentProviders 2026-10-04 WHY: the setup flow is keyed per provider and keeps its finished job, so without this reset a second Add account showed the first add's "Account connected" message instead of the sign-in button.
            self.reset_setup_flows(provider);
            self.setups.remove(*provider);
        }
        self.pane = Some(pane);
        cx.notify();
    }

    /// The account's pane says Connected for six seconds and the list is read again.
    pub(crate) fn mark_connected(&mut self, account_id: String, cx: &mut Context<Self>) {
        let id = account_id.clone();
        let clear = cx.spawn(async move |this, cx| {
            cx.background_executor().timer(Duration::from_secs(6)).await;
            let _ = this.update(cx, |page, cx| {
                if page
                    .connected
                    .as_ref()
                    .is_some_and(|(shown, _)| *shown == id)
                {
                    page.connected = None;
                    page.flows.remove(&format!("reconnect:{id}"));
                    cx.notify();
                }
            });
        });
        let (client, store) = (self.client.clone(), self.store.clone());
        let added = account_id.clone();
        self.connected = Some((account_id, clear));
        self.client.update(cx, |client_state, cx| {
            client_state.request(
                json!({ "operation": "list", "refresh": true }),
                Some(Box::new(move |ok, cx| {
                    if ok {
                        show_added_account_in_sidebar(&client, &store, |a| a.id() == added, cx);
                    }
                })),
                cx,
            )
        });
        cx.notify();
    }

    /// The pane shown: the chosen one while it still exists, else the first saved account, else
    /// the first provider's Add account setup.
    pub(crate) fn effective_pane(&self, data: &data::AccountsState) -> Pane {
        let exists = |pane: &Pane| match pane {
            Pane::Account(id) => data
                .accounts()
                .iter()
                .any(|account| account.id() == *id && account.registered()),
            Pane::Rules(_) | Pane::Add(_) => true,
        };
        if let Some(pane) = self.pane.clone().filter(exists) {
            return pane;
        }
        data::PROVIDERS
            .iter()
            .find_map(|provider| {
                data.registered(provider)
                    .first()
                    .map(|account| account.id())
            })
            .map(Pane::Account)
            .unwrap_or(Pane::Add("claude"))
    }

    /// The Accounts section.
    ///
    /// CDXC:Settings 2026-09-20 DECISION:
    /// User: recommend adding even a single account so usage stats are easy to find in the sidebar usage strip and the status lines. This is the 2026-09-09 decision with the meters' new home named; the titlebar row they used to sit in is gone.
    fn render_accounts_section(&mut self, cx: &mut Context<Self>) -> AnyElement {
        let p = self.store.read(cx).palette();
        let hide = self.hide_emails(cx);
        let (connected, data, error, busy, refreshing) = {
            let client = self.client.read(cx);
            (
                client.connected(cx),
                client.data.is_some(),
                client.error.clone(),
                client.busy,
                client.refreshing,
            )
        };
        let refresh = connected.then(|| {
            settings_sized_button(
                &p,
                "accounts-refresh",
                if refreshing {
                    "Refreshing…"
                } else {
                    "Refresh accounts"
                },
                Some("modals/settings/refresh.svg"),
                None,
                SizedButtonVariant::Outline,
                SizedButtonSize::Default,
                busy || refreshing,
                Some("Accounts are being read.".into()),
                |page: &mut Self, _window, cx| {
                    let this = cx.weak_entity();
                    page.account_request(
                        json!({ "operation": "list", "refresh": true }),
                        Some(Box::new(move |ok, cx| {
                            if ok {
                                let _ = this.update(cx, |page, cx| {
                                    page.flow_toast(
                                        "Accounts refreshed",
                                        "Saved accounts and usage are up to date.",
                                        cx,
                                    )
                                });
                            }
                        })),
                        cx,
                    );
                },
                cx,
            )
        });
        let mut rows: Vec<AnyElement> = vec![toggle_field_with(
            &p,
            "hide-account-emails",
            RowSpec::new("Hide emails").description(
                "Show only the first and last address characters and obscure the domain.",
            ),
            hide,
            None,
            |page: &mut Self, next, _window, cx| {
                let store = page.store.clone();
                store.update(cx, |store, cx| {
                    store.update_setting("hideAccountEmails", json!(next), cx)
                });
            },
            cx,
        )];
        if !connected {
            rows.push(settings_list_item(
                &p,
                None,
                None,
                "No computer connected",
                Some(
                    div()
                        .child("Connect to a computer to manage its accounts.")
                        .into_any_element(),
                ),
                None,
            ));
        }
        if connected && !error.is_empty() {
            let retry = settings_sized_button(
                &p,
                "accounts-try-again",
                "Try again",
                None,
                None,
                SizedButtonVariant::Outline,
                SizedButtonSize::Sm,
                false,
                None,
                |page: &mut Self, _window, cx| {
                    page.account_request(json!({ "operation": "list", "refresh": true }), None, cx)
                },
                cx,
            );
            rows.push(settings_list_item(
                &p,
                Some(super::super::fields::ListItemStatus::Warning),
                None,
                "Accounts could not be read",
                Some(
                    div()
                        .child(widgets::account_text(&error, hide))
                        .into_any_element(),
                ),
                Some(retry),
            ));
        }
        if connected && !data && error.is_empty() {
            rows.push(settings_list_item(
                &p,
                None,
                None,
                if busy {
                    "Reading saved accounts and usage…"
                } else {
                    "Account information is unavailable."
                },
                None,
                None,
            ));
        }
        settings_section(
            &p,
            "Accounts",
            Some("Add your account to see usage and reset times in Ghostex, even if you only use one account. Turn on Show usage in the sidebar for an account to keep its stats there.".into()),
            refresh,
            rows,
        )
        .map(IntoElement::into_any_element)
        .unwrap_or_else(|| div().into_any_element())
    }
}

impl Render for AccountsTab {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        self.sync_active(cx);
        self.apply_preview_state(window, cx);
        self.start_pending_reconnect(cx);
        let (p, search, searching) = {
            let store = self.store.read(cx);
            (
                store.palette(),
                store.tab_search(SettingsTabId::Accounts),
                store.is_searching(),
            )
        };
        let mut blocks: Vec<PageBlock> = Vec::new();
        if !should_show_section(&search.section("accounts"), None) {
            if searching {
                let store = self.store.clone();
                let matching: Vec<SettingsTabId> = rail_pages(self.store.read(cx))
                    .into_iter()
                    .map(|page| page.tab)
                    .collect();
                blocks.push(PageBlock::plain(render_no_matches(
                    &p,
                    SettingsTabId::Accounts,
                    &matching,
                    move |tab, _, cx| store.update(cx, |store, cx| store.set_active_tab(tab, cx)),
                )));
            }
            return settings_page(&self.store, SettingsTabId::Accounts, &p, blocks, cx);
        }
        blocks.push(PageBlock::section(
            "accounts",
            self.render_accounts_section(cx),
        ));
        if self.client.read(cx).data.is_some() {
            let manager = self.render_manager(&p, window, cx);
            blocks.push(PageBlock::section("accounts-manager", manager));
        }
        if let Some(guide) = self.render_guide(&p, window, cx) {
            blocks.push(PageBlock::plain(guide));
        }
        settings_page(&self.store, SettingsTabId::Accounts, &p, blocks, cx)
    }
}

/// Hands an account just added to the sidebar's usage strip, which otherwise picks it up on its
/// next 30-second read. gxserver saves a new account as shown there (CDXC:AgentProviders 2026-11-03
/// on `new_saved_account` in server/src/accounts/endpoint.rs); one the list reports as hidden is
/// left alone.
pub(crate) fn show_added_account_in_sidebar(
    client: &Entity<AccountsClient>,
    store: &Entity<SettingsStore>,
    is_added: impl Fn(&data::Account) -> bool,
    cx: &mut App,
) {
    let Some(account) = client
        .read(cx)
        .data
        .as_ref()
        .and_then(|data| data.accounts().into_iter().find(|a| is_added(a)))
    else {
        return;
    };
    if !account.show_in_titlebar() {
        return;
    }
    super::super::store::store_host_message(
        store,
        json!({ "type": "accountTitlebarChanged", "machineId": "local", "account": account.raw }),
        cx,
    );
}
