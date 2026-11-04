use super::{
    appearance::SidebarAppearance,
    images::{agent_image, sidebar_image},
    model::NativeSidebarSession,
};
use crate::app::{consts::*, helpers::*};
use gpui::{AnyElement, IntoElement, ParentElement, Styled, div, img, px, rgb};
use serde_json::Value;

pub(crate) fn session_icon(
    session: &NativeSidebarSession,
    hud: &Value,
    appearance: &SidebarAppearance,
    buttons_shown: bool,
    hovered: bool,
) -> AnyElement {
    render_session_icon(session, hud, appearance, buttons_shown, hovered, false)
}

pub(crate) fn session_drag_icon(
    session: &NativeSidebarSession,
    appearance: &SidebarAppearance,
) -> AnyElement {
    render_session_icon(session, &Value::Null, appearance, false, false, true)
}

fn render_session_icon(
    session: &NativeSidebarSession,
    hud: &Value,
    appearance: &SidebarAppearance,
    buttons_shown: bool,
    hovered: bool,
    dragging: bool,
) -> AnyElement {
    let scale = appearance.scale;
    let settings = &hud["settings"];
    let delayed = session
        .details
        .get("delayedSendDeadlineAt")
        .and_then(Value::as_str)
        .is_some()
        || session
            .details
            .get("delayedSendRemainingLabel")
            .and_then(Value::as_str)
            .is_some();
    let close = session
        .details
        .get("closeAfterDone")
        .and_then(Value::as_bool)
        == Some(true)
        || session
            .details
            .get("closeAfterDoneDeadlineAt")
            .and_then(Value::as_str)
            .is_some()
        || session
            .details
            .get("closeAfterDoneRemainingLabel")
            .and_then(Value::as_str)
            .is_some();
    if delayed || close {
        return titlebar_svg_icon(
            "titlebar/clock.svg",
            18.0 * scale,
            rgb(if delayed { 0xf4ce6b } else { 0xf2a2a2 }).into(),
        )
        .into_any_element();
    }
    if !buttons_shown
        && let Some(tag) = session
            .details
            .get("tagPresentation")
            .filter(|value| value.is_object())
    {
        let color = tag["iconColor"]
            .as_str()
            .and_then(|value| u32::from_str_radix(value.trim_start_matches('#'), 16).ok())
            .unwrap_or(0xb4b8c0);
        let light = appearance.light;
        let color = if light
            && session.details.get("effectiveTag").and_then(Value::as_str) == Some("favorite")
        {
            0xf6c944
        } else if light
            && session.details.get("effectiveTag").and_then(Value::as_str) != Some("favorite")
        {
            let darken = |channel| ((color >> channel & 255u32) as f32 * 0.42).round() as u32;
            darken(16) << 16 | darken(8) << 8 | darken(0)
        } else {
            color
        };
        return gpui::svg()
            .path(gpui_sidebar_command_icon_asset_path(
                if tag["icon"] == "star" {
                    Some("star-filled")
                } else {
                    tag["icon"].as_str()
                },
            ))
            .size(px(15.0 * scale))
            .text_color(rgb(color))
            .into_any_element();
    }
    if session.is_draft {
        return titlebar_svg_icon(
            "titlebar/pencil.svg",
            15.0 * scale,
            appearance.foreground.opacity(0.48),
        )
        .into_any_element();
    }
    let hidden = settings
        .get(if session.is_browser() {
            "hideBrowserFaviconUntilHover"
        } else {
            "hideSessionAgentIconUntilHover"
        })
        .and_then(Value::as_bool)
        == Some(true)
        && !hovered;
    let mut opacity: f32 = if hidden {
        0.0
    } else if session.is_focused {
        0.8
    } else if session.is_visible || buttons_shown {
        0.26
    } else if session.lifecycle_state.as_deref() == Some("sleeping") {
        0.48
    } else {
        // CDXC:Sidebar 2026-10-24 DECISION: User: "ok do 70% for non sleeping basically for now (for agent icons)". An awake row that is not focused, visible or hovered rests its agent icon at 70%; a sleeping row's icon stays at 48%. The phone's session row (`SessionRow.tsx`, `styles.icon` and `iconAwake`) uses the same two values.
        0.70
    };
    if appearance.light {
        opacity = (opacity * 1.3).min(1.0);
    }
    if dragging {
        opacity = 1.0;
    }
    if super::threads::is_coordinator(session) {
        return div()
            .size(px(15.0 * scale))
            .flex()
            .items_center()
            .justify_center()
            .opacity(opacity)
            .child(super::threads::coordinator_icon(appearance))
            .into_any_element();
    }
    let image = session
        .favicon_data_url
        .as_deref()
        .or_else(|| {
            session
                .details
                .get("agentLogoDataUrl")
                .and_then(Value::as_str)
        })
        .and_then(|value| {
            if session.is_browser() {
                sidebar_image(value)
            } else {
                agent_image(value, session.agent_icon.as_deref(), appearance.light)
            }
        });
    div()
        .size(px(15.0 * scale))
        .flex()
        .items_center()
        .justify_center()
        .opacity(opacity)
        .child(match image {
            Some(image) => img(image).size(px(13.0 * scale)).into_any_element(),
            None => titlebar_svg_icon(
                if session.is_browser() {
                    BROWSER_ICON_WORLD
                } else {
                    "titlebar/terminal-2.svg"
                },
                15.0 * scale,
                appearance.foreground,
            )
            .into_any_element(),
        })
        .into_any_element()
}
