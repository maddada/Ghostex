// C1 wave-4 deferred split: apps/desktop/src/app/titlebar.rs (~3.9k lines)
// further divided into responsibility-scoped submodules, pure move (the
// only edit from the original app/titlebar.rs body is wrapping each group
// of `impl GhostexGpuiApp` methods in its own impl block; multiple impl
// blocks for the same type across files is the established pattern used by
// every sibling file in apps/desktop/src/app/). This file holds the actions and open-targets titlebar button renderers plus their popup content-height helpers.
// See docs/2026-08-22/repo-restructure/SPLITS.md C1.

// C1 wave-4 extraction: `impl GhostexGpuiApp` methods moved verbatim out of
// main.rs (pure move; the only edit is the `pub(crate) ` visibility prefix the
// cross-module split requires). See docs/2026-08-22/repo-restructure/SPLITS.md C1.
//
// Cluster: titlebar menus, popups, actions, and titlebar render_* builders

use std::time::Duration;

// RefCell backs cross-platform runtime state (window frame persistence), not
// just the macOS-only shims that first introduced the import.

use gpui::AnyElement;
use gpui::InteractiveElement as _;
use gpui::IntoElement;
use gpui::MouseButton;
use gpui::MouseDownEvent;
use gpui::ParentElement as _;
use gpui::Styled as _;
use gpui::Window;
use gpui::div;
use gpui::prelude::FluentBuilder as _;
use gpui::px;
use gpui_component::ElementExt;
use gpui_component::tooltip::ManagedTooltipExt as _;
use gpui_component::tooltip::ManagedTooltipPlacement;

use crate::app::consts::*;
use crate::app::helpers::*;
use crate::app::window::*;
use crate::*;

impl GhostexGpuiApp {
    pub(crate) fn render_titlebar_actions_button(
        &self,
        icon_path: &'static str,
        window: &mut Window,
        cx: &mut gpui::Context<Self>,
    ) -> AnyElement {
        let open = self.titlebar_popup_menu_open(GpuiTitlebarPopupKind::Actions);
        let on_cooldown = self.titlebar_quick_action_button_on_cooldown();
        let icon_color = if open {
            titlebar_icon_hover_color()
        } else {
            titlebar_icon_color()
        };
        let anchor_state =
            window.use_keyed_state("ghostex-gpui-titlebar-actions-popup-anchor", cx, |_, _| {
                GpuiTitlebarPopupAnchorState::default()
            });
        let anchor_bounds = anchor_state.read(cx).bounds;
        let trigger_bounds_captured = anchor_state.read(cx).trigger_bounds_captured;
        let trigger_bounds = trigger_bounds_captured.then_some(anchor_bounds);

        div()
            .id("ghostex-gpui-titlebar-button-actions")
            .relative()
            .flex()
            .h(px(TITLEBAR_CONTROL_HEIGHT))
            .w(px(TITLEBAR_BUTTON_WIDTH))
            .items_center()
            .justify_center()
            .when(cfg!(target_os = "windows"), |this| this.occlude())
            .border_l_1()
            .border_color(titlebar_button_border_color())
            .text_color(icon_color)
            .cursor_default()
            .when(open, |this| this.bg(titlebar_active_segment_color()))
            .when(on_cooldown, |this| this.opacity(0.5))
            .when(!on_cooldown, |this| {
                this.hover(move |this| {
                    if open {
                        this.bg(titlebar_active_segment_color())
                            .text_color(titlebar_icon_hover_color())
                    } else {
                        this.bg(titlebar_button_hover_color())
                            .text_color(titlebar_icon_hover_color())
                    }
                })
            })
            .on_mouse_down(
                MouseButton::Left,
                cx.listener(move |this, event: &MouseDownEvent, window, cx| {
                    window.prevent_default();
                    cx.stop_propagation();
                    log_gpui_titlebar_popup_mouse_down(
                        GpuiTitlebarPopupKind::Actions,
                        "left",
                        "runPrimaryAction",
                        open,
                        trigger_bounds,
                        event,
                        window,
                    );
                    this.run_active_gpui_titlebar_action(window, cx);
                }),
            )
            .on_mouse_down(
                MouseButton::Right,
                cx.listener(move |this, event: &MouseDownEvent, window, cx| {
                    window.prevent_default();
                    cx.stop_propagation();
                    log_gpui_titlebar_popup_mouse_down(
                        GpuiTitlebarPopupKind::Actions,
                        "right",
                        "togglePopup",
                        open,
                        trigger_bounds,
                        event,
                        window,
                    );
                    this.set_gpui_titlebar_popup_open(
                        GpuiTitlebarPopupKind::Actions,
                        !open,
                        trigger_bounds,
                        window,
                        cx,
                    );
                }),
            )
            .when(!open, |this| {
                this.managed_discrete_tooltip_with_placement(
                    ManagedTooltipPlacement::Left,
                    Duration::from_millis(300),
                    |window, cx| titlebar_tooltip(TITLEBAR_ACTIONS_TOOLTIP, window, cx),
                )
            })
            .on_prepaint({
                let anchor_state = anchor_state.clone();
                move |bounds, window, cx| {
                    let (first_capture, moved) = anchor_state.update(cx, |state, _| {
                        let first_capture = !state.trigger_bounds_captured;
                        let moved = state.bounds != bounds;
                        state.bounds = bounds;
                        state.trigger_bounds_captured = true;
                        (first_capture, moved)
                    });
                    if first_capture || moved {
                        log_gpui_titlebar_popup_anchor(
                            GpuiTitlebarPopupKind::Actions,
                            bounds,
                            first_capture,
                            moved,
                            window,
                        );
                        window.request_animation_frame();
                    }
                }
            })
            .child(titlebar_svg_icon(icon_path, 16.0, icon_color))
            .into_any_element()
    }

    pub(crate) fn render_titlebar_open_targets_button(
        &self,
        window: &mut Window,
        cx: &mut gpui::Context<Self>,
    ) -> AnyElement {
        let (icon_path, _icon_size) = self.titlebar_open_target_icon();
        let open = self.titlebar_popup_menu_open(GpuiTitlebarPopupKind::OpenTargets);
        let icon_color = if open {
            titlebar_icon_hover_color()
        } else {
            titlebar_icon_color()
        };
        let anchor_state = window.use_keyed_state(
            "ghostex-gpui-titlebar-open-targets-popup-anchor",
            cx,
            |_, _| GpuiTitlebarPopupAnchorState::default(),
        );
        let anchor_bounds = anchor_state.read(cx).bounds;
        let trigger_bounds_captured = anchor_state.read(cx).trigger_bounds_captured;
        let trigger_bounds = trigger_bounds_captured.then_some(anchor_bounds);

        div()
            .id("ghostex-gpui-titlebar-button-open-project")
            .relative()
            .flex()
            .h(px(TITLEBAR_CONTROL_HEIGHT))
            .w(px(TITLEBAR_BUTTON_WIDTH))
            .items_center()
            .justify_center()
            .when(cfg!(target_os = "windows"), |this| this.occlude())
            .border_l_1()
            .when(
                cfg!(any(target_os = "windows", target_os = "linux")),
                |this| this.border_r_1(),
            )
            .border_color(titlebar_button_border_color())
            .text_color(icon_color)
            .cursor_default()
            .when(open, |this| this.bg(titlebar_active_segment_color()))
            .hover(move |this| {
                if open {
                    this.bg(titlebar_active_segment_color())
                        .text_color(titlebar_icon_hover_color())
                } else {
                    this.bg(titlebar_button_hover_color())
                        .text_color(titlebar_icon_hover_color())
                }
            })
            .on_mouse_down(
                MouseButton::Left,
                cx.listener(move |this, event: &MouseDownEvent, window, cx| {
                    window.prevent_default();
                    cx.stop_propagation();
                    log_gpui_titlebar_popup_mouse_down(
                        GpuiTitlebarPopupKind::OpenTargets,
                        "left",
                        "openPrimaryTarget",
                        open,
                        trigger_bounds,
                        event,
                        window,
                    );
                    this.open_active_project_with_active_open_target(window, cx);
                }),
            )
            .on_mouse_down(
                MouseButton::Right,
                cx.listener(move |this, event: &MouseDownEvent, window, cx| {
                    window.prevent_default();
                    cx.stop_propagation();
                    log_gpui_titlebar_popup_mouse_down(
                        GpuiTitlebarPopupKind::OpenTargets,
                        "right",
                        "togglePopup",
                        open,
                        trigger_bounds,
                        event,
                        window,
                    );
                    this.set_gpui_titlebar_popup_open(
                        GpuiTitlebarPopupKind::OpenTargets,
                        !open,
                        trigger_bounds,
                        window,
                        cx,
                    );
                }),
            )
            .when(!open, |this| {
                this.managed_discrete_tooltip_with_placement(
                    ManagedTooltipPlacement::Left,
                    Duration::from_millis(300),
                    |window, cx| titlebar_tooltip(TITLEBAR_OPEN_TARGETS_TOOLTIP, window, cx),
                )
            })
            .on_prepaint({
                let anchor_state = anchor_state.clone();
                move |bounds, window, cx| {
                    let (first_capture, moved) = anchor_state.update(cx, |state, _| {
                        let first_capture = !state.trigger_bounds_captured;
                        let moved = state.bounds != bounds;
                        state.bounds = bounds;
                        state.trigger_bounds_captured = true;
                        (first_capture, moved)
                    });
                    if first_capture || moved {
                        log_gpui_titlebar_popup_anchor(
                            GpuiTitlebarPopupKind::OpenTargets,
                            bounds,
                            first_capture,
                            moved,
                            window,
                        );
                        window.request_animation_frame();
                    }
                }
            })
            .child(titlebar_svg_icon(icon_path, 12.0, icon_color))
            .into_any_element()
    }

    pub(crate) fn titlebar_open_targets_popup_content_height(&self) -> f32 {
        let target_count = gpui_visible_open_targets_from_current_settings().len();
        let mut rows = vec![TITLEBAR_POPUP_MENU_ROW_HEIGHT; target_count];
        if target_count > 0 {
            rows.push(TITLEBAR_POPUP_MENU_SEPARATOR_HEIGHT);
        }
        rows.push(TITLEBAR_POPUP_MENU_ROW_HEIGHT);
        titlebar_popup_menu_height_for_rows(&rows)
    }

    pub(crate) fn titlebar_actions_popup_content_height(&self) -> f32 {
        let action_count = self.visible_gpui_titlebar_actions().len();
        let mut rows = if action_count == 0 {
            vec![TITLEBAR_POPUP_MENU_ROW_HEIGHT]
        } else {
            vec![TITLEBAR_POPUP_ACTION_ROW_HEIGHT; action_count]
        };
        rows.push(TITLEBAR_POPUP_MENU_SEPARATOR_HEIGHT);
        rows.push(TITLEBAR_POPUP_MENU_ROW_HEIGHT);
        titlebar_popup_menu_height_for_rows(&rows)
    }

    pub(crate) fn titlebar_git_popup_content_height(&self) -> f32 {
        let Some(state) = self.titlebar_git_menu_state.as_ref() else {
            return titlebar_popup_menu_height_for_rows(&[TITLEBAR_POPUP_MENU_ROW_HEIGHT]);
        };
        let section_label_height =
            TITLEBAR_POPUP_GIT_SECTION_LABEL_HEIGHT.max(TITLEBAR_POPUP_MENU_MIN_ITEM_HEIGHT);
        let mut rows = vec![
            section_label_height,
            TITLEBAR_POPUP_MENU_ROW_HEIGHT,
            TITLEBAR_POPUP_MENU_ROW_HEIGHT,
            TITLEBAR_POPUP_MENU_ROW_HEIGHT,
            TITLEBAR_POPUP_MENU_SEPARATOR_HEIGHT,
            section_label_height,
        ];
        rows.extend(std::iter::repeat_n(
            TITLEBAR_POPUP_MENU_ROW_HEIGHT,
            state.rows.len(),
        ));
        titlebar_popup_menu_height_for_rows(&rows)
    }

    pub(crate) fn titlebar_popup_content_height(&self, kind: GpuiTitlebarPopupKind) -> f32 {
        match kind {
            GpuiTitlebarPopupKind::RemoteSites => 650.0,
            GpuiTitlebarPopupKind::Actions => self.titlebar_actions_popup_content_height(),
            GpuiTitlebarPopupKind::Extensions => {
                let extension_count = self
                    .extensions_snapshot
                    .installed
                    .values()
                    .filter(|extension| extension.enabled)
                    .count();
                let mut rows = if extension_count == 0 {
                    vec![TITLEBAR_POPUP_MENU_ROW_HEIGHT]
                } else {
                    vec![TITLEBAR_POPUP_EXTENSION_ROW_HEIGHT; extension_count]
                };
                rows.push(TITLEBAR_POPUP_MENU_SEPARATOR_HEIGHT);
                rows.push(TITLEBAR_POPUP_MENU_ROW_HEIGHT);
                titlebar_popup_menu_height_for_rows(&rows)
            }
            GpuiTitlebarPopupKind::Git => self.titlebar_git_popup_content_height(),
            GpuiTitlebarPopupKind::OpenTargets => self.titlebar_open_targets_popup_content_height(),
            GpuiTitlebarPopupKind::Resources | GpuiTitlebarPopupKind::Tips => {
                TITLEBAR_POPUP_READING_MENU_MAX_HEIGHT
            }
        }
    }
}
