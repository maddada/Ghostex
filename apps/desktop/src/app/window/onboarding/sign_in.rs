//! The Agents panel's sign-in line: whether the chosen agent is installed and signed in, and its own
//! browser sign-in when it is not (gxserver `/api/agentSignIn`, server/src/agent_sign_in.rs).
//!
//! CDXC:Onboarding 2026-11-11 DECISION:
//! User, on the first-run page (step 15), picked "Check install and sign-in during setup": setup checks that the chosen agent is installed and signed in, says so, and offers the sign-in when it is not, so the first message goes through instead of meeting a "Sign in to Claude" card. Agents whose CLI cannot say (anything but Claude Code, Codex and Cursor) only read as installed.
//! SEE-ALSO: server/src/agent_sign_in.rs (the status and login commands), apps/desktop/src/app/onboarding_modal_lifecycle.rs (`AgentSignIn`).
use super::interact;
use super::model::{default_agent_id, installed_agents};
use super::primitives::*;
use super::stage::*;
use super::{GpuiOnboardingWindow, OnboardingCommand};
use gpui::prelude::FluentBuilder as _;
use gpui::{AnyElement, Context, IntoElement, ParentElement as _, Styled as _, div};
use serde_json::Value;
use std::time::{Duration, Instant};

/// How often a sign-in in the browser is asked about while it runs.
const SIGN_IN_POLL: Duration = Duration::from_secs(2);

/// What the panel knows about the chosen agent's sign-in.
#[derive(Clone, Debug, Default)]
pub(crate) struct SignInState {
    pub(crate) agent_id: String,
    /// A status read is on its way.
    pub(crate) checking: bool,
    /// `None` until read, or when the CLI cannot say.
    pub(crate) signed_in: Option<bool>,
    pub(crate) read: bool,
    pub(crate) signing_in: bool,
    pub(crate) url: Option<String>,
    pub(crate) failed: bool,
    pub(crate) next_poll: Option<Instant>,
}

impl GpuiOnboardingWindow {
    /// The agent the first session starts with: the default when it is installed, else the first
    /// installed one (the radio the panel shows as chosen).
    fn chosen_agent(&self) -> Option<(String, String)> {
        let id = default_agent_id(&self.settings, &self.agents)?;
        installed_agents(&self.agents)
            .into_iter()
            .find(|agent| agent.agent_id == id)
            .map(|agent| (agent.agent_id.clone(), agent.name.clone()))
    }

    fn ask_sign_in(&mut self, agent_id: String, operation: &'static str, cx: &mut Context<Self>) {
        self.sign_in.checking = true;
        self.send(
            OnboardingCommand::AgentSignIn {
                agent_id,
                operation,
            },
            cx,
        );
    }

    /// Reads the chosen agent's sign-in once the scan settles, again whenever the choice changes,
    /// and every two seconds while a browser sign-in runs.
    pub(super) fn tick_sign_in(&mut self, now: Instant, cx: &mut Context<Self>) {
        if self.agents_loading {
            return;
        }
        let Some((agent_id, _)) = self.chosen_agent() else {
            self.sign_in = SignInState::default();
            return;
        };
        if self.sign_in.agent_id != agent_id {
            let previous = std::mem::take(&mut self.sign_in);
            if previous.signing_in && !previous.agent_id.is_empty() {
                self.send(
                    OnboardingCommand::AgentSignIn {
                        agent_id: previous.agent_id,
                        operation: "cancel",
                    },
                    cx,
                );
            }
            self.sign_in.agent_id = agent_id.clone();
            self.ask_sign_in(agent_id, "status", cx);
            return;
        }
        if !self.sign_in.checking
            && self.sign_in.signing_in
            && self.sign_in.next_poll.is_some_and(|at| now >= at)
        {
            self.sign_in.next_poll = None;
            self.ask_sign_in(agent_id, "status", cx);
        }
    }

    /// An `/api/agentSignIn` answer (or `None` when gxserver could not be asked).
    pub(crate) fn receive_agent_sign_in(
        &mut self,
        agent_id: &str,
        answer: Option<Value>,
        cx: &mut Context<Self>,
    ) {
        if self.sign_in.agent_id != agent_id {
            return;
        }
        let state = &mut self.sign_in;
        state.checking = false;
        state.read = true;
        match answer {
            Some(answer) => {
                state.signed_in = answer.get("signedIn").and_then(Value::as_bool);
                state.signing_in = answer.get("signingIn").and_then(Value::as_bool) == Some(true);
                state.url = answer
                    .get("signInUrl")
                    .and_then(Value::as_str)
                    .map(str::to_string);
                if answer.get("signInFailed").and_then(Value::as_bool) == Some(true) {
                    state.failed = true;
                }
                if state.signed_in == Some(true) {
                    state.failed = false;
                }
            }
            None => {
                state.signed_in = None;
                state.signing_in = false;
            }
        }
        state.next_poll = state.signing_in.then(|| Instant::now() + SIGN_IN_POLL);
        cx.notify();
    }

    fn start_sign_in(&mut self, cx: &mut Context<Self>) {
        let agent_id = self.sign_in.agent_id.clone();
        if agent_id.is_empty() {
            return;
        }
        self.sign_in.failed = false;
        self.sign_in.signing_in = true;
        self.ask_sign_in(agent_id, "start", cx);
        cx.notify();
    }

    /// The preview binary presses Sign in.
    #[allow(dead_code)]
    pub(crate) fn preview_start_sign_in(&mut self, cx: &mut Context<Self>) {
        self.start_sign_in(cx);
    }

    fn cancel_sign_in(&mut self, cx: &mut Context<Self>) {
        let agent_id = self.sign_in.agent_id.clone();
        self.sign_in.signing_in = false;
        self.sign_in.next_poll = None;
        self.ask_sign_in(agent_id, "cancel", cx);
        cx.notify();
    }

    /// The line under the integration row: what the chosen agent needs before the first message.
    pub(super) fn render_sign_in_line(
        &mut self,
        s: S,
        now: Instant,
        cx: &mut Context<Self>,
    ) -> Option<AnyElement> {
        if self.agents_loading {
            return None;
        }
        let chosen = self.chosen_agent();
        let state = self.sign_in.clone();
        let (lead, text, actions): (AnyElement, String, Vec<AnyElement>) = match chosen {
            None => (
                icon(s, "x", 18.0, 1.6, hex(0xffc46b)).into_any_element(),
                "No agent is installed yet. Install one above, so your first message has an agent to answer it.".to_string(),
                Vec::new(),
            ),
            Some((_, name)) if state.checking && !state.signing_in => (
                spinner(s, 16.0, 1.8, self.opened_at, now).into_any_element(),
                format!("Checking that {name} is signed in…"),
                Vec::new(),
            ),
            Some((_, name)) if state.signing_in => {
                let mut actions = Vec::new();
                if let Some(url) = state.url.clone() {
                    actions.push(
                        self.control(
                            s,
                            ghost(s, "sign-in-page", "Open the sign-in page", 14.5, false),
                            "sign-in-page",
                            interact::Ring::new(0.0, 0.0),
                            interact::Keys::EnterSpace,
                            cx,
                            move |this, _, cx| {
                                this.send(OnboardingCommand::OpenExternalUrl(url.clone()), cx)
                            },
                        )
                        .into_any_element(),
                    );
                }
                actions.push(
                    self.control(
                        s,
                        ghost(s, "sign-in-cancel", "Cancel", 14.5, false),
                        "sign-in-cancel",
                        interact::Ring::new(0.0, 0.0),
                        interact::Keys::EnterSpace,
                        cx,
                        |this, _, cx| this.cancel_sign_in(cx),
                    )
                    .into_any_element(),
                );
                (
                    spinner(s, 16.0, 1.8, self.opened_at, now).into_any_element(),
                    format!("Finish signing in to {name} in the browser that opened."),
                    actions,
                )
            }
            Some((_, name)) if state.signed_in == Some(false) => (
                icon(s, "lock", 18.0, 1.6, hex(0xffc46b)).into_any_element(),
                if state.failed {
                    format!("The sign-in to {name} didn't finish. Try again, so your first message goes through.")
                } else {
                    format!("{name} isn't signed in yet. Sign in now, so your first message goes through.")
                },
                vec![
                    self.control(
                        s,
                        cta(
                            s,
                            "sign-in-start",
                            if state.failed { "Try again" } else { "Sign in" },
                            true,
                            false,
                            false,
                            CtaSize::Small,
                        ),
                        "sign-in-start",
                        interact::Ring::new(10.0, 1.0),
                        interact::Keys::EnterSpace,
                        cx,
                        |this, _, cx| this.start_sign_in(cx),
                    )
                    .into_any_element(),
                ],
            ),
            Some((_, name)) if state.signed_in == Some(true) => (
                icon(s, "checkCircle", 18.0, 1.6, hex(0x3be3a2)).into_any_element(),
                format!("{name} is installed and signed in."),
                Vec::new(),
            ),
            // The CLI cannot say whether it is signed in, or it was not read yet.
            Some((_, name)) if state.read => (
                icon(s, "checkCircle", 18.0, 1.6, hex(0x3be3a2)).into_any_element(),
                format!("{name} is installed."),
                Vec::new(),
            ),
            Some(_) => return None,
        };
        Some(
            glass(s)
                .absolute()
                .left(s.px(48.0))
                .top(s.px(660.0))
                .w(s.px(706.0))
                .min_h(s.px(64.0))
                .flex()
                .items_center()
                .gap(s.px(16.0))
                .py(s.px(10.0))
                .pl(s.px(22.0))
                .pr(s.px(14.0))
                .child(div().flex_none().child(lead))
                .child(
                    sans(s, 14.5, 400.0, hex(0xdfe4ee))
                        .flex_1()
                        .min_w_0()
                        .line_height(s.px(14.5 * 1.4))
                        .child(text),
                )
                .when(!actions.is_empty(), |this| {
                    this.child(
                        div()
                            .flex_none()
                            .flex()
                            .items_center()
                            .gap(s.px(10.0))
                            .children(actions),
                    )
                })
                .into_any_element(),
        )
    }
}
