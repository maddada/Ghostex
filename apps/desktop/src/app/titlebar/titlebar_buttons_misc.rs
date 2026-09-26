// C1 wave-4 deferred split: apps/desktop/src/app/titlebar.rs (~3.9k lines)
// further divided into responsibility-scoped submodules, pure move (the
// only edit from the original app/titlebar.rs body is wrapping each group
// of `impl GhostexGpuiApp` methods in its own impl block; multiple impl
// blocks for the same type across files is the established pattern used by
// every sibling file in apps/desktop/src/app/). This file holds the update, prompt-editor, exit-focus, and git titlebar button renderers.
// See docs/2026-08-22/repo-restructure/SPLITS.md C1.

// C1 wave-4 extraction: `impl GhostexGpuiApp` methods moved verbatim out of
// main.rs (pure move; the only edit is the `pub(crate) ` visibility prefix the
// cross-module split requires). See docs/2026-08-22/repo-restructure/SPLITS.md C1.
//
// Cluster: titlebar menus, popups, actions, and titlebar render_* builders

use std::time::Duration;

// RefCell backs cross-platform runtime state (window frame persistence), not
// just the macOS-only shims that first introduced the import.

use gpui::Animation;
use gpui::AnimationExt as _;
use gpui::AnyElement;
use gpui::FontWeight;
use gpui::InteractiveElement as _;
use gpui::IntoElement;
use gpui::MouseButton;
use gpui::MouseDownEvent;
use gpui::ParentElement as _;
use gpui::Styled as _;
use gpui::canvas;
use gpui::div;
use gpui::prelude::FluentBuilder as _;
use gpui::px;
use gpui::rgb;
use gpui::svg;
use gpui_component::tooltip::ManagedTooltipExt as _;
use gpui_component::tooltip::ManagedTooltipPlacement;

use crate::app::consts::*;
use crate::app::helpers::*;
use crate::app::model::*;
use crate::*;

impl GhostexGpuiApp {
    /// The titlebar update affordance: renders only while an update is
    /// available or downloading, shows a download icon at rest and a circular
    /// progress ring during the platform download, and disables clicks while
    /// downloading.
    pub(crate) fn render_titlebar_update_button(&self, cx: &mut gpui::Context<Self>) -> AnyElement {
        let downloading = self.update_downloading;
        let progress = self.update_download_progress;
        let update_tooltip: gpui::SharedString = if downloading {
            match progress {
                Some(progress) => format!(
                    "Downloading... {}%",
                    (progress * 100.0).round().clamp(0.0, 100.0) as u8
                )
                .into(),
                None => "Downloading...".into(),
            }
        } else {
            TITLEBAR_UPDATE_AVAILABLE_TOOLTIP.into()
        };
        div()
            .id("ghostex-gpui-titlebar-button-update")
            .relative()
            .flex()
            .h(px(TITLEBAR_LEADING_TALL_BUTTON_HEIGHT))
            .px(px(TITLEBAR_BUTTON_HORIZONTAL_PADDING))
            .flex_shrink_0()
            .items_center()
            .justify_center()
            .text_color(if downloading {
                titlebar_update_downloading_color()
            } else {
                titlebar_update_available_color()
            })
            .cursor_default()
            .when(!downloading, |this| {
                this.hover(|this| {
                    this.bg(titlebar_button_hover_color())
                        .text_color(titlebar_update_available_color())
                })
            })
            .on_mouse_down(
                MouseButton::Left,
                cx.listener(move |this, _event: &MouseDownEvent, window, cx| {
                    window.prevent_default();
                    cx.stop_propagation();
                    this.check_for_gpui_updates(window, cx);
                }),
            )
            .managed_tooltip_with_placement(ManagedTooltipPlacement::Right, move |window, cx| {
                titlebar_tooltip(update_tooltip.clone(), window, cx)
            })
            .map(|this| {
                if downloading {
                    let ring = canvas(
                        move |_bounds, _window, _cx| {},
                        move |bounds, _state: (), window, _cx| {
                            paint_titlebar_update_progress_ring(bounds, progress, window);
                        },
                    )
                    .size(px(TITLEBAR_UPDATE_PROGRESS_RING_SIZE))
                    .ml(px(1.0))
                    .mt(px(1.5));
                    if progress.is_none() {
                        this.child(ring.with_animation(
                            "ghostex-gpui-titlebar-update-progress-pending",
                            Animation::new(Duration::from_millis(1_250)).repeat(),
                            |ring, _delta| ring,
                        ))
                    } else {
                        this.child(ring)
                    }
                } else {
                    this.child(
                        svg()
                            .size(px(TITLEBAR_UPDATE_ICON_SIZE))
                            .ml(px(1.0))
                            .mt(px(0.5))
                            .path(TITLEBAR_ICON_DOWNLOAD)
                            .text_color(titlebar_update_available_color())
                            .hover(|this| this.text_color(titlebar_update_available_color())),
                    )
                }
            })
            .into_any_element()
    }

    /// Bring-the-open-standalone-editor-forward affordance. Occupies the
    /// Exit Focus slot with the same text-button chrome, but stays in the
    /// resting (non-active-tab) skin because it does not represent a mode
    /// the workspace is currently in.
    pub(crate) fn render_titlebar_prompt_editor_button(
        &self,
        cx: &mut gpui::Context<Self>,
    ) -> AnyElement {
        div()
            .id("ghostex-gpui-titlebar-prompt-editor")
            .relative()
            .flex()
            .h(px(TITLEBAR_CONTROL_HEIGHT))
            .min_w(px(70.0))
            .flex_shrink_0()
            .items_center()
            .justify_center()
            .gap(px(7.0))
            .when(cfg!(target_os = "windows"), |this| this.occlude())
            .px(px(14.0))
            .text_size(px(13.55))
            .font_weight(FontWeight::NORMAL)
            .line_height(px(TITLEBAR_CONTROL_HEIGHT))
            .text_color(titlebar_icon_color())
            .cursor_default()
            .hover(|this| {
                this.bg(titlebar_button_hover_color())
                    .text_color(titlebar_icon_hover_color())
            })
            .on_mouse_down(
                MouseButton::Left,
                cx.listener(move |_this, _event: &MouseDownEvent, window, cx| {
                    window.prevent_default();
                    cx.stop_propagation();
                    cx.background_executor()
                        .spawn(async move { gpui_ghostex_editor_daemon_bring_to_front() })
                        .detach();
                }),
            )
            .child(div().size(px(6.0)).rounded_full().bg(rgb(0x95d7f6)))
            .child("Prompt Editor")
            .into_any_element()
    }

    pub(crate) fn render_titlebar_exit_focus_button(
        &self,
        signature: GpuiTitlebarExitFocusControlSignature,
        cx: &mut gpui::Context<Self>,
    ) -> AnyElement {
        let styled_as_active_mode_tab = signature.styled_as_active_mode_tab;
        let clears_agents_focus_mode = signature.clears_agents_focus_mode;
        div()
            .id("ghostex-gpui-titlebar-exit-focus")
            .relative()
            .flex()
            .h(px(TITLEBAR_MODE_TAB_HEIGHT))
            .mt(px(TITLEBAR_MODE_TAB_TOP_INSET))
            .min_w(px(70.0))
            .flex_shrink_0()
            .items_center()
            .justify_center()
            .when(cfg!(target_os = "windows"), |this| this.occlude())
            .rounded(px(TITLEBAR_MODE_TAB_RADIUS))
            .px(px(TITLEBAR_MODE_TAB_HORIZONTAL_PADDING))
            .text_size(px(13.55))
            .font_weight(FontWeight::NORMAL)
            .line_height(px(TITLEBAR_MODE_TAB_HEIGHT))
            .text_color(titlebar_active_text_color())
            .cursor_default()
            .when(styled_as_active_mode_tab, |this| {
                this.bg(titlebar_active_segment_color())
            })
            .hover(move |this| {
                if styled_as_active_mode_tab {
                    this.bg(titlebar_active_segment_color())
                        .text_color(titlebar_active_text_color())
                } else {
                    this
                }
            })
            .on_mouse_down(
                MouseButton::Left,
                cx.listener(move |this, _event: &MouseDownEvent, window, cx| {
                    window.prevent_default();
                    cx.stop_propagation();
                    if clears_agents_focus_mode {
                        this.exit_titlebar_focus_mode(cx);
                    }
                }),
            )
            .child(signature.label)
            .into_any_element()
    }
}
