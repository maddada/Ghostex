//! The Connect a device window's Tailscale way: what it is for, the checklist Settings > Remote's
//! Tailscale row shows (the same four lines, from the same function), and a button to that row,
//! where the code to scan and the switch are.
//!
//! CDXC:RemotePairing 2026-11-11 DECISION:
//! User picked the list of three ways (the decision record in window/connect_device_modal.rs), which makes Tailscale a way with a screen of its own; before, its word closed the window and opened Settings. Via the coordinator: the screen is kept simple and honest, it says what Tailscale is for and what the reader needs to see to use it, it reuses what Settings > Remote's Tailscale row shows instead of a second copy of its logic, its row always shows, and while Tailscale is off the screen says so and its button leads to where it is turned on. So the checklist's titles, details and ticks are `tailscale_checklist` (settings_modal/tabs/remote/tailscale.rs), read from `/api/remoteAccessStatus` when the way is picked and every few seconds while it shows; the code stays in Settings, behind its own Show QR.
use super::super::settings_modal::tabs::remote::{TAILSCALE_OFF_NOTE, TailscaleCheck};
use super::*;

/// How often the way reads Tailscale again while it shows.
const READ_EVERY: Duration = Duration::from_secs(4);
/// The way's button, as wide as its longest words.
const BUTTON_WIDTH: f32 = 220.0;

/// What the Tailscale way knows.
#[derive(Clone, Debug, PartialEq)]
pub(crate) enum TailscalePane {
    /// Tailscale is on in Settings and its status has not been read yet.
    Unread,
    /// Tailscale is switched off in Settings.
    Off,
    /// The checklist as it stands.
    Checklist([TailscaleCheck; 4]),
    /// The status could not be read.
    Failed(String),
}

impl TailscalePane {
    pub(super) fn new(enabled: bool) -> Self {
        if enabled { Self::Unread } else { Self::Off }
    }

    /// Tailscale is switched on in Settings (as far as the window knows).
    pub(super) fn is_on(&self) -> bool {
        !matches!(self, Self::Off)
    }

    /// Which of the way's screens shows, for sizing the window.
    pub(super) fn screen(&self) -> u8 {
        match self {
            Self::Unread => 0,
            Self::Off => 1,
            Self::Checklist(_) => 2,
            Self::Failed(_) => 3,
        }
    }
}

impl GpuiConnectDeviceWindow {
    /// The answer to `ReadTailscale`: whether Tailscale is switched on in Settings and, when it
    /// is, its checklist.
    pub(crate) fn tailscale_answered(
        &mut self,
        enabled: bool,
        checklist: Result<[TailscaleCheck; 4], String>,
        cx: &mut Context<Self>,
    ) {
        let pane = match (enabled, checklist) {
            (false, _) => TailscalePane::Off,
            (true, Ok(checklist)) => TailscalePane::Checklist(checklist),
            // A read that fails once the checklist shows keeps the checklist.
            (true, Err(_)) if matches!(self.tailscale, TailscalePane::Checklist(_)) => return,
            (true, Err(error)) => TailscalePane::Failed(error),
        };
        if self.tailscale != pane {
            self.tailscale = pane;
            cx.notify();
        }
    }

    /// Reads Tailscale now, and again every few seconds while the way shows.
    pub(super) fn watch_tailscale(&mut self, cx: &mut Context<Self>) {
        (self.host)(ConnectDeviceCommand::ReadTailscale, cx);
        self.tailscale_refresh = Some(cx.spawn(async move |this, cx| {
            loop {
                cx.background_executor().timer(READ_EVERY).await;
                let shows = this.update(cx, |this, cx| {
                    let shows = this.way == ConnectWay::Tailscale;
                    if shows {
                        (this.host)(ConnectDeviceCommand::ReadTailscale, cx);
                    }
                    shows
                });
                if !matches!(shows, Ok(true)) {
                    break;
                }
            }
        }));
    }

    /// One line of the checklist: its number, or a tick once it is done, its title and what it
    /// says now.
    fn tailscale_check(&self, number: usize, check: &TailscaleCheck) -> AnyElement {
        let p = self.palette;
        let mark = div()
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
            .text_color(hsla(p.foreground));
        h_flex()
            .w_full()
            .items_start()
            .gap(px(10.0))
            .child(if check.done {
                mark.child(modal_icon(ICON_CHECK, 11.0, p.success))
            } else {
                mark.child(SharedString::from(number.to_string()))
            })
            .child(
                v_flex()
                    .flex_1()
                    .min_w_0()
                    .child(
                        div()
                            .text_size(px(13.0))
                            .line_height(px(19.0))
                            .text_color(hsla(p.foreground))
                            .child(check.title),
                    )
                    .child(
                        div()
                            .text_size(px(12.0))
                            .line_height(px(17.0))
                            .text_color(hsla(p.muted))
                            .child(check.detail.clone()),
                    ),
            )
            .into_any_element()
    }

    /// The button to Settings > Remote's Tailscale row.
    fn tailscale_button(&self, label: &'static str, cx: &mut Context<Self>) -> AnyElement {
        div()
            .pt(px(2.0))
            .w(px(BUTTON_WIDTH))
            .max_w_full()
            .child(modal_action_button(
                &self.palette,
                "connect-device-open-tailscale",
                label,
                None,
                ModalButtonTone::Primary,
                false,
                |this: &mut Self, window, cx| {
                    this.close_and_send(ConnectDeviceCommand::OpenTailscale, window, cx)
                },
                cx,
            ))
            .into_any_element()
    }

    /// The Tailscale way: what it is for; then, switched off, the note and where to turn it on;
    /// switched on, the checklist and the button to the code.
    pub(super) fn tailscale_way(&self, cx: &mut Context<Self>) -> AnyElement {
        let way = v_flex().w_full().gap(px(12.0)).child(self.muted(
            "For a phone on your own Tailscale network: it joins your tailnet and connects to this computer over SSH.",
        ));
        match &self.tailscale {
            TailscalePane::Off => way
                .child(self.muted(TAILSCALE_OFF_NOTE))
                .child(self.tailscale_button("Turn on in Settings", cx)),
            TailscalePane::Unread => way
                .child(
                    h_flex()
                        .h(px(20.0))
                        .gap(px(8.0))
                        .items_center()
                        .child(modal_spinner(self.palette.muted))
                        .child(self.muted("Checking Tailscale on this computer…")),
                )
                .child(self.tailscale_button("Open Tailscale in Settings", cx)),
            TailscalePane::Failed(error) => way
                .child(modal_error(&self.palette, error.clone()))
                .child(self.tailscale_button("Open Tailscale in Settings", cx)),
            TailscalePane::Checklist(checklist) => way
                .child(
                    v_flex().w_full().gap(px(8.0)).children(
                        checklist
                            .iter()
                            .enumerate()
                            .map(|(index, check)| self.tailscale_check(index + 1, check)),
                    ),
                )
                .child(self.muted("The code is in Settings > Remote, under Tailscale."))
                .child(self.tailscale_button("Open Tailscale in Settings", cx)),
        }
        .into_any_element()
    }
}
