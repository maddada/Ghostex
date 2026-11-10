//! The paired device list (remote-paired-devices.tsx (deleted 2026-10-01)).
//!
//! CDXC:RemotePairing 2026-09-03:
//! The friendly face of the pairing registry: one row per device with its platform glyph, when it paired, and whether it checked in within the last three minutes. Remove asks for confirmation inline, naming the device, and then posts `/api/removePairedDevice`, which also drops the device's SSH key.
use super::super::super::super::native_modal_kit::*;
use super::RemoteTab;
use super::model::{
    PendingDevice, format_paired_device_detail, is_paired_device_connected_now, is_phone_platform,
};
use super::style::*;
use gpui::prelude::FluentBuilder as _;
use gpui::{
    AnyElement, Context, IntoElement, ParentElement as _, SharedString, Styled as _, div, px,
};
use gpui_component::{h_flex, v_flex};

/// `.settings-remote-rows`: a bordered list whose rows divide with hairlines.
pub(super) fn rows_frame(t: &RemoteTokens, rows: Vec<AnyElement>) -> AnyElement {
    let divider = t.edge(0.70);
    v_flex()
        .w_full()
        .min_w_0()
        .rounded(px(MODAL_RADIUS_CONTROL))
        .border_1()
        .border_color(hsla(t.edge(0.76)))
        .children(rows.into_iter().enumerate().map(|(index, row)| {
            div()
                .w_full()
                .when(index > 0, |this| {
                    this.border_t_1().border_color(hsla(divider))
                })
                .child(row)
        }))
        .into_any_element()
}

/// `.settings-remote-row`: 40px min, 8px/12px padding, the main text left and the value right.
pub(super) fn remote_row(main: AnyElement, value: Option<AnyElement>) -> AnyElement {
    h_flex()
        .w_full()
        .min_w_0()
        .min_h(px(40.0))
        .px(px(12.0))
        .py(px(8.0))
        .gap(px(12.0))
        .items_center()
        .justify_between()
        // A row with no value on its right takes the whole width, so a control in it (the Relay
        // picker) stretches across the card instead of sizing to its text.
        .child(
            div()
                .min_w_0()
                .when(value.is_none(), |this| this.flex_1())
                .child(main),
        )
        .children(value)
        .into_any_element()
}

/// The waiting and paired devices, waiting first, whether Easy Connect is on or off: a phone that
/// signs in over Tailscale or SSH pairs too (06c0253b8, Gregg's report).
pub(super) fn device_rows(
    tab: &mut RemoteTab,
    t: &RemoteTokens,
    rpc: bool,
    cx: &mut Context<RemoteTab>,
) -> AnyElement {
    // A confirmation for a device that went away is dropped.
    if let Some(confirming) = tab.confirming_device.clone()
        && tab
            .paired_devices
            .as_ref()
            .is_some_and(|devices| !devices.iter().any(|device| device.id == confirming))
    {
        tab.confirming_device = None;
    }
    let rows: Vec<AnyElement> = match tab.paired_devices.clone() {
        None => vec![remote_row(
            detail(t, "Loading paired devices…", false).into_any_element(),
            None,
        )],
        Some(devices) if devices.is_empty() => vec![remote_row(
            detail(
                t,
                "None yet. Click Connect a phone and scan the code with the Ghostex app.",
                false,
            )
            .into_any_element(),
            None,
        )],
        Some(devices) => devices
            .into_iter()
            .map(|device| {
                let connected = is_paired_device_connected_now(&device, tab.now);
                let glyph = if is_phone_platform(&device.platform) {
                    ICON_DEVICE_MOBILE
                } else {
                    ICON_DEVICE_LAPTOP
                };
                let confirming = tab.confirming_device.as_deref() == Some(device.id.as_str());
                let removing = tab.removing_device.as_deref() == Some(device.id.as_str());
                let main = v_flex()
                    .min_w_0()
                    .gap(px(3.0))
                    .child(
                        h_flex()
                            .items_center()
                            .gap(px(6.0))
                            .text_size(px(14.0))
                            .line_height(px(20.0))
                            .text_color(hsla(t.foreground))
                            .child(super::super::super::fields::settings_icon(
                                glyph,
                                15.0,
                                t.foreground,
                            ))
                            .child(device.name.clone()),
                    )
                    .child(detail(
                        t,
                        format_paired_device_detail(&device, tab.now),
                        false,
                    ))
                    .into_any_element();
                let id: SharedString = device.id.clone().into();
                let spinner_element = |suffix: &str| {
                    removing.then(|| {
                        spinner(
                            SharedString::from(format!("remote-device-{id}-{suffix}")),
                            14.0,
                            t.muted,
                            900,
                        )
                    })
                };
                let value = if confirming {
                    let remove_id = device.id.clone();
                    h_flex()
                        .flex_shrink_0()
                        .items_center()
                        .gap(px(6.0))
                        .text_size(px(13.0))
                        .text_color(hsla(t.muted))
                        .child(detail(t, format!("Remove {}?", device.name), false))
                        .child(compact_button(
                            &t.p,
                            SharedString::from(format!("remote-device-{id}-confirm")),
                            "Remove",
                            spinner_element("confirm-spinner"),
                            Look::Destructive,
                            24.0,
                            14.0,
                            None,
                            removing,
                            None,
                            move |tab: &mut RemoteTab, _window, cx| {
                                tab.confirming_device = None;
                                tab.remove_paired_device(remove_id.clone(), cx);
                            },
                            cx,
                        ))
                        .child(compact_button(
                            &t.p,
                            SharedString::from(format!("remote-device-{id}-cancel")),
                            "Cancel",
                            None,
                            Look::Filled,
                            24.0,
                            14.0,
                            Some(t.muted),
                            false,
                            None,
                            |tab: &mut RemoteTab, _window, cx| {
                                tab.confirming_device = None;
                                cx.notify();
                            },
                            cx,
                        ))
                        .into_any_element()
                } else {
                    let confirm_id = device.id.clone();
                    h_flex()
                        .flex_shrink_0()
                        .items_center()
                        .gap(px(6.0))
                        .child(div().size(px(8.0)).rounded_full().bg(hsla(if connected {
                            t.success
                        } else {
                            css_fade(t.muted, 0.5)
                        })))
                        .child(compact_button(
                            &t.p,
                            SharedString::from(format!("remote-device-{id}-remove")),
                            "Remove",
                            spinner_element("remove-spinner"),
                            Look::Filled,
                            24.0,
                            14.0,
                            Some(t.muted),
                            !rpc || removing,
                            None,
                            move |tab: &mut RemoteTab, _window, cx| {
                                tab.confirming_device = Some(confirm_id.clone());
                                cx.notify();
                            },
                            cx,
                        ))
                        .into_any_element()
                };
                remote_row(main, Some(value))
            })
            .collect(),
    };
    let mut rows = rows;
    let pending: Vec<AnyElement> = tab
        .pending_devices
        .clone()
        .into_iter()
        .map(|device| pending_device_row(tab, t, rpc, device, cx))
        .collect();
    if !pending.is_empty() {
        // An empty-list placeholder makes no sense beside a phone that is pairing.
        if tab.paired_devices.as_ref().is_some_and(Vec::is_empty) {
            rows.clear();
        }
        rows.splice(0..0, pending);
    }
    rows_frame(t, rows)
}

/*
CDXC:RemotePairing 2026-11-08 DECISION:
User (GitHub #495, Q1721): a phone that scanned the QR is Pending until the owner compares three words shown on both screens and approves with Touch ID, Windows Hello or the system password. The row shows the words; Approve here (or on the approval card) runs Touch ID, Windows Hello or the system password and signs the approval (`owner_security`), so an agent driving the GUI can't click it through. Deny needs no approval: it only closes a door.
*/
fn pending_device_row(
    tab: &RemoteTab,
    t: &RemoteTokens,
    rpc: bool,
    device: PendingDevice,
    cx: &mut Context<RemoteTab>,
) -> AnyElement {
    // The same request reads the same everywhere on the computer: the device, its three words,
    // one sentence, Not my phone beside Approve (CDXC:RemotePairing 2026-11-10 in
    // window/connect_device_modal.rs).
    let phone = is_phone_platform(&device.platform);
    let glyph = if phone {
        ICON_DEVICE_MOBILE
    } else {
        ICON_DEVICE_LAPTOP
    };
    let kind_label = pending_kind_label(&device.platform);
    let removing = tab.removing_device.as_deref() == Some(device.id.as_str());
    let id: SharedString = device.id.clone().into();
    let deny_id = device.approval_id.clone();
    let approving = tab.approving_device.as_deref() == Some(device.approval_id.as_str());
    let approve_id = device.approval_id.clone();
    let approve = compact_button(
        &t.p,
        SharedString::from(format!("remote-pending-{id}-approve")),
        "Approve",
        approving.then(|| {
            spinner(
                SharedString::from(format!("remote-pending-{id}-approve-spinner")),
                14.0,
                t.muted,
                900,
            )
        }),
        Look::Filled,
        28.0,
        14.0,
        None,
        !rpc || tab.approving_device.is_some() || removing,
        None,
        move |tab: &mut RemoteTab, _window, cx| {
            tab.approve_pending_device(approve_id.clone(), cx);
        },
        cx,
    );
    let deny = compact_button(
        &t.p,
        SharedString::from(format!("remote-pending-{id}-deny")),
        pairing_deny_label(phone),
        removing.then(|| {
            spinner(
                SharedString::from(format!("remote-pending-{id}-spinner")),
                14.0,
                t.muted,
                900,
            )
        }),
        Look::Filled,
        28.0,
        14.0,
        Some(t.muted),
        !rpc || removing || tab.approving_device.is_some(),
        None,
        move |tab: &mut RemoteTab, _window, cx| {
            tab.deny_pending_device(deny_id.clone(), cx);
        },
        cx,
    );
    v_flex()
        .w_full()
        .min_w_0()
        .gap(px(10.0))
        .px(px(12.0))
        .py(px(12.0))
        .child(
            h_flex()
                .items_center()
                .gap(px(6.0))
                .text_size(px(14.0))
                .line_height(px(20.0))
                .text_color(hsla(t.foreground))
                .child(super::super::super::fields::settings_icon(
                    glyph,
                    15.0,
                    t.foreground,
                ))
                .child(device.name.clone())
                .child(detail(
                    t,
                    if device.over_ssh {
                        format!("· {kind_label} · signs in over SSH")
                    } else {
                        format!("· {kind_label}")
                    },
                    false,
                ))
                .child(div().flex_1())
                .child(detail(t, pairing_question(phone), false)),
        )
        .child(pairing_words(&t.p.modal, &device.words, true))
        .child(
            h_flex()
                .w_full()
                .min_w_0()
                .items_center()
                .gap(px(12.0))
                .child(div().flex_1().min_w_0().child(detail(
                    t,
                    pairing_words_sentence(phone),
                    true,
                )))
                .child(
                    h_flex()
                        .flex_shrink_0()
                        .gap(px(6.0))
                        .child(deny)
                        .child(approve),
                ),
        )
        .into_any_element()
}

/// What a waiting device says it is, in the user's words.
fn pending_kind_label(platform: &str) -> &'static str {
    match platform.trim().to_lowercase().as_str() {
        "ios" | "iphone" => "iPhone",
        "ipad" => "iPad",
        "android" => "Android",
        "mobile" | "phone" => "phone",
        _ => "computer",
    }
}
