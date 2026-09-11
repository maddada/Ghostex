use super::account_reset_flow::{self as flow, Mode};
use crate::*;
use gpui_component::{WindowExt, notification::Notification};
use serde_json::Value;
use std::{
    collections::HashSet,
    sync::{Mutex, OnceLock},
    time::Duration,
};

static ACTIVE: OnceLock<Mutex<HashSet<String>>> = OnceLock::new();
struct ResetGuard(String);
impl Drop for ResetGuard {
    fn drop(&mut self) {
        if let Ok(mut active) = ACTIVE.get_or_init(Mutex::default).lock() {
            active.remove(&self.0);
        }
    }
}
fn rpc(
    target: &Option<GpuiRemoteGxserverRequestTarget>,
    path: &str,
    params: &Value,
) -> Result<Value, String> {
    match target {
        Some(target) => {
            gpui_remote_gxserver_rpc_result(target, path, params, Duration::from_secs(60))
        }
        None => gpui_gxserver_rpc_result(path, params, Duration::from_secs(60)),
    }
}

fn reset_project_id(active_project: Option<&str>, account_machine: &str) -> Result<String, String> {
    let project_id = active_project
        .filter(|id| !id.is_empty())
        .ok_or("Open a project before redeeming a reset.")?;
    if let Some(reference) = gpui_remote_project_reference_from_project_id(project_id) {
        if reference.remote_machine_id == account_machine {
            return Ok(reference.project_id);
        }
    } else if account_machine == "local" && !project_id.starts_with("remote:") {
        return Ok(project_id.to_string());
    }
    Err("Open a project on the same computer as this account before redeeming a reset.".into())
}

impl GhostexGpuiApp {
    /// The account popup owns this one action; the account and machine are resolved from its current native generation.
    pub(crate) fn account_reset_popup_handler(
        &self,
        generation: u64,
        id: ExtensionId,
        cx: &mut gpui::Context<Self>,
    ) -> cef::BrowserPopupOpenHandler {
        let app = cx.entity().downgrade();
        let async_cx = cx.to_async();
        let foreground = cx.foreground_executor().clone();
        Rc::new(move |url, _| {
            if url != "ghostex-account:redeem-reset" {
                let _ = gpui_open_external_http_url(&url);
                return;
            }
            let app = app.clone();
            let mut async_cx = async_cx.clone();
            foreground
                .spawn(async move {
                    let _ = app.update_in(&mut async_cx, |this, window, cx| {
                        if !this.titlebar_extension_popup.as_ref().is_some_and(|popup| {
                            popup.account && popup.generation == generation && popup.id == id
                        }) {
                            return;
                        }
                        let Some(account) = this
                            .titlebar_accounts
                            .iter()
                            .find(|account| account["titlebarKey"] == id.as_str())
                            .cloned()
                        else {
                            return;
                        };
                        this.start_account_reset(&account, window, cx);
                    });
                })
                .detach();
        })
    }

    fn start_account_reset(
        &mut self,
        account: &Value,
        window: &mut Window,
        cx: &mut gpui::Context<Self>,
    ) {
        if account["provider"] != "codex" || account["registered"] != true {
            return;
        }
        let Some(account_id) = account["id"].as_str().map(str::to_string) else {
            return;
        };
        let machine = account["titlebarMachine"]
            .as_str()
            .unwrap_or("local")
            .to_string();
        let snapshot = self
            .latest_sidebar_project_snapshot
            .as_ref()
            .filter(|snapshot| !snapshot.is_quick_projectless);
        let project_id = reset_project_id(gpui_active_project_id_from_snapshot(snapshot), &machine);
        self.close_titlebar_extension_popup(window, cx);
        let project_id = match project_id {
            Ok(project_id) => project_id,
            Err(error) => {
                window.push_notification(Notification::warning(error), cx);
                return;
            }
        };
        let target = if machine == "local" {
            None
        } else {
            let Some(connection) = self.remote_gxserver_connections.get(&machine) else {
                return;
            };
            Some(connection.request_target())
        };
        let key = format!("{machine}:{account_id}");
        let Ok(mut active) = ACTIVE.get_or_init(Mutex::default).lock() else {
            return;
        };
        if !active.insert(key.clone()) {
            window.push_notification(
                Notification::warning("A reset is already in progress for this account."),
                cx,
            );
            return;
        }
        drop(active);
        let guard = ResetGuard(key);
        window.push_notification(
            Notification::info("Opening a Codex chat to redeem the reset expiring soonest."),
            cx,
        );
        cx.spawn(async move |this, cx| {
            let create_target = target.clone();
            let create_id = account_id.clone();
            let created = cx.background_executor().spawn(async move {
                flow::create(&|path, params| rpc(&create_target, path, params), &create_id, &project_id)
            }).await;
            let result = match created {
                Ok(session) => {
                    let session_id = session.target["sessionId"].as_str().unwrap_or("");
                    let project_id = session.target["projectId"].as_str().unwrap_or("");
                    let sidebar_id = if machine == "local" {
                        gpui_combined_presentation_session_id(project_id, session_id)
                    } else {
                        gpui_remote_scoped_session_id(&machine, project_id, session_id)
                    };
                    let focused = this.update_in(cx, |this, _window, cx| {
                        let focused = this.dispatch_gpui_command_palette_session_focus(&sidebar_id, cx);
                        this.reveal_sidebar_session(&sidebar_id, cx);
                        focused
                    }).unwrap_or(false);
                    if !focused {
                        Err("The reset chat was created, but could not be opened. Continue from the sidebar.".to_string())
                    } else {
                        cx.background_executor().spawn(async move {
                            let result = flow::drive(&|path, params| rpc(&target, path, params), &session, &account_id, Mode::Redeem);
                            if result.is_ok() {
                                let _ = rpc(&target, "/api/agentAccounts", &serde_json::json!({"operation":"titlebar","refresh":true}));
                            }
                            result
                        }).await
                    }
                }
                Err(error) => Err(error),
            };
            let _ = this.update_in(cx, |this, window, cx| {
                match result {
                    Ok(()) => window.push_notification(Notification::success("Reset redeemed. Account limits are refreshing."), cx),
                    Err(error) => window.push_notification(Notification::warning(super::account_usage::account_display_text(&error)), cx),
                }
                this.refresh_titlebar_accounts(cx);
            });
            drop(guard);
        }).detach();
    }
}

#[cfg(test)]
mod tests {
    use super::reset_project_id;

    #[test]
    fn reset_uses_the_active_project_on_the_accounts_computer() {
        assert_eq!(reset_project_id(Some("P3lv0"), "local").unwrap(), "P3lv0");
        assert_eq!(
            reset_project_id(
                Some("remote:remote-workstation:project:P42"),
                "remote-workstation"
            )
            .unwrap(),
            "P42"
        );
        assert!(reset_project_id(Some("remote:remote-workstation:project:P42"), "local").is_err());
        assert!(reset_project_id(Some("P3lv0"), "remote-workstation").is_err());
        assert!(reset_project_id(None, "local").is_err());
    }
}
