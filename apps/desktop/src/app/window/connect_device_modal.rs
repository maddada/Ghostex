//! The "Connect a phone" window (sidebar menu > Connect a phone, Settings > Remote's Connect a phone
//! and Connect a computer): it turns Easy Connect on by itself, shows the code, turns into the
//! three words and Approve the moment a phone scans, and ends on "<phone> is connected".
//!
//! CDXC:RemotePairing 2026-11-10 DECISION:
//! User, on the pairing design page (Q2178 to Q2186, the suggested screen for steps 1, 2, 3, 7 and 8): one "Connect a phone" window that opens on the code replaces the Remote Setup dialog, the stop at Settings > Remote and the separate Show QR popup. The menu item opens it straight away and it turns Easy Connect on by itself (installing the helper and turning on SSH access first when they are missing, as Remote Setup's Connect did). Two plain steps that match the phone's button word for word, a line that shows it is waiting, the code at 432px (CDXC:RemotePairing 2026-10-27 in settings_modal/fields/qr.rs), "Need the app?" as one line and Copy the code. His note on step 1: "we need bit more there tho, tell the user we can use tailscale and we can connect a laptop to another laptop", so one quiet line under it says both, each leading to its place (Tailscale opens Settings > Remote's Tailscale, "another computer" turns this window into Connect a computer with its own two lines). When a phone scans, the same window becomes "Is this your phone?" with the three words as its biggest thing and Not my phone beside Approve; after Approve it says "<phone> is connected" with Connect another phone and Done. Approve follows the approvals rule (one plain click; Settings > Security's switch adds Touch ID or the password first, `owner_security::plain`).
//! SEE-ALSO: apps/desktop/src/app/connect_device_modal_lifecycle.rs (the gxserver calls, the approval, the approval card kept away while this window shows the request), apps/desktop/src/app/owner_security/ui.rs (the "Is this your phone?" notice for a request that arrives while this window is closed), apps/desktop/src/app/window/settings_modal/tabs/remote/paired_devices.rs (the same request's row in Settings > Remote), apps/desktop/src/bin/native_modal_demo/connect_device.rs (preview).
use super::native_modal_kit::*;
use super::settings_modal::fields::{QrGrid, qr_code};
use gpui::prelude::FluentBuilder as _;
use gpui::{
    AnyElement, App, Context, Div, FocusHandle, FontWeight, InteractiveElement as _, IntoElement,
    KeyDownEvent, ParentElement as _, Render, SharedString, StatefulInteractiveElement as _,
    Styled as _, Task, Window, div, px,
};
use gpui_component::{h_flex, v_flex};
use std::collections::HashSet;
use std::rc::Rc;
use std::time::Duration;

pub(crate) const CONNECT_DEVICE_MODAL_WIDTH: f32 = 520.0;
/// First-frame height only. The window is resized to the measured layout as soon as the first
/// prepaint reports it.
pub(crate) const CONNECT_DEVICE_MODAL_INITIAL_HEIGHT: f32 = 780.0;

/// The code's side: 432px, so an Android scanner gets large modules (CDXC:RemotePairing
/// 2026-10-27 in settings_modal/fields/qr.rs). The "Getting a code ready" square has the same size,
/// so the window doesn't jump when the code arrives.
const QR_SIZE: f32 = 432.0;
/// How often the window asks for the code and for a waiting phone: every second while it is open
/// (the Show QR popup asked every four, so the phone showed its words first and the computer
/// looked dead).
const POLL_EVERY: Duration = Duration::from_secs(1);
const COPIED_RESET: Duration = Duration::from_millis(1_600);

const ICON_COPY: &str = "modals/settings/copy.svg";
const ICON_CHECK: &str = "modals/settings/check.svg";
const ICON_PHONE: &str = "modals/settings/device-mobile.svg";
const ICON_COMPUTER: &str = "modals/settings/device-desktop.svg";
const ICON_CONNECTED: &str = "modals/settings/circle-check-filled.svg";

/// The short website link that redirects to the latest Android APK.
pub(crate) const GHOSTEX_ANDROID_INSTALL_URL: &str = "https://ghostex.dev/android";
/// Ghostex for iPhone on the App Store.
pub(crate) const GHOSTEX_IPHONE_INSTALL_URL: &str = "https://apps.apple.com/app/id6771707394";

/// What the window connects: a phone (the QR) or another computer (the code to paste there).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum ConnectDeviceKind {
    Phone,
    Computer,
}

impl ConnectDeviceKind {
    /// The open message's `device` value (`phone` when absent).
    pub(crate) fn from_open_message(message: &serde_json::Value) -> Self {
        match message.get("device").and_then(serde_json::Value::as_str) {
            Some("computer") => Self::Computer,
            _ => Self::Phone,
        }
    }
}

/// This computer's Easy Connect code, while it can pair (it carries the one-time secret).
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct ConnectCode {
    /// What the QR encodes (the link).
    pub(crate) payload: String,
    /// The bare code Copy the code puts on the clipboard.
    pub(crate) text: String,
}

/// A phone or computer that used the code and waits for Approve, with its three words.
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct WaitingDevice {
    pub(crate) name: String,
    /// "iPhone", "Android" or "computer": what the device says it is, in the user's words.
    pub(crate) kind_label: String,
    pub(crate) phone: bool,
    pub(crate) words: Vec<String>,
    pub(crate) approval_id: String,
}

/// One answer to [`ConnectDeviceCommand::Poll`].
#[derive(Clone, Debug, Default)]
pub(crate) struct ConnectPoll {
    pub(crate) code: Option<ConnectCode>,
    /// This computer's name, as the phone will show it.
    pub(crate) computer_name: Option<String>,
    pub(crate) waiting: Vec<WaitingDevice>,
    /// The ids of the devices paired now, to tell when an approved one has finished pairing.
    pub(crate) paired_ids: Vec<String>,
    pub(crate) error: Option<String>,
}

/// What the window asks its host to do. The window removes its own window before `Close` and
/// `OpenTailscale`.
pub(crate) enum ConnectDeviceCommand {
    /// Install the helper if needed, turn SSH access and Easy Connect on; the host reports each
    /// step with `prepare_progress` and answers with `prepared`.
    Prepare,
    /// Read the code and the waiting devices (opening the door for a new phone while this window
    /// shows); the host answers with `poll_answered`.
    Poll,
    /// Approve or deny a waiting device's request; the host answers with `decided`.
    Decide {
        approval_id: String,
        approve: bool,
    },
    /// The code was copied: play the copy sound.
    Copied,
    OpenExternalUrl(String),
    OpenTailscale,
    Close,
}

pub(crate) type ConnectDeviceHost = Rc<dyn Fn(ConnectDeviceCommand, &mut App)>;

pub(crate) struct ConnectDeviceConfig {
    pub(crate) palette: ModalPalette,
    pub(crate) kind: ConnectDeviceKind,
    /// This computer's name, until gxserver says it.
    pub(crate) computer_name: String,
    /// `Settings.remoteTailscaleEnabled`: off leaves Tailscale out of the quiet line.
    pub(crate) tailscale_enabled: bool,
    /// Approve asks for Touch ID or the password first (Settings > Security, or an older gxserver).
    pub(crate) require_presence: bool,
}

#[derive(Clone, Debug, PartialEq)]
enum Stage {
    /// Turning things on; the line says which step.
    Preparing(Option<String>),
    /// Turning on failed; the message and Try again.
    Failed(String),
    /// The code (or "Getting a code ready…" until it arrives), and a waiting device when one asks.
    Ready,
    /// Approved: waiting for the device to finish pairing on its next ask.
    Finishing {
        device: WaitingDevice,
        paired_before: HashSet<String>,
    },
    Connected(WaitingDevice),
}

pub(crate) struct GpuiConnectDeviceWindow {
    host: ConnectDeviceHost,
    palette: ModalPalette,
    kind: ConnectDeviceKind,
    computer_name: String,
    tailscale_enabled: bool,
    require_presence: bool,
    stage: Stage,
    code: Option<ConnectCode>,
    qr: Option<(String, Option<QrGrid>)>,
    waiting: Option<WaitingDevice>,
    /// Requests answered here, kept out of a poll answer that was already on its way.
    answered: HashSet<String>,
    paired_ids: HashSet<String>,
    deciding: bool,
    decide_error: Option<String>,
    poll_error: Option<String>,
    poll_in_flight: bool,
    polling: Option<Task<()>>,
    copied: bool,
    copied_generation: u64,
    fit: ModalScrollFit,
    /// Which screen the window was last sized for: a new one (the words, connected, the other
    /// kind) is measured again, so the window fits each screen instead of the first.
    fitted_screen: Option<(u8, ConnectDeviceKind)>,
    focus_handle: FocusHandle,
}

impl GpuiConnectDeviceWindow {
    pub(crate) fn new(
        config: ConnectDeviceConfig,
        host: ConnectDeviceHost,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Self {
        let focus_handle = cx.focus_handle();
        focus_handle.focus(window, cx);
        let this = Self {
            host,
            palette: config.palette,
            kind: config.kind,
            computer_name: config.computer_name,
            tailscale_enabled: config.tailscale_enabled,
            require_presence: config.require_presence,
            stage: Stage::Preparing(None),
            code: None,
            qr: None,
            waiting: None,
            answered: HashSet::new(),
            paired_ids: HashSet::new(),
            deciding: false,
            decide_error: None,
            poll_error: None,
            poll_in_flight: false,
            polling: None,
            copied: false,
            copied_generation: 0,
            fit: ModalScrollFit::new(),
            fitted_screen: None,
            focus_handle,
        };
        (this.host)(ConnectDeviceCommand::Prepare, cx);
        this
    }

    // ---- the host's answers ----------------------------------------------------------------------

    /// A step of turning things on started ("Installing Easy Connect…").
    pub(crate) fn prepare_progress(&mut self, note: String, cx: &mut Context<Self>) {
        if matches!(self.stage, Stage::Preparing(_)) {
            self.stage = Stage::Preparing(Some(note));
            cx.notify();
        }
    }

    /// Easy Connect is on (or couldn't be turned on): start asking for the code.
    pub(crate) fn prepared(&mut self, result: Result<(), String>, cx: &mut Context<Self>) {
        match result {
            Ok(()) => {
                self.stage = Stage::Ready;
                self.start_polling(cx);
            }
            Err(message) => self.stage = Stage::Failed(message),
        }
        cx.notify();
    }

    pub(crate) fn poll_answered(&mut self, poll: ConnectPoll, cx: &mut Context<Self>) {
        self.poll_in_flight = false;
        self.poll_error = poll.error;
        if let Some(name) = poll.computer_name.filter(|name| !name.trim().is_empty()) {
            self.computer_name = name;
        }
        if poll.code.is_some() || self.code.is_none() {
            self.code = poll.code;
        }
        self.paired_ids = poll.paired_ids.into_iter().collect();
        let answered = &self.answered;
        self.waiting = poll
            .waiting
            .into_iter()
            .find(|device| !answered.contains(&device.approval_id));
        if let Stage::Finishing {
            device,
            paired_before,
        } = &self.stage
            && self.paired_ids.iter().any(|id| !paired_before.contains(id))
        {
            self.stage = Stage::Connected(device.clone());
        }
        cx.notify();
    }

    /// The answer to Approve or Not my phone.
    pub(crate) fn decided(
        &mut self,
        approval_id: String,
        approved: bool,
        result: Result<(), String>,
        cx: &mut Context<Self>,
    ) {
        self.deciding = false;
        match result {
            Ok(()) => {
                self.answered.insert(approval_id.clone());
                let device = self
                    .waiting
                    .take()
                    .filter(|device| device.approval_id == approval_id);
                if approved && let Some(device) = device {
                    self.stage = Stage::Finishing {
                        device,
                        paired_before: self.paired_ids.clone(),
                    };
                }
                self.decide_error = None;
            }
            Err(message) => self.decide_error = Some(message),
        }
        cx.notify();
    }

    // ---- actions ---------------------------------------------------------------------------------

    fn start_polling(&mut self, cx: &mut Context<Self>) {
        if self.polling.is_some() {
            return;
        }
        self.poll(cx);
        self.polling = Some(cx.spawn(async move |this, cx| {
            loop {
                cx.background_executor().timer(POLL_EVERY).await;
                if this.update(cx, |this, cx| this.poll(cx)).is_err() {
                    break;
                }
            }
        }));
    }

    fn poll(&mut self, cx: &mut Context<Self>) {
        if self.poll_in_flight || matches!(self.stage, Stage::Connected(_)) {
            return;
        }
        self.poll_in_flight = true;
        (self.host)(ConnectDeviceCommand::Poll, cx);
    }

    fn retry(&mut self, cx: &mut Context<Self>) {
        self.stage = Stage::Preparing(None);
        cx.notify();
        (self.host)(ConnectDeviceCommand::Prepare, cx);
    }

    fn decide(&mut self, approve: bool, cx: &mut Context<Self>) {
        let Some(device) = self.waiting.as_ref() else {
            return;
        };
        if self.deciding {
            return;
        }
        self.deciding = true;
        self.decide_error = None;
        cx.notify();
        (self.host)(
            ConnectDeviceCommand::Decide {
                approval_id: device.approval_id.clone(),
                approve,
            },
            cx,
        );
    }

    fn connect_another(&mut self, cx: &mut Context<Self>) {
        self.stage = Stage::Ready;
        cx.notify();
    }

    fn switch_kind(&mut self, kind: ConnectDeviceKind, cx: &mut Context<Self>) {
        self.kind = kind;
        cx.notify();
    }

    fn copy_code(&mut self, cx: &mut Context<Self>) {
        let Some(code) = self.code.as_ref() else {
            return;
        };
        cx.write_to_clipboard(gpui::ClipboardItem::new_string(code.text.clone()));
        (self.host)(ConnectDeviceCommand::Copied, cx);
        self.copied = true;
        self.copied_generation = self.copied_generation.wrapping_add(1);
        let generation = self.copied_generation;
        cx.notify();
        cx.spawn(async move |this, cx| {
            cx.background_executor().timer(COPIED_RESET).await;
            let _ = this.update(cx, |this, cx| {
                if this.copied_generation == generation {
                    this.copied = false;
                    cx.notify();
                }
            });
        })
        .detach();
    }

    fn open_url(&mut self, url: &'static str, cx: &mut Context<Self>) {
        (self.host)(ConnectDeviceCommand::OpenExternalUrl(url.to_string()), cx);
    }

    fn close_and_send(
        &mut self,
        command: ConnectDeviceCommand,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.polling = None;
        window.remove_window();
        (self.host)(command, cx);
    }

    fn close(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.close_and_send(ConnectDeviceCommand::Close, window, cx);
    }

    fn on_key_down(&mut self, event: &KeyDownEvent, window: &mut Window, cx: &mut Context<Self>) {
        if event.keystroke.key.as_str() == "escape" {
            self.close(window, cx);
            cx.stop_propagation();
        }
    }

    // ---- drawing ---------------------------------------------------------------------------------

    fn title(&self) -> String {
        match (&self.stage, &self.waiting) {
            (Stage::Connected(device), _) => format!("{} is connected", device.name),
            (Stage::Ready, Some(device)) => pairing_question(device.phone).to_string(),
            _ => match self.kind {
                ConnectDeviceKind::Phone => "Connect a phone".to_string(),
                ConnectDeviceKind::Computer => "Connect a computer".to_string(),
            },
        }
    }

    fn muted(&self, text: impl Into<SharedString>) -> Div {
        div()
            .text_size(px(13.0))
            .line_height(px(19.0))
            .text_color(hsla(self.palette.muted))
            .child(text.into())
    }

    /// A word that does something inside a line of text: the foreground colour, underlined on hover.
    fn link(
        &self,
        id: &'static str,
        label: &'static str,
        on_press: impl Fn(&mut Self, &mut Window, &mut Context<Self>) + 'static,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        div()
            .id(id)
            .role(gpui::Role::Link)
            .aria_label(label)
            .text_color(hsla(self.palette.foreground))
            .cursor_pointer()
            .hover(|style| style.underline())
            .on_press(cx, on_press)
            .child(label)
            .into_any_element()
    }

    fn step(&self, number: usize, parts: Vec<(&'static str, bool)>) -> AnyElement {
        let p = self.palette;
        h_flex()
            .w_full()
            .items_start()
            .gap(px(10.0))
            .child(
                div()
                    .mt(px(0.5))
                    .flex_shrink_0()
                    .flex()
                    .items_center()
                    .justify_center()
                    .size(px(18.0))
                    .rounded_full()
                    .border_1()
                    .border_color(hsla(p.hairline))
                    .bg(hsla(p.raised))
                    .text_size(px(11.0))
                    .line_height(px(15.0))
                    .font_weight(FontWeight::MEDIUM)
                    .text_color(hsla(p.foreground))
                    .child(SharedString::from(number.to_string())),
            )
            .child(
                h_flex()
                    .flex_1()
                    .min_w_0()
                    .flex_wrap()
                    .text_size(px(13.0))
                    .line_height(px(19.0))
                    .children(parts.into_iter().map(|(text, strong)| {
                        div()
                            .text_color(hsla(if strong { p.foreground } else { p.muted }))
                            .child(text)
                    })),
            )
            .into_any_element()
    }

    fn spinner_line(&self, text: impl Into<SharedString>) -> AnyElement {
        h_flex()
            .justify_center()
            .gap(px(8.0))
            .child(modal_spinner(self.palette.muted))
            .child(self.muted(text))
            .into_any_element()
    }

    /// What went wrong turning things on, and Try again.
    fn failed_content(&self, message: String, cx: &mut Context<Self>) -> AnyElement {
        let p = self.palette;
        v_flex()
            .items_center()
            .gap(px(12.0))
            .px(px(28.0))
            .text_center()
            .child(
                div()
                    .text_size(px(13.0))
                    .line_height(px(19.0))
                    .text_color(hsla(p.destructive))
                    .child(message),
            )
            .child(div().w(px(140.0)).child(modal_action_button(
                &p,
                "connect-device-retry",
                "Try again",
                None,
                ModalButtonTone::Neutral,
                false,
                |this: &mut Self, _window, cx| this.retry(cx),
                cx,
            )))
            .into_any_element()
    }

    /// The dashed square the code appears in (`height` tall): what is happening, or what went
    /// wrong.
    fn code_placeholder(&self, height: f32, cx: &mut Context<Self>) -> AnyElement {
        let p = self.palette;
        let content = match &self.stage {
            Stage::Failed(message) => self.failed_content(message.clone(), cx),
            Stage::Preparing(Some(note)) => self.spinner_line(note.clone()),
            _ => self.spinner_line("Getting a code ready…"),
        };
        div()
            .w(px(QR_SIZE))
            .max_w_full()
            .h(px(height))
            .flex_shrink_0()
            .flex()
            .items_center()
            .justify_center()
            .rounded(px(MODAL_RADIUS_CONTROL))
            .border_1()
            .border_dashed()
            .border_color(hsla(p.hairline))
            .bg(hsla(p.panel))
            .child(content)
            .into_any_element()
    }

    fn qr_grid(&mut self) -> Option<QrGrid> {
        let payload = self.code.as_ref()?.payload.clone();
        if self.qr.as_ref().map(|(cached, _)| cached.as_str()) != Some(payload.as_str()) {
            self.qr = Some((payload.clone(), QrGrid::encode(&payload)));
        }
        self.qr.as_ref().and_then(|(_, grid)| grid.clone())
    }

    /// The phone's code: the QR (or its square), the computer's name, and the waiting line.
    fn phone_code(&mut self, cx: &mut Context<Self>) -> AnyElement {
        let ready = self.stage == Stage::Ready && self.code.is_some();
        let code = if ready {
            let grid = self.qr_grid();
            qr_code(grid.as_ref(), QR_SIZE)
        } else {
            self.code_placeholder(QR_SIZE, cx)
        };
        v_flex()
            .w_full()
            .items_center()
            .gap(px(8.0))
            .child(code)
            .child(
                div()
                    .pt(px(4.0))
                    .text_size(px(14.0))
                    .line_height(px(20.0))
                    .text_color(hsla(self.palette.foreground))
                    .child(self.computer_name.clone()),
            )
            // The line's row is there before the code too, so the window measured on its first
            // frame (while Easy Connect turns on) fits the code screen.
            .child(div().h(px(20.0)).when(ready, |this| {
                this.child(self.spinner_line("Waiting for your phone…"))
            }))
            .into_any_element()
    }

    /// The code another computer pastes: this computer, the code on one line, and the waiting line.
    fn computer_code(&mut self, cx: &mut Context<Self>) -> AnyElement {
        let p = self.palette;
        let ready = self.stage == Stage::Ready && self.code.is_some();
        if !ready {
            return self.code_placeholder(140.0, cx);
        }
        let text = self
            .code
            .as_ref()
            .map(|code| code.text.clone())
            .unwrap_or_default();
        v_flex()
            .w_full()
            .gap(px(10.0))
            .child(
                v_flex()
                    .w_full()
                    .gap(px(8.0))
                    .p(px(14.0))
                    .rounded(px(MODAL_RADIUS_CONTROL))
                    .border_1()
                    .border_color(hsla(p.hairline))
                    .bg(hsla(p.panel))
                    .child(
                        h_flex()
                            .gap(px(8.0))
                            .items_center()
                            .child(modal_icon(ICON_COMPUTER, 16.0, p.foreground))
                            .child(self.computer_name.clone()),
                    )
                    .child(
                        div()
                            .w_full()
                            .min_w_0()
                            .overflow_hidden()
                            .whitespace_nowrap()
                            .text_ellipsis()
                            .font_family(MODAL_MONO_FONT)
                            .text_size(px(12.0))
                            .line_height(px(17.0))
                            .text_color(hsla(p.muted))
                            .child(text),
                    ),
            )
            .child(self.spinner_line("Waiting for the other computer…"))
            .into_any_element()
    }

    fn steps(&self) -> AnyElement {
        let steps = match self.kind {
            ConnectDeviceKind::Phone => vec![
                self.step(1, vec![("Open Ghostex on your phone.", false)]),
                self.step(
                    2,
                    vec![
                        ("Tap ", false),
                        ("Scan the code", true),
                        (" and point it here.", false),
                    ],
                ),
            ],
            ConnectDeviceKind::Computer => vec![
                self.step(1, vec![("Open Ghostex on the other computer.", false)]),
                self.step(
                    2,
                    vec![
                        ("In Settings > Remote, click ", false),
                        ("Add a computer", true),
                        (" and paste this code.", false),
                    ],
                ),
            ],
        };
        v_flex()
            .w_full()
            .gap(px(8.0))
            .children(steps)
            .into_any_element()
    }

    /// Copy the code: the neutral button, as wide as its words, with Copied for a moment.
    fn copy_button(&self, cx: &mut Context<Self>) -> Option<AnyElement> {
        let ready = self.stage == Stage::Ready && self.code.is_some();
        if !ready {
            return None;
        }
        let p = self.palette;
        let tone = match self.kind {
            ConnectDeviceKind::Phone => ModalButtonTone::Neutral,
            ConnectDeviceKind::Computer => ModalButtonTone::Primary,
        };
        let icon_color = if tone == ModalButtonTone::Primary {
            p.primary_button_text()
        } else {
            p.foreground
        };
        Some(
            div()
                .w(px(150.0))
                .flex_shrink_0()
                .child(modal_action_button(
                    &p,
                    "connect-device-copy",
                    if self.copied {
                        "Copied"
                    } else {
                        "Copy the code"
                    },
                    Some(
                        modal_icon(
                            if self.copied { ICON_CHECK } else { ICON_COPY },
                            14.0,
                            icon_color,
                        )
                        .into_any_element(),
                    ),
                    tone,
                    false,
                    |this: &mut Self, _window, cx| this.copy_code(cx),
                    cx,
                ))
                .into_any_element(),
        )
    }

    /// "Need the app? Android · iPhone" beside Copy the code, over a hairline; then the quiet line
    /// that says Tailscale works too and that another computer (or a phone) can connect.
    fn footer(&self, cx: &mut Context<Self>) -> AnyElement {
        let p = self.palette;
        let left: AnyElement = match self.kind {
            ConnectDeviceKind::Phone => h_flex()
                .flex_wrap()
                .text_size(px(13.0))
                .line_height(px(19.0))
                .text_color(hsla(p.muted))
                .child("Need the app?\u{a0}")
                .child(self.link(
                    "connect-device-android",
                    "Android",
                    |this, _window, cx| this.open_url(GHOSTEX_ANDROID_INSTALL_URL, cx),
                    cx,
                ))
                .child("\u{a0}·\u{a0}")
                .child(self.link(
                    "connect-device-iphone",
                    "iPhone",
                    |this, _window, cx| this.open_url(GHOSTEX_IPHONE_INSTALL_URL, cx),
                    cx,
                ))
                .into_any_element(),
            ConnectDeviceKind::Computer => self
                .muted("The code works once, for five minutes.")
                .into_any_element(),
        };
        let (other_label, other_kind) = match self.kind {
            ConnectDeviceKind::Phone => ("another computer", ConnectDeviceKind::Computer),
            ConnectDeviceKind::Computer => ("a phone", ConnectDeviceKind::Phone),
        };
        let mut also = h_flex()
            .w_full()
            .flex_wrap()
            .text_size(px(12.0))
            .line_height(px(17.0))
            .text_color(hsla(p.muted))
            .child("You can also\u{a0}");
        if self.tailscale_enabled {
            also = also
                .child("use\u{a0}")
                .child(self.link(
                    "connect-device-tailscale",
                    "Tailscale",
                    |this, window, cx| {
                        this.close_and_send(ConnectDeviceCommand::OpenTailscale, window, cx)
                    },
                    cx,
                ))
                .child(",\u{a0}or\u{a0}");
        }
        also = also
            .child("connect\u{a0}")
            .child(self.link(
                "connect-device-other-kind",
                other_label,
                move |this, _window, cx| this.switch_kind(other_kind, cx),
                cx,
            ))
            .child(".");
        v_flex()
            .w_full()
            .gap(px(12.0))
            .pt(px(14.0))
            .border_t_1()
            .border_color(hsla(p.hairline))
            .child(
                h_flex()
                    .w_full()
                    .min_h(px(MODAL_CONTROL_HEIGHT))
                    .items_center()
                    .justify_between()
                    .gap(px(12.0))
                    .child(div().flex_1().min_w_0().child(left))
                    .children(self.copy_button(cx)),
            )
            .child(also)
            .into_any_element()
    }

    /// "Is this your phone?": the device, its three words as the biggest thing, and the two answers.
    fn ask_body(&self, device: &WaitingDevice, cx: &mut Context<Self>) -> AnyElement {
        let p = self.palette;
        let deciding = self.deciding;
        v_flex()
            .w_full()
            .gap(px(16.0))
            .child(
                h_flex()
                    .gap(px(8.0))
                    .items_center()
                    .child(modal_icon(
                        if device.phone {
                            ICON_PHONE
                        } else {
                            ICON_COMPUTER
                        },
                        16.0,
                        p.foreground,
                    ))
                    .child(device.name.clone())
                    .child(self.muted(format!("· {}", device.kind_label))),
            )
            .child(pairing_words(&p, &device.words, false))
            .child(self.muted(pairing_words_sentence(device.phone)))
            .children(
                self.decide_error
                    .clone()
                    .map(|error| modal_error(&p, error)),
            )
            .child(modal_footer(vec![
                modal_action_button(
                    &p,
                    "connect-device-deny",
                    pairing_deny_label(device.phone),
                    None,
                    ModalButtonTone::Neutral,
                    deciding,
                    |this: &mut Self, _window, cx| this.decide(false, cx),
                    cx,
                ),
                modal_action_button(
                    &p,
                    "connect-device-approve",
                    "Approve",
                    deciding.then(|| modal_spinner(p.primary_button_text())),
                    ModalButtonTone::Primary,
                    deciding,
                    |this: &mut Self, _window, cx| this.decide(true, cx),
                    cx,
                ),
            ]))
            .when(self.require_presence, |this| {
                this.child(
                    self.muted(approve_presence_hint())
                        .text_size(px(12.0))
                        .text_center(),
                )
            })
            .into_any_element()
    }

    fn finishing_body(&self, device: &WaitingDevice) -> AnyElement {
        v_flex()
            .w_full()
            .gap(px(16.0))
            .child(pairing_words(&self.palette, &device.words, false))
            .child(self.spinner_line(format!(
                "Approved. Waiting for {} to finish connecting…",
                device.name
            )))
            .into_any_element()
    }

    fn connected_body(&self, cx: &mut Context<Self>) -> AnyElement {
        let p = self.palette;
        v_flex()
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
                            .child(modal_icon(ICON_CONNECTED, 18.0, p.success)),
                    )
                    .child(self.muted(format!(
                        "It can open your chats and terminals on {} from anywhere. Remove it any time in Settings > Remote.",
                        self.computer_name
                    )).flex_1().min_w_0()),
            )
            .child(modal_footer(vec![
                modal_action_button(
                    &p,
                    "connect-device-another",
                    match self.kind {
                        ConnectDeviceKind::Phone => "Connect another phone",
                        ConnectDeviceKind::Computer => "Connect another computer",
                    },
                    None,
                    ModalButtonTone::Neutral,
                    false,
                    |this: &mut Self, _window, cx| this.connect_another(cx),
                    cx,
                ),
                modal_action_button(
                    &p,
                    "connect-device-done",
                    "Done",
                    None,
                    ModalButtonTone::Primary,
                    false,
                    |this: &mut Self, window, cx| this.close(window, cx),
                    cx,
                ),
            ]))
            .into_any_element()
    }

    fn code_body(&mut self, cx: &mut Context<Self>) -> AnyElement {
        let code = match self.kind {
            ConnectDeviceKind::Phone => self.phone_code(cx),
            ConnectDeviceKind::Computer => self.computer_code(cx),
        };
        v_flex()
            .w_full()
            .gap(px(16.0))
            .child(code)
            .child(self.steps())
            .children(
                self.poll_error
                    .clone()
                    .filter(|_| self.stage == Stage::Ready)
                    .map(|error| modal_error(&self.palette, error)),
            )
            .child(self.footer(cx))
            .into_any_element()
    }
}

impl Render for GpuiConnectDeviceWindow {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let p = self.palette;
        let screen = match (&self.stage, &self.waiting) {
            (Stage::Connected(_), _) => 3,
            (Stage::Finishing { .. }, _) => 2,
            (Stage::Ready, Some(_)) => 1,
            // Connect a computer's code box is shorter than its "Getting a code ready" square.
            (Stage::Ready, None)
                if self.kind == ConnectDeviceKind::Computer && self.code.is_some() =>
            {
                4
            }
            _ => 0,
        };
        if self
            .fitted_screen
            .is_some_and(|fitted| fitted != (screen, self.kind))
        {
            self.fit = ModalScrollFit::new();
        }
        self.fitted_screen = Some((screen, self.kind));
        let body = match (self.stage.clone(), self.waiting.clone()) {
            (Stage::Connected(_), _) => self.connected_body(cx),
            (Stage::Finishing { device, .. }, _) => self.finishing_body(&device),
            (Stage::Ready, Some(device)) => self.ask_body(&device, cx),
            _ => self.code_body(cx),
        };
        let header = div()
            .text_size(px(16.0))
            .line_height(px(20.8))
            .text_color(hsla(p.foreground))
            .child(self.title())
            .into_any_element();
        modal_shell_scrolling(
            &p,
            "ghostex-gpui-connect-device-modal",
            "ghostex-gpui-connect-device-modal-body",
            &self.focus_handle,
            &self.fit,
            Self::on_key_down,
            header,
            body,
            None,
            None,
            cx,
        )
    }
}

impl ModalCornerClose for GpuiConnectDeviceWindow {
    fn close_from_corner(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.close(window, cx);
    }
}
