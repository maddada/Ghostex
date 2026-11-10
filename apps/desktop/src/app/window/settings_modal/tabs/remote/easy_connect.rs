//! The Remote page's card frame (`path_card`, `path_head`) and the hidden code tile with its Show
//! QR popup (remote-pairing-qr-preview.tsx (deleted 2026-10-01)). Since 2026-11-10 only the
//! Tailscale code uses the tile and the popup: Easy Connect's code shows in the Connect a phone
//! window (CDXC:RemotePairing 2026-11-10 in window/connect_device_modal.rs), and the Easy Connect
//! card, its phone and computer panels, its steps and its "Only paired devices" box are gone from
//! the page (CDXC:RemotePairing 2026-11-10 in mod.rs).
//!
//! CDXC:RemotePairing 2026-10-24 DECISION:
//! User: "let's please not show the real qr here", "this is so when im in settings we dont show my real QR until i click on "Show QR" which opens it in a pop up", "also pls make that pop up that shows qr bigger by 20%", "make qr 20% bigger more so text above it is 1 line pls and qr is bigger", and "I want this area to look better also please".
//! The Connect a phone panel never encodes or draws the pairing payload: it shows a placeholder tile drawn without the data (corner markers and a hidden-eye badge) that opens the popup when clicked (2026-11-03: no separate Show QR button). The popup is the only place the real QR renders (the Tailscale card uses the same tile and popup for its code), drawn at its size rather than scaled, in a sheet wide enough that the instruction above it stays on one line. The panel's explanation is a short numbered list beside the tile. (2026-10-27: 432px, one more 20% step from 360px, so an Android scanner gets larger modules; see CDXC:RemotePairing 2026-10-27 in fields/qr.rs.)
//!
//! CDXC:RemotePairing 2026-11-03 DECISION:
//! User: "move show qr to be what happens when i click on the qr icon there ... so show text goes closer upwards to the qr icon there". The hidden QR tile is the button (pointer cursor, hover, keyboard focus); Copy as text sits right under it.
//!
//! CDXC:RemotePairing 2026-10-27 WHY:
//! The phone's Paste a pairing code sheet tells the user to click Copy as text on the computer, and testers found no such button. Copy as text sits right under the QR tile on the card and under the QR in the popup, and copies the bare `ghostex-ec1:`/`ghostex-ts1:` code (gxserver's `text`), not the link the QR carries: every phone app and every computer reads the bare code, including phones from before the link (10.32 and older), whose scanner calls the link "not a Ghostex code". The button reads Copied for a moment after a click, like the other copy buttons on this page.
use super::super::super::super::native_modal_kit::*;
use super::super::super::fields::RowControl;
use super::super::super::fields::{
    ButtonSize, ButtonVariant, icon, qr_code, settings_button_sized, settings_icon,
    settings_icon_button,
};
use super::style::*;
use super::{PathCard, RemoteTab};
use gpui::prelude::FluentBuilder as _;
use gpui::{
    AnyElement, ClickEvent, Context, FontWeight, InteractiveElement as _, IntoElement,
    KeyDownEvent, ParentElement as _, SharedString, StatefulInteractiveElement as _, Styled as _,
    Window, div, px,
};
use gpui_component::{h_flex, v_flex};

/// `.settings-remote-path-card`: the bordered card with its header row (the accordion handle
/// and the switch beside it) and, while open, its body.
#[allow(clippy::too_many_arguments)]
pub(super) fn path_card(
    t: &RemoteTokens,
    id: &'static str,
    head: AnyElement,
    switch: impl Into<RowControl>,
    body: Option<AnyElement>,
) -> AnyElement {
    let switch = switch.into();
    v_flex()
        .id(id)
        .w_full()
        .min_w_0()
        .rounded(px(MODAL_RADIUS_CONTROL))
        .border_1()
        .border_color(hsla(t.edge(0.76)))
        .bg(hsla(t.card(0.18)))
        .child(
            h_flex()
                .id(SharedString::from(format!("{id}-head")))
                .w_full()
                .items_center()
                .gap(px(12.0))
                .pr(px(12.0))
                .when_some(switch.toggle, |row, press| {
                    row.switch_row_press(switch_row_wash(t.p.foreground), false, press)
                })
                .child(head)
                .child(switch.element),
        )
        .children(body.map(|body| {
            v_flex()
                .w_full()
                .min_w_0()
                .border_t_1()
                .border_color(hsla(t.edge(0.70)))
                .p(px(12.0))
                .gap(px(10.0))
                .child(body)
        }))
        .into_any_element()
}

/// `.settings-remote-path-toggle`: the card's icon, title and badges. The cards sit in their own
/// tabs and are always open, so the row no longer expands anything.
pub(super) fn path_head(
    t: &RemoteTokens,
    id: &'static str,
    icon_path: &'static str,
    accent: bool,
    title: &'static str,
    badges: Vec<AnyElement>,
) -> AnyElement {
    h_flex()
        .id(id)
        .flex_1()
        .min_w_0()
        .min_h(px(48.0))
        .px(px(12.0))
        .py(px(10.0))
        .gap(px(10.0))
        .items_center()
        .rounded_tl(px(7.0))
        .child(path_icon(t, icon_path, accent))
        .child(
            div()
                .flex_shrink_0()
                .text_size(px(14.0))
                .line_height(px(20.0))
                .text_color(hsla(t.foreground))
                .whitespace_nowrap()
                .child(title),
        )
        .child(
            h_flex()
                .min_w_0()
                .flex_wrap()
                .items_center()
                .gap(px(6.0))
                .children(badges),
        )
        .into_any_element()
}

/// `.settings-remote-qr-block`: a QR tile on the left, its explanation on the right.
pub(super) fn qr_block(t: &RemoteTokens, left: AnyElement, meta: AnyElement) -> AnyElement {
    h_flex()
        .w_full()
        .min_w_0()
        .items_start()
        .gap(px(14.0))
        .p(px(12.0))
        .rounded(px(MODAL_RADIUS_CONTROL))
        .border_1()
        .border_color(hsla(t.edge(0.70)))
        .bg(hsla(t.card(0.18)))
        .child(div().flex_shrink_0().child(left))
        .child(meta)
        .into_any_element()
}

/// `.settings-remote-qr-meta`: 13px muted lines 6px apart.
pub(super) fn qr_meta(t: &RemoteTokens) -> gpui::Div {
    v_flex()
        .flex_1()
        .min_w_0()
        .gap(px(6.0))
        .text_size(px(13.0))
        .line_height(px(19.5))
        .text_color(hsla(t.muted))
}

/// `.settings-remote-qr-pending`: the dashed tile shown until a code exists.
pub(super) fn qr_pending(t: &RemoteTokens, size: f32, message: &'static str) -> AnyElement {
    div()
        .size(px(size))
        .flex_shrink_0()
        .flex()
        .items_center()
        .justify_center()
        .p(px(12.0))
        .rounded(px(MODAL_RADIUS_CONTROL))
        .border_1()
        .border_dashed()
        .border_color(hsla(t.edge(0.90)))
        .bg(hsla(t.card(0.40)))
        .child(
            div()
                .text_center()
                .overflow_hidden()
                .whitespace_nowrap()
                .text_ellipsis()
                .text_size(px(13.0))
                .line_height(px(18.57))
                .text_color(hsla(t.muted))
                .child(message),
        )
        .into_any_element()
}

/// Side of the placeholder tile in the Connect a phone panel.
pub(super) const QR_PLACEHOLDER_SIZE: f32 = 132.0;
/// Side of the real QR in the Show QR dialog: the old 250px grown 20% three times, or less when
/// the window is too short for it and the rest of the sheet.
const QR_PREVIEW_SIZE: f32 = 432.0;
/// The Show QR sheet's height without its QR (padding, heading, instruction, name, footnote and
/// Copy as text), plus a margin to the window's edges.
const QR_PREVIEW_CHROME_HEIGHT: f32 = 240.0;
/// Width of the Show QR dialog: wide enough for the scan instruction on one line.
const QR_PREVIEW_SHEET_WIDTH: f32 = 520.0;

/// A QR-shaped tile drawn without any pairing data: three corner markers and an eye-off badge,
/// so it reads as "the code is hidden" and can never be scanned. Clicking it (or Enter/Space when
/// focused) opens `card`'s pairing code in the Show QR dialog.
fn qr_placeholder(
    t: &RemoteTokens,
    id: &'static str,
    card: PathCard,
    cx: &mut Context<RemoteTab>,
) -> AnyElement {
    let marker = |left: bool, top: bool| {
        let inset = px(14.0);
        div()
            .absolute()
            .when(left, |this| this.left(inset))
            .when(!left, |this| this.right(inset))
            .when(top, |this| this.top(inset))
            .when(!top, |this| this.bottom(inset))
            .size(px(24.0))
            .flex()
            .items_center()
            .justify_center()
            .rounded(px(6.0))
            .border_2()
            .border_color(hsla(css_fade(t.muted, 0.45)))
            .child(
                div()
                    .size(px(8.0))
                    .rounded(px(2.0))
                    .bg(hsla(css_fade(t.muted, 0.45))),
            )
    };
    let hover_bg = t.card(0.70);
    let hover_border = t.edge(1.0);
    let focus_border = t.foreground;
    div()
        .id(id)
        .relative()
        .flex_shrink_0()
        .size(px(QR_PLACEHOLDER_SIZE))
        .flex()
        .items_center()
        .justify_center()
        .rounded(px(MODAL_RADIUS_CONTROL))
        .border_1()
        .border_color(hsla(t.edge(0.80)))
        .bg(hsla(t.card(0.40)))
        .cursor_pointer()
        .tab_stop(true)
        .hover(move |this| this.bg(hsla(hover_bg)).border_color(hsla(hover_border)))
        .focus_visible(move |this| this.border_color(hsla(focus_border)))
        .tooltip(super::super::super::fields::tooltip_text("Show QR"))
        .on_click(cx.listener(move |tab, _: &ClickEvent, window, cx| {
            // Easy Connect's code shows in the Connect a phone window (CDXC:RemotePairing
            // 2026-11-10 in window/connect_device_modal.rs); the Tailscale code keeps its popup.
            if card == PathCard::EasyConnect {
                tab.open_connect_device("phone", cx);
                return;
            }
            tab.qr_preview = Some(card);
            tab.qr_preview_focus.focus(window, cx);
            tab.refresh_fast(cx);
            cx.notify();
        }))
        .child(marker(true, true))
        .child(marker(false, true))
        .child(marker(true, false))
        .child(
            div()
                .size(px(40.0))
                .flex()
                .items_center()
                .justify_center()
                .rounded_full()
                .border_1()
                .border_color(hsla(t.edge(0.80)))
                .bg(hsla(t.card(0.18)))
                .child(settings_icon(icon::EYE_OFF, 18.0, t.muted)),
        )
        .into_any_element()
}

/*
CDXC:RemotePairing 2026-11-08 FIX:
User (GitHub #495, 11.1.1): every copy button must give a code that can pair. Since 11.1.0 gxserver
puts the one-time secret in the Easy Connect code only while the Show QR sheet is open
(`openTunnel`); with it closed it answers the address without a secret (an empty QR payload), which
a phone can't pair with and a computer refuses ("This code can't pair a computer by itself"). So an
Easy Connect code is copied only when it carries the secret, which its non-empty QR payload says:
Copy as text shows in the Show QR sheet and nowhere the code can't pair. The Tailscale code carries
no secret and is copied as before.
*/
/// `card`'s bare pairing code (gxserver's `text`), what Copy as text puts on the clipboard: for Easy
/// Connect only a code that can pair (with its one-time secret, so with a QR payload).
fn pairing_text(tab: &RemoteTab, card: PathCard) -> Option<String> {
    let pairing = tab.pairing.as_ref()?;
    let text = match card {
        PathCard::EasyConnect => {
            let code = pairing.easy_connect.as_ref()?;
            if code.payload.is_empty() {
                return None;
            }
            code.text.clone()
        }
        PathCard::Tailscale => pairing.tailscale.as_ref()?.text.clone(),
    };
    (!text.is_empty()).then_some(text)
}

/// Copy as text: copies `card`'s bare code and reads Copied with a check for a moment.
fn copy_text_button(
    tab: &RemoteTab,
    t: &RemoteTokens,
    id: &'static str,
    card: PathCard,
    cx: &mut Context<RemoteTab>,
) -> Option<AnyElement> {
    let text = pairing_text(tab, card)?;
    let copied = tab.copied.contains(id);
    Some(settings_button_sized(
        &t.p,
        id,
        // Q1879: an Easy Connect code is called that everywhere; the Tailscale code keeps its label.
        match (copied, card) {
            (true, _) => "Copied",
            (false, PathCard::EasyConnect) => "Copy Easy Connect code",
            (false, PathCard::Tailscale) => "Copy as text",
        },
        Some(if copied {
            ICON_CIRCLE_CHECK_FILLED
        } else {
            ICON_COPY
        }),
        ButtonVariant::Outline,
        ButtonSize::Sm,
        false,
        None,
        move |tab: &mut RemoteTab, _window, cx| tab.copy(id.into(), text.clone(), cx),
        cx,
    ))
}

/// The placeholder tile, which opens `card`'s pairing code in the Show QR dialog when clicked, and
/// Copy as text right under it. Shared by the Easy Connect phone panel and the Tailscale card.
pub(super) fn hidden_qr(
    tab: &RemoteTab,
    t: &RemoteTokens,
    id: &'static str,
    copy_id: &'static str,
    card: PathCard,
    cx: &mut Context<RemoteTab>,
) -> AnyElement {
    let copy = copy_text_button(tab, t, copy_id, card, cx);
    // CDXC:SettingsOverflow 2026-11-09: at least the tile's width, and as wide as the Copy button
    // under it ("Copy Easy Connect code" is wider than the tile); a fixed width let the button run
    // over the steps beside it.
    v_flex()
        .flex_shrink_0()
        .min_w(px(QR_PLACEHOLDER_SIZE))
        .items_center()
        .gap(px(8.0))
        .child(qr_placeholder(t, id, card, cx))
        .children(copy)
        .into_any_element()
}

/// The Show QR dialog: the only place the real pairing QR renders, 360px over the whole Settings
/// window.
pub(super) fn qr_preview_dialog(
    tab: &mut RemoteTab,
    t: &RemoteTokens,
    window: &mut Window,
    cx: &mut Context<RemoteTab>,
) -> Option<AnyElement> {
    let card = tab.qr_preview?;
    let pairing = tab.pairing.as_ref()?;
    // The name under the QR and its footnote; Tailscale codes carry the tailnet address instead of
    // a computer name and do not rotate after pairing.
    let (tailscale, payload, name, footnote) = match card {
        PathCard::EasyConnect => {
            let code = pairing.easy_connect.as_ref()?;
            (
                false,
                code.payload.clone(),
                code.name.clone(),
                "The QR refreshes automatically after pairing.",
            )
        }
        PathCard::Tailscale => {
            let code = pairing.tailscale.as_ref()?;
            (
                true,
                code.payload.clone(),
                code.host
                    .clone()
                    .or_else(|| code.ip.clone())
                    .unwrap_or_else(|| "This computer".to_string()),
                "The app recognizes this as a Tailscale code.",
            )
        }
    };
    let grid = tab.qr_for(tailscale, &payload);
    let copy = copy_text_button(tab, t, "remote-qr-copy-text", card, cx);
    let qr_size = QR_PREVIEW_SIZE
        .min(window.viewport_size().height.as_f32() - QR_PREVIEW_CHROME_HEIGHT)
        .max(QR_PLACEHOLDER_SIZE)
        .floor();
    let colors = dialog_colors(t);
    let close = |tab: &mut RemoteTab, _window: &mut Window, cx: &mut Context<RemoteTab>| {
        tab.qr_preview = None;
        cx.notify();
    };
    let heading = h_flex()
        .w_full()
        .items_center()
        .justify_between()
        .gap(px(12.0))
        .child(
            div()
                .text_size(px(16.0))
                .line_height(px(16.0))
                .font_weight(FontWeight::MEDIUM)
                .text_color(hsla(colors.foreground))
                .child("Connect your phone"),
        )
        .child(settings_icon_button(
            &t.p,
            "remote-qr-preview-close",
            icon::X,
            16.0,
            28.0,
            ButtonVariant::Ghost,
            None,
            false,
            close,
            cx,
        ));
    let sheet = v_flex()
        .id("remote-qr-preview")
        .track_focus(&tab.qr_preview_focus)
        .occlude()
        .w(px(QR_PREVIEW_SHEET_WIDTH))
        .max_w(window.viewport_size().width - px(32.0))
        .p(px(20.0))
        .gap(px(16.0))
        .rounded(px(MODAL_RADIUS_SECTION))
        .border_1()
        .border_color(hsla(colors.border))
        .bg(hsla(colors.background))
        .shadow(vec![gpui::BoxShadow {
            color: hsla(css_fade(colors.foreground, 0.1)),
            offset: gpui::point(px(0.0), px(0.0)),
            blur_radius: px(0.0),
            spread_radius: px(1.0),
            inset: false,
        }])
        .font_family(MODAL_UI_FONT)
        .text_size(px(14.0))
        .line_height(px(20.0))
        .text_color(hsla(colors.foreground))
        .on_mouse_down(gpui::MouseButton::Left, |_, _, cx| cx.stop_propagation())
        .on_key_down(cx.listener(move |tab, event: &KeyDownEvent, window, cx| {
            if event.keystroke.key == "escape" {
                cx.stop_propagation();
                close(tab, window, cx);
            }
        }))
        .child(heading)
        .child(
            div()
                .w_full()
                .text_size(px(14.0))
                .line_height(px(20.0))
                .whitespace_nowrap()
                .text_color(hsla(colors.muted))
                .child(rich_text(
                    colors.foreground,
                    &[
                        ("On your phone, open Ghostex → ", Emph::Plain),
                        ("Connect your computer", Emph::Em),
                        (" → ", Emph::Plain),
                        ("Scan code", Emph::Em),
                        (".", Emph::Plain),
                    ],
                )),
        )
        .child(
            h_flex()
                .w_full()
                .justify_center()
                .child(qr_code(grid.as_ref(), qr_size)),
        )
        .child(
            v_flex()
                .w_full()
                .items_center()
                .gap(px(4.0))
                .text_center()
                .child(div().text_color(hsla(colors.foreground)).child(name))
                .child(
                    div()
                        .text_size(px(13.0))
                        .line_height(px(18.57))
                        .text_color(hsla(colors.muted))
                        .child(footnote),
                )
                .children(copy.map(|button| div().pt(px(8.0)).child(button))),
        )
        .into_any_element();
    Some(dialog_overlay(
        "remote-qr-preview-overlay",
        sheet,
        close,
        window,
        cx,
    ))
}
