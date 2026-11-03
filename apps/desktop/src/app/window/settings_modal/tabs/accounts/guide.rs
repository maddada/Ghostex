//! `AccountConnectionGuide` (accounts/connection-guide.tsx (deleted 2026-10-01)): the "Connect your accounts" dialog
//! over Settings, at most 90% of the Settings window's height, with the shared steps once, a card
//! per provider (the chosen one highlighted, its helper's install command while it is missing) and
//! the sign-in form for the chosen provider.
//!
//! CDXC:Settings 2026-09-07 DECISION:
//! Each provider has a connection-guide button. Both open the same Settings dialog with a backdrop, shared Ghostex instructions once, and two bullets for the provider-specific steps, each with a short helper and author credit. The tutorial is at most 90% of its Settings modal's height, including when Settings resizes.
use super::super::super::super::native_modal_kit::*;
use super::super::super::fields::settings_dialog_sheet;
use super::super::super::palette::SettingsPalette;
use super::AccountsTab;
use super::widgets::account_logo;
use gpui::prelude::FluentBuilder as _;
use gpui::{
    AnyElement, Context, FontWeight, IntoElement, ParentElement as _, Styled as _, Window, div, px,
};
use gpui_component::{h_flex, v_flex};

impl AccountsTab {
    pub(crate) fn open_guide(
        &mut self,
        provider: String,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.guide = Some(provider);
        self.guide_focus.focus(window, cx);
        cx.notify();
    }

    pub(crate) fn render_guide(
        &mut self,
        p: &SettingsPalette,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Option<AnyElement> {
        let p = &p.nested_dialog();
        let provider = self.guide.clone()?;
        let data = self.client.read(cx).data.clone()?;
        let border = modal_rgba(
            if p.light { 0x000000 } else { 0xffffff },
            if p.light { 0.14 } else { 0.11 },
        );
        let heading = |text: &str| {
            div()
                .text_size(px(13.0))
                .line_height(px(18.57))
                .font_weight(FontWeight::SEMIBOLD)
                .text_color(hsla(p.foreground))
                .child(text.to_string())
                .into_any_element()
        };
        let paragraph = |text: &str| {
            div()
                .text_size(px(12.0))
                .line_height(px(19.2))
                .text_color(hsla(p.muted))
                .child(text.to_string())
                .into_any_element()
        };
        let section = |children: Vec<AnyElement>| {
            v_flex()
                .w_full()
                .gap(px(10.0))
                .children(children)
                .into_any_element()
        };
        let mut providers: Vec<AnyElement> = Vec::new();
        for id in ["claude", "codex"] {
            let helper = data.helper(id);
            let mut parts: Vec<AnyElement> = vec![
                h_flex()
                    .items_center()
                    .gap(px(8.0))
                    .text_size(px(12.0))
                    .font_weight(FontWeight::SEMIBOLD)
                    .text_color(hsla(p.foreground))
                    .child(account_logo(p, id, 21.0))
                    .child(if id == "claude" {
                        "Claude Code"
                    } else {
                        "Codex"
                    })
                    .into_any_element(),
                paragraph(if id == "claude" {
                    "Claude accounts use Claude Swap (cswap) by Onur Cetinkol (realiti4)."
                } else {
                    "Codex accounts use Codex Swap (xswap CLI) by Mohamad Yahia (maddada)."
                }),
                paragraph(if id == "claude" {
                    "Choose Sign in with browser and pick the account in the browser. Ghostex uses a separate login profile and saves the account with cswap. To refresh an existing login, use that account’s Reconnect button."
                } else {
                    "Enter the new account’s email, then choose Sign in with browser and pick that account. xswap checks the login before saving a separate account home with shared session history. To refresh an existing login, use that account’s Reconnect button."
                }),
            ];
            if let Some(helper) =
                helper.filter(|helper| helper["installed"].as_bool() != Some(true))
            {
                if let Some(button) = self.render_helper_install_button(p, id, "guide", cx) {
                    parts.push(h_flex().child(button).into_any_element());
                } else {
                    let command = helper["installCommand"]
                        .as_str()
                        .unwrap_or_default()
                        .to_string();
                    parts.push(self.copy_command_block(
                        p,
                        format!("guide-{id}-install"),
                        &command,
                        cx,
                    ));
                }
            }
            providers.push(
                v_flex()
                    .w_full()
                    .gap(px(10.0))
                    .p(px(14.0))
                    .rounded(px(MODAL_RADIUS_CONTROL))
                    .border_1()
                    .border_color(hsla(border))
                    .when(provider == id, |this| {
                        this.bg(hsla(p.foreground_alpha(0.05)))
                    })
                    .children(parts)
                    .into_any_element(),
            );
        }
        let installed = data
            .helper(&provider)
            .is_some_and(|helper| helper["installed"].as_bool() == Some(true));
        let flow = installed.then(|| {
            let provider_id: &'static str = if provider == "claude" {
                "claude"
            } else {
                "codex"
            };
            let consent_needed = !data
                .registered(provider_id)
                .iter()
                .any(|account| account.raw["sharedHistory"] == true);
            div()
                .w_full()
                .my(px(12.0))
                .child(self.render_connect_flow(
                    p,
                    format!("guide:{provider}"),
                    provider_id,
                    None,
                    None,
                    consent_needed,
                    true,
                    window,
                    cx,
                ))
                .into_any_element()
        });
        let mut first: Vec<AnyElement> = vec![
            heading("1. Save a login"),
            paragraph(
                "Allow shared chat history the first time, then choose Sign in with browser. Finish signing in through your browser; Settings shows the progress automatically.",
            ),
            v_flex()
                .w_full()
                .my(px(4.0))
                .gap(px(10.0))
                .children(providers)
                .into_any_element(),
        ];
        first.extend(flow);
        let body = vec![
            section(first),
            section(vec![
                heading("2. Your account is ready"),
                paragraph(
                    "Ghostex checks and adds the connected account automatically, then opens Settings > Accounts with it selected. Turn on Show usage in the sidebar to keep its usage there. In chat context details, star Account limits to show usage in the status line.",
                ),
                paragraph(
                    "Give the account a name. A small badge centered over the session’s agent icon identifies the account: its number, or up to two letters or numbers you set under the account’s Advanced settings, where you can also swap its number with another account.",
                ),
            ]),
            section(vec![
                heading("3. Start with an account"),
                paragraph(
                    "Open the agent dropdown in a project’s sidebar header. Choose Claude or Codex, then an account to start a session immediately. Custom agents keep their own settings.",
                ),
                paragraph(
                    "Quick launch uses the account chosen under New sessions start on in the provider’s rules. Choosing another account from the launcher applies only to that new session. Until you add an account for a provider, Ghostex uses its current CLI login without an account switcher.",
                ),
            ]),
            section(vec![
                heading("Let work continue"),
                paragraph(
                    "In the provider’s rules, choose what happens when an account hits its limit: switch to another account, wait for it to reset, or do nothing. Switching uses only connected accounts with Auto-switch on, trying them in the order you pick. Shared history lets the conversation resume.",
                ),
                paragraph(
                    "With error retries enabled, a lost connection or server error retries once the network is back, after 5s, 15s, 1m, 2m and 5m, then like other temporary failures after 5, 10, 20, 40, then 60 minutes between attempts. Login and permission problems need your attention. The computer and Ghostex server must stay running.",
                ),
                paragraph(
                    "Existing sessions keep their saved settings. Change one session through More actions → Switch account, where you can choose a login or override auto-continue.",
                ),
            ]),
        ];
        let max_height = f32::from(window.viewport_size().height) * 0.9;
        let description = div()
            .text_size(px(12.0))
            .line_height(px(19.2))
            .child("Add an account for usage stats on the computer where your sessions run. This is optional, even for a single account.")
            .into_any_element();
        let focus = self.guide_focus.clone();
        Some(settings_dialog_sheet(
            p,
            "account-connection-guide",
            &focus,
            "Connect your accounts",
            Some(description),
            680.0,
            Some(max_height),
            24.0,
            20.0,
            true,
            body,
            None,
            |page: &mut Self, _window, cx| {
                page.guide = None;
                cx.notify();
            },
            window,
            cx,
        ))
    }
}
