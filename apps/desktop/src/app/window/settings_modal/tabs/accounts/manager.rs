//! `AccountManager` and `AccountSetup` (accounts/manager.tsx (deleted 2026-10-01)): the account
//! list beside the selected pane (an account, a provider's rules, or Add account), with Claude
//! Swap and Codex Swap as quiet lines under them.
//!
//! CDXC:Settings 2026-10-14 DECISION:
//! User picked layout B, "List and detail", for Settings > Accounts: each provider's saved accounts (session badge, name, a usage bar, Reconnect when the login stopped working) and its rules in a list on the left, the selected account's usage and settings on the right, Add account and the connection guide in each provider's list header, and Claude Swap / Codex Swap as one quiet line each under the panes. Supersedes the Settings 2026-09-10 decision that listed the accounts as expandable rows in one card per provider.
use super::super::super::super::native_modal_kit::*;
use super::super::super::catalog::SettingOption;
use super::super::super::fields::{RowSpec, select_field};
use super::super::super::palette::SettingsPalette;
use super::data::{Account, AccountsState, helper_label, provider_label};
use super::editor::DETAIL_INSET;
use super::widgets::{account_logo, account_text};
use super::{AccountsTab, Pane};
use gpui::{
    AnyElement, Context, FontWeight, InteractiveElement as _, IntoElement, ParentElement as _,
    SharedString, Styled as _, Window, div, px,
};
use gpui_component::{h_flex, v_flex};

/// `AccountSetup`'s own state: the existing login it adds.
#[derive(Clone, Debug, Default)]
pub(crate) struct SetupDraft {
    pub(crate) selected: String,
}

impl AccountsTab {
    /// The list, the selected pane, and the helper lines.
    pub(crate) fn render_manager(
        &mut self,
        p: &SettingsPalette,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let Some(data) = self.client.read(cx).data.clone() else {
            return div().into_any_element();
        };
        let pane = self.effective_pane(&data);
        let list = self.render_account_list(p, &data, &pane, cx);
        let detail = match &pane {
            Pane::Account(id) => {
                let account = data
                    .accounts()
                    .into_iter()
                    .find(|account| account.id() == *id);
                match account {
                    Some(account) => {
                        let accounts = data.registered(&account.provider());
                        self.render_account_detail(p, &account, &accounts, window, cx)
                    }
                    None => div().into_any_element(),
                }
            }
            Pane::Rules(provider) => self.render_rules_pane(p, provider, &data, window, cx),
            Pane::Add(provider) => self.render_add_pane(p, provider, &data, window, cx),
        };
        let footer: Vec<AnyElement> = super::data::PROVIDERS
            .into_iter()
            .flat_map(|provider| self.render_helper_footer(p, provider, cx))
            .collect();
        v_flex()
            .w_full()
            .gap(px(10.0))
            .child(
                h_flex()
                    .w_full()
                    .items_start()
                    .gap(px(12.0))
                    .child(list)
                    .child(
                        div()
                            .flex_1()
                            .min_w_0()
                            .rounded(px(MODAL_RADIUS_SECTION))
                            .border_1()
                            .border_color(hsla(p.hairline))
                            .bg(hsla(p.raised))
                            .child(detail),
                    ),
            )
            .children(footer)
            .into_any_element()
    }

    /// `AccountSetup`: Add a <provider> account. Installs the helper first when it is missing,
    /// offers logins the helper already saved, and otherwise signs in to a new one.
    fn render_add_pane(
        &mut self,
        p: &SettingsPalette,
        provider: &'static str,
        data: &AccountsState,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let hide = self.hide_emails(cx);
        let label = provider_label(provider);
        let helper = data.helper(provider);
        let registered = data.registered(provider);
        let available: Vec<Account> = data
            .accounts()
            .into_iter()
            .filter(|account| account.provider() == provider && !account.registered())
            .collect();
        let draft = self
            .setups
            .entry(provider.to_string())
            .or_insert_with(|| SetupDraft {
                selected: "new".into(),
            })
            .clone();
        let installed = helper
            .as_ref()
            .is_some_and(|helper| helper["installed"].as_bool() == Some(true));
        let mut body: Vec<AnyElement> = Vec::new();
        let intro = if registered.is_empty() {
            format!(
                "{label} uses your current CLI login until you add an account. Adding one shows its usage and reset times in Ghostex and lets sessions switch accounts at a limit."
            )
        } else {
            format!(
                "Sign in with the {label} account you want to add. Ghostex opens your browser and saves the account when you finish."
            )
        };
        body.push(muted(p, intro));
        if !installed && helper.is_some() && self.helper_offers_install(provider) {
            body.push(muted(
                p,
                format!(
                    "Ghostex needs {} to keep several {label} logins. It installs it on this computer, then continues here.",
                    helper_label(provider)
                ),
            ));
            if let Some(button) = self.render_helper_install_button(p, provider, "setup", cx) {
                body.push(h_flex().child(button).into_any_element());
            }
        } else if !installed {
            if let Some(helper) = helper {
                body.push(muted(
                    p,
                    format!(
                        "Install {} on this computer, then refresh accounts.",
                        helper_label(provider)
                    ),
                ));
                let command = helper["installCommand"]
                    .as_str()
                    .unwrap_or_default()
                    .to_string();
                body.push(self.copy_command_block(
                    p,
                    format!("setup-{provider}-install"),
                    &command,
                    cx,
                ));
            }
        } else {
            if !available.is_empty() {
                let mut options = vec![SettingOption {
                    label: "Sign in to a new account".into(),
                    value: "new".into(),
                }];
                options.extend(available.iter().map(|account| SettingOption {
                    label: account_text(
                        &if account.name().is_empty() {
                            account.email()
                        } else {
                            account.name()
                        },
                        hide,
                    ),
                    value: account.id(),
                }));
                body.push(
                    div()
                        .mx(px(-DETAIL_INSET))
                        .child(select_field(
                            self,
                            p,
                            if provider == "claude" {
                                "accounts-claude-setup-login"
                            } else {
                                "accounts-codex-setup-login"
                            },
                            RowSpec::new("Login to add")
                                .description(format!(
                                    "{} already has these logins saved. Pick one to add it without signing in again.",
                                    helper_label(provider)
                                ))
                                .inset_x(DETAIL_INSET),
                            None,
                            &options,
                            &draft.selected,
                            Some(super::editor::DETAIL_CONTROL_WIDTH),
                            move |page: &mut Self, next, _window, cx| {
                                if let Some(draft) = page.setups.get_mut(provider) {
                                    draft.selected = next;
                                }
                                cx.notify();
                            },
                            window,
                            cx,
                        ))
                        .into_any_element(),
                );
            }
            let consent_needed = !registered
                .iter()
                .any(|account| account.raw["sharedHistory"] == true);
            let account = available
                .iter()
                .find(|account| account.id() == draft.selected)
                .cloned();
            match account.filter(|account| account.status() == "ready") {
                Some(account) => {
                    body.push(self.render_register_existing(p, provider, &account, hide, cx));
                }
                account => {
                    let flow = self.render_connect_flow(
                        p,
                        format!("setup:{provider}:{}", draft.selected),
                        provider,
                        account,
                        None,
                        consent_needed,
                        false,
                        window,
                        cx,
                    );
                    body.push(flow);
                }
            }
        }
        v_flex()
            .id(SharedString::from(format!("accounts-{provider}-add-pane")))
            .w_full()
            .p(px(DETAIL_INSET))
            .gap(px(12.0))
            .child(
                h_flex()
                    .gap(px(10.0))
                    .items_center()
                    .child(account_logo(p, provider, 20.0))
                    .child(
                        div()
                            .text_size(px(16.0))
                            .line_height(px(22.0))
                            .font_weight(FontWeight::MEDIUM)
                            .text_color(hsla(p.foreground))
                            .child(format!("Add a {label} account")),
                    ),
            )
            .children(body)
            .into_any_element()
    }

    /// A login the helper already saved: add it to Ghostex without signing in.
    fn render_register_existing(
        &mut self,
        p: &SettingsPalette,
        provider: &'static str,
        account: &Account,
        hide: bool,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let busy = self.client.read(cx).busy;
        let selector = account.selector();
        let name = account_text(
            &if account.name().is_empty() {
                account.email()
            } else {
                account.name()
            },
            hide,
        );
        let add = super::super::super::fields::settings_sized_button(
            p,
            SharedString::from(format!("accounts-{provider}-setup-register")),
            format!("Add {name}"),
            None,
            None,
            super::super::super::fields::SizedButtonVariant::Primary,
            super::super::super::fields::SizedButtonSize::Default,
            busy,
            None,
            move |page: &mut Self, _window, cx| {
                let this = cx.weak_entity();
                let (client, store) = (page.client.clone(), page.store.clone());
                let added = selector.clone();
                page.account_request(
                    serde_json::json!({ "operation": "register", "provider": provider, "selector": selector, "shareHistory": true }),
                    Some(Box::new(move |ok, cx| {
                        if ok {
                            super::show_added_account_in_sidebar(
                                &client,
                                &store,
                                |a| a.provider() == provider && a.selector() == added,
                                cx,
                            );
                            let _ = this.update(cx, |page, cx| {
                                page.pane = None;
                                cx.notify();
                            });
                        }
                    })),
                    cx,
                );
            },
            cx,
        );
        v_flex()
            .gap(px(10.0))
            .child(muted(
                p,
                format!(
                    "Adding it shares chat history between your {} accounts, so a conversation can carry on with another account when one runs out.",
                    provider_label(provider)
                ),
            ))
            .child(h_flex().child(add))
            .into_any_element()
    }

    /// `CopyCommand` with its 1.8s check.
    pub(crate) fn copy_command_block(
        &mut self,
        p: &SettingsPalette,
        key: String,
        command: &str,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let copied = self
            .copied
            .as_ref()
            .is_some_and(|(copied, _)| *copied == key);
        let text = command.to_string();
        let copy_key = key.clone();
        super::widgets::copy_command(
            p,
            SharedString::from(key),
            command,
            copied,
            move |page: &mut Self, _window, cx| {
                super::super::super::store::store_copy_to_clipboard(&page.store, text.clone(), cx);
                let key = copy_key.clone();
                let reset = cx.spawn(async move |this, cx| {
                    cx.background_executor()
                        .timer(std::time::Duration::from_millis(1800))
                        .await;
                    let _ = this.update(cx, |page, cx| {
                        if page
                            .copied
                            .as_ref()
                            .is_some_and(|(copied, _)| *copied == key)
                        {
                            page.copied = None;
                            cx.notify();
                        }
                    });
                });
                page.copied = Some((copy_key.clone(), reset));
                cx.notify();
            },
            cx,
        )
    }
}

fn muted(p: &SettingsPalette, text: String) -> AnyElement {
    div()
        .text_size(px(13.0))
        .line_height(px(19.0))
        .text_color(hsla(p.muted))
        .child(text)
        .into_any_element()
}
