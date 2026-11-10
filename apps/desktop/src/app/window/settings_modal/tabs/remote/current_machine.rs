//! "This computer": what belongs to the computer you're on (its name, its short name, its SSH
//! access), behind its row on the Remote page, and the one-line SSH status the Tailscale steps show
//! instead of their own SSH controls.
//!
//! The 2026-11-08 "Current machine" card above the tabs (Q1970 to Q1973) became this row on
//! 2026-11-10 (CDXC:RemotePairing 2026-11-10 in mod.rs). What it holds stays as the user decided
//! then: the name read-only from the OS, the short name, SSH access; the Tailscale steps show "SSH
//! access is on", or "SSH access is off" with a link that opens this row and highlights its SSH
//! access row.
use super::super::super::super::native_modal_kit::*;
use super::super::super::fields::settings_icon;
use super::RemoteTab;
use super::paired_devices::remote_row;
use super::style::*;
use gpui::prelude::FluentBuilder as _;
use gpui::{
    AnyElement, AppContext as _, Context, InteractiveElement as _, IntoElement, ParentElement as _,
    SharedString, StatefulInteractiveElement as _, Styled as _, Window, div, px,
};
use gpui_component::input::{InputEvent, InputState};
use gpui_component::{h_flex, v_flex};
use serde_json::Value;
use std::time::Duration;

/// How long the SSH access row stays highlighted after a setup step's link brought you to it.
pub(super) const SSH_HIGHLIGHT: Duration = Duration::from_millis(2000);

/// A row's title and its one-line detail.
fn row_main(t: &RemoteTokens, title: &'static str, detail_text: &'static str) -> AnyElement {
    v_flex()
        .min_w_0()
        .gap(px(3.0))
        .child(
            div()
                .text_size(px(14.0))
                .line_height(px(20.0))
                .text_color(hsla(t.foreground))
                .child(title),
        )
        .child(detail(t, detail_text, true))
        .into_any_element()
}

/// The muted value at a row's right end.
fn row_value(t: &RemoteTokens) -> gpui::Div {
    h_flex()
        .flex_shrink_0()
        .max_w(gpui::relative(0.5))
        .min_w_0()
        .items_center()
        .gap(px(6.0))
        .text_size(px(13.0))
        .line_height(px(18.57))
        .text_color(hsla(t.muted))
}

/// The computer's name: the one gxserver reports (the one phones and computers pair with), else
/// this app's.
fn computer_name(tab: &RemoteTab) -> String {
    tab.access
        .as_ref()
        .map(|access| access.computer_name.clone())
        .filter(|name| !name.trim().is_empty())
        .unwrap_or_else(crate::host_name::local_computer_name)
}

/// The row's one line: "Mohamad's Laptop · short name MM · SSH access on".
pub(super) fn this_computer_summary(tab: &RemoteTab, cx: &gpui::App) -> String {
    let mut settings = serde_json::Map::new();
    settings.insert(
        "remoteMachines".to_string(),
        Value::Array(tab.remote_machines(cx)),
    );
    settings.insert(
        "localMachineShortName".to_string(),
        tab.store.read(cx).value("localMachineShortName"),
    );
    let short = ghostex_gx_core::machine_short_names::short_names_by_machine_id(
        &crate::host_name::local_computer_name(),
        &settings,
    )
    .remove(ghostex_gx_core::LOCAL_MACHINE_ID)
    .unwrap_or_default();
    let mut parts = vec![computer_name(tab)];
    if !short.is_empty() {
        parts.push(format!("short name {short}"));
    }
    match tab.access.as_ref().map(|access| access.ssh.enabled) {
        Some(true) => parts.push("SSH access on".to_string()),
        Some(false) => parts.push("SSH access off".to_string()),
        None => {}
    }
    parts.join(" · ")
}

/// The rows behind "This computer": its name, its short name and SSH access, under hairlines.
pub(super) fn this_computer_rows(
    tab: &mut RemoteTab,
    t: &RemoteTokens,
    window: &mut Window,
    cx: &mut Context<RemoteTab>,
) -> Vec<AnyElement> {
    let name = computer_name(tab);
    let name_row = remote_row(
        row_main(
            t,
            "Name",
            "Set by the system. Your phone and other computers show this name.",
        ),
        Some(
            row_value(t)
                .child(
                    div()
                        .min_w_0()
                        .overflow_hidden()
                        .whitespace_nowrap()
                        .text_ellipsis()
                        .child(name),
                )
                .into_any_element(),
        ),
    );
    let short_row = short_name_row(tab, t, &crate::host_name::local_computer_name(), window, cx);
    let ssh_row = ssh_row(tab, t, window, cx);
    vec![name_row, short_row, ssh_row]
}

/// This computer's short name (`localMachineShortName`); empty uses the one made from the name,
/// shown as the placeholder (gx-core machine_short_names.rs). It is per computer, like the machine
/// list, so Settings sync does not carry it.
fn short_name_row(
    tab: &mut RemoteTab,
    t: &RemoteTokens,
    local_name: &str,
    window: &mut Window,
    cx: &mut Context<RemoteTab>,
) -> AnyElement {
    if tab.local_short_name.is_none() {
        // This computer's made short name, with the saved machines' names left out of the
        // clash check the way the sidebar makes it.
        let mut settings = serde_json::Map::new();
        settings.insert(
            "remoteMachines".to_string(),
            Value::Array(tab.remote_machines(cx)),
        );
        let made =
            ghostex_gx_core::machine_short_names::short_names_by_machine_id(local_name, &settings)
                .remove(ghostex_gx_core::LOCAL_MACHINE_ID)
                .unwrap_or_default();
        let saved = tab
            .store
            .read(cx)
            .value("localMachineShortName")
            .as_str()
            .unwrap_or_default()
            .to_string();
        let input = cx.new(|cx| {
            InputState::new(window, cx)
                .placeholder(made)
                .default_value(saved)
        });
        let subscription = cx.subscribe_in(
            &input,
            window,
            |tab: &mut RemoteTab, input, event: &InputEvent, window, cx| {
                if !matches!(event, InputEvent::Change) {
                    return;
                }
                let text = input.read(cx).value().to_string();
                let short = ghostex_gx_core::machine_short_names::clean_machine_short_name(&text);
                if short != text {
                    let value = short.clone();
                    input.update(cx, |input, cx| input.set_value(value, window, cx));
                }
                let mut patch = serde_json::Map::new();
                patch.insert("localMachineShortName".to_string(), Value::String(short));
                tab.store.update(cx, |store, cx| {
                    store.apply_patch(patch, "settings:localMachineShortName", cx)
                });
            },
        );
        tab.local_short_name = Some((input, subscription));
    }
    let input = tab
        .local_short_name
        .as_ref()
        .map(|(input, _)| input.clone())
        .expect("made above");
    remote_row(
        row_main(
            t,
            "Short name",
            "Up to 2 characters, shown on the sidebar's computer pills. Each remote machine's own is in its settings.",
        ),
        Some(
            div()
                .flex_shrink_0()
                .w(px(64.0))
                .child(remote_input(
                    &t.p,
                    &input,
                    32.0,
                    10.0,
                    Some(64.0),
                    false,
                    false,
                    window,
                    cx,
                ))
                .into_any_element(),
        ),
    )
}

/// SSH access: "On", "Checking…", or off with Turn on SSH access and the by-hand steps under it.
fn ssh_row(
    tab: &mut RemoteTab,
    t: &RemoteTokens,
    window: &mut Window,
    cx: &mut Context<RemoteTab>,
) -> AnyElement {
    let ssh = tab.access.as_ref().map(|access| access.ssh.enabled);
    let highlighted = tab
        .ssh_highlight
        .is_some_and(|since| since.elapsed() < SSH_HIGHLIGHT);
    let main = row_main(
        t,
        "SSH access",
        "Lets your phone and other computers sign in to this machine. Easy Connect and Tailscale need it.",
    );
    let content = match ssh {
        None => remote_row(
            main,
            Some(
                row_value(t)
                    .child(spinner("current-machine-ssh-checking", 14.0, t.muted, 900))
                    .child("Checking…")
                    .into_any_element(),
            ),
        ),
        Some(true) => remote_row(
            main,
            Some(
                row_value(t)
                    .child(settings_icon(ICON_CIRCLE_CHECK_FILLED, 14.0, t.success))
                    .child("On")
                    .into_any_element(),
            ),
        ),
        Some(false) => v_flex()
            .w_full()
            .min_w_0()
            .child(remote_row(
                main,
                Some(
                    row_value(t)
                        .child(settings_icon(ICON_ALERT, 14.0, t.warn))
                        .child("Off")
                        .into_any_element(),
                ),
            ))
            .child(div().w_full().min_w_0().px(px(12.0)).pb(px(10.0)).child(
                super::ssh_row::ssh_access_row(
                    tab,
                    t,
                    "current-machine-ssh",
                    "",
                    "",
                    true,
                    window,
                    cx,
                ),
            ))
            .into_any_element(),
    };
    div()
        .id("remote-current-machine-ssh")
        .w_full()
        .min_w_0()
        .when(highlighted, |this| {
            this.bg(hsla(css_fade(t.foreground, 0.08)))
        })
        .child(content)
        .into_any_element()
}

/// The setup steps' SSH line: "SSH access is on", or "SSH access is off" with a link that opens
/// This computer.
pub(super) fn ssh_status_line(
    tab: &RemoteTab,
    t: &RemoteTokens,
    id: &'static str,
    cx: &mut Context<RemoteTab>,
) -> AnyElement {
    let line = h_flex()
        .w_full()
        .min_w_0()
        .flex_wrap()
        .items_center()
        .gap(px(6.0))
        .text_size(px(13.0))
        .line_height(px(18.57))
        .text_color(hsla(t.muted));
    let Some(enabled) = tab.access.as_ref().map(|access| access.ssh.enabled) else {
        return line
            .child(spinner(
                SharedString::from(format!("{id}-checking")),
                14.0,
                t.muted,
                900,
            ))
            .child("Checking SSH access…")
            .into_any_element();
    };
    if enabled {
        return line
            .child(settings_icon(ICON_CIRCLE_CHECK_FILLED, 14.0, t.success))
            .child("SSH access is on")
            .into_any_element();
    }
    let link_color = t.foreground;
    line.child(settings_icon(ICON_ALERT, 14.0, t.warn))
        .child("SSH access is off:")
        .child(
            div()
                .id(SharedString::from(format!("{id}-link")))
                .cursor_pointer()
                .text_color(hsla(link_color))
                .underline()
                .child("turn it on in This computer")
                .on_click(cx.listener(|tab: &mut RemoteTab, _, _window, cx| {
                    tab.show_current_machine_ssh(cx);
                })),
        )
        .into_any_element()
}
