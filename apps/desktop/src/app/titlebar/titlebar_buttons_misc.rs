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
use gpui_component::ElementExt as _;
use gpui_component::tooltip::ManagedTooltipExt as _;
use gpui_component::tooltip::ManagedTooltipPlacement;

use crate::app::consts::*;
use crate::app::helpers::*;
use crate::app::model::*;
use crate::*;

impl GhostexGpuiApp {
    /// The titlebar update affordance: renders while a Ghostex update is available or downloading,
    /// or while Settings > Updates counts anything (app/updates/), shows a download icon at rest
    /// with that count beside it and a circular progress ring during the platform download, and
    /// opens the Updates list (titlebar/updates_menu.rs). While an update runs the icon is a
    /// spinner, and while a failure stands (until dismissed in that list) the button is red.
    ///
    /// CDXC:Updates 2026-11-05 DECISION:
    /// User (Q1361): the chat's top bar tells the user about updates with a quiet badge, and Update all is one click away from it. The existing ↑ button (left of the project name, 2026-09-20 below) carries the count of what Q1362 lights up; a click opens the small Updates list with Update all and Open Updates…
    pub(crate) fn render_titlebar_update_button(&self, cx: &mut gpui::Context<Self>) -> AnyElement {
        let downloading = self.update_downloading;
        let progress = self.update_download_progress;
        let badge = self.updates_badge_items().len();
        let busy = self.updates_running();
        let failed_count = self.updates_failed_count();
        let failed = failed_count > 0;
        let button_bounds = self.titlebar_update_button_bounds.clone();
        let app = cx.entity().downgrade();
        let update_tooltip: gpui::SharedString = if downloading {
            match progress {
                Some(progress) => format!(
                    "Downloading... {}%",
                    (progress * 100.0).round().clamp(0.0, 100.0) as u8
                )
                .into(),
                None => "Downloading...".into(),
            }
        } else if failed_count == 1 {
            "1 update failed".into()
        } else if failed {
            format!("{failed_count} updates failed").into()
        } else if busy {
            "Updating…".into()
        } else if badge == 1 {
            "1 update".into()
        } else if badge > 1 {
            format!("{badge} updates").into()
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
            // Same corner rounding as Back/Forward next to it, so its hover background matches theirs.
            .rounded(px(TITLEBAR_BUTTON_RADIUS))
            .when(failed && !downloading, |this| {
                this.bg(super::updates_menu_rows::update_button_failed_background())
            })
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
                    this.toggle_gpui_titlebar_updates_menu(window, cx);
                }),
            )
            .on_prepaint(move |bounds, window, cx| {
                button_bounds.set(Some(bounds));
                GhostexGpuiApp::open_pending_titlebar_popup(
                    app.clone(),
                    &[GpuiTitlebarPopupKind::Updates],
                    bounds,
                    window,
                    cx,
                );
            })
            // CDXC:Updates 2026-11-07 WHY:
            // To the button's right the "5 updates" tooltip covered the "Ghostex" breadcrumb beside it; below the button, like the other titlebar tooltips, it covers nothing the user is reading.
            .managed_tooltip_with_placement(ManagedTooltipPlacement::Below, move |window, cx| {
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
                        // The ring reads the clock itself (`paint_titlebar_update_progress_ring`);
                        // this only redraws it, at the shared indicator rate rather than on every
                        // display frame.
                        this.child(ring.with_throttled_animation(
                            "ghostex-gpui-titlebar-update-progress-pending",
                            Duration::from_millis(1_250),
                            |ring, _progress| ring,
                        ))
                    } else {
                        this.child(ring)
                    }
                } else {
                    this.child(super::updates_menu_rows::update_button_face(
                        busy, failed, badge,
                    ))
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
            .rounded(px(TITLEBAR_BUTTON_RADIUS))
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
