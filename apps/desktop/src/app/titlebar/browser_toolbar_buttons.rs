// C1 wave-4 deferred split: apps/desktop/src/app/titlebar.rs (~3.9k lines)
// further divided into responsibility-scoped submodules, pure move (the
// only edit from the original app/titlebar.rs body is wrapping each group
// of `impl GhostexGpuiApp` methods in its own impl block; multiple impl
// blocks for the same type across files is the established pattern used by
// every sibling file in apps/desktop/src/app/). This file holds the browser toolbar overflow/address-field/button renderers.
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
use gpui::MouseUpEvent;
use gpui::ParentElement as _;
use gpui::Styled as _;
use gpui::div;
use gpui::prelude::FluentBuilder as _;
use gpui::px;
use gpui_component::ElementExt as _;
use gpui_component::Sizable as _;
use gpui_component::Size as ComponentSize;
use gpui_component::h_flex;
use gpui_component::input::Input;
use gpui_component::tooltip::ManagedTooltipExt as _;
use gpui_component::tooltip::ManagedTooltipPlacement;

use crate::app::consts::*;
use crate::app::context_menu::GpuiContextMenu;
use crate::app::helpers::*;
use crate::app::model::*;
use crate::*;

impl GhostexGpuiApp {
    pub(crate) fn render_browser_toolbar_overflow_button(
        &self,
        pane_id: BrowserPaneId,
        cx: &mut gpui::Context<Self>,
    ) -> AnyElement {
        self.render_browser_pane_actions_button(pane_id, true, cx)
    }

    /// CDXC:ContextMenus 2026-09-17 DECISION:
    /// User: the address bar right-click menu must use the GPUI context menu instead of the system menu.
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
                    CDXC:Browser 2026-06-14-17:42:
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
            .on_mouse_up(
                MouseButton::Right,
                cx.listener(move |this, event: &MouseUpEvent, window, cx| {
                    cx.stop_propagation();
                    if !this.focus_browser_address_input_for_pane(pane_id, window, cx) {
                        return;
                    }
                    let input = this.browser_address_inputs[&pane_id].read(cx);
                    let has_selection = !input.selected_range().is_empty();
                    // Paste is offered without peeking at the clipboard, as GPUI Kit's own edit
                    // menu does: a Windows read can block the UI thread (CDXC:Clipboard in
                    // helpers/clipboard_read.rs), and the Paste action reads in the background.
                    GpuiContextMenu::new()
                        .menu_with_disabled(
                            "Cut",
                            !has_selection,
                            Box::new(gpui_component::input::Cut),
                        )
                        .menu_with_disabled(
                            "Copy",
                            !has_selection,
                            Box::new(gpui_component::input::Copy),
                        )
                        .menu("Paste", Box::new(gpui_component::input::Paste))
                        .separator()
                        .menu("Select All", Box::new(gpui_component::input::SelectAll))
                        .show(event.position, window, cx);
                }),
            )
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
                            // Keep Input's right-click caret/selection handling; an empty
                            // native menu lets the parent show the shared GPUI popup on release.
                            .context_menu(|menu, _, _| menu)
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
        let link_page = pane_id == crate::app::custom_links::CUSTOM_LINK_PAGE_PANE;
        let profile_number = if matches!(action, BrowserToolbarAction::ProfileMenu) {
            if link_page {
                crate::app::custom_links::custom_link_page_profile()
                    .and_then(BrowserProfileId::display_number)
            } else {
                self.browser_tabs
                    .active_tab_for_pane(pane_id)
                    .and_then(|tab| tab.profile_id.display_number())
            }
        } else {
            None
        };
        let trigger_bounds = std::rc::Rc::new(std::cell::Cell::new(None));
        div()
            .id(format!(
                "ghostex-gpui-browser-toolbar-button-{}-{id}",
                pane_id.0
            ))
            .flex()
            .flex_shrink_0()
            .h(px(TITLEBAR_CONTROL_HEIGHT))
            .w(px(BROWSER_TOOLBAR_BUTTON_WIDTH))
            .items_center()
            .justify_center()
            .rounded(px(TITLEBAR_BUTTON_RADIUS))
            .cursor_default()
            .text_color(if enabled {
                titlebar_icon_color()
            } else {
                browser_toolbar_disabled_icon_color()
            })
            .on_prepaint({
                let trigger_bounds = trigger_bounds.clone();
                move |bounds, _, _| trigger_bounds.set(Some(bounds))
            })
            .when(enabled, |this| {
                this.hover(|this| {
                    this.bg(titlebar_button_hover_color())
                        .text_color(titlebar_icon_hover_color())
                })
                .on_mouse_down(
                    MouseButton::Left,
                    cx.listener(move |this, _: &gpui::MouseDownEvent, window, cx| {
                        // The link page's toolbar (app/custom_links/page.rs) runs each action on
                        // its own page.
                        if link_page {
                            window.prevent_default();
                            cx.stop_propagation();
                            this.run_custom_link_toolbar_action(
                                action,
                                trigger_bounds.get(),
                                window,
                                cx,
                            );
                            return;
                        }
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
                                this.show_browser_history_popup(pane_id, window, cx);
                            }
                            BrowserToolbarAction::ProfileMenu => {
                                if let Some(trigger_bounds) = trigger_bounds.get() {
                                    this.show_browser_profile_menu(
                                        pane_id,
                                        trigger_bounds,
                                        window,
                                        cx,
                                    );
                                }
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
                        .border_color(chrome_ink().opacity(0.5))
                        .bg(chrome_ink().opacity(0.12))
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
