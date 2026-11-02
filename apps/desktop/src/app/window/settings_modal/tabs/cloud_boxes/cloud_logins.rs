//! "Agent logins for boxes" (plan §2.5): one row per saved Claude and Codex account with Allow on
//! Cloud Boxes. A Claude account makes its one-year box token through `claude setup-token`, which
//! gxserver runs and which opens the approval page in the browser (D5); a Codex account is allowed
//! at once and its login is copied at boot (D6).
use super::super::super::fields::{
    ButtonVariant, ListItemStatus, settings_button, settings_section,
};
use super::super::super::palette::SettingsPalette;
use super::{CloudBoxesTab, info_row};
use ghostex_gx_protocol::cloud_box::{AgentLoginsResult, CloudAgentLogin, CloudAgentLoginState};
use gpui::{AnyElement, Context, IntoElement, SharedString};
use serde_json::json;
use std::time::Duration;

const ICON_KEY: &str = "modals/settings/key.svg";
const ICON_EXTERNAL: &str = "modals/settings/external-link.svg";
const ICON_ROBOT: &str = "modals/settings/robot.svg";
const SET_TIMEOUT: Duration = Duration::from_secs(30);

impl CloudBoxesTab {
    fn set_agent_login(&mut self, account_id: String, allowed: bool, cx: &mut Context<Self>) {
        self.cloud.busy.insert(account_id.clone());
        cx.notify();
        self.cloud_rpc(
            "setAgentLogin",
            json!({ "accountId": account_id, "allowed": allowed }),
            SET_TIMEOUT,
            move |page, result, cx| {
                page.cloud.busy.remove(&account_id);
                match result.and_then(|value| {
                    serde_json::from_value::<AgentLoginsResult>(value)
                        .map_err(|error| format!("Unexpected answer: {error}"))
                }) {
                    Ok(answer) => page.cloud.logins = Some(answer.logins),
                    Err(error) => page.toast(
                        "error",
                        if allowed {
                            "Couldn't allow the account on boxes"
                        } else {
                            "Couldn't remove the account from boxes"
                        },
                        &error,
                        cx,
                    ),
                }
            },
            cx,
        );
    }

    pub(super) fn cloud_logins_section(
        &mut self,
        p: &SettingsPalette,
        show: impl Fn(&str) -> bool,
        cx: &mut Context<Self>,
    ) -> Option<AnyElement> {
        let claude = show("cloudBoxesClaudeLogins");
        let codex = show("cloudBoxesCodexLogins");
        if !claude && !codex {
            return None;
        }
        let mut rows: Vec<AnyElement> = Vec::new();
        match (self.cloud.logins.clone(), self.cloud.logins_error.clone()) {
            (_, Some(error)) => rows.push(info_row(
                p,
                "cloud-logins-error",
                Some(ListItemStatus::Warning),
                ICON_KEY,
                "Agent logins",
                format!("Ghostex could not read your accounts: {error}"),
                Some(format!("Ghostex could not read your accounts: {error}")),
                Vec::new(),
            )),
            (None, None) => rows.push(info_row(
                p,
                "cloud-logins-loading",
                None,
                ICON_KEY,
                "Agent logins",
                "",
                Some("Reading your accounts…".to_string()),
                Vec::new(),
            )),
            (Some(logins), None) => {
                let logins: Vec<CloudAgentLogin> = logins
                    .into_iter()
                    .filter(|login| match login.agent.as_str() {
                        "claude" => claude,
                        "codex" => codex,
                        _ => false,
                    })
                    .collect();
                if logins.is_empty() {
                    rows.push(info_row(
                        p,
                        "cloud-logins-empty",
                        None,
                        ICON_KEY,
                        "No Claude or Codex accounts",
                        "Add an account in Settings › Accounts first; boxes use the accounts you allow here.",
                        Some("Add one in Settings › Accounts".to_string()),
                        Vec::new(),
                    ));
                }
                for login in logins {
                    rows.push(self.cloud_login_row(p, &login, cx));
                }
            }
        }
        settings_section(
            p,
            "Agent logins for boxes",
            Some("Boxes run Claude and Codex on the accounts you allow here.".into()),
            None,
            rows,
        )
        .map(IntoElement::into_any_element)
    }

    fn cloud_login_row(
        &mut self,
        p: &SettingsPalette,
        login: &CloudAgentLogin,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let busy = self.cloud.busy.contains(&login.account_id);
        let agent = if login.agent == "codex" {
            "Codex"
        } else {
            "Claude"
        };
        let (dot, state) = match &login.state {
            CloudAgentLoginState::Ready => (ListItemStatus::Success, "On cloud boxes".to_string()),
            CloudAgentLoginState::Waiting => (
                ListItemStatus::Neutral,
                "Approve in your browser to make the box token…".to_string(),
            ),
            CloudAgentLoginState::Error => (
                ListItemStatus::Warning,
                login
                    .detail
                    .clone()
                    .unwrap_or_else(|| "The box token wasn't made.".to_string()),
            ),
            _ => (ListItemStatus::Neutral, "Not on cloud boxes".to_string()),
        };
        let mut controls: Vec<AnyElement> = Vec::new();
        let account_id = login.account_id.clone();
        match &login.state {
            CloudAgentLoginState::Ready | CloudAgentLoginState::Waiting => {
                if let Some(url) = login
                    .auth_url
                    .clone()
                    .filter(|_| login.state == CloudAgentLoginState::Waiting)
                {
                    controls.push(settings_button(
                        p,
                        SharedString::from(format!("cloud-login-open-{account_id}")),
                        "Open the Page Again",
                        Some(ICON_EXTERNAL),
                        ButtonVariant::Ghost,
                        false,
                        None,
                        move |page: &mut Self, _window, cx| page.open_url(&url, cx),
                        cx,
                    ));
                }
                let id = account_id.clone();
                controls.push(settings_button(
                    p,
                    SharedString::from(format!("cloud-login-remove-{account_id}")),
                    if login.state == CloudAgentLoginState::Waiting {
                        "Cancel"
                    } else {
                        "Remove"
                    },
                    None,
                    ButtonVariant::Ghost,
                    busy,
                    None,
                    move |page: &mut Self, _window, cx| page.set_agent_login(id.clone(), false, cx),
                    cx,
                ));
            }
            _ => {
                let id = account_id.clone();
                controls.push(settings_button(
                    p,
                    SharedString::from(format!("cloud-login-allow-{account_id}")),
                    if login.state == CloudAgentLoginState::Error {
                        "Try Again"
                    } else {
                        "Allow on Cloud Boxes"
                    },
                    Some(ICON_KEY),
                    ButtonVariant::Outline,
                    busy || !self.rpc_available(cx),
                    Some("Working…".into()),
                    move |page: &mut Self, _window, cx| page.set_agent_login(id.clone(), true, cx),
                    cx,
                ));
            }
        }
        let tooltip = login.detail.clone().unwrap_or_else(|| {
            if login.agent == "codex" {
                "Each box gets a fresh copy of this login at boot and sends refreshed sign-ins back to this computer.".to_string()
            } else {
                "Allow makes a one-year box token after one approval in your browser, so Claude on this computer stays signed in. claude.ai connectors and Remote Control don't work on a box.".to_string()
            }
        });
        let title = if login.name.trim().is_empty() {
            agent.to_string()
        } else {
            format!("{agent} · {}", login.name)
        };
        info_row(
            p,
            &format!("cloud-login-{account_id}"),
            Some(dot),
            ICON_ROBOT,
            title,
            tooltip,
            Some(state),
            controls,
        )
    }
}
