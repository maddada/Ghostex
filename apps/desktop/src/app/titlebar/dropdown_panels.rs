// C1 wave-4 deferred split: apps/desktop/src/app/titlebar.rs (~3.9k lines)
// further divided into responsibility-scoped submodules, pure move (the
// only edit from the original app/titlebar.rs body is wrapping each group
// of `impl GhostexGpuiApp` methods in its own impl block; multiple impl
// blocks for the same type across files is the established pattern used by
// every sibling file in apps/desktop/src/app/). This file holds the extension popup panel renderer.
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
use gpui::InteractiveElement as _;
use gpui::IntoElement;
use gpui::MouseDownEvent;
use gpui::ParentElement as _;
use gpui::Pixels;
use gpui::Styled as _;
use gpui::Window;
use gpui::anchored;
use gpui::deferred;
use gpui::div;
use gpui::point;
use gpui::prelude::FluentBuilder as _;
use gpui::px;
use gpui::size;
use gpui_component::v_flex;

use crate::app::actions::*;
use crate::app::consts::*;
use crate::app::helpers::*;
use crate::*;

impl GhostexGpuiApp {
    pub(crate) fn titlebar_extension_popup_bounds(
        &self,
        window: &Window,
    ) -> Option<Bounds<Pixels>> {
        let state = self.titlebar_extension_popup.as_ref()?;
        let horizontal_margin = 8.0;
        let width = state
            .size
            .width
            .min((window.viewport_size().width.as_f32() - horizontal_margin * 2.0).max(1.0));
        let top = state.trigger_bounds.bottom().as_f32() + HEADER_MENU_TRIGGER_GAP;
        let height = state
            .size
            .height
            .min((window.viewport_size().height.as_f32() - top - horizontal_margin).max(1.0));
        let min_right_edge = width + horizontal_margin;
        let max_right_edge =
            (window.viewport_size().width.as_f32() - horizontal_margin).max(min_right_edge);
        let right_edge = state
            .trigger_bounds
            .top_right()
            .x
            .as_f32()
            .clamp(min_right_edge, max_right_edge);
        Some(Bounds::new(
            point(px(right_edge - width), px(top)),
            size(px(width), px(height)),
        ))
    }

    pub(crate) fn render_titlebar_extension_popup_panel(
        &self,
        window: &mut Window,
        cx: &mut gpui::Context<Self>,
    ) -> AnyElement {
        let Some((id, trigger_bounds, panel, error)) =
            self.titlebar_extension_popup.as_ref().map(|state| {
                (
                    state.id,
                    state.trigger_bounds,
                    state.panel.clone(),
                    state.error.clone(),
                )
            })
        else {
            return div().size_0().into_any_element();
        };
        let Some(popup_bounds) = self.titlebar_extension_popup_bounds(window) else {
            return div().size_0().into_any_element();
        };
        let position = popup_bounds.top_right();

        deferred(
            anchored()
                .anchor(Anchor::TopRight)
                .position(position)
                .child(
                    div()
                        .id(format!(
                            "ghostex-gpui-titlebar-extension-popup-{}",
                            id.as_str()
                        ))
                        .occlude()
                        .tab_group()
                        .key_context(TITLEBAR_DROPDOWN_KEY_CONTEXT)
                        .track_focus(&self.titlebar_dropdown_focus_handle)
                        .w(popup_bounds.size.width)
                        .h(popup_bounds.size.height)
                        .overflow_hidden()
                        .rounded(px(2.0))
                        .border_1()
                        .border_color(titlebar_popup_menu_border_color())
                        .bg(titlebar_popup_menu_background())
                        .on_action(cx.listener(|this, _: &TitlebarDropdownCancel, window, cx| {
                            this.close_titlebar_extension_popup(window, cx);
                        }))
                        .on_mouse_down_out(cx.listener(
                            move |this, event: &MouseDownEvent, window, cx| {
                                if trigger_bounds.contains(&event.position) {
                                    return;
                                }
                                this.close_titlebar_extension_popup(window, cx);
                            },
                        ))
                        .map(|this| {
                            if let Some(panel) = panel {
                                this.child(panel)
                            } else if let Some(error) = error {
                                this.child(
                                    v_flex()
                                        .size_full()
                                        .items_center()
                                        .justify_center()
                                        .px(px(24.0))
                                        .text_center()
                                        .text_size(px(12.0))
                                        .text_color(titlebar_inactive_text_color())
                                        .child(error),
                                )
                            } else {
                                this.child(
                                    v_flex()
                                        .size_full()
                                        .items_center()
                                        .justify_center()
                                        .text_size(px(12.0))
                                        .text_color(titlebar_inactive_text_color())
                                        .child("Starting extension…"),
                                )
                            }
                        }),
                ),
        )
        .with_priority(1)
        .into_any_element()
    }
}
