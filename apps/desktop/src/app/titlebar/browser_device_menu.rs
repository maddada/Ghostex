//! The Browser toolbar's page-size menu (phone and tablet preview) and the handler that applies a
//! tab's choice.

use std::{cell::Cell, rc::Rc};

use gpui_component::{
    ElementExt as _, Side,
    menu::{PopupMenu, PopupMenuItem},
    tooltip::{ManagedTooltipExt as _, ManagedTooltipPlacement},
};

use super::popup_menu_builders::titlebar_popup_menu_with_scroll_behavior;
use crate::app::helpers::*;
use crate::*;

/// The `modals/settings` copy is the same Tabler icon; it is embedded with the other Ghostex assets.
const BROWSER_ICON_DEVICE_MOBILE: &str = "modals/settings/device-mobile.svg";

impl GhostexGpuiApp {
    pub(crate) fn browser_device_view_for_pane(&self, pane_id: BrowserPaneId) -> BrowserDeviceView {
        if pane_id == crate::app::custom_links::CUSTOM_LINK_PAGE_PANE {
            return self.custom_link_device_view();
        }
        self.browser_tabs
            .active_tab_for_pane(pane_id)
            .map(|tab| tab.device_view)
            .unwrap_or_default()
    }

    /// Stores the tab's new page size or user agent. A user agent change reloads the page so the
    /// site answers with its mobile (or desktop) layout; a size change only moves the page frame.
    pub(crate) fn set_browser_device_view_for_pane(
        &mut self,
        pane_id: BrowserPaneId,
        device_view: BrowserDeviceView,
        cx: &mut gpui::Context<Self>,
    ) {
        if pane_id == crate::app::custom_links::CUSTOM_LINK_PAGE_PANE {
            self.set_custom_link_device_view(device_view, cx);
            return;
        }
        if !self.titlebar_mode_available(TitlebarMode::Browser) {
            return;
        }
        let Some(tab_id) = self.browser_tabs.active_tab_id_for_pane(pane_id) else {
            return;
        };
        let previous_user_agent = self.browser_device_view_for_pane(pane_id).user_agent();
        if !self.browser_tabs.set_tab_device_view(tab_id, device_view) {
            return;
        }
        if previous_user_agent != device_view.user_agent()
            && let Some(surface) = self.browser_surfaces.get(&tab_id).cloned()
        {
            surface.update(cx, |surface, _| {
                surface.set_mobile_user_agent(device_view.user_agent())
            });
        }
        self.persist_shell_layout_state();
        cx.notify();
    }

    pub(crate) fn render_browser_device_view_button(
        &self,
        pane_id: BrowserPaneId,
        cx: &mut gpui::Context<Self>,
    ) -> AnyElement {
        let kind = GpuiTitlebarPopupKind::BrowserDeviceView(pane_id);
        let open = self.titlebar_popup_menu_open(kind);
        let device_view = self.browser_device_view_for_pane(pane_id);
        let previewing = !device_view.is_default();
        let tooltip: gpui::SharedString = format!(
            "Page Size: {}{}",
            device_view.size.label(),
            if device_view.mobile_user_agent {
                ", mobile user agent"
            } else {
                ""
            }
        )
        .into();
        let trigger_bounds = Rc::new(Cell::new(None));
        div()
            .id(format!(
                "ghostex-gpui-browser-toolbar-button-{}-device-view",
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
            .when(open || previewing, |this| {
                this.bg(titlebar_active_segment_color())
            })
            .hover(|this| this.bg(titlebar_button_hover_color()))
            .on_prepaint({
                let trigger_bounds = trigger_bounds.clone();
                move |bounds, _, _| trigger_bounds.set(Some(bounds))
            })
            .on_mouse_down(
                MouseButton::Left,
                cx.listener(move |this, _: &MouseDownEvent, window, cx| {
                    window.prevent_default();
                    cx.stop_propagation();
                    if pane_id != crate::app::custom_links::CUSTOM_LINK_PAGE_PANE
                        && this.browser_tabs.find_leaf(pane_id).is_none()
                    {
                        return;
                    }
                    this.set_gpui_titlebar_popup_open(
                        kind,
                        !this.titlebar_popup_menu_open(kind),
                        trigger_bounds.get(),
                        window,
                        cx,
                    );
                }),
            )
            .child(titlebar_svg_icon(
                BROWSER_ICON_DEVICE_MOBILE,
                BROWSER_TOOLBAR_BUTTON_ICON_SIZE,
                if previewing {
                    titlebar_icon_hover_color()
                } else {
                    titlebar_icon_color()
                },
            ))
            .managed_tooltip_with_placement(ManagedTooltipPlacement::Left, move |window, cx| {
                titlebar_tooltip(tooltip.clone(), window, cx)
            })
            .into_any_element()
    }

    pub(crate) fn build_gpui_browser_device_view_popup_menu(
        &self,
        menu: PopupMenu,
        pane_id: BrowserPaneId,
        width: f32,
        max_height: f32,
        scrollable: bool,
    ) -> PopupMenu {
        let device_view = self.browser_device_view_for_pane(pane_id);
        let mut menu =
            titlebar_popup_menu_with_scroll_behavior(menu, width, max_height, scrollable)
                .check_side(Side::Right)
                .item(
                    PopupMenuItem::element(|_, _| titlebar_popup_git_section_label("Page size"))
                        .disabled(true),
                );
        for size in BrowserViewportSize::ALL {
            let label = match size.portrait_size() {
                Some((width, height)) => {
                    format!("{}  {} × {}", size.label(), width as u32, height as u32)
                }
                None => size.label().to_string(),
            };
            menu = menu.menu_element_with_check(
                device_view.size == size,
                Box::new(SetBrowserDeviceView {
                    pane_id: pane_id.0,
                    device_view: BrowserDeviceView {
                        size,
                        ..device_view
                    },
                }),
                move |_, _| browser_device_menu_row(label.clone()),
            );
        }
        menu.separator()
            .menu_element_with_check(
                device_view.landscape,
                Box::new(SetBrowserDeviceView {
                    pane_id: pane_id.0,
                    device_view: BrowserDeviceView {
                        landscape: !device_view.landscape,
                        ..device_view
                    },
                }),
                |_, _| browser_device_menu_row("Landscape".to_string()),
            )
            .menu_element_with_check(
                device_view.mobile_user_agent,
                Box::new(SetBrowserDeviceView {
                    pane_id: pane_id.0,
                    device_view: BrowserDeviceView {
                        mobile_user_agent: !device_view.mobile_user_agent,
                        ..device_view
                    },
                }),
                |_, _| browser_device_menu_row("Mobile user agent".to_string()),
            )
    }

    pub(crate) fn browser_device_view_popup_content_height(&self) -> f32 {
        let mut rows = vec![TITLEBAR_POPUP_MENU_MIN_ITEM_HEIGHT];
        rows.extend(std::iter::repeat_n(
            TITLEBAR_POPUP_MENU_ROW_HEIGHT,
            BrowserViewportSize::ALL.len(),
        ));
        rows.push(TITLEBAR_POPUP_MENU_SEPARATOR_HEIGHT);
        rows.extend([TITLEBAR_POPUP_MENU_ROW_HEIGHT; 2]);
        titlebar_popup_menu_height_for_rows(&rows)
    }
}

fn browser_device_menu_row(label: String) -> impl IntoElement {
    div()
        .flex()
        .flex_1()
        .min_w_0()
        .items_center()
        .min_h(px(TITLEBAR_POPUP_MENU_ROW_HEIGHT))
        .text_size(px(TITLEBAR_POPUP_MENU_ROW_TEXT_SIZE))
        .text_color(titlebar_text_color())
        .child(label)
}
