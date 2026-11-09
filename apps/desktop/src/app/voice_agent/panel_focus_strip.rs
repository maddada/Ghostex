//! The voice panel's focus strip: under the panel's top bar while a call is about one session, the
//! agent's icon, the session's title (click it to show the session), "<project> · thread of
//! <orchestrator>" in muted text, the computer the call runs on when it isn't this one, and All
//! sessions, which widens the call; a call with a GBot shows the GBot instead and keeps no All
//! sessions (`gbot_call.rs`). A child of `panel.rs`, whose snapshot and controls it uses.
//!
//! CDXC:VoiceMode 2026-10-23 DECISION: User, asked "How does the call panel show which session the call is about?", picked "A. A strip under the top bar with the name, project and orchestrator, plus All sessions (and a small headset on that session's sidebar row)". A session closed during the call says Closed and keeps All sessions; the call goes on (the sidebar's headset is `sidebar_badge.rs`).

use gpui::prelude::FluentBuilder as _;
use gpui::{
    AnyElement, Context, FontWeight, InteractiveElement as _, IntoElement, MouseButton,
    MouseDownEvent, MouseUpEvent, ParentElement as _, SharedString,
    StatefulInteractiveElement as _, Styled as _, div, px, rgb,
};
use gpui_component::{h_flex, v_flex};

use super::{PanelAction, PanelColors, PanelSnapshot, VoicePanelWindow};
use crate::app::helpers::*;

const ICON_WIDEN: &str = "titlebar/arrows-diagonal.svg";
const ICON_SESSION: &str = "titlebar/terminal-2.svg";

/// "Ghostex · thread of Main Coordinator · on studio": what the strip's muted line says.
pub(super) fn focus_subtitle(
    project: &str,
    orchestrator: Option<&str>,
    machine_name: Option<&str>,
) -> String {
    let mut parts: Vec<String> = Vec::new();
    if !project.trim().is_empty() {
        parts.push(project.trim().to_string());
    }
    if let Some(orchestrator) = orchestrator.map(str::trim).filter(|name| !name.is_empty()) {
        parts.push(format!("thread of {orchestrator}"));
    }
    if let Some(machine) = machine_name.map(str::trim).filter(|name| !name.is_empty()) {
        parts.push(format!("on {machine}"));
    }
    parts.join(" · ")
}

impl VoicePanelWindow {
    /// The strip, or `None` while the call is about all sessions of this computer. A call on
    /// another computer widened to all sessions keeps one line saying which computer it runs on.
    pub(super) fn focus_strip(
        &self,
        snapshot: &PanelSnapshot,
        colors: &PanelColors,
        cx: &mut Context<Self>,
    ) -> Option<AnyElement> {
        let Some(focus) = snapshot.focus.as_ref() else {
            let machine = snapshot.machine_name.as_deref()?;
            return Some(
                h_flex()
                    .flex_shrink_0()
                    .px(px(12.0))
                    .py(px(5.0))
                    .border_b_1()
                    .border_color(colors.border)
                    .min_w_0()
                    .truncate()
                    .text_size(px(11.0))
                    .text_color(colors.muted)
                    .child(format!("All sessions on {machine}"))
                    .into_any_element(),
            );
        };
        let subtitle = focus_subtitle(
            &focus.project,
            focus.orchestrator.as_deref(),
            snapshot.machine_name.as_deref(),
        );
        // A call with a GBot: its icon in its colour, "Call with <name>" and the chat under it,
        // and no All sessions (the call stays with the GBot, gbot_call.rs).
        let gbot = focus.gbot.as_ref();
        let subtitle = match gbot {
            Some(_) => focus_subtitle(&focus.title, None, snapshot.machine_name.as_deref()),
            None => subtitle,
        };
        let shown_title = gbot.map_or_else(|| focus.title.clone(), |gbot| gbot.strip_title());
        let icon = match gbot {
            Some(gbot) => gpui::svg()
                .path(SharedString::from(format!("titlebar/{}.svg", gbot.icon)))
                .size(px(14.0))
                .flex_shrink_0()
                .text_color(
                    gpui::Rgba::try_from(gbot.color_hex.as_str())
                        .map(gpui::Hsla::from)
                        .unwrap_or(colors.muted),
                )
                .into_any_element(),
            None => match focus
            .agent_icon
            .as_deref()
            .and_then(|icon| workspace_tab_agent_icon_path(icon).map(|path| (icon, path)))
        {
            Some((icon, path)) => gpui::svg()
                .path(path)
                .size(px(14.0))
                .flex_shrink_0()
                .text_color(rgb(workspace_tab_agent_icon_accent_color(icon)))
                .into_any_element(),
            None => titlebar_svg_icon(ICON_SESSION, 14.0, colors.muted).into_any_element(),
        },
        };
        let title_color = if focus.closed {
            colors.muted
        } else {
            colors.text
        };
        let title = div()
            .id("voice-panel-focus-title")
            .role(gpui::Role::Button)
            .aria_label(SharedString::from(format!("Show {}", focus.title)))
            .min_w_0()
            .truncate()
            .text_size(px(12.5))
            .font_weight(FontWeight::SEMIBOLD)
            .text_color(title_color)
            .cursor_pointer()
            .hover(|this| this.underline())
            .on_mouse_down(
                MouseButton::Left,
                cx.listener(|_, _: &MouseDownEvent, window, cx| {
                    window.prevent_default();
                    cx.stop_propagation();
                }),
            )
            .on_mouse_up(
                MouseButton::Left,
                cx.listener(|this, _: &MouseUpEvent, window, cx| {
                    window.prevent_default();
                    cx.stop_propagation();
                    this.run(PanelAction::ShowFocus, cx);
                }),
            )
            .on_a11y_action(gpui::AccessibleAction::Click, {
                let panel = cx.entity().downgrade();
                move |_, _, cx| {
                    let _ = panel.update(cx, |this, cx| this.run(PanelAction::ShowFocus, cx));
                }
            })
            .child(shown_title);
        Some(
            h_flex()
                .flex_shrink_0()
                .px(px(12.0))
                .py(px(6.0))
                .gap(px(8.0))
                .items_center()
                .border_b_1()
                .border_color(colors.border)
                .bg(colors.ink.opacity(0.03))
                .child(icon)
                .child(
                    v_flex()
                        .flex_1()
                        .min_w_0()
                        .child(
                            h_flex()
                                .min_w_0()
                                .gap(px(6.0))
                                .items_center()
                                .child(title)
                                .when(focus.closed, |this| {
                                    this.child(
                                        div()
                                            .flex_shrink_0()
                                            .text_size(px(11.0))
                                            .text_color(colors.muted)
                                            .child("Closed"),
                                    )
                                }),
                        )
                        .when(!subtitle.is_empty(), |this| {
                            this.child(
                                div()
                                    .min_w_0()
                                    .truncate()
                                    .text_size(px(11.0))
                                    .text_color(colors.muted)
                                    .child(subtitle),
                            )
                        }),
                )
                .when(gbot.is_none(), |this| {
                    this.child(self.button(
                        "voice-panel-all-sessions",
                        ICON_WIDEN,
                        Some("All sessions"),
                        PanelAction::AllSessions,
                        colors,
                        false,
                        cx,
                    ))
                })
                .into_any_element(),
        )
    }
}

#[cfg(test)]
mod tests {
    use super::focus_subtitle;

    #[test]
    fn subtitle_names_project_orchestrator_and_computer() {
        assert_eq!(focus_subtitle("Ghostex", None, None), "Ghostex");
        assert_eq!(
            focus_subtitle("Ghostex", Some("Main Coordinator"), None),
            "Ghostex · thread of Main Coordinator"
        );
        assert_eq!(
            focus_subtitle("Ghostex", Some("Main Coordinator"), Some("studio")),
            "Ghostex · thread of Main Coordinator · on studio"
        );
        assert_eq!(focus_subtitle("", Some(" "), Some("studio")), "on studio");
    }
}
