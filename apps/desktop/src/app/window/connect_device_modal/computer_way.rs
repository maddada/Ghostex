//! Connect a computer: this computer's code to copy, a box for the other computer's code, and the
//! pairing that follows on the computer that entered the code (the words, signing in, Connect both
//! ways), each in plain words with one button that fixes what went wrong.
//!
//! CDXC:RemotePairing 2026-11-14 DECISION:
//! User, on the Easy Connect page ("the flow we have now for connecting my machine to another pc is super confusing and dumb... there's no clear guidance, we have 2 windows for it"): Q1 "A. One window, both ways": both computers open the same Connect a computer window, which opens on this computer's code with a box for the other computer's code under it; pasting there pairs this computer with that one, the other approves its three words, and one progress list shows each step until "<computer> is connected". Q2 "Connect back after", named "Connect both ways": that Done screen offers Connect both ways, which makes the other computer connect back with no second code (server/src/remote_access/connect_back.rs). Q3 "Key first, password fallback": no password is asked unless the other computer runs an older Ghostex that didn't keep this computer's key; then the window asks for that computer's password once. Q4 "Two items": Connect a Phone and Connect a Computer are separate items in the sidebar menu and on Settings > Remote, and Add a computer opens this window too, with SSH details one quiet link away. Errors follow the card wording rule: a short title, at most two sentences, one button that fixes it.
//! SEE-ALSO: apps/desktop/src/app/connect_computer_lifecycle.rs (the gxserver calls), server/src/remote_access/computer_pairing.rs.
use super::*;
use gpui_component::input::{InputEvent, InputState};

const ICON_ALERT_COMPUTER: &str = "modals/settings/alert-triangle.svg";
const ICON_STEP_DONE: &str = "modals/settings/circle-check-filled.svg";
/// How often the window asks how a pairing or a sign-in stands.
const JOIN_POLL_EVERY: Duration = Duration::from_secs(1);
/// How many asks a failure left from the attempt before a password or Reconnect is not taken as
/// the new attempt's answer (the new attempt starts within a second or two).
const STALE_FAILURE_POLLS: u8 = 8;

/// The other computer, as its code names it: what saving it after the approval needs.
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct JoinTarget {
    pub(crate) name: String,
    pub(crate) address: String,
    pub(crate) user: String,
    pub(crate) ssh_port: u16,
}

/// Where Connect both ways stands.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum BackStep {
    /// Asking the other computer to connect back.
    Asking,
    /// Approving its request here.
    Approving,
    /// Waiting for it to add this computer.
    Adding,
}

/// The one button a failure offers.
#[derive(Clone, Debug, PartialEq)]
pub(crate) enum JoinFix {
    /// Back to the code screen, for a new code.
    NewCode,
    /// Sign in to the saved computer again.
    Reconnect(String),
    /// Run Connect both ways again.
    ConnectBackAgain(String),
}

#[derive(Clone, Debug, PartialEq)]
pub(crate) struct JoinFailure {
    pub(crate) title: String,
    pub(crate) message: String,
    pub(crate) fix: JoinFix,
}

/// The pairing this computer started by entering the other computer's code.
#[derive(Clone, Debug, PartialEq)]
pub(crate) enum JoinStep {
    /// Asking the other computer to let this one in.
    Asking,
    /// The three words; waiting for Approve there.
    Waiting {
        pending_id: String,
        words: Vec<String>,
    },
    /// Approved: saved here and signing in.
    SigningIn {
        remote_machine_id: String,
    },
    /// The other computer runs an older Ghostex: its password, once.
    NeedsPassword {
        remote_machine_id: String,
        error: Option<String>,
        saving: bool,
    },
    /// This computer can open the other one.
    Connected {
        remote_machine_id: String,
    },
    ConnectingBack {
        remote_machine_id: String,
        step: BackStep,
    },
    /// Both computers can open each other.
    BothWays,
    Failed(JoinFailure),
}

impl JoinStep {
    /// Which screen it is, for sizing the window again when it changes.
    pub(super) fn screen(&self) -> u8 {
        match self {
            Self::Asking => 0,
            Self::Waiting { .. } => 1,
            Self::SigningIn { .. } => 2,
            Self::NeedsPassword { error: None, .. } => 3,
            Self::NeedsPassword { .. } => 4,
            Self::Connected { .. } => 5,
            Self::ConnectingBack { .. } => 6,
            Self::BothWays => 7,
            Self::Failed(_) => 8,
        }
    }
}

pub(crate) struct JoinFlow {
    pub(crate) computer_name: String,
    pub(crate) target: Option<JoinTarget>,
    pub(crate) step: JoinStep,
}

/// The fields the Connect a computer pane types into.
pub(crate) struct ComputerInputs {
    pub(crate) code: Entity<InputState>,
    pub(crate) user: Entity<InputState>,
    pub(crate) password: Entity<InputState>,
    _subscriptions: Vec<gpui::Subscription>,
}

impl ComputerInputs {
    pub(super) fn new(window: &mut Window, cx: &mut Context<GpuiConnectDeviceWindow>) -> Self {
        let code = cx.new(|cx| InputState::new(window, cx).placeholder("Paste it here"));
        let user = cx.new(|cx| InputState::new(window, cx));
        let password = cx.new(|cx| {
            InputState::new(window, cx)
                .masked(true)
                .placeholder("Password")
        });
        let subscriptions = vec![
            cx.subscribe_in(
                &code,
                window,
                |this: &mut GpuiConnectDeviceWindow, _, event: &InputEvent, window, cx| match event
                {
                    InputEvent::Change => {
                        this.code_error = None;
                        cx.notify();
                    }
                    InputEvent::PressEnter { .. } => this.join(window, cx),
                    _ => {}
                },
            ),
            cx.subscribe_in(
                &password,
                window,
                |this: &mut GpuiConnectDeviceWindow, _, event: &InputEvent, window, cx| {
                    if matches!(event, InputEvent::PressEnter { .. }) {
                        this.submit_password(window, cx);
                    }
                },
            ),
        ];
        Self {
            code,
            user,
            password,
            _subscriptions: subscriptions,
        }
    }
}

impl GpuiConnectDeviceWindow {
    // ---- the host's answers ----------------------------------------------------------------------

    /// `/api/pairComputer` answered: the words, or why it didn't start.
    pub(crate) fn join_started(
        &mut self,
        result: Result<(String, Vec<String>, JoinTarget), String>,
        cx: &mut Context<Self>,
    ) {
        let Some(flow) = self.join.as_mut() else {
            return;
        };
        if flow.step != JoinStep::Asking {
            return;
        }
        match result {
            Ok((pending_id, words, target)) => {
                flow.computer_name = target.name.clone();
                flow.target = Some(target);
                flow.step = JoinStep::Waiting { pending_id, words };
                self.start_join_polling(cx);
            }
            Err(message) => {
                let name = flow.computer_name.clone();
                flow.step = JoinStep::Failed(start_failure(&name, &message));
            }
        }
        cx.notify();
    }

    /// `/api/computerPairingStatus` answered (`status`, and whether that computer kept this one's
    /// key and which user it signs in as).
    pub(crate) fn join_status(&mut self, answer: Result<Value, String>, cx: &mut Context<Self>) {
        self.join_poll_in_flight = false;
        let Some(flow) = self.join.as_mut() else {
            return;
        };
        let JoinStep::Waiting { .. } = &flow.step else {
            return;
        };
        let name = flow.computer_name.clone();
        let status = answer
            .as_ref()
            .ok()
            .and_then(|answer| answer.get("status")?.as_str().map(str::to_string));
        match (status.as_deref(), answer) {
            (Some("pending"), _) => {}
            (Some("approved"), Ok(answer)) => {
                let key_installed =
                    answer.get("sshKeyInstalled").and_then(Value::as_bool) == Some(true);
                let mut target = flow.target.clone();
                if let (Some(target), Some(user)) =
                    (target.as_mut(), answer.get("user").and_then(Value::as_str))
                {
                    if !user.trim().is_empty() {
                        target.user = user.trim().to_string();
                    }
                }
                if let Some(target) = target {
                    (self.host)(
                        ConnectDeviceCommand::SaveJoined {
                            target,
                            key_installed,
                        },
                        cx,
                    );
                }
            }
            (Some("denied"), _) => {
                flow.step = JoinStep::Failed(JoinFailure {
                    title: format!("{name} said no"),
                    message: format!(
                        "Someone clicked Not my computer there. Copy a new code on {name} to try again."
                    ),
                    fix: JoinFix::NewCode,
                });
            }
            (_, Err(message)) => {
                flow.step = JoinStep::Failed(JoinFailure {
                    title: format!("Lost touch with {name}"),
                    message: format!("{message} Copy a new code on {name} and try again."),
                    fix: JoinFix::NewCode,
                });
            }
            _ => {
                flow.step = JoinStep::Failed(JoinFailure {
                    title: "Nobody approved in time".to_string(),
                    message: format!("Copy a new code on {name} and try again."),
                    fix: JoinFix::NewCode,
                });
            }
        }
        cx.notify();
    }

    /// The other computer is saved here (`remote_machine_id`): signing in with this computer's key,
    /// or asking for its password when it didn't keep the key.
    pub(crate) fn join_saved(
        &mut self,
        remote_machine_id: String,
        key_installed: bool,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(flow) = self.join.as_mut() else {
            return;
        };
        if key_installed {
            flow.step = JoinStep::SigningIn { remote_machine_id };
        } else {
            let user = flow
                .target
                .as_ref()
                .map(|target| target.user.clone())
                .unwrap_or_default();
            flow.step = JoinStep::NeedsPassword {
                remote_machine_id,
                error: None,
                saving: false,
            };
            self.computer_inputs
                .user
                .update(cx, |input, cx| input.set_value(user, window, cx));
        }
        cx.notify();
    }

    /// How signing in to the saved computer stands (its sidebar state, and why it failed).
    pub(crate) fn sign_in_state(
        &mut self,
        state: Option<String>,
        message: Option<String>,
        cx: &mut Context<Self>,
    ) {
        self.join_poll_in_flight = false;
        let Some(flow) = self.join.as_mut() else {
            return;
        };
        let JoinStep::SigningIn { remote_machine_id } = &flow.step else {
            return;
        };
        let remote_machine_id = remote_machine_id.clone();
        let name = flow.computer_name.clone();
        // Right after a password or a Reconnect the last attempt's failure still stands until the
        // new attempt starts: it is not this attempt's answer.
        let failed = state
            .as_deref()
            .is_some_and(ghostex_gx_core::machine_state_is_failure);
        if self.stale_failure_polls > 0 {
            if failed {
                self.stale_failure_polls -= 1;
                return;
            }
            self.stale_failure_polls = 0;
        }
        match state.as_deref() {
            Some("connected") => {
                flow.step = JoinStep::Connected { remote_machine_id };
            }
            // The key didn't work and no password is saved: the other computer didn't keep it.
            Some("authFailed") => {
                flow.step = JoinStep::NeedsPassword {
                    remote_machine_id,
                    error: None,
                    saving: false,
                };
            }
            Some(state) if ghostex_gx_core::machine_state_is_failure(state) => {
                let (_, reason) = ghostex_gx_core::machine_state_copy(state);
                flow.step = JoinStep::Failed(JoinFailure {
                    title: format!("Can't sign in to {name}"),
                    message: message
                        .or_else(|| reason.map(str::to_string))
                        .unwrap_or_else(|| "The connection didn't work.".to_string()),
                    fix: JoinFix::Reconnect(remote_machine_id),
                });
            }
            _ => {}
        }
        cx.notify();
    }

    /// Connect both ways moved on a step, or ended.
    pub(crate) fn connect_back_progress(
        &mut self,
        result: Result<Option<BackStep>, String>,
        cx: &mut Context<Self>,
    ) {
        let Some(flow) = self.join.as_mut() else {
            return;
        };
        let JoinStep::ConnectingBack {
            remote_machine_id, ..
        } = &flow.step
        else {
            return;
        };
        let remote_machine_id = remote_machine_id.clone();
        let name = flow.computer_name.clone();
        flow.step = match result {
            Ok(Some(step)) => JoinStep::ConnectingBack {
                remote_machine_id,
                step,
            },
            Ok(None) => JoinStep::BothWays,
            Err(message) => JoinStep::Failed(JoinFailure {
                title: if message.starts_with("Update Ghostex on") {
                    format!("Update Ghostex on {name}")
                } else {
                    "Couldn't connect both ways".to_string()
                },
                message,
                fix: JoinFix::ConnectBackAgain(remote_machine_id),
            }),
        };
        cx.notify();
    }

    // ---- actions ---------------------------------------------------------------------------------

    /// Connect: pairs this computer with the one whose code is in the box.
    pub(crate) fn join(&mut self, _window: &mut Window, cx: &mut Context<Self>) {
        if self.join.is_some() {
            return;
        }
        let code = self
            .computer_inputs
            .code
            .read(cx)
            .value()
            .trim()
            .to_string();
        if code.is_empty() {
            self.code_error = Some("Paste the other computer's code first.".to_string());
            cx.notify();
            return;
        }
        if self
            .code
            .as_ref()
            .is_some_and(|own| own.text.trim() == code)
        {
            self.code_error = Some(
                "That is this computer's own code. Paste the code the other computer shows."
                    .to_string(),
            );
            cx.notify();
            return;
        }
        let Some(target) = join_target(&code) else {
            self.code_error = Some(
                "That isn't a computer's code. On the other computer, click Connect a Computer and copy its code."
                    .to_string(),
            );
            cx.notify();
            return;
        };
        self.join = Some(JoinFlow {
            computer_name: target.name.clone(),
            target: Some(target),
            step: JoinStep::Asking,
        });
        cx.notify();
        (self.host)(ConnectDeviceCommand::JoinComputer { code }, cx);
    }

    /// The preview binary's `computer-*` states: `code` pasted and Connect pressed.
    #[allow(dead_code)]
    pub(crate) fn preview_join(&mut self, code: &str, window: &mut Window, cx: &mut Context<Self>) {
        if self.join.is_some() {
            return;
        }
        let code = code.to_string();
        self.computer_inputs
            .code
            .update(cx, |input, cx| input.set_value(code, window, cx));
        self.join(window, cx);
    }

    /// The preview binary's `computer-both` state: Connect both ways pressed.
    #[allow(dead_code)]
    pub(crate) fn preview_connect_back(&mut self, cx: &mut Context<Self>) {
        self.connect_back(cx);
    }

    fn start_join_polling(&mut self, cx: &mut Context<Self>) {
        if self.join_polling.is_some() {
            return;
        }
        self.join_polling = Some(cx.spawn(async move |this, cx| {
            loop {
                cx.background_executor().timer(JOIN_POLL_EVERY).await;
                let Ok(keep_going) = this.update(cx, |this, cx| this.poll_join(cx)) else {
                    break;
                };
                if !keep_going {
                    break;
                }
            }
        }));
    }

    /// One ask about the pairing or the sign-in; `false` once nothing is in flight any more.
    fn poll_join(&mut self, cx: &mut Context<Self>) -> bool {
        let command = match self.join.as_ref().map(|flow| &flow.step) {
            Some(JoinStep::Waiting { pending_id, .. }) => ConnectDeviceCommand::JoinStatus {
                pending_id: pending_id.clone(),
            },
            Some(JoinStep::SigningIn { remote_machine_id }) => ConnectDeviceCommand::ReadSignIn {
                remote_machine_id: remote_machine_id.clone(),
            },
            Some(JoinStep::Asking | JoinStep::NeedsPassword { .. }) => return true,
            _ => {
                self.join_polling = None;
                return false;
            }
        };
        if !self.join_poll_in_flight {
            self.join_poll_in_flight = true;
            (self.host)(command, cx);
        }
        true
    }

    pub(crate) fn submit_password(&mut self, _window: &mut Window, cx: &mut Context<Self>) {
        let Some(flow) = self.join.as_mut() else {
            return;
        };
        let JoinStep::NeedsPassword {
            remote_machine_id,
            saving: false,
            ..
        } = &flow.step
        else {
            return;
        };
        let remote_machine_id = remote_machine_id.clone();
        let user = self
            .computer_inputs
            .user
            .read(cx)
            .value()
            .trim()
            .to_string();
        let password = self.computer_inputs.password.read(cx).value().to_string();
        if user.is_empty() || password.is_empty() {
            flow.step = JoinStep::NeedsPassword {
                remote_machine_id,
                error: Some("Enter the username and password you sign in with there.".to_string()),
                saving: false,
            };
            cx.notify();
            return;
        }
        flow.step = JoinStep::SigningIn {
            remote_machine_id: remote_machine_id.clone(),
        };
        self.stale_failure_polls = STALE_FAILURE_POLLS;
        self.start_join_polling(cx);
        cx.notify();
        (self.host)(
            ConnectDeviceCommand::SubmitPassword {
                remote_machine_id,
                user,
                password,
            },
            cx,
        );
    }

    fn connect_back(&mut self, cx: &mut Context<Self>) {
        let Some(flow) = self.join.as_mut() else {
            return;
        };
        let remote_machine_id = match &flow.step {
            JoinStep::Connected { remote_machine_id } => remote_machine_id.clone(),
            JoinStep::Failed(JoinFailure {
                fix: JoinFix::ConnectBackAgain(remote_machine_id),
                ..
            }) => remote_machine_id.clone(),
            _ => return,
        };
        let Some(code) = self.code.as_ref().map(|code| code.text.clone()) else {
            return;
        };
        flow.step = JoinStep::ConnectingBack {
            remote_machine_id: remote_machine_id.clone(),
            step: BackStep::Asking,
        };
        cx.notify();
        (self.host)(
            ConnectDeviceCommand::ConnectBack {
                remote_machine_id,
                code,
            },
            cx,
        );
    }

    fn apply_fix(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(JoinStep::Failed(failure)) = self.join.as_ref().map(|flow| flow.step.clone())
        else {
            return;
        };
        match failure.fix {
            JoinFix::NewCode => {
                self.join = None;
                self.join_polling = None;
                self.computer_inputs
                    .code
                    .update(cx, |input, cx| input.set_value("", window, cx));
                cx.notify();
            }
            JoinFix::Reconnect(remote_machine_id) => {
                if let Some(flow) = self.join.as_mut() {
                    flow.step = JoinStep::SigningIn {
                        remote_machine_id: remote_machine_id.clone(),
                    };
                }
                self.stale_failure_polls = STALE_FAILURE_POLLS;
                self.start_join_polling(cx);
                cx.notify();
                (self.host)(ConnectDeviceCommand::Reconnect { remote_machine_id }, cx);
            }
            JoinFix::ConnectBackAgain(_) => self.connect_back(cx),
        }
    }

    fn cancel_join(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if let Some(JoinStep::Waiting { pending_id, .. }) =
            self.join.as_ref().map(|flow| flow.step.clone())
        {
            (self.host)(ConnectDeviceCommand::CancelJoin { pending_id }, cx);
        }
        self.join = None;
        self.join_polling = None;
        self.computer_inputs
            .code
            .update(cx, |input, cx| input.set_value("", window, cx));
        cx.notify();
    }

    // ---- drawing ---------------------------------------------------------------------------------

    /// The window's title while a pairing this computer started runs.
    pub(super) fn join_title(&self) -> Option<String> {
        let flow = self.join.as_ref()?;
        let name = &flow.computer_name;
        Some(match &flow.step {
            JoinStep::Asking | JoinStep::SigningIn { .. } => format!("Connecting to {name}"),
            JoinStep::Waiting { .. } => "Check the words".to_string(),
            JoinStep::NeedsPassword { .. } => format!("Sign in to {name}"),
            JoinStep::Connected { .. } => format!("{name} is connected"),
            JoinStep::ConnectingBack { .. } => "Connecting both ways".to_string(),
            JoinStep::BothWays => "Computers connected".to_string(),
            JoinStep::Failed(failure) => failure.title.clone(),
        })
    }

    fn label(&self, text: &'static str) -> Div {
        div()
            .text_size(px(13.0))
            .line_height(px(18.0))
            .font_weight(FontWeight::MEDIUM)
            .text_color(hsla(self.palette.foreground))
            .child(text)
    }

    fn or_divider(&self) -> AnyElement {
        let p = self.palette;
        let rule = || div().flex_1().h(px(1.0)).bg(hsla(p.hairline));
        h_flex()
            .w_full()
            .gap(px(10.0))
            .items_center()
            .child(rule())
            .child(
                div()
                    .text_size(px(12.0))
                    .text_color(hsla(p.muted))
                    .child("or"),
            )
            .child(rule())
            .into_any_element()
    }

    /// This computer's code with Copy, then the box for the other computer's code with Connect.
    pub(super) fn computer_way(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let p = self.palette;
        let ready = self.stage == Stage::Ready && self.code.is_some();
        let own_code: AnyElement = if ready {
            let text = self
                .code
                .as_ref()
                .map(|code| code.text.clone())
                .unwrap_or_default();
            v_flex()
                .w_full()
                .gap(px(6.0))
                .child(self.label("This computer's code"))
                .child(
                    h_flex()
                        .w_full()
                        .gap(px(8.0))
                        .child(
                            div()
                                .id("connect-computer-own-code")
                                .flex_1()
                                .min_w_0()
                                .h(px(MODAL_CONTROL_HEIGHT))
                                .px(px(12.0))
                                .flex()
                                .items_center()
                                .rounded(px(MODAL_RADIUS_CONTROL))
                                .border_1()
                                .border_color(hsla(p.hairline))
                                .bg(hsla(p.panel))
                                .child(
                                    div()
                                        .w_full()
                                        .min_w_0()
                                        .overflow_hidden()
                                        .whitespace_nowrap()
                                        .text_ellipsis()
                                        .font_family(MODAL_MONO_FONT)
                                        .text_size(px(12.0))
                                        .text_color(hsla(p.muted))
                                        .child(text),
                                ),
                        )
                        .child(
                            div().flex_shrink_0().child(modal_action_button(
                                &p,
                                "connect-computer-copy",
                                if self.copied { "Copied" } else { "Copy" },
                                Some(
                                    modal_icon(
                                        if self.copied { ICON_CHECK } else { ICON_COPY },
                                        14.0,
                                        p.foreground,
                                    )
                                    .into_any_element(),
                                ),
                                ModalButtonTone::Neutral,
                                false,
                                |this: &mut Self, _window, cx| this.copy_code(cx),
                                cx,
                            )),
                        ),
                )
                .child(
                    self.muted("Works once, for five minutes.")
                        .text_size(px(12.0)),
                )
                .into_any_element()
        } else {
            self.code_placeholder(120.0, cx)
        };
        let paste = v_flex()
            .w_full()
            .gap(px(6.0))
            .child(self.label("The other computer's code"))
            .child(
                h_flex()
                    .w_full()
                    .gap(px(8.0))
                    .child(div().flex_1().min_w_0().child(modal_text_input(
                        &p,
                        &self.computer_inputs.code,
                        !ready,
                        window,
                        cx,
                    )))
                    .child(div().flex_shrink_0().child(modal_action_button(
                        &p,
                        "connect-computer-connect",
                        "Connect",
                        None,
                        ModalButtonTone::Primary,
                        !ready,
                        |this: &mut Self, window, cx| this.join(window, cx),
                        cx,
                    ))),
            )
            .children(self.code_error.clone().map(|error| {
                div()
                    .text_size(px(12.5))
                    .line_height(px(17.0))
                    .text_color(hsla(p.destructive))
                    .child(error)
            }));
        v_flex()
            .w_full()
            .gap(px(14.0))
            .child(own_code)
            .child(self.or_divider())
            .child(paste)
            .child(
                h_flex()
                    .w_full()
                    .justify_between()
                    .items_center()
                    .child(self.muted(
                        "Copy one code to the other computer, or paste its code here. Either way works.",
                    )
                    .text_size(px(12.0))
                    .flex_1()
                    .min_w_0())
                    .child(div().flex_shrink_0().pl(px(12.0)).child(self.link(
                        "connect-computer-ssh",
                        "Use SSH details",
                        |this, window, cx| {
                            this.close_and_send(ConnectDeviceCommand::OpenAddComputerSsh, window, cx)
                        },
                        cx,
                    ))),
            )
            .into_any_element()
    }

    /// One line of the progress list: done (a check), running (a spinner) or still to come.
    fn progress_row(&self, text: String, state: Option<bool>) -> AnyElement {
        let p = self.palette;
        let mark: AnyElement = match state {
            Some(true) => modal_icon(ICON_STEP_DONE, 16.0, p.success).into_any_element(),
            Some(false) => modal_spinner(p.muted).into_any_element(),
            None => div()
                .size(px(14.0))
                .m(px(1.0))
                .rounded_full()
                .border_1()
                .border_color(hsla(p.hairline))
                .into_any_element(),
        };
        h_flex()
            .w_full()
            .gap(px(10.0))
            .items_center()
            .child(div().w(px(16.0)).flex_shrink_0().child(mark))
            .child(
                div()
                    .flex_1()
                    .min_w_0()
                    .text_size(px(13.0))
                    .line_height(px(19.0))
                    .text_color(hsla(if state == Some(true) {
                        p.foreground
                    } else {
                        p.muted
                    }))
                    .child(text),
            )
            .into_any_element()
    }

    fn progress_list(&self, rows: Vec<(String, Option<bool>)>) -> AnyElement {
        v_flex()
            .w_full()
            .gap(px(8.0))
            .children(
                rows.into_iter()
                    .map(|(text, state)| self.progress_row(text, state)),
            )
            .into_any_element()
    }

    /// The window while a pairing this computer started runs.
    pub(super) fn join_body(&mut self, window: &mut Window, cx: &mut Context<Self>) -> AnyElement {
        let p = self.palette;
        let Some(flow) = self.join.as_ref() else {
            return div().into_any_element();
        };
        let name = flow.computer_name.clone();
        let this_computer = self.computer_name.clone();
        let step = flow.step.clone();
        let approved = format!("Approved on {name}");
        let opens = format!("This computer can open {name}");
        match step {
            JoinStep::Asking => v_flex()
                .w_full()
                .gap(px(16.0))
                .child(self.progress_list(vec![
                    (format!("Asking {name} to let this computer in"), Some(false)),
                    (approved, None),
                    (opens, None),
                ]))
                .into_any_element(),
            JoinStep::Waiting { words, .. } => v_flex()
                .w_full()
                .gap(px(16.0))
                .child(
                    h_flex()
                        .gap(px(8.0))
                        .items_center()
                        .child(modal_icon(ICON_COMPUTER, 16.0, p.foreground))
                        .child(name.clone()),
                )
                .child(pairing_words(&p, &words, false))
                .child(self.muted(format!(
                    "{name} shows three words now. Click Approve there only if they are the same as these."
                )))
                .child(self.progress_list(vec![
                    (format!("Waiting for Approve on {name}"), Some(false)),
                    (opens, None),
                ]))
                .child(modal_footer(vec![modal_action_button(
                    &p,
                    "connect-computer-cancel",
                    "Cancel",
                    None,
                    ModalButtonTone::Neutral,
                    false,
                    |this: &mut Self, window, cx| this.cancel_join(window, cx),
                    cx,
                )]))
                .into_any_element(),
            JoinStep::SigningIn { .. } => v_flex()
                .w_full()
                .gap(px(16.0))
                .child(self.progress_list(vec![
                    (approved, Some(true)),
                    (format!("Signing in to {name}"), Some(false)),
                ]))
                .into_any_element(),
            JoinStep::NeedsPassword { error, saving, .. } => v_flex()
                .w_full()
                .gap(px(14.0))
                .child(self.muted(format!(
                    "{name} runs an older Ghostex, so this computer signs in with its password. Update Ghostex there to skip this next time."
                )))
                .child(
                    v_flex()
                        .gap(px(6.0))
                        .child(self.label("Username"))
                        .child(modal_text_input(
                            &p,
                            &self.computer_inputs.user,
                            saving,
                            window,
                            cx,
                        )),
                )
                .child(
                    v_flex()
                        .gap(px(6.0))
                        .child(self.label("Password"))
                        .child(modal_text_input(
                            &p,
                            &self.computer_inputs.password,
                            saving,
                            window,
                            cx,
                        )),
                )
                .children(error.map(|error| modal_error(&p, error)))
                .child(modal_footer(vec![modal_action_button(
                    &p,
                    "connect-computer-sign-in",
                    "Sign in",
                    None,
                    ModalButtonTone::Primary,
                    saving,
                    |this: &mut Self, window, cx| this.submit_password(window, cx),
                    cx,
                )]))
                .into_any_element(),
            JoinStep::Connected { .. } => v_flex()
                .w_full()
                .gap(px(20.0))
                .child(self.progress_list(vec![
                    (approved, Some(true)),
                    (
                        format!("This computer can open {name}'s chats and terminals"),
                        Some(true),
                    ),
                ]))
                .child(modal_footer(vec![
                    modal_action_button(
                        &p,
                        "connect-computer-both-ways",
                        "Connect both ways",
                        None,
                        ModalButtonTone::Neutral,
                        false,
                        |this: &mut Self, _window, cx| this.connect_back(cx),
                        cx,
                    ),
                    modal_action_button(
                        &p,
                        "connect-computer-done",
                        "Done",
                        None,
                        ModalButtonTone::Primary,
                        false,
                        |this: &mut Self, window, cx| this.close(window, cx),
                        cx,
                    ),
                ]))
                .into_any_element(),
            JoinStep::ConnectingBack { step, .. } => {
                let state = |at: BackStep| match (step, at) {
                    (now, at) if now == at => Some(false),
                    (BackStep::Approving, BackStep::Asking)
                    | (BackStep::Adding, BackStep::Asking | BackStep::Approving) => Some(true),
                    _ => None,
                };
                v_flex()
                    .w_full()
                    .gap(px(16.0))
                    .child(self.progress_list(vec![
                        (opens, Some(true)),
                        (
                            format!("Asking {name} to connect back"),
                            state(BackStep::Asking),
                        ),
                        (
                            format!("Approving {name} on this computer"),
                            state(BackStep::Approving),
                        ),
                        (
                            format!("{name} can open {this_computer}"),
                            state(BackStep::Adding),
                        ),
                    ]))
                    .into_any_element()
            }
            JoinStep::BothWays => v_flex()
                .w_full()
                .gap(px(20.0))
                .child(
                    h_flex()
                        .w_full()
                        .items_start()
                        .gap(px(10.0))
                        .child(
                            div()
                                .mt(px(1.0))
                                .flex_shrink_0()
                                .child(modal_icon(ICON_STEP_DONE, 18.0, p.success)),
                        )
                        .child(
                            self.muted(format!(
                                "{this_computer} and {name} can open each other's chats and terminals. Remove either one any time in Settings > Remote."
                            ))
                            .flex_1()
                            .min_w_0(),
                        ),
                )
                .child(modal_footer(vec![modal_action_button(
                    &p,
                    "connect-computer-both-done",
                    "Done",
                    None,
                    ModalButtonTone::Primary,
                    false,
                    |this: &mut Self, window, cx| this.close(window, cx),
                    cx,
                )]))
                .into_any_element(),
            JoinStep::Failed(failure) => v_flex()
                .w_full()
                .gap(px(16.0))
                .child(
                    h_flex()
                        .w_full()
                        .items_start()
                        .gap(px(10.0))
                        .child(
                            div()
                                .mt(px(1.0))
                                .flex_shrink_0()
                                .child(modal_icon(ICON_ALERT_COMPUTER, 18.0, p.destructive)),
                        )
                        .child(self.muted(failure.message.clone()).flex_1().min_w_0()),
                )
                .child(modal_footer(vec![modal_action_button(
                    &p,
                    "connect-computer-fix",
                    match failure.fix {
                        JoinFix::NewCode => "Try another code",
                        JoinFix::Reconnect(_) => "Reconnect",
                        JoinFix::ConnectBackAgain(_) => "Try again",
                    },
                    None,
                    ModalButtonTone::Primary,
                    false,
                    |this: &mut Self, window, cx| this.apply_fix(window, cx),
                    cx,
                )]))
                .into_any_element(),
        }
    }
}

/// The computer a pasted code names; `None` for anything but a computer's code with its secret.
pub(crate) fn join_target(code: &str) -> Option<JoinTarget> {
    let (name, address, user, ssh_port) =
        super::super::settings_modal::tabs::remote::easy_connect_join_target(code)?;
    Some(JoinTarget {
        name,
        address,
        user,
        ssh_port,
    })
}

/// Why `/api/pairComputer` didn't start, in the window's words.
fn start_failure(name: &str, message: &str) -> JoinFailure {
    if message.contains("different words") {
        return JoinFailure {
            title: "The words didn't match".to_string(),
            message: format!(
                "Pairing stopped to keep this computer safe. Copy a new code on {name} and try again."
            ),
            fix: JoinFix::NewCode,
        };
    }
    if message.contains("expired")
        || message.contains("already used")
        || message.contains("used up")
    {
        return JoinFailure {
            title: "That code ran out".to_string(),
            message: format!("A code works once, for five minutes. Copy a new one on {name}."),
            fix: JoinFix::NewCode,
        };
    }
    if message.contains("did not answer") || message.contains("tunnel") {
        return JoinFailure {
            title: format!("Can't reach {name}"),
            message: format!(
                "Keep Connect a Computer open on {name}, check both computers are online, then try again."
            ),
            fix: JoinFix::NewCode,
        };
    }
    JoinFailure {
        title: format!("Can't connect to {name}"),
        message: message.to_string(),
        fix: JoinFix::NewCode,
    }
}
