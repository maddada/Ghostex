//! The computer you're on, titled by its name: what belongs to it (its name, its icon, its SSH
//! access), behind its row on the Remote page, and the one-line SSH status the Tailscale steps show
//! instead of their own SSH controls.
//!
//! The 2026-11-08 "Current machine" card above the tabs (Q1970 to Q1973) became this row on
//! 2026-11-10 (CDXC:RemotePairing 2026-11-10 in mod.rs). What it holds stays as the user decided
//! then: the name read-only from the OS, the short name (an icon since 2026-11-12, computer_icon.rs), SSH access; the Tailscale steps show "SSH
//! access is on", or "SSH access is off" with a link that opens this row and highlights its SSH
//! access row.
use super::super::super::super::native_modal_kit::*;
use super::super::super::fields::settings_icon;
use super::RemoteTab;
use super::paired_devices::remote_row;
use super::style::*;
use gpui::prelude::FluentBuilder as _;
use gpui::{
    AnyElement, Context, InteractiveElement as _, IntoElement, ParentElement as _, SharedString,
    StatefulInteractiveElement as _, Styled as _, Window, div, px,
};
use gpui_component::{h_flex, v_flex};
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
pub(super) fn computer_name(tab: &RemoteTab) -> String {
    tab.access
        .as_ref()
        .map(|access| access.computer_name.clone())
        .filter(|name| !name.trim().is_empty())
        .unwrap_or_else(crate::host_name::local_computer_name)
}

/// The faint dot under this computer's icon: its diameter, gap under the icon and share of the muted
/// text colour, as the sidebar draws it (native_sidebar/computer_mark.rs `with_here_dot`, which
/// Settings, also drawn by the preview binary, can't call).
const DOT_SIZE: f32 = 3.0;
const DOT_GAP: f32 = 2.0;
const DOT_OPACITY: f32 = 0.55;

/// This computer's icon (the one picked for it, else its system's), in the tile a saved computer's
/// row has, with the faint dot under it while another computer shows in the sidebar.
///
/// CDXC:RemoteMachines 2026-11-20 SEE-ALSO: native_sidebar/computer_mark.rs `with_here_dot` (the faint
/// dot under the computer you're on, like the macOS Dock's).
pub(super) fn this_computer_icon_tile(
    tab: &RemoteTab,
    t: &RemoteTokens,
    cx: &gpui::App,
) -> AnyElement {
    let id = ghostex_gx_core::LOCAL_MACHINE_ID;
    let asset = tab
        .computer_icons
        .picked(id)
        .and_then(ghostex_gx_core::computer_icons::computer_icon)
        .map(|icon| icon.asset)
        .or_else(|| tab.computer_icons.default_asset(id))
        .unwrap_or(ICON_DEVICE_DESKTOP);
    // Another computer the sidebar shows is connected (a saved one that is on, isn't this one
    // again, and answers), and Mark the computer you're on is on: the sidebar's rule
    // (gx_store/remote_clients.rs).
    let several = tab.store.read(cx).value("markComputerYouAreOn").as_bool() == Some(true)
        && tab.remote_machines(cx).iter().any(|machine| {
            machine.get("disabled").and_then(serde_json::Value::as_bool) != Some(true)
                && machine
                    .get("id")
                    .and_then(serde_json::Value::as_str)
                    .is_some_and(|machine_id| {
                        !tab.this_computer.contains(machine_id)
                            && tab.connect_states.get(machine_id).map(String::as_str)
                                == Some("connected")
                    })
        });
    const SIZE: f32 = 18.0;
    super::machines::tile_frame(t)
        .child(
            div()
                .relative()
                .size(px(SIZE))
                .flex()
                .items_center()
                .justify_center()
                .child(settings_icon(
                    asset,
                    SIZE,
                    css_mix(t.muted, 0.70, t.foreground),
                ))
                .when(several, |icon| {
                    icon.child(
                        div()
                            .absolute()
                            .left(px((SIZE - DOT_SIZE) / 2.0))
                            .top(px(SIZE + DOT_GAP))
                            .size(px(DOT_SIZE))
                            .rounded_full()
                            .bg(hsla(t.muted).opacity(DOT_OPACITY)),
                    )
                }),
        )
        .into_any_element()
}

/// The row's one line under its title, the computer's name: "Name, icon · SSH access on".
pub(super) fn this_computer_summary(tab: &RemoteTab, _cx: &gpui::App) -> String {
    let mut parts = vec!["The computer you're on".to_string()];
    match tab.access.as_ref().map(|access| access.ssh.enabled) {
        Some(true) => parts.push("SSH access on".to_string()),
        Some(false) => parts.push("SSH access off".to_string()),
        None => {}
    }
    parts.join(" · ")
}

/// The rows behind "This computer": its name, its icon and SSH access, under hairlines.
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
    let icon_row = icon_row(tab, t, cx);
    let ssh_row = ssh_row(tab, t, window, cx);
    vec![name_row, icon_row, ssh_row]
}

/// This computer's icon (CDXC:RemoteMachines 2026-11-12 in gx-core computer_icons.rs): kept on
/// this computer by its gxserver, so the phone and other computers show it too. It replaces the
/// short name, which the sidebar no longer draws.
fn icon_row(tab: &RemoteTab, t: &RemoteTokens, cx: &mut Context<RemoteTab>) -> AnyElement {
    remote_row(
        v_flex()
            .w_full()
            .min_w_0()
            .gap(px(10.0))
            .py(px(10.0))
            .child(row_main(
                t,
                "Icon",
                "Marks this computer in the sidebar while more than one computer is there, on every device that reaches it.",
            ))
            .child(super::computer_icon::computer_icon_picker(
                tab,
                t,
                ghostex_gx_core::LOCAL_MACHINE_ID,
                cx,
            ))
            .into_any_element(),
        None,
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
