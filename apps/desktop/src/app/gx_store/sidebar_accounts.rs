//! The two account pages, answered by the store: the agent launcher's (`agentAccounts`) and a
//! session row's Switch Account flyout (`sessionAccounts`).
//!
//! CDXC:AgentProviders 2026-09-21 WHY:
//! The pages, their cached list, their generations and the per-session switch memory are gx-core's
//! (`sidebar_accounts/`); this file performs what it asks for, each through the function the old
//! path ENDED in, named here because this port has twice called an app function that was not
//! that one:
//!
//! - A page (`publish({ kind: 'menu', ownerId, items, close })`): `postNativeSidebarSnapshot`
//!   reached `NativeSidebarUpdate::Menu` in `native_sidebar/state.rs`, whose body is now
//!   [`GhostexGpuiApp::apply_native_sidebar_menu_page`] and is called by both.
//! - A local call: the runtime's `GpuiGxserverClient.rpc('/api/agentAccounts')`, which is the
//!   typed-operation POST (`gxserver_post_typed_operation`) with the envelope read the way the
//!   client read it (gx-core `agent_accounts_http_answer`, so the daemon's own error sentence is
//!   what the page shows). A remote call: `requestRemoteGxserver`, which ended in
//!   `start_gpui_remote_sidebar_rpc` (`remote_conn/sidebar_rpc.rs`), the allowlist and all, and
//!   the answer shaped by `gpui_remote_sidebar_response_payload` as the bridge shapes it.
//! - A launch: `runNativeProjectAction({ projectAction: agent, accountId })`, the store's own
//!   launcher run since A-state (`plan_agent_run`, then `dispatch_gpui_sidebar_host_message`).
//!   The mounting placeholder the old dispatcher staged for an `agentAccounts: launch` command
//!   before handing it on is staged here, at the click, as it was; the run itself stages nothing.
//! - The switch progress: the native host's `accountSwitchProgress` arm
//!   (`remote_conn/app_modal_bridge.rs`), whose whole body is `set_session_account_switch_progress`.
//!
//! **The counters that prove this path fires in the app** are the `accounts` object on the
//! periodic `gxStore.sidebarActions.summary`, whose first line is written at zero. Account names,
//! emails and usage never reach a log line: only counts do.
//!
//! SEE-ALSO: packages/gx-core/src/sidebar_accounts/, apps/desktop/src/app/native_sidebar/menus.rs
//! (the panel a flyout or the launcher opens).

use std::time::Duration;

use ghostex_gx_core::{
    AGENT_ACCOUNTS_PATH, AccountMenuHost, AccountMenuStep, AccountsRequest, AccountsTarget,
    MenuItem, SessionKey, SidebarAccountMenus, account_session_working, agent_accounts_http_answer,
    plan_agent_run,
};
use serde_json::{Value, json};

use crate::GhostexGpuiApp;
use crate::app::helpers::{
    GpuiRemoteAttachSessionKey, GpuiWorkspaceTerminalSessionKey, gpui_normalize_remote_machine_id,
    gpui_remote_sidebar_response_payload, gxserver_post_typed_operation,
};
use crate::app::model::GpuiLocalWorkspaceSessionKey;
use crate::app::remote_conn::sidebar_rpc::{
    GPUI_REMOTE_GXSERVER_REQUEST_FAILED, GpuiRemoteSidebarRpcMode,
};

/// The runtime's local call had no timeout of its own; a refresh reads every account's usage, so
/// this is the titlebar usage read's bound (titlebar/account_usage.rs). It bounds each socket read
/// and write, not the whole call (declared difference 44).
const LOCAL_ACCOUNTS_TIMEOUT: Duration = Duration::from_secs(60);
/// `requestRemoteGxserver`'s default.
const REMOTE_ACCOUNTS_TIMEOUT: Duration = Duration::from_secs(20);

/// The two pages' state and what this app run did with them.
#[derive(Default)]
pub(crate) struct SidebarAccountsHost {
    pub(super) menus: SidebarAccountMenus,
    pub(super) counters: SidebarAccountCounters,
}

/// Rides `gxStore.sidebarActions.summary` as `accounts`.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) struct SidebarAccountCounters {
    pub(crate) launcher_commands: u64,
    pub(crate) session_commands: u64,
    pub(crate) local_requests: u64,
    pub(crate) remote_requests: u64,
    /// Answers the daemon gave, and failures (including a refused or unreachable remote).
    pub(crate) answers: u64,
    pub(crate) failures: u64,
    /// Answers a newer command on the same page overtook: nothing was published from them.
    pub(crate) overtaken: u64,
    /// Pages that replaced the panel their owner opened, and menus a page closed.
    pub(crate) pages: u64,
    pub(crate) closes: u64,
    /// Pages whose panel was gone (the menu closed or another one opened) when they arrived.
    pub(crate) pages_without_panel: u64,
    pub(crate) launches: u64,
    /// A launch the store's run did not start: no sidebar page, or a group the list does not draw.
    pub(crate) launches_not_run: u64,
    pub(crate) switch_progress: u64,
    pub(crate) switch_progress_cleared: u64,
    /// Commands of the two types that named no group, session or action.
    pub(crate) malformed: u64,
    /// Commands that went to the old runtime because the store's list is not drawn.
    pub(crate) declined_source: u64,
}

impl GhostexGpuiApp {
    /// Answers `agentAccounts` and `sessionAccounts` when the store owns them. Returns whether it
    /// did, in which case the command must NOT also reach the old runtime, which would call the
    /// daemon a second time and publish its own page over this one.
    pub(crate) fn gx_store_run_sidebar_accounts(
        &mut self,
        command: &Value,
        cx: &mut gpui::Context<Self>,
    ) -> bool {
        if !SidebarAccountMenus::owns_command(command) {
            return false;
        }
        if !self.gx_store_sidebar_list_ready() {
            self.gx_store.sidebar_accounts.counters.declined_source += 1;
            return false;
        }
        let launcher = command["type"] == "agentAccounts";
        // What the old dispatcher did for this command before handing it on, and which it now
        // never reaches: the instant mounting tab of a launch (CDXC:AgentLauncher 2026-09-19
        // DECISION).
        if launcher && command["action"] == "launch" {
            self.stage_agent_launch_placeholder(command, cx);
        }
        let menu_host = self.gx_store_menu_host();
        let host = AccountMenuHost {
            menu: &menu_host,
            hide_account_emails: hide_account_emails(),
            session_working: false,
        };
        let Some(steps) = self.gx_store.sidebar_accounts.menus.command(command, &host) else {
            self.gx_store.sidebar_accounts.counters.malformed += 1;
            return false;
        };
        let counters = &mut self.gx_store.sidebar_accounts.counters;
        if launcher {
            counters.launcher_commands += 1;
        } else {
            counters.session_commands += 1;
        }
        self.run_sidebar_account_steps(steps, cx);
        true
    }

    fn run_sidebar_account_steps(
        &mut self,
        steps: Vec<AccountMenuStep>,
        cx: &mut gpui::Context<Self>,
    ) {
        for step in steps {
            match step {
                AccountMenuStep::Publish {
                    owner_id,
                    items,
                    close,
                } => {
                    let items = items.iter().map(MenuItem::to_json).collect();
                    let applied = self.apply_native_sidebar_menu_page(&owner_id, items, close, cx);
                    let counters = &mut self.gx_store.sidebar_accounts.counters;
                    match (applied, close) {
                        (false, _) => counters.pages_without_panel += 1,
                        (true, true) => counters.closes += 1,
                        (true, false) => counters.pages += 1,
                    }
                    // A page published at once (a cached count button, or Right on it) arrives
                    // through a caller that does not redraw, so the page asks for its own frame,
                    // as the `NativeSidebarUpdate::Menu` arm did.
                    if applied {
                        cx.notify();
                    }
                }
                AccountMenuStep::Request(request) => {
                    self.start_sidebar_accounts_request(request, cx)
                }
                AccountMenuStep::Launch {
                    group_id,
                    agent_id,
                    account_id,
                } => self.launch_from_sidebar_accounts(&group_id, &agent_id, account_id, cx),
                AccountMenuStep::SwitchProgress { session, progress } => {
                    let counters = &mut self.gx_store.sidebar_accounts.counters;
                    if progress.is_some() {
                        counters.switch_progress += 1;
                    } else {
                        counters.switch_progress_cleared += 1;
                    }
                    let progress = progress.unwrap_or(Value::Null);
                    self.set_session_account_switch_progress(
                        workspace_key(session),
                        &progress,
                        None,
                        cx,
                    );
                }
            }
        }
    }

    /// The call, on the background executor for a local daemon and down the machine's tunnel for
    /// a remote one; the answer comes back to [`Self::apply_sidebar_accounts_answer`].
    fn start_sidebar_accounts_request(
        &mut self,
        request: AccountsRequest,
        cx: &mut gpui::Context<Self>,
    ) {
        let params = request.params.clone();
        let task: gpui::Task<Result<Value, String>> = match &request.target {
            AccountsTarget::Local => {
                self.gx_store.sidebar_accounts.counters.local_requests += 1;
                let background = cx.background_executor().clone();
                background.spawn(async move {
                    let (status, body) = gxserver_post_typed_operation(
                        AGENT_ACCOUNTS_PATH,
                        &params,
                        LOCAL_ACCOUNTS_TIMEOUT,
                    )?;
                    agent_accounts_http_answer(status, &body)
                })
            }
            AccountsTarget::Remote(machine_id) => {
                self.gx_store.sidebar_accounts.counters.remote_requests += 1;
                // The bridge arm normalizes the id before the one shared function; a group or row
                // id whose machine does not normalize names no machine this app can reach.
                let Some(machine_id) = gpui_normalize_remote_machine_id(machine_id) else {
                    self.apply_sidebar_accounts_answer(
                        request,
                        Err(GPUI_REMOTE_GXSERVER_REQUEST_FAILED.to_string()),
                        cx,
                    );
                    return;
                };
                let task = self.start_gpui_remote_sidebar_rpc(
                    &machine_id,
                    AGENT_ACCOUNTS_PATH,
                    Some(params),
                    REMOTE_ACCOUNTS_TIMEOUT,
                    GpuiRemoteSidebarRpcMode::Awaited,
                    cx,
                );
                cx.background_executor().spawn(async move {
                    task.await.map(|result| {
                        gpui_remote_sidebar_response_payload(AGENT_ACCOUNTS_PATH, result)
                    })
                })
            }
        };
        cx.spawn(async move |this, cx| {
            let result = task.await;
            let _ = this.update(cx, |this, cx| {
                this.apply_sidebar_accounts_answer(request, result, cx);
            });
        })
        .detach();
    }

    fn apply_sidebar_accounts_answer(
        &mut self,
        request: AccountsRequest,
        result: Result<Value, String>,
        cx: &mut gpui::Context<Self>,
    ) {
        let counters = &mut self.gx_store.sidebar_accounts.counters;
        if result.is_ok() {
            counters.answers += 1;
        } else {
            counters.failures += 1;
        }
        // Read when the page is built, as the TypeScript read the store after its `await`.
        let session_working = request.session_id().is_some_and(|session_id| {
            account_session_working(self.gx_store.sidebar_list.view(), session_id)
        });
        let menu_host = self.gx_store_menu_host();
        let host = AccountMenuHost {
            menu: &menu_host,
            hide_account_emails: hide_account_emails(),
            session_working,
        };
        let answer = self
            .gx_store
            .sidebar_accounts
            .menus
            .answer(request, result, &host);
        if answer.overtaken {
            self.gx_store.sidebar_accounts.counters.overtaken += 1;
        }
        self.run_sidebar_account_steps(answer.steps, cx);
        cx.notify();
    }

    /// The launcher's run as a `projectAction: agent` row runs it, minus the placeholder, which
    /// the click staged.
    fn launch_from_sidebar_accounts(
        &mut self,
        group_id: &str,
        agent_id: &str,
        account_id: Option<String>,
        cx: &mut gpui::Context<Self>,
    ) {
        let mut command = json!({
            "type": "projectAction",
            "action": "agent",
            "groupId": group_id,
            "agentId": agent_id,
        });
        if let Some(account_id) = account_id {
            command["accountId"] = Value::String(account_id);
        }
        // The runtime writes the primary agent id on this launch, which the menus read.
        self.gx_store_note_menu_host_write(&command);
        let message =
            plan_agent_run(self.gx_store.sidebar_list.view(), &command).and_then(|plan| {
                plan.effects.into_iter().find_map(|effect| match effect {
                    ghostex_gx_core::ActionEffect::SidebarHostMessage { message } => Some(message),
                    _ => None,
                })
            });
        match message {
            Some(message) => {
                self.gx_store.sidebar_accounts.counters.launches += 1;
                self.dispatch_gpui_sidebar_host_message(message, cx);
            }
            _ => self.gx_store.sidebar_accounts.counters.launches_not_run += 1,
        }
    }

    /// `NativeSidebarUpdate::Menu`: replaces the items of the panel `owner_id` opened, or closes
    /// the menu, when that owner's panel is still the one on screen. Returns whether it was.
    pub(crate) fn apply_native_sidebar_menu_page(
        &mut self,
        owner_id: &str,
        items: Vec<Value>,
        close: bool,
        cx: &mut gpui::Context<Self>,
    ) -> bool {
        let Some(menu) = self.native_sidebar.menu.as_mut() else {
            return false;
        };
        let Some((owner, index)) = &menu.account_panel else {
            return false;
        };
        if owner != owner_id {
            return false;
        }
        if close {
            self.dismiss_native_sidebar_menu(cx);
        } else if let Some(panel) = menu.panels.get_mut(*index) {
            panel.replace_items(items);
        }
        true
    }
}

/// Settings' Hide account emails, as the titlebar and the New Thread picker read it.
fn hide_account_emails() -> bool {
    crate::shared_settings::shared_sidebar_settings_snapshot()
        .object()
        .get("hideAccountEmails")
        .and_then(Value::as_bool)
        == Some(true)
}

/// The key `accountSwitchProgress` builds from its `projectId`, `sessionId` and `machineId`.
fn workspace_key(session: SessionKey) -> GpuiWorkspaceTerminalSessionKey {
    match session.machine {
        ghostex_gx_core::MachineId::Remote(remote_machine_id) => {
            GpuiWorkspaceTerminalSessionKey::Remote(GpuiRemoteAttachSessionKey {
                remote_machine_id,
                project_id: session.project_id,
                session_id: session.session_id,
            })
        }
        ghostex_gx_core::MachineId::Local => {
            GpuiWorkspaceTerminalSessionKey::Local(GpuiLocalWorkspaceSessionKey {
                project_id: session.project_id,
                session_id: session.session_id,
            })
        }
    }
}

/// The counters as the summary carries them: 16 keys at depth 2.
pub(super) fn account_counters_json(counters: &SidebarAccountCounters) -> Value {
    json!({
        "launcherCommands": counters.launcher_commands,
        "sessionCommands": counters.session_commands,
        "localRequests": counters.local_requests,
        "remoteRequests": counters.remote_requests,
        "answers": counters.answers,
        "failures": counters.failures,
        "overtaken": counters.overtaken,
        "pages": counters.pages,
        "closes": counters.closes,
        "pagesWithoutPanel": counters.pages_without_panel,
        "launches": counters.launches,
        "launchesNotRun": counters.launches_not_run,
        "switchProgress": counters.switch_progress,
        "switchProgressCleared": counters.switch_progress_cleared,
        "malformed": counters.malformed,
        "declinedSource": counters.declined_source,
    })
}
