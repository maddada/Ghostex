//! The Advanced collapsible at the bottom of Settings > Remote (remote-advanced.tsx (deleted 2026-10-01)).
//!
//! (Since 2026-10-24 it is the page's Advanced tab, always open; see CDXC:RemotePairing 2026-10-24 in mod.rs.)
//!
//! CDXC:RemotePairing 2026-09-03:
//! One collapsible for the controls a regular user never needs: the ports Easy Connect serves, the raw allowed client key list (Paired devices above is its friendly face), the bare pairing address for pasting by hand, the binary, the local gxserver endpoint, and the raw sidecar status for bug reports.
use super::super::super::super::native_modal_kit::*;
use super::super::super::fields::{
    FieldStates, full_value_tooltip, icon, settings_textarea, short_id, short_path, short_version,
};
use super::RemoteTab;
use super::easy_connect::{path_card, path_head};
use super::model::*;
use super::paired_devices::{remote_row, rows_frame};
use super::style::*;
use super::tailscale::copy_icon_button;
use gpui::prelude::FluentBuilder as _;
use gpui::{
    AnyElement, AppContext as _, Context, Entity, InteractiveElement as _, IntoElement,
    ParentElement as _, SharedString, StatefulInteractiveElement as _, Styled as _, Subscription,
    Window, div, px,
};
use gpui_component::input::{InputEvent, InputState};
use gpui_component::{h_flex, v_flex};
use serde_json::json;

#[derive(Default)]
pub(crate) struct AdvancedState {
    pub(super) raw_open: bool,
    keys_open: bool,
    /// The ports text while it differs from the status (`portsDraft`).
    ports_draft: Option<String>,
    ports_error: Option<String>,
    /// The key list text while it is being edited (`allowedKeysDraft`).
    keys_draft: Option<String>,
    keys_blur_subscribed: bool,
    ports_input: Option<Entity<InputState>>,
    subscriptions: Vec<Subscription>,
}

impl AdvancedState {
    /// The ports or allowed keys text is being edited and not saved yet.
    pub(super) fn has_draft(&self) -> bool {
        self.ports_draft.is_some() || self.keys_draft.is_some()
    }
}

/// `commitPorts`: parse, report a bad list, or save a changed one.
fn commit_ports(tab: &mut RemoteTab, cx: &mut Context<RemoteTab>) {
    let Some(draft) = tab.advanced.ports_draft.clone() else {
        return;
    };
    let Some(ports) = parse_easy_connect_ports_input(&draft) else {
        tab.advanced.ports_error = Some(format!(
            "Ports must be numbers between {EASY_CONNECT_MIN_PORT} and {EASY_CONNECT_MAX_PORT}, separated by commas."
        ));
        cx.notify();
        return;
    };
    tab.advanced.ports_error = None;
    tab.advanced.ports_draft = None;
    if let Some(status) = &tab.easy_connect
        && format_easy_connect_ports(&status.ports) == format_easy_connect_ports(&ports)
    {
        cx.notify();
        return;
    }
    tab.set_easy_connect_state(json!({ "kind": "setPorts", "ports": ports }), cx);
}

/// `commitAllowedClientKeys` on blur.
fn commit_allowed_keys(tab: &mut RemoteTab, cx: &mut Context<RemoteTab>) {
    let Some(draft) = tab.advanced.keys_draft.take() else {
        return;
    };
    let keys = parse_easy_connect_allowed_client_keys(&draft);
    if let Some(status) = &tab.easy_connect
        && status.allowed_client_keys.join("\n") == keys.join("\n")
    {
        cx.notify();
        return;
    }
    tab.set_easy_connect_state(
        json!({ "allowedClientKeys": keys, "kind": "setAllowedClientKeys" }),
        cx,
    );
}

fn ports_input(
    tab: &mut RemoteTab,
    value: &str,
    placeholder: &str,
    window: &mut Window,
    cx: &mut Context<RemoteTab>,
) -> Entity<InputState> {
    if let Some(input) = tab.advanced.ports_input.clone() {
        if tab.advanced.ports_draft.is_none() && input.read(cx).value().as_ref() != value {
            let value = value.to_string();
            input.update(cx, |input, cx| input.set_value(value, window, cx));
        }
        return input;
    }
    let placeholder = placeholder.to_string();
    let initial = value.to_string();
    let input = cx.new(|cx| {
        InputState::new(window, cx)
            .placeholder(placeholder)
            .default_value(initial)
    });
    let subscription = cx.subscribe_in(
        &input,
        window,
        |tab: &mut RemoteTab, input, event: &InputEvent, _window, cx| match event {
            InputEvent::Change => {
                let text = input.read(cx).value().to_string();
                let shown = tab
                    .easy_connect
                    .as_ref()
                    .map(|status| format_easy_connect_ports(&status.ports))
                    .unwrap_or_default();
                if tab.advanced.ports_draft.is_some() || text != shown {
                    tab.advanced.ports_draft = Some(text);
                    tab.advanced.ports_error = None;
                    cx.notify();
                }
            }
            InputEvent::PressEnter { .. } | InputEvent::Blur => commit_ports(tab, cx),
            _ => {}
        },
    );
    tab.advanced.subscriptions.push(subscription);
    tab.advanced.ports_input = Some(input.clone());
    input
}

/// The main text of an Advanced row: a 14px title over an optional one-line detail.
fn row_main(
    t: &RemoteTokens,
    title: &'static str,
    detail_text: Option<&'static str>,
) -> AnyElement {
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
                .child(title),
        )
        .children(detail_text.map(|text| detail(t, text, false)))
        .into_any_element()
}

/// `.settings-remote-row-value`: the right side, 13px muted, at most 60% wide.
fn row_value(t: &RemoteTokens, mono: bool) -> gpui::Div {
    h_flex()
        .flex_shrink_0()
        .max_w(gpui::relative(0.6))
        .min_w_0()
        .items_center()
        .gap(px(6.0))
        .text_size(px(13.0))
        .line_height(px(18.57))
        .text_color(hsla(t.muted))
        .when(mono, |this| this.font_family(MODAL_MONO_FONT))
}

/// The Advanced tab: the same path card as the other tabs, always open.
pub(super) fn advanced_card(
    tab: &mut RemoteTab,
    t: &RemoteTokens,
    window: &mut Window,
    cx: &mut Context<RemoteTab>,
) -> AnyElement {
    let head = path_head(
        t,
        "remote-advanced-head",
        icon::INFO_CIRCLE,
        false,
        "Advanced",
        Vec::new(),
    );
    let body = v_flex()
        .w_full()
        .min_w_0()
        .gap(px(10.0))
        .child(
            div()
                .w_full()
                .text_size(px(13.0))
                .line_height(px(19.5))
                .text_color(hsla(t.muted))
                .child(
                    "Easy Connect ports and keys, gxserver, and the raw status for bug reports.",
                ),
        )
        .child(advanced_rows(tab, t, window, cx))
        .into_any_element();
    path_card(
        t,
        "remote-advanced-card",
        head,
        div().into_any_element(),
        Some(body),
    )
}

fn advanced_rows(
    tab: &mut RemoteTab,
    t: &RemoteTokens,
    window: &mut Window,
    cx: &mut Context<RemoteTab>,
) -> AnyElement {
    let rpc = tab.rpc_available(cx);
    let status = tab.easy_connect.clone();
    let api_port = tab
        .pairing
        .as_ref()
        .and_then(|pairing| pairing.easy_connect.as_ref())
        .and_then(|code| code.port)
        .unwrap_or(GXSERVER_LOCAL_API_PORT);
    let ports_value = tab.advanced.ports_draft.clone().unwrap_or_else(|| {
        format_easy_connect_ports(
            status
                .as_ref()
                .map(|status| status.ports.as_slice())
                .unwrap_or(&[]),
        )
    });
    let fields_disabled = !rpc || status.is_none();
    let input = ports_input(tab, &ports_value, &api_port.to_string(), window, cx);
    let mut rows: Vec<AnyElement> = Vec::new();
    rows.push(remote_row(
        row_main(
            t,
            "Easy Connect served ports",
            Some("Comma-separated local ports exposed to paired phones."),
        ),
        Some(remote_input(
            &t.p,
            &input,
            32.0,
            12.0,
            Some(172.0),
            false,
            fields_disabled,
            window,
            cx,
        )),
    ));
    if let Some(error) = tab.advanced.ports_error.clone() {
        rows.push(
            div()
                .w_full()
                .px(px(12.0))
                .pt(px(4.0))
                .pb(px(8.0))
                .text_size(px(13.0))
                .line_height(px(18.85))
                .text_color(hsla(t.failed))
                .child(error)
                .into_any_element(),
        );
    }
    let keys_open = tab.advanced.keys_open;
    rows.push(remote_row(
        row_main(
            t,
            "Allowed client keys",
            Some("Empty lets no device in; paired phones are added by themselves and listed on the Easy Connect tab. Add a computer that connects with Easy Connect here."),
        ),
        Some(compact_button(
            &t.p,
            "remote-advanced-keys-toggle",
            if keys_open { "Hide list" } else { "Edit list" },
            None,
            Look::Filled,
            24.0,
            14.0,
            None,
            false,
            None,
            |tab: &mut RemoteTab, _window, cx| {
                tab.advanced.keys_open = !tab.advanced.keys_open;
                cx.notify();
            },
            cx,
        )),
    ));
    if keys_open {
        let keys_value = tab.advanced.keys_draft.clone().unwrap_or_else(|| {
            status
                .as_ref()
                .map(|status| status.allowed_client_keys.join("\n"))
                .unwrap_or_default()
        });
        let id = SharedString::from("remote-allowed-keys");
        let textarea = FieldStates::textarea_state(
            tab,
            &id,
            &keys_value,
            None,
            (4, 12),
            |tab: &mut RemoteTab, text, _window, _cx| {
                let shown = tab
                    .easy_connect
                    .as_ref()
                    .map(|status| status.allowed_client_keys.join("\n"))
                    .unwrap_or_default();
                if tab.advanced.keys_draft.is_some() || text != shown {
                    tab.advanced.keys_draft = Some(text);
                }
            },
            window,
            cx,
        );
        if !tab.advanced.keys_blur_subscribed {
            tab.advanced.keys_blur_subscribed = true;
            let blur = cx.subscribe(
                &textarea,
                |tab: &mut RemoteTab, _, event: &InputEvent, cx| {
                    if matches!(event, InputEvent::Blur) {
                        commit_allowed_keys(tab, cx);
                    }
                },
            );
            tab.advanced.subscriptions.push(blur);
        }
        rows.push(
            v_flex()
                .w_full()
                .px(px(12.0))
                .py(px(8.0))
                .gap(px(8.0))
                .child(detail(t, "One client key per line.", false))
                .child(div().font_family(MODAL_MONO_FONT).child(settings_textarea(
                    &t.p,
                    &textarea,
                    64.0,
                    true,
                    fields_disabled,
                    window,
                    cx,
                )))
                .into_any_element(),
        );
    }
    let token = status.as_ref().and_then(|status| status.token.clone());
    let pairing_value = match token {
        // Shown cut to its start; the whole address is on hover and copies (long_value.rs).
        Some(token) => row_value(t, true)
            .child(full_value_tooltip(
                div()
                    .id("remote-pairing-address")
                    .max_w(px(220.0))
                    .min_w_0()
                    .overflow_hidden()
                    .whitespace_nowrap()
                    .text_ellipsis()
                    .px(px(8.0))
                    .py(px(4.0))
                    .rounded(px(MODAL_RADIUS_CONTROL))
                    .border_1()
                    .border_color(hsla(t.edge(0.72)))
                    .bg(hsla(css_fade(t.background, 0.44)))
                    .text_color(hsla(css_mix(t.foreground, 0.88, t.muted)))
                    .child(short_id(&token)),
                token.clone(),
            ))
            .child(copy_icon_button(
                tab,
                t,
                "remote-copy-pairing-address".into(),
                "Copy the pairing address".to_string(),
                token,
                t.muted,
                cx,
            ))
            .into_any_element(),
        None => row_value(t, false)
            .child(if status.as_ref().is_some_and(|status| status.enabled) {
                "Not published yet"
            } else {
                "Turn on Easy Connect"
            })
            .into_any_element(),
    };
    rows.push(remote_row(
        row_main(
            t,
            "Pairing address",
            Some("The raw address inside the QR, for pasting into the app by hand."),
        ),
        Some(pairing_value),
    ));
    // Its path cut to the last two folders and its version number; both whole on hover.
    let binary = status
        .as_ref()
        .filter(|status| status.binary_found)
        .and_then(|status| {
            status.binary_path.clone().map(|path| {
                let full = match &status.binary_version {
                    Some(version) => format!("{path} ({version})"),
                    None => path.clone(),
                };
                let version = status.binary_version.as_deref().and_then(short_version);
                let short = match version {
                    Some(version) => format!("{} ({version})", short_path(&path)),
                    None => short_path(&path),
                };
                (short, full)
            })
        });
    let binary_value = match binary {
        Some((short, full)) => row_value(t, true)
            .child(full_value_tooltip(
                div().id("remote-easy-connect-binary").child(short),
                full,
            ))
            .into_any_element(),
        None => row_value(t, true).child("Not found").into_any_element(),
    };
    rows.push(remote_row(
        row_main(t, "Easy Connect binary", None),
        Some(binary_value),
    ));
    rows.push(remote_row(
        row_main(t, "gxserver", Some("Local API the app and phones talk to.")),
        Some(
            row_value(t, true)
                .child(format!(
                    "127.0.0.1:{api_port} · {}",
                    if rpc && status.is_some() {
                        "running"
                    } else {
                        "unreachable"
                    }
                ))
                .into_any_element(),
        ),
    ));
    let raw_open = tab.advanced.raw_open;
    rows.push(remote_row(
        row_main(t, "Raw Easy Connect status", None),
        Some(compact_button(
            &t.p,
            "remote-advanced-raw-toggle",
            if raw_open { "Hide JSON" } else { "Show JSON" },
            None,
            Look::Filled,
            24.0,
            14.0,
            None,
            status.is_none(),
            None,
            |tab: &mut RemoteTab, _window, cx| {
                tab.advanced.raw_open = !tab.advanced.raw_open;
                cx.notify();
            },
            cx,
        )),
    ));
    if raw_open && let Some(status) = &status {
        let json = serde_json::to_string_pretty(&status.raw).unwrap_or_default();
        rows.push(
            div()
                .id("remote-advanced-raw-status")
                .w_full()
                .max_h(px(260.0))
                .overflow_y_scroll()
                .px(px(12.0))
                .py(px(10.0))
                .font_family(MODAL_MONO_FONT)
                .text_size(px(13.0))
                .line_height(px(18.57))
                .text_color(hsla(css_mix(t.foreground, 0.88, t.muted)))
                .children(json.lines().map(|line| {
                    div().whitespace_nowrap().child(if line.is_empty() {
                        " ".to_string()
                    } else {
                        line.to_string()
                    })
                }))
                .into_any_element(),
        );
    }
    rows_frame(t, rows)
}
