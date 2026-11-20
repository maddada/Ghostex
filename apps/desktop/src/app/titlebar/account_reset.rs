use crate::app::window::account_usage::AccountUsagePanel;
use crate::*;
use serde_json::{Value, json};
use std::time::Duration;

impl GhostexGpuiApp {
    /// CDXC:Notifications 2026-10-05 DECISION:
    /// User: a banked reset expiry warning has "an action to redeem now / open the usage panel". Opening the row (click, jump key, or its system notification) opens that account's usage panel, where its resets are used, anchored to the bell. An account without a titlebar usage button on this computer has no panel to open, so it opens Settings > Accounts instead.
    pub(crate) fn open_account_notification(
        &mut self,
        account_id: &str,
        cx: &mut gpui::Context<Self>,
    ) {
        let usage = self
            .titlebar_accounts
            .iter()
            .find(|a| a["id"].as_str() == Some(account_id) && a["titlebarMachine"] == "local")
            .and_then(|a| ExtensionId::new(a["titlebarKey"].as_str().unwrap_or("")));
        let anchor = self.titlebar_notification_bell_bounds.get();
        let (Some(id), Some(main), Some(anchor)) = (usage, self.main_window_handle, anchor) else {
            self.open_gpui_settings_accounts_page(None, cx);
            return;
        };
        let app = cx.weak_entity();
        cx.defer(move |cx| {
            let _ = main.update(cx, |_, window, cx| {
                let _ = app.update(cx, |app, cx| {
                    if !app.titlebar_popup_menu_open(GpuiTitlebarPopupKind::AccountUsage(id)) {
                        app.open_titlebar_account_usage(id, anchor, window, cx);
                    }
                });
            });
        });
    }

    /// A usage popup's "Don't use on <computer>" (`ignored`) and its Undo: tells `machine`'s
    /// gxserver (`setSignInIgnored`, `SavedAccount::sign_in_ignored`), which keeps it for every
    /// client reading that computer, the phone too. The card and the popup change at once; the
    /// next read brings that computer's own answer, so a computer too old to keep it shows the
    /// warning again.
    pub(crate) fn set_account_sign_in_ignored(
        &mut self,
        account_id: String,
        machine: String,
        ignored: bool,
        cx: &mut gpui::Context<Self>,
    ) {
        let target = if machine == "local" {
            Some(None)
        } else {
            self.remote_gxserver_connections
                .get(&machine)
                .map(|connection| Some(connection.request_target()))
        };
        let Some(target) = target else {
            return;
        };
        for account in self.titlebar_accounts.iter_mut().filter(|account| {
            account["id"].as_str() == Some(account_id.as_str())
                && account["titlebarMachine"].as_str().unwrap_or("local") == machine
        }) {
            account["signInIgnored"] = json!(ignored);
        }
        self.titlebar_accounts_revision = self.titlebar_accounts_revision.wrapping_add(1);
        self.sync_titlebar_account_privacy(cx);
        cx.notify();
        let params = json!({"operation": "setSignInIgnored", "id": account_id, "ignored": ignored});
        cx.spawn(async move |this, cx| {
            let _ = cx
                .background_executor()
                .spawn(async move {
                    let timeout = Duration::from_secs(30);
                    match target {
                        Some(target) => gpui_remote_gxserver_rpc_result(
                            &target,
                            "/api/agentAccounts",
                            &params,
                            timeout,
                        ),
                        None => gpui_gxserver_rpc_result("/api/agentAccounts", &params, timeout),
                    }
                })
                .await;
            let _ = this.update(cx, |this, cx| this.refresh_titlebar_accounts(cx));
        })
        .detach();
    }

    /// The usage panel's Refresh now: reads the account's usage again on the gxserver that owns it
    /// (`readUsageNow`), then refreshes every account button and tells the panel.
    pub(crate) fn refresh_account_usage_now(
        &mut self,
        titlebar_key: &str,
        panel: gpui::WeakEntity<AccountUsagePanel>,
        cx: &mut gpui::Context<Self>,
    ) {
        let account = self
            .titlebar_accounts
            .iter()
            .find(|a| a["titlebarKey"] == titlebar_key && a["registered"] == true)
            .cloned()
            .unwrap_or(Value::Null);
        let machine = account["titlebarMachine"].as_str().unwrap_or("local");
        let target = if machine == "local" {
            Some(None)
        } else {
            self.remote_gxserver_connections
                .get(machine)
                .map(|connection| Some(connection.request_target()))
        };
        let request = account["id"]
            .as_str()
            .zip(target)
            .map(|(id, target)| (target, json!({"operation":"readUsageNow","id":id})));
        cx.spawn(async move |this, cx| {
            if let Some((target, params)) = request {
                let _ = cx
                    .background_executor()
                    .spawn(async move {
                        let timeout = Duration::from_secs(120);
                        match target {
                            Some(target) => gpui_remote_gxserver_rpc_result(
                                &target,
                                "/api/agentAccounts",
                                &params,
                                timeout,
                            ),
                            None => {
                                gpui_gxserver_rpc_result("/api/agentAccounts", &params, timeout)
                            }
                        }
                    })
                    .await;
                let _ = this.update(cx, |this, cx| {
                    this.titlebar_accounts_revision =
                        this.titlebar_accounts_revision.wrapping_add(1);
                    this.refresh_titlebar_accounts(cx);
                });
            }
            let _ = panel.update(cx, |panel, cx| panel.finish_refresh(cx));
        })
        .detach();
    }

    /// Claims the reset the user confirmed in an account's usage panel on the gxserver that owns the account, then refreshes every account button and hands the result back to the panel.
    pub(crate) fn redeem_account_reset(
        &mut self,
        titlebar_key: &str,
        credit_id: String,
        request_id: String,
        panel: gpui::WeakEntity<AccountUsagePanel>,
        cx: &mut gpui::Context<Self>,
    ) {
        let account = self
            .titlebar_accounts
            .iter()
            .find(|a| a["titlebarKey"] == titlebar_key && a["registered"] == true)
            .cloned()
            .unwrap_or(Value::Null);
        let machine = account["titlebarMachine"]
            .as_str()
            .unwrap_or("local")
            .to_string();
        let target = if machine == "local" {
            Ok(None)
        } else {
            self.remote_gxserver_connections
                .get(&machine)
                .map(|connection| Some(connection.request_target()))
                .ok_or("That computer is no longer connected.")
        };
        let request = match (account["id"].as_str(), target) {
            (Some(id), Ok(target)) => Ok((
                target,
                json!({"operation":"redeemReset","id":id,"creditId":credit_id,"requestId":request_id}),
            )),
            (None, _) => Err("This account is no longer available.".to_string()),
            (_, Err(error)) => Err(error.to_string()),
        };
        cx.spawn(async move |this, cx| {
            let result = match request {
                Ok((target, params)) => {
                    cx.background_executor()
                        .spawn(async move {
                            let timeout = Duration::from_secs(90);
                            match target {
                                Some(target) => gpui_remote_gxserver_rpc_result(
                                    &target,
                                    "/api/agentAccounts",
                                    &params,
                                    timeout,
                                ),
                                None => {
                                    gpui_gxserver_rpc_result("/api/agentAccounts", &params, timeout)
                                }
                            }
                        })
                        .await
                }
                Err(error) => Err(error),
            };
            let (outcome, message) = match result {
                Ok(value) => (
                    value["outcome"].as_str().unwrap_or("failed").to_string(),
                    value["message"].as_str().unwrap_or("").to_string(),
                ),
                Err(error) => (
                    "failed".to_string(),
                    super::account_usage::account_display_text(&error),
                ),
            };
            let _ = this.update(cx, |this, cx| {
                this.titlebar_accounts_revision = this.titlebar_accounts_revision.wrapping_add(1);
                this.refresh_titlebar_accounts(cx);
            });
            let _ = panel.update(cx, |panel, cx| panel.finish_claim(&outcome, message, cx));
        })
        .detach();
    }
}
