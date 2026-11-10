//! Settings > Remote > Easy Connect's Relay row: Ghostex's relay, the public relays only, or a
//! relay the user runs.
//!
//! CDXC:RemotePairing 2026-11-06 DECISION:
//! User (Q1490 B, approved plan "Setting"): one Easy Connect relay choice, Ghostex relay (the default) or public relays only, plus an optional custom hostname. The choice only adds, replaces or drops the second address; the first one, which every phone paired so far holds, never changes, so the row says phones keep working whichever is picked.
//! SEE-ALSO: server/src/tailcat/types.rs (`TailcatRelaySetting`), server/src/tailcat/supervisor.rs.
use super::super::super::super::native_modal_kit::*;
use super::RemoteTab;
use super::model::TailcatRelay;
use super::paired_devices::{remote_row, rows_frame};
use super::style::*;
use gpui::{
    AnyElement, AppContext as _, Context, Entity, IntoElement, ParentElement as _, Styled as _,
    Subscription, Window, div, px,
};
use gpui_component::input::{InputEvent, InputState};
use gpui_component::v_flex;
use serde_json::json;

const MODES: [&str; 3] = ["ghostex", "public", "custom"];

/// What each option's (i) says, in the order of [`MODES`].
const RELAY_INFO: [&str; 3] = [
    "Phones try Ghostex's own relay (relay-eu.ghostex.dev, in the EU) first and Tailscale's public relays second. Nothing to set up, so it is the right pick for most people. If Ghostex's relay is not answering, phones use the public relays.",
    "Phones use only Tailscale's public relays. Ghostex's relay is not involved, so pick this to keep connections off a server Ghostex runs. Phones paired earlier keep working.",
    "Phones try a relay you run yourself first, then Tailscale's public relays. You need a Tailscale relay (derper) reachable at a hostname such as relay.example.com; enter that hostname in the field that appears below.",
];

#[derive(Default)]
pub(crate) struct RelayState {
    /// Custom was picked and no hostname is saved yet, so only the field shows until one is entered.
    custom_open: bool,
    host_input: Option<Entity<InputState>>,
    /// The hostname text while it differs from the saved one.
    host_draft: Option<String>,
    subscriptions: Vec<Subscription>,
}

fn send_relay(tab: &mut RemoteTab, mode: &str, custom_host: &str, cx: &mut Context<RemoteTab>) {
    tab.set_easy_connect_state(
        json!({ "kind": "setRelay", "mode": mode, "customHost": custom_host }),
        cx,
    );
}

fn select_mode(tab: &mut RemoteTab, index: usize, cx: &mut Context<RemoteTab>) {
    let Some(mode) = MODES.get(index).copied() else {
        return;
    };
    let saved_host = tab
        .easy_connect
        .as_ref()
        .map(|status| status.relay.custom_host.trim().to_string())
        .unwrap_or_default();
    if mode == "custom" && saved_host.is_empty() {
        tab.relay.custom_open = true;
        cx.notify();
        return;
    }
    tab.relay.custom_open = false;
    send_relay(tab, mode, "", cx);
}

fn commit_host(tab: &mut RemoteTab, cx: &mut Context<RemoteTab>) {
    let Some(draft) = tab.relay.host_draft.take() else {
        return;
    };
    let host = draft.trim().to_string();
    if host.is_empty() {
        cx.notify();
        return;
    }
    tab.relay.custom_open = false;
    send_relay(tab, "custom", &host, cx);
}

fn host_input(
    tab: &mut RemoteTab,
    value: &str,
    window: &mut Window,
    cx: &mut Context<RemoteTab>,
) -> Entity<InputState> {
    if let Some(input) = tab.relay.host_input.clone() {
        if tab.relay.host_draft.is_none() && input.read(cx).value().as_ref() != value {
            let value = value.to_string();
            input.update(cx, |input, cx| input.set_value(value, window, cx));
        }
        return input;
    }
    let initial = value.to_string();
    let input = cx.new(|cx| {
        InputState::new(window, cx)
            .placeholder("relay.example.com")
            .default_value(initial)
    });
    let subscription = cx.subscribe_in(
        &input,
        window,
        |tab: &mut RemoteTab, input, event: &InputEvent, _window, cx| match event {
            InputEvent::Change => {
                let text = input.read(cx).value().to_string();
                let saved = tab
                    .easy_connect
                    .as_ref()
                    .map(|status| status.relay.custom_host.clone())
                    .unwrap_or_default();
                if tab.relay.host_draft.is_some() || text != saved {
                    tab.relay.host_draft = Some(text);
                }
            }
            InputEvent::PressEnter { .. } | InputEvent::Blur => commit_host(tab, cx),
            _ => {}
        },
    );
    tab.relay.subscriptions.push(subscription);
    tab.relay.host_input = Some(input.clone());
    input
}

/// What the row says about where phones meet this computer.
fn relay_detail(relay: &TailcatRelay, custom_open: bool) -> String {
    let name = |host: &str| {
        if relay.mode == "ghostex" {
            "Ghostex's relay".to_string()
        } else {
            host.to_string()
        }
    };
    if custom_open {
        return "Enter the hostname of a relay you run (Tailscale's derper). Phones fall back to Tailscale's public relays.".to_string();
    }
    match (relay.mode.as_str(), relay.host.as_deref()) {
        ("public", _) | (_, None) => "Phones use Tailscale's public relays only.".to_string(),
        (_, Some(host)) if relay.reachable == Some(false) => format!(
            "{} isn't answering, so phones use Tailscale's public relays for now.",
            name(host)
        ),
        (_, Some(host)) => format!(
            "Phones reach this computer through {} first, then Tailscale's public relays. Phones paired earlier keep working.",
            name(host)
        ),
    }
}

/// The Relay row, at the top of Advanced.
pub(super) fn relay_section(
    tab: &mut RemoteTab,
    t: &RemoteTokens,
    rpc: bool,
    window: &mut Window,
    cx: &mut Context<RemoteTab>,
) -> AnyElement {
    let relay = tab
        .easy_connect
        .as_ref()
        .map(|status| status.relay.clone())
        .unwrap_or_default();
    let custom_open = tab.relay.custom_open;
    let selected = if custom_open {
        2
    } else {
        MODES
            .iter()
            .position(|mode| *mode == relay.mode)
            .unwrap_or(0)
    };
    let items = [
        ModalSegmentedItem {
            icon: None,
            label: "Ghostex relay",
        },
        ModalSegmentedItem {
            icon: None,
            label: "Public relays only",
        },
        ModalSegmentedItem {
            icon: None,
            label: "Custom",
        },
    ];
    // CDXC:RemotePairing 2026-11-09 DECISION:
    // User, of the Relay picker: "also need an i icon i can hover next to each one's name to explain the different options". Each option carries an (i) right after its label, inside the segment: three segments fit a row at the Settings width, so no second row is needed. Pressing the (i) is shielded, so it explains without changing the selection.
    let extras = RELAY_INFO
        .iter()
        .enumerate()
        .map(|(index, text)| {
            Some(click_shield(info_icon(
                t,
                ("remote-relay-info", index),
                *text,
            )))
        })
        .collect();
    let control = modal_segmented_control_with_extras(
        &t.p.modal,
        "remote-easy-connect-relay",
        &items,
        extras,
        selected,
        |tab: &mut RemoteTab, index, _window, cx| select_mode(tab, index, cx),
        cx,
    );
    let main = v_flex()
        .w_full()
        .min_w_0()
        .gap(px(6.0))
        .child(
            div()
                .text_size(px(14.0))
                .line_height(px(20.0))
                .text_color(hsla(t.foreground))
                .child("Relay"),
        )
        .child(detail(t, relay_detail(&relay, custom_open), true))
        .child(
            div()
                .w_full()
                .opacity(if rpc { 1.0 } else { 0.5 })
                .child(control),
        )
        .into_any_element();
    let mut rows = vec![remote_row(main, None)];
    if custom_open || selected == 2 {
        let value = tab
            .relay
            .host_draft
            .clone()
            .unwrap_or_else(|| relay.custom_host.clone());
        let input = host_input(tab, &value, window, cx);
        rows.push(remote_row(
            v_flex()
                .min_w_0()
                .child(
                    div()
                        .text_size(px(14.0))
                        .line_height(px(20.0))
                        .text_color(hsla(t.foreground))
                        .child("Relay hostname"),
                )
                .into_any_element(),
            Some(remote_input(
                &t.p,
                &input,
                32.0,
                12.0,
                Some(220.0),
                true,
                !rpc,
                window,
                cx,
            )),
        ));
    }
    let mut section = v_flex()
        .w_full()
        .min_w_0()
        .gap(px(6.0))
        .child(rows_frame(t, rows));
    if let Some(error) = relay.last_error.clone() {
        section = section.child(error_line(t, error));
    }
    section.into_any_element()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn relay(mode: &str, host: Option<&str>, reachable: Option<bool>) -> TailcatRelay {
        TailcatRelay {
            mode: mode.to_string(),
            host: host.map(str::to_string),
            reachable,
            running: host.is_some(),
            ..TailcatRelay::default()
        }
    }

    #[test]
    fn the_row_says_which_relay_phones_meet_on() {
        assert!(
            relay_detail(
                &relay("ghostex", Some("relay-eu.ghostex.dev"), Some(true)),
                false
            )
            .starts_with("Phones reach this computer through Ghostex's relay first")
        );
        assert_eq!(
            relay_detail(
                &relay("ghostex", Some("relay-eu.ghostex.dev"), Some(false)),
                false
            ),
            "Ghostex's relay isn't answering, so phones use Tailscale's public relays for now."
        );
        assert_eq!(
            relay_detail(&relay("public", None, None), false),
            "Phones use Tailscale's public relays only."
        );
        assert!(
            relay_detail(&relay("custom", Some("relay.example.com"), None), false)
                .contains("through relay.example.com first")
        );
        assert!(
            relay_detail(&relay("ghostex", None, None), true).starts_with("Enter the hostname")
        );
    }
}
