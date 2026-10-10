//! The Easy Connect path card (remote-easy-connect.tsx (deleted 2026-10-01)) and its Enlarge QR dialog
//! (remote-pairing-qr-preview.tsx (deleted 2026-10-01)).
//!
//! CDXC:RemotePairing 2026-09-03 DECISION:
//! User: show Easy Connect and Tailscale "as expandible cards so the user clicks to expand the one they want to use. i dont want the user confused by seeing 2 qr codes in front of themselves".
//! The card is collapsed to its header row (icon, title, badges, the enable switch, a chevron) until expanded; the QR, SSH row, and paired devices only render inside the open body. The switch sits beside the header button, not inside it, so toggling Easy Connect never expands or collapses the card.
//!
//! CDXC:RemotePairing 2026-09-05 DECISION:
//! User: explain why the Tailcat CLI is needed and install it with one click; separate Connect a Phone (compact QR) from Connect a Remote machine (copy button only), and explain the SSH login on the other device.
//!
//! CDXC:RemotePairing 2026-09-06 DECISION:
//! User: show phone and computer connection choices as two tabs at the top, with only the selected instructions visible instead of both numbered sections.
//!
//! CDXC:RemotePairing 2026-09-06 DECISION:
//! User: add an enlarge button for the pairing QR that opens a 250×250 preview with a backdrop over the whole Settings dialog.
use super::super::super::super::native_modal_kit::*;
use super::super::super::fields::{
    ButtonSize, ButtonVariant, icon, qr_code, settings_button_sized, settings_icon,
    settings_icon_button, switch_control,
};
use super::model::easy_connect_status_badge;
use super::style::*;
use super::{PathCard, RemoteTab, paired_devices, ssh_row};
use gpui::prelude::FluentBuilder as _;
use gpui::{
    AnyElement, ClickEvent, Context, FontWeight, InteractiveElement as _, IntoElement,
    KeyDownEvent, ParentElement as _, SharedString, StatefulInteractiveElement as _, Styled as _,
    Window, div, px,
};
use gpui_component::{h_flex, v_flex};
use serde_json::json;

const SSH_REQUIRED_ON: &str = "Required. Easy Connect carries SSH to this computer.";
const SSH_REQUIRED_OFF: &str = "Required. Easy Connect carries SSH to this computer; turning it on asks for an admin password once.";

/// `.settings-remote-path-card`: the bordered card with its header row (the accordion handle
/// and the switch beside it) and, while open, its body.
#[allow(clippy::too_many_arguments)]
pub(super) fn path_card(
    t: &RemoteTokens,
    id: &'static str,
    head: AnyElement,
    switch: AnyElement,
    body: Option<AnyElement>,
) -> AnyElement {
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
                .w_full()
                .items_center()
                .gap(px(12.0))
                .pr(px(12.0))
                .child(head)
                .child(switch),
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

/// `.settings-remote-path-toggle`: icon, title, badges and the chevron; `on_toggle` is `None` for
/// a card that cannot open (Tailscale switched off).
#[allow(clippy::too_many_arguments)]
pub(super) fn path_head(
    t: &RemoteTokens,
    id: &'static str,
    icon_path: &'static str,
    accent: bool,
    title: &'static str,
    badges: Vec<AnyElement>,
    chevron_open: Option<bool>,
    expanded: bool,
    on_toggle: Option<PathCard>,
    cx: &mut Context<RemoteTab>,
) -> AnyElement {
    let hover = t.card(0.30);
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
        .when(!expanded, |this| this.rounded_bl(px(7.0)))
        .when(on_toggle.is_none(), |this| this.opacity(0.55))
        .when_some(on_toggle, |this, card| {
            this.cursor_pointer()
                .hover(move |this| this.bg(hsla(hover)))
                .on_click(cx.listener(move |tab, _: &ClickEvent, _window, cx| {
                    tab.toggle_path_card(card, cx);
                }))
        })
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
        .children(chevron_open.map(|open| {
            div()
                .ml_auto()
                .flex_shrink_0()
                .child(chevron(open, 16.0, t.muted))
        }))
        .into_any_element()
}

pub(super) fn easy_connect_card(
    tab: &mut RemoteTab,
    t: &RemoteTokens,
    window: &mut Window,
    cx: &mut Context<RemoteTab>,
) -> AnyElement {
    let rpc = tab.rpc_available(cx);
    let status = tab.easy_connect.clone();
    let (badge_label, badge_tone) = easy_connect_status_badge(status.as_ref());
    let binary_found = status.as_ref().is_some_and(|status| status.binary_found);
    let is_on = status.as_ref().is_some_and(|status| status.enabled);
    let expanded = tab.expanded == Some(PathCard::EasyConnect);
    let head = path_head(
        t,
        "remote-easy-connect-toggle",
        ICON_QRCODE,
        is_on,
        "Easy Connect",
        vec![
            badge(t, "Recommended", super::model::BadgeTone::Plain, true),
            badge(t, badge_label, badge_tone, false),
        ],
        Some(expanded),
        expanded,
        Some(PathCard::EasyConnect),
        cx,
    );
    let switch_disabled = !rpc || status.is_none() || !binary_found;
    let switch = switch_control(
        &t.p,
        "remote-easy-connect-switch",
        "Easy Connect",
        is_on,
        switch_disabled,
        None,
        |tab: &mut RemoteTab, checked, _window, cx| {
            tab.set_easy_connect_state(json!({ "enabled": checked, "kind": "setEnabled" }), cx);
        },
        cx,
    );
    let body = expanded.then(|| easy_connect_body(tab, t, rpc, window, cx));
    path_card(t, "remote-easy-connect-card", head, switch, body)
}

fn easy_connect_body(
    tab: &mut RemoteTab,
    t: &RemoteTokens,
    rpc: bool,
    window: &mut Window,
    cx: &mut Context<RemoteTab>,
) -> AnyElement {
    let status = tab.easy_connect.clone();
    let binary_found = status.as_ref().is_some_and(|status| status.binary_found);
    let is_on = status.as_ref().is_some_and(|status| status.enabled);
    let mut body = v_flex().w_full().min_w_0().gap(px(10.0)).child(
        div()
            .w_full()
            .text_size(px(13.0))
            .line_height(px(19.5))
            .text_color(hsla(t.muted))
            .child(if is_on {
                "Reach this computer from your phone or another computer. No VPN setup or account needed."
            } else {
                "Reach this computer from your phone or another computer. Turn on Easy Connect to get a pairing code."
            }),
    );
    if let Some(status) = status
        .as_ref()
        .filter(|status| !status.binary_found || status.installing)
    {
        body = body.child(install_block(
            tab,
            t,
            rpc,
            status.install_progress.clone(),
            cx,
        ));
    }
    let install_error = tab.install_request_error.clone().or_else(|| {
        status
            .as_ref()
            .and_then(|status| status.install_error.clone())
    });
    if let Some(error) = install_error {
        body = body.child(error_line(t, error));
    }
    body = body.child(ssh_row::ssh_access_row(
        tab,
        t,
        "easy-connect-ssh",
        SSH_REQUIRED_OFF,
        SSH_REQUIRED_ON,
        false,
        window,
        cx,
    ));
    body = if is_on {
        body.child(connect_sections(tab, t, cx))
    } else {
        body.child(off_block(t, rpc && status.is_some() && binary_found, cx))
    };
    if let Some(error) = status.as_ref().and_then(|status| status.last_error.clone()) {
        body = body.child(error_line(t, error));
    }
    if is_on {
        body = body.child(paired_devices::paired_devices_list(tab, t, rpc, cx));
    }
    body.into_any_element()
}

/// `.settings-remote-install`: why the helper is needed, the one-click install and its progress.
fn install_block(
    tab: &RemoteTab,
    t: &RemoteTokens,
    rpc: bool,
    progress: Option<String>,
    cx: &mut Context<RemoteTab>,
) -> AnyElement {
    let installing = tab.install_pending
        || tab
            .easy_connect
            .as_ref()
            .is_some_and(|status| status.installing);
    let leading = if installing {
        spinner("remote-install-spinner", 16.0, t.p.foreground, 1000)
    } else {
        settings_icon(icon::DOWNLOAD, 16.0, t.p.foreground).into_any_element()
    };
    v_flex()
        .w_full()
        .min_w_0()
        .gap(px(10.0))
        .p(px(12.0))
        .rounded(px(MODAL_RADIUS_CONTROL))
        .border_1()
        .border_color(hsla(t.edge(0.70)))
        .bg(hsla(t.card(0.18)))
        .child(
            v_flex()
                .min_w_0()
                .gap(px(3.0))
                .child(
                    div()
                        .min_w_0()
                        .overflow_hidden()
                        .whitespace_nowrap()
                        .text_ellipsis()
                        .text_size(px(14.0))
                        .line_height(px(20.0))
                        .text_color(hsla(t.foreground))
                        .child("Install the Easy Connect helper"),
                )
                .child(detail(
                    t,
                    "Easy Connect uses the Tailcat CLI to carry your SSH connection through an encrypted tunnel, so your devices can reach this computer across networks. Ghostex installs and manages the helper for you.",
                    true,
                )),
        )
        .child(div().flex().child(compact_button(
            &t.p,
            "remote-install-easy-connect",
            if installing {
                "Installing Easy Connect…"
            } else {
                "Install Easy Connect"
            },
            Some(leading),
            Look::Bordered,
            28.0,
            14.0,
            None,
            !rpc || installing,
            None,
            |tab: &mut RemoteTab, _window, cx| tab.install_easy_connect(cx),
            cx,
        )))
        .child(detail(
            t,
            progress.unwrap_or_else(|| {
                "One-time setup. Any required build tools are downloaded automatically. No terminal commands needed.".to_string()
            }),
            true,
        ))
        .into_any_element()
}

/// The two connection tabs and the selected one's instructions.
fn connect_sections(
    tab: &mut RemoteTab,
    t: &RemoteTokens,
    cx: &mut Context<RemoteTab>,
) -> AnyElement {
    let segmented = raised_segmented(
        &t.p,
        "remote-connect-device",
        &[
            Segment {
                value: "phone",
                label: "Connect a phone",
                icon: Some(ICON_DEVICE_MOBILE),
            },
            Segment {
                value: "computer",
                label: "Connect a computer",
                icon: Some(ICON_DEVICE_DESKTOP),
            },
        ],
        tab.connection_device,
        false,
        |tab: &mut RemoteTab, value, _window, cx| {
            tab.connection_device = value;
            cx.notify();
        },
        cx,
    );
    let panel = if tab.connection_device == "phone" {
        phone_panel(tab, t, cx)
    } else {
        computer_panel(tab, t, cx)
    };
    v_flex()
        .w_full()
        .min_w_0()
        .gap(px(12.0))
        .child(segmented)
        .child(panel)
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

fn phone_panel(tab: &mut RemoteTab, t: &RemoteTokens, cx: &mut Context<RemoteTab>) -> AnyElement {
    let code = tab
        .pairing
        .as_ref()
        .and_then(|pairing| pairing.easy_connect.clone());
    let left = match &code {
        Some(code) => {
            let grid = tab.qr_for(false, &code.payload);
            v_flex()
                .w(px(144.0))
                .gap(px(8.0))
                .child(qr_code(grid.as_ref(), 144.0))
                .child(compact_button(
                    &t.p,
                    "remote-enlarge-qr",
                    "Enlarge QR",
                    Some(
                        settings_icon(ICON_ARROWS_MAXIMIZE, 16.0, t.p.foreground)
                            .into_any_element(),
                    ),
                    Look::Outline,
                    28.0,
                    14.0,
                    None,
                    false,
                    None,
                    |tab: &mut RemoteTab, window, cx| {
                        tab.qr_preview_open = true;
                        tab.qr_preview_focus.focus(window, cx);
                        cx.notify();
                    },
                    cx,
                ))
                .into_any_element()
        }
        None => qr_pending(t, 144.0, "Waiting for the address…"),
    };
    let mut meta = qr_meta(t)
        .child(
            div()
                .text_color(hsla(t.foreground))
                .child("Scan with your phone"),
        )
        .child(rich_text(
            t.foreground,
            &[
                ("On your phone, open Ghostex → ", Emph::Plain),
                ("Connect your computer", Emph::Em),
                (" → ", Emph::Plain),
                ("Scan code", Emph::Em),
                (" and scan this QR.", Emph::Plain),
            ],
        ));
    if let Some(code) = &code {
        meta = meta.child(rich_text(
            t.foreground,
            &[
                ("Pairs as ", Emph::Plain),
                (&code.user, Emph::Strong),
                (" on ", Emph::Plain),
                (&code.name, Emph::Strong),
                (". Nothing to type on the phone.", Emph::Plain),
            ],
        ));
    }
    meta =
        meta.child("The QR refreshes after pairing. Remove a paired phone below to disconnect it.");
    qr_block(t, left, meta.into_any_element())
}

fn computer_panel(
    tab: &mut RemoteTab,
    t: &RemoteTokens,
    cx: &mut Context<RemoteTab>,
) -> AnyElement {
    let code = tab
        .pairing
        .as_ref()
        .and_then(|pairing| pairing.easy_connect.clone());
    let user = code.as_ref().map(|code| format!("{}", code.user));
    let step_three: Vec<(&str, Emph)> = match &user {
        Some(user) => vec![
            ("Confirm this computer’s SSH username (", Emph::Plain),
            (user.as_str(), Emph::Strong),
            (
                ") and enter its SSH password. If you use an SSH key, choose it under Advanced instead.",
                Emph::Plain,
            ),
        ],
        None => vec![(
            "Confirm this computer’s SSH username and enter its SSH password. If you use an SSH key, choose it under Advanced instead.",
            Emph::Plain,
        )],
    };
    let steps: Vec<Vec<(&str, Emph)>> = vec![
        vec![(
            "On the other computer, open Ghostex → Settings → Remote → Add a machine.",
            Emph::Plain,
        )],
        vec![
            ("Choose ", Emph::Plain),
            ("Easy Connect code", Emph::Strong),
            (" and paste the copied code.", Emph::Plain),
        ],
        step_three,
        vec![
            ("Click ", Emph::Plain),
            ("Add machine", Emph::Strong),
            (", then open it from the sidebar.", Emph::Plain),
        ],
    ];
    let list = v_flex()
        .w_full()
        .gap(px(6.0))
        .my(px(4.0))
        .children(steps.iter().enumerate().map(|(index, parts)| {
            h_flex()
                .w_full()
                .items_start()
                .child(
                    div()
                        .w(px(20.0))
                        .flex_shrink_0()
                        .flex()
                        .justify_end()
                        .pr(px(5.0))
                        .child(format!("{}.", index + 1)),
                )
                .child(
                    div()
                        .flex_1()
                        .min_w_0()
                        .child(rich_text(t.foreground, parts)),
                )
        }));
    let mut meta = qr_meta(t)
        .child(
            div()
                .text_color(hsla(t.foreground))
                .child("Use a code on your other computer"),
        )
        .child("To add this computer on another computer, copy its Easy Connect code below.")
        .child(list);
    if let Some(code) = &code {
        let copied = tab.copied.contains("remote-copy-code");
        let payload = code.payload.clone();
        let leading_icon = if copied {
            ICON_CIRCLE_CHECK_FILLED
        } else {
            ICON_COPY
        };
        let button = settings_button_sized(
            &t.p,
            "remote-copy-code",
            "Copy Easy Connect code",
            Some(leading_icon),
            ButtonVariant::Primary,
            ButtonSize::Sm,
            false,
            None,
            move |tab: &mut RemoteTab, _window, cx| {
                tab.copy("remote-copy-code".into(), payload.clone(), cx);
            },
            cx,
        );
        let tooltip: SharedString = if copied {
            "Copied".into()
        } else {
            "Copy Easy Connect code for another computer".into()
        };
        meta = meta.child(
            h_flex().mt(px(2.0)).gap(px(8.0)).child(
                div()
                    .id("remote-copy-code-tooltip")
                    .tooltip(super::super::super::fields::tooltip_text(tooltip))
                    .child(button),
            ),
        );
    }
    div()
        .w_full()
        .min_w_0()
        .p(px(14.0))
        .rounded(px(MODAL_RADIUS_CONTROL))
        .border_1()
        .border_color(hsla(t.edge(0.70)))
        .bg(hsla(t.card(0.18)))
        .child(meta)
        .into_any_element()
}

/// `.settings-remote-off-block`: the faded QR placeholder and the Turn on button.
fn off_block(t: &RemoteTokens, can_turn_on: bool, cx: &mut Context<RemoteTab>) -> AnyElement {
    h_flex()
        .w_full()
        .min_w_0()
        .items_center()
        .gap(px(14.0))
        .p(px(16.0))
        .rounded(px(MODAL_RADIUS_CONTROL))
        .border_1()
        .border_color(hsla(t.edge(0.70)))
        .bg(hsla(t.card(0.18)))
        .child(
            div()
                .flex_shrink_0()
                .size(px(120.0))
                .bg(hsla(gpui::rgb(0xf4f4f5)))
                .opacity(0.15),
        )
        .child(
            v_flex()
                .flex_1()
                .min_w_0()
                .items_start()
                .gap(px(8.0))
                .text_size(px(13.0))
                .line_height(px(18.57))
                .text_color(hsla(t.foreground))
                .child("Turn on Easy Connect to get a pairing code")
                .child(detail(
                    t,
                    "Ghostex keeps it running while the app is open. You can turn it off any time; paired phones and computers stop being able to reach this computer.",
                    true,
                ))
                .child(compact_button(
                    &t.p,
                    "remote-turn-on-easy-connect",
                    "Turn on Easy Connect",
                    Some(settings_icon(ICON_POWER, 16.0, t.p.foreground).into_any_element()),
                    Look::Bordered,
                    28.0,
                    14.0,
                    None,
                    !can_turn_on,
                    None,
                    |tab: &mut RemoteTab, _window, cx| {
                        tab.set_easy_connect_state(json!({ "enabled": true, "kind": "setEnabled" }), cx);
                    },
                    cx,
                )),
        )
        .into_any_element()
}

/// The Enlarge QR dialog: a 250px QR over the whole Settings window.
pub(super) fn qr_preview_dialog(
    tab: &mut RemoteTab,
    t: &RemoteTokens,
    window: &mut Window,
    cx: &mut Context<RemoteTab>,
) -> Option<AnyElement> {
    if !tab.qr_preview_open {
        return None;
    }
    let code = tab
        .pairing
        .as_ref()
        .and_then(|pairing| pairing.easy_connect.clone())?;
    let grid = tab.qr_for(false, &code.payload);
    let colors = dialog_colors(t);
    let close = |tab: &mut RemoteTab, _window: &mut Window, cx: &mut Context<RemoteTab>| {
        tab.qr_preview_open = false;
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
        .w(px(340.0))
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
                .child(qr_code(grid.as_ref(), 250.0)),
        )
        .child(
            v_flex()
                .w_full()
                .items_center()
                .gap(px(4.0))
                .text_center()
                .child(
                    div()
                        .text_color(hsla(colors.foreground))
                        .child(code.name.clone()),
                )
                .child(
                    div()
                        .text_size(px(13.0))
                        .line_height(px(18.57))
                        .text_color(hsla(colors.muted))
                        .child("The QR refreshes automatically after pairing."),
                ),
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
