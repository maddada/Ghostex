// C1 wave-4 re-cluster: further split out of app/render.rs (~7,340
// lines, itself moved verbatim out of main.rs) into descriptively named
// modules; pure move, no logic changes. Cluster: titlebar shell (project slot, sidebar collapse, mode switcher/dropdown, mode tab) and the cross-platform titlebar double-click zoom action.

use gpui::FontWeight;
use gpui::InteractiveElement as _;
use gpui::IntoElement;
use gpui::MouseButton;
use gpui::MouseDownEvent;
use gpui::ObjectFit;
use gpui::ParentElement as _;
use gpui::StatefulInteractiveElement as _;
use gpui::Styled as _;
use gpui::StyledImage as _;
use gpui::Window;
use gpui::WindowControlArea;
use gpui::div;
use gpui::img;
use gpui::prelude::FluentBuilder as _;
use gpui::px;
use gpui_component::h_flex;
use gpui_component::tooltip::ManagedTooltipExt as _;
use gpui_component::tooltip::ManagedTooltipPlacement;

use crate::app::consts::*;
use crate::app::helpers::*;
use crate::app::model::*;
use crate::*;

/*
CDXC:GPUITitlebarDoubleClick 2026-08-23:
GPUI paints the whole titlebar strip itself, so AppKit's own titlebar view
never sees a double click there and the standard macOS zoom gesture silently
did nothing. Forward it to the platform window, which honours the user's
NSGlobalDomain AppleActionOnDoubleClick preference (Maximize/Fill/Minimize/
Do Nothing). Linux compositors leave the same gesture to the client, so zoom
directly there; Windows already resolves it from the WindowControlArea::Drag
hit test in the platform layer.
*/
#[cfg(target_os = "macos")]
fn gpui_titlebar_double_click_window_action(window: &Window) {
    window.titlebar_double_click();
}

#[cfg(target_os = "linux")]
fn gpui_titlebar_double_click_window_action(window: &Window) {
    window.zoom_window();
}

#[cfg(target_os = "windows")]
fn gpui_titlebar_double_click_window_action(_window: &Window) {}

impl GhostexGpuiApp {
    pub(crate) fn render_titlebar(
        &self,
        window: &mut Window,
        cx: &mut gpui::Context<Self>,
    ) -> impl IntoElement {
        /*
        CDXC:GPUITitlebar 2026-06-14-16:47:
        The GPUI titlebar mirrors the macOS app: native traffic lights, passive project identity, full-width mode tabs for Agents/Source/Browser/Kanban/Automate/Docs, a compact mode dropdown below 1050px, and right-side icon buttons.

        CDXC:GPUTitlebarAvailability 2026-07-04-01:00:
        Quick/projectless GPUI contexts keep Agents and Source selectable, keep Browser, Kanban, Automate, and Docs visible but disabled, and use the same availability helper for tabs, the compact dropdown, hotkeys, restore, and persistence.

        CDXC:GPUITitlebarParity 2026-06-22-19:39:
        The GPUI titlebar must match the current macOS titlebar chrome: the sidebar toggle is a flat Tabler layout-sidebar glyph instead of the older blue circular chevron, and the right controls are the same project/window actions as macOS: Tips, Resources, Git, Actions, and Open In. Settings and Keep Awake live in sidebar shortcut chrome, not this titlebar strip.
        */
        let show_mode_switcher = !self.titlebar_mode_switcher_items().is_empty();
        let use_compact_mode_dropdown = show_mode_switcher
            && window.bounds().size.width.as_f32() < TITLEBAR_COMPACT_MODE_WIDTH_THRESHOLD;
        div()
            .id("ghostex-gpui-titlebar")
            .relative()
            .flex_shrink_0()
            .w_full()
            .h(px(TITLEBAR_HEIGHT))
            .bg(titlebar_gradient_fill())
            .text_color(titlebar_text_color())
            .font_family("Inter Variable")
            .line_height(px(TITLEBAR_CONTROL_HEIGHT))
            .window_control_area(WindowControlArea::Drag)
            .on_click(|event, window, _cx| {
                if event.click_count() != 2 {
                    return;
                }
                gpui_titlebar_double_click_window_action(window);
            })
            .on_mouse_down(
                MouseButton::Right,
                cx.listener(|this, event: &MouseDownEvent, window, cx| {
                    window.prevent_default();
                    cx.stop_propagation();
                    this.show_gpui_titlebar_customize_menu(event.position, window, cx);
                }),
            )
            .child(
                h_flex()
                    .h_full()
                    .w_full()
                    .items_center()
                    .justify_center()
                    .when(show_mode_switcher && !use_compact_mode_dropdown, |this| {
                        this.child(self.render_mode_switcher(cx))
                    }),
            )
            .child(self.render_project_slot(use_compact_mode_dropdown, cx))
            .child(self.render_right_titlebar_controls(window, cx))
    }

    pub(crate) fn render_project_slot(
        &self,
        show_compact_mode_dropdown: bool,
        cx: &mut gpui::Context<Self>,
    ) -> impl IntoElement {
        let project_icon = self
            .latest_sidebar_project_snapshot
            .as_ref()
            .and_then(|snapshot| snapshot.project_icon_data_url.as_deref())
            .and_then(gpui_project_icon_image_from_data_url);
        h_flex()
            .absolute()
            .left(px(TITLEBAR_PROJECT_LEFT))
            .top(px(1.0))
            .h(px(TITLEBAR_CONTROL_HEIGHT))
            .max_w(px(620.0))
            .min_w_0()
            .items_center()
            .window_control_area(WindowControlArea::Drag)
            .child(self.render_sidebar_collapse_button(cx))
            .when(self.update_available || self.update_downloading, |this| {
                this.child(self.render_titlebar_update_button(cx))
            })
            /*
            CDXC:NavigationHistory 2026-08-19:
            Back/Forward sit LEFT of the project name, next to the sidebar
            toggle. Placing them after the name made them slide horizontally
            every time the active project's title changed length, which is
            exactly the kind of moving target a frequently clicked control must
            not be.
            */
            .child(self.render_titlebar_navigation_history_buttons(cx))
            .child(
                h_flex()
                    .h(px(TITLEBAR_CONTROL_HEIGHT))
                    .max_w(px(210.0))
                    .min_w_0()
                    .items_center()
                    .overflow_hidden()
                    .whitespace_nowrap()
                    .px(px(3.0))
                    .ml(px(5.0))
                    .mt(px(2.0))
                    .text_size(px(13.5))
                    .font_weight(FontWeight::SEMIBOLD)
                    .line_height(px(TITLEBAR_CONTROL_HEIGHT))
                    .text_color(titlebar_project_text_color())
                    .when_some(project_icon, |this, image| {
                        this.child(
                            img(image)
                                .size(px(16.0))
                                .mr(px(6.0))
                                .flex_shrink_0()
                                .rounded(px(4.0))
                                .object_fit(ObjectFit::Fill),
                        )
                    })
                    .child(self.project_name.clone()),
            )
            .when(show_compact_mode_dropdown, |this| {
                this.child(self.render_compact_mode_dropdown(cx))
            })
    }

    pub(crate) fn render_sidebar_collapse_button(&self, cx: &mut gpui::Context<Self>) -> impl IntoElement {
        /*
        CDXC:GPUITitlebarParity 2026-06-22-19:39:
        The visible sidebar toggle should match the macOS React titlebar's current flat layout-sidebar icon. Keep its 29px GPUI hit target 11px away from the native traffic lights (widened from 3px on 2026-08-23; macOS only, Windows/Linux keep their own frame). Do not render the old blue circular chevron visual.

        CDXC:GPUISidebarCollapse 2026-06-26-10:04:
        The GPUI titlebar sidebar button toggles the same in-shell collapsed chrome state as Cmd+B and the shared command-palette action. Collapse hides the sidebar and divider siblings without writing sidebarWidth, so the user's expanded width is restored on the next toggle.

        Windows and Linux do not have traffic lights to clear. Their collapse
        control uses the same full-height 42px segmented frame as Open In,
        mirrored with a trailing divider, and remains inside the 9px titlebar
        inset instead of extending past the window edge.
        */
        let icon = match self.sidebar_side {
            GpuiSidebarSide::Left => TITLEBAR_ICON_LAYOUT_SIDEBAR,
            GpuiSidebarSide::Right => TITLEBAR_ICON_LAYOUT_SIDEBAR_RIGHT,
        };
        let button = div()
            .id("ghostex-gpui-sidebar-collapse")
            .relative()
            .flex()
            .items_center()
            .justify_center()
            .cursor_default()
            .hover(|this| this.bg(titlebar_button_hover_color()))
            .on_mouse_down(
                MouseButton::Left,
                cx.listener(|this, _, window, cx| {
                    window.prevent_default();
                    cx.stop_propagation();
                    this.toggle_gpui_sidebar_collapsed(cx);
                }),
            )
            .managed_tooltip_with_placement(ManagedTooltipPlacement::Right, |window, cx| {
                titlebar_tooltip("Collapse Sidebar", window, cx)
            });
        #[cfg(target_os = "macos")]
        let button = button
            .h(px(TITLEBAR_CONTROL_HEIGHT))
            .w(px(29.0))
            .ml(px(-5.0))
            .flex_shrink_0()
            .rounded(px(5.0))
            .child(
                div()
                    .flex()
                    .ml(px(TITLEBAR_SIDEBAR_COLLAPSE_ICON_LEFT_OFFSET))
                    .mt(px(TITLEBAR_SIDEBAR_COLLAPSE_ICON_TOP_OFFSET))
                    .items_center()
                    .justify_center()
                    .child(titlebar_svg_icon(
                        icon,
                        TITLEBAR_SIDEBAR_COLLAPSE_ICON_SIZE,
                        titlebar_active_text_color(),
                    )),
            );
        #[cfg(not(target_os = "macos"))]
        let button = button
            .h(px(TITLEBAR_CONTROL_HEIGHT))
            .w(px(TITLEBAR_BUTTON_WIDTH))
            .border_r_1()
            .border_color(titlebar_button_border_color())
            .child(titlebar_svg_icon(
                icon,
                TITLEBAR_SIDEBAR_COLLAPSE_ICON_SIZE,
                titlebar_icon_color(),
            ));
        button
    }

    pub(crate) fn render_mode_switcher(&self, cx: &mut gpui::Context<Self>) -> impl IntoElement {
        let items = self.titlebar_mode_switcher_items();
        let mode_count = items.len();

        let mut switcher = h_flex()
            .id("ghostex-gpui-titlebar-mode-switcher")
            .relative()
            .h(px(TITLEBAR_CONTROL_HEIGHT))
            .items_center();
        for (index, item) in items.into_iter().enumerate() {
            switcher = switcher.child(self.render_mode_tab(
                item.mode,
                item.mode.display_label(),
                index + 1 == mode_count,
                item.is_available,
                item.disabled_reason,
                cx,
            ));
        }
        switcher
    }

    pub(crate) fn render_compact_mode_dropdown(&self, cx: &mut gpui::Context<Self>) -> impl IntoElement {
        let label = self.active_mode.display_label();
        h_flex()
            .id("ghostex-gpui-titlebar-compact-mode-dropdown")
            .h(px(25.0))
            .min_w(px(108.0))
            .ml(px(7.0))
            .mt(px(2.0))
            .items_center()
            .justify_center()
            .gap(px(7.0))
            .rounded(px(5.0))
            .border_1()
            .border_color(titlebar_button_border_color())
            .px(px(9.0))
            .text_size(px(12.5))
            .font_weight(FontWeight::NORMAL)
            .line_height(px(25.0))
            .text_color(titlebar_active_text_color())
            .cursor_default()
            .hover(|this| this.bg(titlebar_button_hover_color()))
            .on_mouse_down(
                MouseButton::Left,
                cx.listener(|this, event: &MouseDownEvent, window, cx| {
                    window.prevent_default();
                    cx.stop_propagation();
                    this.show_gpui_titlebar_mode_menu(event.position, window, cx);
                }),
            )
            .on_mouse_down(
                MouseButton::Right,
                cx.listener(|this, event: &MouseDownEvent, window, cx| {
                    window.prevent_default();
                    cx.stop_propagation();
                    this.show_gpui_titlebar_customize_menu(event.position, window, cx);
                }),
            )
            .child(label)
            .child(titlebar_svg_icon(
                TITLEBAR_ICON_CHEVRON_DOWN,
                12.0,
                titlebar_icon_color(),
            ))
    }

    pub(crate) fn render_mode_tab(
        &self,
        mode: TitlebarMode,
        label: &'static str,
        is_last: bool,
        is_available: bool,
        disabled_reason: Option<&'static str>,
        cx: &mut gpui::Context<Self>,
    ) -> impl IntoElement {
        let is_active = is_available && self.active_mode == mode;
        /*
        CDXC:GPUTitlebarAvailability 2026-07-04-01:00:
        Disabled Quick/projectless tabs remain normal titlebar segments with a hover reason and no separate hit target. Browser, Kanban, Automate, and Docs share the native disabled reason while click handling still calls the central availability guard before changing active workspace mode.
        */
        div()
            .id(format!("ghostex-gpui-titlebar-mode-{label}"))
            .relative()
            .flex()
            .h(px(TITLEBAR_CONTROL_HEIGHT))
            .min_w(px(70.0))
            .items_center()
            .justify_center()
            .border_l_1()
            .when(is_last, |this| this.border_r_1())
            .border_color(titlebar_button_border_color())
            .px(px(14.0))
            .text_size(px(13.55))
            .font_weight(FontWeight::NORMAL)
            .line_height(px(TITLEBAR_CONTROL_HEIGHT))
            .text_color(if !is_available {
                titlebar_disabled_text_color()
            } else if is_active {
                titlebar_active_text_color()
            } else {
                titlebar_inactive_text_color()
            })
            .cursor_default()
            .when(is_active, |this| this.bg(titlebar_active_segment_color()))
            .when(!is_available, |this| {
                this.bg(titlebar_disabled_segment_color())
            })
            .hover(move |this| {
                if !is_available {
                    return this;
                }
                let this = this.text_color(titlebar_active_text_color());
                if is_active {
                    this.bg(titlebar_active_segment_color())
                } else {
                    this.bg(titlebar_button_hover_color())
                }
            })
            .on_mouse_down(
                MouseButton::Left,
                cx.listener(move |this, _event: &gpui::MouseDownEvent, window, cx| {
                    window.prevent_default();
                    cx.stop_propagation();
                    if is_available && this.set_active_mode(mode, window, cx) {
                        cx.notify();
                    }
                }),
            )
            .on_mouse_down(
                MouseButton::Right,
                cx.listener(|this, event: &MouseDownEvent, window, cx| {
                    window.prevent_default();
                    cx.stop_propagation();
                    this.show_gpui_titlebar_customize_menu(event.position, window, cx);
                }),
            )
            .when_some(disabled_reason, |this, reason| {
                this.managed_tooltip_with_placement(
                    ManagedTooltipPlacement::Right,
                    move |window, cx| titlebar_tooltip(reason, window, cx),
                )
            })
            .child(label)
    }

}
