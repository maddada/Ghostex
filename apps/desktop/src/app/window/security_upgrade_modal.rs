//! The 11.0.1 security notice, in the middle of the screen: what changed and why, the live pairing
//! QR for phones, one row per old pairing that ticks off as it is paired again, a Pair button for
//! each computer, and Remind me later / Done.
//!
//! CDXC:Security 2026-11-08 DECISION:
//! User, of the first toast: "This is a very ugly notice man. Show it in the middle of the screen and make it clearer please." Picked design B (Q1726): the notice holds the live pairing QR, phone rows tick off as they pair, each computer has its own Pair button. Q1727: one row per pairing, same names included. Q1728: pairings unused for over a month are left out. Q1729: Remind me later brings it back tomorrow, at most twice more.
//! SEE-ALSO: server/src/security_upgrade.rs (the rows and their status), apps/desktop/src/app/security_upgrade_modal_lifecycle.rs (opening, polling, the commands), apps/desktop/src/app/gx_store/security_notice.rs (when it opens).
use super::native_modal_kit::*;
use super::settings_modal::fields::{QrGrid, qr_code};
use gpui::prelude::FluentBuilder as _;
use gpui::{
    AnyElement, App, Context, FocusHandle, FontWeight, InteractiveElement as _, IntoElement,
    KeyDownEvent, ParentElement as _, Render, SharedString, StatefulInteractiveElement as _,
    Styled as _, Window, div, px,
};
use gpui_component::{h_flex, v_flex};
use std::rc::Rc;

pub(crate) const SECURITY_UPGRADE_MODAL_WIDTH: f32 = 640.0;
/// First-frame height only; the window fits its content on the first prepaint.
pub(crate) const SECURITY_UPGRADE_MODAL_INITIAL_HEIGHT: f32 = 520.0;

const ICON_SHIELD: &str = "modals/settings/shield-check.svg";
const ICON_PHONE: &str = "modals/settings/device-mobile.svg";
const ICON_COMPUTER: &str = "modals/settings/device-desktop.svg";
const ICON_PAIRED: &str = "modals/install-hooks/circle-check.svg";
/// The one row for phones that sign in over SSH (`sshPhones` of `/api/securityUpgradeNotice`).
pub(crate) const SSH_PHONES_ROW_ID: &str = "ssh-phones";
const QR_SIZE: f32 = 156.0;
const QR_PANEL_WIDTH: f32 = 196.0;

/// Where one row stands (`status` of `/api/securityUpgradeNotice`).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum SecurityNoticeRowStatus {
    Waiting,
    Paired,
    /// Scanned the new code and waits for the owner's approval (pairing v2): the row shows the
    /// three words and Approve.
    Pending,
    /// Nothing to do: a computer reached over SSH, shown only to say so.
    Info,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct SecurityNoticeRow {
    pub(crate) id: String,
    pub(crate) name: String,
    /// "iPhone · used 2 hours ago", "Mac · connected today".
    pub(crate) detail: String,
    pub(crate) status: SecurityNoticeRowStatus,
    /// A `Pending` phone's three words, and its approval request.
    pub(crate) words: Vec<String>,
    pub(crate) approval_id: Option<String>,
}

/// The phone pairing code the notice shows (the Show QR sheet's code).
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum SecurityNoticeCode {
    /// Nothing asked for yet: the panel shows Show code, and no live code exists until it is
    /// clicked (CDXC:RemotePairing 2026-11-08 FIX in app/security_upgrade_modal_lifecycle.rs).
    Hidden,
    Loading,
    /// The QR payload.
    Ready(String),
    /// No code can be made now, with the reason (Easy Connect is off, for one).
    Unavailable(String),
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct SecurityUpgradeModalState {
    pub(crate) phones: Vec<SecurityNoticeRow>,
    pub(crate) computers: Vec<SecurityNoticeRow>,
    /// Phones that sign in over Tailscale, the local network or SSH: they pair once through that
    /// login (one row for them all, or the one waiting with its words).
    pub(crate) ssh_phones: Vec<SecurityNoticeRow>,
    /// Computers this one reaches over SSH: nothing to pair again (`Info` rows).
    pub(crate) ssh_computers: Vec<SecurityNoticeRow>,
    pub(crate) code: SecurityNoticeCode,
}

/// What the notice asks its host to do. The notice removes its own window before `Later` and
/// `Done`; `PairComputer` and `OpenRemoteSettings` close it too, so Settings is not hidden by it.
pub(crate) enum SecurityUpgradeModalCommand {
    /// Approve a phone waiting with its three words (Touch ID / Windows Hello / the system
    /// password, then the signed approval); the notice stays open and ticks the row off.
    ApprovePending(String),
    /// Open that computer's entry in Settings > Remote, to paste its new code.
    PairComputer(String),
    /// Settings > Remote, to turn on Easy Connect when no code can be made.
    OpenRemoteSettings,
    /// Show code: make the live pairing code now; the notice stays open and shows it.
    ShowCode,
    /// Remind me later (also Escape and the corner close).
    Later,
    Done,
}

pub(crate) type SecurityUpgradeModalHost = Rc<dyn Fn(SecurityUpgradeModalCommand, &mut App)>;

pub(crate) struct GpuiSecurityUpgradeModalWindow {
    host: SecurityUpgradeModalHost,
    palette: ModalPalette,
    state: SecurityUpgradeModalState,
    qr: Option<(String, Option<QrGrid>)>,
    fit: ModalFit,
    focus_handle: FocusHandle,
}

impl GpuiSecurityUpgradeModalWindow {
    pub(crate) fn new(
        palette: ModalPalette,
        state: SecurityUpgradeModalState,
        host: SecurityUpgradeModalHost,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Self {
        let focus_handle = cx.focus_handle();
        focus_handle.focus(window, cx);
        let mut modal = Self {
            host,
            palette,
            state: state.clone(),
            qr: None,
            fit: ModalFit::new(),
            focus_handle,
        };
        modal.set_state(state, cx);
        modal
    }

    /// The host's newest rows and code, polled while the notice is open.
    pub(crate) fn set_state(&mut self, state: SecurityUpgradeModalState, cx: &mut Context<Self>) {
        if let SecurityNoticeCode::Ready(payload) = &state.code
            && self.qr.as_ref().is_none_or(|(known, _)| known != payload)
        {
            self.qr = Some((payload.clone(), QrGrid::encode(payload)));
        }
        if self.state != state {
            self.state = state;
            cx.notify();
        }
    }

    fn send(
        &mut self,
        command: SecurityUpgradeModalCommand,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        // Approving keeps the notice open: the row ticks off when the phone finishes pairing. Show
        // code keeps it open too, with the code on its way.
        match command {
            SecurityUpgradeModalCommand::ApprovePending(_) => {}
            SecurityUpgradeModalCommand::ShowCode => {
                self.state.code = SecurityNoticeCode::Loading;
                cx.notify();
            }
            _ => window.remove_window(),
        }
        (self.host)(command, cx);
    }

    fn on_key_down(&mut self, event: &KeyDownEvent, window: &mut Window, cx: &mut Context<Self>) {
        if event.keystroke.key.as_str() != "escape" {
            return;
        }
        self.send(SecurityUpgradeModalCommand::Later, window, cx);
        cx.stop_propagation();
    }

    fn render_header(&self) -> AnyElement {
        let p = self.palette;
        let mut how: Vec<&str> = Vec::new();
        match (
            self.state.phones.is_empty(),
            self.state.computers.is_empty(),
        ) {
            (false, false) => {
                how.push("Scan this code with each phone, and pair each computer with its button.")
            }
            (false, true) => how.push("Scan this code with each phone."),
            (true, false) => how.push("Pair each computer with its button."),
            (true, true) => {}
        }
        if !self.state.ssh_phones.is_empty() {
            how.push("A phone that signs in over Tailscale, your network or SSH pairs once: open Ghostex on it, then approve it here when its three words match. Until then, answers, Stop and typing from it don't go through.");
        }
        if !self.state.ssh_computers.is_empty() {
            how.push(
                "Computers you reach over SSH don't pair again; they may ask you once to approve.",
            );
        }
        let how = how.join(" ");
        let title = if self.state.phones.is_empty() && self.state.computers.is_empty() {
            "Pair your phone with this computer"
        } else {
            "Pair your devices again"
        };
        h_flex()
            .w_full()
            .items_start()
            .gap(px(12.0))
            .child(
                div()
                    .flex()
                    .flex_shrink_0()
                    .items_center()
                    .justify_center()
                    .size(px(36.0))
                    .rounded(px(10.0))
                    .bg(hsla(p.raised))
                    .child(modal_icon(ICON_SHIELD, 20.0, p.foreground)),
            )
            .child(
                v_flex()
                    .min_w_0()
                    .gap(px(6.0))
                    .child(
                        div()
                            .text_size(px(16.0))
                            .line_height(px(21.0))
                            .font_weight(FontWeight::SEMIBOLD)
                            .text_color(hsla(p.foreground))
                            .child(title),
                    )
                    .child(
                        div()
                            .text_size(px(13.0))
                            .line_height(px(20.0))
                            .text_color(hsla(p.muted))
                            // CDXC:Security 2026-11-08 DECISION: User: "what do users who use tailscale need to do … we need to mention tailscale", then "outline this in the message": each route says what it needs (server/src/security_upgrade.rs `SshPhones`).
                            .child(format!(
                                "Ghostex now gives every phone and computer its own key when you pair it, so a copied code or file can no longer let anyone in. {how}"
                            )),
                    ),
            )
            .into_any_element()
    }

    fn render_qr_panel(&self, cx: &mut Context<Self>) -> AnyElement {
        let p = self.palette;
        let panel = v_flex()
            .flex_shrink_0()
            .w(px(QR_PANEL_WIDTH))
            .items_center()
            .gap(px(10.0))
            .p(px(14.0))
            .rounded(px(MODAL_RADIUS_SECTION))
            .bg(hsla(p.panel));
        let caption = |text: SharedString| {
            div()
                .text_size(px(12.0))
                .line_height(px(17.0))
                .text_color(hsla(p.muted))
                .text_center()
                .child(text)
        };
        match &self.state.code {
            SecurityNoticeCode::Hidden => panel
                .child(
                    div()
                        .rounded(px(8.0))
                        .overflow_hidden()
                        .child(qr_code(None, QR_SIZE)),
                )
                .child(caption(
                    "Show the code when a phone is ready to scan it".into(),
                ))
                .child(small_button(
                    &p,
                    "security-upgrade-show-code",
                    "Show code",
                    |this: &mut Self, window, cx| {
                        this.send(SecurityUpgradeModalCommand::ShowCode, window, cx)
                    },
                    cx,
                ))
                .into_any_element(),
            SecurityNoticeCode::Ready(_) => panel
                .child(div().rounded(px(8.0)).overflow_hidden().child(qr_code(
                    self.qr.as_ref().and_then(|(_, grid)| grid.as_ref()),
                    QR_SIZE,
                )))
                .child(caption("Scan with the Ghostex app on each phone".into()))
                .into_any_element(),
            SecurityNoticeCode::Loading => panel
                .child(
                    div()
                        .rounded(px(8.0))
                        .overflow_hidden()
                        .child(qr_code(None, QR_SIZE)),
                )
                .child(caption("Making a pairing code\u{2026}".into()))
                .into_any_element(),
            SecurityNoticeCode::Unavailable(reason) => panel
                .justify_center()
                .min_h(px(QR_SIZE + 60.0))
                .child(caption(reason.clone().into()))
                .child(small_button(
                    &p,
                    "security-upgrade-open-remote",
                    "Open Remote settings",
                    |this: &mut Self, window, cx| {
                        this.send(SecurityUpgradeModalCommand::OpenRemoteSettings, window, cx)
                    },
                    cx,
                ))
                .into_any_element(),
        }
    }

    fn render_row(
        &self,
        row: &SecurityNoticeRow,
        icon: &'static str,
        first: bool,
        computer: bool,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let p = self.palette;
        let trailing = match row.status {
            SecurityNoticeRowStatus::Paired => h_flex()
                .flex_shrink_0()
                .gap(px(5.0))
                .items_center()
                .text_size(px(12.0))
                .text_color(hsla(p.muted))
                .child(modal_icon(ICON_PAIRED, 14.0, p.success))
                .child(if row.id == SSH_PHONES_ROW_ID {
                    "Paired"
                } else {
                    "Paired again"
                })
                .into_any_element(),
            SecurityNoticeRowStatus::Waiting if computer => {
                let id = row.id.clone();
                small_button(
                    &p,
                    format!("security-upgrade-pair-{}", row.id),
                    "Pair",
                    move |this: &mut Self, window, cx| {
                        this.send(
                            SecurityUpgradeModalCommand::PairComputer(id.clone()),
                            window,
                            cx,
                        )
                    },
                    cx,
                )
            }
            SecurityNoticeRowStatus::Waiting => div()
                .flex_shrink_0()
                .text_size(px(12.0))
                .text_color(hsla(p.muted))
                .child("Waiting")
                .into_any_element(),
            SecurityNoticeRowStatus::Info => div()
                .flex_shrink_0()
                .text_size(px(12.0))
                .text_color(hsla(p.muted))
                .child("Nothing to do")
                .into_any_element(),
            SecurityNoticeRowStatus::Pending => {
                let approval_id = row.approval_id.clone().unwrap_or_default();
                h_flex()
                    .flex_shrink_0()
                    .gap(px(8.0))
                    .items_center()
                    .child(
                        div()
                            .text_size(px(13.0))
                            .font_weight(gpui::FontWeight::SEMIBOLD)
                            .text_color(hsla(p.foreground))
                            .child(row.words.join(" \u{b7} ")),
                    )
                    .child(small_button(
                        &p,
                        format!("security-upgrade-approve-{}", row.id),
                        "Approve",
                        move |this: &mut Self, window, cx| {
                            this.send(
                                SecurityUpgradeModalCommand::ApprovePending(approval_id.clone()),
                                window,
                                cx,
                            )
                        },
                        cx,
                    ))
                    .into_any_element()
            }
        };
        h_flex()
            .w_full()
            .items_center()
            .gap(px(10.0))
            .px(px(12.0))
            .py(px(8.0))
            .when(!first, |this| {
                this.border_t_1().border_color(hsla(p.hairline))
            })
            .child(modal_icon(icon, 16.0, p.muted))
            .child(
                v_flex()
                    .flex_1()
                    .min_w_0()
                    .child(
                        div()
                            .text_size(px(13.0))
                            .line_height(px(18.0))
                            .text_color(hsla(p.foreground))
                            .truncate()
                            .child(row.name.clone()),
                    )
                    .child(
                        div()
                            .text_size(px(12.0))
                            .line_height(px(17.0))
                            .text_color(hsla(p.muted))
                            .truncate()
                            .child(row.detail.clone()),
                    ),
            )
            .child(trailing)
            .into_any_element()
    }

    fn render_list(&self, cx: &mut Context<Self>) -> AnyElement {
        let p = self.palette;
        let group = |title: &'static str| {
            div()
                .px(px(12.0))
                .pt(px(8.0))
                .pb(px(4.0))
                .text_size(px(12.0))
                .line_height(px(17.0))
                .text_color(hsla(p.muted))
                .child(title)
        };
        let mut list = v_flex()
            .flex_1()
            .min_w_0()
            .rounded(px(MODAL_RADIUS_SECTION))
            .bg(hsla(p.panel))
            .overflow_hidden();
        if !self.state.phones.is_empty() {
            list = list.child(group("Phones \u{b7} scan the code"));
            for (index, row) in self.state.phones.iter().enumerate() {
                list = list.child(self.render_row(row, ICON_PHONE, index == 0, false, cx));
            }
        }
        if !self.state.computers.is_empty() {
            list = list.child(group("Computers"));
            for (index, row) in self.state.computers.iter().enumerate() {
                list = list.child(self.render_row(row, ICON_COMPUTER, index == 0, true, cx));
            }
        }
        if !self.state.ssh_phones.is_empty() {
            list = list.child(group(
                "Phones on Tailscale, your network or SSH \u{b7} open Ghostex on the phone",
            ));
            for (index, row) in self.state.ssh_phones.iter().enumerate() {
                list = list.child(self.render_row(row, ICON_PHONE, index == 0, false, cx));
            }
        }
        if !self.state.ssh_computers.is_empty() {
            list = list.child(group("Computers over SSH \u{b7} nothing to pair again"));
            for (index, row) in self.state.ssh_computers.iter().enumerate() {
                list = list.child(self.render_row(row, ICON_COMPUTER, index == 0, true, cx));
            }
        }
        list.into_any_element()
    }

    fn render_body(&self, cx: &mut Context<Self>) -> AnyElement {
        let mut body = h_flex().w_full().items_start().gap(px(20.0));
        if !self.state.phones.is_empty() {
            body = body.child(self.render_qr_panel(cx));
        }
        body.child(self.render_list(cx)).into_any_element()
    }

    fn render_footer(&self, cx: &mut Context<Self>) -> AnyElement {
        let p = self.palette;
        modal_footer(vec![
            modal_action_button(
                &p,
                "security-upgrade-later",
                "Remind me later",
                None,
                ModalButtonTone::Neutral,
                false,
                |this, window, cx| this.send(SecurityUpgradeModalCommand::Later, window, cx),
                cx,
            ),
            modal_action_button(
                &p,
                "security-upgrade-done",
                "Done",
                None,
                ModalButtonTone::Primary,
                false,
                |this, window, cx| this.send(SecurityUpgradeModalCommand::Done, window, cx),
                cx,
            ),
        ])
    }
}

/// A 26px filled button for a row or the code panel (the neutral fill, never an outline).
fn small_button<V: 'static>(
    p: &ModalPalette,
    id: impl Into<SharedString>,
    label: &'static str,
    on_click: impl Fn(&mut V, &mut Window, &mut Context<V>) + 'static,
    cx: &mut Context<V>,
) -> AnyElement {
    let fills = neutral_button_fills(p.light);
    let on_click = bind_press(cx, on_click);
    h_flex()
        .id(id.into())
        .flex_shrink_0()
        .h(px(26.0))
        .px(px(10.0))
        .items_center()
        .justify_center()
        .rounded(px(7.0))
        .bg(hsla(fills.rest))
        .text_size(px(12.0))
        .text_color(hsla(p.foreground))
        .whitespace_nowrap()
        .cursor_pointer()
        .hover(move |this| this.bg(hsla(fills.hover)))
        .active(move |this| this.bg(hsla(fills.pressed)))
        .on_bound_press(on_click)
        .child(label)
        .into_any_element()
}

impl Render for GpuiSecurityUpgradeModalWindow {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let p = self.palette;
        let content = vec![self.render_header(), self.render_body(cx)];
        let footer = self.render_footer(cx);
        modal_shell(
            &p,
            "ghostex-gpui-security-upgrade-modal",
            &self.focus_handle,
            &self.fit,
            Self::on_key_down,
            content,
            footer,
            None,
            cx,
        )
    }
}

impl ModalCornerClose for GpuiSecurityUpgradeModalWindow {
    fn close_from_corner(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.send(SecurityUpgradeModalCommand::Later, window, cx);
    }
}
