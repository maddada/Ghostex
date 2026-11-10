//! The notice modal: a notice that needs the user's answer (an approval, an Undo, Open Accounts,
//! Copy details), shown in the middle of the window instead of as a toast.
//!
//! CDXC:AppModal 2026-11-08 DECISION:
//! User: "please 1 thing we must fix is to never show toasts with buttons inside them 1- they look ugly 2- they can be missed. show a nice modal middle of screen for all such cases so the user doesnt' miss it and make sure the dialog looks nice please". On Windows they nearly missed a Windows Hello approval that came as a toast while adding a remote computer. Every app toast that carries a button (`GpuiAppToast::has_buttons`) opens this modal instead, centred over the main window in the dialog style: an icon, a clear title, one or two plain lines, who asked and on which computer when there is one, and the buttons. Several waiting show one at a time with "1 of 3" and move on after each answer; they never stack. An approval never approves on Enter: only a click on Approve (and then the system prompt) does, and Escape, Close or the corner X only puts it aside, still waiting, until the next notice arrives. Toasts stay for plain information with no button.
//!
//! The view only draws the notices it is given and reports clicks; the app keeps the queue
//! (app/notice_modal_host.rs). It holds no app types, so the native-modal-demo draws it too.
//! SEE-ALSO: apps/desktop/src/app/notice_modal_host.rs, apps/desktop/src/app/window/toast.rs.
use super::native_modal_kit::*;
use gpui::prelude::FluentBuilder as _;
use gpui::{
    AnyElement, App, ClipboardItem, Context, FocusHandle, FontWeight, InteractiveElement as _,
    IntoElement, KeyDownEvent, ParentElement as _, Render, Rgba, SharedString,
    StatefulInteractiveElement as _, Styled as _, Window, div, px,
};
use gpui_component::tooltip::Tooltip;
use gpui_component::{h_flex, v_flex};
use std::rc::Rc;

pub(crate) const NOTICE_MODAL_WIDTH: f32 = 440.0;
/// First-frame height only; the window is resized to the measured layout on the first prepaint.
pub(crate) const NOTICE_MODAL_INITIAL_HEIGHT: f32 = 200.0;

/// Between the words, the who-asked panel and the hint.
const NOTICE_CONTENT_GAP: f32 = 16.0;
/// The window padding, the gap above the footer and the footer row, added to the content column.
const NOTICE_FIT_EXTRA_HEIGHT: f32 =
    MODAL_WINDOW_PADDING * 2.0 + MODAL_SECTION_GAP + MODAL_FOOTER_BUTTON_HEIGHT;

const ICON_APPROVAL: &str = "modals/settings/shield-check.svg";
const ICON_INFO: &str = "modals/settings/info-circle.svg";
const ICON_SUCCESS: &str = "modals/settings/circle-check.svg";
const ICON_WARNING: &str = "modals/settings/alert-triangle.svg";
const ICON_ERROR: &str = "modals/settings/circle-x.svg";
const ICON_COPY: &str = "modals/settings/copy.svg";
const ICON_COPIED: &str = "modals/settings/circle-check.svg";
const ICON_PHONE: &str = "modals/settings/device-mobile.svg";
const ICON_COMPUTER: &str = "modals/settings/device-desktop.svg";

/// What kind of notice it is: its icon, and whether its buttons are Deny and Approve.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum NoticeModalTone {
    Info,
    Success,
    Warning,
    Error,
    /// An owner approval: Deny and Approve, and Enter does nothing.
    Approval,
}

/// One notice as the modal draws it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct NoticeModalItem {
    pub(crate) id: String,
    pub(crate) tone: NoticeModalTone,
    pub(crate) title: String,
    /// One or two plain sentences.
    pub(crate) message: Option<String>,
    /// Who asked and where ("Asked by", "Computer"), drawn as label and value rows.
    pub(crate) source: Vec<(String, String)>,
    /// The notice's own button (Open Accounts, Undo, Install with Homebrew); Approve on an
    /// approval.
    pub(crate) action_label: Option<String>,
    /// The raw error behind Copy details.
    pub(crate) copy_text: Option<String>,
    /// A small line under the rest: what Approve does next (the system prompt).
    pub(crate) hint: Option<String>,
    /// A phone or computer asking to pair: drawn like the Connect a phone window, with its name,
    /// its three words and Not my phone beside Approve (CDXC:RemotePairing 2026-11-10 in
    /// window/connect_device_modal.rs).
    pub(crate) pairing: Option<NoticeModalPairing>,
}

/// The pairing a notice asks about.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct NoticeModalPairing {
    pub(crate) name: String,
    /// "iPhone", "Android", "phone" or "computer".
    pub(crate) kind_label: String,
    pub(crate) phone: bool,
    pub(crate) words: Vec<String>,
}

/// What a click asks the app to do with the notice `id`.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum NoticeModalCommand {
    /// The notice's own button, or Approve.
    Act { id: String },
    /// Deny on an approval.
    Deny { id: String },
    /// Close, Escape or the corner X: an approval stays waiting, anything else is done.
    Close { id: String },
}

pub(crate) type NoticeModalHost = Rc<dyn Fn(NoticeModalCommand, &mut App)>;

pub(crate) struct GpuiNoticeModalWindow {
    host: NoticeModalHost,
    palette: ModalPalette,
    /// Waiting notices, oldest first; the first is shown.
    items: Vec<NoticeModalItem>,
    /// The notice whose Copy details was just pressed, which reads Copied.
    copied: Option<String>,
    fit: ModalFit,
    focus_handle: FocusHandle,
}

impl GpuiNoticeModalWindow {
    pub(crate) fn new(
        palette: ModalPalette,
        host: NoticeModalHost,
        items: Vec<NoticeModalItem>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Self {
        let focus_handle = cx.focus_handle();
        focus_handle.focus(window, cx);
        Self {
            host,
            palette,
            items,
            copied: None,
            fit: ModalFit::new(),
            focus_handle,
        }
    }

    /// The waiting notices changed: a new one joined, or the shown one was answered.
    pub(crate) fn set_items(&mut self, items: Vec<NoticeModalItem>, cx: &mut Context<Self>) {
        if items.first().map(|item| &item.id) != self.items.first().map(|item| &item.id) {
            // Another notice is shown: fit the window to it, shrinking too.
            self.fit.refit();
            self.copied = None;
        }
        self.items = items;
        cx.notify();
    }

    /// The waiting notices, the shown one first.
    #[allow(dead_code)] // The native-modal-demo's host moves its queue on with it.
    pub(crate) fn items(&self) -> &[NoticeModalItem] {
        &self.items
    }

    /// Shows the first notice's Copy details as just pressed (the native-modal-demo's `copied`).
    #[allow(dead_code)]
    pub(crate) fn mark_copied(&mut self) {
        self.copied = self.items.first().map(|item| item.id.clone());
    }

    pub(crate) fn set_palette(&mut self, palette: ModalPalette, cx: &mut Context<Self>) {
        self.palette = palette;
        cx.notify();
    }

    fn send(&mut self, command: impl FnOnce(String) -> NoticeModalCommand, cx: &mut Context<Self>) {
        let Some(item) = self.items.first() else {
            return;
        };
        (self.host)(command(item.id.clone()), cx);
    }

    fn close_current(&mut self, cx: &mut Context<Self>) {
        self.send(|id| NoticeModalCommand::Close { id }, cx);
    }

    fn on_key_down(&mut self, event: &KeyDownEvent, _window: &mut Window, cx: &mut Context<Self>) {
        let approval = self
            .items
            .first()
            .is_some_and(|item| item.tone == NoticeModalTone::Approval);
        match event.keystroke.key.as_str() {
            "escape" => self.close_current(cx),
            // Enter never approves; on any other notice it closes it, as its Close button does.
            "enter" if !approval => self.close_current(cx),
            _ => return,
        }
        cx.stop_propagation();
    }

    fn text(size: f32, line: f32, color: Rgba, text: impl Into<SharedString>) -> gpui::Div {
        div()
            .text_size(px(size))
            .line_height(px(line))
            .text_color(hsla(color))
            .child(text.into())
    }

    fn icon(&self, tone: NoticeModalTone) -> AnyElement {
        let p = self.palette;
        let (path, color) = match tone {
            NoticeModalTone::Approval => (ICON_APPROVAL, p.foreground),
            NoticeModalTone::Info => (ICON_INFO, p.foreground),
            NoticeModalTone::Success => (ICON_SUCCESS, p.success),
            NoticeModalTone::Warning => (
                ICON_WARNING,
                if p.light {
                    gpui::rgb(0xb45309)
                } else {
                    gpui::rgb(0xfbbf24)
                },
            ),
            NoticeModalTone::Error => (ICON_ERROR, p.destructive),
        };
        div()
            .flex()
            .flex_shrink_0()
            .items_center()
            .justify_center()
            .size(px(36.0))
            .rounded(px(10.0))
            .bg(hsla(p.raised))
            .child(modal_icon(path, 20.0, color))
            .into_any_element()
    }

    fn header(&self, item: &NoticeModalItem) -> AnyElement {
        let p = self.palette;
        let count = (self.items.len() > 1).then(|| {
            div()
                .flex_none()
                .whitespace_nowrap()
                .px(px(8.0))
                .h(px(20.0))
                .flex()
                .items_center()
                .rounded_full()
                .bg(hsla(p.raised))
                .text_size(px(12.0))
                .text_color(hsla(p.muted))
                .child(format!("1 of {}", self.items.len()))
        });
        h_flex()
            .w_full()
            .items_start()
            .gap(px(12.0))
            .child(self.icon(item.tone))
            .child(
                v_flex()
                    .flex_1()
                    .min_w_0()
                    .gap(px(4.0))
                    .child(
                        h_flex()
                            .w_full()
                            .items_start()
                            .gap(px(8.0))
                            .child(
                                Self::text(16.0, 21.0, p.foreground, item.title.clone())
                                    .flex_1()
                                    .min_w_0()
                                    .font_weight(FontWeight::SEMIBOLD),
                            )
                            .children(count),
                    )
                    .children(
                        item.message
                            .clone()
                            .map(|message| Self::text(13.0, 20.0, p.muted, message)),
                    ),
            )
            .into_any_element()
    }

    /// A pairing: the device, its three words and the sentence, as in the Connect a phone window.
    fn pairing(&self, item: &NoticeModalItem) -> Option<AnyElement> {
        let pairing = item.pairing.as_ref()?;
        let p = self.palette;
        Some(
            v_flex()
                .w_full()
                .gap(px(NOTICE_CONTENT_GAP))
                .child(
                    h_flex()
                        .gap(px(8.0))
                        .items_center()
                        .child(modal_icon(
                            if pairing.phone {
                                ICON_PHONE
                            } else {
                                ICON_COMPUTER
                            },
                            16.0,
                            p.foreground,
                        ))
                        .child(pairing.name.clone())
                        .child(Self::text(
                            13.0,
                            19.0,
                            p.muted,
                            format!("· {}", pairing.kind_label),
                        )),
                )
                .child(pairing_words(&p, &pairing.words, false))
                .child(Self::text(
                    13.0,
                    19.0,
                    p.muted,
                    pairing_words_sentence(pairing.phone),
                ))
                .into_any_element(),
        )
    }

    /// Who asked and where, as label and value rows in a panel.
    fn source(&self, item: &NoticeModalItem) -> Option<AnyElement> {
        if item.source.is_empty() {
            return None;
        }
        let p = self.palette;
        Some(
            modal_panel(&p)
                .children(
                    item.source
                        .iter()
                        .enumerate()
                        .map(|(index, (label, value))| {
                            h_flex()
                                .w_full()
                                .items_start()
                                .gap(px(12.0))
                                .px(px(12.0))
                                .py(px(8.0))
                                .when(index > 0, |row| {
                                    row.border_t_1().border_color(hsla(p.hairline))
                                })
                                .child(
                                    Self::text(12.0, 18.0, p.muted, label.clone())
                                        .w(px(84.0))
                                        .flex_none(),
                                )
                                .child(
                                    Self::text(13.0, 18.0, p.foreground, value.clone())
                                        .flex_1()
                                        .min_w_0(),
                                )
                        }),
                )
                .into_any_element(),
        )
    }

    /// Copy details, in the button row, never a small button of its own.
    ///
    /// CDXC:AppModal 2026-11-08 DECISION:
    /// User, answering the notices page (Q1948): never show Copy details as a small separate
    /// button. A dialog with one button gets Copy details as a second full button on the same row,
    /// left of it, each half the width; a dialog that already has two buttons gets it as an
    /// icon-only button (copy icon, "Copy details" on hover), leftmost on the row. Once pressed it
    /// reads Copied, with a check.
    fn copy_details_button(
        &self,
        item: &NoticeModalItem,
        icon_only: bool,
        cx: &mut Context<Self>,
    ) -> Option<AnyElement> {
        let text = item.copy_text.clone()?;
        let p = self.palette;
        let copied = self.copied.as_deref() == Some(item.id.as_str());
        let id = item.id.clone();
        let copy = move |this: &mut Self, cx: &mut Context<Self>| {
            cx.write_to_clipboard(ClipboardItem::new_string(text.clone()));
            this.copied = Some(id.clone());
            cx.notify();
        };
        let icon = if copied {
            modal_icon(ICON_COPIED, 16.0, p.success)
        } else {
            modal_icon(ICON_COPY, 16.0, p.foreground)
        };
        if !icon_only {
            return Some(modal_action_button(
                &p,
                "notice-modal-copy-details",
                if copied { "Copied" } else { "Copy details" },
                Some(icon.into_any_element()),
                ModalButtonTone::Neutral,
                false,
                move |this: &mut Self, _, cx| copy(this, cx),
                cx,
            ));
        }
        let fills = neutral_button_fills(p.light);
        let tip: &'static str = if copied { "Copied" } else { "Copy details" };
        Some(
            div()
                .id("notice-modal-copy-details")
                .role(gpui::Role::Button)
                .aria_label("Copy details")
                .flex()
                .flex_none()
                .items_center()
                .justify_center()
                .size(px(MODAL_FOOTER_BUTTON_HEIGHT))
                .rounded(px(MODAL_RADIUS_CONTROL))
                .bg(hsla(fills.rest))
                .cursor_pointer()
                .hover(move |button| button.bg(hsla(fills.hover)))
                .active(move |button| button.bg(hsla(fills.pressed)))
                .tooltip(move |window, cx| Tooltip::new(tip).build(window, cx))
                .on_click(cx.listener(move |this, _, _, cx| copy(this, cx)))
                .child(icon)
                .into_any_element(),
        )
    }

    fn footer(&self, item: &NoticeModalItem, cx: &mut Context<Self>) -> AnyElement {
        let p = self.palette;
        if item.tone == NoticeModalTone::Approval {
            return modal_footer(vec![
                modal_action_button(
                    &p,
                    "notice-modal-deny",
                    item.pairing
                        .as_ref()
                        .map_or("Deny", |pairing| pairing_deny_label(pairing.phone)),
                    None,
                    ModalButtonTone::Neutral,
                    false,
                    |this: &mut Self, _, cx| this.send(|id| NoticeModalCommand::Deny { id }, cx),
                    cx,
                ),
                modal_action_button(
                    &p,
                    "notice-modal-approve",
                    item.action_label
                        .clone()
                        .unwrap_or_else(|| "Approve".into()),
                    None,
                    ModalButtonTone::Primary,
                    false,
                    |this: &mut Self, _, cx| this.send(|id| NoticeModalCommand::Act { id }, cx),
                    cx,
                ),
            ]);
        }
        // Copy details joins the row: a full half-width button beside a lone Close, an icon
        // button ahead of Close and the notice's own button (`copy_details_button`).
        let mut buttons: Vec<AnyElement> = self
            .copy_details_button(item, item.action_label.is_some(), cx)
            .into_iter()
            .collect();
        buttons.push(modal_action_button(
            &p,
            "notice-modal-close",
            "Close",
            None,
            if item.action_label.is_some() {
                ModalButtonTone::Neutral
            } else {
                ModalButtonTone::Primary
            },
            false,
            |this: &mut Self, _, cx| this.close_current(cx),
            cx,
        ));
        if let Some(label) = item.action_label.clone() {
            buttons.push(modal_action_button(
                &p,
                "notice-modal-action",
                label,
                None,
                ModalButtonTone::Primary,
                false,
                |this: &mut Self, _, cx| this.send(|id| NoticeModalCommand::Act { id }, cx),
                cx,
            ));
        }
        modal_footer(buttons)
    }
}

impl Render for GpuiNoticeModalWindow {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let p = self.palette;
        let Some(item) = self.items.first().cloned() else {
            return div().id("ghostex-gpui-notice-modal").size_full();
        };
        let mut content = vec![self.header(&item)];
        content.extend(self.pairing(&item));
        content.extend(self.source(&item));
        content.extend(
            item.hint
                .clone()
                .map(|hint| modal_hint(&p, hint).into_any_element()),
        );
        let footer = self.footer(&item, cx);
        // One content column, measured whole: the kit's `modal_shell` fits a header and a body
        // with a 20px gap between them, which left a notice without a body 20px too tall and cut
        // the bottom off an approval's three parts.
        let measured = v_flex()
            .w_full()
            .on_children_prepainted(self.fit.listener(NOTICE_FIT_EXTRA_HEIGHT))
            .child(
                v_flex()
                    .w_full()
                    .gap(px(NOTICE_CONTENT_GAP))
                    .children(content),
            );
        let content = if self.fit.scrolls() {
            div()
                .id("ghostex-gpui-notice-modal-content")
                .flex_1()
                .min_h_0()
                .w_full()
                .overflow_y_scroll()
                .child(measured)
                .into_any_element()
        } else {
            measured.flex_1().into_any_element()
        };
        div()
            .id("ghostex-gpui-notice-modal")
            .size_full()
            .overflow_hidden()
            .bg(hsla(p.surface))
            .font_family(MODAL_UI_FONT)
            .text_size(px(14.0))
            .line_height(px(20.0))
            .text_color(hsla(p.foreground))
            .track_focus(&self.focus_handle)
            .on_key_down(cx.listener(Self::on_key_down))
            .child(
                v_flex()
                    .size_full()
                    .p(px(MODAL_WINDOW_PADDING))
                    .gap(px(MODAL_SECTION_GAP))
                    .child(content)
                    .child(footer),
            )
    }
}

impl ModalCornerClose for GpuiNoticeModalWindow {
    fn close_from_corner(&mut self, _window: &mut Window, cx: &mut Context<Self>) {
        self.close_current(cx);
    }
}
