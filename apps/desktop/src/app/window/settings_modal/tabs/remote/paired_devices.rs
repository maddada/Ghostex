//! The paired device list (remote-paired-devices.tsx (deleted 2026-10-01)).
//!
//! CDXC:RemotePairing 2026-09-03:
//! The friendly face of the pairing registry: one row per device with its platform glyph, when it paired, and whether it checked in within the last three minutes. Remove asks for confirmation inline, naming the device, and then posts `/api/removePairedDevice`, which also drops the device's SSH key.
use super::super::super::super::native_modal_kit::*;
use super::super::super::fields::switch_control;
use super::RemoteTab;
use super::model::{
    format_paired_device_detail, is_paired_device_connected_now, is_phone_platform,
};
use super::style::*;
use gpui::prelude::FluentBuilder as _;
use gpui::{
    AnyElement, Context, IntoElement, ParentElement as _, SharedString, Styled as _, div, px,
};
use gpui_component::{h_flex, v_flex};
use serde_json::json;

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
        .child(div().min_w_0().child(main))
        .children(value)
        .into_any_element()
}

pub(super) fn paired_devices_list(
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
                "No devices yet. Scan the code with the Ghostex app and it appears here.",
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
    let paired_only = paired_only_row(tab, t, rpc, cx);
    v_flex()
        .w_full()
        .min_w_0()
        .gap(px(6.0))
        .child(section_label(t, "Paired devices"))
        .child(rows_frame(t, rows))
        .child(rows_frame(t, vec![paired_only]))
        .into_any_element()
}

/*
CDXC:RemotePairing 2026-10-25 DECISION:
User (security audit, Q4 "per-device tokens and a tunnel allow-list"): "Only paired devices can connect" is the friendly face of the allow-list. Turning it on lists every paired phone's Easy Connect key (plus any key already on the list), so a phone removed above can no longer reach this computer at all; turning it off empties the list, which lets in anyone with the code again. It is off until the user turns it on, because computers that connect with Easy Connect, and phones paired by an older Ghostex app, have no listed key and would be locked out: the row names those phones, and the Connect a computer panel says how to add a computer's key. New phones still pair while the QR is showing (CDXC:RemotePairing 2026-10-25 in server/src/tailcat/supervisor.rs).
*/
fn paired_only_row(
    tab: &RemoteTab,
    t: &RemoteTokens,
    rpc: bool,
    cx: &mut Context<RemoteTab>,
) -> AnyElement {
    let listed = tab
        .easy_connect
        .as_ref()
        .map(|status| status.allowed_client_keys.clone())
        .unwrap_or_default();
    let devices = tab.paired_devices.clone().unwrap_or_default();
    let device_keys: Vec<String> = devices
        .iter()
        .filter_map(|device| device.tailcat_client_key.clone())
        .collect();
    let without_key: Vec<String> = devices
        .iter()
        .filter(|device| device.tailcat_client_key.is_none())
        .map(|device| device.name.clone())
        .collect();
    let on = !listed.is_empty();
    let can_turn_on = !device_keys.is_empty();
    let detail_text: SharedString = if !without_key.is_empty() {
        format!(
            "{} paired with an older Ghostex app and {} not listed; pair {} again to keep {} in.",
            without_key.join(", "),
            if without_key.len() == 1 { "is" } else { "are" },
            if without_key.len() == 1 { "it" } else { "them" },
            if without_key.len() == 1 { "it" } else { "them" },
        )
        .into()
    } else if !on && !can_turn_on {
        "Pair a phone first. Until this is on, anyone with this computer's Easy Connect code can reach its sign-in.".into()
    } else {
        "A removed phone can no longer reach this computer. Computers that connect with Easy Connect need their key added under Advanced.".into()
    };
    let switch = switch_control(
        &t.p,
        "remote-paired-only-switch",
        "Only paired devices can connect",
        on,
        !rpc || tab.easy_connect.is_none() || (!on && !can_turn_on),
        None,
        move |tab: &mut RemoteTab, checked, _window, cx| {
            let keys: Vec<String> = if checked {
                let mut keys = listed.clone();
                for key in &device_keys {
                    if !keys.contains(key) {
                        keys.push(key.clone());
                    }
                }
                keys
            } else {
                Vec::new()
            };
            tab.set_easy_connect_state(
                json!({ "allowedClientKeys": keys, "kind": "setAllowedClientKeys" }),
                cx,
            );
        },
        cx,
    );
    let main = v_flex()
        .min_w_0()
        .gap(px(3.0))
        .child(
            div()
                .text_size(px(14.0))
                .line_height(px(20.0))
                .text_color(hsla(t.foreground))
                .child("Only paired devices can connect"),
        )
        .child(detail(t, detail_text, true))
        .into_any_element();
    remote_row(main, Some(switch))
}
