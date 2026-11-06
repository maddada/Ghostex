//! `AccountEditor` (accounts/manager.tsx (deleted 2026-10-01)): the selected account in the
//! manager's right pane. Its header (session badge, name, email, sessions, the Auto-switch or Needs
//! sign-in tag), Reconnect and its sign-in flow, the usage bars, then General-page rows: Name,
//! Auto-switch, Show usage in the sidebar, and under Advanced the session badge and the account
//! number swap; Sign in again and Remove (with its confirmation) close the pane.
//!
//! CDXC:AgentProviders 2026-10-01 DECISION:
//! User: drop the Save changes button and apply edits right away. The name and indicator save when
//! their field loses focus or takes Enter (and when the pane closes), automatic switching saves
//! when it is flipped, so Cancel, which only discarded unsaved edits, is gone too. Removing an
//! account and swapping slots still ask for their own confirmation click. Every button here is the
//! filled Settings button (CDXC:AppModal 2026-10-14 on `neutral_button_fills`).
//!
//! CDXC:AgentProviders 2026-10-14 DECISION:
//! User picked "Hide the account number (Swap slots) under Advanced" and "Hide the session badge (Account indicator) under Advanced, with its preview inline", and asked to "align the controls to the right side and text to the left in most cases (like we do in the General page of Settings)". The pane's rows are General-page rows with every text field and menu the same width; the badge's preview sits beside its field, so the separate Session icon preview row is gone.
use super::super::super::super::native_modal_kit::*;
use super::super::super::catalog::SettingOption;
use super::super::super::fields::{
    ButtonVariant, FieldStates, RowSpec, SizedButtonSize, SizedButtonVariant, setting_row,
    settings_button, settings_icon, settings_select, settings_sized_button, settings_text_input,
    switch_control, tooltip_text,
};
use super::super::super::palette::SettingsPalette;
use super::data::{Account, helper_label, normalize_account_indicator_input};
use super::widgets::{TagTone, account_badge, account_mark, account_text, status_tag, usage_line};
use super::{AccountsTab, Pane};
use gpui::prelude::FluentBuilder as _;
use gpui::{
    AnyElement, ClickEvent, Context, Entity, FontWeight, InteractiveElement as _, IntoElement,
    ParentElement as _, SharedString, StatefulInteractiveElement as _, Styled as _, Window, div,
    px,
};
use gpui_component::input::{InputEvent, InputState};
use gpui_component::{h_flex, v_flex};
use serde_json::json;

/// The width of every text field and menu in the detail pane.
pub(crate) const DETAIL_CONTROL_WIDTH: f32 = 184.0;
/// The detail pane's side padding; its rows use it as their inset.
pub(crate) const DETAIL_INSET: f32 = 16.0;

/// The pane's own state (its React `useState`s).
#[derive(Clone, Debug)]
pub(crate) struct EditorDraft {
    pub(crate) name: String,
    pub(crate) indicator: String,
    pub(crate) eligible: bool,
    pub(crate) remove: bool,
    pub(crate) advanced: bool,
    pub(crate) swap_target: String,
}

impl AccountsTab {
    pub(crate) fn editor_draft(&mut self, account: &Account) -> EditorDraft {
        self.editors
            .entry(account.id())
            .or_insert_with(|| EditorDraft {
                name: account.name(),
                indicator: account.indicator(),
                eligible: account.eligible(),
                remove: false,
                advanced: false,
                swap_target: String::new(),
            })
            .clone()
    }

    pub(crate) fn update_editor(
        &mut self,
        id: &str,
        cx: &mut Context<Self>,
        apply: impl FnOnce(&mut EditorDraft),
    ) {
        if let Some(draft) = self.editors.get_mut(id) {
            apply(draft);
            cx.notify();
        }
    }

    /// Saves the pane's name, badge and automatic switching when they differ from the saved
    /// account. A blank name goes back to the saved one instead.
    pub(crate) fn commit_editor(&mut self, id: &str, cx: &mut Context<Self>) {
        let Some(draft) = self.editors.get(id).cloned() else {
            return;
        };
        let Some(account) = self.client.read(cx).data.as_ref().and_then(|data| {
            data.accounts()
                .into_iter()
                .find(|account| account.id() == id)
        }) else {
            return;
        };
        if draft.name.trim().is_empty() {
            self.update_editor(id, cx, |draft| draft.name = account.name());
            return;
        }
        if draft.name.trim() == account.name()
            && draft.indicator == account.indicator()
            && draft.eligible == account.eligible()
        {
            return;
        }
        self.account_request(
            json!({
                "operation": "update",
                "id": id,
                "name": draft.name.trim(),
                "color": account.color(),
                "eligible": draft.eligible,
                "indicator": draft.indicator,
            }),
            None,
            cx,
        );
    }

    /// Saves the pane when `input` loses focus or takes Enter (subscribed once per input).
    pub(crate) fn commit_on_blur(
        &mut self,
        input_id: &SharedString,
        input: &Entity<InputState>,
        id: &str,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if !self.commit_inputs.insert(input_id.clone()) {
            return;
        }
        let id = id.to_string();
        let subscription = cx.subscribe_in(
            input,
            window,
            move |page: &mut Self, _input, event: &InputEvent, _window, cx| {
                if matches!(event, InputEvent::Blur | InputEvent::PressEnter { .. }) {
                    page.commit_editor(&id, cx);
                }
            },
        );
        self.fields.subscriptions.push(subscription);
    }

    /// `setTitlebar`, then `accountTitlebarChanged` for the sidebar's usage strip.
    pub(crate) fn set_sidebar_usage(
        &mut self,
        account: &Account,
        shown: bool,
        cx: &mut Context<Self>,
    ) {
        let mut changed = account.raw.clone();
        changed["showInTitlebar"] = json!(shown);
        let store = self.store.clone();
        self.account_request(
            json!({ "operation": "setTitlebar", "id": account.id(), "shown": shown }),
            Some(Box::new(move |ok, cx| {
                if ok {
                    super::super::super::store::store_host_message(
                        &store,
                        json!({ "type": "accountTitlebarChanged", "machineId": "local", "account": changed }),
                        cx,
                    );
                }
            })),
            cx,
        );
    }

    pub(crate) fn render_account_detail(
        &mut self,
        p: &SettingsPalette,
        account: &Account,
        accounts: &[Account],
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let hide = self.hide_emails(cx);
        let busy = self.client.read(cx).busy;
        let id = account.id();
        let provider: &'static str = if account.provider() == "claude" {
            "claude"
        } else {
            "codex"
        };
        let draft = self.editor_draft(account);
        let flow_key = format!("reconnect:{id}");
        let flow_started = self.flow_started(&flow_key);
        let connected = self
            .connected
            .as_ref()
            .is_some_and(|(shown, _)| *shown == id);
        let more = self.render_account_menu(p, account, cx);
        let mut blocks: Vec<AnyElement> =
            vec![self.render_detail_header(p, account, &draft, connected, hide, more)];
        if account.user_disabled() {
            blocks.push(
                div()
                    .mx(px(DETAIL_INSET))
                    .p(px(12.0))
                    .rounded(px(MODAL_RADIUS_CONTROL))
                    .border_1()
                    .border_color(hsla(p.hairline))
                    .bg(hsla(p.foreground_alpha(0.03)))
                    .text_size(px(12.5))
                    .line_height(px(17.0))
                    .text_color(hsla(p.muted))
                    .child(format!(
                        "This account is paused. Ghostex keeps it saved but doesn't start sessions on it, switch to it, read its usage or ask you to sign in.{}",
                        match account.session_count() {
                            0 => String::new(),
                            1 => " 1 session still uses it and keeps running.".to_string(),
                            count => format!(" {count} sessions still use it and keep running."),
                        }
                    ))
                    .into_any_element(),
            );
        } else if flow_started {
            let flow = self.render_connect_flow(
                p,
                flow_key,
                provider,
                Some(account.clone()),
                None,
                false,
                false,
                window,
                cx,
            );
            blocks.push(div().px(px(DETAIL_INSET)).child(flow).into_any_element());
        } else if account.needs_sign_in() {
            let reconnect_account = account.clone();
            blocks.push(
                v_flex()
                    .mx(px(DETAIL_INSET))
                    .gap(px(10.0))
                    .p(px(12.0))
                    .rounded(px(MODAL_RADIUS_CONTROL))
                    .border_1()
                    .border_color(hsla(modal_rgba(0xfbbf24, 0.3)))
                    .bg(hsla(modal_rgba(0xfbbf24, 0.05)))
                    .child(
                        div()
                            .text_size(px(12.5))
                            .line_height(px(17.0))
                            .text_color(hsla(if p.light {
                                gpui::rgb(0xb45309)
                            } else {
                                gpui::rgb(0xfcd34d)
                            }))
                            .child(match account.synced_from() {
                                // CDXC:SettingsSync 2026-11-07 WHY in gx-core settings_sync/lists.rs: logins stay on each computer.
                                Some(from) => format!(
                                    "Added from {from}. Logins stay on each computer, so sign in on this one once to use it here."
                                ),
                                None => format!(
                                    "Its saved login stopped working, so Ghostex can't read its usage or switch to it.{}",
                                    sessions_sentence(account.session_count())
                                ),
                            }),
                    )
                    .child(
                        h_flex().child(settings_sized_button(
                            p,
                            SharedString::from(format!("account-{id}-reconnect")),
                            if account.synced_from().is_some() {
                                "Sign in on this computer"
                            } else {
                                "Reconnect"
                            },
                            None,
                            None,
                            SizedButtonVariant::Primary,
                            SizedButtonSize::Default,
                            false,
                            None,
                            move |page: &mut Self, _window, cx| {
                                page.start_reconnect(&reconnect_account, cx)
                            },
                            cx,
                        )),
                    )
                    .into_any_element(),
            );
        } else if connected {
            blocks.push(
                div()
                    .px(px(DETAIL_INSET))
                    .child(status_tag(
                        p,
                        "Connected. Usage is up to date.",
                        TagTone::Success,
                    ))
                    .into_any_element(),
            );
        }
        let bars = account.usage_bars();
        if !account.needs_sign_in() && !bars.is_empty() {
            blocks.push(
                v_flex()
                    .mx(px(DETAIL_INSET))
                    .gap(px(8.0))
                    .p(px(12.0))
                    .rounded(px(MODAL_RADIUS_CONTROL))
                    .border_1()
                    .border_color(hsla(p.hairline))
                    .bg(hsla(p.foreground_alpha(0.025)))
                    .children(bars.iter().map(|window| usage_line(p, window)))
                    .children(
                        account
                            .reset_credits()
                            .filter(|credits| *credits > 0 && provider == "codex")
                            .map(|credits| {
                                h_flex()
                                    .child(status_tag(
                                        p,
                                        format!(
                                            "{credits} saved reset{}",
                                            if credits == 1 { "" } else { "s" }
                                        ),
                                        TagTone::Plain,
                                    ))
                                    .into_any_element()
                            }),
                    )
                    .into_any_element(),
            );
        }
        blocks.push(self.render_detail_rows(p, account, accounts, &draft, busy, hide, window, cx));
        if draft.remove {
            blocks.push(self.render_remove_confirmation(p, account, busy, hide, cx));
        }
        v_flex()
            .id(SharedString::from(format!("account-{id}-detail")))
            .w_full()
            .pt(px(DETAIL_INSET))
            .gap(px(14.0))
            .children(blocks)
            .into_any_element()
    }

    /// The badge, name, email, sessions and the account's tag.
    fn render_detail_header(
        &self,
        p: &SettingsPalette,
        account: &Account,
        draft: &EditorDraft,
        connected: bool,
        hide: bool,
        more: AnyElement,
    ) -> AnyElement {
        let id = account.id();
        let badge = if draft.indicator.is_empty() {
            account_badge(account)
        } else {
            draft.indicator.clone()
        };
        let subtitle = {
            let email = account.email();
            let sessions = account.session_count();
            let sessions = format!(
                "{sessions} session{} use{} it",
                if sessions == 1 { "" } else { "s" },
                if sessions == 1 { "s" } else { "" }
            );
            if email.is_empty() || email == account.name() {
                sessions
            } else {
                format!("{} · {sessions}", account_text(&email, hide))
            }
        };
        // CDXC:AgentProviders 2026-10-14 DECISION: User picked an "Auto-switch on/off" tag instead of the Automatic or Manual label with its info icon (supersedes the 2026-09-09 label). The tag's tooltip explains it.
        let tag = if account.user_disabled() {
            status_tag(p, "Paused", TagTone::Off)
        } else if connected && !account.needs_sign_in() {
            status_tag(p, "Connected", TagTone::Success)
        } else if account.needs_sign_in() {
            status_tag(p, "Needs sign-in", TagTone::Warning)
        } else if draft.eligible {
            status_tag(p, "Auto-switch on", TagTone::On)
        } else {
            status_tag(p, "Auto-switch off", TagTone::Off)
        };
        let tag_tip = if account.user_disabled() {
            "You paused this account. Set Use this account to Automatically or Only when I pick it to use it again."
        } else if account.synced_from().is_some() {
            "Sign in on this computer to use this account here."
        } else if account.needs_sign_in() {
            "Reconnect this account to read its usage and use it again."
        } else if draft.eligible {
            "Ghostex can move a session to this account when another account hits its limit."
        } else {
            "Ghostex uses this account only when you pick it. Automatic switching skips it."
        };
        // CDXC:AgentProviders 2026-10-08 DECISION: User: show why gxserver set an account to Manual, as a short line under its Automatic/Manual status ("Subscription access disabled"), with the full reason in its tooltip. Since 2026-10-14 the line sits under the Auto-switch off tag.
        let disabled = account.disabled_reason().filter(|_| !draft.eligible).map(|reason| {
            div()
                .id(SharedString::from(format!("account-{id}-disabled-reason")))
                .text_size(px(11.5))
                .text_color(hsla(p.destructive))
                .tooltip(tooltip_text(format!(
                    "{reason} Ghostex turned Auto-switch off for this account. Set Use this account back to Automatically, reconnect it, or pick it for a session when its access works again."
                )))
                .child("Subscription access disabled")
        });
        h_flex()
            .px(px(DETAIL_INSET))
            .gap(px(12.0))
            .items_center()
            .child(
                div()
                    .size(px(42.0))
                    .flex_shrink_0()
                    .flex()
                    .items_center()
                    .justify_center()
                    .rounded(px(10.0))
                    .border_1()
                    .border_color(hsla(p.hairline))
                    .bg(hsla(p.foreground_alpha(0.03)))
                    .when(account.needs_sign_in(), |this| this.opacity(0.6))
                    .when(account.user_disabled(), |this| this.opacity(0.35))
                    .child(account_mark(p, &account.provider(), &badge, 1.5)),
            )
            .child(
                v_flex()
                    .flex_1()
                    .min_w_0()
                    .child(
                        div()
                            .overflow_hidden()
                            .whitespace_nowrap()
                            .text_ellipsis()
                            .text_size(px(16.0))
                            .line_height(px(22.0))
                            .font_weight(FontWeight::MEDIUM)
                            .text_color(hsla(p.foreground))
                            .child(account_text(&account.name(), hide)),
                    )
                    .child(
                        div()
                            .overflow_hidden()
                            .whitespace_nowrap()
                            .text_ellipsis()
                            .text_size(px(12.5))
                            .line_height(px(17.0))
                            .text_color(hsla(p.muted))
                            .child(subtitle),
                    ),
            )
            .child(
                v_flex()
                    .flex_shrink_0()
                    .items_end()
                    .gap(px(4.0))
                    .child(
                        div()
                            .id(SharedString::from(format!("account-{id}-status")))
                            .tooltip(tooltip_text(tag_tip))
                            .child(tag),
                    )
                    .children(disabled),
            )
            .child(more)
            .into_any_element()
    }

    /// The account's ⋯ menu: Sign in again and Remove…, which used to be buttons under the pane.
    fn render_account_menu(
        &mut self,
        p: &SettingsPalette,
        account: &Account,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let id = account.id();
        let mut rows = Vec::new();
        if !account.user_disabled() {
            let sign_in_account = account.clone();
            rows.push(
                super::menus::MenuRow::action("Sign in again", move |page, cx| {
                    page.start_reconnect(&sign_in_account, cx)
                })
                .enabled(!self.flow_running(&format!("reconnect:{id}"))),
            );
        }
        let remove_id = id.clone();
        rows.push(
            super::menus::MenuRow::action("Remove…", move |page, cx| {
                page.update_editor(&remove_id, cx, |draft| draft.remove = true)
            })
            .destructive()
            .group(),
        );
        self.render_more_menu(
            p,
            &format!("account-{id}"),
            "More account actions",
            rows,
            cx,
        )
    }

    /// Use this account: Automatically, Only when I pick it, or Paused.
    ///
    /// CDXC:AgentProviders 2026-11-05 DECISION:
    /// User picked "One menu" for an account's switches (Q1335) and said "'Use this account: Off' sounds wrong man, find better word". The Enabled and Auto-switch switches are one Use this account menu: Automatically (auto-switch on), Only when I pick it (auto-switch off) and Paused (the account turned off, CDXC:AgentProviders 2026-10-14 on `SavedAccount::user_disabled`), still saved as `eligible` and `userDisabled`. Show usage in the sidebar stays its own switch. Supersedes the 2026-10-14 Auto-switch switch and Enabled row.
    fn render_use_row(
        &mut self,
        p: &SettingsPalette,
        account: &Account,
        draft: &EditorDraft,
        busy: bool,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let id = account.id();
        let value = if account.user_disabled() {
            "paused"
        } else if draft.eligible {
            "auto"
        } else {
            "manual"
        };
        let options = [
            ("auto", "Automatically"),
            ("manual", "Only when I pick it"),
            ("paused", "Paused"),
        ]
        .map(|(value, label)| SettingOption {
            label: label.into(),
            value: value.into(),
        });
        let description = match value {
            "auto" => {
                "Ghostex can start sessions here and move a session here when another account hits its limit."
            }
            "manual" => {
                "Ghostex uses it only when you pick it for a session. Automatic switching skips it."
            }
            _ => {
                "Kept saved but out of use: no new sessions, no switching, no usage reads. Sessions already on it keep running."
            }
        };
        let disabled_now = account.user_disabled();
        let select = settings_select(
            self,
            p,
            SharedString::from(format!("account-editor-{id}-use")),
            &options,
            value,
            Some(DETAIL_CONTROL_WIDTH),
            busy,
            Some("Accounts are being updated.".into()),
            {
                let id = id.clone();
                move |page: &mut Self, next, _window, cx| {
                    let pause = next == "paused";
                    if pause != disabled_now {
                        page.flows.remove(&format!("reconnect:{id}"));
                        page.account_request(
                            json!({ "operation": "setEnabled", "id": id, "enabled": !pause }),
                            None,
                            cx,
                        );
                    }
                    if !pause {
                        page.update_editor(&id, cx, |draft| draft.eligible = next == "auto");
                        page.commit_editor(&id, cx);
                    }
                }
            },
            window,
            cx,
        );
        setting_row(
            p,
            SharedString::from(format!("account-editor-{id}-use-row")),
            RowSpec::new("Use this account")
                .description(description)
                .inset_x(0.0),
            None,
            select,
            cx,
        )
    }

    /// "claude.ai in Ghostex": whether this account's own Browser profile is signed in to
    /// claude.ai, with Sign in to claude.ai or Sign out (CDXC:ClaudeCloud in
    /// app/claude_cloud_page.rs). Ghostex learns it from the account's claude.ai pages.
    fn render_claude_ai_row(
        &mut self,
        p: &SettingsPalette,
        account: &Account,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let id = account.id();
        let signin = claude_ai_signin(&id);
        let (description, signed_in) = match &signin {
            Some(Some(email)) => (
                format!("Signed in as {email}. Its cloud sessions open here."),
                true,
            ),
            Some(None) => (
                "Not signed in. Its cloud sessions' pages ask you to sign in.".to_string(),
                false,
            ),
            None => (
                "Not checked yet. Sign in once so this account's cloud sessions open in Ghostex."
                    .to_string(),
                false,
            ),
        };
        let waiting = super::claude_ai_wait::waiting(&id, signed_in);
        if waiting {
            self.watch_claude_ai_sign_ins(cx);
        }
        let (name, email) = (account.name(), account.email());
        let button = if signed_in {
            let id = id.clone();
            settings_button(
                p,
                SharedString::from(format!("account-editor-{id}-claude-ai-signout")),
                "Sign out",
                None,
                ButtonVariant::Outline,
                false,
                None,
                move |page: &mut Self, _window, cx| {
                    super::super::super::store::post_store_message(
                        &page.store,
                        json!({ "type": "signOutClaudeAccountFromClaudeAi", "accountId": id }),
                        cx,
                    );
                },
                cx,
            )
        } else {
            let id = id.clone();
            settings_button(
                p,
                SharedString::from(format!("account-editor-{id}-claude-ai-signin")),
                "Sign in to claude.ai",
                None,
                ButtonVariant::Outline,
                false,
                None,
                move |page: &mut Self, _window, cx| {
                    super::claude_ai_wait::start(&id);
                    page.watch_claude_ai_sign_ins(cx);
                    super::super::super::store::post_store_message(
                        &page.store,
                        json!({
                            "type": "signInClaudeAccountToClaudeAi",
                            "accountId": id,
                            "name": name,
                            "email": email,
                        }),
                        cx,
                    );
                },
                cx,
            )
        };
        setting_row(
            p,
            SharedString::from(format!("account-editor-{}-claude-ai", account.id())),
            RowSpec::new("claude.ai in Ghostex")
                .description(description)
                .inset_x(0.0),
            None,
            h_flex()
                .gap(px(8.0))
                .items_center()
                .when(waiting, |row| {
                    row.child(
                        div()
                            .id(SharedString::from(format!(
                                "account-editor-{}-claude-ai-waiting",
                                account.id()
                            )))
                            .flex()
                            .tooltip(tooltip_text(super::claude_ai_wait::WAITING_TOOLTIP))
                            .child(super::connect::spinner(
                                format!("account-editor-{}-claude-ai-spinner", account.id()),
                                p.muted,
                            )),
                    )
                })
                .child(button)
                .into_any_element(),
            cx,
        )
    }

    /// Enabled, Name, Auto-switch, Show usage in the sidebar, and Advanced.
    #[allow(clippy::too_many_arguments)]
    fn render_detail_rows(
        &mut self,
        p: &SettingsPalette,
        account: &Account,
        accounts: &[Account],
        draft: &EditorDraft,
        busy: bool,
        hide: bool,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let id = account.id();
        let mut rows: Vec<AnyElement> = Vec::new();
        let enabled = !account.user_disabled();
        // Name.
        let name_id = SharedString::from(format!("account-editor-{id}-name"));
        let name_input = FieldStates::text_state(
            self,
            &name_id,
            &draft.name,
            None,
            {
                let id = id.clone();
                move |page: &mut Self, text, _window, cx| {
                    let text: String = text.chars().take(80).collect();
                    page.update_editor(&id, cx, |draft| draft.name = text)
                }
            },
            window,
            cx,
        );
        self.commit_on_blur(&name_id, &name_input, &id, window, cx);
        super::widgets::sync_masked(
            &mut self.masked_inputs,
            &name_id,
            &name_input,
            hide && draft.name.contains('@'),
            window,
            cx,
        );
        rows.push(setting_row(
            p,
            SharedString::from(format!("account-editor-{id}-name-row")),
            RowSpec::new("Name")
                .description("Saves when you press Enter or click away.")
                .inset_x(0.0),
            None,
            settings_text_input(
                p,
                &name_input,
                Some(DETAIL_CONTROL_WIDTH),
                false,
                window,
                cx,
            ),
            cx,
        ));
        // claude.ai in Ghostex: the sign-in this Claude account's cloud pages use.
        if enabled && account.provider() == "claude" {
            rows.push(self.render_claude_ai_row(p, account, cx));
        }
        rows.push(self.render_use_row(p, account, draft, busy, window, cx));
        // Show usage in the sidebar means nothing while the account is paused.
        if enabled {
            // Show usage in the sidebar.
            let shown = account.show_in_titlebar();
            let sidebar_account = account.clone();
            rows.push(setting_row(
                p,
                SharedString::from(format!("account-editor-{id}-sidebar")),
                RowSpec::new("Show usage in the sidebar")
                    .description("Keeps this account's usage bars at the bottom of the sidebar.")
                    .inset_x(0.0),
                None,
                switch_control(
                    p,
                    SharedString::from(format!("account-editor-{id}-sidebar-switch")),
                    "Show usage in the sidebar",
                    shown,
                    busy,
                    Some("Accounts are being updated.".into()),
                    move |page: &mut Self, next, _window, cx| {
                        page.set_sidebar_usage(&sidebar_account, next, cx)
                    },
                    cx,
                ),
                cx,
            ));
        }
        // Advanced.
        let advanced_id = id.clone();
        let open = draft.advanced;
        rows.push(
            h_flex()
                .id(SharedString::from(format!("account-editor-{id}-advanced")))
                .w_full()
                .min_h(px(44.0))
                .gap(px(8.0))
                .items_center()
                .cursor_pointer()
                .on_click(cx.listener(move |page, _: &ClickEvent, _window, cx| {
                    page.update_editor(&advanced_id, cx, |draft| draft.advanced = !draft.advanced)
                }))
                .child(settings_icon(
                    if open {
                        "modals/settings/chevron-down.svg"
                    } else {
                        "modals/settings/chevron-right.svg"
                    },
                    14.0,
                    p.muted,
                ))
                .child(
                    div()
                        .text_size(px(14.0))
                        .text_color(hsla(p.foreground))
                        .child("Advanced"),
                )
                .child(
                    div()
                        .min_w_0()
                        .overflow_hidden()
                        .whitespace_nowrap()
                        .text_ellipsis()
                        .text_size(px(12.5))
                        .text_color(hsla(p.muted))
                        .child("Session badge, account number"),
                )
                .into_any_element(),
        );
        if open {
            rows.push(self.render_badge_row(p, account, draft, window, cx));
            if accounts.len() > 1 {
                rows.push(
                    self.render_number_row(p, account, accounts, draft, busy, hide, window, cx),
                );
            }
        }
        let hairline = hsla(p.hairline);
        v_flex()
            .mx(px(DETAIL_INSET))
            .children(rows.into_iter().enumerate().map(|(index, row)| {
                div()
                    .w_full()
                    .when(index > 0, |this| this.border_t_1().border_color(hairline))
                    .child(row)
            }))
            .into_any_element()
    }

    /// Session badge: its field with the session icon it gives, side by side.
    fn render_badge_row(
        &mut self,
        p: &SettingsPalette,
        account: &Account,
        draft: &EditorDraft,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let id = account.id();
        let indicator_id = SharedString::from(format!("account-editor-{id}-indicator"));
        let indicator_input = FieldStates::text_state(
            self,
            &indicator_id,
            &draft.indicator,
            Some(&account.selector()),
            {
                let id = id.clone();
                let indicator_id = indicator_id.clone();
                move |page: &mut Self, text, window, cx| {
                    let normalized = normalize_account_indicator_input(&text);
                    page.update_editor(&id, cx, |draft| draft.indicator = normalized.clone());
                    if normalized != text
                        && let Some(state) = page.fields.texts.get(&indicator_id)
                    {
                        let input = state.input.clone();
                        input.update(cx, |input, cx| input.set_value(normalized, window, cx));
                    }
                }
            },
            window,
            cx,
        );
        self.commit_on_blur(&indicator_id, &indicator_input, &id, window, cx);
        let badge = if draft.indicator.is_empty() {
            account.selector()
        } else {
            draft.indicator.clone()
        };
        setting_row(
            p,
            SharedString::from(format!("account-editor-{id}-indicator-row")),
            RowSpec::new("Session badge")
                .description(format!(
                    "Up to two letters or numbers on this account's session icons, such as cw for Claude work. Enter - to show none, or leave it empty to show its number, {}.",
                    account.selector()
                ))
                .inset_x(0.0),
            None,
            h_flex()
                .w(px(DETAIL_CONTROL_WIDTH))
                .gap(px(8.0))
                .items_center()
                .child(
                    div()
                        .size(px(32.0))
                        .flex_shrink_0()
                        .flex()
                        .items_center()
                        .justify_center()
                        .rounded(px(MODAL_RADIUS_CONTROL))
                        .border_1()
                        .border_color(hsla(p.hairline))
                        .child(account_mark(p, &account.provider(), &badge, 1.0)),
                )
                .child(div().flex_1().min_w_0().child(settings_text_input(
                    p,
                    &indicator_input,
                    None,
                    false,
                    window,
                    cx,
                )))
                .into_any_element(),
            cx,
        )
    }

    /// Account number: swap it with another saved account's.
    #[allow(clippy::too_many_arguments)]
    fn render_number_row(
        &mut self,
        p: &SettingsPalette,
        account: &Account,
        accounts: &[Account],
        draft: &EditorDraft,
        busy: bool,
        hide: bool,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let id = account.id();
        let options: Vec<SettingOption> = accounts
            .iter()
            .filter(|other| other.id() != id)
            .map(|other| SettingOption {
                label: account_text(&format!("{} ({})", other.name(), other.selector()), hide),
                value: other.id(),
            })
            .collect();
        let select = settings_select(
            self,
            p,
            SharedString::from(format!("account-editor-{id}-swap")),
            &options,
            &draft.swap_target,
            None,
            false,
            None,
            {
                let id = id.clone();
                move |page: &mut Self, next, _window, cx| {
                    page.update_editor(&id, cx, |draft| draft.swap_target = next)
                }
            },
            window,
            cx,
        );
        let select = if draft.swap_target.is_empty() {
            placeholder_select(p, select)
        } else {
            select
        };
        let swap = {
            let id = id.clone();
            let target = draft.swap_target.clone();
            settings_sized_button(
                p,
                SharedString::from(format!("account-editor-{id}-swap-button")),
                "Swap",
                None,
                None,
                SizedButtonVariant::Outline,
                SizedButtonSize::Default,
                busy || draft.swap_target.is_empty(),
                None,
                move |page: &mut Self, _window, cx| {
                    page.account_request(
                        json!({ "operation": "swapSlots", "firstId": id, "secondId": target }),
                        None,
                        cx,
                    )
                },
                cx,
            )
        };
        let helper = helper_label(&account.provider());
        setting_row(
            p,
            SharedString::from(format!("account-editor-{id}-swap-row")),
            RowSpec::new(format!("Account number {}", account.selector())).description(
                if account.provider() == "claude" {
                    format!("{helper}'s number for this login (cswap run {}). Swap it with another account to change the order. Stop sessions using either account first.", account.selector())
                } else {
                    format!("{helper}'s number for this login (xswap run {}). Swap it with another account to change the order.", account.selector())
                },
            )
            .inset_x(0.0),
            None,
            h_flex()
                .w(px(DETAIL_CONTROL_WIDTH))
                .gap(px(8.0))
                .items_center()
                .child(div().flex_1().min_w_0().child(select))
                .child(swap)
                .into_any_element(),
            cx,
        )
    }

    /// Sign in again and Remove, and while asked the Remove confirmation.
    /// The Remove… confirmation, opened from the account's ⋯ menu.
    pub(crate) fn render_remove_confirmation(
        &mut self,
        p: &SettingsPalette,
        account: &Account,
        busy: bool,
        hide: bool,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let id = account.id();
        let mut column = v_flex().w_full().gap(px(12.0));
        {
            let remove_id = id.clone();
            let confirm = settings_sized_button(
                p,
                SharedString::from(format!("account-editor-{id}-remove-confirm")),
                "Remove account",
                None,
                None,
                SizedButtonVariant::Destructive,
                SizedButtonSize::Sm,
                busy,
                None,
                move |page: &mut Self, _window, cx| {
                    let this = cx.weak_entity();
                    let close_id = remove_id.clone();
                    page.account_request(
                        json!({ "operation": "remove", "id": remove_id }),
                        Some(Box::new(move |ok, cx| {
                            if ok {
                                let _ = this.update(cx, |page, cx| {
                                    page.editors.remove(&close_id);
                                    if page.pane == Some(Pane::Account(close_id.clone())) {
                                        page.pane = None;
                                    }
                                    cx.notify();
                                });
                            }
                        })),
                        cx,
                    );
                },
                cx,
            );
            let cancel_id = id.clone();
            let cancel = settings_sized_button(
                p,
                SharedString::from(format!("account-editor-{id}-remove-cancel")),
                "Cancel",
                None,
                None,
                SizedButtonVariant::Outline,
                SizedButtonSize::Sm,
                false,
                None,
                move |page: &mut Self, _window, cx| {
                    page.update_editor(&cancel_id, cx, |draft| draft.remove = false)
                },
                cx,
            );
            let sessions = account.session_count();
            let effect = if sessions == 0 {
                "No sessions use it.".to_string()
            } else {
                format!(
                    "{} use it. Each one needs another account before it resumes.",
                    if sessions == 1 {
                        "1 session".to_string()
                    } else {
                        format!("{sessions} sessions")
                    }
                )
            };
            column = column.child(
                v_flex()
                    .w_full()
                    .gap(px(8.0))
                    .p(px(12.0))
                    .rounded(px(MODAL_RADIUS_CONTROL))
                    .border_1()
                    .border_color(hsla(css_fade(p.destructive, 0.35)))
                    .bg(hsla(css_fade(p.destructive, 0.06)))
                    .child(
                        div()
                            .text_size(px(13.0))
                            .font_weight(FontWeight::MEDIUM)
                            .text_color(hsla(p.foreground))
                            .child(format!(
                                "Remove {} from Ghostex?",
                                account_text(&account.name(), hide)
                            )),
                    )
                    .child(
                        div()
                            .text_size(px(12.5))
                            .line_height(px(17.0))
                            .text_color(hsla(p.muted))
                            .child(if account.usage_only() {
                                "Its sign-in is deleted from this computer. You can add it again at any time.".to_string()
                            } else {
                                format!(
                                    "{effect} The login and shared conversations stay saved in {}, so you can add it back later.",
                                    helper_label(&account.provider())
                                )
                            }),
                    )
                    .child(h_flex().gap(px(8.0)).child(confirm).child(cancel)),
            );
        }
        div()
            .px(px(DETAIL_INSET))
            .pt(px(12.0))
            .pb(px(DETAIL_INSET))
            .border_t_1()
            .border_color(hsla(p.hairline))
            .child(column)
            .into_any_element()
    }
}

/// " 37 sessions use it." for the needs-sign-in note.
/// What the app last saw of a Claude account's claude.ai sign-in (`claude-cloud-signins.json`,
/// written by app/claude_cloud_page.rs): `Some(Some(email))` signed in, `Some(None)` signed out,
/// `None` not seen yet. Read again only when the file changes.
fn claude_ai_signin(account_id: &str) -> Option<Option<String>> {
    thread_local! {
        static CACHE: std::cell::RefCell<(Option<std::time::SystemTime>, serde_json::Value)> =
            std::cell::RefCell::new((None, serde_json::Value::Null));
    }
    let path = ghostex_paths::GhostexPaths::resolve()
        .state_dir
        .join("claude-cloud-signins.json");
    let modified = std::fs::metadata(&path)
        .and_then(|meta| meta.modified())
        .ok();
    CACHE.with(|cache| {
        let mut cache = cache.borrow_mut();
        if cache.0 != modified {
            cache.1 = std::fs::read_to_string(&path)
                .ok()
                .and_then(|text| serde_json::from_str(&text).ok())
                .unwrap_or(serde_json::Value::Null);
            cache.0 = modified;
        }
        cache.1.get(account_id).map(|entry| {
            entry
                .get("email")
                .and_then(serde_json::Value::as_str)
                .map(str::to_string)
        })
    })
}

fn sessions_sentence(count: u64) -> String {
    match count {
        0 => String::new(),
        1 => " 1 session uses it.".into(),
        count => format!(" {count} sessions use it."),
    }
}

/// A select with no value shows its placeholder ("Swap with…") in the muted tone; the label sits
/// over the empty trigger and lets clicks through to it.
fn placeholder_select(p: &SettingsPalette, select: AnyElement) -> AnyElement {
    div()
        .relative()
        .child(select)
        .child(
            div()
                .absolute()
                .left(px(13.0))
                .top_0()
                .bottom_0()
                .flex()
                .items_center()
                .text_size(px(14.0))
                .text_color(hsla(p.muted))
                .child("Swap with…"),
        )
        .into_any_element()
}
