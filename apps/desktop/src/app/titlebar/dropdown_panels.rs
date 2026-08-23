// C1 wave-4 deferred split: apps/desktop/src/app/titlebar.rs (~3.9k lines)
// further divided into responsibility-scoped submodules, pure move (the
// only edit from the original app/titlebar.rs body is wrapping each group
// of `impl GhostexGpuiApp` methods in its own impl block; multiple impl
// blocks for the same type across files is the established pattern used by
// every sibling file in apps/desktop/src/app/). This file holds the anchored dropdown panel, tips popover, and resources popover renderers.
// See docs/2026-08-22/repo-restructure/SPLITS.md C1.

// C1 wave-4 extraction: `impl GhostexGpuiApp` methods moved verbatim out of
// main.rs (pure move; the only edit is the `pub(crate) ` visibility prefix the
// cross-module split requires). See docs/2026-08-22/repo-restructure/SPLITS.md C1.
//
// Cluster: titlebar menus, popups, actions, and titlebar render_* builders

// RefCell backs cross-platform runtime state (window frame persistence), not
// just the macOS-only shims that first introduced the import.

use gpui::Anchor;
use gpui::AnyElement;
use gpui::Bounds;
use gpui::Hsla;
use gpui::InteractiveElement as _;
use gpui::IntoElement;
use gpui::MouseButton;
use gpui::MouseDownEvent;
use gpui::ParentElement as _;
use gpui::Pixels;
use gpui::Point;
use gpui::Styled as _;
use gpui::Window;
use gpui::anchored;
use gpui::deferred;
use gpui::div;
use gpui::point;
use gpui::prelude::FluentBuilder as _;
use gpui::px;
use gpui::rgb;
use gpui_component::ElementExt;
use gpui_component::Selectable;
use gpui_component::h_flex;
use gpui_component::tooltip::ManagedTooltipExt as _;
use gpui_component::tooltip::ManagedTooltipPlacement;
use gpui_component::v_flex;

use crate::app::actions::*;
use crate::app::consts::*;
use crate::app::helpers::*;
use crate::app::window::*;
use crate::*;

impl GhostexGpuiApp {
    pub(crate) fn render_titlebar_anchored_dropdown_panel(
        &self,
        id: &'static str,
        width: f32,
        open: bool,
        position: Point<Pixels>,
        trigger_bounds: Bounds<Pixels>,
        close: fn(&mut Self, &mut Window, &mut gpui::Context<Self>),
        child: impl IntoElement + 'static,
        cx: &mut gpui::Context<Self>,
    ) -> AnyElement {
        if !open {
            return div().size_0().into_any_element();
        }

        deferred(
            anchored()
                .anchor(Anchor::TopRight)
                .position(position)
                .child(
                    div()
                        .id(id)
                        .occlude()
                        .tab_group()
                        .key_context(TITLEBAR_DROPDOWN_KEY_CONTEXT)
                        .track_focus(&self.titlebar_dropdown_focus_handle)
                        .w(px(width))
                        .h(px(TITLEBAR_DROPDOWN_READING_PANEL_HEIGHT))
                        .overflow_hidden()
                        .rounded(px(2.0))
                        .border_1()
                        .border_color(titlebar_popup_menu_border_color())
                        .bg(titlebar_popup_menu_background())
                        .on_action(cx.listener(
                            move |this, _: &TitlebarDropdownCancel, window, cx| {
                                close(this, window, cx);
                            },
                        ))
                        .on_mouse_down_out(cx.listener(
                            move |this, event: &MouseDownEvent, window, cx| {
                                // A mouse-down on the trigger button is the
                                // button's own toggle-close; closing here too
                                // made the toggle reopen the panel instead.
                                if trigger_bounds.contains(&event.position) {
                                    return;
                                }
                                close(this, window, cx);
                            },
                        ))
                        .child(child),
                ),
        )
        .with_priority(1)
        .into_any_element()
    }

    #[allow(dead_code)] // no caller: Tips is a titlebar-host CEF panel now; this is the superseded native popover
    pub(crate) fn render_titlebar_tips_popover(
        &self,
        window: &mut Window,
        cx: &mut gpui::Context<Self>,
    ) -> impl IntoElement {
        let state =
            window.use_keyed_state("ghostex-gpui-titlebar-tips-dropdown-anchor", cx, |_, _| {
                GpuiTitlebarAnchoredDropdownState::default()
            });
        let tips_open = self.titlebar_tips_panel_open;
        let panel = self.titlebar_tips_panel.clone();
        let anchor_position = state.read(cx).position;
        let trigger_bounds_captured = state.read(cx).trigger_bounds_captured;

        div()
            .id("ghostex-gpui-titlebar-tips-popover")
            .child(self.render_titlebar_tips_trigger().selected(tips_open))
            .when(!tips_open, |this| {
                this.managed_tooltip_with_placement(ManagedTooltipPlacement::Left, |window, cx| {
                    titlebar_tooltip(TITLEBAR_TIPS_TOOLTIP, window, cx)
                })
            })
            .on_mouse_down(
                MouseButton::Left,
                cx.listener(move |this, _event: &MouseDownEvent, window, cx| {
                    window.prevent_default();
                    cx.stop_propagation();
                    this.set_gpui_titlebar_tips_panel_open(!tips_open, window, cx);
                }),
            )
            .on_prepaint({
                let state = state.clone();
                move |bounds, window, cx| {
                    let trigger_right_x = bounds.top_right().x.as_f32();
                    let horizontal_margin = 8.0;
                    let min_right_edge = TITLEBAR_DROPDOWN_TIPS_PANEL_WIDTH + horizontal_margin;
                    let max_right_edge = (window.viewport_size().width.as_f32()
                        - horizontal_margin)
                        .max(min_right_edge);
                    let right_edge = trigger_right_x.clamp(min_right_edge, max_right_edge);
                    let next_position = point(px(right_edge), px(TITLEBAR_HEIGHT));
                    let request_frame = state.update(cx, |state, _| {
                        let first_capture = !state.trigger_bounds_captured;
                        let moved = state.position != next_position;
                        state.position = next_position;
                        state.trigger_bounds = bounds;
                        state.trigger_bounds_captured = true;
                        first_capture || moved
                    });
                    if request_frame {
                        window.request_animation_frame();
                    }
                }
            })
            .child(
                self.render_titlebar_anchored_dropdown_panel(
                    "ghostex-gpui-titlebar-tips-dropdown-panel",
                    TITLEBAR_DROPDOWN_TIPS_PANEL_WIDTH,
                    tips_open && trigger_bounds_captured,
                    anchor_position,
                    state.read(cx).trigger_bounds,
                    Self::close_gpui_titlebar_tips_dropdown,
                    div()
                        .size_full()
                        .when_some(panel, |this, panel| this.child(panel)),
                    cx,
                ),
            )
    }

    pub(crate) fn close_gpui_titlebar_tips_dropdown(
        &mut self,
        window: &mut Window,
        cx: &mut gpui::Context<Self>,
    ) {
        self.set_gpui_titlebar_tips_panel_open(false, window, cx);
    }

    #[allow(dead_code)] // no caller: Resources is a titlebar-host CEF panel now; this is the superseded native popover
    pub(crate) fn render_titlebar_resources_popover(
        &self,
        window: &mut Window,
        cx: &mut gpui::Context<Self>,
    ) -> impl IntoElement {
        let state = window.use_keyed_state(
            "ghostex-gpui-titlebar-resources-dropdown-anchor",
            cx,
            |_, _| GpuiTitlebarAnchoredDropdownState::default(),
        );
        let resources_open = self.titlebar_resources_panel_open;
        let resources_ready = self.titlebar_resources_panel_ready;
        let panel = self.titlebar_resources_panel.clone();
        let anchor_position = state.read(cx).position;
        let trigger_bounds_captured = state.read(cx).trigger_bounds_captured;

        div()
            .id("ghostex-gpui-titlebar-resources-popover")
            .child(self.render_titlebar_icon_button(
                "resources",
                TITLEBAR_ICON_DEVICE_DESKTOP,
                16.0,
                false,
                cx,
            ))
            .when(!resources_open, |this| {
                this.managed_tooltip_with_placement(ManagedTooltipPlacement::Left, |window, cx| {
                    titlebar_tooltip(TITLEBAR_RESOURCES_TOOLTIP, window, cx)
                })
            })
            .on_prepaint({
                let state = state.clone();
                move |bounds, window, cx| {
                    let trigger_right_x = bounds.top_right().x.as_f32();
                    let horizontal_margin = 8.0;
                    let min_right_edge =
                        TITLEBAR_DROPDOWN_RESOURCES_PANEL_WIDTH + horizontal_margin;
                    let max_right_edge = (window.viewport_size().width.as_f32()
                        - horizontal_margin)
                        .max(min_right_edge);
                    let right_edge = trigger_right_x.clamp(min_right_edge, max_right_edge);
                    let next_position = point(px(right_edge), px(TITLEBAR_HEIGHT));
                    let request_frame = state.update(cx, |state, _| {
                        let first_capture = !state.trigger_bounds_captured;
                        let moved = state.position != next_position;
                        state.position = next_position;
                        state.trigger_bounds = bounds;
                        state.trigger_bounds_captured = true;
                        first_capture || moved
                    });
                    if request_frame {
                        window.request_animation_frame();
                    }
                }
            })
            .child(
                self.render_titlebar_anchored_dropdown_panel(
                    "ghostex-gpui-titlebar-resources-dropdown-panel",
                    TITLEBAR_DROPDOWN_RESOURCES_PANEL_WIDTH,
                    resources_open && trigger_bounds_captured,
                    anchor_position,
                    state.read(cx).trigger_bounds,
                    Self::close_gpui_titlebar_resources_dropdown,
                    div()
                        .relative()
                        .size_full()
                        .when_some(panel, |this, panel| this.child(panel))
                        .when(!resources_ready, |this| {
                            this.child(
                                div()
                                    .absolute()
                                    .top_0()
                                    .right_0()
                                    .bottom_0()
                                    .left_0()
                                    .child(Self::render_titlebar_resources_loading_skeleton()),
                            )
                        }),
                    cx,
                ),
            )
    }

    pub(crate) fn render_titlebar_resources_loading_skeleton() -> impl IntoElement {
        /*
        CDXC:GPUIResourcesInstantOpen 2026-07-11:
        CEF browser creation and the first Resources process sample must not
        delay the dropdown itself. GPUI owns this immediate placeholder in the
        same non-overlapping content frame; the hidden CEF child replaces it
        only after React reports ready.
        */
        let skeleton_fill: Hsla = rgb(0xffffff).opacity(0.08).into();
        let skeleton_border: Hsla = rgb(0xffffff).opacity(0.06).into();

        v_flex()
            .size_full()
            .p(px(14.0))
            .gap(px(12.0))
            .bg(titlebar_popup_menu_background())
            .child(
                h_flex()
                    .items_center()
                    .justify_between()
                    .child(
                        div()
                            .w(px(128.0))
                            .h(px(18.0))
                            .rounded(px(3.0))
                            .bg(skeleton_fill),
                    )
                    .child(
                        div()
                            .w(px(210.0))
                            .h(px(28.0))
                            .rounded(px(3.0))
                            .bg(skeleton_fill),
                    ),
            )
            .child(
                div()
                    .w(px(96.0))
                    .h(px(10.0))
                    .rounded(px(2.0))
                    .bg(skeleton_fill),
            )
            .children((0..5).map(move |_| {
                h_flex()
                    .items_center()
                    .gap(px(12.0))
                    .h(px(46.0))
                    .px(px(12.0))
                    .border_1()
                    .border_color(skeleton_border)
                    .child(div().size(px(28.0)).rounded(px(2.0)).bg(skeleton_fill))
                    .child(
                        v_flex()
                            .flex_1()
                            .gap(px(6.0))
                            .child(
                                div()
                                    .w(px(190.0))
                                    .h(px(10.0))
                                    .rounded(px(2.0))
                                    .bg(skeleton_fill),
                            )
                            .child(
                                div()
                                    .w(px(126.0))
                                    .h(px(8.0))
                                    .rounded(px(2.0))
                                    .bg(skeleton_fill),
                            ),
                    )
                    .child(
                        div()
                            .w(px(84.0))
                            .h(px(24.0))
                            .rounded(px(2.0))
                            .bg(skeleton_fill),
                    )
                    .child(
                        div()
                            .w(px(92.0))
                            .h(px(24.0))
                            .rounded(px(2.0))
                            .bg(skeleton_fill),
                    )
            }))
    }

    pub(crate) fn close_gpui_titlebar_resources_dropdown(
        &mut self,
        window: &mut Window,
        cx: &mut gpui::Context<Self>,
    ) {
        self.set_gpui_titlebar_resources_panel_open(false, window, cx);
    }

    pub(crate) fn render_titlebar_tips_trigger(&self) -> GpuiTitlebarTipsTrigger {
        GpuiTitlebarTipsTrigger::new(self.titlebar_tips_badge_count() > 0)
    }

    pub(crate) fn titlebar_tips_badge_count(&self) -> u64 {
        /*
        CDXC:GPUITitlebarTipsBadge 2026-07-04-03:00:
        The GPUI strip stores only the last unread tip count sampled from the
        shared React titlebar panel's own localStorage key, plus notice facts
        Rust already owns through shared Settings. The titlebar must not keep a
        second read-id store, duplicate tip rows in UI, or infer notices from
        project/session labels.
        */
        let settings_snapshot = shared_settings::shared_sidebar_settings_snapshot();
        let notice_count = u64::from(settings_snapshot.debugging_mode())
            + u64::from(
                gpui_titlebar_session_persistence_provider_from_settings(
                    settings_snapshot.object(),
                ) == "off",
            );
        gpui_titlebar_tips_unread_count_from_settings().saturating_add(notice_count)
    }

}
