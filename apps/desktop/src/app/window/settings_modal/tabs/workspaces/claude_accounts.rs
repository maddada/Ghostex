//! Per-workspace Claude accounts selector and `claude.ai` connectors status + setup guide inside
//! Settings > Workspaces.
//!
//! Lets each workspace pick one or several Claude accounts (saved in `claudeAccountIds`, with
//! `claudeAccountId` kept in sync for older clients/phones), shows each account's detected
//! `claude.ai` connectors from its local `.claude.json` profile (`claudeAiMcpEverConnected`),
//! gently highlights missing Slack or Linear, and opens `claude.ai/settings/connectors` in that
//! account's isolated Ghostex Browser profile.

use super::super::super::fields::{settings_checkbox, settings_icon, tooltip_text};
use super::super::super::super::native_modal_kit::{a11y_toggled, hsla, PressExt as _};
use super::*;
use gpui::{FontWeight, rgba};
use gpui_component::v_flex;

#[derive(Clone, Debug, Default)]
pub(crate) struct ClaudeAiConnectorsInfo {
    pub(crate) detected: bool,
    pub(crate) apps: Vec<String>,
    pub(crate) slack: bool,
    pub(crate) linear: bool,
}

#[derive(Clone, Debug)]
pub(crate) struct ClaudeAccountInfo {
    pub(crate) id: String,
    pub(crate) slot: String,
    pub(crate) label: String,
    pub(crate) email: String,
    pub(crate) user_disabled: bool,
    pub(crate) eligible: bool,
    pub(crate) signed_in: bool,
    pub(crate) connectors: ClaudeAiConnectorsInfo,
}

impl ClaudeAccountInfo {
    fn auto_selectable(&self) -> bool {
        !self.user_disabled && self.eligible && self.signed_in
    }
}

/// Parses registered Claude accounts from `/api/agentAccounts`, ordered by numeric slot.
pub(super) fn parse_claude_accounts(result: &Value) -> Vec<ClaudeAccountInfo> {
    let mut accounts: Vec<(f64, ClaudeAccountInfo)> = result
        .get("accounts")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .filter(|account| {
            account.get("provider").and_then(Value::as_str) == Some("claude")
                && account.get("registered").and_then(Value::as_bool) == Some(true)
        })
        .filter_map(|account| {
            let id = account.get("id").and_then(Value::as_str)?.to_string();
            let slot = text(account, "selector");
            let email = text(account, "email");
            let label = [text(account, "name"), email.clone()]
                .into_iter()
                .map(|value| value.trim().to_string())
                .find(|value| !value.is_empty())
                .unwrap_or_else(|| format!("Account {slot}"));
            let user_disabled =
                account.get("userDisabled").and_then(Value::as_bool) == Some(true);
            let eligible = account.get("eligible").and_then(Value::as_bool) != Some(false);
            let status = text(account, "status");
            let login_expired = account
                .get("loginExpiredAt")
                .and_then(Value::as_str)
                .is_some_and(|v| !v.trim().is_empty());
            let disabled_by_error = account.get("disabled").is_some_and(|v| !v.is_null());
            let signed_in = !slot.trim().is_empty()
                && !login_expired
                && !disabled_by_error
                && !matches!(
                    status.as_str(),
                    "loginRequired" | "disabled" | "identityChanged"
                );
            let connectors_val = account.get("claudeAiConnectors");
            let connectors = ClaudeAiConnectorsInfo {
                detected: connectors_val
                    .and_then(|value| value.get("detected"))
                    .and_then(Value::as_bool)
                    == Some(true),
                apps: connectors_val
                    .and_then(|value| value.get("apps"))
                    .and_then(Value::as_array)
                    .into_iter()
                    .flatten()
                    .filter_map(Value::as_str)
                    .map(str::to_string)
                    .collect(),
                slack: connectors_val
                    .and_then(|value| value.get("slack"))
                    .and_then(Value::as_bool)
                    == Some(true),
                linear: connectors_val
                    .and_then(|value| value.get("linear"))
                    .and_then(Value::as_bool)
                    == Some(true),
            };
            Some((
                slot.parse::<f64>().unwrap_or(f64::MAX),
                ClaudeAccountInfo {
                    id,
                    slot,
                    label,
                    email,
                    user_disabled,
                    eligible,
                    signed_in,
                    connectors,
                },
            ))
        })
        .collect();
    accounts.sort_by(|left, right| {
        left.0
            .partial_cmp(&right.0)
            .unwrap_or(std::cmp::Ordering::Equal)
    });
    accounts.into_iter().map(|(_, info)| info).collect()
}

/// Selected Claude account ids on a workspace document, falling back to legacy `claudeAccountId`.
pub(super) fn selected_claude_account_ids(workspace: &Value) -> Vec<String> {
    let mut ids: Vec<String> = workspace
        .get("claudeAccountIds")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .filter_map(Value::as_str)
        .map(str::trim)
        .filter(|id| !id.is_empty())
        .map(str::to_string)
        .collect();
    if ids.is_empty() {
        let single = text(workspace, "claudeAccountId");
        if !single.trim().is_empty() {
            ids.push(single);
        }
    }
    ids
}

impl WorkspacesTab {
    pub(super) fn reload_claude_accounts(&mut self, cx: &mut Context<Self>) {
        if !self.rpc_available(cx) {
            return;
        }
        self.reloading_accounts = true;
        cx.notify();
        let this = cx.weak_entity();
        store_gxserver_rpc(
            &self.store.clone(),
            "/api/agentAccounts",
            json!({ "operation": "list", "cachedOnly": true }),
            RPC_TIMEOUT,
            move |result, cx| {
                let _ = this.update(cx, |page, cx| {
                    page.reloading_accounts = false;
                    if let Ok(result) = result {
                        page.claude_accounts = parse_claude_accounts(&result);
                    }
                    cx.notify();
                });
            },
            cx,
        );
    }

    fn toggle_workspace_claude_account(
        &mut self,
        workspace_id: &str,
        account_id: &str,
        cx: &mut Context<Self>,
    ) {
        let current = self
            .workspace(workspace_id)
            .map(selected_claude_account_ids)
            .unwrap_or_default();
        let mut next = current;
        if let Some(pos) = next.iter().position(|id| id == account_id) {
            next.remove(pos);
        } else {
            next.push(account_id.to_string());
        }
        let patch = if next.is_empty() {
            json!({ "claudeAccountIds": Value::Null, "claudeAccountId": Value::Null })
        } else {
            json!({
                "claudeAccountIds": next,
                "claudeAccountId": next.first().cloned(),
            })
        };
        self.update_workspace(workspace_id, patch, cx);
    }

    pub(super) fn claude_accounts_rows(
        &mut self,
        p: &SettingsPalette,
        workspace: &Value,
        cx: &mut Context<Self>,
    ) -> Vec<AnyElement> {
        let workspace_id = text(workspace, "workspaceId");
        let selected_ids = selected_claude_account_ids(workspace);
        let hide_emails = {
            let store = self.store.read(cx);
            store.bool("hideAccountEmails") || store.bool("screenshotMode")
        };
        let active_accounts: Vec<ClaudeAccountInfo> = self
            .claude_accounts
            .iter()
            .filter(|account| !account.user_disabled)
            .cloned()
            .collect();

        let mut rows = Vec::new();
        rows.push(self.claude_accounts_picker_row(
            p,
            &workspace_id,
            &selected_ids,
            &active_accounts,
            hide_emails,
            cx,
        ));
        rows.push(self.claude_connectors_row(
            p,
            &workspace_id,
            &selected_ids,
            &active_accounts,
            hide_emails,
            cx,
        ));
        rows
    }

    fn claude_accounts_picker_row(
        &mut self,
        p: &SettingsPalette,
        workspace_id: &str,
        selected_ids: &[String],
        active_accounts: &[ClaudeAccountInfo],
        hide_emails: bool,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let summary = if selected_ids.is_empty() {
            "Following Settings > Accounts (Account for new sessions)".to_string()
        } else if selected_ids.len() == 1 {
            "1 Claude account pinned for this workspace".to_string()
        } else {
            format!(
                "{} Claude accounts in rotation for this workspace (ranked by your Accounts rule)",
                selected_ids.len()
            )
        };

        let on_bg = p.foreground_alpha(0.11);
        let on_edge = p.foreground_alpha(0.32);
        let hover_bg = p.foreground_alpha(0.06);
        let hairline = p.hairline;
        let fg = p.foreground;
        let muted = p.muted;

        let control = if active_accounts.is_empty() {
            div()
                .text_size(px(12.5))
                .text_color(hsla(muted))
                .child("No active Claude accounts saved yet. Add Claude accounts in Settings > Accounts.")
                .into_any_element()
        } else {
            let clear_workspace = workspace_id.to_string();
            v_flex()
                .w_full()
                .gap(px(8.0))
                .child(
                    h_flex()
                        .w_full()
                        .flex_wrap()
                        .gap(px(8.0))
                        .children(active_accounts.iter().map(|account| {
                            let selected = selected_ids.iter().any(|id| id == &account.id);
                            let selectable = account.auto_selectable();
                            let disabled = !selectable && !selected;
                            let status_badge: Option<&'static str> = if !account.signed_in {
                                Some("Sign in")
                            } else if !account.eligible {
                                Some("Manual")
                            } else {
                                None
                            };
                            let disabled_tip: Option<String> = if !account.signed_in {
                                Some(
                                    "Sign in to this account on this computer in Settings > Accounts before using it in a workspace pool."
                                        .to_string(),
                                )
                            } else if !account.eligible {
                                Some(
                                    "Manual-only accounts are never picked automatically. Turn on Automatic in Settings > Accounts to use it here."
                                        .to_string(),
                                )
                            } else {
                                None
                            };
                            let display_name =
                                super::super::accounts::widgets::account_text(&account.label, hide_emails);
                            let target_ws = workspace_id.to_string();
                            let target_id = account.id.clone();
                            let mut chip = div()
                                .id(SharedString::from(format!(
                                    "workspace-claude-pick-{workspace_id}-{}",
                                    account.id
                                )))
                                .role(gpui::Role::CheckBox)
                                .aria_label(display_name.clone())
                                .aria_toggled(a11y_toggled(selected))
                                .flex()
                                .items_center()
                                .gap(px(8.0))
                                .h(px(30.0))
                                .px(px(10.0))
                                .rounded(px(8.0))
                                .border_1()
                                .border_color(hsla(if selected { on_edge } else { hairline }))
                                .when(selected, |chip| chip.bg(hsla(on_bg)))
                                .when(!disabled, |chip| {
                                    chip.hover(move |chip| chip.bg(hsla(hover_bg)))
                                        .cursor_pointer()
                                })
                                .when(disabled, |chip| chip.opacity(0.55))
                                .text_size(px(12.5))
                                .text_color(hsla(if disabled { muted } else { fg }))
                                .child(settings_checkbox(p, selected, disabled))
                                .child(display_name)
                                .when_some(status_badge, |chip, badge| {
                                    chip.child(
                                        div()
                                            .px(px(5.0))
                                            .py(px(1.0))
                                            .rounded(px(4.0))
                                            .border_1()
                                            .border_color(hsla(hairline))
                                            .text_size(px(10.5))
                                            .text_color(hsla(muted))
                                            .child(badge),
                                    )
                                });
                            if let Some(tip) = disabled_tip {
                                chip = chip.tooltip(tooltip_text(tip));
                            }
                            if !disabled {
                                chip = chip.on_press(cx, move |page, _window, cx| {
                                    page.toggle_workspace_claude_account(
                                        &target_ws, &target_id, cx,
                                    );
                                });
                            }
                            chip
                        })),
                )
                .child(
                    h_flex()
                        .w_full()
                        .justify_between()
                        .items_center()
                        .gap(px(8.0))
                        .child(
                            div()
                                .text_size(px(11.5))
                                .text_color(hsla(muted))
                                .child(summary),
                        )
                        .when(!selected_ids.is_empty(), |row| {
                            row.child(settings_button(
                                p,
                                SharedString::from(format!(
                                    "workspace-claude-clear-{workspace_id}"
                                )),
                                "Use global default",
                                None,
                                ButtonVariant::Ghost,
                                false,
                                None,
                                move |page: &mut Self, _window, cx| {
                                    page.update_workspace(
                                        &clear_workspace,
                                        json!({
                                            "claudeAccountIds": Value::Null,
                                            "claudeAccountId": Value::Null
                                        }),
                                        cx,
                                    );
                                },
                                cx,
                            ))
                        }),
                )
                .into_any_element()
        };

        setting_row(
            p,
            format!("workspace-claude-{workspace_id}"),
            RowSpec::new("Claude accounts")
                .description(
                    "Pick one or more Claude accounts for this workspace's projects. Leave none selected to follow Settings > Accounts. Phone and remote sessions follow this choice automatically.",
                )
                .wide(),
            None,
            control,
            cx,
        )
    }

    fn claude_connectors_row(
        &mut self,
        p: &SettingsPalette,
        workspace_id: &str,
        selected_ids: &[String],
        active_accounts: &[ClaudeAccountInfo],
        hide_emails: bool,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let shown_accounts: Vec<&ClaudeAccountInfo> = if selected_ids.is_empty() {
            active_accounts.iter().collect()
        } else {
            active_accounts
                .iter()
                .filter(|account| selected_ids.iter().any(|id| id == &account.id))
                .collect()
        };

        let missing_count = shown_accounts
            .iter()
            .filter(|account| {
                !account.connectors.detected
                    || !account.connectors.slack
                    || !account.connectors.linear
            })
            .count();

        let header_note = if shown_accounts.is_empty() {
            "Add a Claude account in Settings > Accounts to see its claude.ai connectors."
                .to_string()
        } else if missing_count == 0 {
            format!(
                "Slack and Linear last seen connected on all {} account(s) in this workspace.",
                shown_accounts.len()
            )
        } else {
            format!(
                "{} of {} account(s) missing Slack or Linear on claude.ai.",
                missing_count,
                shown_accounts.len()
            )
        };

        let good_color = if p.light {
            rgba(0x15803dff)
        } else {
            rgba(0x4ade80ff)
        };
        let good_bg = if p.light {
            rgba(0x15803d14)
        } else {
            rgba(0x4ade8018)
        };
        let good_border = if p.light {
            rgba(0x15803d40)
        } else {
            rgba(0x4ade8045)
        };
        let warn_color = if p.light {
            rgba(0xb45309ff)
        } else {
            rgba(0xfbbf24ff)
        };
        let warn_bg = if p.light {
            rgba(0xb4530916)
        } else {
            rgba(0xfbbf2418)
        };
        let warn_border = if p.light {
            rgba(0xb4530945)
        } else {
            rgba(0xfbbf2448)
        };

        let mut list = v_flex().w_full().gap(px(8.0));

        // Top summary + Refresh button.
        list = list.child(
            h_flex()
                .w_full()
                .justify_between()
                .items_center()
                .gap(px(8.0))
                .child(
                    div()
                        .text_size(px(12.0))
                        .text_color(hsla(if missing_count > 0 && !shown_accounts.is_empty() {
                            warn_color
                        } else {
                            p.muted
                        }))
                        .child(header_note),
                )
                .child(settings_button(
                    p,
                    SharedString::from(format!("workspace-connectors-refresh-{workspace_id}")),
                    if self.reloading_accounts {
                        "Refreshing..."
                    } else {
                        "Refresh"
                    },
                    Some("modals/settings/refresh.svg"),
                    ButtonVariant::Ghost,
                    self.reloading_accounts || !self.rpc_available(cx),
                    None,
                    |page: &mut Self, _window, cx| page.reload_claude_accounts(cx),
                    cx,
                )),
        );

        // One card per account in this workspace.
        for account in shown_accounts {
            let display_name = super::super::accounts::widgets::account_text(&account.label, hide_emails);
            let connectors = &account.connectors;
            let missing_slack = !connectors.detected || !connectors.slack;
            let missing_linear = !connectors.detected || !connectors.linear;
            let has_missing = missing_slack || missing_linear;

            let other_apps: Vec<String> = connectors
                .apps
                .iter()
                .filter(|app| !is_slack_or_linear_app(app))
                .cloned()
                .collect();

            let mut badges = h_flex().flex_wrap().gap(px(6.0)).items_center();

            // Slack pill.
            badges = badges.child(connector_pill(
                if connectors.slack {
                    "Slack"
                } else if connectors.detected {
                    "Slack missing"
                } else {
                    "Slack not verified"
                },
                connectors.slack,
                good_color,
                good_bg,
                good_border,
                warn_color,
                warn_bg,
                warn_border,
            ));

            // Linear pill.
            badges = badges.child(connector_pill(
                if connectors.linear {
                    "Linear"
                } else if connectors.detected {
                    "Linear missing"
                } else {
                    "Linear not verified"
                },
                connectors.linear,
                good_color,
                good_bg,
                good_border,
                warn_color,
                warn_bg,
                warn_border,
            ));

            // Other connected apps (capped cleanly).
            let shown_other = other_apps.len().min(4);
            for app in other_apps.iter().take(shown_other) {
                badges = badges.child(
                    div()
                        .px(px(7.0))
                        .py(px(2.0))
                        .rounded(px(99.0))
                        .border_1()
                        .border_color(hsla(p.hairline))
                        .bg(hsla(p.surface))
                        .text_size(px(11.0))
                        .text_color(hsla(p.muted))
                        .child(app.clone()),
                );
            }
            if other_apps.len() > shown_other {
                badges = badges.child(
                    div()
                        .text_size(px(11.0))
                        .text_color(hsla(p.muted))
                        .child(format!("+{} more", other_apps.len() - shown_other)),
                );
            }

            let open_id = account.id.clone();
            let open_name = account.label.clone();
            let open_email = account.email.clone();
            let button_label = if has_missing {
                "Connect on claude.ai"
            } else {
                "Manage"
            };

            list = list.child(
                h_flex()
                    .w_full()
                    .justify_between()
                    .items_center()
                    .gap(px(12.0))
                    .px(px(12.0))
                    .py(px(10.0))
                    .rounded(px(8.0))
                    .border_1()
                    .border_color(hsla(if has_missing { warn_border } else { p.hairline }))
                    .bg(hsla(if has_missing {
                        warn_bg
                    } else {
                        p.surface
                    }))
                    .child(
                        v_flex()
                            .flex_1()
                            .min_w_0()
                            .gap(px(5.0))
                            .child(
                                h_flex()
                                    .gap(px(6.0))
                                    .items_center()
                                    .child(
                                        div()
                                            .text_size(px(12.5))
                                            .font_weight(FontWeight::MEDIUM)
                                            .text_color(hsla(p.foreground))
                                            .child(display_name),
                                    )
                                    .when(!account.slot.is_empty(), |row| {
                                        row.child(
                                            div()
                                                .text_size(px(11.0))
                                                .text_color(hsla(p.muted))
                                                .child(format!("#{}", account.slot)),
                                        )
                                    }),
                            )
                            .child(badges)
                            .when(!connectors.detected, |col| {
                                col.child(
                                    div()
                                        .text_size(px(11.0))
                                        .text_color(hsla(p.muted))
                                        .child("Not seen in local Claude profile yet. Connect on claude.ai, start a Claude session on this account, then click Refresh."),
                                )
                            }),
                    )
                    .child(settings_button(
                        p,
                        SharedString::from(format!(
                            "workspace-connectors-open-{workspace_id}-{}",
                            account.id
                        )),
                        button_label,
                        Some("modals/settings/external-link.svg"),
                        if has_missing {
                            ButtonVariant::Default
                        } else {
                            ButtonVariant::Outline
                        },
                        false,
                        None,
                        move |page: &mut Self, _window, cx| {
                            post_store_message(
                                &page.store,
                                json!({
                                    "type": "openClaudeAccountConnectors",
                                    "accountId": open_id,
                                    "name": open_name,
                                    "email": open_email,
                                }),
                                cx,
                            );
                        },
                        cx,
                    )),
            );
        }

        // Concise setup guide & CLIProxyAPI caveat.
        list = list.child(
            v_flex()
                .w_full()
                .gap(px(4.0))
                .px(px(12.0))
                .py(px(10.0))
                .rounded(px(8.0))
                .border_1()
                .border_color(hsla(p.hairline))
                .bg(hsla(p.surface))
                .child(
                    h_flex()
                        .gap(px(6.0))
                        .items_center()
                        .child(settings_icon("modals/settings/info-circle.svg", 13.0, p.muted))
                        .child(
                            div()
                                .text_size(px(12.0))
                                .font_weight(FontWeight::MEDIUM)
                                .text_color(hsla(p.foreground))
                                .child("Connecting Slack and Linear via claude.ai"),
                        ),
                )
                .child(guide_line(
                    p,
                    "1. Each Claude login keeps its own claude.ai connectors. Click Connect on claude.ai above to open claude.ai/settings/connectors in that account's isolated Ghostex Browser profile.",
                ))
                .child(guide_line(
                    p,
                    "2. Connect Slack and Linear inside claude.ai, start a Claude session on that account so Claude Code refreshes its local profile, then click Refresh. Status shows apps connected at some point (disconnecting in claude.ai may not show here until Claude refreshes its local profile).",
                ))
                .child(guide_line(
                    p,
                    "3. CLIProxyAPI hub sessions do not carry claude.ai connectors, and claude.ai connectors only load in Claude Code (not Codex or Antigravity). Workspace instructions, accounts and connector status stay on this workspace's computer (no multi-PC sync).",
                )),
        );

        setting_row(
            p,
            format!("workspace-claude-connectors-{workspace_id}"),
            RowSpec::new("claude.ai connectors")
                .description(
                    "Slack, Linear and other apps connected at some point on claude.ai per Claude account (read from each account's local Claude profile; disconnecting in claude.ai may not show here until Claude refreshes its profile). No Linear API key or company Slack bot needed.",
                )
                .wide(),
            None,
            list.into_any_element(),
            cx,
        )
    }
}

fn is_slack_or_linear_app(app: &str) -> bool {
    app.split(|ch: char| !ch.is_ascii_alphanumeric())
        .any(|word| word.eq_ignore_ascii_case("slack") || word.eq_ignore_ascii_case("linear"))
}

#[allow(clippy::too_many_arguments)]
fn connector_pill(
    label: &'static str,
    connected: bool,
    good_color: gpui::Rgba,
    good_bg: gpui::Rgba,
    good_border: gpui::Rgba,
    warn_color: gpui::Rgba,
    warn_bg: gpui::Rgba,
    warn_border: gpui::Rgba,
) -> AnyElement {
    let (fg, bg, border) = if connected {
        (good_color, good_bg, good_border)
    } else {
        (warn_color, warn_bg, warn_border)
    };
    h_flex()
        .gap(px(5.0))
        .items_center()
        .px(px(8.0))
        .py(px(2.0))
        .rounded(px(99.0))
        .border_1()
        .border_color(hsla(border))
        .bg(hsla(bg))
        .child(div().size(px(6.0)).rounded_full().bg(hsla(fg)))
        .child(
            div()
                .text_size(px(11.0))
                .font_weight(FontWeight::MEDIUM)
                .text_color(hsla(fg))
                .child(label),
        )
        .into_any_element()
}

fn guide_line(p: &SettingsPalette, text: &'static str) -> AnyElement {
    div()
        .text_size(px(11.5))
        .line_height(px(16.0))
        .text_color(hsla(p.muted))
        .child(text)
        .into_any_element()
}
