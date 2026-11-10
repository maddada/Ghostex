//! The pair-again notice after the security update, in the middle of the screen: a plain list with
//! one row per device and its status, and a page per device with only what that device needs.
//!
//! CDXC:Security 2026-11-08 DECISION:
//! User (11.1.1): "please just list for me the devices and their status (something simpler, 1 row
//! each device). then when i click on each one i can go ahead and do the reconnect stuff (each
//! depending on its own requirements)"; the list in a good-news tone ("it's great news that ghostex
//! got more secure but it'll require repairing", "need to say we did massive security audits");
//! and the code called "Easy Connect code" everywhere (Q1879). An Easy Connect phone's page makes
//! the live code (only then, so Easy Connect opens to new devices only while it shows), with Copy
//! Easy Connect code and the 5-minute note, then the phone's words and Approve. A phone on SSH: open
//! Ghostex on it, then its words and Approve. An Easy Connect computer: where to get the other
//! computer's code, a paste field, then the words to compare there (Q1876 A; Q1877: no row on the
//! other computer). A computer over SSH: nothing to do.
//!
//! CDXC:Security 2026-11-08 DECISION:
//! User, of the first toast: "This is a very ugly notice man. Show it in the middle of the screen and make it clearer please." Picked design B (Q1726): the notice holds the live pairing QR, phone rows tick off as they pair, each computer has its own Pair button. Q1727: one row per pairing, same names included. Q1728: pairings unused for over a month are left out. Q1729: Remind me later brings it back tomorrow, at most twice more.
//! SEE-ALSO: server/src/security_upgrade.rs (the rows and their status), apps/desktop/src/app/security_upgrade_modal_lifecycle.rs (opening, polling, the commands), apps/desktop/src/app/gx_store/security_notice.rs (when it opens).
use super::native_modal_kit::*;
use super::settings_modal::fields::{QrGrid, qr_code};
use gpui::prelude::FluentBuilder as _;
use gpui::{
    AnyElement, App, AppContext as _, ClipboardItem, Context, Entity, FocusHandle, FontWeight,
    InteractiveElement as _, IntoElement, KeyDownEvent, ParentElement as _, Render, Rgba,
    SharedString, StatefulInteractiveElement as _, Styled as _, Subscription, Window, div, px,
};
use gpui_component::input::{InputEvent, InputState};
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
const ICON_BACK: &str = "modals/settings/chevron-left.svg";
const ICON_CHEVRON: &str = "modals/settings/chevron-right.svg";
const ICON_COPY: &str = "modals/settings/copy.svg";
/// The phone page's QR: big enough for a phone across a desk.
const QR_SIZE: f32 = 220.0;
/// Deny and Approve under a device's words.
const ACTION_WIDTH: f32 = 110.0;
/// Pair beside the Easy Connect code field.
const PAIR_WIDTH: f32 = 84.0;

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
    /// An old pairing nobody needs to pair again (never used, an older pairing of the same phone,
    /// or a phone that signs in over SSH now): not counted, and the row offers Forget.
    Leftover,
}

/// What a row is, which decides its page.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum NoticeDevice {
    /// Pairs again with the code (QR or Copy Easy Connect code), then its words and Approve.
    EasyConnectPhone,
    /// Asks by itself from Ghostex on the phone; its words and Approve.
    SshPhone,
    /// Pairs again with the other computer's Easy Connect code, pasted here.
    EasyConnectComputer,
    /// Nothing to pair.
    SshComputer,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct SecurityNoticeRow {
    pub(crate) id: String,
    pub(crate) name: String,
    /// "iPhone · Easy Connect", "Computer · SSH": what the list shows under the name.
    pub(crate) kind_label: String,
    pub(crate) device: NoticeDevice,
    /// A `Pending` device's pending pairing id, for Deny.
    pub(crate) pending_id: Option<String>,
    /// "iPhone · used 2 hours ago", "Mac · connected today".
    pub(crate) detail: String,
    pub(crate) status: SecurityNoticeRowStatus,
    /// A `Pending` phone's three words, and its approval request.
    pub(crate) words: Vec<String>,
    pub(crate) approval_id: Option<String>,
}

/// Why a `Leftover` row needs nothing (`leftover` of `/api/securityUpgradeNotice`).
pub(crate) fn leftover_reason(reason: &str) -> &'static str {
    match reason {
        "neverUsed" => "Never connected after it was paired",
        "duplicate" => "An older pairing of the same phone",
        "ssh" => "Signs in over Tailscale or SSH now",
        _ => "Nothing to pair",
    }
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

/// Where this computer's pairing with an Easy Connect computer stands (the host runs it).
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum NoticeComputerPairing {
    Starting,
    /// Waiting for the owner to approve on the other computer, with the words it shows.
    Waiting {
        words: Vec<String>,
        computer: String,
    },
    Failed(String),
    Paired,
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
    /// The live code as text, for Copy Easy Connect code (with its one-time secret).
    pub(crate) code_text: Option<String>,
    /// Each Easy Connect computer's pairing in progress, by its Settings > Remote id.
    pub(crate) computer_pairing: Vec<(String, NoticeComputerPairing)>,
}

impl SecurityUpgradeModalState {
    /// Every row in list order.
    pub(crate) fn rows(&self) -> Vec<&SecurityNoticeRow> {
        self.phones
            .iter()
            .chain(self.ssh_phones.iter())
            .chain(self.computers.iter())
            .chain(self.ssh_computers.iter())
            .collect()
    }

    fn computer_pairing(&self, id: &str) -> Option<&NoticeComputerPairing> {
        self.computer_pairing
            .iter()
            .find(|(machine, _)| machine == id)
            .map(|(_, pairing)| pairing)
    }

    /// Nothing left to pair: every row paired or needing nothing.
    pub(crate) fn all_paired(&self) -> bool {
        self.rows().iter().all(|row| {
            matches!(
                row.status,
                SecurityNoticeRowStatus::Paired
                    | SecurityNoticeRowStatus::Info
                    | SecurityNoticeRowStatus::Leftover
            ) || self.computer_pairing(&row.id) == Some(&NoticeComputerPairing::Paired)
        })
    }

    /// Something was paired again here (not only rows that needed nothing).
    fn any_paired(&self) -> bool {
        self.rows().iter().any(|row| {
            row.status == SecurityNoticeRowStatus::Paired
                || self.computer_pairing(&row.id) == Some(&NoticeComputerPairing::Paired)
        })
    }

    /// Drops a forgotten row until the next read leaves it out too.
    fn forget(&mut self, id: &str) {
        self.phones.retain(|row| row.id != id);
        self.computers.retain(|row| row.id != id);
    }
}

/// What the notice asks its host to do. The notice removes its own window before `Later`, `Done`
/// and `OpenRemoteSettings`; the others keep it open.
pub(crate) enum SecurityUpgradeModalCommand {
    /// Approve a device waiting with its three words (Touch ID / Windows Hello / the system
    /// password, then the signed approval).
    ApprovePending(String),
    /// Deny a device waiting with its three words (its pending id).
    DenyPending(String),
    /// Pair with the Easy Connect computer `machine_id`, with the code pasted from it.
    PairComputer {
        machine_id: String,
        code: String,
    },
    /// Settings > Remote, to turn on Easy Connect when no code can be made.
    OpenRemoteSettings,
    /// A phone's page is open: make the live pairing code and keep it fresh.
    ShowCode,
    /// The phone's page closed: stop asking for the code.
    HideCode,
    /// Forget a `Leftover` row's old pairing record (its row id), after the row's confirm.
    Forget(String),
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
    /// The device whose page is open (its row id), or the list.
    page: Option<String>,
    /// The `Leftover` row asking "Forget …?" (its row id).
    confirming_forget: Option<String>,
    code_input: Entity<InputState>,
    code_text: String,
    copied: bool,
    fit: ModalFit,
    focus_handle: FocusHandle,
    _subscriptions: Vec<Subscription>,
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
        let code_input = cx.new(|cx| InputState::new(window, cx).placeholder("Easy Connect code"));
        let subscriptions = vec![cx.subscribe_in(
            &code_input,
            window,
            |this: &mut Self, input, event: &InputEvent, _window, cx| {
                if matches!(event, InputEvent::Change) {
                    this.code_text = input.read(cx).value().to_string();
                    cx.notify();
                }
            },
        )];
        let mut modal = Self {
            host,
            palette,
            state: state.clone(),
            qr: None,
            page: None,
            confirming_forget: None,
            code_input,
            code_text: String::new(),
            copied: false,
            fit: ModalFit::new(),
            focus_handle,
            _subscriptions: subscriptions,
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

    /// A leftover row's "Forget it?" (its Forget button; the preview asks it directly).
    pub(crate) fn ask_forget(&mut self, id: &str, cx: &mut Context<Self>) {
        self.confirming_forget = Some(id.to_string());
        cx.notify();
    }

    /// Forget's confirm: the old record goes, and the row with it.
    fn forget(&mut self, id: String, cx: &mut Context<Self>) {
        self.confirming_forget = None;
        self.state.forget(&id);
        (self.host)(SecurityUpgradeModalCommand::Forget(id), cx);
        cx.notify();
    }

    /// Opens `id`'s page (the preview opens on one too).
    pub(crate) fn open_page(&mut self, id: &str, cx: &mut Context<Self>) {
        let device = self.row(id).map(|row| row.device);
        self.page = Some(id.to_string());
        self.copied = false;
        if device == Some(NoticeDevice::EasyConnectPhone) {
            if self.state.code == SecurityNoticeCode::Hidden {
                self.state.code = SecurityNoticeCode::Loading;
            }
            (self.host)(SecurityUpgradeModalCommand::ShowCode, cx);
        }
        cx.notify();
    }

    fn back(&mut self, cx: &mut Context<Self>) {
        let device = self
            .page
            .as_deref()
            .and_then(|id| self.row(id))
            .map(|row| row.device);
        self.page = None;
        if device == Some(NoticeDevice::EasyConnectPhone) {
            self.state.code = SecurityNoticeCode::Hidden;
            (self.host)(SecurityUpgradeModalCommand::HideCode, cx);
        }
        cx.notify();
    }

    fn row(&self, id: &str) -> Option<&SecurityNoticeRow> {
        self.state.rows().into_iter().find(|row| row.id == id)
    }

    fn send(
        &mut self,
        command: SecurityUpgradeModalCommand,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if matches!(
            command,
            SecurityUpgradeModalCommand::Later
                | SecurityUpgradeModalCommand::Done
                | SecurityUpgradeModalCommand::OpenRemoteSettings
        ) {
            window.remove_window();
        }
        (self.host)(command, cx);
    }

    fn copy_code(&mut self, cx: &mut Context<Self>) {
        let Some(text) = self.state.code_text.clone() else {
            return;
        };
        cx.write_to_clipboard(ClipboardItem::new_string(text));
        self.copied = true;
        cx.notify();
        cx.spawn(async move |this, cx| {
            cx.background_executor()
                .timer(std::time::Duration::from_millis(1600))
                .await;
            let _ = this.update(cx, |this, cx| {
                this.copied = false;
                cx.notify();
            });
        })
        .detach();
    }

    fn on_key_down(&mut self, event: &KeyDownEvent, window: &mut Window, cx: &mut Context<Self>) {
        if event.keystroke.key.as_str() != "escape" {
            return;
        }
        if self.page.is_some() {
            self.back(cx);
        } else {
            self.send(SecurityUpgradeModalCommand::Later, window, cx);
        }
        cx.stop_propagation();
    }

    fn text(&self, size: f32, line: f32, color: Rgba, text: impl Into<SharedString>) -> gpui::Div {
        div()
            .text_size(px(size))
            .line_height(px(line))
            .text_color(hsla(color))
            .child(text.into())
    }

    // ---- the list -------------------------------------------------------------------------------

    fn render_list_header(&self) -> AnyElement {
        let p = self.palette;
        let (title, line) = if self.state.all_paired() && self.state.any_paired() {
            (
                "All paired",
                "Every phone and computer has its own key now. Thanks for pairing them.",
            )
        } else if self.state.all_paired() {
            (
                "Nothing left to pair",
                "None of these needs pairing again. Forget the old pairings you no longer use.",
            )
        } else {
            (
                "Ghostex just got more secure",
                "We put Ghostex through a big security audit, and every phone and computer now gets its own key. Pair each one below once.",
            )
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
                        self.text(16.0, 21.0, p.foreground, title)
                            .font_weight(FontWeight::SEMIBOLD),
                    )
                    .child(self.text(13.0, 20.0, p.muted, line)),
            )
            .into_any_element()
    }

    fn status_label(&self, row: &SecurityNoticeRow) -> AnyElement {
        let p = self.palette;
        let pairing = self.state.computer_pairing(&row.id);
        let paired = row.status == SecurityNoticeRowStatus::Paired
            || pairing == Some(&NoticeComputerPairing::Paired);
        let base = h_flex()
            .flex_shrink_0()
            .gap(px(5.0))
            .items_center()
            .text_size(px(12.0))
            .whitespace_nowrap();
        if paired {
            return base
                .text_color(hsla(p.muted))
                .child(modal_icon(ICON_PAIRED, 14.0, p.success))
                .child("Paired")
                .into_any_element();
        }
        let (label, strong): (String, bool) = match (row.status, pairing) {
            (SecurityNoticeRowStatus::Info, _) => ("Nothing to do".into(), false),
            (SecurityNoticeRowStatus::Pending, _) => ("Waiting for you".into(), true),
            (_, Some(NoticeComputerPairing::Waiting { computer, .. })) => {
                (format!("Waiting for {computer}"), false)
            }
            (_, Some(NoticeComputerPairing::Starting)) => ("Pairing\u{2026}".into(), false),
            _ => ("Needs pairing".into(), false),
        };
        base.text_color(hsla(if strong { p.foreground } else { p.muted }))
            .when(strong, |this| this.font_weight(FontWeight::SEMIBOLD))
            .child(label)
            .into_any_element()
    }

    fn render_row(
        &self,
        row: &SecurityNoticeRow,
        first: bool,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let p = self.palette;
        let icon = match row.device {
            NoticeDevice::EasyConnectPhone | NoticeDevice::SshPhone => ICON_PHONE,
            _ => ICON_COMPUTER,
        };
        let id = row.id.clone();
        let hover = p.raised;
        // A leftover has no page: its row only offers Forget.
        let leftover = row.status == SecurityNoticeRowStatus::Leftover;
        h_flex()
            .id(SharedString::from(format!(
                "security-upgrade-row-{}",
                row.id
            )))
            .w_full()
            .items_center()
            .gap(px(10.0))
            .px(px(12.0))
            .py(px(10.0))
            .when(!first, |this| {
                this.border_t_1().border_color(hsla(p.hairline))
            })
            .when(!leftover, |this| {
                this.cursor_pointer()
                    .hover(move |this| this.bg(hsla(hover)))
                    .on_click(cx.listener(move |this, _, _window, cx| this.open_page(&id, cx)))
            })
            .child(modal_icon(icon, 16.0, p.muted))
            .child(
                v_flex()
                    .flex_1()
                    .min_w_0()
                    .child(
                        self.text(13.0, 18.0, p.foreground, row.name.clone())
                            .truncate(),
                    )
                    .child(
                        self.text(12.0, 17.0, p.muted, row.kind_label.clone())
                            .truncate(),
                    ),
            )
            .when(leftover, |this| this.child(self.render_forget(row, cx)))
            .when(!leftover, |this| {
                this.child(self.status_label(row))
                    .child(modal_icon(ICON_CHEVRON, 14.0, p.muted))
            })
            .into_any_element()
    }

    /*
    CDXC:Security 2026-11-09 DECISION:
    User (9 Nov): "please why do i see 9 devices here??? i don't have 9 devices connected even, there's something wrong". An old pairing nobody can or needs to pair again (never used, an older pairing of the same phone, a phone that signs in over SSH now) is a `Leftover` row: not counted in Settings > Remote's line, and its row offers Forget, which asks "Forget …?" in the row (as Paired devices' Remove does) and then drops that old record from the notice only.
    */
    fn render_forget(&self, row: &SecurityNoticeRow, cx: &mut Context<Self>) -> AnyElement {
        let p = self.palette;
        let id = row.id.clone();
        if self.confirming_forget.as_deref() == Some(row.id.as_str()) {
            return h_flex()
                .flex_shrink_0()
                .items_center()
                .gap(px(6.0))
                .child(self.text(12.0, 17.0, p.muted, "Forget it?"))
                .child(small_button(
                    &p,
                    SharedString::from(format!("security-upgrade-forget-yes-{id}")),
                    "Forget",
                    move |this: &mut Self, _window, cx| this.forget(id.clone(), cx),
                    cx,
                ))
                .child(small_button(
                    &p,
                    SharedString::from(format!("security-upgrade-forget-no-{}", row.id)),
                    "Cancel",
                    |this: &mut Self, _window, cx| {
                        this.confirming_forget = None;
                        cx.notify();
                    },
                    cx,
                ))
                .into_any_element();
        }
        h_flex()
            .flex_shrink_0()
            .items_center()
            .gap(px(8.0))
            .child(self.text(12.0, 17.0, p.muted, "Not needed"))
            .child(small_button(
                &p,
                SharedString::from(format!("security-upgrade-forget-{id}")),
                "Forget",
                move |this: &mut Self, _window, cx| this.ask_forget(&id, cx),
                cx,
            ))
            .into_any_element()
    }

    fn render_list(&self, cx: &mut Context<Self>) -> AnyElement {
        let p = self.palette;
        let mut list = v_flex()
            .w_full()
            .rounded(px(MODAL_RADIUS_SECTION))
            .bg(hsla(p.panel))
            .overflow_hidden();
        for (index, row) in self.state.rows().into_iter().enumerate() {
            list = list.child(self.render_row(row, index == 0, cx));
        }
        list.into_any_element()
    }

    fn render_list_footer(&self, cx: &mut Context<Self>) -> AnyElement {
        let p = self.palette;
        let done = modal_action_button(
            &p,
            "security-upgrade-done",
            "Done",
            None,
            ModalButtonTone::Primary,
            false,
            |this, window, cx| this.send(SecurityUpgradeModalCommand::Done, window, cx),
            cx,
        );
        if self.state.all_paired() {
            return modal_footer(vec![done]);
        }
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
            done,
        ])
    }

    // ---- a device's page ------------------------------------------------------------------------

    fn render_page_header(&self, row: &SecurityNoticeRow, cx: &mut Context<Self>) -> AnyElement {
        let p = self.palette;
        let icon = match row.device {
            NoticeDevice::EasyConnectPhone | NoticeDevice::SshPhone => ICON_PHONE,
            _ => ICON_COMPUTER,
        };
        let fills = neutral_button_fills(p.light);
        h_flex()
            .w_full()
            .items_center()
            .gap(px(10.0))
            .child(
                h_flex()
                    .id("security-upgrade-back")
                    .flex_shrink_0()
                    .h(px(26.0))
                    .pl(px(6.0))
                    .pr(px(10.0))
                    .gap(px(4.0))
                    .items_center()
                    .rounded(px(7.0))
                    .bg(hsla(fills.rest))
                    .text_size(px(12.0))
                    .text_color(hsla(p.foreground))
                    .cursor_pointer()
                    .hover(move |this| this.bg(hsla(fills.hover)))
                    .on_click(cx.listener(|this, _, _window, cx| this.back(cx)))
                    .child(modal_icon(ICON_BACK, 14.0, p.foreground))
                    .child("Back"),
            )
            .child(
                div()
                    .flex()
                    .flex_shrink_0()
                    .items_center()
                    .justify_center()
                    .size(px(30.0))
                    .rounded(px(8.0))
                    .bg(hsla(p.raised))
                    .child(modal_icon(icon, 16.0, p.foreground)),
            )
            .child(
                v_flex()
                    .min_w_0()
                    .child(
                        self.text(15.0, 20.0, p.foreground, row.name.clone())
                            .font_weight(FontWeight::SEMIBOLD),
                    )
                    .child(self.text(12.0, 17.0, p.muted, row.kind_label.clone())),
            )
            .into_any_element()
    }

    fn panel(&self) -> gpui::Div {
        v_flex()
            .w_full()
            .items_center()
            .gap(px(12.0))
            .px(px(16.0))
            .py(px(18.0))
            .rounded(px(MODAL_RADIUS_SECTION))
            .bg(hsla(self.palette.panel))
    }

    fn centered(&self, text: impl Into<SharedString>) -> gpui::Div {
        self.text(13.0, 19.0, self.palette.foreground, text)
            .max_w(px(470.0))
            .text_center()
    }

    fn note(&self, text: impl Into<SharedString>) -> gpui::Div {
        self.text(12.0, 17.0, self.palette.muted, text)
            .max_w(px(470.0))
            .text_center()
    }

    fn words_block(&self, words: &[String]) -> gpui::Div {
        self.text(22.0, 28.0, self.palette.foreground, words.join(" \u{b7} "))
            .font_weight(FontWeight::BOLD)
            .text_center()
    }

    /// The words and Deny / Approve of a device waiting for the owner.
    fn render_approve(
        &self,
        row: &SecurityNoticeRow,
        ask: String,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let p = self.palette;
        let approval = row.approval_id.clone().unwrap_or_default();
        let pending = row.pending_id.clone().unwrap_or_default();
        self.panel()
            .child(self.centered(ask))
            .child(self.words_block(&row.words))
            .child(
                h_flex()
                    .gap(px(8.0))
                    .child(div().w(px(ACTION_WIDTH)).child(modal_action_button(
                        &p,
                        "security-upgrade-deny",
                        "Deny",
                        None,
                        ModalButtonTone::Neutral,
                        false,
                        move |this, window, cx| {
                            this.send(
                                SecurityUpgradeModalCommand::DenyPending(pending.clone()),
                                window,
                                cx,
                            )
                        },
                        cx,
                    )))
                    .child(div().w(px(ACTION_WIDTH)).child(modal_action_button(
                        &p,
                        "security-upgrade-approve",
                        "Approve",
                        None,
                        ModalButtonTone::Primary,
                        false,
                        move |this, window, cx| {
                            this.send(
                                SecurityUpgradeModalCommand::ApprovePending(approval.clone()),
                                window,
                                cx,
                            )
                        },
                        cx,
                    ))),
            )
            .child(self.note("Approving asks for Touch ID."))
            .into_any_element()
    }

    fn render_paired(
        &self,
        title: String,
        line: &'static str,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let p = self.palette;
        self.panel()
            .child(modal_icon(ICON_PAIRED, 30.0, p.success))
            .child(self.centered(title).font_weight(FontWeight::SEMIBOLD))
            .child(self.note(line))
            .child(small_button(
                &p,
                "security-upgrade-to-list",
                "Back to the list",
                |this: &mut Self, _window, cx| this.back(cx),
                cx,
            ))
            .into_any_element()
    }

    fn render_phone_code(&self, row: &SecurityNoticeRow, cx: &mut Context<Self>) -> AnyElement {
        let p = self.palette;
        let panel = self.panel();
        match &self.state.code {
            SecurityNoticeCode::Ready(_) => panel
                .child(div().rounded(px(8.0)).overflow_hidden().child(qr_code(
                    self.qr.as_ref().and_then(|(_, grid)| grid.as_ref()),
                    QR_SIZE,
                )))
                .child(self.centered(format!(
                    "On the {}, open Ghostex \u{203a} Connect your computer \u{203a} Scan code, and point it at this code.",
                    row.name
                )))
                .when(self.state.code_text.is_some(), |this| {
                    this.child(small_button(
                        &p,
                        "security-upgrade-copy-code",
                        if self.copied {
                            "Copied"
                        } else {
                            "Copy Easy Connect code"
                        },
                        |this: &mut Self, _window, cx| this.copy_code(cx),
                        cx,
                    ))
                })
                .child(self.note(
                    "No camera? Paste the copied code on the phone instead. The code works once, for 5 minutes, and only while this page is open.",
                ))
                .into_any_element(),
            SecurityNoticeCode::Unavailable(reason) => panel
                .child(self.centered(reason.clone()))
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
            SecurityNoticeCode::Hidden | SecurityNoticeCode::Loading => panel
                .child(
                    div()
                        .rounded(px(8.0))
                        .overflow_hidden()
                        .child(qr_code(None, QR_SIZE)),
                )
                .child(self.note("Making the Easy Connect code\u{2026}"))
                .into_any_element(),
        }
    }

    fn render_computer(
        &self,
        row: &SecurityNoticeRow,
        window: &Window,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let p = self.palette;
        match self.state.computer_pairing(&row.id) {
            Some(NoticeComputerPairing::Waiting { words, computer }) => {
                return self
                    .panel()
                    .child(self.centered(format!(
                        "Approve this computer on {computer}, in Settings \u{203a} Remote \u{203a} Paired devices, if it shows these words:"
                    )))
                    .child(self.words_block(words))
                    .child(self.note(format!("Waiting for approval on {computer}\u{2026}")))
                    .into_any_element();
            }
            Some(NoticeComputerPairing::Starting) => {
                return self
                    .panel()
                    .child(self.note(format!("Asking {}\u{2026}", row.name)))
                    .into_any_element();
            }
            _ => {}
        }
        let failed = match self.state.computer_pairing(&row.id) {
            Some(NoticeComputerPairing::Failed(message)) => Some(message.clone()),
            _ => None,
        };
        let step = |number: &'static str, body: AnyElement| {
            h_flex()
                .w_full()
                .items_start()
                .gap(px(10.0))
                .child(
                    div()
                        .flex()
                        .flex_shrink_0()
                        .items_center()
                        .justify_center()
                        .size(px(20.0))
                        .rounded_full()
                        .bg(hsla(p.raised))
                        .text_size(px(11.5))
                        .text_color(hsla(p.muted))
                        .child(number),
                )
                .child(div().flex_1().min_w_0().child(body))
        };
        let machine_id = row.id.clone();
        let code = self.code_text.trim().to_string();
        let empty = code.is_empty();
        v_flex()
            .w_full()
            .gap(px(12.0))
            .px(px(16.0))
            .py(px(18.0))
            .rounded(px(MODAL_RADIUS_SECTION))
            .bg(hsla(p.panel))
            .child(step(
                "1",
                self.text(
                    13.0,
                    19.0,
                    p.foreground,
                    format!(
                        "On {}, open Settings \u{203a} Remote \u{203a} Connect a computer \u{203a} Copy the code.",
                        row.name
                    ),
                )
                .into_any_element(),
            ))
            .child(step(
                "2",
                v_flex()
                    .gap(px(6.0))
                    .child(self.text(13.0, 19.0, p.foreground, "Paste it here:"))
                    .child(
                        h_flex()
                            .gap(px(8.0))
                            .items_center()
                            .child(div().flex_1().min_w_0().child(modal_text_input(
                                &p,
                                &self.code_input,
                                false,
                                window,
                                cx,
                            )))
                            .child(div().flex_shrink_0().w(px(PAIR_WIDTH)).child(modal_action_button(
                                &p,
                                "security-upgrade-pair-computer",
                                "Pair",
                                None,
                                ModalButtonTone::Primary,
                                empty,
                                move |this, window, cx| {
                                    this.send(
                                        SecurityUpgradeModalCommand::PairComputer {
                                            machine_id: machine_id.clone(),
                                            code: code.clone(),
                                        },
                                        window,
                                        cx,
                                    )
                                },
                                cx,
                            ))),
                    )
                    .into_any_element(),
            ))
            .when_some(failed, |this, message| this.child(modal_error(&p, message)))
            .child(self.text(
                12.0,
                17.0,
                p.muted,
                format!(
                    "The code works once, for 5 minutes. This computer keeps signing in to {} with its SSH login.",
                    row.name
                ),
            ))
            .into_any_element()
    }

    fn render_page(
        &self,
        row: &SecurityNoticeRow,
        window: &Window,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let paired = row.status == SecurityNoticeRowStatus::Paired
            || self.state.computer_pairing(&row.id) == Some(&NoticeComputerPairing::Paired);
        let pending = row.status == SecurityNoticeRowStatus::Pending;
        match row.device {
            NoticeDevice::EasyConnectPhone if paired => self.render_paired(
                format!("{} is paired again", row.name),
                "It connects with its own key from now on.",
                cx,
            ),
            NoticeDevice::EasyConnectPhone if pending => self.render_approve(
                row,
                format!(
                    "The {} scanned the code. Approve it only if it shows these words:",
                    row.name
                ),
                cx,
            ),
            NoticeDevice::EasyConnectPhone => self.render_phone_code(row, cx),
            NoticeDevice::SshPhone if paired => self.render_paired(
                format!("{} is paired", row.name),
                "Its answers, Stop and typing go through without asking.",
                cx,
            ),
            NoticeDevice::SshPhone if pending => self.render_approve(
                row,
                format!(
                    "The {} asked to pair. Approve it only if it shows these words:",
                    row.name
                ),
                cx,
            ),
            NoticeDevice::SshPhone => self
                .panel()
                .child(self.centered(
                    "Open Ghostex on the phone. It asks to pair with this computer and shows three words.",
                ))
                .child(self.note("Waiting for the phone\u{2026}"))
                .child(self.note(
                    "Until it's paired, answering an agent, Stop and typing from that phone wait for your approval here.",
                ))
                .into_any_element(),
            NoticeDevice::EasyConnectComputer if paired => self.render_paired(
                format!("{} is paired again", row.name),
                "This computer reaches it through Easy Connect again.",
                cx,
            ),
            NoticeDevice::EasyConnectComputer => self.render_computer(row, window, cx),
            NoticeDevice::SshComputer => self
                .panel()
                .child(self.centered(format!(
                    "Nothing to pair: this computer reaches {} over SSH. The first time its agents connect, you may be asked once for Touch ID.",
                    row.name
                )))
                .child(small_button(
                    &self.palette,
                    "security-upgrade-to-list",
                    "Back to the list",
                    |this: &mut Self, _window, cx| this.back(cx),
                    cx,
                ))
                .into_any_element(),
        }
    }
}

/// A 26px filled button (the neutral fill, never an outline); Copy and Copied carry their icon.
fn small_button<V: 'static>(
    p: &ModalPalette,
    id: impl Into<SharedString>,
    label: &'static str,
    on_click: impl Fn(&mut V, &mut Window, &mut Context<V>) + 'static,
    cx: &mut Context<V>,
) -> AnyElement {
    let fills = neutral_button_fills(p.light);
    let on_click = bind_press(cx, on_click);
    let icon = if label == "Copied" {
        Some(ICON_PAIRED)
    } else if label.starts_with("Copy") {
        Some(ICON_COPY)
    } else {
        None
    };
    h_flex()
        .id(id.into())
        .flex_shrink_0()
        .h(px(26.0))
        .px(px(10.0))
        .gap(px(6.0))
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
        .when_some(icon, |this, icon| {
            this.child(modal_icon(icon, 13.0, p.foreground))
        })
        .child(label)
        .into_any_element()
}

impl Render for GpuiSecurityUpgradeModalWindow {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let p = self.palette;
        let page = self.page.clone().and_then(|id| self.row(&id).cloned());
        let (content, footer) = match page {
            Some(row) => (
                vec![
                    self.render_page_header(&row, cx),
                    self.render_page(&row, window, cx),
                ],
                div().into_any_element(),
            ),
            None => (
                vec![self.render_list_header(), self.render_list(cx)],
                self.render_list_footer(cx),
            ),
        };
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
