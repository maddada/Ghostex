//! Cursor, Gemini and Copilot in Settings > Accounts: their sign-in (gxserver's `usageSignIn*`
//! job: the page it opens, a code to type for Copilot, a box for a pasted code for Gemini) and the
//! pane of a saved one (its usage, plan, name, sidebar switch and ⋯ menu).
//!
//! CDXC:Settings 2026-11-05 DECISION:
//! The Add account picker signs in the way the reference flow the user chose does: picking a provider opens the browser at once, the pane says it is waiting with Copy link and Open browser, shows a code to type when the provider has one, and closes on the new account when the sign-in is saved; the sidebar's usage strip then pins with it (CDXC:Sidebar 2026-11-05 on `update_titlebar_account_from_ui`).
//! SEE-ALSO: server/src/accounts/usage_only/signin.rs.
use super::super::super::super::native_modal_kit::*;
use super::super::super::fields::{
    FieldStates, RowSpec, SizedButtonSize, SizedButtonVariant, setting_row, settings_sized_button,
    settings_text_input, switch_control,
};
use super::super::super::palette::SettingsPalette;
use super::super::super::store::{post_store_message, store_copy_to_clipboard};
use super::client::ACCOUNT_SETUP_OWNER;
use super::connect::spinner;
use super::data::{Account, provider_label};
use super::editor::{DETAIL_CONTROL_WIDTH, DETAIL_INSET};
use super::menus::MenuRow;
use super::widgets::{TagTone, account_logo, account_text, status_tag, usage_line};
use super::{AccountsTab, Pane};
use gpui::prelude::FluentBuilder as _;
use gpui::{
    AnyElement, Context, FontWeight, InteractiveElement as _, IntoElement, ParentElement as _,
    SharedString, Styled as _, Task, Window, div, px,
};
use gpui_component::{h_flex, v_flex};
use serde_json::{Value, json};
use std::time::Duration;

const POLL: Duration = Duration::from_secs(1);

/// The usage sign-in in progress (one at a time).
pub(crate) struct UsageSignIn {
    pub(crate) provider: String,
    /// The saved account being signed in again.
    pub(crate) account_id: Option<String>,
    job: Option<Value>,
    starting: bool,
    error: String,
    /// The page the job named was opened already.
    opened: Option<String>,
    code: String,
    poll: Option<Task<()>>,
}

fn job_active(job: &Value) -> bool {
    matches!(
        job["status"].as_str(),
        Some("starting" | "waiting" | "saving")
    )
}

impl AccountsTab {
    /// Starts signing in to `provider`, or signing `account_id` in again.
    pub(crate) fn start_usage_sign_in(
        &mut self,
        provider: &str,
        account_id: Option<String>,
        cx: &mut Context<Self>,
    ) {
        self.usage_sign_in = Some(UsageSignIn {
            provider: provider.to_string(),
            account_id: account_id.clone(),
            job: None,
            starting: true,
            error: String::new(),
            opened: None,
            code: String::new(),
            poll: None,
        });
        let this = cx.weak_entity();
        super::client::AccountsClient::call(
            &self.client.clone(),
            json!({
                "operation": "usageSignInStart",
                "owner": ACCOUNT_SETUP_OWNER,
                "provider": provider,
                "accountId": account_id,
            }),
            move |result, cx| {
                let _ = this.update(cx, |page, cx| {
                    page.apply_usage_job(result, cx);
                    page.ensure_usage_poll(cx);
                });
            },
            cx,
        );
        cx.notify();
    }

    /// Whether a sign-in for this saved account is under way.
    pub(crate) fn usage_sign_in_active_for(&self, id: &str) -> bool {
        self.usage_sign_in.as_ref().is_some_and(|flow| {
            flow.account_id.as_deref() == Some(id)
                && (flow.starting || flow.job.as_ref().is_some_and(job_active))
        })
    }

    fn apply_usage_job(&mut self, result: Result<Value, String>, cx: &mut Context<Self>) {
        let Some(flow) = self.usage_sign_in.as_mut() else {
            return;
        };
        flow.starting = false;
        let job = match result {
            Ok(answer) => answer["usageSignIn"].clone(),
            Err(error) => {
                flow.error = if error.trim().is_empty() {
                    "Could not start the sign-in.".into()
                } else {
                    error
                };
                cx.notify();
                return;
            }
        };
        let url = job["url"]
            .as_str()
            .filter(|url| !url.is_empty())
            .map(str::to_string);
        let open = url
            .clone()
            .filter(|url| flow.opened.as_deref() != Some(url.as_str()));
        if let Some(url) = &open {
            flow.opened = Some(url.clone());
        }
        let finished = (job["status"] == "complete")
            .then(|| job["accountId"].as_str().map(str::to_string))
            .flatten();
        flow.job = Some(job);
        if let Some(url) = open {
            // The browser opens by itself, like the Claude and Codex sign-ins.
            post_store_message(
                &self.store,
                json!({ "type": "openExternalUrl", "url": url }),
                cx,
            );
        }
        if let Some(id) = finished {
            self.finish_usage_sign_in(id, cx);
        }
        cx.notify();
    }

    /// The account is saved: show it, say Connected, and pin its usage in the sidebar.
    fn finish_usage_sign_in(&mut self, id: String, cx: &mut Context<Self>) {
        let job_id = self
            .usage_sign_in
            .as_ref()
            .and_then(|flow| flow.job.as_ref())
            .and_then(|job| job["id"].as_str().map(str::to_string));
        self.usage_sign_in = None;
        if let Some(job_id) = job_id {
            super::client::AccountsClient::call(
                &self.client.clone(),
                json!({ "operation": "usageSignInAcknowledge", "owner": ACCOUNT_SETUP_OWNER, "jobId": job_id }),
                |_, _| {},
                cx,
            );
        }
        self.select_pane(Pane::Account(id.clone()), cx);
        self.mark_connected(id, cx);
    }

    fn ensure_usage_poll(&mut self, cx: &mut Context<Self>) {
        let Some(flow) = self.usage_sign_in.as_mut() else {
            return;
        };
        if !flow.job.as_ref().is_some_and(job_active) {
            flow.poll = None;
            return;
        }
        if flow.poll.is_some() {
            return;
        }
        flow.poll = Some(cx.spawn(async move |this, cx| {
            loop {
                cx.background_executor().timer(POLL).await;
                let keep = this
                    .update(cx, |page, cx| page.poll_usage_sign_in(cx))
                    .unwrap_or(false);
                if !keep {
                    break;
                }
            }
        }));
    }

    fn poll_usage_sign_in(&mut self, cx: &mut Context<Self>) -> bool {
        let Some(job_id) = self
            .usage_sign_in
            .as_ref()
            .and_then(|flow| flow.job.as_ref())
            .filter(|job| job_active(job))
            .and_then(|job| job["id"].as_str().map(str::to_string))
        else {
            if let Some(flow) = self.usage_sign_in.as_mut() {
                flow.poll = None;
            }
            return false;
        };
        let this = cx.weak_entity();
        super::client::AccountsClient::call(
            &self.client.clone(),
            json!({ "operation": "usageSignInStatus", "owner": ACCOUNT_SETUP_OWNER, "jobId": job_id }),
            move |result, cx| {
                let _ = this.update(cx, |page, cx| page.apply_usage_job(result, cx));
            },
            cx,
        );
        true
    }

    fn usage_job_call(&mut self, operation: &str, input: Option<String>, cx: &mut Context<Self>) {
        let Some(job_id) = self
            .usage_sign_in
            .as_ref()
            .and_then(|flow| flow.job.as_ref())
            .and_then(|job| job["id"].as_str().map(str::to_string))
        else {
            return;
        };
        let this = cx.weak_entity();
        super::client::AccountsClient::call(
            &self.client.clone(),
            json!({ "operation": operation, "owner": ACCOUNT_SETUP_OWNER, "jobId": job_id, "input": input }),
            move |result, cx| {
                let _ = this.update(cx, |page, cx| page.apply_usage_job(result, cx));
            },
            cx,
        );
    }

    /// The sign-in for `provider` as the pane shows it: waiting with its page, a code, the box
    /// for a pasted code, or why it stopped.
    pub(crate) fn render_usage_sign_in(
        &mut self,
        p: &SettingsPalette,
        provider: &str,
        account_id: Option<&str>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let label = provider_label(provider);
        let state = self
            .usage_sign_in
            .as_ref()
            .filter(|flow| flow.provider == provider && flow.account_id.as_deref() == account_id);
        let Some(flow) = state else {
            let provider = provider.to_string();
            let again = account_id.map(str::to_string);
            return h_flex()
                .child(settings_sized_button(
                    p,
                    SharedString::from(format!("usage-{provider}-start")),
                    "Sign in with browser",
                    None,
                    None,
                    SizedButtonVariant::Primary,
                    SizedButtonSize::Default,
                    false,
                    None,
                    move |page: &mut Self, _window, cx| {
                        page.start_usage_sign_in(&provider, again.clone(), cx)
                    },
                    cx,
                ))
                .into_any_element();
        };
        let job = flow.job.clone().unwrap_or(Value::Null);
        let (starting, error, code) = (flow.starting, flow.error.clone(), flow.code.clone());
        let status = job["status"].as_str().unwrap_or_default().to_string();
        let mut children: Vec<AnyElement> = Vec::new();
        if !error.is_empty() || status == "failed" {
            let message = if error.is_empty() {
                job["error"]
                    .as_str()
                    .unwrap_or("The sign-in stopped.")
                    .to_string()
            } else {
                error
            };
            children.push(
                div()
                    .text_size(px(12.5))
                    .text_color(hsla(p.destructive))
                    .child(message)
                    .into_any_element(),
            );
            let retry_provider = provider.to_string();
            let again = account_id.map(str::to_string);
            children.push(
                h_flex()
                    .child(settings_sized_button(
                        p,
                        SharedString::from(format!("usage-{provider}-retry")),
                        "Try again",
                        None,
                        None,
                        SizedButtonVariant::Primary,
                        SizedButtonSize::Sm,
                        false,
                        None,
                        move |page: &mut Self, _window, cx| {
                            page.start_usage_sign_in(&retry_provider, again.clone(), cx)
                        },
                        cx,
                    ))
                    .into_any_element(),
            );
        } else {
            let (title, detail) = if starting || status == "starting" {
                (
                    "Opening your browser…".to_string(),
                    format!("Ghostex is starting the {label} sign-in."),
                )
            } else if status == "saving" {
                (
                    "Saving the account…".to_string(),
                    "Signed in. Ghostex is reading its usage.".to_string(),
                )
            } else if job["userCode"].is_string() {
                (
                    "Waiting for your browser…".to_string(),
                    "Type this code on the page that opened. This closes when you're signed in."
                        .to_string(),
                )
            } else {
                (
                    "Waiting for your browser…".to_string(),
                    "Continue in your browser. This closes when you're signed in.".to_string(),
                )
            };
            children.push(
                h_flex()
                    .gap(px(10.0))
                    .items_start()
                    .child(
                        div()
                            .pt(px(2.0))
                            .child(spinner(format!("usage-{provider}-spinner"), p.foreground)),
                    )
                    .child(
                        v_flex()
                            .gap(px(2.0))
                            .child(
                                div()
                                    .text_size(px(13.0))
                                    .font_weight(FontWeight::MEDIUM)
                                    .text_color(hsla(p.foreground))
                                    .child(title),
                            )
                            .child(
                                div()
                                    .text_size(px(12.5))
                                    .line_height(px(17.0))
                                    .text_color(hsla(p.muted))
                                    .child(detail),
                            ),
                    )
                    .into_any_element(),
            );
            if let Some(user_code) = job["userCode"].as_str().map(str::to_string) {
                let copy = user_code.clone();
                children.push(
                    h_flex()
                        .gap(px(10.0))
                        .items_center()
                        .child(
                            div()
                                .px(px(12.0))
                                .py(px(6.0))
                                .rounded(px(MODAL_RADIUS_CONTROL))
                                .border_1()
                                .border_color(hsla(p.hairline))
                                .bg(hsla(p.foreground_alpha(0.04)))
                                .font_family(MODAL_MONO_FONT)
                                .text_size(px(20.0))
                                .font_weight(FontWeight::SEMIBOLD)
                                .text_color(hsla(p.foreground))
                                .child(user_code),
                        )
                        .child(settings_sized_button(
                            p,
                            SharedString::from(format!("usage-{provider}-copy-code")),
                            "Copy code",
                            None,
                            None,
                            SizedButtonVariant::Outline,
                            SizedButtonSize::Sm,
                            false,
                            None,
                            move |page: &mut Self, _window, cx| {
                                store_copy_to_clipboard(&page.store, copy.clone(), cx)
                            },
                            cx,
                        ))
                        .into_any_element(),
                );
            }
            if job["acceptsCode"] == true && status == "waiting" {
                let code_id = SharedString::from(format!("usage-{provider}-code"));
                let input = FieldStates::text_state(
                    self,
                    &code_id,
                    &code,
                    Some("Code or the page's address"),
                    move |page: &mut Self, text, _window, cx| {
                        if let Some(flow) = page.usage_sign_in.as_mut() {
                            flow.code = text;
                        }
                        cx.notify();
                    },
                    window,
                    cx,
                );
                children.push(
                    v_flex()
                        .gap(px(6.0))
                        .child(div().text_size(px(12.0)).text_color(hsla(p.muted)).child(
                            job["note"].as_str().map(str::to_string).unwrap_or_else(|| {
                                "Paste the code here if your browser shows one.".to_string()
                            }),
                        ))
                        .child(
                            h_flex()
                                .gap(px(8.0))
                                .items_center()
                                .child(
                                    div().flex_1().min_w_0().child(settings_text_input(
                                        p, &input, None, false, window, cx,
                                    )),
                                )
                                .child(settings_sized_button(
                                    p,
                                    SharedString::from(format!("usage-{provider}-send-code")),
                                    "Continue",
                                    None,
                                    None,
                                    SizedButtonVariant::Primary,
                                    SizedButtonSize::Default,
                                    code.trim().is_empty(),
                                    None,
                                    |page: &mut Self, _window, cx| {
                                        let code = page
                                            .usage_sign_in
                                            .as_mut()
                                            .map(|flow| std::mem::take(&mut flow.code))
                                            .unwrap_or_default();
                                        page.usage_job_call("usageSignInInput", Some(code), cx)
                                    },
                                    cx,
                                )),
                        )
                        .into_any_element(),
                );
            }
            let mut actions: Vec<AnyElement> = Vec::new();
            if let Some(url) = job["url"]
                .as_str()
                .filter(|url| !url.is_empty())
                .map(str::to_string)
            {
                let copy = url.clone();
                actions.push(settings_sized_button(
                    p,
                    SharedString::from(format!("usage-{provider}-copy-link")),
                    "Copy link",
                    None,
                    None,
                    SizedButtonVariant::Outline,
                    SizedButtonSize::Sm,
                    false,
                    None,
                    move |page: &mut Self, _window, cx| {
                        store_copy_to_clipboard(&page.store, copy.clone(), cx)
                    },
                    cx,
                ));
                actions.push(settings_sized_button(
                    p,
                    SharedString::from(format!("usage-{provider}-open")),
                    "Open browser",
                    None,
                    None,
                    SizedButtonVariant::Outline,
                    SizedButtonSize::Sm,
                    false,
                    None,
                    move |page: &mut Self, _window, cx| {
                        post_store_message(
                            &page.store,
                            json!({ "type": "openExternalUrl", "url": url }),
                            cx,
                        );
                    },
                    cx,
                ));
            }
            actions.push(settings_sized_button(
                p,
                SharedString::from(format!("usage-{provider}-cancel")),
                "Cancel",
                None,
                None,
                SizedButtonVariant::Outline,
                SizedButtonSize::Sm,
                starting,
                None,
                |page: &mut Self, _window, cx| {
                    page.usage_job_call("usageSignInCancel", None, cx);
                    page.usage_sign_in = None;
                    cx.notify();
                },
                cx,
            ));
            children.push(
                h_flex()
                    .flex_wrap()
                    .gap(px(8.0))
                    .children(actions)
                    .into_any_element(),
            );
        }
        v_flex()
            .w_full()
            .gap(px(10.0))
            .children(children)
            .into_any_element()
    }

    /// Add a Cursor, Gemini or Copilot account: its sign-in, started by the picker.
    pub(crate) fn render_add_usage_pane(
        &mut self,
        p: &SettingsPalette,
        provider: &'static str,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let label = provider_label(provider);
        let flow = self.render_usage_sign_in(p, provider, None, window, cx);
        v_flex()
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
                            .child(format!("Sign in to {label}")),
                    ),
            )
            .child(
                div()
                    .text_size(px(12.5))
                    .line_height(px(17.0))
                    .text_color(hsla(p.muted))
                    .child(format!(
                        "Ghostex shows this account's {label} usage in the sidebar and here. It doesn't run sessions on it."
                    )),
            )
            .child(flow)
            .into_any_element()
    }

    /// A saved Cursor, Gemini or Copilot account.
    pub(crate) fn render_usage_account_detail(
        &mut self,
        p: &SettingsPalette,
        account: &Account,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let hide = self.hide_emails(cx);
        let busy = self.client.read(cx).busy;
        let id = account.id();
        let provider = account.provider();
        let connected = self
            .connected
            .as_ref()
            .is_some_and(|(shown, _)| *shown == id);
        let signing_in = self
            .usage_sign_in
            .as_ref()
            .is_some_and(|flow| flow.account_id.as_deref() == Some(id.as_str()));
        let mut menu = Vec::new();
        if !account.user_disabled() {
            let (again_provider, again_id) = (provider.clone(), id.clone());
            menu.push(MenuRow::action("Sign in again", move |page, cx| {
                page.start_usage_sign_in(&again_provider, Some(again_id.clone()), cx)
            }));
        }
        let (toggle_id, enable) = (id.clone(), account.user_disabled());
        menu.push(
            MenuRow::action(
                if enable { "Turn on" } else { "Turn off" },
                move |page, cx| {
                    page.account_request(
                        json!({ "operation": "setEnabled", "id": toggle_id, "enabled": enable }),
                        None,
                        cx,
                    )
                },
            )
            .detail(if enable {
                "Read its usage again."
            } else {
                "Keep it saved without reading its usage."
            }),
        );
        let remove_id = id.clone();
        menu.push(
            MenuRow::action("Remove…", move |page, cx| {
                page.update_editor(&remove_id, cx, |draft| draft.remove = true)
            })
            .destructive()
            .group(),
        );
        let more = self.render_more_menu(
            p,
            &format!("account-{id}"),
            "More account actions",
            menu,
            cx,
        );
        let tag = if account.user_disabled() {
            Some(status_tag(p, "Off", TagTone::Off))
        } else if connected {
            Some(status_tag(p, "Connected", TagTone::Success))
        } else if account.status() == "loginRequired" {
            Some(status_tag(p, "Needs sign-in", TagTone::Warning))
        } else {
            account
                .plan()
                .map(|plan| status_tag(p, plan, TagTone::Plain))
        };
        let email = account.email();
        let subtitle = if email.is_empty() || email == account.name() {
            format!("{} · usage only", provider_label(&provider))
        } else {
            format!("{} · usage only", account_text(&email, hide))
        };
        let mut blocks: Vec<AnyElement> = vec![
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
                        .when(account.user_disabled(), |this| this.opacity(0.35))
                        .child(account_logo(p, &provider, 22.0)),
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
                                .text_color(hsla(p.muted))
                                .child(subtitle),
                        ),
                )
                .children(tag)
                .child(more)
                .into_any_element(),
        ];
        if signing_in {
            let flow = self.render_usage_sign_in(p, &provider, Some(&id), window, cx);
            blocks.push(div().px(px(DETAIL_INSET)).child(flow).into_any_element());
        } else if account.status() == "loginRequired" {
            let (again_provider, again_id) = (provider.clone(), id.clone());
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
                            .text_color(hsla(p.muted))
                            .child("Its sign-in stopped working, so Ghostex can't read its usage."),
                    )
                    .child(h_flex().child(settings_sized_button(
                        p,
                        SharedString::from(format!("account-{id}-usage-reconnect")),
                        "Sign in again",
                        None,
                        None,
                        SizedButtonVariant::Primary,
                        SizedButtonSize::Default,
                        false,
                        None,
                        move |page: &mut Self, _window, cx| {
                            page.start_usage_sign_in(&again_provider, Some(again_id.clone()), cx)
                        },
                        cx,
                    )))
                    .into_any_element(),
            );
        }
        let bars = account.usage_bars();
        let details = account.details();
        if !account.user_disabled()
            && (!bars.is_empty() || !details.is_empty() || account.usage_error().is_some())
        {
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
                    .children(details.into_iter().map(|(label, value)| {
                        h_flex()
                            .gap(px(8.0))
                            .text_size(px(12.5))
                            .child(div().flex_1().text_color(hsla(p.muted)).child(label))
                            .child(
                                div()
                                    .font_family(MODAL_MONO_FONT)
                                    .text_color(hsla(p.foreground))
                                    .child(value),
                            )
                    }))
                    .children(
                        account
                            .usage_error()
                            .filter(|_| account.status() != "loginRequired")
                            .map(|error| {
                                div()
                                    .text_size(px(12.0))
                                    .text_color(hsla(p.muted))
                                    .child(error)
                            }),
                    )
                    .into_any_element(),
            );
        }
        // Name and Show usage in the sidebar, as on a Claude or Codex account.
        let draft = self.editor_draft(account);
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
        let mut rows = vec![setting_row(
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
        )];
        if !account.user_disabled() {
            let sidebar_account = account.clone();
            rows.push(setting_row(
                p,
                SharedString::from(format!("account-editor-{id}-sidebar")),
                RowSpec::new("Show usage in the sidebar")
                    .description("Keeps this account's usage at the bottom of the sidebar.")
                    .inset_x(0.0),
                None,
                switch_control(
                    p,
                    SharedString::from(format!("account-editor-{id}-sidebar-switch")),
                    "Show usage in the sidebar",
                    account.show_in_titlebar(),
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
        let hairline = hsla(p.hairline);
        blocks.push(
            v_flex()
                .mx(px(DETAIL_INSET))
                .children(rows.into_iter().enumerate().map(|(index, row)| {
                    div()
                        .w_full()
                        .when(index > 0, |this| this.border_t_1().border_color(hairline))
                        .child(row)
                }))
                .into_any_element(),
        );
        if draft.remove {
            blocks.push(self.render_remove_confirmation(p, account, busy, hide, cx));
        }
        v_flex()
            .id(SharedString::from(format!("account-{id}-detail")))
            .w_full()
            .py(px(DETAIL_INSET))
            .gap(px(14.0))
            .children(blocks)
            .into_any_element()
    }
}
