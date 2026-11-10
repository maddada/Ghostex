//! "Phones and computers that can reach this machine": the top of Settings > Remote. Easy
//! Connect's switch, Connect a phone and Connect a computer (both open the Connect a phone window),
//! and every waiting and paired device, waiting first.
//!
//! CDXC:RemotePairing 2026-11-08 DECISION:
//! User (GitHub #495 security decision): an empty Easy Connect allow-list means closed, replacing the 2026-10-25 on/off switch (its "off" emptied the list, which let in anyone with the code). "Only paired devices can connect" is always so: since 2026-11-10 the card's line says it ("Only the devices in this list get in") instead of a box of its own. Every paired phone's key goes on the list when its pairing is approved; phones paired before 11.0.1 must pair again (the Pair again line above). New phones still get in while the Connect a phone window shows its code (CDXC:RemotePairing in server/src/tailcat/supervisor.rs).
use super::super::super::super::native_modal_kit::*;
use super::super::super::fields::{
    ButtonSize, ButtonVariant, settings_button_sized, settings_icon, switch_control,
};
use super::easy_connect::path_card;
use super::style::*;
use super::{RemoteTab, paired_devices};
use gpui::{
    AnyElement, Context, InteractiveElement as _, IntoElement, ParentElement as _, SharedString,
    Styled as _, div, px,
};
use gpui_component::{h_flex, v_flex};
use serde_json::json;

/// A card's head: its icon, its title and one line under it.
pub(super) fn section_head(
    t: &RemoteTokens,
    id: &'static str,
    icon_path: &'static str,
    title: impl Into<SharedString>,
    subtitle: impl Into<SharedString>,
) -> AnyElement {
    h_flex()
        .id(id)
        .flex_1()
        .min_w_0()
        .min_h(px(56.0))
        .px(px(12.0))
        .py(px(10.0))
        .gap(px(10.0))
        .items_center()
        .child(path_icon(t, icon_path, false))
        .child(
            v_flex()
                .flex_1()
                .min_w_0()
                .gap(px(2.0))
                .child(
                    div()
                        .text_size(px(14.0))
                        .line_height(px(20.0))
                        .text_color(hsla(t.foreground))
                        .child(title.into()),
                )
                .child(detail(t, subtitle.into(), true)),
        )
        .into_any_element()
}

/// The card's line under its title: whether Easy Connect is on, and what that means.
fn reach_line(tab: &RemoteTab) -> &'static str {
    let Some(status) = tab.easy_connect.as_ref() else {
        return "Checking Easy Connect…";
    };
    if status.installing || tab.install_pending {
        "Installing the Easy Connect helper…"
    } else if !status.binary_found {
        "The Easy Connect helper isn't installed yet."
    } else if status.enabled {
        "Easy Connect is on. Only the devices in this list get in."
    } else {
        "Easy Connect is off. Phones on Tailscale or your own network can still connect."
    }
}

const ICON_HELPER: &str = "modals/settings/plug-connected.svg";

/*
CDXC:RemotePairing 2026-11-11 DECISION:
GitHub #559: with the helper missing, this card said "Part of Easy Connect is missing on this computer. Reinstall Ghostex to get it back." and its switch was greyed out, and a reinstall can't bring the helper back (the Mac app doesn't carry it). The card now says the helper isn't installed yet, with Install, the install's step while it runs, a plain error with Try again when it fails, and what works meanwhile. Easy Connect starts by itself once the helper is in when it is turned on.
*/
/// The Easy Connect helper isn't installed: what it is and Install; while it installs, its step;
/// after a failed install, why and Try again. `None` once the helper is in.
fn helper_missing_block(
    tab: &RemoteTab,
    t: &RemoteTokens,
    rpc: bool,
    cx: &mut Context<RemoteTab>,
) -> Option<AnyElement> {
    let status = tab.easy_connect.as_ref()?;
    if status.binary_found {
        return None;
    }
    let installing = status.installing || tab.install_pending;
    if installing {
        let step = status
            .install_progress
            .clone()
            .unwrap_or_else(|| "Starting…".to_string());
        return Some(
            v_flex()
                .w_full()
                .gap(px(4.0))
                .child(
                    h_flex()
                        .w_full()
                        .min_w_0()
                        .items_center()
                        .gap(px(8.0))
                        .child(spinner("remote-helper-install-spinner", 14.0, t.muted, 900))
                        .child(
                            div()
                                .flex_1()
                                .min_w_0()
                                .text_size(px(13.0))
                                .line_height(px(18.57))
                                .text_color(hsla(t.foreground))
                                .child(step),
                        ),
                )
                .child(detail(
                    t,
                    "This takes a few minutes and keeps going if you close Settings.",
                    true,
                ))
                .into_any_element(),
        );
    }
    let failure = tab
        .install_request_error
        .clone()
        .or_else(|| status.install_error.clone());
    let failed = failure.is_some();
    let message = if failed {
        "The Easy Connect helper didn't install."
    } else {
        "The Easy Connect helper isn't installed yet. Phones reach this computer through it from anywhere; Ghostex builds it here once, in a few minutes."
    };
    Some(
        v_flex()
            .w_full()
            .gap(px(6.0))
            .child(
                h_flex()
                    .w_full()
                    .min_w_0()
                    .items_center()
                    .gap(px(8.0))
                    .child(settings_icon(
                        if failed { ICON_ALERT } else { ICON_HELPER },
                        14.0,
                        if failed { t.failed } else { t.foreground },
                    ))
                    .child(div().flex_1().min_w_0().child(detail(t, message, true)))
                    .child(compact_button(
                        &t.p,
                        "remote-helper-install",
                        if failed { "Try again" } else { "Install" },
                        None,
                        Look::Filled,
                        28.0,
                        14.0,
                        None,
                        !rpc,
                        None,
                        |tab: &mut RemoteTab, _window, cx| tab.install_easy_connect_helper(cx),
                        cx,
                    )),
            )
            .children(failure.map(|message| error_line(t, message)))
            .child(detail(
                t,
                "Meanwhile, phones on Tailscale or on your own network can still connect.",
                true,
            ))
            .into_any_element(),
    )
}

/// SSH access is off: a warning line with Turn on, since phones and computers sign in over SSH.
fn ssh_off_line(
    tab: &RemoteTab,
    t: &RemoteTokens,
    rpc: bool,
    cx: &mut Context<RemoteTab>,
) -> Option<AnyElement> {
    if tab.access.as_ref()?.ssh.enabled {
        return None;
    }
    let enabling = tab.enabling_ssh;
    let failed = tab
        .ssh_attempt
        .as_ref()
        .and_then(|attempt| attempt.message.clone());
    Some(
        v_flex()
            .w_full()
            .gap(px(6.0))
            .child(
                h_flex()
                    .w_full()
                    .min_w_0()
                    .items_center()
                    .gap(px(8.0))
                    .child(settings_icon(ICON_ALERT, 14.0, t.warn))
                    .child(
                        div()
                            .flex_1()
                            .min_w_0()
                            .child(detail(
                                t,
                                "SSH access is off, so your phone and other computers can't sign in to this machine.",
                                true,
                            )),
                    )
                    .child(compact_button(
                        &t.p,
                        "remote-reach-ssh-on",
                        if enabling { "Turning on…" } else { "Turn on" },
                        enabling.then(|| spinner("remote-reach-ssh-spinner", 14.0, t.muted, 900)),
                        Look::Filled,
                        28.0,
                        14.0,
                        None,
                        !rpc || enabling,
                        None,
                        |tab: &mut RemoteTab, _window, cx| tab.enable_ssh_access(cx),
                        cx,
                    )),
            )
            .children(failed.map(|message| error_line(t, message)))
            .into_any_element(),
    )
}

pub(super) fn reach_card(
    tab: &mut RemoteTab,
    t: &RemoteTokens,
    rpc: bool,
    cx: &mut Context<RemoteTab>,
) -> AnyElement {
    let status = tab.easy_connect.clone();
    let is_on = status.as_ref().is_some_and(|status| status.enabled);
    let binary_found = status.as_ref().is_some_and(|status| status.binary_found);
    let head = section_head(
        t,
        "remote-reach-head",
        ICON_DEVICE_MOBILE,
        "Phones and computers that can reach this machine",
        reach_line(tab),
    );
    let switch = switch_control(
        &t.p,
        "remote-easy-connect-switch",
        "Easy Connect",
        is_on,
        // A switch left on without the helper can still be turned off.
        !rpc || status.is_none() || (!binary_found && !is_on),
        None,
        |tab: &mut RemoteTab, checked, _window, cx| {
            tab.set_easy_connect_state(json!({ "enabled": checked, "kind": "setEnabled" }), cx);
        },
        cx,
    );
    let mut body = v_flex().w_full().min_w_0().gap(px(10.0));
    body = body.children(helper_missing_block(tab, t, rpc, cx));
    body = body.children(ssh_off_line(tab, t, rpc, cx));
    // Without the helper, the block above says what is wrong and what to do.
    let errors = [
        status
            .as_ref()
            .filter(|status| status.binary_found)
            .and_then(|status| status.last_error.clone()),
        tab.request_error.clone(),
    ];
    body = body.children(
        errors
            .into_iter()
            .flatten()
            .map(|error| error_line(t, error)),
    );
    body = body.child(
        h_flex()
            .gap(px(8.0))
            .child(settings_button_sized(
                &t.p,
                "remote-connect-phone",
                "Connect a Device",
                Some(ICON_QRCODE),
                ButtonVariant::Default,
                ButtonSize::Sm,
                !rpc,
                None,
                |tab: &mut RemoteTab, _window, cx| tab.open_connect_device("phone", cx),
                cx,
            ))
            .child(settings_button_sized(
                &t.p,
                "remote-connect-computer",
                "Connect a Computer",
                Some(ICON_DEVICE_DESKTOP),
                ButtonVariant::Outline,
                ButtonSize::Sm,
                !rpc,
                None,
                |tab: &mut RemoteTab, _window, cx| tab.open_connect_device("computer", cx),
                cx,
            )),
    );
    body = body.child(paired_devices::device_rows(tab, t, rpc, cx));
    path_card(
        t,
        "remote-reach-card",
        head,
        switch,
        Some(body.into_any_element()),
    )
}
