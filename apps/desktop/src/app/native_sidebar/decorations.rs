use super::{appearance::SidebarAppearance, model::NativeSidebarSession};
use crate::{GhostexGpuiApp, app::helpers::*};
use gpui::prelude::FluentBuilder;
use gpui::{
    AnyElement, FontWeight, InteractiveElement, IntoElement, ParentElement,
    StatefulInteractiveElement, Styled, div, px, rgb,
};
use serde_json::{Value, json};

impl GhostexGpuiApp {
    pub(crate) fn render_native_session_identity(
        &self,
        session: &NativeSidebarSession,
        icon: AnyElement,
        hovered: bool,
        appearance: &SidebarAppearance,
        cx: &mut gpui::Context<Self>,
    ) -> AnyElement {
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
        let closing = session
            .details
            .get("closeAfterDone")
            .and_then(Value::as_bool)
            == Some(true)
            || session
                .details
                .get("closeAfterDoneDeadlineAt")
                .and_then(Value::as_str)
                .is_some();
        let id = session.session_id.clone();
        // CDXC:Coordinators 2026-10-06 DECISION: User: the fold chevron replaces the coordinator's crown while its card is hovered, with no chevron slot of its own. The crown's spot is the single fold hit area and the accessibility button, hovered or not; a pending delayed send or close-after-done keeps its own clock icon and click.
        let fold =
            super::threads::coordinator_fold_state(session).filter(|_| !(delayed || closing));
        let icon = match fold {
            Some(collapsed) if hovered => {
                super::threads::coordinator_fold_chevron(collapsed, appearance)
            }
            // CDXC:Coordinators 2026-10-22 SEE-ALSO: the user's decision on gx-core `RowNesting::contains_active_session` (the hollow circle in the crown's spot while the card is not hovered).
            Some(true) if super::threads::coordinator_holds_active(session) => {
                super::threads::contains_active_marker(appearance)
            }
            _ => icon,
        };
        div().id(format!("native-session-identity-{id}")).size(px(15.0 * appearance.scale)).flex_shrink_0().flex().items_center().justify_center().child(icon)
            .when_some(fold, |identity, collapsed| {
                let id = id.clone();
                let scale = appearance.scale;
                let (light, glass, hover) = (appearance.light, appearance.glass, appearance.hover);
                // CDXC:Coordinators 2026-10-06 DECISION: User: "please make the chevron button have a highlight button behind it when i hover over it (rounded bg behind it)". Same radius and colours as the card's hover action buttons (`hover_actions.rs`); the spot grows to 18px with a negative margin so the row's layout stays that of the 15px crown.
                identity
                    .size(px(18.0 * scale))
                    .m(px(-1.5 * scale))
                    .rounded(px(4.0 * scale))
                    .hover(move |item| {
                        item.bg(if light && glass {
                            gpui::Hsla::from(gpui::rgb(0xffffff)).opacity(0.85)
                        } else if light {
                            gpui::Hsla::from(gpui::rgb(0xe4e4e7))
                        } else {
                            hover
                        })
                    })
                    .role(gpui::Role::Button)
                    .aria_label(if collapsed { "Show threads" } else { "Hide threads" })
                    .cursor_pointer()
                    .on_mouse_down(gpui::MouseButton::Left, |_, _, cx| cx.stop_propagation())
                    .on_click(cx.listener(move |app, _, _, cx| {
                        cx.stop_propagation();
                        app.dispatch_native_sidebar_ui(json!({"type": "toggleCoordinator", "sessionId": id}), cx);
                    }))
            })
            .when(delayed || closing, |identity| identity.cursor_pointer().on_click(cx.listener(move |app, _, _, cx| {
                cx.stop_propagation();
                if delayed { app.dispatch_native_sidebar_ui(json!({"type": "sessionAction", "sessionId": id, "action": "delayedSend"}), cx); }
                else { app.dispatch_native_sidebar_command(json!({"type": "toggleCloseAfterDone", "sessionId": id}), cx); }
            })))
            .into_any_element()
    }

    pub(crate) fn render_native_session_decorations(
        &self,
        session: &NativeSidebarSession,
        appearance: &SidebarAppearance,
        _cx: &mut gpui::Context<Self>,
    ) -> Vec<AnyElement> {
        let scale = appearance.scale;
        let mut decorations = Vec::new();
        // CDXC:Sessions 2026-09-17 DECISION:
        // User: remove the sidebar row's floating pin and unpin icons.
        if session
            .session_note
            .as_ref()
            .is_some_and(|note| !note.trim().is_empty())
        {
            decorations.push(
                div()
                    .absolute()
                    .left(px(0.0))
                    .top(px(15.0 * scale))
                    .size(px(4.0 * scale))
                    .rounded_full()
                    .bg(chrome_color(0xffffff, 0x262626))
                    .into_any_element(),
            );
        }
        let queued = session.queued_prompt_count > 0;
        if session.has_composer_draft {
            // CDXC:Sessions 2026-09-21 DECISION:
            // User: the composer-draft dot is #B9D8FA in both themes (was white on dark, near-black on light).
            decorations.push(
                div()
                    .absolute()
                    .left(px((if queued { 19.0 } else { 15.0 }) * scale))
                    .top(px((if queued { 4.5 } else { 8.5 }) * scale))
                    .size(px(6.0 * scale))
                    .rounded_full()
                    .bg(rgb(0xb9d8fa))
                    .into_any_element(),
            );
        }
        if queued {
            let failed = session
                .details
                .get("queuedPromptFailedCount")
                .and_then(Value::as_u64)
                .unwrap_or(0)
                > 0;
            decorations.push(
                div()
                    .absolute()
                    .left(px(13.0 * scale))
                    .top(px(6.5 * scale))
                    .min_w(px(10.0 * scale))
                    .h(px(10.0 * scale))
                    .px(px(2.0 * scale))
                    .flex()
                    .items_center()
                    .justify_center()
                    .rounded_full()
                    .bg(rgb(if failed { 0xff6b6b } else { 0xf6c945 }))
                    .text_color(rgb(0x1d1704))
                    .text_size(px(7.0 * scale))
                    .font_weight(FontWeight::BOLD)
                    .child(if session.queued_prompt_count > 99 {
                        "99+".to_owned()
                    } else {
                        session.queued_prompt_count.to_string()
                    })
                    .into_any_element(),
            );
        }
        decorations
    }
}

pub(crate) fn selected_outline(appearance: &SidebarAppearance) -> AnyElement {
    outline(appearance, appearance.selected_outline)
}

pub(crate) fn session_outline(appearance: &SidebarAppearance) -> AnyElement {
    outline(appearance, appearance.session_outline)
}

/// The blue outline of a session card in a multi-selection.
///
/// CDXC:Sidebar 2026-10-25 WHY: drawn over the card rather than as the card's own border, because a border moves everything the card places inside it by its width: the row's icon and title, and its tree lines, so the line of an outer level jogged sideways beside a selected thread.
pub(crate) fn multi_selected_outline(scale: f32) -> AnyElement {
    div()
        .absolute()
        .inset_0()
        .rounded(px(5.0 * scale))
        .border_1()
        .border_color(rgb(0x2f8cff))
        .into_any_element()
}

fn outline(appearance: &SidebarAppearance, outline: gpui::Hsla) -> AnyElement {
    let highlight = appearance.selected_highlight;
    let scale = appearance.scale;
    gpui::canvas(
        |_, _, _| {},
        move |bounds, _, window, _| {
            window.paint_quad(gpui::quad(
                bounds,
                px(5.0 * scale),
                gpui::transparent_black(),
                px(1.0),
                outline,
                gpui::BorderStyle::Solid,
            ));
            let inset = px(5.0 * scale);
            let line = gpui::Bounds {
                origin: gpui::point(bounds.left() + inset, bounds.top() + px(1.0)),
                size: gpui::size((bounds.size.width - inset * 2.0).max(px(0.0)), px(1.0)),
            };
            window.paint_quad(gpui::quad(
                line,
                px(0.0),
                highlight,
                px(0.0),
                highlight,
                gpui::BorderStyle::Solid,
            ));
        },
    )
    .absolute()
    .inset_0()
    .into_any_element()
}
