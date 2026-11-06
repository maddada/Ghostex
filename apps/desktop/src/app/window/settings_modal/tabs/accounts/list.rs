//! The account manager's left pane: per provider a header (Connection guide, Add account), its
//! saved accounts (session badge, name, the tightest usage bar, or Reconnect when the login
//! stopped working) and its rules.
use super::super::super::super::native_modal_kit::*;
use super::super::super::fields::{
    SizedButtonSize, SizedButtonVariant, icon, settings_icon, settings_sized_button, tooltip_text,
};
use super::super::super::palette::SettingsPalette;
use super::data::{Account, AccountsState, provider_label};
use super::widgets::{
    TagTone, account_badge, account_logo, account_mark, account_text, status_tag, usage_bar,
};
use super::{AccountsTab, Pane};
use gpui::prelude::FluentBuilder as _;
use gpui::{
    AnyElement, ClickEvent, Context, FontWeight, InteractiveElement as _, IntoElement,
    ParentElement as _, SharedString, StatefulInteractiveElement as _, Styled as _, div, px,
};
use gpui_component::{h_flex, v_flex};

/// The list pane's width; the detail pane takes the rest of the page column.
pub(crate) const LIST_WIDTH: f32 = 212.0;

impl AccountsTab {
    pub(crate) fn render_account_list(
        &mut self,
        p: &SettingsPalette,
        data: &AccountsState,
        pane: &Pane,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let hide = self.hide_emails(cx);
        let divider = || {
            div()
                .mx(px(4.0))
                .my(px(6.0))
                .h(px(1.0))
                .bg(hsla(p.hairline))
                .into_any_element()
        };
        let mut children: Vec<AnyElement> = vec![
            div()
                .px(px(2.0))
                .pt(px(2.0))
                .pb(px(4.0))
                .child(settings_sized_button(
                    p,
                    "accounts-add",
                    "Add account",
                    Some(icon::PLUS),
                    None,
                    if matches!(pane, Pane::AddPick | Pane::Add(_) | Pane::AddUsage(_)) {
                        SizedButtonVariant::Primary
                    } else {
                        SizedButtonVariant::Outline
                    },
                    SizedButtonSize::Sm,
                    false,
                    None,
                    |page: &mut Self, _window, cx| page.select_pane(Pane::AddPick, cx),
                    cx,
                ))
                .into_any_element(),
        ];
        for (index, provider) in super::data::PROVIDERS.into_iter().enumerate() {
            if index > 0 {
                children.push(divider());
            }
            children.push(self.render_list_header(p, provider));
            let accounts = data.registered(provider);
            if accounts.is_empty() {
                children.push(list_item(
                    p,
                    SharedString::from(format!("accounts-{provider}-empty")),
                    *pane == Pane::Add(provider),
                    div()
                        .size(px(24.0))
                        .flex()
                        .items_center()
                        .justify_center()
                        .child(account_logo(p, provider, 16.0))
                        .into_any_element(),
                    "Current CLI login".into(),
                    Some(
                        div()
                            .text_color(hsla(p.muted))
                            .child("No accounts added")
                            .into_any_element(),
                    ),
                    move |page, cx| page.select_pane(Pane::Add(provider), cx),
                    cx,
                ));
            }
            for account in &accounts {
                children.push(self.render_list_account(p, account, pane, hide, cx));
            }
            children.push(list_item(
                p,
                SharedString::from(format!("accounts-{provider}-rules")),
                *pane == Pane::Rules(provider),
                div()
                    .size(px(24.0))
                    .flex()
                    .items_center()
                    .justify_center()
                    .child(settings_icon("modals/settings/settings.svg", 15.0, p.muted))
                    .into_any_element(),
                format!("{} rules", provider_label(provider)).into(),
                None,
                move |page, cx| page.select_pane(Pane::Rules(provider), cx),
                cx,
            ));
        }
        for provider in super::data::USAGE_PROVIDERS {
            let accounts = data.registered(provider);
            if accounts.is_empty() {
                continue;
            }
            children.push(divider());
            children.push(self.render_list_header(p, provider));
            for account in &accounts {
                children.push(self.render_list_account(p, account, pane, hide, cx));
            }
        }
        v_flex()
            .id("accounts-list")
            .w(px(LIST_WIDTH))
            .flex_shrink_0()
            .p(px(6.0))
            .rounded(px(MODAL_RADIUS_SECTION))
            .border_1()
            .border_color(hsla(p.hairline))
            .bg(hsla(p.raised))
            .children(children)
            .into_any_element()
    }

    /// The provider's name over its accounts.
    ///
    /// CDXC:Settings 2026-11-05 DECISION:
    /// User picked "B. Today's list and detail, trimmed" for Settings > Accounts (Q1330): one Add account button above the list replaces each provider's Connection guide (?) and Add (+) buttons, and rarely used actions move into ⋯ menus. Supersedes the part of the Settings 2026-10-14 "List and detail" decision that put Add account and the guide in each provider's header.
    fn render_list_header(&mut self, p: &SettingsPalette, provider: &'static str) -> AnyElement {
        let label = provider_label(provider);
        h_flex()
            .w_full()
            .px(px(8.0))
            .pt(px(4.0))
            .pb(px(4.0))
            .gap(px(8.0))
            .items_center()
            .child(account_logo(p, provider, 14.0))
            .child(
                div()
                    .flex_1()
                    .text_size(px(13.0))
                    .line_height(px(18.0))
                    .font_weight(FontWeight::MEDIUM)
                    .text_color(hsla(p.foreground))
                    .child(label),
            )
            .into_any_element()
    }

    /// One saved account: its session badge, name, and the tightest usage bar, or Reconnect.
    fn render_list_account(
        &mut self,
        p: &SettingsPalette,
        account: &Account,
        pane: &Pane,
        hide: bool,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let id = account.id();
        let selected = *pane == Pane::Account(id.clone());
        let mark = div()
            .size(px(24.0))
            .flex()
            .items_center()
            .justify_center()
            .when(account.needs_sign_in(), |this| this.opacity(0.6))
            .when(account.user_disabled(), |this| this.opacity(0.35))
            .child(if account.usage_only() {
                account_logo(p, &account.provider(), 16.0)
            } else {
                account_mark(p, &account.provider(), &account_badge(account), 1.15)
            })
            .into_any_element();
        let signing_in = self
            .flows
            .get(&format!("reconnect:{id}"))
            .is_some_and(|flow| {
                flow.starting || flow.job.as_ref().is_some_and(super::connect::job_active)
            })
            || self.usage_sign_in_active_for(&id);
        let detail: AnyElement = if account.user_disabled() {
            h_flex()
                .child(status_tag(p, "Disabled", TagTone::Off))
                .into_any_element()
        } else if account.needs_sign_in() {
            if signing_in {
                div()
                    .text_color(hsla(p.muted))
                    .child("Waiting for sign-in…")
                    .into_any_element()
            } else {
                let reconnect_id = id.clone();
                let account = account.clone();
                h_flex()
                    .gap(px(6.0))
                    .items_center()
                    .child(settings_sized_button(
                        p,
                        SharedString::from(format!("account-{id}-list-reconnect")),
                        if account.synced_from().is_some() {
                            "Sign in on this computer"
                        } else {
                            "Reconnect"
                        },
                        None,
                        None,
                        SizedButtonVariant::Primary,
                        SizedButtonSize::Xs,
                        false,
                        None,
                        move |page: &mut Self, _window, cx| {
                            page.select_pane(Pane::Account(reconnect_id.clone()), cx);
                            if account.usage_only() {
                                page.start_usage_sign_in(
                                    &account.provider(),
                                    Some(account.id()),
                                    cx,
                                );
                            } else {
                                page.start_reconnect(&account, cx);
                            }
                        },
                        cx,
                    ))
                    .into_any_element()
            }
        } else {
            match account.tightest_percent() {
                Some(used) => h_flex()
                    .w_full()
                    .gap(px(6.0))
                    .items_center()
                    .child(usage_bar(p, used, 4.0))
                    .child(
                        div()
                            .flex_shrink_0()
                            .font_family(MODAL_MONO_FONT)
                            .text_size(px(10.5))
                            .text_color(hsla(p.muted))
                            .child(format!("{}%", used.round() as i64)),
                    )
                    .into_any_element(),
                None => div()
                    .text_color(hsla(p.muted))
                    .child("No usage yet")
                    .into_any_element(),
            }
        };
        let title = h_flex()
            .min_w_0()
            .gap(px(6.0))
            .items_center()
            .child(
                div()
                    .min_w_0()
                    .overflow_hidden()
                    .whitespace_nowrap()
                    .text_ellipsis()
                    .child(account_text(&account.name(), hide)),
            )
            .when(account.needs_sign_in(), |this| {
                this.child(
                    div()
                        .id(SharedString::from(format!("account-{id}-needs-sign-in")))
                        .flex_shrink_0()
                        .size(px(7.0))
                        .rounded_full()
                        .bg(hsla(modal_rgba(0xfbbf24, 0.95)))
                        .tooltip(tooltip_text("Needs sign-in")),
                )
            })
            .into_any_element();
        let select_id = id.clone();
        list_item_with_title(
            p,
            SharedString::from(format!("account-{id}-list")),
            selected,
            mark,
            title,
            Some(detail),
            move |page, cx| page.select_pane(Pane::Account(select_id.clone()), cx),
            cx,
        )
    }
}

/// A list row with a plain-text title.
#[allow(clippy::too_many_arguments)]
fn list_item(
    p: &SettingsPalette,
    id: SharedString,
    selected: bool,
    leading: AnyElement,
    title: SharedString,
    detail: Option<AnyElement>,
    on_select: impl Fn(&mut AccountsTab, &mut Context<AccountsTab>) + 'static,
    cx: &mut Context<AccountsTab>,
) -> AnyElement {
    let title = div()
        .min_w_0()
        .overflow_hidden()
        .whitespace_nowrap()
        .text_ellipsis()
        .child(title)
        .into_any_element();
    list_item_with_title(p, id, selected, leading, title, detail, on_select, cx)
}

/// A selectable list row: leading mark, title and an optional second line.
#[allow(clippy::too_many_arguments)]
fn list_item_with_title(
    p: &SettingsPalette,
    id: SharedString,
    selected: bool,
    leading: AnyElement,
    title: AnyElement,
    detail: Option<AnyElement>,
    on_select: impl Fn(&mut AccountsTab, &mut Context<AccountsTab>) + 'static,
    cx: &mut Context<AccountsTab>,
) -> AnyElement {
    let hover = p.foreground_alpha(0.04);
    h_flex()
        .id(id)
        .w_full()
        .min_h(px(40.0))
        .px(px(8.0))
        .py(px(7.0))
        .gap(px(10.0))
        .items_center()
        .rounded(px(MODAL_RADIUS_CONTROL))
        .cursor_pointer()
        .when(selected, |this| this.bg(hsla(p.foreground_alpha(0.08))))
        .when(!selected, |this| {
            this.hover(move |this| this.bg(hsla(hover)))
        })
        .on_click(cx.listener(move |page, _: &ClickEvent, _window, cx| on_select(page, cx)))
        .child(leading)
        .child(
            v_flex()
                .flex_1()
                .min_w_0()
                .gap(px(4.0))
                .child(
                    div()
                        .min_w_0()
                        .text_size(px(13.0))
                        .line_height(px(17.0))
                        .text_color(hsla(p.foreground))
                        .child(title),
                )
                .children(detail.map(|detail| {
                    div()
                        .min_w_0()
                        .text_size(px(11.5))
                        .line_height(px(15.0))
                        .child(detail)
                })),
        )
        .into_any_element()
}
