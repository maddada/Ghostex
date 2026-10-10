//! The Tailscale path card (remote-tailscale-card.tsx (deleted 2026-10-01)).
//!
//! CDXC:RemotePairing 2026-09-03:
//! Four steps, the first two self-checking from `/api/remoteAccessStatus`, the last one a QR built from `/api/remotePairingCode.tailscale` that the app recognises by its prefix. Scanning only fills the form on the phone; the connection stays SSH over the tailnet with host, user, and password, so the typed values stay reachable behind "Or type these in".
//!
//! CDXC:RemotePairing 2026-09-03 DECISION:
//! User: "please add a toggle for tailscale. if tailscale is disabled …" (the sentence was cut off; the off behaviour below is the assumed reading).
//! The header switch sits in the same spot as Easy Connect's, beside the header button, so toggling never expands the card. It writes Settings.remoteTailscaleEnabled; nothing is sent to gxserver because Tailscale itself is not managed by Ghostex. Off: the card is collapsed and cannot be expanded (no chevron), it is dimmed like a hidden machine tile, the status badge reads Off, and no QR is rendered because the body never mounts.
use super::super::super::super::native_modal_kit::*;
use super::super::super::fields::{qr_code, settings_icon, switch_control};
use super::easy_connect::{path_card, path_head, qr_block, qr_meta, qr_pending};
use super::model::BadgeTone;
use super::style::*;
use super::{PathCard, RemoteTab, ssh_row};
use gpui::{
    AnyElement, Context, InteractiveElement as _, IntoElement, ParentElement as _, SharedString,
    StatefulInteractiveElement as _, Styled as _, Window, div, px,
};
use gpui_component::{h_flex, v_flex};

pub(super) fn tailscale_card(
    tab: &mut RemoteTab,
    t: &RemoteTokens,
    window: &mut Window,
    cx: &mut Context<RemoteTab>,
) -> AnyElement {
    let enabled = tab.store.read(cx).bool("remoteTailscaleEnabled");
    let tailscale = tab
        .access
        .as_ref()
        .and_then(|access| access.tailscale.clone());
    let open = enabled && tab.expanded == Some(PathCard::Tailscale);
    let detected = if !enabled {
        Some(("Off", BadgeTone::Disabled))
    } else {
        tailscale.as_ref().map(|status| {
            if status.running {
                ("Detected", BadgeTone::Active)
            } else if status.installed {
                ("Not running", BadgeTone::NeedsSetup)
            } else {
                ("Not installed", BadgeTone::Disabled)
            }
        })
    };
    let mut badges = vec![badge(t, "If you already use it", BadgeTone::Plain, false)];
    if let Some((label, tone)) = detected {
        badges.push(badge(t, label, tone, false));
    }
    let head = path_head(
        t,
        "remote-tailscale-toggle",
        ICON_SHIELD,
        false,
        "Tailscale",
        badges,
        enabled.then_some(open),
        open,
        enabled.then_some(PathCard::Tailscale),
        cx,
    );
    let switch = switch_control(
        &t.p,
        "remote-tailscale-switch",
        "Tailscale",
        enabled,
        false,
        None,
        |tab: &mut RemoteTab, checked, _window, cx| tab.set_tailscale_enabled(checked, cx),
        cx,
    );
    let body = open.then(|| tailscale_body(tab, t, window, cx));
    path_card(t, "remote-tailscale-card", head, switch, body)
}

/// `TailscaleStep`: the numbered (or checked) circle, the title, its detail and extra content.
fn step(
    t: &RemoteTokens,
    number: usize,
    done: bool,
    title: &'static str,
    detail_text: String,
    children: Option<AnyElement>,
) -> AnyElement {
    let (background, border, color) = if done {
        (
            css_fade(gpui::rgb(0x6ee7b7), 0.18),
            css_fade(gpui::rgb(0x6ee7b7), 0.40),
            t.success,
        )
    } else {
        (t.card(0.58), t.edge(0.76), t.muted)
    };
    h_flex()
        .w_full()
        .min_w_0()
        .items_start()
        .gap(px(10.0))
        .child(
            div()
                .flex_shrink_0()
                .mt(px(1.0))
                .size(px(20.0))
                .flex()
                .items_center()
                .justify_center()
                .rounded_full()
                .border_1()
                .border_color(hsla(border))
                .bg(hsla(background))
                .text_size(px(11.0))
                .line_height(px(15.71))
                .text_color(hsla(color))
                .child(if done {
                    settings_icon(ICON_CHECK, 12.0, color).into_any_element()
                } else {
                    div().child(format!("{number}")).into_any_element()
                }),
        )
        .child(
            v_flex()
                .flex_1()
                .min_w_0()
                .gap(px(4.0))
                .child(
                    div()
                        .text_size(px(14.0))
                        .line_height(px(19.6))
                        .text_color(hsla(t.foreground))
                        .child(title),
                )
                .child(detail(t, detail_text, false))
                .children(children),
        )
        .into_any_element()
}

fn tailscale_body(
    tab: &mut RemoteTab,
    t: &RemoteTokens,
    window: &mut Window,
    cx: &mut Context<RemoteTab>,
) -> AnyElement {
    let access = tab.access.clone();
    let tailscale = access.as_ref().and_then(|access| access.tailscale.clone());
    let running = tailscale.as_ref().is_some_and(|status| status.running);
    let ssh_on = access.as_ref().is_some_and(|access| access.ssh.enabled);
    let code = tab
        .pairing
        .as_ref()
        .and_then(|pairing| pairing.tailscale.clone());
    let host = code
        .as_ref()
        .and_then(|code| code.host.clone())
        .or_else(|| {
            tailscale
                .as_ref()
                .and_then(|status| status.magic_dns_name.clone())
        });
    let ip = code
        .as_ref()
        .and_then(|code| code.ip.clone())
        .or_else(|| tailscale.as_ref().and_then(|status| status.ip.clone()));
    let user = code
        .as_ref()
        .and_then(|code| code.user.clone())
        .or_else(|| access.as_ref().map(|access| access.username.clone()));
    let first_detail = match &tailscale {
        None => "Checking…".to_string(),
        Some(status) if status.running => format!(
            "Signed in as {}{}",
            status
                .account
                .clone()
                .unwrap_or_else(|| "your account".to_string()),
            if status.magic_dns_name.is_some() {
                " · MagicDNS on"
            } else {
                ""
            }
        ),
        Some(status) if status.installed => {
            "Tailscale is installed but not connected. Open it and sign in.".to_string()
        }
        Some(_) => "Install Tailscale on this computer and sign in.".to_string(),
    };
    let ssh_step_children = (!ssh_on).then(|| {
        ssh_row::ssh_access_row(
            tab,
            t,
            "tailscale-ssh",
            "Your phone signs in over SSH.",
            "Your phone signs in over SSH.",
            true,
            window,
            cx,
        )
    });
    let qr_left = match &code {
        Some(code) => {
            let grid = tab.qr_for(true, &code.payload);
            qr_code(grid.as_ref(), 140.0)
        }
        None => qr_pending(
            t,
            140.0,
            if running {
                "Waiting for the address…"
            } else {
                "Appears once Tailscale is running."
            },
        ),
    };
    let mut meta = qr_meta(t)
        .child(
            div()
                .w_full()
                .overflow_hidden()
                .whitespace_nowrap()
                .text_ellipsis()
                .child(rich_text(
                    t.foreground,
                    &[
                        ("Open the app → ", Emph::Plain),
                        ("Connect your computer", Emph::Em),
                        (" → ", Emph::Plain),
                        ("Scan code", Emph::Em),
                        (
                            ". The app recognizes this as a Tailscale code.",
                            Emph::Plain,
                        ),
                    ],
                )),
        )
        .child(div().flex().child(compact_button(
            &t.p,
            "remote-tailscale-manual-toggle",
            "Or type these in",
            Some(chevron(tab.manual_open, 16.0, t.p.foreground)),
            Look::Ghost,
            24.0,
            14.0,
            None,
            false,
            None,
            |tab: &mut RemoteTab, _window, cx| {
                tab.manual_open = !tab.manual_open;
                cx.notify();
            },
            cx,
        )));
    if tab.manual_open {
        let mut values = v_flex().w_full().gap(px(6.0));
        let has_host = host.is_some();
        if let Some(host) = host.clone() {
            values = values.child(manual_value_row(tab, t, "Host", host, cx));
        }
        if let Some(ip) = ip.clone() {
            values = values.child(manual_value_row(
                tab,
                t,
                if has_host { "or IP" } else { "IP" },
                ip,
                cx,
            ));
        }
        if let Some(user) = user.filter(|user| !user.is_empty()) {
            values = values.child(manual_value_row(tab, t, "Username", user, cx));
        }
        values = values.child(
            h_flex()
                .w_full()
                .items_center()
                .justify_between()
                .gap(px(10.0))
                .child(div().flex_shrink_0().child("Password"))
                .child(detail(t, "Your login password", false)),
        );
        if host.is_none() && ip.is_none() {
            values = values.child(detail(
                t,
                "The host and IP appear once Tailscale is running on this computer.",
                false,
            ));
        }
        meta = meta.child(values);
    }
    let steps = v_flex()
        .w_full()
        .min_w_0()
        .gap(px(8.0))
        .px(px(12.0))
        .py(px(10.0))
        .rounded(px(MODAL_RADIUS_CONTROL))
        .border_1()
        .border_color(hsla(t.edge(0.76)))
        .bg(hsla(t.card(0.28)))
        .child(step(
            t,
            1,
            running,
            "Tailscale is running on this computer",
            first_detail,
            None,
        ))
        .child(step(
            t,
            2,
            ssh_on,
            "Turn on SSH access",
            if ssh_on {
                "Ghostex checked it just now.".to_string()
            } else {
                "Ghostex can enable it; your computer asks for an admin password once.".to_string()
            },
            ssh_step_children,
        ))
        .child(step(
            t,
            3,
            false,
            "Install Tailscale on your phone",
            "Sign in to the same account, then make sure it shows Connected.".to_string(),
            None,
        ))
        .child(step(
            t,
            4,
            false,
            "In the Ghostex app, scan this code",
            "Fills in the name, address and username. The app then asks for your computer password once and saves it on the phone.".to_string(),
            Some(qr_block(t, qr_left, meta.into_any_element())),
        ));
    v_flex()
        .w_full()
        .min_w_0()
        .gap(px(10.0))
        .child(
            div()
                .w_full()
                .text_size(px(13.0))
                .line_height(px(19.5))
                .text_color(hsla(t.muted))
                .child("Your phone joins your tailnet and connects over SSH. Nothing to enable here; follow the checklist, then scan the code at the end with the app."),
        )
        .child(steps)
        .into_any_element()
}

/// `ManualValueRow`: a muted key and the monospace value with its copy button.
fn manual_value_row(
    tab: &mut RemoteTab,
    t: &RemoteTokens,
    label: &'static str,
    value: String,
    cx: &mut Context<RemoteTab>,
) -> AnyElement {
    h_flex()
        .w_full()
        .items_center()
        .justify_between()
        .gap(px(10.0))
        .child(div().flex_shrink_0().child(label))
        .child(
            h_flex()
                .min_w_0()
                .items_center()
                .gap(px(4.0))
                .child(
                    div()
                        .min_w_0()
                        .font_family(MODAL_MONO_FONT)
                        .text_color(hsla(t.foreground))
                        .child(value.clone()),
                )
                .child(copy_icon_button(
                    tab,
                    t,
                    SharedString::from(format!(
                        "remote-copy-{}",
                        label.to_lowercase().replace(' ', "-")
                    )),
                    format!("Copy {}", label.to_lowercase()),
                    value,
                    t.foreground,
                    cx,
                )),
        )
        .into_any_element()
}

/// `RemoteCopyButton` at its default size: a 24px ghost icon button that copies `value` and shows
/// a check (and the "Copied" tooltip) for a moment.
pub(super) fn copy_icon_button(
    tab: &mut RemoteTab,
    t: &RemoteTokens,
    id: SharedString,
    label: String,
    value: String,
    color: gpui::Rgba,
    cx: &mut Context<RemoteTab>,
) -> AnyElement {
    let copied = tab.copied.contains(&id);
    let hover = if t.p.light {
        gpui::rgb(0xf1f1f1)
    } else {
        gpui::rgb(0x2a2a2a)
    };
    let click_id = id.clone();
    div()
        .id(id)
        .flex_shrink_0()
        .size(px(24.0))
        .flex()
        .items_center()
        .justify_center()
        .rounded(px(MODAL_RADIUS_CONTROL))
        .cursor_pointer()
        .hover(move |this| this.bg(hsla(hover)))
        .tooltip(super::super::super::fields::tooltip_text(if copied {
            "Copied".to_string()
        } else {
            label
        }))
        .on_click(cx.listener(move |tab, _: &gpui::ClickEvent, _window, cx| {
            tab.copy(click_id.clone(), value.clone(), cx);
        }))
        .child(settings_icon(
            if copied {
                ICON_CIRCLE_CHECK_FILLED
            } else {
                ICON_COPY
            },
            16.0,
            color,
        ))
        .into_any_element()
}
