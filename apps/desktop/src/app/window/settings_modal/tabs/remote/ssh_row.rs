//! The "SSH access is on / off" row (remote-ssh-access-row.tsx (deleted 2026-10-01)) shared by the Easy Connect card and
//! the Tailscale card's step 2, and its per-OS "by hand" buttons with their instruction popovers.
//!
//! CDXC:RemotePairing 2026-09-03:
//! Three OS buttons, the current OS marked, each opening a small popover with that OS's manual steps. Shown with the row's off state: doing it by hand is the designed second route, not a fallback, so the buttons carry the same steps the mobile app shows in its help sheet.
use super::super::super::super::native_modal_kit::*;
use super::super::super::fields::{ButtonVariant, icon, settings_icon, settings_icon_button};
use super::RemoteTab;
use super::model::{SSH_ACCESS_PLATFORMS, ssh_access_instructions};
use super::style::*;
use gpui::prelude::FluentBuilder as _;
use gpui::{
    AnchoredPositionMode, AnyElement, Context, FontWeight, InteractiveElement as _, IntoElement,
    KeyDownEvent, MouseDownEvent, ParentElement as _, SharedString, Styled as _, Window, anchored,
    deferred, div, point, px, rgb,
};
use gpui_component::{h_flex, v_flex};
use std::cell::Cell;
use std::rc::Rc;

fn platform_icon(platform: &str) -> &'static str {
    match platform {
        "windows" => ICON_BRAND_WINDOWS,
        "linux" => ICON_BRAND_UBUNTU,
        _ => ICON_BRAND_APPLE,
    }
}

/// `SshAccessRow`. `compact` renders only the actions (a Tailscale step owns the title).
#[allow(clippy::too_many_arguments)]
pub(super) fn ssh_access_row(
    tab: &mut RemoteTab,
    t: &RemoteTokens,
    row_id: &'static str,
    detail_when_off: &'static str,
    detail_when_on: &'static str,
    compact: bool,
    window: &mut Window,
    cx: &mut Context<RemoteTab>,
) -> AnyElement {
    let rpc = tab.rpc_available(cx);
    let frame = |state_off: bool| {
        let border = if state_off {
            css_mix(t.warn, 0.30, t.border)
        } else {
            t.edge(0.76)
        };
        v_flex()
            .w_full()
            .min_w_0()
            .gap(px(4.0))
            .when(!compact, |this| {
                this.px(px(12.0))
                    .py(px(10.0))
                    .rounded(px(MODAL_RADIUS_CONTROL))
                    .border_1()
                    .border_color(hsla(border))
                    .bg(hsla(t.card(0.28)))
            })
    };
    let title = |icon_element: AnyElement, label: &'static str| {
        h_flex()
            .items_center()
            .gap(px(6.0))
            .text_size(px(14.0))
            .line_height(px(20.0))
            .text_color(hsla(t.foreground))
            .child(icon_element)
            .child(label)
    };
    let Some(ssh) = tab.access.as_ref().map(|access| access.ssh.clone()) else {
        return frame(false)
            .child(title(
                spinner(
                    SharedString::from(format!("{row_id}-checking")),
                    16.0,
                    t.foreground,
                    900,
                ),
                "Checking SSH access…",
            ))
            .into_any_element();
    };
    if ssh.enabled {
        return frame(false)
            .child(title(
                settings_icon(ICON_CIRCLE_CHECK_FILLED, 16.0, t.success).into_any_element(),
                "SSH access is on",
            ))
            .child(detail(t, detail_when_on, false))
            .into_any_element();
    }
    let attempt_text = match &tab.ssh_attempt {
        None => "Or do it by hand:".to_string(),
        Some(attempt) if attempt.outcome == "cancelled" => {
            "The admin prompt was cancelled. Or do it by hand:".to_string()
        }
        Some(attempt) => format!(
            "{} Or do it by hand:",
            attempt
                .message
                .clone()
                .unwrap_or_else(|| "Ghostex could not turn on SSH access.".to_string())
        ),
    };
    let enabling = tab.enabling_ssh;
    let button = compact_button(
        &t.p,
        SharedString::from(format!("{row_id}-enable")),
        if enabling {
            "Waiting for the admin prompt…"
        } else {
            "Turn on SSH access"
        },
        enabling.then(|| {
            spinner(
                SharedString::from(format!("{row_id}-enable-spinner")),
                16.0,
                t.p.foreground,
                900,
            )
        }),
        Look::Filled,
        24.0,
        14.0,
        None,
        !rpc || enabling,
        (!rpc).then(|| "This action needs the Ghostex server connection.".into()),
        |tab: &mut RemoteTab, _window, cx| tab.enable_ssh_access(cx),
        cx,
    );
    let platform = tab.access.as_ref().and_then(|access| access.platform);
    let mut row = frame(true);
    if !compact {
        row = row
            .child(title(
                settings_icon(ICON_ALERT, 16.0, t.warn).into_any_element(),
                "SSH access is off",
            ))
            .child(detail(t, detail_when_off, false));
    }
    row.child(h_flex().mt(px(4.0)).gap(px(8.0)).child(button))
        .child(div().mt(px(4.0)).child(detail(t, attempt_text, false)))
        .child(os_buttons(tab, t, row_id, platform, window, cx))
        .into_any_element()
}

/// `SshAccessInstructionButtons`: the three OS buttons (the current OS as `secondary`), each
/// opening its popover.
fn os_buttons(
    tab: &mut RemoteTab,
    t: &RemoteTokens,
    row_id: &'static str,
    current: Option<&'static str>,
    window: &mut Window,
    cx: &mut Context<RemoteTab>,
) -> AnyElement {
    let row: SharedString = row_id.into();
    let mut buttons = Vec::new();
    let mut popover = None;
    for (platform, label) in SSH_ACCESS_PLATFORMS {
        let key: SharedString = format!("{row_id}-{platform}").into();
        let bounds = tab
            .os_button_bounds
            .entry(key.clone())
            .or_insert_with(|| Rc::new(Cell::new(None)))
            .clone();
        let open = tab
            .os_popover
            .as_ref()
            .is_some_and(|(owner, open)| *owner == row && *open == platform);
        let is_current = current == Some(platform);
        let look = if is_current {
            Look::Secondary
        } else {
            Look::Filled
        };
        let text = if is_current {
            if t.p.light {
                rgb(0x18181b)
            } else {
                rgb(0xfafafa)
            }
        } else {
            t.p.foreground
        };
        let toggle_row = row.clone();
        let mut button = compact_button(
            &t.p,
            SharedString::from(format!("{key}-button")),
            label,
            Some(settings_icon(platform_icon(platform), 14.0, text).into_any_element()),
            look,
            24.0,
            12.0,
            None,
            false,
            None,
            move |tab: &mut RemoteTab, window, cx| {
                let already = tab
                    .os_popover
                    .as_ref()
                    .is_some_and(|(owner, open)| *owner == toggle_row && *open == platform);
                tab.os_popover = if already {
                    None
                } else {
                    tab.os_popover_focus.focus(window, cx);
                    Some((toggle_row.clone(), platform))
                };
                cx.notify();
            },
            cx,
        );
        if open && !is_current {
            // `data-popup-open` keeps the trigger a step stronger than its rest fill.
            button = div()
                .rounded(px(MODAL_RADIUS_CONTROL))
                .bg(hsla(neutral_button_fills(t.p.light).rest))
                .child(button)
                .into_any_element();
        }
        buttons.push(
            div()
                .on_children_prepainted(capture_child_bounds(bounds.clone(), 0))
                .child(button)
                .into_any_element(),
        );
        if open {
            popover = instructions_popover(tab, t, platform, bounds, window, cx);
        }
    }
    h_flex()
        .flex_wrap()
        .gap(px(6.0))
        .children(buttons)
        .children(popover)
        .into_any_element()
}

/// The instruction popover under an OS button (`PopoverContent align='start' side='bottom'
/// sideOffset={6}`, 320px, 16px padding, 12px gap), portaled like Base UI's so it draws in the
/// system UI face.
fn instructions_popover(
    tab: &mut RemoteTab,
    t: &RemoteTokens,
    platform: &'static str,
    trigger: Rc<Cell<Option<gpui::Bounds<gpui::Pixels>>>>,
    _window: &mut Window,
    cx: &mut Context<RemoteTab>,
) -> Option<AnyElement> {
    let bounds = trigger.get()?;
    let instructions = ssh_access_instructions(platform);
    let background = t.p.solid_dialog_surface();
    let border = if t.p.light {
        modal_rgba(0x000000, 0.12)
    } else {
        modal_rgba(0xffffff, 0.12)
    };
    let foreground = t.foreground;
    let close = |tab: &mut RemoteTab, _window: &mut Window, cx: &mut Context<RemoteTab>| {
        tab.os_popover = None;
        cx.notify();
    };
    let steps = instructions.steps.iter().enumerate().map(|(index, step)| {
        h_flex()
            .w_full()
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
                    .border_color(hsla(t.edge(0.76)))
                    .bg(hsla(t.card(0.58)))
                    .text_size(px(14.0))
                    .font_weight(FontWeight::BOLD)
                    .text_color(hsla(t.muted))
                    .child(format!("{}", index + 1)),
            )
            .child(
                div()
                    .flex_1()
                    .min_w_0()
                    .text_size(px(14.0))
                    .line_height(px(19.6))
                    .font_weight(FontWeight::SEMIBOLD)
                    .text_color(hsla(foreground))
                    .child(*step),
            )
    });
    let panel = v_flex()
        .id("remote-os-popover")
        .track_focus(&tab.os_popover_focus)
        .occlude()
        .w(px(320.0))
        .p(px(16.0))
        .gap(px(12.0))
        .rounded(px(MODAL_RADIUS_CONTROL))
        .border_1()
        .border_color(hsla(border))
        .bg(hsla(background))
        .shadow_md()
        .font_family(MODAL_UI_FONT)
        .text_size(px(14.0))
        .line_height(px(20.0))
        .text_color(hsla(foreground))
        .on_key_down(cx.listener(move |tab, event: &KeyDownEvent, window, cx| {
            if event.keystroke.key == "escape" {
                cx.stop_propagation();
                close(tab, window, cx);
            }
        }))
        .on_mouse_down_out(cx.listener(move |tab, event: &MouseDownEvent, window, cx| {
            if bounds.contains(&event.position) {
                return;
            }
            close(tab, window, cx);
        }))
        .child(
            h_flex()
                .w_full()
                .items_center()
                .justify_between()
                .gap(px(8.0))
                .child(
                    h_flex()
                        .items_center()
                        .gap(px(6.0))
                        .font_weight(FontWeight::SEMIBOLD)
                        .child(settings_icon(platform_icon(platform), 16.0, foreground))
                        .child(instructions.title),
                )
                .child(settings_icon_button(
                    &t.p,
                    "remote-os-popover-close",
                    icon::X,
                    16.0,
                    24.0,
                    ButtonVariant::Ghost,
                    None,
                    false,
                    close,
                    cx,
                )),
        )
        .child(v_flex().w_full().gap(px(8.0)).children(steps))
        .child(
            div()
                .w_full()
                .overflow_hidden()
                .whitespace_nowrap()
                .text_ellipsis()
                .text_color(hsla(t.muted))
                .child(instructions.note),
        );
    Some(
        deferred(
            anchored()
                .position_mode(AnchoredPositionMode::Window)
                .position(point(
                    bounds.origin.x,
                    bounds.origin.y + bounds.size.height + px(6.0),
                ))
                .snap_to_window_with_margin(px(16.0))
                .child(panel),
        )
        .with_priority(1)
        .into_any_element(),
    )
}
