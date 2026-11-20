//! Meet the rail: the one-time pop-up for someone who had Ghostex before the rail, whose window
//! has just switched to it. What the rail gives them, in four rows with icons, and two buttons:
//! Switch (to Classic, or back to Rail, as often as they like, the window changing behind it) and
//! Done (keeps whichever layout is showing).
//!
//! CDXC:Sidebar 2026-11-20 DECISION:
//! User, for 11.12.0: make the "Meet the rail" pop-up "very beautiful" and "explain the benefits clearly: customizable (pin what you use), drag icons around to reorder, a calmer, less busy sidebar, plus Agents Hub / All Automations / Claude / Grok one click away", in Ghostex's own look, glass-aware, dark and light. Then, replacing the toggle, preview and Accept / Keep Classic of the same day: "existing users are switched to the Rail automatically (not left on Classic), and the pop-up explains it. Only TWO buttons: 'Switch' and 'Done'. Switch flips the layout live (Rail <-> Classic) and can be clicked as many times as they like; make its label say clearly where it goes ('Switch to Classic' / 'Switch to Rail'). Done closes and keeps whichever layout is showing. No preview inside the pop-up". And: "make sure the pop-up shows only ONCE per install" (app/sidebar_rail/intro.rs).
//! SEE-ALSO: app/sidebar_rail/intro.rs (when it opens, what its buttons do, analytics), bin/native_modal_demo/rail_intro.rs (the preview).

use super::native_modal_kit::*;
use gpui::{
    AnyElement, App, Context, FocusHandle, FontWeight, IntoElement, KeyDownEvent,
    ParentElement as _, Render, Styled as _, Window, div, px,
};
use gpui_component::{h_flex, v_flex};
use std::rc::Rc;

pub(crate) const RAIL_INTRO_MODAL_WIDTH: f32 = 520.0;
pub(crate) const RAIL_INTRO_MODAL_INITIAL_HEIGHT: f32 = 560.0;

const TITLE: &str = "Meet the rail";
const DESCRIPTION_RAIL: &str = "Your window now has the rail: a slim column of icons at its left edge for what you open most. It stays put when you hide the sidebar.";
const DESCRIPTION_CLASSIC: &str = "You're on Classic, the sidebar without the rail. Switch back to try the rail again, or keep Classic.";
const SETTINGS_HINT: &str = "You can switch any time in Settings > Sidebar > Sidebar layout.";

/// The benefit rows: icon, title, line.
const BENEFITS: [(&str, &str, &str); 4] = [
    (
        "titlebar/pin-outline.svg",
        "Pin what you use",
        "Everything lives in the rail's ⋯ menu. Pin what you reach for and unpin the rest.",
    ),
    (
        "titlebar/arrows-sort.svg",
        "Drag icons into your order",
        "Move icons up and down. Anything you open from ⋯ shows on the rail while it's open.",
    ),
    (
        "titlebar/layout-sidebar.svg",
        "A calmer sidebar",
        "Search, the bell and Hermes move to the rail, so the sidebar is just your projects and sessions.",
    ),
    (
        "titlebar/users-group.svg",
        "Hubs and agents one click away",
        "Agents Hub, All Automations, Claude and Grok open across the whole window, even with the sidebar hidden.",
    ),
];

/// What the pop-up asks the app to do.
pub(crate) enum RailIntroCommand {
    /// Switch: show `rail` (true) or Classic in the window now.
    Switch { rail: bool },
    /// Done, Escape or its corner: keep the layout on screen.
    Done { rail: bool },
}

pub(crate) type RailIntroHost = Rc<dyn Fn(RailIntroCommand, &mut App)>;

pub(crate) struct RailIntroModalConfig {
    pub(crate) palette: ModalPalette,
    /// The layout the window shows as the pop-up opens.
    pub(crate) rail: bool,
}

pub(crate) struct GpuiRailIntroModalWindow {
    host: RailIntroHost,
    palette: ModalPalette,
    rail: bool,
    fit: ModalFit,
    focus_handle: FocusHandle,
}

impl GpuiRailIntroModalWindow {
    pub(crate) fn new(
        config: RailIntroModalConfig,
        host: RailIntroHost,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Self {
        let focus_handle = cx.focus_handle();
        focus_handle.focus(window, cx);
        Self {
            host,
            palette: config.palette,
            rail: config.rail,
            fit: ModalFit::new(),
            focus_handle,
        }
    }

    fn switch(&mut self, cx: &mut Context<Self>) {
        self.rail = !self.rail;
        (self.host)(RailIntroCommand::Switch { rail: self.rail }, cx);
        cx.notify();
    }

    fn done(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        window.remove_window();
        (self.host)(RailIntroCommand::Done { rail: self.rail }, cx);
    }

    fn on_key_down(&mut self, event: &KeyDownEvent, window: &mut Window, cx: &mut Context<Self>) {
        match event.keystroke.key.as_str() {
            "escape" | "enter" => self.done(window, cx),
            _ => return,
        }
        cx.stop_propagation();
    }

    /// The rail's mark over the title: the layout icon on a raised tile.
    fn render_mark(&self) -> AnyElement {
        let p = self.palette;
        div()
            .size(px(44.0))
            .rounded(px(12.0))
            .bg(hsla(p.raised))
            .border_1()
            .border_color(hsla(p.hairline))
            .flex()
            .items_center()
            .justify_center()
            .child(modal_icon(
                if self.rail {
                    "titlebar/layout-sidebar-left-expand.svg"
                } else {
                    "titlebar/layout-sidebar.svg"
                },
                22.0,
                p.foreground,
            ))
            .into_any_element()
    }

    fn render_benefits(&self) -> AnyElement {
        let p = self.palette;
        v_flex()
            .w_full()
            .rounded(px(12.0))
            .bg(hsla(p.panel))
            .border_1()
            .border_color(hsla(p.hairline))
            .p(px(14.0))
            .gap(px(14.0))
            .children(BENEFITS.iter().map(|(icon, title, line)| {
                h_flex()
                    .w_full()
                    .items_start()
                    .gap(px(12.0))
                    .child(
                        div()
                            .flex_shrink_0()
                            .size(px(32.0))
                            .rounded(px(9.0))
                            .bg(hsla(p.raised))
                            .border_1()
                            .border_color(hsla(p.hairline))
                            .flex()
                            .items_center()
                            .justify_center()
                            .child(modal_icon(icon, 16.0, p.foreground)),
                    )
                    .child(
                        v_flex()
                            .min_w_0()
                            .flex_1()
                            .gap(px(2.0))
                            .child(
                                div()
                                    .text_size(px(14.0))
                                    .line_height(px(19.0))
                                    .font_weight(FontWeight::MEDIUM)
                                    .child(*title),
                            )
                            .child(
                                div()
                                    .text_size(px(12.5))
                                    .line_height(px(18.0))
                                    .text_color(hsla(p.muted))
                                    .child(*line),
                            ),
                    )
            }))
            .into_any_element()
    }
}

impl Render for GpuiRailIntroModalWindow {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let p = self.palette;
        let description = if self.rail {
            DESCRIPTION_RAIL
        } else {
            DESCRIPTION_CLASSIC
        };
        let content = vec![
            v_flex()
                .w_full()
                .gap(px(14.0))
                .child(self.render_mark())
                .child(
                    v_flex()
                        .gap(px(6.0))
                        .child(
                            div()
                                .text_size(px(21.0))
                                .line_height(px(27.0))
                                .font_weight(FontWeight::SEMIBOLD)
                                .child(TITLE),
                        )
                        .child(
                            div()
                                .text_size(px(13.5))
                                .line_height(px(20.5))
                                .text_color(hsla(p.muted))
                                .child(description),
                        ),
                )
                .into_any_element(),
            // One body under the header: the dialog fit counts one gap between its two children.
            v_flex()
                .w_full()
                .gap(px(16.0))
                .child(self.render_benefits())
                .child(modal_hint(&p, SETTINGS_HINT))
                .into_any_element(),
        ];
        let switch = modal_action_button(
            &p,
            "rail-intro-switch",
            if self.rail {
                "Switch to Classic"
            } else {
                "Switch to Rail"
            },
            None,
            ModalButtonTone::Neutral,
            false,
            |this: &mut Self, _window, cx| this.switch(cx),
            cx,
        );
        let done = modal_action_button(
            &p,
            "rail-intro-done",
            "Done",
            None,
            ModalButtonTone::Primary,
            false,
            |this: &mut Self, window, cx| this.done(window, cx),
            cx,
        );
        modal_shell(
            &p,
            "ghostex-gpui-rail-intro",
            &self.focus_handle,
            &self.fit,
            Self::on_key_down,
            content,
            modal_footer(vec![switch, done]),
            None,
            cx,
        )
    }
}

impl ModalCornerClose for GpuiRailIntroModalWindow {
    fn close_from_corner(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.done(window, cx);
    }
}
