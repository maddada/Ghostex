//! `AccountConnectFlow` (accounts/connect-flow.tsx (deleted 2026-10-01)): signing in from Settings.
//! Reconnect starts at once from the account's Reconnect button; Add account asks only what it
//! must (shared conversations the first time, and the email for Codex), then Sign in with
//! browser. While the sign-in runs the flow waits ("Waiting for you to sign in…") with Open the
//! page again, Cancel, a code box when the helper asks for one, and the helper's output under
//! Details; it ends Connected, on the account the browser chose instead ("You signed in as …"),
//! or on the Codex windows that stopped it.
//!
//! CDXC:Settings 2026-09-07 DECISION:
//! Login commands are hidden in Accounts and its tutorial. Click to run login starts the flow; hovering the button reveals the command. This replaces the visible login command boxes and Log in to fix label.
//! The command stays hidden behind the start button's tooltip, and the sign-in runs inside Settings (server/src/accounts/setup.rs, 2026-09-08 DECISION).
//!
//! CDXC:AgentProviders 2026-10-14 DECISION:
//! User picked "One-click Reconnect: no email box, no second click; the log hides under Details" and "Add account without typing the email". Reconnect starts the sign-in the moment it is clicked and gxserver pre-fills the browser with the saved account's email; a Claude account is added without an email (the browser decides). Codex Swap (xswap) still needs the email of a new Codex account up front, so Add account asks for it there only. The flow shows Waiting for you to sign in…, a code box only when the helper asks for a code, Connected when done, and the account the browser signed in to when it was the wrong one. Supersedes the 2026-09-07 email field and the separate Reconnect account button.
use super::super::super::super::native_modal_kit::*;
use super::super::super::fields::{
    FieldStates, SizedButtonSize, SizedButtonVariant, settings_icon, settings_sized_button,
    settings_text_input, tooltip_text,
};
use super::super::super::palette::SettingsPalette;
use super::super::super::store::post_store_message;
use super::client::ACCOUNT_SETUP_OWNER;
use super::data::{Account, provider_label};
use super::widgets::account_text;
use super::{AccountsTab, Pane};
use gpui::prelude::FluentBuilder as _;
use gpui::{
    Animation, AnimationExt as _, AnyElement, ClickEvent, Context, FontWeight,
    InteractiveElement as _, IntoElement, ParentElement as _, SharedString,
    StatefulInteractiveElement as _, Styled as _, Task, Transformation, Window, div, percentage,
    px, svg,
};
use gpui_component::{h_flex, v_flex};
use serde_json::{Value, json};
use std::time::Duration;

/// `setInterval(poll, 1000)` while a sign-in runs.
const JOB_POLL: Duration = Duration::from_secs(1);

/// One flow's state (its React `useState`s).
pub(crate) struct FlowState {
    pub(crate) email: String,
    pub(crate) consent: bool,
    pub(crate) job: Option<Value>,
    pub(crate) error: String,
    pub(crate) starting: bool,
    /// Details (the helper's output) is open.
    pub(crate) terminal: bool,
    pub(crate) code: String,
    pub(crate) poll: Option<Task<()>>,
}

impl FlowState {
    pub(crate) fn new(email: String, consent: bool, job: Option<Value>) -> Self {
        Self {
            email,
            consent,
            job,
            error: String::new(),
            starting: false,
            terminal: false,
            code: String::new(),
            poll: None,
        }
    }
}

pub(crate) fn job_active(job: &Value) -> bool {
    !matches!(job["status"].as_str(), Some("complete" | "failed"))
}

/// `quote(value)` for the tooltip's shell command.
fn quote(value: &str) -> String {
    format!("'{}'", value.replace('\'', "'\\''"))
}

/// The helper is asking for a code the browser shows (Claude's "Paste code here if prompted").
fn asks_for_code(output: &str) -> bool {
    let output = output.to_lowercase();
    output.contains("paste code")
        || output.contains("enter code")
        || output.contains("authorization code")
        || output.contains("paste the code")
}

/// The login command a flow runs, for the start button's tooltip.
fn login_command(provider: &str, account: Option<&Account>, email: &str) -> String {
    match (provider, account) {
        ("claude", Some(account)) => format!(
            "ghostex account-login claude --account {} --json",
            quote(&account.selector())
        ),
        ("claude", None) => "ghostex account-login claude --json".to_string(),
        (_, Some(account)) => format!("xswap login {}", quote(&account.selector())),
        (_, None) if !email.is_empty() => format!(
            "xswap add --login --share-history --email {} --json",
            quote(email)
        ),
        (_, None) => "xswap add --login --share-history --any-account --json".to_string(),
    }
}

/// A spinning loader icon.
pub(super) fn spinner(id: impl Into<SharedString>, color: gpui::Rgba) -> AnyElement {
    svg()
        .path("modals/settings/loader-2.svg")
        .size(px(16.0))
        .flex_shrink_0()
        .text_color(hsla(color))
        .with_animation(
            gpui::ElementId::Name(id.into()),
            Animation::new(Duration::from_millis(900))
                .repeat()
                .with_max_fps(crate::app::helpers::INDICATOR_MAX_FPS),
            |icon, delta| icon.with_transformation(Transformation::rotate(percentage(delta))),
        )
        .into_any_element()
}

impl AccountsTab {
    /// Forgets the Add account flows of `provider` (a finished sign-in's success message, its
    /// email and consent) so the next Add account starts from an empty form.
    pub(crate) fn reset_setup_flows(&mut self, provider: &str) {
        let prefix = format!("setup:{provider}:");
        self.flows.retain(|key, _| !key.starts_with(&prefix));
    }

    /// Reconnect: the account's sign-in starts at once, the browser pre-filled with its email.
    pub(crate) fn start_reconnect(&mut self, account: &Account, cx: &mut Context<Self>) {
        let key = format!("reconnect:{}", account.id());
        if self.flow_running(&key) {
            return;
        }
        self.flows
            .insert(key.clone(), FlowState::new(account.email(), true, None));
        let provider = if account.provider() == "claude" {
            "claude"
        } else {
            "codex"
        };
        self.start_flow(&key, provider, Some(account.clone()), None, cx);
    }

    /// Starts the one-second status poll of a flow whose job is running.
    fn ensure_flow_poll(&mut self, key: &str, cx: &mut Context<Self>) {
        let Some(flow) = self.flows.get_mut(key) else {
            return;
        };
        let active = flow.job.as_ref().is_some_and(job_active);
        if !active {
            flow.poll = None;
            return;
        }
        if flow.poll.is_some() {
            return;
        }
        let key = key.to_string();
        flow.poll = Some(cx.spawn(async move |this, cx| {
            loop {
                cx.background_executor().timer(JOB_POLL).await;
                let keep = this
                    .update(cx, |page, cx| page.poll_flow_job(&key, cx))
                    .unwrap_or(false);
                if !keep {
                    break;
                }
            }
        }));
    }

    /// One poll: the job's latest state. Returns whether to keep polling.
    fn poll_flow_job(&mut self, key: &str, cx: &mut Context<Self>) -> bool {
        let Some(job_id) = self
            .flows
            .get(key)
            .and_then(|flow| flow.job.as_ref())
            .filter(|job| job_active(job))
            .and_then(|job| job["id"].as_str().map(str::to_string))
        else {
            if let Some(flow) = self.flows.get_mut(key) {
                flow.poll = None;
            }
            return false;
        };
        let this = cx.weak_entity();
        let key = key.to_string();
        super::client::AccountsClient::call(
            &self.client.clone(),
            json!({ "operation": "setupStatus", "owner": ACCOUNT_SETUP_OWNER }),
            move |result, cx| {
                let _ = this.update(cx, |page, cx| {
                    let Some(flow) = page.flows.get_mut(&key) else {
                        return;
                    };
                    let mut connected = None;
                    match result {
                        Ok(state) => {
                            if let Some(next) = state["setupJobs"].as_array().and_then(|jobs| {
                                jobs.iter().find(|job| job["id"] == job_id.as_str())
                            }) {
                                if next["status"] == "complete" {
                                    connected = next["accountId"]
                                        .as_str()
                                        .filter(|id| !id.is_empty())
                                        .map(str::to_string);
                                }
                                flow.job = Some(next.clone());
                            }
                        }
                        Err(error) => flow.error = error,
                    }
                    if let Some(account_id) = connected {
                        page.select_pane(Pane::Account(account_id.clone()), cx);
                        page.mark_connected(account_id, cx);
                    }
                    cx.notify();
                });
            },
            cx,
        );
        true
    }

    /// `start(stop?)`: `setupStart`, optionally sleeping the sessions and closing the Codex
    /// windows that stopped the last attempt.
    pub(crate) fn start_flow(
        &mut self,
        key: &str,
        provider: &'static str,
        account: Option<Account>,
        stop: Option<Value>,
        cx: &mut Context<Self>,
    ) {
        let Some(flow) = self.flows.get_mut(key) else {
            return;
        };
        flow.starting = true;
        flow.error.clear();
        flow.terminal = false;
        let mut params = json!({
            "operation": "setupStart",
            "owner": ACCOUNT_SETUP_OWNER,
            "provider": provider,
            "shareHistory": true,
        });
        if !flow.email.trim().is_empty() {
            params["email"] = json!(flow.email.trim());
        }
        if let Some(account) = &account {
            if account.registered() {
                params["accountId"] = json!(account.id());
            }
            params["selector"] = json!(account.selector());
        }
        if let Some(stop) = stop {
            params["stopCodex"] = json!(true);
            params["sleepSessions"] = Value::Array(
                stop["sessions"]
                    .as_array()
                    .map(|sessions| {
                        sessions
                            .iter()
                            .map(|session| {
                                json!({ "projectId": session["projectId"], "sessionId": session["sessionId"] })
                            })
                            .collect()
                    })
                    .unwrap_or_default(),
            );
        }
        cx.notify();
        let this = cx.weak_entity();
        let key = key.to_string();
        super::client::AccountsClient::call(
            &self.client.clone(),
            params,
            move |result, cx| {
                let _ = this.update(cx, |page, cx| {
                    if let Some(flow) = page.flows.get_mut(&key) {
                        flow.starting = false;
                        match result {
                            Ok(state) => {
                                flow.job = state["setupJobs"].as_array().and_then(|jobs| {
                                    jobs.iter()
                                        .filter(|job| {
                                            job["acknowledged"].as_bool() != Some(true)
                                                && job["provider"] == provider
                                        })
                                        .next_back()
                                        .cloned()
                                });
                            }
                            Err(error) => {
                                flow.error = if error.trim().is_empty() {
                                    "Could not start the sign-in.".into()
                                } else {
                                    error
                                };
                            }
                        }
                    }
                    page.ensure_flow_poll(&key, cx);
                    cx.notify();
                });
            },
            cx,
        );
    }

    /// A job operation (`setupCancel`, `setupInput`) whose failure shows as the flow's error.
    fn flow_job_call(
        &mut self,
        key: &str,
        params: Value,
        clear_code: bool,
        cx: &mut Context<Self>,
    ) {
        let this = cx.weak_entity();
        let key = key.to_string();
        super::client::AccountsClient::call(
            &self.client.clone(),
            params,
            move |result, cx| {
                let _ = this.update(cx, |page, cx| {
                    if let Some(flow) = page.flows.get_mut(&key) {
                        match result {
                            Ok(_) if clear_code => flow.code.clear(),
                            Ok(_) => {}
                            Err(error) => flow.error = error,
                        }
                    }
                    cx.notify();
                });
            },
            cx,
        );
    }

    /// Whether the flow keyed `key` is starting or waiting for its sign-in.
    pub(crate) fn flow_running(&self, key: &str) -> bool {
        self.flows
            .get(key)
            .is_some_and(|flow| flow.starting || flow.job.as_ref().is_some_and(job_active))
    }

    /// Whether the flow keyed `key` has started (running, or finished with a result to show).
    pub(crate) fn flow_started(&self, key: &str) -> bool {
        self.flows
            .get(key)
            .is_some_and(|flow| flow.starting || flow.job.is_some() || !flow.error.is_empty())
    }

    /// The flow keyed `key` (created on first render). `consent_needed` asks to share
    /// conversations (only for a provider's first account); `guide` draws it in the guide dialog's
    /// 12px type instead of the Settings 13px.
    #[allow(clippy::too_many_arguments)]
    pub(crate) fn render_connect_flow(
        &mut self,
        p: &SettingsPalette,
        key: String,
        provider: &'static str,
        account: Option<Account>,
        initial_job: Option<Value>,
        consent_needed: bool,
        guide: bool,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let hide = self.hide_emails(cx);
        if !self.flows.contains_key(&key) {
            self.flows.insert(
                key.clone(),
                FlowState::new(
                    account.as_ref().map(Account::email).unwrap_or_default(),
                    account.is_some() || !consent_needed,
                    initial_job,
                ),
            );
        }
        self.ensure_flow_poll(&key, cx);
        let (email, job, error, starting, terminal, code) = {
            let flow = &self.flows[&key];
            (
                flow.email.clone(),
                flow.job.clone(),
                flow.error.clone(),
                flow.starting,
                flow.terminal,
                flow.code.clone(),
            )
        };
        let text_size = if guide { 12.0 } else { 13.0 };
        let label = provider_label(provider);
        let status = job
            .as_ref()
            .and_then(|job| job["status"].as_str().map(str::to_string));
        let active = starting || job.as_ref().is_some_and(job_active);
        let mut children: Vec<AnyElement> = Vec::new();
        let output = job
            .as_ref()
            .and_then(|job| job["output"].as_str())
            .filter(|output| !output.is_empty())
            .map(str::to_string);
        if active {
            let saving = job.as_ref().is_some_and(|job| job["status"] == "saving");
            let shown_email = job
                .as_ref()
                .and_then(|job| job["email"].as_str())
                .filter(|email| !email.is_empty())
                .map(str::to_string)
                .or_else(|| (!email.is_empty()).then(|| email.clone()));
            let (title, detail) = if starting && job.is_none() {
                (
                    "Opening your browser…".to_string(),
                    format!("Ghostex is starting the {label} sign-in."),
                )
            } else if saving {
                (
                    "Saving the account…".to_string(),
                    "Signed in. Ghostex is checking and saving the login.".to_string(),
                )
            } else {
                (
                    "Waiting for you to sign in…".to_string(),
                    match &shown_email {
                        Some(email) => format!(
                            "Finish in your browser as {}, then come back here.",
                            account_text(email, hide)
                        ),
                        None => format!(
                            "Finish in your browser with the {label} account you want to add, then come back here."
                        ),
                    },
                )
            };
            children.push(notice_box(
                p,
                spinner(format!("{key}-spinner"), p.foreground),
                title,
                detail,
            ));
            let mut actions: Vec<AnyElement> = Vec::new();
            if let Some(url) = job
                .as_ref()
                .and_then(|job| job["url"].as_str())
                .filter(|url| !url.is_empty())
            {
                let url = url.to_string();
                actions.push(settings_sized_button(
                    p,
                    SharedString::from(format!("{key}-open-page")),
                    "Open the page again",
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
            if let Some(job_id) = job.as_ref().map(|job| job["id"].clone()) {
                let cancel_key = key.clone();
                actions.push(settings_sized_button(
                    p,
                    SharedString::from(format!("{key}-cancel")),
                    "Cancel",
                    None,
                    None,
                    SizedButtonVariant::Outline,
                    SizedButtonSize::Sm,
                    false,
                    None,
                    move |page: &mut Self, _window, cx| {
                        page.flow_job_call(
                            &cancel_key,
                            json!({ "operation": "setupCancel", "owner": ACCOUNT_SETUP_OWNER, "jobId": job_id }),
                            false,
                            cx,
                        )
                    },
                    cx,
                ));
            }
            if !actions.is_empty() {
                children.push(
                    h_flex()
                        .flex_wrap()
                        .items_center()
                        .gap(px(8.0))
                        .children(actions)
                        .into_any_element(),
                );
            }
            if !saving
                && let Some(job) = job.as_ref()
                && output.as_deref().is_some_and(asks_for_code)
            {
                children.push(self.render_code_box(p, &key, job, &code, window, cx));
            }
        } else if status.as_deref() == Some("complete") {
            let email = job
                .as_ref()
                .and_then(|job| job["email"].as_str())
                .filter(|email| !email.is_empty())
                .map(|email| format!(" as {}", account_text(email, hide)))
                .unwrap_or_default();
            children.push(notice_box(
                p,
                check_mark(),
                "Connected".to_string(),
                format!("Signed in{email}. Usage is up to date."),
            ));
        } else if status.as_deref() == Some("failed") || !error.is_empty() {
            let job = job.clone().unwrap_or(Value::Null);
            children.push(self.render_flow_failure(
                p,
                &key,
                provider,
                account.as_ref(),
                &job,
                &error,
                hide,
                cx,
            ));
        } else if account.is_none() {
            // CDXC:AgentProviders 2026-11-05 DECISION:
            // User picked "Drop it" for the Share chat history tick box (Q1334) and "No email, like Claude" for Codex (Q1331): adding an account asks nothing. Shared history is always on (the Add account picker says so once) and Codex Swap saves whichever account the browser signs in to. Picking a provider in Add account starts this sign-in at once; this button is only left for a sign-in that could not start (a helper installed meanwhile, a cancelled one). Supersedes the consent box and Codex email of the 2026-10-14 decision above.
            let command = login_command(provider, None, "");
            let start_key = key.clone();
            children.push(
                h_flex()
                    .gap(px(8.0))
                    .child(
                        div()
                            .id(SharedString::from(format!("{key}-start-tooltip")))
                            .flex()
                            .tooltip(tooltip_text(account_text(&command, hide)))
                            .child(settings_sized_button(
                                p,
                                SharedString::from(format!("{key}-start")),
                                "Sign in with browser",
                                None,
                                None,
                                SizedButtonVariant::Primary,
                                SizedButtonSize::Default,
                                false,
                                None,
                                move |page: &mut Self, _window, cx| {
                                    page.start_flow(&start_key, provider, None, None, cx)
                                },
                                cx,
                            )),
                    )
                    .into_any_element(),
            );
        }
        if let Some(output) = output
            && (active || status.as_deref() == Some("failed"))
        {
            children.push(self.render_details(p, &key, terminal, &output, hide, cx));
        }
        v_flex()
            .w_full()
            .items_stretch()
            .gap(px(10.0))
            .text_size(px(text_size))
            .children(children)
            .into_any_element()
    }

    /// The code box: shown while the helper asks for a code the browser displays.
    fn render_code_box(
        &mut self,
        p: &SettingsPalette,
        key: &str,
        job: &Value,
        code: &str,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let code_id = SharedString::from(format!("{key}-code"));
        let code_key = key.to_string();
        let input = FieldStates::text_state(
            self,
            &code_id,
            code,
            Some("Code"),
            move |page: &mut Self, text, _window, cx| {
                if let Some(flow) = page.flows.get_mut(&code_key) {
                    flow.code = text;
                }
                cx.notify();
            },
            window,
            cx,
        );
        let send_key = key.to_string();
        let job_id = job["id"].clone();
        let send = settings_sized_button(
            p,
            SharedString::from(format!("{key}-send-code")),
            "Continue",
            None,
            None,
            SizedButtonVariant::Primary,
            SizedButtonSize::Default,
            code.trim().is_empty(),
            None,
            move |page: &mut Self, _window, cx| {
                let input = page
                    .flows
                    .get(&send_key)
                    .map(|flow| flow.code.clone())
                    .unwrap_or_default();
                page.flow_job_call(
                    &send_key,
                    json!({ "operation": "setupInput", "owner": ACCOUNT_SETUP_OWNER, "jobId": job_id, "input": input }),
                    true,
                    cx,
                )
            },
            cx,
        );
        v_flex()
            .w_full()
            .gap(px(6.0))
            .child(
                div()
                    .text_size(px(12.0))
                    .text_color(hsla(p.muted))
                    .child("If your browser shows a code, paste it here."),
            )
            .child(
                h_flex()
                    .w_full()
                    .gap(px(8.0))
                    .items_center()
                    .child(
                        div()
                            .flex_1()
                            .min_w_0()
                            .child(settings_text_input(p, &input, None, false, window, cx)),
                    )
                    .child(send),
            )
            .into_any_element()
    }

    /// A finished sign-in that did not save the account: the wrong account, Codex still running,
    /// or the helper's error, each with its way forward.
    #[allow(clippy::too_many_arguments)]
    fn render_flow_failure(
        &mut self,
        p: &SettingsPalette,
        key: &str,
        provider: &'static str,
        account: Option<&Account>,
        job: &Value,
        error: &str,
        hide: bool,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let mut children: Vec<AnyElement> = Vec::new();
        let retry_key = key.to_string();
        let retry_account = account.cloned();
        let try_again = settings_sized_button(
            p,
            SharedString::from(format!("{key}-retry")),
            "Try again",
            None,
            None,
            SizedButtonVariant::Primary,
            SizedButtonSize::Sm,
            false,
            None,
            move |page: &mut Self, _window, cx| {
                page.start_flow(&retry_key, provider, retry_account.clone(), None, cx)
            },
            cx,
        );
        if job["wrongAccount"].is_object() {
            let signed_in = job["wrongAccount"]["signedIn"]
                .as_str()
                .unwrap_or_default()
                .to_string();
            let wanted = account
                .map(Account::email)
                .unwrap_or_else(|| job["email"].as_str().unwrap_or_default().to_string());
            let message = match (signed_in.is_empty(), wanted.is_empty()) {
                (false, false) => format!(
                    "You signed in as {}, not {}.",
                    account_text(&signed_in, hide),
                    account_text(&wanted, hide)
                ),
                (false, true) => format!(
                    "You signed in as {}, a different account.",
                    account_text(&signed_in, hide)
                ),
                _ => "The browser signed in to a different account.".to_string(),
            };
            children.push(warning_text(p, message));
            let mut actions = vec![try_again];
            if account.is_some() {
                let label = if signed_in.is_empty() {
                    "Add it as a new account".to_string()
                } else {
                    format!("Add {} as a new account", account_text(&signed_in, hide))
                };
                actions.push(settings_sized_button(
                    p,
                    SharedString::from(format!("{key}-add-instead")),
                    label,
                    None,
                    None,
                    SizedButtonVariant::Outline,
                    SizedButtonSize::Sm,
                    false,
                    None,
                    move |page: &mut Self, _window, cx| {
                        page.select_pane(Pane::Add(provider), cx);
                        let setup_key = format!("setup:{provider}:new");
                        page.flows
                            .insert(setup_key, FlowState::new(signed_in.clone(), true, None));
                        cx.notify();
                    },
                    cx,
                ));
            }
            children.push(button_row(actions));
        } else if let Some(blockers) = job
            .get("blockers")
            .filter(|blockers| blockers.is_object())
            .cloned()
        {
            let sessions = blockers["sessions"].as_array().cloned().unwrap_or_default();
            let others = blockers["others"].as_u64().unwrap_or(0) as usize;
            let plural = |count: usize, word: &str| {
                format!("{count} {word}{}", if count == 1 { "" } else { "s" })
            };
            let mut message = if sessions.is_empty() {
                format!("Codex is open in {}.", plural(others.max(1), "window"))
            } else {
                format!(
                    "{} running Codex on this account. They pause while you sign in and pick up when you open them.",
                    if sessions.len() == 1 {
                        "1 session is".to_string()
                    } else {
                        format!("{} sessions are", sessions.len())
                    }
                )
            };
            if !sessions.is_empty() && others > 0 {
                message.push_str(&format!(
                    " Also closes {} outside Ghostex.",
                    plural(others, "Codex window")
                ));
            } else if sessions.is_empty() {
                message.push_str(" Signing in closes it.");
            }
            children.push(warning_text(p, message));
            if !sessions.is_empty() {
                children.push(
                    v_flex()
                        .pl(px(12.0))
                        .text_size(px(12.0))
                        .text_color(hsla(p.muted))
                        .children(sessions.iter().map(|session| {
                            div().child(format!(
                                "• {}",
                                session["title"].as_str().unwrap_or_default()
                            ))
                        }))
                        .into_any_element(),
                );
            }
            let stop_key = key.to_string();
            let stop_account = account.cloned();
            let label = if sessions.is_empty() {
                "Close Codex and continue".to_string()
            } else {
                format!("Pause {} and continue", plural(sessions.len(), "session"))
            };
            children.push(button_row(vec![settings_sized_button(
                p,
                SharedString::from(format!("{key}-stop-codex")),
                label,
                None,
                None,
                SizedButtonVariant::Primary,
                SizedButtonSize::Sm,
                false,
                None,
                move |page: &mut Self, _window, cx| {
                    page.start_flow(
                        &stop_key,
                        provider,
                        stop_account.clone(),
                        Some(blockers.clone()),
                        cx,
                    )
                },
                cx,
            )]));
        } else {
            let message = if !error.is_empty() {
                error.to_string()
            } else {
                job["error"]
                    .as_str()
                    .filter(|error| !error.is_empty())
                    .unwrap_or("The sign-in did not finish.")
                    .to_string()
            };
            children.push(warning_text(p, account_text(&message, hide)));
            children.push(button_row(vec![try_again]));
        }
        v_flex()
            .w_full()
            .gap(px(10.0))
            .p(px(12.0))
            .rounded(px(MODAL_RADIUS_CONTROL))
            .border_1()
            .border_color(hsla(modal_rgba(0xfbbf24, 0.3)))
            .bg(hsla(modal_rgba(0xfbbf24, 0.05)))
            .children(children)
            .into_any_element()
    }

    /// Details: the helper's output, closed until asked for.
    fn render_details(
        &mut self,
        p: &SettingsPalette,
        key: &str,
        open: bool,
        output: &str,
        hide: bool,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let toggle_key = key.to_string();
        let toggle = h_flex()
            .id(SharedString::from(format!("{key}-details")))
            .gap(px(6.0))
            .items_center()
            .cursor_pointer()
            .text_size(px(12.0))
            .text_color(hsla(p.muted))
            .on_click(cx.listener(move |page, _: &ClickEvent, _window, cx| {
                if let Some(flow) = page.flows.get_mut(&toggle_key) {
                    flow.terminal = !flow.terminal;
                }
                cx.notify();
            }))
            .child(settings_icon(
                if open {
                    "modals/settings/chevron-down.svg"
                } else {
                    "modals/settings/chevron-right.svg"
                },
                12.0,
                p.muted,
            ))
            .child("Details");
        v_flex()
            .w_full()
            .gap(px(6.0))
            .child(toggle)
            .when(open, |this| {
                this.child(
                    div()
                        .id(SharedString::from(format!("{key}-output")))
                        .w_full()
                        .max_h(px(200.0))
                        .overflow_y_scroll()
                        .p(px(10.0))
                        .rounded(px(MODAL_RADIUS_CONTROL))
                        .bg(hsla(if p.glass {
                            p.modal.solid_surface
                        } else {
                            p.surface
                        }))
                        .font_family(MODAL_MONO_FONT)
                        .text_size(px(11.0))
                        .line_height(px(16.0))
                        .text_color(hsla(p.foreground))
                        .child(account_text(output, hide)),
                )
            })
            .into_any_element()
    }
}

/// A rounded box with an icon, a bold line and a muted line ("Waiting for you to sign in…").
fn notice_box(p: &SettingsPalette, icon: AnyElement, title: String, detail: String) -> AnyElement {
    h_flex()
        .w_full()
        .items_start()
        .gap(px(10.0))
        .p(px(12.0))
        .rounded(px(MODAL_RADIUS_CONTROL))
        .border_1()
        .border_color(hsla(p.hairline))
        .bg(hsla(p.foreground_alpha(0.03)))
        .child(div().pt(px(1.0)).child(icon))
        .child(
            v_flex()
                .flex_1()
                .min_w_0()
                .gap(px(2.0))
                .child(
                    div()
                        .text_size(px(13.0))
                        .line_height(px(18.0))
                        .font_weight(FontWeight::MEDIUM)
                        .text_color(hsla(p.foreground))
                        .child(title),
                )
                .child(
                    div()
                        .text_size(px(12.0))
                        .line_height(px(16.0))
                        .text_color(hsla(p.muted))
                        .child(detail),
                ),
        )
        .into_any_element()
}

/// The green check of a finished sign-in.
fn check_mark() -> AnyElement {
    div()
        .size(px(18.0))
        .rounded_full()
        .flex()
        .items_center()
        .justify_center()
        .bg(hsla(modal_rgba(0x34d399, 0.16)))
        .child(settings_icon(
            super::super::super::fields::CHECK_ICON,
            12.0,
            gpui::rgb(0x34d399),
        ))
        .into_any_element()
}

fn warning_text(p: &SettingsPalette, text: String) -> AnyElement {
    div()
        .text_size(px(12.5))
        .line_height(px(17.0))
        .text_color(hsla(if p.light {
            gpui::rgb(0xb45309)
        } else {
            gpui::rgb(0xfcd34d)
        }))
        .child(text)
        .into_any_element()
}

fn button_row(buttons: Vec<AnyElement>) -> AnyElement {
    h_flex()
        .flex_wrap()
        .gap(px(8.0))
        .items_center()
        .children(buttons)
        .into_any_element()
}
