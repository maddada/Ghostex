//! The Connect a device window's three ways: the list at the left (Phone, Computer, Tailscale),
//! the picked way at the right, the footer under both, and the phone's code large enough to scan.
//! The decision record is in the window's own file (CDXC:RemotePairing 2026-11-11 DECISION in
//! window/connect_device_modal.rs); the Tailscale way is tailscale_pane.rs.
use super::*;

/// One of the three ways the list offers.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum ConnectWay {
    Phone,
    Computer,
    Tailscale,
}

impl ConnectWay {
    const ALL: [Self; 3] = [Self::Phone, Self::Computer, Self::Tailscale];

    /// The way the window opens on for the open message's device.
    pub(super) fn of_kind(kind: ConnectDeviceKind) -> Self {
        match kind {
            ConnectDeviceKind::Phone => Self::Phone,
            ConnectDeviceKind::Computer => Self::Computer,
        }
    }

    fn id(self) -> &'static str {
        match self {
            Self::Phone => "connect-device-way-phone",
            Self::Computer => "connect-device-way-computer",
            Self::Tailscale => "connect-device-way-tailscale",
        }
    }

    fn name(self) -> &'static str {
        match self {
            Self::Phone => "Phone",
            Self::Computer => "Computer",
            Self::Tailscale => "Tailscale",
        }
    }

    /// What the way takes, in three words under its name.
    fn hint(self) -> &'static str {
        match self {
            Self::Phone => "Scan a code",
            Self::Computer => "Paste a code",
            Self::Tailscale => "Your own network",
        }
    }

    fn icon(self) -> &'static str {
        match self {
            Self::Phone => ICON_PHONE,
            Self::Computer => ICON_COMPUTER,
            Self::Tailscale => ICON_TAILSCALE,
        }
    }
}

/// The list's width, and the gap between it and the picked way.
const WAYS_WIDTH: f32 = 148.0;
const WAYS_GAP: f32 = 20.0;
/// The Remote button's width in the footer.
const REMOTE_BUTTON_WIDTH: f32 = 104.0;

impl GpuiConnectDeviceWindow {
    /// The list and a way show: no device is waiting, finishing or connected.
    pub(super) fn shows_code_screen(&self) -> bool {
        !matches!(self.stage, Stage::Connected(_) | Stage::Finishing { .. })
            && !(self.stage == Stage::Ready && self.waiting.is_some())
    }

    /// The phone's code fills the window.
    pub(super) fn shows_large_code(&self) -> bool {
        self.large_code
            && self.way == ConnectWay::Phone
            && self.stage == Stage::Ready
            && self.code.is_some()
            && self.waiting.is_none()
    }

    /// Shows the phone's code large, or small in the list again.
    pub(crate) fn set_large_code(&mut self, large: bool, cx: &mut Context<Self>) {
        self.large_code = large;
        cx.notify();
    }

    /// Which screen shows, for sizing the window again when it changes.
    pub(super) fn screen(&self) -> u8 {
        if let Some(flow) = &self.join {
            return 20 + flow.step.screen();
        }
        match (&self.stage, &self.waiting) {
            (Stage::Connected(_), _) => 3,
            (Stage::Finishing { .. }, _) => 2,
            (Stage::Ready, Some(_)) => 1,
            _ if self.shows_large_code() => 4,
            _ if self.way == ConnectWay::Tailscale => 10 + self.tailscale.screen(),
            (Stage::Ready, None) if self.code.is_some() && self.code_error.is_some() => 19,
            (Stage::Ready, None) if self.code.is_some() => 5,
            (Stage::NeedsHelper(None), _) => 6,
            (Stage::NeedsHelper(Some(_)), _) => 7,
            (Stage::InstallingHelper(_), _) => 8,
            (Stage::Failed(_), _) => 9,
            _ => 0,
        }
    }

    /// Picks a way from the list.
    pub(crate) fn switch_way(&mut self, way: ConnectWay, cx: &mut Context<Self>) {
        self.way = way;
        self.large_code = false;
        if way == ConnectWay::Tailscale {
            self.watch_tailscale(cx);
        } else {
            self.tailscale_refresh = None;
        }
        cx.notify();
    }

    /// The list of ways: each its icon, its name and what it takes; the picked one is filled.
    fn ways_list(&self, cx: &mut Context<Self>) -> AnyElement {
        let p = self.palette;
        let fills = neutral_button_fills(p.light);
        v_flex()
            .w(px(WAYS_WIDTH))
            .flex_shrink_0()
            .gap(px(4.0))
            .children(ConnectWay::ALL.map(|way| {
                let picked = way == self.way;
                h_flex()
                    .id(way.id())
                    .role(gpui::Role::Button)
                    .aria_label(format!("{}: {}", way.name(), way.hint()))
                    .aria_selected(picked)
                    .w_full()
                    .px(px(10.0))
                    .py(px(8.0))
                    .gap(px(10.0))
                    .items_center()
                    .rounded(px(MODAL_RADIUS_CONTROL))
                    .cursor_pointer()
                    .when(picked, |this| this.bg(hsla(fills.rest)))
                    .when(!picked, |this| {
                        this.hover(move |this| this.bg(hsla(p.panel)))
                    })
                    .on_press(cx, move |this, _window, cx| this.switch_way(way, cx))
                    .child(
                        div()
                            .flex_shrink_0()
                            .child(modal_icon(way.icon(), 16.0, p.foreground)),
                    )
                    .child(
                        v_flex()
                            .min_w_0()
                            .child(
                                div()
                                    .text_size(px(13.5))
                                    .line_height(px(19.0))
                                    .font_weight(FontWeight::MEDIUM)
                                    .text_color(hsla(p.foreground))
                                    .child(way.name()),
                            )
                            .child(
                                div()
                                    .text_size(px(11.5))
                                    .line_height(px(16.0))
                                    .text_color(hsla(p.muted))
                                    .child(way.hint()),
                            ),
                    )
            }))
            .into_any_element()
    }

    /// The spinner and a line beside it, starting at the way's left edge.
    fn waiting_line(&self, text: &'static str) -> AnyElement {
        h_flex()
            .h(px(20.0))
            .gap(px(8.0))
            .items_center()
            .child(modal_spinner(self.palette.muted))
            .child(self.muted(text))
            .into_any_element()
    }

    /// The Phone way: the small code to click with Copy the code under it, this computer's name,
    /// the waiting line and the steps. Until there is a code, its square says what is happening.
    fn phone_way(&mut self, cx: &mut Context<Self>) -> AnyElement {
        let p = self.palette;
        let ready = self.stage == Stage::Ready && self.code.is_some();
        if !ready {
            return v_flex()
                .w_full()
                .gap(px(16.0))
                .child(self.code_placeholder(QR_SMALL_SIZE, cx))
                .child(self.steps())
                .into_any_element();
        }
        let grid = self.qr_grid();
        let code = v_flex()
            .id("connect-device-code")
            .role(gpui::Role::Button)
            .aria_label("The code. Click to enlarge it for scanning")
            .w(px(QR_SMALL_SIZE))
            .gap(px(6.0))
            .items_center()
            .cursor_pointer()
            .on_press(cx, |this, _window, cx| this.set_large_code(true, cx))
            .child(qr_code(grid.as_ref(), QR_SMALL_SIZE))
            .child(
                h_flex()
                    .gap(px(5.0))
                    .items_center()
                    .text_size(px(12.0))
                    .line_height(px(17.0))
                    .text_color(hsla(p.muted))
                    .child(modal_icon(ICON_ENLARGE, 12.0, p.muted))
                    .child("Click to enlarge"),
            );
        v_flex()
            .w_full()
            .gap(px(12.0))
            .child(code)
            .children(self.copy_button(QR_SMALL_SIZE, cx))
            .child(
                v_flex()
                    .w_full()
                    .gap(px(4.0))
                    .child(
                        div()
                            .text_size(px(14.0))
                            .line_height(px(20.0))
                            .text_color(hsla(p.foreground))
                            .child(self.computer_name.clone()),
                    )
                    .child(self.waiting_line("Waiting for your phone…")),
            )
            .child(self.steps())
            .into_any_element()
    }

    /// The phone's two steps (a computer's pane says its own, computer_way.rs).
    fn steps(&self) -> AnyElement {
        let steps = vec![
            self.step(1, vec![("Open Ghostex on your phone.", false)]),
            self.step(
                2,
                vec![
                    ("Tap ", false),
                    ("Scan the code ", true),
                    ("and point it here.", false),
                ],
            ),
        ];
        v_flex()
            .w_full()
            .gap(px(8.0))
            .children(steps)
            .into_any_element()
    }

    /// Copy the code, `width` wide, right under the code it copies: the neutral button for a
    /// phone, the main button for a computer, with Copied for a moment.
    fn copy_button(&self, width: f32, cx: &mut Context<Self>) -> Option<AnyElement> {
        let ready = self.stage == Stage::Ready && self.code.is_some();
        if !ready {
            return None;
        }
        let p = self.palette;
        let tone = match self.kind() {
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
                .w(px(width))
                .max_w_full()
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

    /// Under the list and the way, over a hairline: the Remote button (Settings > Remote, where
    /// the paired devices are) at the left; at the right "Need the app? Android · iPhone", or for a
    /// computer how long its code works.
    fn footer(&self, cx: &mut Context<Self>) -> AnyElement {
        let p = self.palette;
        let right: AnyElement = match self.way {
            ConnectWay::Phone | ConnectWay::Tailscale => h_flex()
                .flex_wrap()
                .justify_end()
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
            // The pane says how long its code works (computer_way.rs).
            ConnectWay::Computer => div().into_any_element(),
        };
        h_flex()
            .w_full()
            .min_h(px(MODAL_CONTROL_HEIGHT))
            .items_center()
            .justify_between()
            .gap(px(12.0))
            .pt(px(14.0))
            .border_t_1()
            .border_color(hsla(p.hairline))
            .child(
                div()
                    .w(px(REMOTE_BUTTON_WIDTH))
                    .flex_shrink_0()
                    .child(modal_action_button(
                        &p,
                        "connect-device-remote",
                        "Remote",
                        Some(modal_icon(ICON_SETTINGS, 14.0, p.foreground).into_any_element()),
                        ModalButtonTone::Neutral,
                        false,
                        |this: &mut Self, window, cx| {
                            this.close_and_send(ConnectDeviceCommand::OpenRemote, window, cx)
                        },
                        cx,
                    )),
            )
            .child(div().min_w_0().child(right))
            .into_any_element()
    }

    /// The list of ways, the picked way beside it, and the footer under both.
    pub(super) fn ways_body(&mut self, window: &mut Window, cx: &mut Context<Self>) -> AnyElement {
        let way = match self.way {
            ConnectWay::Phone => self.phone_way(cx),
            ConnectWay::Computer => self.computer_way(window, cx),
            ConnectWay::Tailscale => self.tailscale_way(cx),
        };
        v_flex()
            .w_full()
            .gap(px(16.0))
            .child(
                h_flex()
                    .w_full()
                    .items_start()
                    .gap(px(WAYS_GAP))
                    .child(self.ways_list(cx))
                    .child(div().flex_1().min_w_0().child(way)),
            )
            .children(
                self.poll_error
                    .clone()
                    .filter(|_| self.stage == Stage::Ready && self.way != ConnectWay::Tailscale)
                    .map(|error| modal_error(&self.palette, error)),
            )
            .child(self.footer(cx))
            .into_any_element()
    }

    /// The phone's code at the size a phone scans, alone in the window with this computer's name
    /// and the waiting line. A click on it, or Escape, goes back to the list.
    pub(super) fn large_code_body(&mut self, cx: &mut Context<Self>) -> AnyElement {
        let p = self.palette;
        let grid = self.qr_grid();
        v_flex()
            .w_full()
            .items_center()
            .gap(px(8.0))
            .child(
                div()
                    .id("connect-device-large-code")
                    .role(gpui::Role::Button)
                    .aria_label("The code, large. Click to make it small again")
                    .cursor_pointer()
                    .on_press(cx, |this, _window, cx| this.set_large_code(false, cx))
                    .child(qr_code(grid.as_ref(), QR_LARGE_SIZE)),
            )
            .child(
                div()
                    .pt(px(4.0))
                    .text_size(px(14.0))
                    .line_height(px(20.0))
                    .text_color(hsla(p.foreground))
                    .child(self.computer_name.clone()),
            )
            .child(self.spinner_line("Waiting for your phone…"))
            .child(
                div()
                    .pt(px(4.0))
                    .text_size(px(12.0))
                    .line_height(px(17.0))
                    .text_color(hsla(p.muted))
                    .child("Click the code or press Escape to go back."),
            )
            .into_any_element()
    }
}
