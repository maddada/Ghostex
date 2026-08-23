// C1 wave-4 deferred split: apps/desktop/src/app/titlebar.rs (~3.9k lines)
// further divided into responsibility-scoped submodules, pure move (the
// only edit from the original app/titlebar.rs body is wrapping each group
// of `impl GhostexGpuiApp` methods in its own impl block; multiple impl
// blocks for the same type across files is the established pattern used by
// every sibling file in apps/desktop/src/app/). This file holds the browser toolbar new-tab/overflow/address-field/button renderers.
// See docs/2026-08-22/repo-restructure/SPLITS.md C1.

// C1 wave-4 extraction: `impl GhostexGpuiApp` methods moved verbatim out of
// main.rs (pure move; the only edit is the `pub(crate) ` visibility prefix the
// cross-module split requires). See docs/2026-08-22/repo-restructure/SPLITS.md C1.
//
// Cluster: titlebar menus, popups, actions, and titlebar render_* builders

// RefCell backs cross-platform runtime state (window frame persistence), not
// just the macOS-only shims that first introduced the import.

use gpui::AnyElement;
use gpui::FontWeight;
use gpui::InteractiveElement as _;
use gpui::IntoElement;
use gpui::KeyDownEvent;
use gpui::MouseButton;
use gpui::MouseDownEvent;
use gpui::MouseUpEvent;
use gpui::ParentElement as _;
use gpui::Styled as _;
use gpui::div;
use gpui::prelude::FluentBuilder as _;
use gpui::px;
use gpui::rgb;
use gpui_component::Sizable as _;
use gpui_component::Size as ComponentSize;
use gpui_component::h_flex;
use gpui_component::input::Input;
use gpui_component::tooltip::ManagedTooltipExt as _;
use gpui_component::tooltip::ManagedTooltipPlacement;

use crate::app::consts::*;
use crate::app::helpers::*;
use crate::app::model::*;
use crate::*;

impl GhostexGpuiApp {
    pub(crate) fn render_browser_toolbar_new_tab_button(
        &self,
        pane_id: BrowserPaneId,
        cx: &mut gpui::Context<Self>,
    ) -> AnyElement {
        div()
            .id(format!(
                "ghostex-gpui-browser-toolbar-new-tab-{}",
                pane_id.0
            ))
            .flex()
            .flex_shrink_0()
            .h(px(BROWSER_TOOLBAR_HEIGHT - 1.0))
            .w(px(BROWSER_TOOLBAR_BUTTON_WIDTH))
            .items_center()
            .justify_center()
            .border_l_1()
            .border_color(titlebar_button_border_color())
            .cursor_default()
            .hover(|this| this.bg(rgb(0x212121)))
            .on_mouse_down(
                MouseButton::Left,
                cx.listener(move |this, _event: &MouseDownEvent, window, cx| {
                    window.prevent_default();
                    cx.stop_propagation();
                    this.swap_browser_tabs_for_active_project(cx);
                    this.browser_tabs.focus_pane(pane_id);
                    this.add_browser_tab(window, cx);
                }),
            )
            .managed_tooltip_with_placement(ManagedTooltipPlacement::Left, |window, cx| {
                titlebar_tooltip("New browser tab", window, cx)
            })
            .child(self.render_browser_tab_new_icon(12.0))
            .into_any_element()
    }

    pub(crate) fn render_browser_toolbar_overflow_button(
        &self,
        pane_id: BrowserPaneId,
        cx: &mut gpui::Context<Self>,
    ) -> AnyElement {
        div()
            .id(format!(
                "ghostex-gpui-browser-toolbar-overflow-{}",
                pane_id.0
            ))
            .flex()
            .flex_shrink_0()
            .h(px(BROWSER_TOOLBAR_HEIGHT - 1.0))
            .w(px(BROWSER_TOOLBAR_BUTTON_WIDTH - 1.0))
            .items_center()
            .justify_center()
            .border_l_1()
            .border_color(titlebar_button_border_color())
            .cursor_default()
            .hover(|this| this.bg(rgb(0x212121)))
            .on_mouse_down(
                MouseButton::Left,
                cx.listener(move |_this, _event: &MouseDownEvent, window, cx| {
                    window.prevent_default();
                    cx.stop_propagation();
                }),
            )
            .on_mouse_up(
                MouseButton::Left,
                cx.listener(move |this, event: &MouseUpEvent, window, cx| {
                    window.prevent_default();
                    cx.stop_propagation();
                    this.show_browser_pane_actions_menu(pane_id, event.position, window, cx);
                }),
            )
            .managed_tooltip_with_placement(ManagedTooltipPlacement::Left, |window, cx| {
                titlebar_tooltip("Browser pane actions menu", window, cx)
            })
            .child(self.render_browser_tab_overflow_icon())
            .into_any_element()
    }

    pub(crate) fn render_browser_address_field(
        &self,
        pane_id: BrowserPaneId,
        cx: &mut gpui::Context<Self>,
    ) -> AnyElement {
        let address_value = self.browser_tabs.address_value_for_pane(pane_id);
        let address_input = self
            .browser_address_inputs
            .get(&pane_id)
            .cloned()
            .expect("browser address input must exist for rendered pane");

        h_flex()
            .id(format!("ghostex-gpui-browser-address-{}", pane_id.0))
            .flex_1()
            .min_w(px(BROWSER_ADDRESS_MINIMUM_WIDTH))
            .h(px(BROWSER_ADDRESS_HEIGHT))
            .items_center()
            .cursor_text()
            .on_mouse_down(
                MouseButton::Left,
                cx.listener(move |this, _, window, cx| {
                    /*
                    CDXC:GPUIBrowserToolbar 2026-06-14-17:42:
                    GPUI owns the browser toolbar input even though CEF owns the page below it. Route clicks through the complete Browser address-focus boundary so shell focus leaves terminal companion panes before GPUI/AppKit keyboard ownership moves to the input.
                    */
                    let _ = this.focus_browser_address_input_for_pane(pane_id, window, cx);
                }),
            )
            .on_key_down(cx.listener(move |this, event: &KeyDownEvent, window, cx| {
                if event.keystroke.key.as_str() == "escape" {
                    cx.stop_propagation();
                    this.cancel_browser_address_edit_for_pane(pane_id, window, cx);
                }
            }))
            .child(titlebar_svg_icon(
                browser_security_icon_path(&address_value),
                14.0,
                browser_toolbar_security_icon_color(),
            ))
            .child(
                div()
                    .ml(px(8.0))
                    .flex_1()
                    .min_w_0()
                    .h(px(BROWSER_ADDRESS_HEIGHT))
                    .overflow_hidden()
                    .child(
                        Input::new(&address_input)
                            .with_size(ComponentSize::XSmall)
                            .appearance(false)
                            .bordered(false)
                            .focus_bordered(false)
                            .w_full()
                            .px(px(0.0))
                            .py(px(0.0))
                            .text_size(px(13.0))
                            .font_weight(FontWeight::MEDIUM)
                            .line_height(px(BROWSER_ADDRESS_HEIGHT))
                            .text_color(browser_toolbar_text_color()),
                    ),
            )
            .into_any_element()
    }

    pub(crate) fn render_browser_toolbar_button(
        &self,
        id: &'static str,
        icon_path: &'static str,
        enabled: bool,
        tooltip: Option<gpui::SharedString>,
        action: BrowserToolbarAction,
        pane_id: BrowserPaneId,
        cx: &mut gpui::Context<Self>,
    ) -> impl IntoElement {
        let tooltip_placement = ManagedTooltipPlacement::Left;
        let profile_number = if matches!(action, BrowserToolbarAction::ProfileMenu) {
            self.browser_tabs
                .active_tab_for_pane(pane_id)
                .and_then(|tab| tab.profile_id.display_number())
        } else {
            None
        };
        div()
            .id(format!(
                "ghostex-gpui-browser-toolbar-button-{}-{id}",
                pane_id.0
            ))
            .flex()
            .flex_shrink_0()
            .h(px(BROWSER_TOOLBAR_HEIGHT - 1.0))
            .w(px(if id == "back" {
                BROWSER_TOOLBAR_BUTTON_WIDTH - 1.0
            } else {
                BROWSER_TOOLBAR_BUTTON_WIDTH
            }))
            .items_center()
            .justify_center()
            .when(id != "back", |this| this.border_l_1())
            .when(id == "home", |this| this.border_r_1())
            .border_color(titlebar_button_border_color())
            .cursor_default()
            .text_color(if enabled {
                titlebar_icon_color()
            } else {
                browser_toolbar_disabled_icon_color()
            })
            .when(enabled, |this| {
                this.hover(|this| {
                    this.bg(rgb(0x212121))
                        .text_color(titlebar_icon_hover_color())
                })
                .on_mouse_down(
                    MouseButton::Left,
                    cx.listener(move |this, event: &gpui::MouseDownEvent, window, cx| {
                        match action {
                            BrowserToolbarAction::Back
                            | BrowserToolbarAction::Forward
                            | BrowserToolbarAction::Reload
                            | BrowserToolbarAction::StopLoading => {
                                this.perform_browser_toolbar_action(pane_id, action, cx);
                            }
                            BrowserToolbarAction::Home => {
                                this.navigate_browser_home_from_toolbar(pane_id, window, cx);
                            }
                            BrowserToolbarAction::FeedbackTool => {
                                this.run_browser_feedback_tool_from_toolbar(pane_id, window, cx);
                            }
                            BrowserToolbarAction::ResetZoom => {
                                this.reset_browser_zoom_from_toolbar(pane_id, window, cx);
                            }
                            BrowserToolbarAction::ResetMediaPermissions => {
                                this.reset_browser_media_permissions_for_pane(pane_id, cx);
                            }
                            BrowserToolbarAction::HistoryMenu => {
                                this.show_browser_recent_history_menu(
                                    pane_id,
                                    event.position,
                                    window,
                                    cx,
                                );
                            }
                            BrowserToolbarAction::ProfileMenu => {
                                this.show_browser_profile_menu(pane_id, event.position, window, cx);
                            }
                            BrowserToolbarAction::DevTools => {
                                this.toggle_browser_devtools_from_toolbar(pane_id, window, cx);
                            }
                        }
                        window.prevent_default();
                        cx.stop_propagation();
                    }),
                )
            })
            .when(profile_number.is_none(), |this| {
                this.child(titlebar_svg_icon(
                    icon_path,
                    BROWSER_TOOLBAR_BUTTON_ICON_SIZE,
                    if enabled {
                        titlebar_icon_color()
                    } else {
                        browser_toolbar_disabled_icon_color()
                    },
                ))
            })
            .when_some(profile_number, |this, profile_number| {
                this.child(
                    div()
                        .flex()
                        .size(px(BROWSER_TOOLBAR_BUTTON_ICON_SIZE))
                        .items_center()
                        .justify_center()
                        .rounded_full()
                        .border_1()
                        .border_color(rgb(0xffffff).opacity(0.5))
                        .bg(rgb(0xffffff).opacity(0.12))
                        .text_size(px(if profile_number < 10 { 10.0 } else { 8.0 }))
                        .font_weight(FontWeight::SEMIBOLD)
                        .child(profile_number.to_string()),
                )
            })
            .when_some(tooltip, |this, tooltip| {
                this.managed_tooltip_with_placement(tooltip_placement, move |window, cx| {
                    titlebar_tooltip(tooltip.clone(), window, cx)
                })
            })
    }
}
