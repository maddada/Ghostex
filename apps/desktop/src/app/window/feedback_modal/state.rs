use super::offline_issue::{FeedbackScrubContext, offline_issue};
use crate::app::gx_store::{GxRpcError, gx_rpc, gx_rpc_with_timeout, write_client_document_value};
use crate::app::model::GpuiRemoteGxserverRequestTarget;
use crate::app::window::native_modal_kit::*;
use base64::Engine as _;
use gpui::{
    App, AppContext as _, ClipboardItem, Context, Entity, FocusHandle, Image, KeyDownEvent,
    Subscription, Window,
};
use gpui_component::input::{Enter, Escape, InputEvent, InputState, TextareaState};
use serde_json::{Value, json};
use std::rc::Rc;
use std::sync::Arc;
use std::time::Duration;

pub(crate) const FEEDBACK_MODAL_WIDTH: f32 = 600.0;
pub(crate) const FEEDBACK_MODAL_INITIAL_HEIGHT: f32 = 600.0;

/// Five images of 5 MiB in base64 can take minutes to upload on a slow line; gxserver gives the
/// relay 170 seconds. Starting the feedback agent sends the same images, and gxserver answers it
/// once the agent has its brief, within 90 seconds.
const SEND_TIMEOUT: Duration = Duration::from_secs(180);

/// gxserver's answer when the feedback agent could not take the report (server/src/feedback/brief_wait.rs).
const FEEDBACK_AGENT_NOT_READY: &str = "feedbackAgentNotReady";

/// What the pop-up asks its host to do. The pop-up removes its own window before sending it.
pub(crate) enum FeedbackModalCommand {
    Closed,
    /// gxserver started the feedback agent in this session; the host opens it. The ids are that
    /// machine's own: `remote_machine_id` names the remote machine it runs on, `None` this computer.
    OpenSession {
        remote_machine_id: Option<String>,
        project_id: String,
        session_id: String,
        agent_id: String,
    },
}

/// The remote machine the feedback agent starts on.
///
/// CDXC:Feedback 2026-10-18 WHY:
/// Start Agent always asked this computer's gxserver, so a report about a session on a remote machine got an agent that read this computer's logs and settings instead of the machine where the problem is. The machine is the focused session's (else the active project's) when the dialog opens, and the agent starts there through the same SSH tunnel `gx_rpc` already uses for remote sessions (Ghostex Capture, the Git menu); that gxserver saves the screenshots, runs the agent in its own Ghostex config folder, and the agent files the issue with that machine's `ghostex feedback send`. A machine that is not connected is reported as such, never swapped for this computer.
#[derive(Clone)]
pub(crate) struct FeedbackAgentMachine {
    pub(crate) machine_id: String,
    pub(crate) name: String,
    /// The live tunnel to the machine's gxserver; `None` while it is not connected.
    pub(crate) target: Option<GpuiRemoteGxserverRequestTarget>,
    /// The agent to start, as that machine knows it (a custom agent of this computer travels as
    /// the built-in agent its icon names).
    pub(crate) agent_id: Option<String>,
}

pub(crate) type FeedbackModalHost = Rc<dyn Fn(FeedbackModalCommand, &mut App)>;

pub(crate) struct FeedbackModalConfig {
    /// The relay's `client.app`: `desktop` or `web`.
    pub(crate) app: &'static str,
    pub(crate) palette: ModalPalette,
    /// The GitHub username the last feedback named, to fill the field with.
    pub(crate) github_username: String,
    /// The agent the feedback agent runs as: the sidebar's default agent for new sessions, else
    /// gxserver picks the Default Prompt Agent.
    pub(crate) agent_id: Option<String>,
    /// The remote machine the reported session runs on; `None` starts the agent on this computer.
    pub(crate) agent_machine: Option<FeedbackAgentMachine>,
    /// This computer's startup diagnostics for the browser issue when gxserver does not answer;
    /// `None` where there are none to read (the web build), which also hides that offer.
    pub(crate) offline_diagnostics: Option<fn() -> String>,
    /// What to remove from that issue before the browser opens.
    pub(crate) scrub_context: FeedbackScrubContext,
    /// Open with "Collect more data with an agent" on (the chat's Copy Report).
    pub(crate) collect_with_agent: bool,
}

/// Client storage's `feedbackGithubUser` store: the GitHub username the last feedback named.
pub(crate) const FEEDBACK_GITHUB_USERNAME_STORAGE_KEY: &str = "ghostex.feedback.githubUsername";

/// The username in the field, without its `@`: `Ok(None)` when the field is empty, `Err` when it
/// is not a GitHub username (letters, digits and single hyphens, no hyphen at either end, up to 39
/// characters).
pub(super) fn github_username(raw: &str) -> Result<Option<&str>, ()> {
    let name = raw.trim();
    let name = name.strip_prefix('@').unwrap_or(name);
    if name.is_empty() {
        return Ok(None);
    }
    let valid = name.len() <= 39
        && name.chars().all(|c| c.is_ascii_alphanumeric() || c == '-')
        && !name.starts_with('-')
        && !name.ends_with('-')
        && !name.contains("--");
    if valid { Ok(Some(name)) } else { Err(()) }
}

pub(super) enum FeedbackStep {
    /// The message and its pasted images.
    Compose,
    /// The issue exactly as it will be posted, editable.
    Review,
    /// The relay created the issue.
    Sent { number: u64, url: String },
}

/// CDXC:Feedback 2026-10-04 DECISION:
/// User: "add a button at the top of the sidebar. It should be a chat bubble icon… When you click that button, it should show a pop-up made with GPUI. This pop-up should have a text area and the ability to paste images. When someone presses send, it's for sending feedback to us. It should … create an issue [on GitHub] with the required information from the user." Decided with them: the issue is posted through the Ghostex feedback relay, pasted images go with it, and the user ALWAYS sees and can edit the final issue text (the Review step) before it is posted.
/// CDXC:Feedback 2026-10-16 DECISION:
/// User: "add a new field above the text field for the feedback that says "github username" and make it required", "add text that says you can paste images in the input text area", "should show the github user name mentioned at the top of the issue", "implement agent for gathering info (agent launches with the agents description and image attached and can debug and then create the issue him self after collecting useful info about the user's system" and "rename the option to Send Bug / Feedback". The pop-up is titled Send Bug / Feedback; a required GitHub username field ("name" or "@name", checked for a GitHub username's shape, remembered for the next feedback) sits above the message box, whose hint says screenshots can be pasted into it; gxserver puts "Reported by @name" at the top of the issue so GitHub notifies them. With "Collect more data with an agent" on, Start Agent hands the message, username and screenshots to gxserver, which starts the feedback agent (server/src/feedback/agent.rs) and the pop-up opens its session; the agent files the issue with `ghostex feedback send`. This supersedes the 2026-10-14 rule that hid the switch and made the username optional.
/// SEE-ALSO: server/src/feedback/ (the draft, the relay post and the feedback agent), server/src/ghostex_cli/feedback.rs (`ghostex feedback send`), packages/gx-core/src/sidebar_menu/navigation.rs (the More options row), packages/gx-core/src/sidebar_actions/open.rs (the `feedback` action), apps/gpui-web/src/app/web_host/modals.rs (the web build opens it too).
pub(crate) struct GpuiFeedbackModalWindow {
    pub(super) host: FeedbackModalHost,
    pub(super) palette: ModalPalette,
    pub(super) app: &'static str,
    pub(super) step: FeedbackStep,
    pub(super) message_input: Entity<TextareaState>,
    /// Mirror of the message field, refreshed on every change.
    pub(super) message: String,
    pub(super) images: Vec<Arc<Image>>,
    pub(super) github_input: Entity<InputState>,
    /// Mirror of the GitHub username field, refreshed on every change.
    pub(super) github: String,
    /// "Collect more data with an agent": the primary button starts the feedback agent instead of
    /// the Review step.
    pub(super) collect_with_agent: bool,
    pub(super) agent_id: Option<String>,
    pub(super) agent_machine: Option<FeedbackAgentMachine>,
    /// Pasted images still being checked (and shrunk when too large).
    pub(super) pending_images: usize,
    pub(super) image_error: Option<String>,
    pub(super) title_input: Entity<InputState>,
    pub(super) title: String,
    pub(super) body_input: Entity<TextareaState>,
    pub(super) body: String,
    /// The line the relay adds under the issue (`Sent from Ghostex feedback · Desktop … · macOS …`).
    pub(super) footer: String,
    /// A draft or a send is in flight.
    pub(super) busy: bool,
    pub(super) error: Option<String>,
    pub(super) offline_diagnostics: Option<fn() -> String>,
    pub(super) scrub_context: FeedbackScrubContext,
    /// The last request got no answer from this computer's gxserver: offer the browser issue.
    pub(super) offline_offer: bool,
    /// Said once the browser issue opened.
    pub(super) offline_notice: Option<String>,
    pub(super) fit: ModalFit,
    pub(super) focus_handle: FocusHandle,
    _subscriptions: Vec<Subscription>,
}

impl GpuiFeedbackModalWindow {
    pub(crate) fn new(
        config: FeedbackModalConfig,
        host: FeedbackModalHost,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Self {
        let message_input = cx.new(|cx| {
            TextareaState::new(window, cx)
                .placeholder("What happened, what did you expect, or what would you like to see?")
        });
        let github = config.github_username;
        let github_input = cx.new(|cx| {
            InputState::new(window, cx)
                .placeholder("name or @name")
                .default_value(github.clone())
        });
        let title_input = cx.new(|cx| InputState::new(window, cx));
        let body_input = cx.new(|cx| TextareaState::new(window, cx));
        let subscriptions = vec![
            cx.subscribe_in(
                &message_input,
                window,
                |this: &mut Self, input, event: &InputEvent, _window, cx| {
                    if matches!(event, InputEvent::Change) {
                        this.message = input.read(cx).value().to_string();
                        cx.notify();
                    }
                },
            ),
            cx.subscribe_in(
                &github_input,
                window,
                |this: &mut Self, input, event: &InputEvent, _window, cx| {
                    if matches!(event, InputEvent::Change) {
                        this.github = input.read(cx).value().to_string();
                        cx.notify();
                    }
                },
            ),
            cx.subscribe_in(
                &title_input,
                window,
                |this: &mut Self, input, event: &InputEvent, _window, cx| {
                    if matches!(event, InputEvent::Change) {
                        this.title = input.read(cx).value().to_string();
                        cx.notify();
                    }
                },
            ),
            cx.subscribe_in(
                &body_input,
                window,
                |this: &mut Self, input, event: &InputEvent, _window, cx| {
                    if matches!(event, InputEvent::Change) {
                        this.body = input.read(cx).value().to_string();
                        cx.notify();
                    }
                },
            ),
        ];
        // The username comes first; once it is remembered, writing the message is what is left.
        if github_username(&github).ok().flatten().is_some() {
            message_input.update(cx, |input, cx| input.focus(window, cx));
        } else {
            github_input.update(cx, |input, cx| input.focus(window, cx));
        }
        Self {
            host,
            palette: config.palette,
            app: config.app,
            step: FeedbackStep::Compose,
            message_input,
            message: String::new(),
            images: Vec::new(),
            github_input,
            github,
            collect_with_agent: config.collect_with_agent,
            agent_id: config.agent_id,
            agent_machine: config.agent_machine,
            pending_images: 0,
            image_error: None,
            title_input,
            title: String::new(),
            body_input,
            body: String::new(),
            footer: String::new(),
            busy: false,
            error: None,
            offline_diagnostics: config.offline_diagnostics,
            scrub_context: config.scrub_context,
            offline_offer: false,
            offline_notice: None,
            fit: ModalFit::fixed(),
            focus_handle: cx.focus_handle(),
            _subscriptions: subscriptions,
        }
    }

    /// The valid username in the field; the field is required.
    pub(super) fn username(&self) -> Option<&str> {
        github_username(&self.github).ok().flatten()
    }

    /// Whether the primary Compose button (Review, or Start Agent) can go.
    pub(super) fn can_review(&self) -> bool {
        !self.busy
            && self.pending_images == 0
            && !self.message.trim().is_empty()
            && self.username().is_some()
    }

    pub(super) fn toggle_collect_with_agent(&mut self, cx: &mut Context<Self>) {
        if !self.busy {
            self.collect_with_agent = !self.collect_with_agent;
            cx.notify();
        }
    }

    /// Stores the username for the next feedback, off the UI thread.
    fn remember_username(&self, cx: &mut Context<Self>) {
        let username = self.username().map(str::to_string);
        cx.background_executor()
            .spawn(async move {
                let _ = write_client_document_value(
                    FEEDBACK_GITHUB_USERNAME_STORAGE_KEY,
                    username.as_deref(),
                );
            })
            .detach();
    }

    /// The pasted images as the `[{ name, dataBase64 }]` list gxserver takes, encoded off the UI
    /// thread.
    fn encoded_images(&self, cx: &mut Context<Self>) -> gpui::Task<Vec<Value>> {
        let images = self.images.clone();
        cx.background_executor().spawn(async move {
            images
                .iter()
                .enumerate()
                .map(|(index, image)| {
                    json!({
                        "name": format!("screenshot-{}.{}", index + 1, image.format().extension()),
                        "dataBase64": base64::engine::general_purpose::STANDARD.encode(image.bytes()),
                    })
                })
                .collect::<Vec<_>>()
        })
    }

    pub(super) fn can_send(&self) -> bool {
        !self.busy && !self.title.trim().is_empty() && !self.body.trim().is_empty()
    }

    pub(super) fn close(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        window.remove_window();
        (self.host)(FeedbackModalCommand::Closed, cx);
    }

    /// Compose → Review: gxserver turns the message into the issue the relay would post, and the
    /// user sees and edits that before anything leaves the computer.
    pub(super) fn review(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if !self.can_review() {
            return;
        }
        self.busy = true;
        self.error = None;
        self.offline_offer = false;
        self.offline_notice = None;
        cx.notify();
        self.remember_username(cx);
        let params = json!({
            "app": self.app,
            "message": self.message.trim(),
            "githubUsername": self.username(),
        });
        cx.spawn_in(window, async move |this, cx| {
            let result = gx_rpc(None, "/api/draftFeedback", params).await;
            let _ = this.update_in(cx, |this, window, cx| {
                this.busy = false;
                match result {
                    Ok(draft) => this.show_review(&draft, window, cx),
                    Err(error) => this.show_failure(error, true),
                }
                cx.notify();
            });
        })
        .detach();
    }

    /// gxserver's draft already starts with the reporter line, so the Review step shows it, the
    /// user can edit it, and the relay posts it unchanged.
    fn show_review(&mut self, draft: &Value, window: &mut Window, cx: &mut Context<Self>) {
        let text = |key: &str| draft[key].as_str().unwrap_or_default().to_string();
        self.title = text("title");
        self.body = text("body");
        self.footer = text("footer");
        let (title, body) = (self.title.clone(), self.body.clone());
        self.title_input
            .update(cx, |input, cx| input.set_value(title, window, cx));
        self.body_input
            .update(cx, |input, cx| input.set_value(body, window, cx));
        self.title_input
            .update(cx, |input, cx| input.focus(window, cx));
        self.step = FeedbackStep::Review;
    }

    /// Review → Compose, keeping the message and the images; edits made to the issue are dropped
    /// because the next Review drafts it again from the message.
    pub(super) fn back(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.busy {
            return;
        }
        self.step = FeedbackStep::Compose;
        self.error = None;
        self.message_input
            .update(cx, |input, cx| input.focus(window, cx));
        cx.notify();
    }

    pub(super) fn send(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if !self.can_send() {
            return;
        }
        self.busy = true;
        self.error = None;
        self.offline_offer = false;
        self.offline_notice = None;
        cx.notify();
        let app = self.app;
        let title = self.title.trim().to_string();
        let body = self.body.trim().to_string();
        let reporter = self.username().map(str::to_string);
        let encode = self.encoded_images(cx);
        cx.spawn_in(window, async move |this, cx| {
            let images = encode.await;
            let params = json!({
                "app": app,
                "title": title,
                "body": body,
                "images": images,
                "reporter": reporter,
            });
            let result = gx_rpc_with_timeout(None, "/api/sendFeedback", params, SEND_TIMEOUT).await;
            let _ = this.update_in(cx, |this, window, cx| {
                this.busy = false;
                match result {
                    Ok(issue) => {
                        match (issue["issueNumber"].as_u64(), issue["issueUrl"].as_str()) {
                            (Some(number), Some(url)) => {
                                this.step = FeedbackStep::Sent {
                                    number,
                                    url: url.to_string(),
                                };
                                // The fields are gone, so the frame takes the keys (Escape, Cmd+Enter).
                                this.focus_handle.focus(window, cx);
                            }
                            _ => {
                                this.error =
                                    Some("The issue was sent, but no link came back.".to_string())
                            }
                        }
                    }
                    Err(error) => this.show_failure(error, true),
                }
                cx.notify();
            });
        })
        .detach();
    }

    /// Compose → the feedback agent: gxserver (the reported session's machine's) saves the
    /// screenshots, starts the agent with the message, and the pop-up closes onto the agent's
    /// session.
    pub(super) fn start_agent(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if !self.can_review() {
            return;
        }
        if let Some(machine) = self.agent_machine.as_ref().filter(|m| m.target.is_none()) {
            self.error = Some(format!(
                "{} is not connected right now. Reconnect it to start the agent there.",
                machine.name
            ));
            cx.notify();
            return;
        }
        self.busy = true;
        self.error = None;
        self.offline_offer = false;
        self.offline_notice = None;
        cx.notify();
        self.remember_username(cx);
        let mut params = json!({
            "app": self.app,
            "message": self.message.trim(),
            "githubUsername": self.username(),
        });
        let local = self.agent_machine.is_none();
        let (remote, remote_machine_id, agent_id) = match &self.agent_machine {
            Some(machine) => (
                machine.target.clone(),
                Some(machine.machine_id.clone()),
                machine.agent_id.clone(),
            ),
            None => (None, None, self.agent_id.clone()),
        };
        if let Some(agent_id) = agent_id {
            params["agentId"] = json!(agent_id);
        }
        let encode = self.encoded_images(cx);
        cx.spawn_in(window, async move |this, cx| {
            params["images"] = Value::Array(encode.await);
            let result =
                gx_rpc_with_timeout(remote, "/api/startFeedbackAgent", params, SEND_TIMEOUT).await;
            let _ = this.update_in(cx, |this, window, cx| {
                this.busy = false;
                let text = |value: &Value, key: &str| value[key].as_str().map(str::to_string);
                match result {
                    Ok(started) => match (text(&started, "projectId"), text(&started, "sessionId"))
                    {
                        (Some(project_id), Some(session_id)) => {
                            let agent_id = text(&started, "agentId").unwrap_or_default();
                            window.remove_window();
                            (this.host)(
                                FeedbackModalCommand::OpenSession {
                                    remote_machine_id,
                                    project_id,
                                    session_id,
                                    agent_id,
                                },
                                cx,
                            );
                            return;
                        }
                        _ => this.error = Some(
                            "The agent started, but Ghostex did not say which session it is in."
                                .to_string(),
                        ),
                    },
                    // gxserver closed the agent that could not take the report: the report goes
                    // on without it (CDXC:Feedback 2026-10-26 in server/src/feedback/brief_wait.rs).
                    Err(error) => {
                        if error.code.as_deref() == Some(FEEDBACK_AGENT_NOT_READY) {
                            this.collect_with_agent = false;
                        }
                        this.show_failure(error, local)
                    }
                }
                cx.notify();
            });
        })
        .detach();
    }

    /// Shows why a request failed, and offers the browser issue when this computer's gxserver gave
    /// no answer (or said it is still starting) and there are diagnostics to send with it.
    fn show_failure(&mut self, error: GxRpcError, local: bool) {
        self.offline_offer = local
            && self.offline_diagnostics.is_some()
            && (error.unanswered || error.code.as_deref() == Some("serverStarting"));
        self.error = Some(error.message);
    }

    /// "Open a GitHub issue in your browser" (CDXC:Feedback 2026-10-22 DECISION in
    /// offline_issue.rs): the message (or the reviewed issue) and this computer's startup
    /// diagnostics, cleaned of personal data, as a prefilled new-issue page.
    pub(super) fn open_offline_issue(&mut self, cx: &mut Context<Self>) {
        if self.busy {
            return;
        }
        let (title, message) = match self.step {
            FeedbackStep::Review => (self.title.clone(), self.body.clone()),
            _ => (String::new(), self.message.clone()),
        };
        if message.trim().is_empty() {
            return;
        }
        self.busy = true;
        cx.notify();
        let diagnostics = self.offline_diagnostics;
        let context = self.scrub_context.clone();
        let build = cx.background_executor().spawn(async move {
            let diagnostics = diagnostics.map(|read| read());
            offline_issue(&title, &message, diagnostics.as_deref(), &context)
        });
        cx.spawn(async move |this, cx| {
            let issue = build.await;
            let _ = this.update(cx, |this, cx| {
                this.busy = false;
                let notice = match issue.clipboard {
                    Some(body) => {
                        cx.write_to_clipboard(ClipboardItem::new_string(body));
                        "GitHub opened in your browser. The report was too long for a link, so it is on your clipboard: paste it into the issue, check it, then press Create."
                    }
                    None => "GitHub opened in your browser with your report filled in, with personal data removed. Check it, then press Create there.",
                };
                crate::app::helpers::gpui_open_external_url(&issue.url, cx);
                this.offline_notice = Some(notice.to_string());
                cx.notify();
            });
        })
        .detach();
    }

    pub(super) fn open_issue(&mut self, cx: &mut Context<Self>) {
        if let FeedbackStep::Sent { url, .. } = &self.step {
            crate::app::helpers::gpui_open_external_url(url, cx);
        }
    }

    /// Cmd/Ctrl+Enter moves on: Review (or Start Agent) from the message, Send from the review,
    /// Close when sent.
    fn advance(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        match self.step {
            FeedbackStep::Compose if self.collect_with_agent => self.start_agent(window, cx),
            FeedbackStep::Compose => self.review(window, cx),
            FeedbackStep::Review => self.send(window, cx),
            FeedbackStep::Sent { .. } => self.close(window, cx),
        }
    }

    /// Plain Enter types a newline; the platform's Cmd/Ctrl+Enter chord advances. A capture-phase
    /// action listener stops propagation itself, or the field's own handler runs too.
    pub(super) fn on_enter_action(
        &mut self,
        action: &Enter,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if !action.secondary {
            cx.propagate();
            return;
        }
        cx.stop_propagation();
        self.advance(window, cx);
    }

    pub(super) fn on_escape_action(
        &mut self,
        _: &Escape,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        cx.stop_propagation();
        self.close(window, cx);
    }

    /// Keys while the frame itself holds focus.
    pub(super) fn on_key_down(
        &mut self,
        event: &KeyDownEvent,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        match event.keystroke.key.as_str() {
            "escape" => self.close(window, cx),
            "enter" if event.keystroke.modifiers.secondary(cx) && !event.is_held => {
                self.advance(window, cx)
            }
            _ => return,
        }
        cx.stop_propagation();
    }
}

impl ModalCornerClose for GpuiFeedbackModalWindow {
    fn close_from_corner(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.close(window, cx);
    }
}
