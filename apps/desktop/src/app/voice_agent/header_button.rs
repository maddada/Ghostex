//! The work area header's Voice button: the headset, and the arrow beside it that picks what a call
//! is about.
//!
//! CDXC:VoiceMode 2026-10-20 DECISION: User: "the one in the chat box just pastes whatever i say into the chat box, while the one above is more general and for discussing". The header carries a global Voice button that starts and ends a spoken conversation with the manager voice agent, which keeps going while the user moves between projects and chats.
//! CDXC:VoiceMode 2026-10-20 DECISION: User, pointing at the empty space left of Start: "please move this button" … "here" ("i asked for this but it wasn't done!"). The Voice button sits immediately left of the Start split button, first of the header's buttons, with their 4px gap. Supersedes its place beside the ⋯ menu. On a narrow column the header's buttons scroll sideways from their start and nothing folds, so being first keeps the Voice button in view the longest: Commit and Open scroll out of reach before it does.
//! CDXC:VoiceMode 2026-10-23 DECISION: User, asked "Where do you start a voice call about one session on the computer?", picked "C. A little arrow on the header's headset button" (not the recommended menu rows). The Voice button is a split button shaped like Start: the headset still starts and ends the all-sessions call, and its arrow opens "All sessions" (with its hotkey) and "This session: <title>" (the session the work area shows), the open call's scope ticked; picking the other row while a call is open changes that call's scope (`focus.rs`). The "Voice call about this session" hotkey has no default key.

use gpui::prelude::FluentBuilder as _;
use gpui::{
    Bounds, InteractiveElement as _, IntoElement, MouseButton, MouseDownEvent, ParentElement as _,
    Pixels, SharedString, Styled as _, div, px, rgba,
};
use gpui_component::tooltip::{ManagedTooltipExt as _, ManagedTooltipPlacement};
use gpui_component::{ElementExt as _, h_flex};

use super::conversation::VoicePhase;
use super::focus::{VoiceMenuPick, voice_menu_rows};
use crate::app::actions::{VoiceCallAboutSession, VoiceCallAllSessions};
use crate::app::consts::*;
use crate::app::context_menu::GpuiContextMenu;
use crate::app::helpers::*;
use crate::*;

const ICON_HEADSET: &str = "titlebar/headset.svg";
/// The hotkey of "Voice call about this session" (no default key).
pub(crate) const VOICE_CALL_ABOUT_SESSION_HOTKEY: &str = "voiceCallAboutSession";

impl GhostexGpuiApp {
    pub(crate) fn render_workarea_header_voice_button(
        &self,
        cx: &mut gpui::Context<Self>,
    ) -> impl IntoElement {
        let agent = &self.voice_agent;
        let active = agent.active();
        let stopped = agent.phase == VoicePhase::Ended;
        let icon_color = if stopped {
            rgba(0xfbbf24ff).into()
        } else if active {
            rgba(0x4ade80ff).into()
        } else {
            titlebar_icon_color()
        };
        let tooltip: SharedString = match agent.focus.as_ref() {
            Some(focus) if active => format!("End the voice call about {}", focus.title).into(),
            _ if active => "End the voice conversation".into(),
            _ => titlebar_tooltip_label("Talk to the voice agent", "toggleVoiceAgent"),
        };
        let anchor = agent.menu_anchor.clone();
        let main = div()
            .id("ghostex-gpui-workarea-header-voice")
            .role(gpui::Role::Button)
            .aria_label(tooltip.clone())
            .flex()
            .h_full()
            .items_center()
            .justify_center()
            .pl(px(8.0))
            .pr(px(6.0))
            .rounded_l(titlebar_split_button_segment_radius(
                TITLEBAR_CONTROL_HEIGHT,
            ))
            .when(cfg!(target_os = "windows"), |this| this.occlude())
            .cursor_default()
            .when(active, |this| this.bg(titlebar_active_segment_color()))
            .hover(|this| this.bg(titlebar_split_button_hover_color()))
            .on_mouse_down(
                MouseButton::Left,
                cx.listener(|this, _: &MouseDownEvent, window, cx| {
                    window.prevent_default();
                    cx.stop_propagation();
                    this.toggle_voice_agent_conversation(cx);
                }),
            )
            .on_mouse_down(
                MouseButton::Right,
                cx.listener(|this, _: &MouseDownEvent, window, cx| {
                    window.prevent_default();
                    cx.stop_propagation();
                    this.toggle_voice_agent_menu(cx);
                }),
            )
            .on_a11y_action(gpui::AccessibleAction::Click, {
                let app = cx.entity().downgrade();
                move |_, _, cx| {
                    let _ = app.update(cx, |this, cx| this.toggle_voice_agent_conversation(cx));
                }
            })
            .managed_tooltip_with_placement(ManagedTooltipPlacement::Auto, move |window, cx| {
                titlebar_tooltip(tooltip.clone(), window, cx)
            })
            .child(titlebar_svg_icon(ICON_HEADSET, 15.0, icon_color));
        let caret = div()
            .id("ghostex-gpui-workarea-header-voice-menu")
            .role(gpui::Role::Button)
            .aria_label(SharedString::new_static("Voice call about"))
            .flex()
            .h_full()
            .items_center()
            .justify_center()
            .pl(px(4.0))
            .pr(px(5.0))
            .rounded_r(titlebar_split_button_segment_radius(
                TITLEBAR_CONTROL_HEIGHT,
            ))
            .when(cfg!(target_os = "windows"), |this| this.occlude())
            .cursor_default()
            .hover(|this| this.bg(titlebar_split_button_hover_color()))
            .on_mouse_down(
                MouseButton::Left,
                cx.listener(|this, _: &MouseDownEvent, window, cx| {
                    window.prevent_default();
                    cx.stop_propagation();
                    this.toggle_voice_agent_menu(cx);
                }),
            )
            .on_a11y_action(gpui::AccessibleAction::Click, {
                let app = cx.entity().downgrade();
                move |_, _, cx| {
                    let _ = app.update(cx, |this, cx| this.toggle_voice_agent_menu(cx));
                }
            })
            .managed_tooltip_with_placement(ManagedTooltipPlacement::Auto, |window, cx| {
                titlebar_tooltip("All sessions or this session", window, cx)
            })
            .child(titlebar_svg_icon(
                TITLEBAR_ICON_CHEVRON_DOWN,
                11.0,
                titlebar_icon_color(),
            ));
        titlebar_split_button_frame(
            h_flex().id("ghostex-gpui-workarea-header-voice-split"),
            TITLEBAR_CONTROL_HEIGHT,
        )
        .relative()
        .flex_shrink_0()
        .ml(px(4.0))
        .items_center()
        .overflow_hidden()
        .child(main)
        .child(titlebar_split_button_divider(TITLEBAR_CONTROL_HEIGHT))
        .child(caret)
        .on_prepaint(move |bounds: Bounds<Pixels>, _window, _cx| anchor.set(Some(bounds)))
    }

    /// Opens (or closes) the headset arrow's menu under the Voice button.
    pub(crate) fn toggle_voice_agent_menu(&mut self, cx: &mut gpui::Context<Self>) {
        let (Some(main), Some(trigger)) =
            (self.main_window_handle, self.voice_agent.menu_anchor.get())
        else {
            return;
        };
        let target = self.voice_call_target();
        let all_label = titlebar_tooltip_label("All sessions", "toggleVoiceAgent");
        let rows = voice_menu_rows(self.voice_open_call(), target.as_ref(), &all_label);
        let mut menu = GpuiContextMenu::new();
        for row in rows {
            let action: Box<dyn gpui::Action> = match (row.pick, target.as_ref()) {
                (VoiceMenuPick::ThisSession, Some(target)) => Box::new(VoiceCallAboutSession {
                    remote_machine: target.key.machine.remote_id().map(str::to_string),
                    project_id: target.key.project_id.clone(),
                    session_id: target.key.session_id.clone(),
                    gbot: false,
                }),
                _ => Box::new(VoiceCallAllSessions),
            };
            menu = if row.disabled {
                menu.menu_with_disabled(row.label, true, action)
            } else if row.pick == VoiceMenuPick::ThisSession {
                let label = titlebar_tooltip_label(&row.label, VOICE_CALL_ABOUT_SESSION_HOTKEY);
                menu.menu_with_check(label, row.checked, action)
            } else {
                menu.menu_with_check(row.label, row.checked, action)
            };
        }
        // On a GBot's chat: a call with that GBot (gbot_call.rs), ticked while it is the open call.
        if let Some(target) = target.as_ref()
            && let Some(label) = super::gbot_call::gbot_call_menu_label(target)
        {
            let checked = self.voice_agent.focus.as_ref().is_some_and(|focus| {
                focus.gbot.is_some() && focus.key == target.key
            });
            menu = menu.separator().menu_with_check(
                label,
                checked,
                Box::new(VoiceCallAboutSession {
                    remote_machine: target.key.machine.remote_id().map(str::to_string),
                    project_id: target.key.project_id.clone(),
                    session_id: target.key.session_id.clone(),
                    gbot: true,
                }),
            );
        }
        let app = cx.entity();
        cx.defer(move |cx| {
            let _ = main.update(cx, |_, window, cx| {
                menu.toggle_below_for_app(app, trigger, window, cx);
            });
        });
    }
}
