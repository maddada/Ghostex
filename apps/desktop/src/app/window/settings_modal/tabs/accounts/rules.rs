//! `DefaultAccountRow` and `PolicySettingRows` (accounts/manager.tsx and
//! accounts/policy-setting-rows.tsx (deleted 2026-10-01)): a provider's rules in the manager's
//! right pane: the account new sessions start on, what happens when an account hits its limit
//! (and which account to try first), retrying after temporary errors, and the saved-reset
//! switches.
use super::super::super::super::native_modal_kit::*;
use super::super::super::catalog::SettingOption;
use super::super::super::fields::{RowSpec, setting_row, settings_select, switch_control};
use super::super::super::palette::SettingsPalette;
use super::AccountsTab;
use super::data::{AccountsState, POLICY_PRIORITY_OPTIONS, new_session_rules, provider_label};
use super::editor::{DETAIL_CONTROL_WIDTH, DETAIL_INSET};
use super::widgets::{account_logo, account_text};
use gpui::prelude::FluentBuilder as _;
use gpui::{
    AnyElement, Context, FontWeight, InteractiveElement as _, IntoElement, ParentElement as _,
    SharedString, Styled as _, Window, div, px,
};
use gpui_component::{h_flex, v_flex};
use serde_json::{Value, json};

/// The merged limit menu's choices: (value, label, enabled, atLimit).
const LIMIT_CHOICES: [(&str, &str, bool, &str); 3] = [
    ("switch", "Switch accounts", true, "switch"),
    ("wait", "Wait for reset", true, "wait"),
    ("off", "Do nothing", false, "wait"),
];

impl AccountsTab {
    pub(crate) fn render_rules_pane(
        &mut self,
        p: &SettingsPalette,
        provider: &'static str,
        data: &AccountsState,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let busy = self.client.read(cx).busy;
        let hide = self.hide_emails(cx);
        let label = provider_label(provider);
        let policy = data.defaults(provider);
        let mut rows: Vec<AnyElement> =
            vec![self.render_new_session_row(p, provider, data, busy, hide, window, cx)];
        let mut limit_rows = self.render_limit_rows(p, provider, &policy, busy, window, cx);
        if provider == "claude" {
            // Under the limit choice and Try first, above Retry after temporary errors.
            let at = limit_rows.len().saturating_sub(1);
            limit_rows.insert(at, self.render_fable_fallback_row(p, &policy, cx));
        }
        rows.extend(limit_rows);
        rows.extend(self.render_auto_redeem_rows(p, provider, cx));
        let hairline = hsla(p.hairline);
        v_flex()
            .id(SharedString::from(format!("accounts-{provider}-rules-pane")))
            .w_full()
            .pt(px(DETAIL_INSET))
            .pb(px(6.0))
            .gap(px(8.0))
            .child(
                h_flex()
                    .px(px(DETAIL_INSET))
                    .gap(px(10.0))
                    .items_center()
                    .child(account_logo(p, provider, 20.0))
                    .child(
                        v_flex()
                            .child(
                                div()
                                    .text_size(px(16.0))
                                    .line_height(px(22.0))
                                    .font_weight(FontWeight::MEDIUM)
                                    .text_color(hsla(p.foreground))
                                    .child(format!("{label} rules")),
                            )
                            .child(
                                div()
                                    .text_size(px(12.5))
                                    .text_color(hsla(p.muted))
                                    .child(format!("For every {label} account. Existing sessions keep their own settings.")),
                            ),
                    ),
            )
            .child(
                v_flex()
                    .mx(px(DETAIL_INSET))
                    .children(rows.into_iter().enumerate().map(|(index, row)| {
                        div()
                            .w_full()
                            .when(index > 0, |this| this.border_t_1().border_color(hairline))
                            .child(row)
                    })),
            )
            .into_any_element()
    }

    /// New sessions start on: the automatic rules, then each saved account.
    ///
    /// CDXC:AgentProviders 2026-09-11 DECISION:
    /// User: the row lists the automatic rules first, Auto (recommended) on top as the default, then each saved account so one specific account can be pinned instead. Choices saved before these rules existed migrate to Auto.
    /// SEE-ALSO: server/src/accounts/default_account.rs.
    #[allow(clippy::too_many_arguments)]
    fn render_new_session_row(
        &mut self,
        p: &SettingsPalette,
        provider: &'static str,
        data: &AccountsState,
        busy: bool,
        hide: bool,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let label = provider_label(provider);
        let choice = data.new_session_choice(provider);
        let value = if choice["rule"] == "pinned" {
            choice["id"].as_str().unwrap_or_default().to_string()
        } else {
            choice["rule"].as_str().unwrap_or("auto").to_string()
        };
        let mut options: Vec<SettingOption> = new_session_rules()
            .iter()
            .map(|(rule, label)| SettingOption {
                label: label.clone(),
                value: rule.clone(),
            })
            .collect();
        options.extend(
            data.registered(provider)
                .iter()
                .filter(|account| !account.user_disabled() || account.id() == value)
                .map(|account| SettingOption {
                    label: account_text(&account.name(), hide),
                    value: account.id(),
                }),
        );
        let control = settings_select(
            self,
            p,
            SharedString::from(format!("accounts-{provider}-default-account")),
            &options,
            &value,
            Some(DETAIL_CONTROL_WIDTH),
            busy,
            Some("Accounts are being updated.".into()),
            move |page: &mut Self, next, _window, cx| {
                let choice = if new_session_rules().iter().any(|(rule, _)| *rule == next) {
                    json!({ "rule": next })
                } else {
                    json!({ "rule": "pinned", "id": next })
                };
                page.account_request(
                    json!({ "operation": "defaultAccount", "provider": provider, "choice": choice }),
                    None,
                    cx,
                );
            },
            window,
            cx,
        );
        setting_row(
            p,
            SharedString::from(format!("accounts-{provider}-default-account-row")),
            RowSpec::new("New sessions start on")
                .description(format!(
                    "Quick launch starts new {label} sessions with this account. The automatic rules only consider accounts with Auto-switch on: Auto weighs remaining limit against time to reset and picks the account that can absorb the most usage before any of its limits resets, Most limit remaining picks the account with the most limit left, Soonest reset the one whose limit resets first, Most used first keeps draining the account already in use, and Same as last session reuses the account of the last session."
                ))
                .inset_x(0.0),
            None,
            control,
            cx,
        )
    }

    /// When an account hits its limit (switch, wait or do nothing), Try first while switching, and
    /// Retry after temporary errors.
    ///
    /// CDXC:AgentProviders 2026-10-14 DECISION:
    /// User picked "One When an account hits its limit menu instead of a switch plus a choice". The Continue automatically switch and the When the account runs out choice are one menu: Switch accounts, Wait for reset, or Do nothing (auto-continue off), still saved as the policy's `enabled` and `atLimit`. Supersedes the Settings 2026-09-10 decision that drew the policy as a switch, a segmented control and a dropdown.
    fn render_limit_rows(
        &mut self,
        p: &SettingsPalette,
        provider: &'static str,
        policy: &Value,
        busy: bool,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Vec<AnyElement> {
        let enabled = policy["enabled"].as_bool() == Some(true);
        let at_limit = policy["atLimit"].as_str().unwrap_or("wait").to_string();
        let current = if !enabled {
            "off"
        } else if at_limit == "switch" {
            "switch"
        } else {
            "wait"
        };
        let save =
            move |page: &mut Self, patch: Vec<(&'static str, Value)>, cx: &mut Context<Self>| {
                let Some(data) = page.client.read(cx).data.clone() else {
                    return;
                };
                let mut next = data.defaults(provider);
                for (key, value) in patch {
                    next[key] = value;
                }
                page.account_request(
                    json!({ "operation": "defaults", "provider": provider, "policy": next }),
                    None,
                    cx,
                );
            };
        let options: Vec<SettingOption> = LIMIT_CHOICES
            .iter()
            .map(|(value, label, _, _)| SettingOption {
                label: label.to_string(),
                value: value.to_string(),
            })
            .collect();
        let mut rows: Vec<AnyElement> = vec![setting_row(
            p,
            SharedString::from(format!("accounts-{provider}-policy-limit-row")),
            RowSpec::new("When an account hits its limit")
                .description(
                    "Switch accounts moves the session to an account with Auto-switch on, and waits when every account is at its limit. Wait for reset picks up on the same account. Do nothing leaves the session stopped.",
                )
                .inset_x(0.0),
            None,
            settings_select(
                self,
                p,
                SharedString::from(format!("accounts-{provider}-policy-limit")),
                &options,
                current,
                Some(DETAIL_CONTROL_WIDTH),
                busy,
                Some("Accounts are being updated.".into()),
                move |page: &mut Self, next, _window, cx| {
                    if let Some((_, _, on, limit)) =
                        LIMIT_CHOICES.iter().find(|(value, ..)| *value == next)
                    {
                        let mut patch = vec![("enabled", json!(on))];
                        if *on {
                            patch.push(("atLimit", json!(limit)));
                        }
                        save(page, patch, cx);
                    }
                },
                window,
                cx,
            ),
            cx,
        )];
        if current == "switch" {
            let options: Vec<SettingOption> = POLICY_PRIORITY_OPTIONS
                .iter()
                .map(|(value, label)| SettingOption {
                    label: label.to_string(),
                    value: value.to_string(),
                })
                .collect();
            let priority = policy["priority"]
                .as_str()
                .unwrap_or("soonestReset")
                .to_string();
            rows.push(setting_row(
                p,
                SharedString::from(format!("accounts-{provider}-policy-priority-row")),
                RowSpec::new("Try first")
                    .description("Which account with Auto-switch on Ghostex tries first.")
                    .dependent()
                    .inset_x(0.0),
                None,
                settings_select(
                    self,
                    p,
                    SharedString::from(format!("accounts-{provider}-policy-priority")),
                    &options,
                    &priority,
                    Some(DETAIL_CONTROL_WIDTH),
                    busy,
                    Some("Accounts are being updated.".into()),
                    move |page: &mut Self, next, _window, cx| {
                        save(page, vec![("priority", json!(next))], cx)
                    },
                    window,
                    cx,
                ),
                cx,
            ));
        }
        // CDXC:Settings 2026-11-14 DECISION (AGENTS.md, Q3107): hidden while At a limit is Do
        // nothing, no longer greyed out.
        if enabled {
            rows.push(setting_row(
                p,
                SharedString::from(format!("accounts-{provider}-policy-retry")),
                RowSpec::new("Retry after temporary errors")
                    .description(
                        "A lost connection or server error retries once the network is back, after 5s, 15s, 1m, 2m and 5m, then after 5, 10, 20, 40, then every 60 minutes. Other temporary errors use the slower schedule. Login and permission requests need your attention. Stop cancels recovery for the current task.",
                    )
                    .inset_x(0.0),
                None,
                switch_control(
                    p,
                    SharedString::from(format!("accounts-{provider}-policy-retry-switch")),
                    "Retry after temporary errors",
                    policy["retryErrors"].as_bool() == Some(true),
                    busy,
                    Some("Accounts are being updated.".into()),
                    move |page: &mut Self, next, _window, cx| {
                        save(page, vec![("retryErrors", json!(next))], cx)
                    },
                    cx,
                ),
                cx,
            ));
        }
        rows
    }

    /// Continue on Opus when Fable runs out, a shared setting gxserver reads
    /// (server/src/accounts/model_fallback.rs).
    ///
    /// CDXC:AgentProviders 2026-11-13 DECISION:
    /// User: "we also need a new toggle that continues on any open Opus when Fable runs out on all accounts, in Settings for Claude accounts, since that's the most common case. If the user is using Fable then we switch to another account that has Fable usage of course." Claude only, on by default, and only meaningful while At a limit is Switch accounts, so it is dimmed otherwise.
    fn render_fable_fallback_row(
        &mut self,
        p: &SettingsPalette,
        policy: &Value,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        const KEY: &str = "claudeFableFallbackToOpus";
        let switching = policy["enabled"].as_bool() == Some(true)
            && policy["atLimit"].as_str() == Some("switch");
        let on = self.store.read(cx).bool(KEY);
        setting_row(
            p,
            SharedString::from("accounts-claude-fable-fallback"),
            RowSpec::new("Continue on Opus when Fable runs out")
                .description(
                    "When no account with Auto-switch on has Fable usage left, the session continues on Opus at the same effort, on an account with room. It stays on Opus until you pick Fable again.",
                )
                .dependent()
                .inset_x(0.0),
            None,
            switch_control(
                p,
                SharedString::from("accounts-claude-fable-fallback-switch"),
                "Continue on Opus when Fable runs out",
                on,
                !switching,
                (!switching).then(|| SharedString::from("Choose Switch accounts first.")),
                move |page: &mut Self, next: bool, _window: &mut Window, cx: &mut Context<Self>| {
                    let store = page.store.clone();
                    store.update(cx, |store, cx| store.update_setting(KEY, json!(next), cx));
                },
                cx,
            ),
            cx,
        )
    }

    /// The provider's Auto-redeem switches, shared settings gxserver reads
    /// (server/src/accounts/reset_watch.rs): using a reset 5 minutes before it expires, and, under
    /// it, also using one at a usage limit in its last 24 hours.
    fn render_auto_redeem_rows(
        &mut self,
        p: &SettingsPalette,
        provider: &'static str,
        cx: &mut Context<Self>,
    ) -> Vec<AnyElement> {
        let (key, at_limit_key): (&'static str, &'static str) = if provider == "codex" {
            (
                "codexAutoRedeemExpiringResets",
                "codexAutoRedeemResetsAtLimit",
            )
        } else {
            (
                "claudeAutoRedeemExpiringResets",
                "claudeAutoRedeemResetsAtLimit",
            )
        };
        let on = self.store.read(cx).bool(key);
        let at_limit = self.store.read(cx).bool(at_limit_key);
        let save = |key: &'static str| {
            move |page: &mut Self, next: bool, _window: &mut Window, cx: &mut Context<Self>| {
                let store = page.store.clone();
                store.update(cx, |store, cx| store.update_setting(key, json!(next), cx));
            }
        };
        let mut rows = vec![setting_row(
                p,
                SharedString::from(format!("accounts-{provider}-auto-redeem")),
                RowSpec::new("Auto-redeem expiring resets")
                    .description(
                        "Ghostex automatically uses a banked reset 5 minutes before it expires, so it isn't lost.",
                    )
                    .inset_x(0.0),
                None,
                switch_control(
                    p,
                    SharedString::from(format!("accounts-{provider}-auto-redeem-switch")),
                    "Auto-redeem expiring resets",
                    on,
                    false,
                    None,
                    save(key),
                    cx,
                ),
                cx,
            )];
        // CDXC:Settings 2026-11-14 DECISION (AGENTS.md, Q3107): hidden while Auto-redeem expiring
        // resets is off, no longer greyed out.
        if on {
            rows.push(setting_row(
                p,
                SharedString::from(format!("accounts-{provider}-auto-redeem-at-limit")),
                RowSpec::new("Also at a limit")
                    .description(
                        "When you hit a usage limit and a banked reset expires within 24 hours, Ghostex automatically uses it right away instead of waiting for its last 5 minutes.",
                    )
                    .dependent()
                    .inset_x(0.0),
                None,
                switch_control(
                    p,
                    SharedString::from(format!("accounts-{provider}-auto-redeem-at-limit-switch")),
                    "Also use it when I hit a limit",
                    at_limit,
                    false,
                    None,
                    save(at_limit_key),
                    cx,
                ),
                cx,
            ));
        }
        rows
    }
}
