//! `AccountManager` and `AccountSetup` (accounts/manager.tsx (deleted 2026-10-01)): the account
//! list beside the selected pane (an account, a provider's rules, or Add account), with Claude
//! Swap and Codex Swap as quiet lines under them.
//!
//! CDXC:Settings 2026-10-14 DECISION:
//! User picked layout B, "List and detail", for Settings > Accounts: each provider's saved accounts (session badge, name, a usage bar, Reconnect when the login stopped working) and its rules in a list on the left, the selected account's usage and settings on the right, Add account and the connection guide in each provider's list header, and Claude Swap / Codex Swap as one quiet line each under the panes. Supersedes the Settings 2026-09-10 decision that listed the accounts as expandable rows in one card per provider.
use super::super::super::super::native_modal_kit::*;
use super::super::super::palette::SettingsPalette;
use super::data::{Account, AccountsState, helper_label, provider_label};
use super::editor::DETAIL_INSET;
use super::widgets::{account_logo, account_text};
use super::{AccountsTab, Pane};
use gpui::{
    AnyElement, Context, FontWeight, InteractiveElement as _, IntoElement, ParentElement as _,
    SharedString, StatefulInteractiveElement as _, Styled as _, Window, div, px,
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
                    Some(account) if account.usage_only() => {
                        self.render_usage_account_detail(p, &account, window, cx)
                    }
                    Some(account) => {
                        let accounts = data.registered(&account.provider());
                        self.render_account_detail(p, &account, &accounts, window, cx)
                    }
                    None => div().into_any_element(),
                }
            }
            Pane::Rules(provider) => self.render_rules_pane(p, provider, &data, window, cx),
            Pane::Add(provider) => self.render_add_pane(p, provider, &data, window, cx),
            Pane::AddPick => self.render_add_pick(p, &data, cx),
            Pane::AddUsage(provider) => self.render_add_usage_pane(p, provider, window, cx),
        };
        // Only a running Claude Swap or Codex Swap action and the Uninstall confirmation.
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
        let label = provider_label(provider);
        let helper = data.helper(provider);
        let registered = data.registered(provider);
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
        } else if provider == "codex"
            && helper
                .as_ref()
                .is_some_and(|helper| helper["codexSwapNeedsUpdate"] == true)
        {
            body.push(muted(
                p,
                "Codex Swap needs an update to add an account without its email. Update it, then sign in.".into(),
            ));
            body.push(
                h_flex()
                    .child(super::super::super::fields::settings_sized_button(
                        p,
                        "accounts-codex-swap-update",
                        "Update Codex Swap",
                        Some("modals/settings/download.svg"),
                        None,
                        super::super::super::fields::SizedButtonVariant::Primary,
                        super::super::super::fields::SizedButtonSize::Sm,
                        self.helpers.tools.iter().any(|tool| {
                            tool.provider() == "codex" && tool.running_action().is_some()
                        }),
                        Some("Codex Swap is being updated.".into()),
                        |page: &mut Self, _window, cx| {
                            page.run_helper_action("codex".to_string(), "update", cx)
                        },
                        cx,
                    ))
                    .into_any_element(),
            );
        } else {
            let flow = self.render_connect_flow(
                p,
                format!("setup:{provider}:new"),
                provider,
                None,
                None,
                false,
                false,
                window,
                cx,
            );
            body.push(flow);
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

    /// Adds a login the helper already saved, without signing in, and shows it.
    fn register_existing(
        &mut self,
        provider: &'static str,
        selector: String,
        cx: &mut Context<Self>,
    ) {
        let this = cx.weak_entity();
        let (client, store) = (self.client.clone(), self.store.clone());
        let added = selector.clone();
        self.account_request(
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
    }

    /// Picking a provider in Add account: Claude and Codex start their sign-in at once (once their
    /// helper is there), Cursor, Gemini and Copilot theirs.
    pub(crate) fn pick_provider(
        &mut self,
        provider: &'static str,
        data: &AccountsState,
        cx: &mut Context<Self>,
    ) {
        if super::data::USAGE_PROVIDERS.contains(&provider) {
            self.select_pane(Pane::AddUsage(provider), cx);
            self.start_usage_sign_in(provider, None, cx);
            return;
        }
        self.select_pane(Pane::Add(provider), cx);
        let helper = data.helper(provider);
        let ready = helper.as_ref().is_some_and(|helper| {
            helper["installed"] == true && helper["codexSwapNeedsUpdate"] != true
        });
        if ready {
            let key = format!("setup:{provider}:new");
            self.flows.insert(
                key.clone(),
                super::connect::FlowState::new(String::new(), true, None),
            );
            self.start_flow(&key, provider, None, None, cx);
        }
    }

    /// Add account: the providers, and the logins Claude Swap or Codex Swap already saved.
    ///
    /// CDXC:Settings 2026-11-05 DECISION:
    /// User asked for adding an account to work like the reference flow they chose: one Add account button, then a provider list ("Choose a provider to sign in to."), and the browser opens as soon as one is picked. Logins Claude Swap or Codex Swap already keep are listed under it with an Add button, replacing the Login to add menu of the old Add pane.
    fn render_add_pick(
        &mut self,
        p: &SettingsPalette,
        data: &AccountsState,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let hide = self.hide_emails(cx);
        let row = |id: SharedString,
                   logo: AnyElement,
                   title: String,
                   note: Option<String>,
                   trailing: AnyElement| {
            h_flex()
                .id(id)
                .w_full()
                .px(px(10.0))
                .py(px(8.0))
                .gap(px(10.0))
                .items_center()
                .rounded(px(MODAL_RADIUS_CONTROL))
                .border_1()
                .border_color(hsla(p.hairline))
                .bg(hsla(p.foreground_alpha(0.02)))
                .child(logo)
                .child(
                    v_flex()
                        .flex_1()
                        .min_w_0()
                        .child(
                            div()
                                .text_size(px(13.0))
                                .text_color(hsla(p.foreground))
                                .child(title),
                        )
                        .children(note.map(|note| {
                            div()
                                .text_size(px(11.5))
                                .text_color(hsla(p.muted))
                                .child(note)
                        })),
                )
                .child(trailing)
        };
        let mut body: Vec<AnyElement> = vec![muted(p, "Choose a provider to sign in to.".into())];
        // Cursor, Gemini and Copilot sign in through this computer's browser profiles, so another
        // computer offers only Claude and Codex.
        let machine_name = self.shown_machine_name(cx);
        let usage_providers: &[&'static str] = if machine_name.is_some() {
            &[]
        } else {
            &super::data::USAGE_PROVIDERS
        };
        for provider in super::data::PROVIDERS
            .into_iter()
            .chain(usage_providers.iter().copied())
        {
            let usage_only = super::data::USAGE_PROVIDERS.contains(&provider);
            let pick_data = data.clone();
            body.push(
                row(
                    SharedString::from(format!("accounts-pick-{provider}")),
                    account_logo(p, provider, 18.0),
                    provider_label(provider).to_string(),
                    usage_only.then(|| "Usage only".to_string()),
                    super::super::super::fields::settings_icon(
                        "modals/settings/chevron-right.svg",
                        14.0,
                        p.muted,
                    )
                    .into_any_element(),
                )
                .cursor_pointer()
                .hover(|this| this.bg(hsla(p.raised_hover)))
                .on_click(cx.listener(move |page, _: &gpui::ClickEvent, _window, cx| {
                    page.pick_provider(provider, &pick_data, cx)
                }))
                .into_any_element(),
            );
        }
        let saved: Vec<Account> = data
            .accounts()
            .into_iter()
            .filter(|account| {
                !account.usage_only() && !account.registered() && account.status() == "ready"
            })
            .collect();
        if !saved.is_empty() {
            body.push(muted(
                p,
                match &machine_name {
                    Some(name) => format!("Already signed in on {name}:"),
                    None => "Already signed in on this computer:".to_string(),
                },
            ));
            for account in saved {
                let provider: &'static str = if account.provider() == "claude" {
                    "claude"
                } else {
                    "codex"
                };
                let name = account_text(
                    &if account.name().is_empty() {
                        account.email()
                    } else {
                        account.name()
                    },
                    hide,
                );
                let selector = account.selector();
                let add = super::super::super::fields::settings_sized_button(
                    p,
                    SharedString::from(format!("accounts-pick-saved-{}", account.id())),
                    "Add",
                    None,
                    None,
                    super::super::super::fields::SizedButtonVariant::Outline,
                    super::super::super::fields::SizedButtonSize::Sm,
                    self.client.read(cx).busy,
                    None,
                    move |page: &mut Self, _window, cx| {
                        page.register_existing(provider, selector.clone(), cx)
                    },
                    cx,
                );
                body.push(
                    row(
                        SharedString::from(format!("accounts-pick-saved-row-{}", account.id())),
                        account_logo(p, provider, 18.0),
                        name,
                        Some(format!(
                            "{} · kept by {}",
                            provider_label(provider),
                            helper_label(provider)
                        )),
                        add,
                    )
                    .into_any_element(),
                );
            }
        }
        body.push(muted(
            p,
            match &machine_name {
                Some(name) => format!("Claude and Codex accounts share their conversations, so a conversation carries on with another account when one hits its limit. The sign-in runs on {name}, and its page opens here."),
                None => "Claude and Codex accounts share their conversations, so a conversation carries on with another account when one hits its limit. Cursor, Gemini and Copilot accounts only show their usage.".to_string(),
            },
        ));
        v_flex()
            .id("accounts-add-pick")
            .w_full()
            .p(px(DETAIL_INSET))
            .gap(px(8.0))
            .child(
                div()
                    .pb(px(4.0))
                    .text_size(px(16.0))
                    .line_height(px(22.0))
                    .font_weight(FontWeight::MEDIUM)
                    .text_color(hsla(p.foreground))
                    .child("Add account"),
            )
            .children(body)
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
